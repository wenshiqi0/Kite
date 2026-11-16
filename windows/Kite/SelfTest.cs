using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Kite;

internal static class SelfTest
{
    public static int RunUi(string output)
    {
        string directory = Path.Combine(Path.GetTempPath(), "kite-ui-test-" + Guid.NewGuid().ToString("N"));
        var app = new System.Windows.Application();
        Ui.Configure(app);
        System.Windows.Media.RenderOptions.ProcessRenderMode = System.Windows.Interop.RenderMode.SoftwareOnly;
        app.DispatcherUnhandledException += (_, e) => { Console.Error.WriteLine(e.Exception); e.Handled = true; app.Shutdown(1); };
        var window = new MainWindow(directory, restoreConnections: false);
        window.ContentRendered += async (_, _) =>
        {
            try
            {
                for (int n = 0; n < 100 && !window.IsCoreInitialized; ++n) await Task.Delay(100);
                Check(window.IsCoreInitialized, "WPF window initialization failed");
                await Task.Delay(200);
                window.UpdateLayout();
                var bitmap = new System.Windows.Media.Imaging.RenderTargetBitmap((int)window.ActualWidth, (int)window.ActualHeight, 96, 96, System.Windows.Media.PixelFormats.Pbgra32);
                bitmap.Render(window);
                var png = new System.Windows.Media.Imaging.PngBitmapEncoder();
                png.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bitmap));
                using (var file = File.Create(output)) png.Save(file);
                app.Shutdown(0);
            }
            catch (Exception error) { Console.Error.WriteLine(error); app.Shutdown(1); }
        };
        try { return app.Run(window); }
        finally { try { if (Directory.Exists(directory)) Directory.Delete(directory, true); } catch (IOException) { } }
    }
    private static void Check(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
    public static int Run()
    {
        string directory = Path.Combine(Path.GetTempPath(), "kite-windows-test-" + Guid.NewGuid().ToString("N"));
        try
        {
            Directory.CreateDirectory(directory);
            TestMirror(); TestViewport();
            var vault = new SecretVault(Path.Combine(directory, "secrets"));
            Check(vault.Read("ssh-ed25519") is null, "missing secret");
            byte[] secret = Encoding.UTF8.GetBytes("owned test identity 中文"); vault.Write("ssh-ed25519", secret);
            Check(vault.Read("ssh-ed25519")!.SequenceEqual(secret), "DPAPI round trip");
            var path = Path.Combine(directory, "secrets", "ssh-ed25519.dpapi");
            Check(!Encoding.UTF8.GetString(File.ReadAllBytes(path)).Contains("owned test identity", StringComparison.Ordinal), "plaintext secret on disk");
            File.WriteAllBytes(path, [1, 2, 3]);
            try { vault.Read("ssh-ed25519"); throw new InvalidOperationException("corruption silently accepted"); } catch (CryptographicException) { }
            Directory.Delete(Path.Combine(directory, "secrets"), true);
            TestCore(directory).GetAwaiter().GetResult();
            File.WriteAllBytes(path, [1, 2, 3]);
            using (var broken = new NativeClient(directory))
            {
                try { broken.Call(new { op = "initialize", data_directory = directory, device_name = "fixture" }).GetAwaiter().GetResult(); throw new InvalidOperationException("corrupt device identity regenerated"); }
                catch (InvalidOperationException e) when (e.Message.Contains("secret read failed", StringComparison.Ordinal)) { }
                Check(File.ReadAllBytes(path).SequenceEqual(new byte[] { 1, 2, 3 }), "corrupt identity overwritten");
            }
            File.WriteAllText(Path.Combine(directory, "passed.txt"), "DPAPI, identity persistence, host management, snapshots, deltas, viewport passed");
            Console.WriteLine("PASS: Windows native client self-test"); return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
        finally { try { Directory.Delete(directory, true); } catch (IOException) { } }
    }
    private static async Task TestCore(string directory)
    {
        string key;
        using (var core = new NativeClient(directory))
        {
            await core.Call(new { op = "initialize", data_directory = directory, device_name = "isolated Windows fixture" });
            var snapshot = await core.Call(new { op = "snapshot", after_revision = (long?)null }); key = snapshot.Field("device_key").Text("fingerprint"); Check(key.Length > 0, "identity missing");
            var unchanged = await core.Call(new { op = "snapshot", after_revision = snapshot.Number("revision") }); Check(unchanged.ValueKind == JsonValueKind.Null, "unchanged snapshot transmitted");
            var id = await core.Call(new { op = "save_host", host = new Host("", "fixture.invalid", 22, "fixture", null, null, 47990) });
            await core.Call(new { op = "save_host", host = new Host(id.Text(), "edited.invalid", 2222, "fixture", null, null, 47990) });
            var second = await core.Call(new { op = "save_host", host = new Host("", "other.invalid", 22, "fixture", null, null, 47990) });
            await core.Call(new { op = "select_host", host_id = second.Text() });
            snapshot = await core.Call(new { op = "snapshot", after_revision = (long?)null }); Check(snapshot.Array("hosts").Count() == 2, "hosts not persisted"); Check(snapshot.Text("selected_host") == second.Text(), "host selection");
            Check(snapshot.Array("hosts").Any(h => h.Text("host") == "edited.invalid" && h.Number("port") == 2222), "host edit");
        }
        using (var core = new NativeClient(directory))
        {
            await core.Call(new { op = "initialize", data_directory = directory, device_name = "fixture" });
            var snapshot = await core.Call(new { op = "snapshot", after_revision = (long?)null }); Check(snapshot.Field("device_key").Text("fingerprint") == key, "device identity changed"); Check(snapshot.Array("hosts").Count() == 2, "host persistence after restart");
            await core.Call(new { op = "remove_host", host_id = snapshot.Array("hosts").First().Text("id") });
            snapshot = await core.Call(new { op = "snapshot", after_revision = (long?)null }); Check(snapshot.Array("hosts").Count() == 1, "forget host");
        }
    }
    internal static void TestMirror()
    {
        var mirror = new ConversationMirror();
        using (var doc = JsonDocument.Parse("""{"context":"a","revision":3,"reset":true,"edits":[{"at":0,"remove":0,"items":[{"id":"one"},{"id":"two"}]}]}""")) mirror.Apply(doc.RootElement);
        using (var doc = JsonDocument.Parse("""{"context":"a","revision":4,"reset":false,"edits":[{"at":1,"remove":1,"items":[{"id":"edited"}]}]}""")) mirror.Apply(doc.RootElement);
        Check(mirror.Items.Count == 2 && mirror.Items[1].Text("id") == "edited", "conversation replacement");
        using (var doc = JsonDocument.Parse("""{"context":"a","revision":2,"reset":true,"edits":[]}""")) mirror.Apply(doc.RootElement);
        Check(mirror.Items.Count == 2, "stale delta accepted");
        using (var doc = JsonDocument.Parse("""{"context":"a","revision":5,"reset":false,"edits":[{"at":9,"remove":1,"items":[]}]}"""))
        {
            try { mirror.Apply(doc.RootElement); throw new InvalidOperationException("invalid edit accepted"); } catch (InvalidDataException) { }
        }
        Check(mirror.Revision == 4 && mirror.Items.Count == 2, "partial delta mutation");
        using (var doc = JsonDocument.Parse("""{"context":"b","revision":1,"reset":true,"edits":[]}""")) mirror.Apply(doc.RootElement);
        Check(mirror.Context == "b" && mirror.Items.Count == 0, "cross-host transcript leak");
    }
    internal static void TestViewport()
    {
        var viewport = Viewport.Fit(1000, 1000, 1920, 1080);
        Check(viewport.Point(500, 100, 1920, 1080) is null, "letterbox sends remote input");
        Check(viewport.Point(500, 500, 1920, 1080) == (960, 540), "desktop coordinate scale");
        Check(Viewport.Fit(0, 0, 0, 0).Point(0, 0, 0, 0) is null, "zero-size input");
    }
}
