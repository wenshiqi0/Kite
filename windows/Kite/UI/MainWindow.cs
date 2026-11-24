using System.Collections.ObjectModel;
using System.Net.NetworkInformation;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using Microsoft.Win32;

namespace Kite;

internal sealed class MainWindow : Window
{
    private NativeClient? client;
    private MediaReceiver? media;
    private RemoteInput? input;
    private readonly DispatcherTimer timer, clipboardTimer;
    private JsonElement state;
    private bool rendering, refreshing, clipboardBusy, closing;
    private string context = "", hostSignature = "", sessionSignature = "", interactionSignature = "", modelSignature = "";
    private readonly Dictionary<string, string> drafts = [];
    private readonly HashSet<string> sending = [];
    private readonly ConversationMirror conversation = new();
    private readonly ObservableCollection<ChatRow> messages = [];
    private readonly ListBox hosts = new(), sessions = new(), chat = new();
    private readonly TextBlock title = Ui.Label("欢迎使用 Kite", 22), status = Ui.Label("添加主机，开始远程工作。", 12, Ui.Muted), history = Ui.Label("", 12, Ui.Muted);
    private readonly TextBox sessionSearch = new() { ToolTip = "跨主机搜索会话", Margin = new Thickness(0, 4, 0, 4) };
    private readonly TextBox draft = new() { AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MinHeight = 80, MaxHeight = 220, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
    private readonly ComboBox models = new() { MinWidth = 150, MaxWidth = 250, DisplayMemberPath = "Label", SelectedValuePath = "Id" }, efforts = new() { MinWidth = 85, Margin = new Thickness(6, 0, 6, 0) };
    private readonly StackPanel interactions = new(), uploadPanel = new();
    private readonly CheckBox archived = new() { Content = "显示归档", Foreground = Ui.Muted, Margin = new Thickness(0, 10, 0, 4) };
    private readonly ComboBox sort = new() { ItemsSource = new[] { "最新创建", "最早创建", "最近活动", "标题" }, SelectedIndex = 0, Margin = new Thickness(0, 4, 0, 8) };
    private readonly Image screen = new() { Stretch = Stretch.Uniform };
    private readonly TextBox inputSurface = new() { Background = Brushes.Transparent, Foreground = Brushes.Transparent, CaretBrush = Brushes.Transparent, BorderThickness = new Thickness(0), Cursor = Cursors.Hand, IsReadOnly = false, AcceptsReturn = true, Padding = new Thickness(0), IsUndoEnabled = false };
    private readonly Border desktopPanel = new() { Background = Brushes.Black, Visibility = Visibility.Collapsed };
    private readonly Grid desktopRoot = new();
    private readonly TextBlock desktopStatus = Ui.Label("只看", 12, Ui.Muted);
    private readonly CheckBox clipboard = new() { Content = "共享文本剪贴板", Foreground = Ui.Muted, Margin = new Thickness(8, 8, 0, 4) }, mute = new() { Content = "静音", Foreground = Ui.Muted, Margin = new Thickness(8, 8, 0, 4) };
    private Window? fullscreen;
    private Button send = null!, connect = null!, operate = null!, pair = null!;
    private string HostId => state.Text("selected_host");
    private string AgentId => state.Text("selected_agent");
    private Host? ResourceHost => (hosts.SelectedItem as ListBoxItem)?.Tag as Host;
    private NativeClient Core => client ?? throw new InvalidOperationException("客户端尚未就绪。");
    private readonly string dataDirectory;
    private readonly bool restoreConnections;
    internal bool IsCoreInitialized => state.Flag("initialized");
    private sealed record Selection(string Id, string Label);
    private sealed record SessionTarget(string Host, string Id);

    public MainWindow(string? dataDirectory = null, bool restoreConnections = true)
    {
        this.dataDirectory = dataDirectory ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Kite", "Windows");
        this.restoreConnections = restoreConnections;
        Background = Ui.Background; Foreground = Ui.Foreground; FontFamily = new FontFamily("Segoe UI"); FontSize = 14;
        Title = "Kite · Windows"; Width = 1280; Height = 850; MinWidth = 900; MinHeight = 620;
        WindowStartupLocation = WindowStartupLocation.CenterScreen;
        Content = Build();
        timer = new DispatcherTimer(TimeSpan.FromMilliseconds(150), DispatcherPriority.Background, async (_, _) => await Refresh(), Dispatcher);
        clipboardTimer = new DispatcherTimer(TimeSpan.FromSeconds(1), DispatcherPriority.Background, async (_, _) => await SynchronizeClipboard(), Dispatcher);
        Loaded += async (_, _) => await Initialize();
        Deactivated += (_, _) => LoseInput();
        Activated += async (_, _) => { if (client is not null) await Safe(new { op = "network_changed", available = NetworkInterface.GetIsNetworkAvailable() }); };
        Closing += OnClosing;
        NetworkChange.NetworkAvailabilityChanged += NetworkChanged;
    }
    private Grid Build()
    {
        var root = new Grid(); root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(255) }); root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var sidebar = new DockPanel { Background = Ui.Panel, Margin = new Thickness(0, 0, 1, 0) };
        var brand = new StackPanel { Margin = new Thickness(18, 18, 12, 8) };
        brand.Children.Add(Ui.Label("会话", 22));
        brand.Children.Add(Ui.Row(Ui.Button("＋ 新建", CreateSession), Ui.Button("管理", SessionMenu)));
        brand.Children.Add(Ui.Label("搜索会话", 11, Ui.Muted)); brand.Children.Add(sessionSearch);
        brand.Children.Add(archived); brand.Children.Add(sort);
        sessionSearch.TextChanged += (_, _) => RenderSessions(searchChanged: true);
        DockPanel.SetDock(brand, Dock.Top); sidebar.Children.Add(brand);
        var resources = BuildHostMenu(); DockPanel.SetDock(resources, Dock.Bottom); sidebar.Children.Add(resources);
        sessions.Margin = new Thickness(10, 0, 8, 0); sidebar.Children.Add(sessions); root.Children.Add(sidebar);
        sessions.SelectionChanged += async (_, _) => { if (!rendering && sessions.SelectedItem is ListBoxItem item && item.Tag is SessionTarget target) await Safe(new { op = "open_session", host_id = target.Host, session_id = target.Id }); };
        archived.Checked += (_, _) => RenderSessions(); archived.Unchecked += (_, _) => RenderSessions();
        sort.SelectionChanged += async (_, _) => { if (!rendering && sort.SelectedIndex >= 0) await Safe(new { op = "set_workspace_sort", order = new[] { "Newest", "Oldest", "RecentActivity", "Title" }[sort.SelectedIndex] }); };

