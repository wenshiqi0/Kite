using System.Diagnostics;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;

namespace Kite;

internal sealed class RemoteInput : IDisposable
{
    private readonly TextBox surface;
    private readonly ulong target;
    private readonly DispatcherTimer timer;
    private readonly HashSet<Key> forwarded = [];
    private (short X, short Y)? position;
    private int horizontal, vertical;
    private readonly StringBuilder text = new();
    private long textDeadline;
    public int Width { get; set; }
    public int Height { get; set; }
    public bool Enabled { get; private set; }
    public RemoteInput(TextBox surface, ulong target)
    {
        this.surface = surface; this.target = target;
        timer = new DispatcherTimer(TimeSpan.FromMilliseconds(16), DispatcherPriority.Input, (_, _) => Flush(false), surface.Dispatcher);
        surface.PreviewMouseMove += Move;
        surface.PreviewMouseDown += Down;
        surface.PreviewMouseUp += Up;
        surface.PreviewMouseWheel += Wheel;
        surface.PreviewKeyDown += KeyDown;
        surface.PreviewKeyUp += KeyUp;
        surface.PreviewTextInput += Text;
        surface.LostKeyboardFocus += (_, _) => Release();
    }
    public void SetEnabled(bool enabled)
    {
        if (Enabled == enabled) return;
        Release(); Enabled = enabled;
        surface.Cursor = enabled ? Cursors.Arrow : Cursors.Hand;
        if (enabled) surface.Focus();
    }
    public void Release()
    {
        Enabled = false; position = null; horizontal = vertical = 0; text.Clear(); forwarded.Clear();
        if (surface.IsMouseCaptured) surface.ReleaseMouseCapture();
        MediaNative.kite_windows_media_release_input(target);
    }
    private (short X, short Y)? Point(MouseEventArgs e)
    {
        var p = e.GetPosition(surface);
        return Viewport.Fit(surface.ActualWidth, surface.ActualHeight, Width, Height).Point(p.X, p.Y, Width, Height);
    }
    private void Move(object sender, MouseEventArgs e) { if (Enabled) { position = Point(e); e.Handled = true; } }
    private static byte Button(MouseButton b) => b switch { MouseButton.Left => 1, MouseButton.Middle => 2, MouseButton.Right => 3, MouseButton.XButton1 => 4, _ => 5 };
    private void Down(object sender, MouseButtonEventArgs e)
    {
        if (!Enabled || Point(e) is not { } p) return;
        position = p; Flush(true); surface.Focus(); surface.CaptureMouse();
        MediaNative.kite_windows_media_mouse_button(target, Button(e.ChangedButton), 1); e.Handled = true;
    }
    private void Up(object sender, MouseButtonEventArgs e)
    {
        if (!Enabled) return;
        Flush(true); MediaNative.kite_windows_media_mouse_button(target, Button(e.ChangedButton), 0);
        if (e.LeftButton == MouseButtonState.Released && e.RightButton == MouseButtonState.Released && e.MiddleButton == MouseButtonState.Released) surface.ReleaseMouseCapture();
        e.Handled = true;
    }
    private void Wheel(object sender, MouseWheelEventArgs e)
    {
        if (!Enabled) return;
        if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift)) horizontal = Math.Clamp(horizontal + e.Delta, short.MinValue, short.MaxValue);
        else vertical = Math.Clamp(vertical + e.Delta, short.MinValue, short.MaxValue);
        e.Handled = true;
    }
    private static byte Modifiers() => (byte)((Keyboard.Modifiers.HasFlag(ModifierKeys.Shift) ? 1 : 0) |
        (Keyboard.Modifiers.HasFlag(ModifierKeys.Control) ? 2 : 0) |
        (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt) ? 4 : 0) |
        (Keyboard.Modifiers.HasFlag(ModifierKeys.Windows) ? 8 : 0));
    private void KeyDown(object sender, KeyEventArgs e)
    {
        if (!Enabled) return;
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (key is Key.ImeProcessed or Key.DeadCharProcessed) return;
        // Printable unmodified keys arrive once as committed TextInput, including IME.
        bool textKey = key is >= Key.A and <= Key.Z or >= Key.D0 and <= Key.D9 or >= Key.NumPad0 and <= Key.Divide or >= Key.Oem1 and <= Key.OemBackslash or Key.Space;
        if (textKey && (Modifiers() & 14) == 0) return;
        Flush(true);
        if (forwarded.Add(key)) MediaNative.kite_windows_media_key(target, (short)KeyInterop.VirtualKeyFromKey(key), 1, Modifiers());
        e.Handled = true;
    }
    private void KeyUp(object sender, KeyEventArgs e)
    {
        var key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (!Enabled || !forwarded.Remove(key)) return;
        Flush(true); MediaNative.kite_windows_media_key(target, (short)KeyInterop.VirtualKeyFromKey(key), 0, Modifiers()); e.Handled = true;
    }
    private void Text(object sender, TextCompositionEventArgs e)
    {
        if (!Enabled || string.IsNullOrEmpty(e.Text) || (Modifiers() & 14) != 0) return;
        if (text.Length == 0) textDeadline = Stopwatch.GetTimestamp() + Stopwatch.Frequency / 5;
        text.Append(e.Text);
        if (Encoding.UTF8.GetByteCount(text.ToString()) >= 3000) Flush(true);
        e.Handled = true;
    }
    private void Flush(bool barrier)
    {
        if (!Enabled) return;
        if (position is { } p) { MediaNative.kite_windows_media_mouse_position(target, p.X, p.Y, (short)Width, (short)Height); position = null; }
        if (horizontal != 0 || vertical != 0) { MediaNative.kite_windows_media_scroll(target, (short)horizontal, (short)vertical); horizontal = vertical = 0; }
        if (text.Length > 0 && (barrier || Stopwatch.GetTimestamp() >= textDeadline))
        {
            byte[] bytes = Encoding.UTF8.GetBytes(text.ToString());
            MediaNative.kite_windows_media_text(target, bytes, (uint)bytes.Length); text.Clear();
        }
    }
    public void Dispose() { timer.Stop(); Release(); }
}
