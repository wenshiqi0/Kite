using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;

[assembly: DefaultDllImportSearchPaths(DllImportSearchPath.AssemblyDirectory | DllImportSearchPath.System32)]

namespace Kite;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        if (args.Contains("--self-test")) { Environment.Exit(SelfTest.Run()); return; }
        if (args.Length == 2 && args[0] == "--ui-smoke") { Environment.Exit(SelfTest.RunUi(args[1])); return; }
        using var singleton = new Mutex(true, @"Local\Kite.Windows.Client", out bool owns);
        if (!owns) { MessageBox.Show("Kite 已在运行。请从任务栏打开现有窗口。", "Kite"); return; }
        var app = new Application { ShutdownMode = ShutdownMode.OnMainWindowClose };
        Ui.Configure(app);
        app.DispatcherUnhandledException += (_, e) => { MessageBox.Show(e.Exception.Message, "Kite", MessageBoxButton.OK, MessageBoxImage.Error); e.Handled = true; };
        app.Run(new MainWindow());
    }
}

internal static class Ui
{
    public static readonly Brush Background = Brush("#101722"), Panel = Brush("#171F2C"), Muted = Brush("#94A3B8"), Foreground = Brush("#E5EDF6"), Accent = Brush("#67C7A5");
    public static SolidColorBrush Brush(string hex) => (SolidColorBrush)new BrushConverter().ConvertFromString(hex)!;
    public static void Configure(Application app)
    {
        app.Resources[SystemColors.WindowBrushKey] = Background;
        app.Resources[SystemColors.WindowTextBrushKey] = Foreground;
        app.Resources[typeof(Window)] = new Style(typeof(Window)) { Setters = { new Setter(Control.BackgroundProperty, Background), new Setter(Control.ForegroundProperty, Foreground), new Setter(Control.FontFamilyProperty, new FontFamily("Segoe UI")), new Setter(Control.FontSizeProperty, 14d) } };
        app.Resources[typeof(TextBox)] = new Style(typeof(TextBox)) { Setters = { new Setter(Control.BackgroundProperty, Panel), new Setter(Control.ForegroundProperty, Foreground), new Setter(Control.BorderBrushProperty, Brush("#334155")), new Setter(Control.PaddingProperty, new Thickness(8)), new Setter(TextBoxBase.CaretBrushProperty, Foreground) } };
        app.Resources[typeof(ListBox)] = new Style(typeof(ListBox)) { Setters = { new Setter(Control.BackgroundProperty, Brushes.Transparent), new Setter(Control.ForegroundProperty, Foreground), new Setter(Control.BorderThicknessProperty, new Thickness(0)), new Setter(ScrollViewer.HorizontalScrollBarVisibilityProperty, ScrollBarVisibility.Disabled) } };
        app.Resources[typeof(ComboBox)] = new Style(typeof(ComboBox)) { Setters = { new Setter(Control.ForegroundProperty, Brushes.Black), new Setter(Control.BackgroundProperty, Brushes.White) } };
        app.Resources[typeof(ComboBoxItem)] = new Style(typeof(ComboBoxItem)) { Setters = { new Setter(Control.ForegroundProperty, Brushes.Black), new Setter(Control.BackgroundProperty, Brushes.White) } };
        var border = new FrameworkElementFactory(typeof(Border));
        border.SetBinding(Border.BackgroundProperty, new System.Windows.Data.Binding("Background") { RelativeSource = System.Windows.Data.RelativeSource.TemplatedParent });
        border.SetValue(Border.CornerRadiusProperty, new CornerRadius(5));
        var presenter = new FrameworkElementFactory(typeof(ContentPresenter));
        presenter.SetValue(FrameworkElement.MarginProperty, new Thickness(10, 7, 10, 7));
        presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
        border.AppendChild(presenter);
        var template = new ControlTemplate(typeof(Button)) { VisualTree = border };
        var disabled = new Trigger { Property = UIElement.IsEnabledProperty, Value = false };
        disabled.Setters.Add(new Setter(UIElement.OpacityProperty, .4)); template.Triggers.Add(disabled);
        var hover = new Trigger { Property = UIElement.IsMouseOverProperty, Value = true };
        hover.Setters.Add(new Setter(UIElement.OpacityProperty, .8)); template.Triggers.Add(hover);
        app.Resources[typeof(Button)] = new Style(typeof(Button)) { Setters = { new Setter(Control.TemplateProperty, template), new Setter(Control.BackgroundProperty, Brush("#293649")), new Setter(Control.ForegroundProperty, Foreground), new Setter(FrameworkElement.MarginProperty, new Thickness(0, 0, 6, 0)), new Setter(Control.CursorProperty, System.Windows.Input.Cursors.Hand) } };
    }
    public static TextBlock Label(string text, double size = 14, Brush? color = null) => new() { Text = text, FontSize = size, Foreground = color ?? Foreground, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, 4, 0, 6) };
    public static Button Button(string text, Func<Task> action)
    {
        var button = new Button { Content = text };
        bool running = false;
        button.Click += async (_, _) =>
        {
            if (running) return;
            running = true; button.IsEnabled = false;
            try { await action(); }
            catch (Exception error) { MessageBox.Show(error.Message, "Kite", MessageBoxButton.OK, MessageBoxImage.Error); }
            finally { running = false; button.IsEnabled = true; }
        };
        return button;
    }
    public static StackPanel Row(params UIElement[] children) { var row = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 4, 0, 6) }; foreach (var child in children) row.Children.Add(child); return row; }
    public static TextBox Field(Panel panel, string title, string value = "") { panel.Children.Add(Label(title, 12, Muted)); var box = new TextBox { Text = value, MinWidth = 120, Margin = new Thickness(0, 0, 0, 8) }; panel.Children.Add(box); return box; }
}
