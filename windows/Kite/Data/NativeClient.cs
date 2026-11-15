using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Kite;

internal sealed class NativeClient : IDisposable
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void Release(IntPtr context);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate void Changed(IntPtr context, ulong revision);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int Read(IntPtr context, IntPtr name, uint length, IntPtr output, uint capacity);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] private delegate int Write(IntPtr context, IntPtr name, uint length, IntPtr bytes, uint size);
    [StructLayout(LayoutKind.Sequential)] private struct Callbacks { public Release Release; public Changed Changed; public Read Read; public Write Write; }
    [StructLayout(LayoutKind.Sequential)] private struct Buffer { public IntPtr Data; public nuint Length; }
    [DllImport("kite_ffi", CallingConvention = CallingConvention.Cdecl)] private static extern ulong kite_windows_create(IntPtr context, ref Callbacks callbacks);
    [DllImport("kite_ffi", CallingConvention = CallingConvention.Cdecl)] private static extern void kite_windows_destroy(ulong id);
    [DllImport("kite_ffi", CallingConvention = CallingConvention.Cdecl)] private static extern Buffer kite_windows_request(ulong id, byte[] bytes, nuint length);
    [DllImport("kite_ffi", CallingConvention = CallingConvention.Cdecl)] private static extern void kite_windows_buffer_free(Buffer buffer);
    private long handle;
    private readonly Callbacks callbacks;
    private readonly SecretVault vault;
    private long dirty = 1;
    public bool TakeDirty() => Interlocked.Exchange(ref dirty, 0) != 0;
    public void MarkDirty() => Interlocked.Exchange(ref dirty, 1);
    public string DirectoryPath { get; }

    public NativeClient(string directory)
    {
        DirectoryPath = directory;
        vault = new SecretVault(Path.Combine(directory, "secrets"));
        callbacks = new Callbacks
        {
            Release = context => GCHandle.FromIntPtr(context).Free(),
            Changed = (_, _) => MarkDirty(),
            Read = (_, name, length, output, capacity) =>
            {
                byte[]? bytes = null;
                try
                {
                    bytes = vault.Read(Marshal.PtrToStringUTF8(name, checked((int)length))!);
                    if (bytes is null) return -1;
                    if (bytes.Length > capacity) return -2;
                    Marshal.Copy(bytes, 0, output, bytes.Length); return bytes.Length;
                }
                catch { return -2; }
                finally { if (bytes is not null) CryptographicOperations.ZeroMemory(bytes); }
            },
            Write = (_, name, length, data, size) =>
            {
                byte[] bytes = new byte[checked((int)size)];
                try { Marshal.Copy(data, bytes, 0, bytes.Length); vault.Write(Marshal.PtrToStringUTF8(name, checked((int)length))!, bytes); return 0; }
                catch { return -1; }
                finally { CryptographicOperations.ZeroMemory(bytes); }
            }
        };
        var retain = GCHandle.Alloc(this);
        var cb = callbacks;
        handle = checked((long)kite_windows_create(GCHandle.ToIntPtr(retain), ref cb));
        if (handle == 0) { retain.Free(); throw new InvalidOperationException("无法创建 Kite 客户端核心。"); }
    }
    public Task<JsonElement> Call(object command)
    {
        var id = (ulong)Interlocked.Read(ref handle);
        if (id == 0) throw new ObjectDisposedException(nameof(NativeClient));
        var bytes = JsonSerializer.SerializeToUtf8Bytes(command);
        return Task.Run(() =>
        {
            var buffer = kite_windows_request(id, bytes, (nuint)bytes.Length);
            try
            {
                if (buffer.Data == IntPtr.Zero || buffer.Length > 256 * 1024 * 1024) throw new InvalidDataException("无效的核心响应。");
                unsafe
                {
                    using var doc = JsonDocument.Parse(new ReadOnlySpan<byte>((void*)buffer.Data, checked((int)buffer.Length)).ToArray());
                    if (!doc.RootElement.Flag("ok")) throw new InvalidOperationException(doc.RootElement.Text("error"));
                    return doc.RootElement.Field("value").Clone();
                }
            }
            finally { kite_windows_buffer_free(buffer); }
        });
    }
    public void Dispose() { var id = Interlocked.Exchange(ref handle, 0); if (id != 0) kite_windows_destroy((ulong)id); }
}
