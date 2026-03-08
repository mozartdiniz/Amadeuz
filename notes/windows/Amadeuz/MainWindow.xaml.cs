using System;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Amadeuz;

public sealed partial class MainWindow : Window
{
    private readonly NoteViewModel _vm;
    private bool _suppressTextChanged;

    public MainWindow()
    {
        InitializeComponent();

        // ── Fluent Design: Mica material ────────────────────────────────────
        // Mica adapts to the user's system accent colour and wallpaper,
        // giving the app the same translucent depth as native Windows 11 apps.
        SystemBackdrop = new MicaBackdrop();

        // ── View model ──────────────────────────────────────────────────────
        _vm = new NoteViewModel(
            onContentChanged: content =>
                DispatcherQueue.TryEnqueue(() => UpdateText(content)),
            onConnectionChanged: connected =>
                DispatcherQueue.TryEnqueue(() => UpdateStatus(connected)));

        // Populate TextBox with the locally-stored note without firing TextChanged.
        _suppressTextChanged = true;
        NoteTextBox.Text = _vm.Content;
        _suppressTextChanged = false;
    }

    // ── Event handlers ───────────────────────────────────────────────────────

    private void NoteTextBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_suppressTextChanged) return;
        _vm.OnTextChanged(NoteTextBox.Text);
    }

    private async void SettingsButton_Click(object sender, RoutedEventArgs e)
    {
        var urlBox = new TextBox
        {
            PlaceholderText = "ws://hostname:8080/ws",
            Text            = _vm.ServerAddress,
            Width           = 340,
            Margin          = new Thickness(0, 8, 0, 0),
            CornerRadius    = new CornerRadius(6),
        };

        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(new TextBlock
        {
            Text       = "WebSocket URL",
            FontWeight = new Windows.UI.Text.FontWeight(600),
        });
        panel.Children.Add(urlBox);

        var dialog = new ContentDialog
        {
            XamlRoot          = Content.XamlRoot,
            Title             = "Server Settings",
            Content           = panel,
            PrimaryButtonText = "Connect",
            CloseButtonText   = "Cancel",
            DefaultButton     = ContentDialogButton.Primary,
        };

        var result = await dialog.ShowAsync().AsTask();
        if (result == ContentDialogResult.Primary && !string.IsNullOrWhiteSpace(urlBox.Text))
            _vm.ServerAddress = urlBox.Text.Trim();
    }

    // ── UI update helpers (always called on the UI thread) ───────────────────

    private void UpdateText(string text)
    {
        _suppressTextChanged = true;
        NoteTextBox.Text = text;
        _suppressTextChanged = false;
        // Keep cursor at end so the view doesn't jump.
        NoteTextBox.Select(text.Length, 0);
    }

    private void UpdateStatus(bool connected)
    {
        // Smooth colour transition is handled by BrushTransition in XAML.
        StatusDot.Fill = new SolidColorBrush(connected
            ? Color.FromArgb(255, 34,  197, 94)   // #22C55E — green
            : Color.FromArgb(255, 239, 68,  68));  // #EF4444 — red
        StatusText.Text = connected ? "Synced" : "Offline";
    }
}
