using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace Kite;

internal sealed class SecretVault(string directory)
{
    private readonly object gate = new();
    private string PathFor(string name) => name is "ssh-ed25519" or "sunshine-identity"
        ? Path.Combine(directory, name + ".dpapi") : throw new ArgumentException("unknown device secret");
    public byte[]? Read(string name)
    {
        lock (gate)
        {
            var path = PathFor(name);
            // Only absence creates a new identity. Corruption/decryption failures propagate.
            try { return Transform(File.ReadAllBytes(path), false); }
            catch (FileNotFoundException) { return null; }
            catch (DirectoryNotFoundException) { return null; }
        }
    }
    public void Write(string name, byte[] value)
    {
        lock (gate)
        {
            Directory.CreateDirectory(directory);
            var path = PathFor(name);
            var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                {
                    var encrypted = Transform(value, true);
                    stream.Write(encrypted); stream.Flush(true);
                }
                File.Move(temporary, path, true);
            }
            finally { if (File.Exists(temporary)) File.Delete(temporary); }
        }
    }
    [StructLayout(LayoutKind.Sequential)] private struct Blob { public int Size; public IntPtr Data; }
    [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CryptProtectData(ref Blob data, string? description, ref Blob entropy, IntPtr reserved, IntPtr prompt, uint flags, out Blob output);
    [DllImport("crypt32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CryptUnprotectData(ref Blob data, IntPtr description, ref Blob entropy, IntPtr reserved, IntPtr prompt, uint flags, out Blob output);
    [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr memory);
    private static unsafe byte[] Transform(byte[] bytes, bool protect)
    {
        byte[] entropy = Encoding.UTF8.GetBytes("Kite.Windows.DeviceIdentity.v1");
        fixed (byte* data = bytes, extra = entropy)
        {
            var input = new Blob { Size = bytes.Length, Data = (IntPtr)data };
            var salt = new Blob { Size = entropy.Length, Data = (IntPtr)extra };
            Blob output;
            bool ok = protect ? CryptProtectData(ref input, "Kite device identity", ref salt, IntPtr.Zero, IntPtr.Zero, 1, out output)
                : CryptUnprotectData(ref input, IntPtr.Zero, ref salt, IntPtr.Zero, IntPtr.Zero, 1, out output);
            if (!ok) throw new CryptographicException(Marshal.GetLastWin32Error());
            try { var result = new byte[output.Size]; Marshal.Copy(output.Data, result, 0, result.Length); return result; }
            finally { if (output.Data != IntPtr.Zero) { new Span<byte>((void*)output.Data, output.Size).Clear(); LocalFree(output.Data); } }
        }
    }
}
