using System.Runtime.InteropServices;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

namespace Kite;

internal static class MediaNative
{
    private const string Ffi = "kite_ffi", Decoder = "kite_windows_media";
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void Release(IntPtr context);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate byte Supports(IntPtr context, uint w, uint h, uint fps);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate byte Configure(IntPtr context, uint w, uint h, uint fps, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate byte Frame(IntPtr context, IntPtr bytes, uint length, ulong pts, uint flags, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void AudioConfigure(IntPtr context, uint rate, uint channels, uint samples, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void AudioFrame(IntPtr context, IntPtr bytes, uint length, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void Stopped(IntPtr context, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void Pixels(IntPtr context, IntPtr pixels, uint w, uint h, uint stride, ulong gen);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate void Error(IntPtr context, int code, ulong gen);
    [StructLayout(LayoutKind.Sequential)]
    internal struct Callbacks
    {
        public Release Release; public Supports Supports; public Configure Configure; public Frame Frame;
        public AudioConfigure AudioConfigure; public AudioFrame AudioFrame; public Stopped Stopped;
    }
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern ulong kite_windows_media_register(IntPtr context, ref Callbacks cb);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_windows_media_detach(ulong id);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_windows_media_frame_rendered(ulong id, ulong gen);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_windows_media_decoder_failed(ulong id, ulong gen, int error);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_windows_media_mouse_position(ulong id, short x, short y, short w, short h);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_windows_media_mouse_button(ulong id, byte button, byte down);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_windows_media_scroll(ulong id, short horizontal, short vertical);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_windows_media_key(ulong id, short key, byte down, byte modifiers);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_windows_media_text(ulong id, byte[] bytes, uint length);
    [DllImport(Ffi, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_windows_media_release_input(ulong id);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern IntPtr kite_decoder_create(IntPtr context, Pixels pixels, Error error);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_decoder_destroy(IntPtr decoder);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_decoder_configure(IntPtr decoder, uint w, uint h, uint fps, ulong gen);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern byte kite_decoder_frame(IntPtr decoder, IntPtr bytes, uint length, ulong pts, uint flags, ulong gen);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_decoder_audio_configure(IntPtr decoder, uint rate, uint channels, uint samples, ulong gen);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_decoder_audio(IntPtr decoder, IntPtr bytes, uint length, ulong gen);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_decoder_stop(IntPtr decoder, ulong gen);
    [DllImport(Decoder, CallingConvention = CallingConvention.Cdecl)] internal static extern void kite_decoder_mute(IntPtr decoder, byte mute);
}

internal sealed class MediaReceiver : IDisposable
{
    private readonly Dispatcher dispatcher;
    private readonly Image image;
    private readonly object gate = new();
    private readonly MediaNative.Callbacks callbacks;
    private readonly MediaNative.Pixels pixelsCallback;
    private readonly MediaNative.Error errorCallback;
    private readonly IntPtr decoder;
    private byte[] latest = [];
    private uint width, height;
    private ulong generation, latestGeneration;
    private bool scheduled, disposed;
    private WriteableBitmap? bitmap;
    public ulong Target { get; }
    public MediaReceiver(Image image)
    {
        this.image = image; dispatcher = image.Dispatcher;
        var retain = GCHandle.Alloc(this);
        pixelsCallback = OnPixels;
        errorCallback = (_, code, gen) => MediaNative.kite_windows_media_decoder_failed(Target, gen, code);
        decoder = MediaNative.kite_decoder_create(GCHandle.ToIntPtr(retain), pixelsCallback, errorCallback);
        if (decoder == IntPtr.Zero) { retain.Free(); throw new InvalidOperationException("无法创建 Windows 桌面解码器。"); }
        callbacks = new()
        {
            Release = context => { MediaNative.kite_decoder_destroy(decoder); GCHandle.FromIntPtr(context).Free(); },
            Supports = (_, w, h, fps) => (byte)(w > 0 && h > 0 && w <= 8192 && h <= 4320 && (ulong)w * h <= 16777216 && fps is > 0 and <= 120 ? 1 : 0),
            Configure = (_, w, h, fps, gen) => { lock (gate) { generation = gen; latest = []; } return MediaNative.kite_decoder_configure(decoder, w, h, fps, gen); },
            Frame = (_, bytes, length, pts, flags, gen) => MediaNative.kite_decoder_frame(decoder, bytes, length, pts, flags, gen),
            AudioConfigure = (_, rate, channels, samples, gen) => MediaNative.kite_decoder_audio_configure(decoder, rate, channels, samples, gen),
            AudioFrame = (_, bytes, length, gen) => MediaNative.kite_decoder_audio(decoder, bytes, length, gen),
            Stopped = (_, gen) =>
            {
                lock (gate) { generation = gen; latest = []; }
                MediaNative.kite_decoder_stop(decoder, gen);
                dispatcher.BeginInvoke(() => { lock (gate) if (generation == gen) { image.Source = null; bitmap = null; } });
            }
        };
        var cb = callbacks;
        Target = MediaNative.kite_windows_media_register(GCHandle.ToIntPtr(retain), ref cb);
        if (Target == 0) { MediaNative.kite_decoder_destroy(decoder); retain.Free(); throw new InvalidOperationException("无法注册桌面媒体。"); }
    }
    private void OnPixels(IntPtr context, IntPtr data, uint w, uint h, uint stride, ulong gen)
    {
        try
        {
            lock (gate)
            {
                if (disposed || gen != generation || stride != w * 4 || (ulong)w * h > 16777216) return;
                int length = checked((int)(stride * h));
                if (latest.Length != length) latest = new byte[length];
                Marshal.Copy(data, latest, 0, length); width = w; height = h; latestGeneration = gen;
                if (scheduled) return;
                scheduled = true;
            }
            dispatcher.BeginInvoke(Render, DispatcherPriority.Render);
        }
        catch { MediaNative.kite_windows_media_decoder_failed(Target, gen, -1); }
    }
    private void Render()
    {
        lock (gate)
        {
            scheduled = false;
            if (disposed || latest.Length == 0 || latestGeneration != generation) return;
            if (bitmap is null || bitmap.PixelWidth != width || bitmap.PixelHeight != height)
                bitmap = new WriteableBitmap((int)width, (int)height, 96, 96, PixelFormats.Bgra32, null);
            bitmap.WritePixels(new System.Windows.Int32Rect(0, 0, (int)width, (int)height), latest, (int)width * 4, 0);
            image.Source = bitmap;
            MediaNative.kite_windows_media_frame_rendered(Target, generation);
        }
    }
    public void Mute(bool mute) { if (!disposed) MediaNative.kite_decoder_mute(decoder, (byte)(mute ? 1 : 0)); }
    public void Dispose()
    {
        lock (gate) { if (disposed) return; disposed = true; latest = []; }
        MediaNative.kite_windows_media_release_input(Target);
        MediaNative.kite_windows_media_detach(Target);
    }
}
