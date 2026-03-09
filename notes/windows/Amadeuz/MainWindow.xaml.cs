using System;
using System.Linq;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Amadeuz;

public sealed partial class MainWindow : Window
{
    public NotesViewModel Vm { get; }

    // Tracks which note was last communicated to the VM so we can pass the old ID
    // on selection changes, and avoid calling NoteSelectionChanged when the VM itself
    // triggers a ListView update (e.g. auto-select after server creates a note).
    private string? _currentNoteId;
    private bool    _vmChangingNoteSelection;
    private bool    _sidebarVisible = true;

    // Suppress TextChanged callbacks while updating the editor from VM data.
    private bool _suppressEditorChanged;

    // ── Constructor ───────────────────────────────────────────────────────────

    public MainWindow()
    {
        InitializeComponent();

        // Mica: adapts to wallpaper + accent colour — feels like a native Win11 app.
        SystemBackdrop = new MicaBackdrop();

        Vm = new NotesViewModel(
            dispatcher:          DispatcherQueue,
            onEditorChanged:     UpdateEditor,
            onConnectionChanged: UpdateStatus,
            onNoteAutoSelected:  AutoSelectNote);

        // Start with "All Notes" selected (first item in the list).
        FolderListView.SelectedIndex = 0;

        // Set a sensible default window size: 75 % of the work area, capped at 1200×750.
        var displayArea = Microsoft.UI.Windowing.DisplayArea.GetFromWindowId(
            AppWindow.Id, Microsoft.UI.Windowing.DisplayAreaFallback.Primary);
        int w = Math.Min(1200, displayArea.WorkArea.Width  * 3 / 4);
        int h = Math.Min(750,  displayArea.WorkArea.Height * 3 / 4);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(w, h));
    }

    // ── Sidebar toggle ────────────────────────────────────────────────────────

    private void SidebarToggleButton_Click(object sender, RoutedEventArgs e)
    {
        _sidebarVisible = !_sidebarVisible;
        SidebarColumn.MinWidth       = _sidebarVisible ? 160 : 0;
        SidebarColumn.Width          = _sidebarVisible ? new GridLength(220) : new GridLength(0);
        SidebarSeparatorColumn.Width = _sidebarVisible ? new GridLength(1)   : new GridLength(0);
    }

    // ── Folder sidebar ────────────────────────────────────────────────────────

    private void FolderListView_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        var item = FolderListView.SelectedItem as FolderItem;
        NoteListTitle.Text      = item?.Name ?? "Notes";
        NewNoteButton.IsEnabled = item is not null && !item.IsAllNotes;
        DeleteNoteButton.IsEnabled = false;

        // Flush and clear note selection BEFORE calling SelectFolder.
        // SelectFolder calls DiffFilteredNotes which can fire SelectionChanged
        // on the NoteListView; clearing first prevents that from cascading into
        // NoteSelectionChanged with a stale oldId.
        _vmChangingNoteSelection = true;
        if (_currentNoteId is not null)
            Vm.NoteSelectionChanged(_currentNoteId, null, TitleTextBox.Text, ContentTextBox.Text);
        NoteListView.SelectedItem = null;
        _currentNoteId = null;
        _vmChangingNoteSelection = false;

        Vm.SelectFolder(item?.Id);
        ClearEditor();
    }

    private void FolderItem_RightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not FolderItem item) return;
        if (item.IsAllNotes) return; // no context menu for the "All Notes" sentinel

        var flyout = new MenuFlyout();

        var rename = new MenuFlyoutItem { Text = "Rename…" };
        rename.Click += async (_, _) =>
        {
            var box = new TextBox
            {
                Text         = item.Name,
                Width        = 280,
                Margin       = new Thickness(0, 8, 0, 0),
                CornerRadius = new CornerRadius(6),
            };
            var panel = new StackPanel { Spacing = 4 };
            panel.Children.Add(new TextBlock
            {
                Text       = "Folder name",
                FontWeight = new Windows.UI.Text.FontWeight(600),
            });
            panel.Children.Add(box);

            var dialog = new ContentDialog
            {
                XamlRoot          = Content.XamlRoot,
                Title             = "Rename Folder",
                Content           = panel,
                PrimaryButtonText = "Rename",
                CloseButtonText   = "Cancel",
                DefaultButton     = ContentDialogButton.Primary,
            };
            if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            {
                var name = box.Text.Trim();
                if (!string.IsNullOrEmpty(name))
                    Vm.RenameFolder(item.Id, name);
            }
        };

        var delete = new MenuFlyoutItem { Text = "Delete Folder" };
        delete.Click += (_, _) => Vm.DeleteFolder(item.Id);

        flyout.Items.Add(rename);
        flyout.Items.Add(new MenuFlyoutSeparator());
        flyout.Items.Add(delete);
        flyout.ShowAt((FrameworkElement)sender, e.GetPosition((FrameworkElement)sender));
    }

    private async void NewFolderButton_Click(object sender, RoutedEventArgs e)
    {
        var box = new TextBox
        {
            PlaceholderText = "Folder name",
            Width           = 280,
            Margin          = new Thickness(0, 8, 0, 0),
            CornerRadius    = new CornerRadius(6),
        };
        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(new TextBlock
        {
            Text       = "Name",
            FontWeight = new Windows.UI.Text.FontWeight(600),
        });
        panel.Children.Add(box);

        var dialog = new ContentDialog
        {
            XamlRoot          = Content.XamlRoot,
            Title             = "New Folder",
            Content           = panel,
            PrimaryButtonText = "Create",
            CloseButtonText   = "Cancel",
            DefaultButton     = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            var name = box.Text.Trim();
            if (!string.IsNullOrEmpty(name))
                Vm.CreateFolder(name);
        }
    }

    // ── Note list ─────────────────────────────────────────────────────────────

    private void NoteListView_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_vmChangingNoteSelection) return;

        var newId = (NoteListView.SelectedItem as Note)?.Id;
        var oldId = _currentNoteId;
        _currentNoteId = newId;

        Vm.NoteSelectionChanged(oldId, newId, TitleTextBox.Text, ContentTextBox.Text);

        DeleteNoteButton.IsEnabled = newId is not null;
        SetEditorEnabled(newId is not null);
    }

    private void NoteItem_RightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not Note note) return;

        var flyout = new MenuFlyout();
        var delete = new MenuFlyoutItem { Text = "Delete Note" };
        delete.Click += (_, _) => Vm.DeleteNote(note.Id);
        flyout.Items.Add(delete);
        flyout.ShowAt((FrameworkElement)sender, e.GetPosition((FrameworkElement)sender));
    }

    private void NewNoteButton_Click(object sender, RoutedEventArgs e) => Vm.CreateNote();

    private void DeleteNoteButton_Click(object sender, RoutedEventArgs e)
    {
        if (_currentNoteId is not null)
            Vm.DeleteNote(_currentNoteId);
    }

    // ── Editor ────────────────────────────────────────────────────────────────

    private void TitleTextBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_suppressEditorChanged) return;
        Vm.OnEditorChanged(TitleTextBox.Text, ContentTextBox.Text);
    }

    private void ContentTextBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_suppressEditorChanged) return;
        Vm.OnEditorChanged(TitleTextBox.Text, ContentTextBox.Text);
    }

    // Called by VM when the server updates the current note's content.
    private void UpdateEditor(string title, string content)
    {
        _suppressEditorChanged = true;
        TitleTextBox.Text   = title;
        ContentTextBox.Text = content;
        _suppressEditorChanged = false;
    }

    private void ClearEditor()
    {
        _suppressEditorChanged = true;
        TitleTextBox.Text   = "";
        ContentTextBox.Text = "";
        _suppressEditorChanged = false;
        SetEditorEnabled(false);
    }

    private void SetEditorEnabled(bool enabled)
    {
        TitleTextBox.IsEnabled   = enabled;
        ContentTextBox.IsEnabled = enabled;
    }

    // Called by VM when the server created a note and it should be auto-selected.
    private void AutoSelectNote(string noteId, string title, string content)
    {
        _vmChangingNoteSelection = true;
        var note = Vm.FilteredNotes.FirstOrDefault(n => n.Id == noteId);
        if (note is not null)
        {
            NoteListView.SelectedItem = note;
            _currentNoteId = noteId;
        }
        _vmChangingNoteSelection = false;

        UpdateEditor(title, content);
        DeleteNoteButton.IsEnabled = true;
        SetEditorEnabled(true);

        // Focus the title so the user can type the note name immediately.
        TitleTextBox.Focus(FocusState.Programmatic);
    }

    // ── Status bar ────────────────────────────────────────────────────────────

    private void UpdateStatus(bool connected)
    {
        StatusDot.Fill  = new SolidColorBrush(connected
            ? Color.FromArgb(255, 34,  197, 94)   // #22C55E — green
            : Color.FromArgb(255, 239, 68,  68));  // #EF4444 — red
        StatusText.Text = connected ? "Synced" : "Offline";
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    private async void SettingsButton_Click(object sender, RoutedEventArgs e)
    {
        var box = new TextBox
        {
            PlaceholderText = "ws://hostname:8080/ws",
            Text            = Vm.ServerAddress,
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
        panel.Children.Add(box);

        var dialog = new ContentDialog
        {
            XamlRoot          = Content.XamlRoot,
            Title             = "Server Settings",
            Content           = panel,
            PrimaryButtonText = "Connect",
            CloseButtonText   = "Cancel",
            DefaultButton     = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            var url = box.Text.Trim();
            if (!string.IsNullOrEmpty(url))
                Vm.ServerAddress = url;
        }
    }
}