        var workspace = new DockPanel { Margin = new Thickness(22, 16, 22, 16) }; Grid.SetColumn(workspace, 1); root.Children.Add(workspace);
        var header = new StackPanel(); header.Children.Add(title); header.Children.Add(status);
        connect = Ui.Button("连接", async () => { string host = HostId; await Core.Call(new { op = state.Text("connection") is "Connected" or "Connecting" or "Reconnecting" or "AwaitingTrust" ? "disconnect_host" : "connect", host_id = host }); });
        header.Children.Add(Ui.Row(connect, Ui.Button("主机详情", HostResources), Ui.Button("远程桌面", OpenDesktop), Ui.Button("断开全部", async () => { LoseInput(); await Core.Call(new { op = "disconnect_all" }); })));
        DockPanel.SetDock(header, Dock.Top); workspace.Children.Add(header);
        var composer = new StackPanel { Margin = new Thickness(0, 12, 0, 0) };
        composer.Children.Add(interactions); composer.Children.Add(uploadPanel);
        composer.Children.Add(Ui.Row(models, efforts, Ui.Button("设置", ComposerSettings), Ui.Button("附件", Upload), Ui.Button("Skills", Skills)));
        composer.Children.Add(draft);
        send = Ui.Button("发送  Ctrl+Enter", Send);
        composer.Children.Add(Ui.Row(send, Ui.Button("中断", async () => await Core.Call(new { op = "interrupt_agent", host_id = HostId, agent_id = AgentId })), Ui.Button("暂停 / 继续 Agent", async () => await Core.Call(new { op = "set_agent_paused", host_id = HostId, agent_id = AgentId, paused = state.Text("agent_pause") != "Paused" })), Ui.Button("重试历史", async () => await Core.Call(new { op = "retry_history" }))));
        composer.Children.Add(history); DockPanel.SetDock(composer, Dock.Bottom); workspace.Children.Add(composer);
        draft.TextChanged += (_, _) => { if (!rendering) drafts[context] = draft.Text; };
        draft.PreviewKeyDown += async (_, e) => { if (e.Key == Key.Enter && Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) { e.Handled = true; await SendGuarded(); } };
        models.SelectionChanged += async (_, _) => { if (rendering) return; UpdateEfforts(); await SetModel(); };
        efforts.SelectionChanged += async (_, _) => { if (!rendering) await SetModel(); };
        var body = new Grid(); body.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) }); body.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); body.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        chat.ItemsSource = messages;
        VirtualizingPanel.SetIsVirtualizing(chat, true); VirtualizingPanel.SetVirtualizationMode(chat, VirtualizationMode.Recycling); ScrollViewer.SetCanContentScroll(chat, true);
        var factory = new FrameworkElementFactory(typeof(ChatItemControl));
        chat.ItemTemplate = new DataTemplate { VisualTree = factory };
        chat.ItemContainerStyle = new Style(typeof(ListBoxItem)) { Setters = { new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Stretch), new Setter(Control.PaddingProperty, new Thickness(0)), new Setter(UIElement.FocusableProperty, false) } };
        body.Children.Add(chat);
        var splitter = new GridSplitter { Height = 6, HorizontalAlignment = HorizontalAlignment.Stretch, Background = Ui.Panel, ResizeDirection = GridResizeDirection.Rows }; Grid.SetRow(splitter, 1); body.Children.Add(splitter);
        BuildDesktop(); desktopPanel.Child = desktopRoot; desktopPanel.Height = 270; Grid.SetRow(desktopPanel, 2); body.Children.Add(desktopPanel);
        workspace.Children.Add(body); return root;
    }
    private Expander BuildHostMenu()
    {
        var panel = new StackPanel();
        hosts.MaxHeight = 160;
        panel.Children.Add(hosts);
        panel.Children.Add(Ui.Row(Ui.Button("连接 / 打开", async () =>
        {
            if (ResourceHost is not { } host) return;
            if (host.id != HostId) LoseInput();
            await Core.Call(new { op = "connect", host_id = host.id });
        }), Ui.Button("断开", async () =>
        {
            if (ResourceHost is not { } host) return;
            if (host.id == HostId) LoseInput();
            await Core.Call(new { op = "disconnect_host", host_id = host.id });
        })));
        panel.Children.Add(Ui.Row(Ui.Button("设置", EditHost), Ui.Button("移除", RemoveHost)));
        panel.Children.Add(Ui.Row(Ui.Button("＋ 主机", () =>
        {
            new HostEditor(this, Core, null).ShowDialog();
            return Task.CompletedTask;
        }), Ui.Button("设备公钥", ShowKey)));
        return new Expander { Header = "主机", Foreground = Ui.Muted, Margin = new Thickness(10, 8, 10, 12), Content = panel };
    }
    private void BuildDesktop()
    {
        desktopRoot.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); desktopRoot.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        operate = Ui.Button("进入操作", async () => { string host = HostId; await Core.Call(new { op = "set_takeover", host_id = host, enabled = !state.Field("desktop").Flag("takeover") }); });
        pair = Ui.Button("确认配对", async () => await Core.Call(new { op = "approve_pairing", host_id = HostId, pin = state.Field("desktop").Text("pin") }));
        var toolbar = Ui.Row(desktopStatus, pair, operate, Ui.Button("全屏", () => { ToggleFullscreen(); return Task.CompletedTask; }), Ui.Button("关闭桌面", CloseDesktop), mute, clipboard);
        toolbar.Background = Ui.Panel; desktopRoot.Children.Add(toolbar);
        var surface = new Grid { ClipToBounds = true }; surface.Children.Add(screen); surface.Children.Add(inputSurface); Grid.SetRow(surface, 1); desktopRoot.Children.Add(surface);
        mute.Checked += (_, _) => media?.Mute(true); mute.Unchecked += (_, _) => media?.Mute(false);
        inputSurface.LostKeyboardFocus += (_, _) => LoseInput();
    }
    private async Task Initialize()
    {
        try
        {
            client = new NativeClient(dataDirectory);
            await client.Call(new { op = "initialize", data_directory = client.DirectoryPath, device_name = Environment.MachineName });
            await Refresh(true);
            await client.Call(new { op = "network_changed", available = NetworkInterface.GetIsNetworkAvailable() });
            if (restoreConnections) await client.Call(new { op = "restore_connection" });
        }
        catch (Exception error) { status.Text = "初始化失败：" + error.Message; }
    }
    private async Task Safe(object command)
    {
        try { await Core.Call(command); } catch (Exception error) { if (!closing) status.Text = error.Message; }
    }
    private async Task Refresh(bool force = false)
    {
        if (client is null || refreshing || closing || (!force && !client.TakeDirty())) return;
        refreshing = true;
        try
        {
            var next = await client.Call(new { op = "snapshot", after_revision = force ? (long?)null : state.ValueKind == JsonValueKind.Object ? state.Number("revision") : null });
            if (next.ValueKind != JsonValueKind.Object) return;
            state = next;
            RenderState();
            if (state.Text("conversation_context") != conversation.Context || state.Number("conversation_revision") != conversation.Revision)
            {
                var delta = await client.Call(new { op = "conversation", context = conversation.Context, revision = conversation.Revision });
                // Reject a response if a target selection changed while it was in flight.
                if (delta.Text("context") != state.Text("conversation_context")) { client.MarkDirty(); return; }
                conversation.Apply(delta); RenderMessages();
            }
        }
        catch (Exception error) { status.Text = error.Message; client.MarkDirty(); }
        finally { refreshing = false; }
    }
    private void RenderState()
    {
        rendering = true;
        try
        {
            string nextContext = state.Text("conversation_context");
            if (context != nextContext) { drafts[context] = draft.Text; context = nextContext; draft.Text = drafts.GetValueOrDefault(context, ""); messages.Clear(); }
            var host = state.Array("hosts").FirstOrDefault(h => h.Text("id") == HostId);
            title.Text = HostId.Length > 0 ? $"{host.Text("username")}@{host.Text("host")}" : "欢迎使用 Kite";
            string connection = state.Text("connection");
            string phase = connection switch { "Connected" => "已连接", "Connecting" => "连接中", "Reconnecting" => "自动重连中", "AwaitingTrust" => "等待确认主机指纹", "Failed" => "连接失败", _ => "未连接" };
            var failure = state.Field("failure");
            status.Text = phase + (failure.ValueKind == JsonValueKind.Object ? " · " + failure.Text("details") : " · " + state.Text("response_phase"));
            connect.Content = connection is "Connected" or "Connecting" or "Reconnecting" or "AwaitingTrust" ? "断开" : "连接"; connect.IsEnabled = HostId.Length > 0;
            send.IsEnabled = connection == "Connected" && AgentId.Length > 0;
            RenderHosts(); RenderSessions(); RenderComposer(); RenderInteractions();
            var desktop = state.Field("desktop"); bool active = desktop.Text("status") == "Streaming";
            if (input is not null) { input.Width = (int)desktop.Number("source_width"); input.Height = (int)desktop.Number("source_height"); input.SetEnabled(active && desktop.Flag("takeover")); }
            desktopStatus.Text = desktop.Text("status") switch { "Pairing" => "配对 PIN：" + desktop.Text("pin"), "Starting" => "连接桌面中…", "Streaming" => $"{desktop.Number("width")}×{desktop.Number("height")} · " + (desktop.Flag("takeover") ? "操作" : "只看"), "Failed" => "桌面连接失败", _ => "桌面未连接" };
            pair.Visibility = desktop.Text("status") == "Pairing" ? Visibility.Visible : Visibility.Collapsed;
            operate.Content = desktop.Flag("takeover") ? "返回只看" : "进入操作"; operate.IsEnabled = active;
            var h = state.Field("history"); history.Text = h.Flag("loading") ? "正在同步聊天历史…" : h.Flag("waiting") ? "等待首条对话…" : h.Flag("failed") ? "历史同步失败：" + h.Text("details") : state.Field("composer").Text("details");
        }
        finally { rendering = false; }
    }
    private void RenderHosts()
    {
        string signature = state.Field("hosts").GetRawText() + string.Join(";", state.Array("host_connections").Select(h => h.Text("host_id") + h.Text("connection")));
        if (signature == hostSignature) return;
        hostSignature = signature;
        string selected = ResourceHost?.id ?? HostId;
        hosts.Items.Clear();
        foreach (var h in state.Array("hosts"))
        {
            var host = Host.From(h); var connection = state.Array("host_connections").FirstOrDefault(c => c.Text("host_id") == host.id).Text("connection");
            var item = new ListBoxItem { Content = new TextBlock { Text = (connection == "Connected" ? "● " : "○ ") + host.Title, TextTrimming = TextTrimming.CharacterEllipsis, Margin = new Thickness(2, 8, 2, 8) }, Tag = host, ToolTip = host.Title };
            hosts.Items.Add(item);
            if (host.id == selected) hosts.SelectedItem = item;
        }
        if (hosts.SelectedIndex < 0 && hosts.Items.Count > 0) hosts.SelectedIndex = 0;
    }
    private void RenderSessions(bool searchChanged = false)
    {
        var order = state.Field("workspace_session_order");
        if (order.ValueKind != JsonValueKind.Array) return;
        string query = sessionSearch.Text.Trim();
        string selectedId = state.Text("selected_session");
        string ordering = state.Text("workspace_session_sort");
        var workspaces = state.Array("host_connections").ToArray();
        string signature = order.GetRawText() + state.Field("hosts").GetRawText() + HostId + selectedId + ordering + archived.IsChecked + query
            + string.Concat(workspaces.Select(h => h.Text("connection") + h.Field("sessions").GetRawText()));
        if (signature == sessionSignature) return;
        var hostNames = state.Array("hosts").ToDictionary(host => host.Text("id"), host => host.Text("host"));
        var connections = workspaces.ToDictionary(workspace => workspace.Text("host_id"), workspace => workspace.Text("connection"));
        var rows = workspaces.SelectMany(workspace => workspace.Array("sessions")
            .Select(session => (Key: new SessionTarget(workspace.Text("host_id"), session.Text("id")), Session: session)))
            .ToDictionary(row => row.Key, row => row.Session);
        var previousGroups = sessions.Items.OfType<ListBoxItem>()
            .Where(item => item.Tag is SessionTarget && item.Content is StackPanel)
            .ToDictionary(item => (SessionTarget)item.Tag, item => ((StackPanel)item.Content).Children.OfType<Expander>().FirstOrDefault()?.IsExpanded);
        bool before = rendering;
        rendering = true;
        try
        {
            sessions.Items.Clear();
            foreach (var key in order.Array())
            {
                var target = new SessionTarget(key.Text("host_id"), key.Text("session_id"));
                if (!rows.TryGetValue(target, out var session) || !hostNames.TryGetValue(target.Host, out var hostName)) continue;
                bool selected = target.Host == HostId && target.Id == selectedId;
                if (session.Flag("archived") && archived.IsChecked != true) continue;
                if (session.Flag("delegated_child") && query.Length == 0 && !selected) continue;
                if (!MatchesSession(session, hostName, query)) continue;
                bool expanded = !searchChanged && previousGroups.GetValueOrDefault(target) is { } previous
                    ? previous : query.Length > 0;
                var item = BuildSessionRow(target, session, hostName, connections[target.Host] == "Connected", expanded);
                sessions.Items.Add(item);
                if (selected) sessions.SelectedItem = item;
            }
            if (sessions.Items.IsEmpty)
                sessions.Items.Add(new ListBoxItem { Content = Ui.Label(query.Length == 0 ? "尚无会话" : "没有匹配的会话", 12, Ui.Muted), IsEnabled = false });
            sort.SelectedIndex = ordering switch { "Oldest" => 1, "RecentActivity" => 2, "Title" => 3, _ => 0 };
            sessionSignature = signature;
        }
        finally { rendering = before; }
    }
    private static bool MatchesSession(JsonElement session, string host, string query)
    {
        if (query.Length == 0) return true;
        bool Matches(string value) => value.Contains(query, StringComparison.CurrentCultureIgnoreCase);
        return new[] { host, session.Text("title"), session.Text("preview"), session.Text("provider") }.Any(Matches)
            || session.Array("subagents").Any(task =>
                new[] { task.Text("name"), task.Text("task"), task.Text("machine"), task.Text("provider") }.Any(Matches));
    }
    private ListBoxItem BuildSessionRow(SessionTarget target, JsonElement session, string hostName, bool connected, bool expanded)
    {
        string provider = ProviderLabel(session.Text("provider")), host = target.Host, id = target.Id;
        bool isArchived = session.Flag("archived");
        var body = new StackPanel { Margin = new Thickness(2, 7, 2, 7) };
        string name = session.Text("title") is { Length: > 0 } title ? title : provider;
        body.Children.Add(new TextBlock { Text = (isArchived ? "归档 · " : "") + name, TextTrimming = TextTrimming.CharacterEllipsis });
        string status = connected ? AssociatedStatus(session.Text("status")) : "离线";
        body.Children.Add(new TextBlock { Text = $"{provider} · {hostName} · {status}", FontSize = 11, Foreground = Ui.Muted, TextTrimming = TextTrimming.CharacterEllipsis });
        var related = session.Array("subagents").ToArray();
        if (related.Length > 0)
        {
            var group = new Expander { Header = $"关联任务 · {related.Length}", Foreground = Ui.Muted, IsExpanded = expanded };
            var children = new StackPanel { Margin = new Thickness(10, 0, 0, 0) };
            foreach (var child in related)
            {
                string caption = string.Join(" · ", new[] { child.Text("name"), child.Text("machine"),
                    child.Text("provider") is { Length: > 0 } childProvider ? ProviderLabel(childProvider) : "",
                    AssociatedStatus(child.Text("status")) }.Where(field => field.Length > 0));
                children.Children.Add(Ui.Button(caption, () => InspectAssociatedTask(host, id, child)));
            }
            group.Content = children; body.Children.Add(group);
        }
        var item = new ListBoxItem { Content = body, Tag = target, HorizontalContentAlignment = HorizontalAlignment.Stretch };
        item.ContextMenu = new ContextMenu();
        var archive = new MenuItem { Header = isArchived ? "取消归档" : "归档" }; archive.Click += async (_, _) => await Safe(new { op = "archive_session", host_id = host, session_id = id, archived = !isArchived }); item.ContextMenu.Items.Add(archive);
        var resume = new MenuItem { Header = "继续此会话" }; resume.Click += async (_, _) => await Safe(new { op = "resume_session", host_id = host, session_id = id }); item.ContextMenu.Items.Add(resume);
        var delete = new MenuItem { Header = "删除会话…" }; delete.Click += async (_, _) => { if (MessageBox.Show(this, "删除会话将终止此会话正在执行的任务。继续？", "删除会话", MessageBoxButton.YesNo) == MessageBoxResult.Yes) await Safe(new { op = "delete_session", host_id = host, session_id = id }); }; item.ContextMenu.Items.Add(delete);
        return item;
    }
    private static string AssociatedStatus(string status) => status switch
    {
        "working" => "运行中", "starting" => "启动中", "blocked" => "待审批", "question" => "待回答",
        "done" => "已完成", "failed" => "失败", "offline" => "离线",
        "released" => "已释放", "stopped" => "已停止", "idle" => "空闲", "saved" => "已保存", _ => "状态未知",
    };
    private async Task InspectAssociatedTask(string host, string parent, JsonElement task)
    {
        var detail = new Window { Owner = this, Title = task.Text("name"), Width = 640, Height = 480,
            Background = Ui.Background, Foreground = Ui.Foreground, WindowStartupLocation = WindowStartupLocation.CenterOwner };
        var body = new DockPanel { Margin = new Thickness(12) };
        var header = new StackPanel();
        header.Children.Add(Ui.Label(AssociatedStatus(task.Text("status")), 12, Ui.Muted));
        string target = task.Text("host_id"), session = task.Text("session_id");
        if (target.Length > 0 && session.Length > 0)
        {
            var open = Ui.Button("打开并接管", async () =>
            {
                await Safe(new { op = "open_session", host_id = target, session_id = session });
                detail.Close();
            });
            open.IsEnabled = task.Flag("connected"); header.Children.Add(open);
        }
        DockPanel.SetDock(header, Dock.Top); body.Children.Add(header);
        var preview = new ListBox { ItemTemplate = chat.ItemTemplate, ItemContainerStyle = chat.ItemContainerStyle };
        body.Children.Add(preview); detail.Content = body;
        detail.Loaded += async (_, _) =>
        {
            try
            {
                var page = await Core.Call(new { op = "inspect_subagent", host_id = host, parent_id = parent, child_id = task.Text("id") });
                preview.ItemsSource = page.Array("items").Select(item => new ChatRow(item, null, "")).ToArray();
                if (page.Flag("has_older")) header.Children.Add(Ui.Label("最近对话", 12, Ui.Muted));
            }
            catch { header.Children.Add(Ui.Label("暂时无法读取关联任务", 12, Ui.Muted)); }
        };
        detail.ShowDialog();
    }
    private void RenderComposer()
    {
        var composer = state.Field("composer"); string signature = composer.Field("models").GetRawText() + composer.Text("model") + composer.Text("effort");
        if (signature != modelSignature) { modelSignature = signature; models.ItemsSource = composer.Array("models").Select(m => new Selection(m.Text("model"), m.Text("name"))).ToList(); models.SelectedValue = composer.Text("model"); UpdateEfforts(); efforts.SelectedItem = composer.Text("effort"); }
        uploadPanel.Children.Clear();
        foreach (var upload in composer.Array("uploads"))
        {
            string id = upload.Field("file").Text("id"), host = HostId, agent = AgentId;
            uploadPanel.Children.Add(Ui.Button((upload.Flag("failed") ? "上传失败 · " : upload.Flag("ready") ? "✓ " : "上传中 · ") + upload.Field("file").Text("name") + "  ×", async () => await Core.Call(new { op = "remove_attachment", host_id = host, agent_id = agent, upload_id = id })));
        }
    }
    private void UpdateEfforts()
    {
        bool before = rendering; rendering = true;
        var model = state.Field("composer").Array("models").FirstOrDefault(m => m.Text("model") == models.SelectedValue as string);
        efforts.ItemsSource = model.Array("efforts").Select(e => e.Text()).ToList(); efforts.SelectedItem = model.Text("default_effort"); rendering = before;
    }
    private async Task SetModel() { if (models.SelectedValue is string model && AgentId.Length > 0) await Safe(new { op = "set_chat_model", host_id = HostId, agent_id = AgentId, model, effort = efforts.SelectedItem as string ?? "" }); }
    private void RenderMessages()
    {
        // Preserve unchanged rows and the reader's scroll position. The ListBox virtualizes controls.
        int prefix = 0;
        while (prefix < messages.Count && prefix < conversation.Items.Count && messages[prefix].Item.GetRawText() == conversation.Items[prefix].GetRawText()) ++prefix;
        int oldCount = messages.Count;
        bool follow = oldCount == 0 || FindScroll(chat) is { } scroll && scroll.VerticalOffset >= scroll.ScrollableHeight - 2;
        while (messages.Count > prefix) messages.RemoveAt(messages.Count - 1);
        foreach (var item in conversation.Items.Skip(prefix)) { string origin = conversation.Context; messages.Add(new ChatRow(item, (id, name) => SaveAttachment(origin, id, name), origin)); }
        if (follow && messages.Count > 0) chat.ScrollIntoView(messages[^1]);
    }
    private static ScrollViewer? FindScroll(DependencyObject parent) { for (int n = 0; n < VisualTreeHelper.GetChildrenCount(parent); n++) { var child = VisualTreeHelper.GetChild(parent, n); if (child is ScrollViewer scroll) return scroll; if (FindScroll(child) is { } found) return found; } return null; }
    private void RenderInteractions()
    {
        var approval = state.Field("approval"); var question = state.Field("question"); var trust = state.Field("pending_host_key");
        string signature = context + approval.GetRawText() + question.GetRawText() + trust.GetRawText(); if (signature == interactionSignature) return; interactionSignature = signature; interactions.Children.Clear();
        string host = HostId, agent = AgentId;
        if (trust.ValueKind == JsonValueKind.Object) { string id = trust.Text("host_id"), fingerprint = trust.Text("fingerprint"); interactions.Children.Add(Ui.Label("确认 SSH 主机指纹 · " + trust.Text("address"), 16)); interactions.Children.Add(new TextBox { Text = fingerprint, IsReadOnly = true }); interactions.Children.Add(Ui.Row(Ui.Button("信任并连接", async () => await Core.Call(new { op = "trust_host", host_id = id, fingerprint })), Ui.Button("取消连接", async () => await Core.Call(new { op = "disconnect_host", host_id = id })))); }
        if (approval.ValueKind == JsonValueKind.Object)
        {
            string request = approval.Text("id"); interactions.Children.Add(Ui.Label(approval.Text("prompt"), 16));
            var details = new TextBox { Text = approval.Text("explanation") + "\n" + approval.Text("command") + "\n" + approval.Text("working_directory") + "\n" + string.Join("\n", approval.Array("details").Select(d => d.Text("name") + ": " + d.Text("value"))), IsReadOnly = true, MaxHeight = 180, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, TextWrapping = TextWrapping.Wrap }; interactions.Children.Add(details);
            var choices = new WrapPanel(); int n = 0; foreach (var choice in approval.Array("choices")) { int selected = n++; choices.Children.Add(Ui.Button(choice.Text("label"), async () => await Core.Call(new { op = "respond_approval", host_id = host, agent_id = agent, request_id = request, choice = selected }))); }
            interactions.Children.Add(choices);
        }
        if (question.ValueKind == JsonValueKind.Object)
        {
            string request = question.Text("id"); var answers = new List<(string Id, Func<string> Value)>();
            foreach (var field in question.Array("fields"))
            {
                interactions.Children.Add(Ui.Label(field.Text("text")));
                if (field.Flag("secret")) { var secret = new PasswordBox(); interactions.Children.Add(secret); answers.Add((field.Text("id"), () => secret.Password)); }
                else if (field.Array("options").Any()) { var choice = new ComboBox { ItemsSource = field.Array("options").Select(o => new Selection(o.Text("label"), o.Text("label") + "  " + o.Text("description"))).ToList(), DisplayMemberPath = "Label", SelectedValuePath = "Id", IsEditable = field.Flag("allow_other"), Margin = new Thickness(0, 0, 0, 8) }; interactions.Children.Add(choice); answers.Add((field.Text("id"), () => choice.SelectedValue as string ?? choice.Text)); }
                else { var text = new TextBox(); interactions.Children.Add(text); answers.Add((field.Text("id"), () => text.Text)); }
            }
            interactions.Children.Add(Ui.Button("提交回答", async () => { var values = answers.Select(a => new { question_id = a.Id, value = a.Value() }).ToArray(); if (values.Any(v => string.IsNullOrWhiteSpace(v.value))) throw new InvalidOperationException("请完成所有问题。"); await Core.Call(new { op = "answer_question", host_id = host, agent_id = agent, request_id = request, answers = values }); }));
        }
    }
    private async Task SendGuarded() { try { await Send(); } catch (Exception error) { status.Text = error.Message; } }
    private async Task Send()
    {
        string host = HostId, agent = AgentId, origin = context, text = draft.Text;
        if (agent.Length == 0 || (!state.Field("composer").Array("uploads").Any() && string.IsNullOrWhiteSpace(text) && !state.Field("composer").Field("skills").Array("selected").Any())) return;
        if (!sending.Add(origin)) return;
        try
        {
            drafts[origin] = text; await Core.Call(new { op = "prompt_agent", host_id = host, agent_id = agent, text });
            if (drafts.GetValueOrDefault(origin) == text) drafts.Remove(origin);
            if (context == origin && draft.Text == text) draft.Clear();
        }
        finally { sending.Remove(origin); }
    }
    private Task ShowKey()
    {
        var panel = new StackPanel { Margin = new Thickness(20) }; panel.Children.Add(Ui.Label("设备 SSH 公钥", 22)); panel.Children.Add(Ui.Label("将公钥加入远程主机的 ~/.ssh/authorized_keys。首次连接时请核对主机指纹。", 13, Ui.Muted));
        string key = state.Field("device_key").Text("public_key_openssh"); panel.Children.Add(new TextBox { Text = key, IsReadOnly = true, TextWrapping = TextWrapping.Wrap, MinHeight = 110 }); panel.Children.Add(Ui.Button("复制公钥", () => { Clipboard.SetText(key); return Task.CompletedTask; }));
        new Window { Title = "Kite · 设备身份", Owner = this, Content = panel, Width = 620, SizeToContent = SizeToContent.Height, WindowStartupLocation = WindowStartupLocation.CenterOwner }.ShowDialog(); return Task.CompletedTask;
    }
    private Task EditHost() { if (ResourceHost is { } host) new HostEditor(this, Core, host).ShowDialog(); return Task.CompletedTask; }
    private async Task RemoveHost() { if (ResourceHost is { } host && MessageBox.Show(this, $"移除 {host.Title} 及本机保存的信任、配对记录？", "移除主机", MessageBoxButton.YesNo) == MessageBoxResult.Yes) await Core.Call(new { op = "remove_host", host_id = host.id }); }
    private Task SessionMenu() { if (sessions.SelectedItem is ListBoxItem item && item.ContextMenu is { } menu) { menu.PlacementTarget = item; menu.IsOpen = true; } return Task.CompletedTask; }
    private static string ProviderLabel(string id) => id == "antigravity" ? "Antigravity" : id;
    private async Task CreateSession()
    {
        string recommended = (await Core.Call(new { op = "recommend_session_host" })).Text();
        var connected = state.Array("host_connections").Where(h => h.Text("connection") == "Connected")
            .ToDictionary(h => h.Text("host_id"));
        var availableHosts = state.Array("hosts").Select(Host.From).Where(h => connected.ContainsKey(h.id))
            .Select(h => new Selection(h.id, h.Title)).ToArray();
        if (availableHosts.Length == 0) throw new InvalidOperationException("请先连接执行主机。");
        var panel = new StackPanel { Margin = new Thickness(24) };
        var machine = new ComboBox { ItemsSource = availableHosts, DisplayMemberPath = "Label", SelectedValuePath = "Id" };
        var providers = new ComboBox { DisplayMemberPath = "Label", SelectedValuePath = "Id" };
        panel.Children.Add(Ui.Label("执行主机", 12, Ui.Muted)); panel.Children.Add(machine);
        panel.Children.Add(Ui.Label("Harness", 12, Ui.Muted)); panel.Children.Add(providers);
        var name = Ui.Field(panel, "会话名称");
        var cwd = Ui.Field(panel, "远程工作目录（绝对路径）");
        machine.SelectionChanged += (_, _) =>
        {
            cwd.Clear();
            providers.ItemsSource = machine.SelectedValue is string host
                ? connected[host].Array("providers").Select(p => p.Text("id")).Select(id => new Selection(id, ProviderLabel(id))).ToArray()
                : Array.Empty<Selection>();
            providers.SelectedIndex = providers.HasItems ? 0 : -1;
        };
        machine.SelectedValue = recommended;
        var window = new Window { Title = "新建会话", Owner = this, Width = 460, SizeToContent = SizeToContent.Height, Content = panel, WindowStartupLocation = WindowStartupLocation.CenterOwner };
        panel.Children.Add(Ui.Button("创建", async () =>
        {
            if (machine.SelectedValue is not string host || providers.SelectedValue is not string provider || !cwd.Text.StartsWith('/'))
                throw new InvalidOperationException("请选择执行主机和 Harness，并填写远程绝对路径。");
            await Core.Call(new { op = "create_session", host_id = host, provider, cwd = cwd.Text, title = name.Text });
            window.Close();
        }));
        window.ShowDialog();
    }
    private async Task Upload()
    {
        string host = HostId, agent = AgentId; var dialog = new OpenFileDialog { Multiselect = true, Title = "添加附件" }; if (dialog.ShowDialog(this) != true) return;
        foreach (string path in dialog.FileNames) { string mime = Path.GetExtension(path).ToLowerInvariant() switch { ".png" => "image/png", ".jpg" or ".jpeg" => "image/jpeg", ".webp" => "image/webp", ".wav" => "audio/wav", ".mp3" => "audio/mpeg", ".txt" or ".md" => "text/plain", _ => "application/octet-stream" }; await Core.Call(new { op = "upload_attachment", host_id = host, agent_id = agent, local_path = path, name = Path.GetFileName(path), mime, retry_id = (string?)null }); }
    }
    private async Task SaveAttachment(string origin, string id, string name)
    {
        var dialog = new SaveFileDialog { FileName = Path.GetFileName(name), Title = "保存附件" }; if (dialog.ShowDialog(this) != true) return;
        string path = dialog.FileName; var value = await Core.Call(new { op = "attachment", context = origin, id }); await File.WriteAllBytesAsync(path, Convert.FromBase64String(value.Text()));
    }
    private Task ComposerSettings()
    {
        string host = HostId, agent = AgentId; var panel = new StackPanel { Margin = new Thickness(24) }; var window = new Window { Title = "模型与会话设置", Owner = this, Width = 500, SizeToContent = SizeToContent.Height, Content = panel, WindowStartupLocation = WindowStartupLocation.CenterOwner };
        foreach (var setting in state.Field("composer").Array("settings"))
        {
            string id = setting.Text("id"); panel.Children.Add(Ui.Label(id == "service_tier" ? "速度" : setting.Text("name")));
            var select = new ComboBox { ItemsSource = setting.Array("options").Select(o => new Selection(o.Text("value"), o.Text("name"))).ToList(), DisplayMemberPath = "Label", SelectedValuePath = "Id", SelectedValue = setting.Text("current_value"), Margin = new Thickness(0, 0, 0, 12) };
            select.SelectionChanged += async (_, _) => { if (select.SelectedValue is string value) await Safe(new { op = "set_chat_setting", host_id = host, agent_id = agent, setting = id, value }); }; panel.Children.Add(select);
        }
        var usage = state.Field("composer").Field("usage"); panel.Children.Add(Ui.Label("套餐额度 · " + usage.Text("status"), 16));
        foreach (var bucket in usage.Array("buckets"))
        {
            string title = bucket.Text("name"); if (string.IsNullOrEmpty(title)) title = bucket.Text("id");
            panel.Children.Add(Ui.Label(title, 14));
            foreach (var key in new[] { "primary", "secondary" })
            {
                var windowInfo = bucket.Field(key);
                if (windowInfo.ValueKind != JsonValueKind.Object) continue;
                string duration = FormatWindowDuration(windowInfo.Field("duration_mins"));
                string percent = windowInfo.Field("remaining_percent").ValueKind == JsonValueKind.Number
                    ? $"剩余 {windowInfo.Field("remaining_percent").GetInt32()}% · 已用 {Math.Clamp(100 - windowInfo.Field("remaining_percent").GetInt32(), 0, 100)}%"
                    : "未知";
                string reset = windowInfo.Field("resets_at").ValueKind == JsonValueKind.Number && windowInfo.Field("resets_at").GetInt64() > 0
                    ? " · " + DateTimeOffset.FromUnixTimeSeconds(windowInfo.Field("resets_at").GetInt64()).ToLocalTime().ToString("g") + " 重置"
                    : "";
                panel.Children.Add(Ui.Label($"{duration} · {percent}{reset}", 12, Ui.Muted));
            }
        }
        panel.Children.Add(Ui.Button("刷新额度", async () => { await Core.Call(new { op = "refresh_account_usage" }); await Refresh(true); })); window.ShowDialog(); return Task.CompletedTask;
    }
    private static string FormatWindowDuration(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Number) return "额度窗口";
        long mins = value.GetInt64();
        if (mins > 0 && mins % 1440 == 0) return $"{mins / 1440}天";
        if (mins > 0 && mins % 60 == 0) return $"{mins / 60}小时";
        return $"{mins}分钟";
    }
    private async Task Skills()
    {
        string host = HostId, agent = AgentId; await Core.Call(new { op = "refresh_skills", agent_id = agent }); await Refresh(true);
        var panel = new StackPanel { Margin = new Thickness(20) }; var selected = state.Field("composer").Field("skills").Array("selected").Select(s => s.Text("path")).ToHashSet();
        foreach (var skill in state.Field("composer").Field("skills").Array("available"))
        {
            string path = skill.Text("path"); var check = new CheckBox { Content = skill.Text("name") + "\n" + skill.Text("description"), Foreground = Ui.Foreground, IsChecked = selected.Contains(path), Margin = new Thickness(0, 0, 0, 12) };
            check.Checked += async (_, _) => await Safe(new { op = "select_skill", host_id = host, agent_id = agent, path, selected = true }); check.Unchecked += async (_, _) => await Safe(new { op = "select_skill", host_id = host, agent_id = agent, path, selected = false }); panel.Children.Add(check);
        }
        new Window { Title = "选择 Skills", Owner = this, Width = 600, Height = 560, Content = new ScrollViewer { Content = panel }, WindowStartupLocation = WindowStartupLocation.CenterOwner }.ShowDialog();
    }
    private async Task HostResources()
    {
        string host = HostId; await Core.Call(new { op = "refresh_host_resources", host_id = host }); await Refresh(true); var resources = state.Field("host_resources");
        var panel = new StackPanel { Margin = new Thickness(24) }; panel.Children.Add(Ui.Label("远程主机详情", 22));
        if (!resources.Flag("available")) panel.Children.Add(Ui.Label(resources.Text("problem").Length > 0 ? resources.Text("problem") : "此主机尚未提供资源信息。"));
        else
        {
            const double gb = 1073741824d; panel.Children.Add(Ui.Label($"CPU：{resources.Field("cpu_percent")} % · {resources.Number("cpu_cores")} 核\n可用内存：{resources.Number("memory_available_bytes") / gb:F1} / {resources.Number("memory_total_bytes") / gb:F1} GiB\nAgent 用量：{resources.Number("agent_memory_used_bytes") / gb:F1} GiB\n运行进程：{resources.Number("agent_process_count")}"));
            var limit = Ui.Field(panel, "Agent 共享内存预算（GiB）", (resources.Number("agent_memory_limit_bytes") / gb).ToString("F1", System.Globalization.CultureInfo.InvariantCulture));
            panel.Children.Add(Ui.Button("保存远程预算", async () => { if (!double.TryParse(limit.Text, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out double n) || !double.IsFinite(n) || n <= 0 || n > 1048576) throw new InvalidOperationException("请输入有效预算。"); await Core.Call(new { op = "set_agent_memory_limit", host_id = host, bytes = checked((ulong)(n * gb)) }); }));
        }
        panel.Children.Add(Ui.Label("SSH 指纹：" + state.Text("host_key_fingerprint"), 12, Ui.Muted));
        new Window { Title = "主机详情", Owner = this, Width = 560, SizeToContent = SizeToContent.Height, Content = panel, WindowStartupLocation = WindowStartupLocation.CenterOwner }.ShowDialog();
    }
    private async Task OpenDesktop()
    {
        if (media is null) { media = new MediaReceiver(screen); input = new RemoteInput(inputSurface, media.Target); }
        desktopPanel.Visibility = Visibility.Visible;
        var desktop = state.Field("desktop").Text("status"); if (desktop is "Idle" or "Failed") await Core.Call(new { op = "start_desktop", host_id = HostId, media_target = media.Target });
        await Refresh(true);
        if (state.Field("desktop").Text("status") == "Pairing")
        {
            string host = HostId, pin = state.Field("desktop").Text("pin");
            if (MessageBox.Show(this, "配对 PIN：" + pin + "\n允许 Kite 与此主机配对？", "桌面配对", MessageBoxButton.YesNo) == MessageBoxResult.Yes) await Core.Call(new { op = "approve_pairing", host_id = host, pin });
        }
    }
    private async Task CloseDesktop() { LoseInput(); if (fullscreen is not null) fullscreen.Close(); string host = HostId; await Core.Call(new { op = "stop_desktop", host_id = host }); desktopPanel.Visibility = Visibility.Collapsed; }
    private void LoseInput()
    {
        bool wasEnabled = input?.Enabled == true; input?.Release();
        if (wasEnabled && client is not null) { string host = HostId; _ = Safe(new { op = "set_takeover", host_id = host, enabled = false }); }
    }
    private void ToggleFullscreen()
    {
        if (fullscreen is not null) { fullscreen.Close(); return; }
        LoseInput(); desktopPanel.Child = null;
        fullscreen = new Window { Title = "Kite · 远程桌面", Content = desktopRoot, WindowState = WindowState.Maximized, WindowStyle = WindowStyle.None, Background = Brushes.Black };
        fullscreen.Deactivated += (_, _) => LoseInput(); fullscreen.Closed += (_, _) => { LoseInput(); if (fullscreen is { } window) window.Content = null; fullscreen = null; desktopPanel.Child = desktopRoot; };
        fullscreen.PreviewKeyDown += (_, e) => { if (e.Key == Key.Escape && Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) { e.Handled = true; fullscreen?.Close(); } };
        fullscreen.Show();
    }
    private async Task SynchronizeClipboard()
    {
        if (clipboardBusy || clipboard.IsChecked != true || input?.Enabled != true || client is null || closing || (!IsActive && fullscreen?.IsActive != true)) return;
        clipboardBusy = true; string host = HostId; string? local = null;
        try
        {
            if (Clipboard.ContainsText()) { local = Clipboard.GetText(); if (local.Length > 262144) return; }
            var remote = await Core.Call(new { op = "synchronize_clipboard", host_id = host, local });
            if (host == HostId && input.Enabled && clipboard.IsChecked == true && remote.ValueKind == JsonValueKind.String && (!Clipboard.ContainsText() ? local is null : Clipboard.GetText() == local)) Clipboard.SetText(remote.Text());
        }
        catch (Exception error) { status.Text = "剪贴板：" + error.Message; }
        finally { clipboardBusy = false; }
    }
    private void NetworkChanged(object? sender, NetworkAvailabilityEventArgs e) { Dispatcher.BeginInvoke(async () => { if (client is not null && !closing) await Safe(new { op = "network_changed", available = e.IsAvailable }); }); }
    private void OnClosing(object? sender, System.ComponentModel.CancelEventArgs e)
    {
        closing = true; timer.Stop(); clipboardTimer.Stop(); NetworkChange.NetworkAvailabilityChanged -= NetworkChanged;
        input?.Dispose(); fullscreen?.Close(); media?.Dispose(); client?.Dispose();
    }
}
