using System.Text.Json;
using System.Text.RegularExpressions;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

namespace Kite;

internal sealed record ChatRow(JsonElement Item, Func<string, string, Task> Save, string Context);

public sealed class ChatItemControl : ContentControl
{
    public ChatItemControl() { DataContextChanged += (_, _) => Render(); }
    private void Render()
    {
        if (DataContext is not ChatRow row) return;
        var panel = new StackPanel { Margin = new Thickness(6, 10, 12, 12) };
        string role = row.Item.Text("role");
        panel.Children.Add(Ui.Label(role switch { "User" => "你", "Assistant" => "Kite", "Error" => "错误", _ => "工具" }, 12, role == "User" ? Ui.Accent : Ui.Muted));
        Blocks(panel, row.Item.Array("blocks"), row, 0);
        Content = panel;
    }
    private static void Blocks(Panel panel, IEnumerable<JsonElement> blocks, ChatRow row, int depth)
    {
        if (depth > 4) { panel.Children.Add(Ui.Label("嵌套内容过深，请在原会话查看。", 12, Ui.Muted)); return; }
        foreach (var block in blocks)
        {
            var text = block.Field("Text");
            if (text.ValueKind == JsonValueKind.Object) panel.Children.Add(Markdown(text.Text("text")));
            var tool = block.Field("Tool").Field("tool");
            if (tool.ValueKind == JsonValueKind.Object)
            {
                var expander = new Expander { Header = (tool.Flag("failed") ? "失败 · " : tool.Flag("complete") ? "✓ " : "进行中 · ") + tool.Text("label") + "  " + tool.Text("summary"), Foreground = Ui.Muted, Margin = new Thickness(0, 3, 0, 3) };
                expander.Expanded += (_, _) =>
                {
                    if (expander.Content is not null) return;
                    expander.Content = new TextBox { Text = tool.Text("arguments") + "\n" + tool.Text("output"), IsReadOnly = true, TextWrapping = TextWrapping.Wrap, MaxHeight = 380, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, FontFamily = new FontFamily("Cascadia Mono, Consolas") };
                };
                panel.Children.Add(expander);
            }
            var file = block.Field("Attachment").Field("file");
            if (file.ValueKind == JsonValueKind.Object)
            {
                string id = file.Text("id"), name = file.Text("name");
                panel.Children.Add(Ui.Button("↓ " + name, () => row.Save(id, name)));
            }
            var reply = block.Field("QuestionReply").Field("reply");
            if (reply.ValueKind == JsonValueKind.Object) panel.Children.Add(Markdown(reply.Text("question") + "\n\n" + reply.Text("answer")));
            var reference = block.Field("Embed").Field("reference");
            if (reference.Field("Snapshot").ValueKind == JsonValueKind.Object) Blocks(panel, reference.Field("Snapshot").Array("blocks"), row, depth + 1);
            if (reference.Field("Live").ValueKind == JsonValueKind.Object) panel.Children.Add(Ui.Label("引用会话：" + reference.Field("Live").Text("conversation"), 12, Ui.Muted));
        }
    }
    private static RichTextBox Markdown(string text)
    {
        var doc = new FlowDocument { PagePadding = new Thickness(0), FontSize = 14, Foreground = Ui.Foreground };
        bool code = false;
        foreach (string line in text.Replace("\r\n", "\n").Split('\n'))
        {
            if (line.StartsWith("```", StringComparison.Ordinal)) { code = !code; continue; }
            var paragraph = new Paragraph { Margin = new Thickness(0, 0, 0, code ? 0 : 6) };
            if (code) { paragraph.FontFamily = new FontFamily("Cascadia Mono, Consolas"); paragraph.Background = Ui.Panel; paragraph.Inlines.Add(new Run(line)); }
            else
            {
                var heading = Regex.Match(line, "^(#{1,6})\\s+(.*)$");
                string content = heading.Success ? heading.Groups[2].Value : line;
                if (heading.Success) { paragraph.FontSize = Math.Max(16, 24 - heading.Groups[1].Length * 2); paragraph.FontWeight = FontWeights.SemiBold; }
                var parts = Regex.Split(content, "(\\*\\*[^*]+\\*\\*|`[^`]+`)");
                foreach (var part in parts)
                {
                    if (part.StartsWith("**", StringComparison.Ordinal) && part.EndsWith("**", StringComparison.Ordinal) && part.Length > 4) paragraph.Inlines.Add(new Bold(new Run(part[2..^2])));
                    else if (part.StartsWith('`') && part.EndsWith('`') && part.Length > 2) paragraph.Inlines.Add(new Run(part[1..^1]) { FontFamily = new FontFamily("Cascadia Mono, Consolas"), Background = Ui.Panel });
                    else paragraph.Inlines.Add(new Run(part));
                }
            }
            doc.Blocks.Add(paragraph);
        }
        return new RichTextBox { Document = doc, IsReadOnly = true, IsDocumentEnabled = true, Background = Brushes.Transparent, BorderThickness = new Thickness(0), Padding = new Thickness(0), VerticalScrollBarVisibility = ScrollBarVisibility.Disabled, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled };
    }
}
