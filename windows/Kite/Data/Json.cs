using System.Text.Json;

namespace Kite;

internal static class Json
{
    public static JsonElement Field(this JsonElement e, string key) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(key, out var value) ? value : default;
    public static string Text(this JsonElement e, string key = "")
    {
        var v = key.Length == 0 ? e : e.Field(key);
        return v.ValueKind == JsonValueKind.String ? v.GetString() ?? "" : "";
    }
    public static bool Flag(this JsonElement e, string key) => e.Field(key).ValueKind == JsonValueKind.True;
    public static long Number(this JsonElement e, string key) => e.Field(key).TryNumber();
    public static long TryNumber(this JsonElement e) => e.ValueKind == JsonValueKind.Number && e.TryGetInt64(out var n) ? n : 0;
    public static IEnumerable<JsonElement> Array(this JsonElement e, string key = "")
    {
        var v = key.Length == 0 ? e : e.Field(key);
        return v.ValueKind == JsonValueKind.Array ? v.EnumerateArray() : Enumerable.Empty<JsonElement>();
    }
}

internal sealed record Host(string id, string host, ushort port, string username,
    string? herdr_path, string? stream_host, ushort sunshine_port)
{
    public string Title => $"{username}@{host}:{port}";
    public static Host From(JsonElement e) => new(e.Text("id"), e.Text("host"),
        (ushort)e.Number("port"), e.Text("username"),
        string.IsNullOrEmpty(e.Text("herdr_path")) ? null : e.Text("herdr_path"),
        string.IsNullOrEmpty(e.Text("stream_host")) ? null : e.Text("stream_host"), (ushort)e.Number("sunshine_port"));
}

internal sealed class ConversationMirror
{
    public string Context { get; private set; } = "";
    public long Revision { get; private set; }
    public List<JsonElement> Items { get; } = [];
    public void Apply(JsonElement delta)
    {
        var context = delta.Text("context");
        var revision = delta.Number("revision");
        if (context == Context && revision < Revision) return;
        var reset = delta.Flag("reset");
        if (context != Context && !reset) throw new InvalidDataException("conversation context changed without reset");
        // Validate the whole edit transaction before replacing the mirror.
        var next = reset ? new List<JsonElement>() : new List<JsonElement>(Items);
        foreach (var edit in delta.Array("edits"))
        {
            var at = checked((int)edit.Number("at"));
            var remove = checked((int)edit.Number("remove"));
            if (at < 0 || remove < 0 || at > next.Count || remove > next.Count - at)
                throw new InvalidDataException("invalid conversation edit");
            next.RemoveRange(at, remove);
            next.InsertRange(at, edit.Array("items").Select(x => x.Clone()));
        }
        Context = context; Revision = revision;
        Items.Clear(); Items.AddRange(next);
    }
    public void Reset() { Context = ""; Revision = 0; Items.Clear(); }
}

internal readonly record struct Viewport(double Left, double Top, double Width, double Height)
{
    public static Viewport Fit(double availableWidth, double availableHeight, int sourceWidth, int sourceHeight)
    {
        if (availableWidth <= 0 || availableHeight <= 0 || sourceWidth <= 0 || sourceHeight <= 0) return default;
        double scale = Math.Min(availableWidth / sourceWidth, availableHeight / sourceHeight);
        double w = sourceWidth * scale, h = sourceHeight * scale;
        return new((availableWidth - w) / 2, (availableHeight - h) / 2, w, h);
    }
    public (short X, short Y)? Point(double x, double y, int sourceWidth, int sourceHeight)
    {
        if (Width <= 0 || Height <= 0 || x < Left || y < Top || x >= Left + Width || y >= Top + Height) return null;
        return ((short)Math.Clamp((x - Left) / Width * sourceWidth, 0, sourceWidth - 1),
            (short)Math.Clamp((y - Top) / Height * sourceHeight, 0, sourceHeight - 1));
    }
}
