using System.Windows;
using System.Windows.Controls;

namespace Kite;

internal sealed class HostEditor : Window
{
    public HostEditor(Window owner, NativeClient client, Host? host)
    {
        Owner = owner; Title = host is null ? "添加远程主机" : "主机设置";
        Width = 460; SizeToContent = SizeToContent.Height; ResizeMode = ResizeMode.NoResize; WindowStartupLocation = WindowStartupLocation.CenterOwner;
        var panel = new StackPanel { Margin = new Thickness(24) }; Content = panel;
        panel.Children.Add(Ui.Label(Title, 23));
        panel.Children.Add(Ui.Label("连接已配置的 Kite 主机。将此设备公钥加入主机的 SSH 授权后，即可连接。", 13, Ui.Muted));
        var address = Ui.Field(panel, "主机地址 / Tailscale 地址", host?.host ?? "");
        var username = Ui.Field(panel, "SSH 用户名", host?.username ?? "");
        var port = Ui.Field(panel, "SSH 端口", (host?.port ?? 22).ToString());
        var herdr = Ui.Field(panel, "Herdr 路径（可留空）", host?.herdr_path ?? "");
        var stream = Ui.Field(panel, "桌面串流地址（可留空）", host?.stream_host ?? "");
        var sunshine = Ui.Field(panel, "Sunshine HTTPS 端口", (host?.sunshine_port ?? 47990).ToString());
        panel.Children.Add(Ui.Row(Ui.Button("保存", async () =>
        {
            if (string.IsNullOrWhiteSpace(address.Text) || string.IsNullOrWhiteSpace(username.Text) || !ushort.TryParse(port.Text, out var sshPort) || sshPort == 0 || !ushort.TryParse(sunshine.Text, out var sunPort) || sunPort == 0)
                throw new InvalidOperationException("请填写主机地址、用户名和有效端口（1–65535）。");
            await client.Call(new { op = "save_host", host = new Host(host?.id ?? "", address.Text.Trim(), sshPort, username.Text.Trim(), Empty(herdr.Text), Empty(stream.Text), sunPort) });
            DialogResult = true;
        }), Ui.Button("取消", () => { Close(); return Task.CompletedTask; })));
    }
    private static string? Empty(string value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
