using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;

namespace Amadeuz;

/// <summary>
/// All business logic: offline-first load, folder/note CRUD, 500 ms debounce,
/// timestamp-based merge. Mirror of the macOS NotesViewModel.
///
/// Key design: FilteredNotes is NEVER cleared wholesale. Instead it is updated
/// via a diff algorithm using ObservableCollection.Move() so the ListView never
/// loses its selection — which would otherwise cascade into clearing the editor.
/// </summary>
public sealed class NotesViewModel : IDisposable
{
    public const string AllNotesId = "__all__";

    // ── Observable collections (bound to ListViews) ───────────────────────────

    /// Flat list for the folder sidebar: AllNotes sentinel + actual folders.
    public ObservableCollection<FolderItem> FolderItems   { get; } = new();

    /// Notes visible in the current folder view, sorted by updatedAt desc.
    /// Updated via diff (never cleared wholesale) to preserve ListView selection.
    public ObservableCollection<Note> FilteredNotes        { get; } = new();

    // ── Simple properties ─────────────────────────────────────────────────────

    private string _serverAddress;
    public string ServerAddress
    {
        get => _serverAddress;
        set { _serverAddress = value; SaveSettings(value); Reconnect(); }
    }

    // ── Private state ─────────────────────────────────────────────────────────

    private readonly List<Folder> _folders  = new();
    private readonly List<Note>   _notes    = new();

    private string? _selectedFolderId = AllNotesId;
    private string? _editingNoteId;

    private readonly LocalStore     _localStore = new();
    private SyncService?            _syncService;
    private CancellationTokenSource _debounceCts = new();

    private readonly DispatcherQueue _dispatcher;

    // Callbacks → code-behind (always invoked on the UI thread via DispatcherQueue)
    private readonly Action<string, string>        _onEditorChanged;     // update editor TextBoxes
    private readonly Action<bool>                  _onConnectionChanged;
    private readonly Action<string, string, string> _onNoteAutoSelected; // noteId, title, content

    private static readonly string SettingsFile = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "amadeuz", "settings.json");
    private const string DefaultServer = "ws://localhost:8080/ws";

    // ── Constructor ───────────────────────────────────────────────────────────

    public NotesViewModel(
        DispatcherQueue dispatcher,
        Action<string, string> onEditorChanged,
        Action<bool> onConnectionChanged,
        Action<string, string, string> onNoteAutoSelected)
    {
        _dispatcher          = dispatcher;
        _onEditorChanged     = onEditorChanged;
        _onConnectionChanged = onConnectionChanged;
        _onNoteAutoSelected  = onNoteAutoSelected;

        _serverAddress = LoadSettings();

        var saved = _localStore.Load();
        _folders.AddRange(saved.folders);
        _notes.AddRange(saved.notes);

        RebuildFolderItems();
        DiffFilteredNotes();

        StartSync();
    }

    // ── Folder selection ──────────────────────────────────────────────────────

    public void SelectFolder(string? id)
    {
        _selectedFolderId = id;
        DiffFilteredNotes();
    }

    // ── Note selection (called from code-behind on ListView SelectionChanged) ─

    /// <param name="editorTitle">Current title TextBox value — flushed to the old note immediately.</param>
    /// <param name="editorContent">Current content TextBox value — flushed to the old note immediately.</param>
    public void NoteSelectionChanged(string? oldId, string? newId, string editorTitle, string editorContent)
    {
        if (oldId is not null)
        {
            // Cancel any pending debounce and flush the old note synchronously
            // with whatever is currently in the editor. This ensures the note list
            // row updates as soon as the user leaves the note (< 500 ms).
            _debounceCts.Cancel();
            _debounceCts = new CancellationTokenSource();
            FlushNote(oldId, editorTitle, editorContent);
        }

        _editingNoteId = newId;
        var note = newId is null ? null : _notes.FirstOrDefault(n => n.Id == newId);
        _onEditorChanged(note?.Title ?? "", note?.Content ?? "");
    }

    // ── Editor changes (called from code-behind on TextChanged) ───────────────

    public void OnEditorChanged(string title, string content)
    {
        _debounceCts.Cancel();
        _debounceCts = new CancellationTokenSource();
        var ct     = _debounceCts.Token;
        var noteId = _editingNoteId;

        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(500, ct);
                if (noteId is not null)
                    _dispatcher.TryEnqueue(() => FlushNote(noteId, title, content));
            }
            catch (OperationCanceledException) { }
        }, ct);
    }

    // ── Folder actions ────────────────────────────────────────────────────────

    public void CreateFolder(string name) =>
        _ = _syncService?.SendAsync(new WsMessage { Type = "create_folder", Name = name });

    public void RenameFolder(string id, string name)
    {
        UpdateFolderName(id, name);
        _localStore.Save(_folders, _notes);
        _ = _syncService?.SendAsync(new WsMessage { Type = "rename_folder", FolderId = id, Name = name });
    }

    public void DeleteFolder(string id)
    {
        _folders.RemoveAll(f => f.Id == id);
        _notes.RemoveAll(n => n.FolderId == id);
        var item = FolderItems.FirstOrDefault(f => f.Id == id);
        if (item is not null) FolderItems.Remove(item);
        if (_selectedFolderId == id) _selectedFolderId = AllNotesId;
        DiffFilteredNotes();
        _localStore.Save(_folders, _notes);
        _ = _syncService?.SendAsync(new WsMessage { Type = "delete_folder", FolderId = id });
    }

    // ── Note actions ──────────────────────────────────────────────────────────

    public void CreateNote()
    {
        if (_selectedFolderId is null || _selectedFolderId == AllNotesId) return;
        _ = _syncService?.SendAsync(new WsMessage
        {
            Type     = "create_note",
            FolderId = _selectedFolderId,
            Title    = ""
        });
    }

    public void DeleteNote(string id)
    {
        _notes.RemoveAll(n => n.Id == id);
        DiffFilteredNotes();
        if (_editingNoteId == id)
        {
            _editingNoteId = null;
            _onEditorChanged("", "");
        }
        _localStore.Save(_folders, _notes);
        _ = _syncService?.SendAsync(new WsMessage { Type = "delete_note", NoteId = id });
    }

    // ── Connection ────────────────────────────────────────────────────────────

    public void Reconnect()
    {
        _syncService?.Dispose();
        _syncService = null;
        StartSync();
    }

    // ── Private: flush ────────────────────────────────────────────────────────

    /// Persist and send only if content actually changed. No collection rebuild.
    private void FlushNote(string id, string title, string content)
    {
        var note = _notes.FirstOrDefault(n => n.Id == id);
        if (note is null) return;
        if (note.Title == title && note.Content == content) return;

        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

        // Update in-place: INotifyPropertyChanged causes the ListView row to
        // refresh without removing/inserting the item (selection preserved).
        note.Title     = title;
        note.Content   = content;
        note.UpdatedAt = now;

        // Move to position 0 (updated notes sort first) using Move() —
        // ObservableCollection.Move fires NotifyCollectionChangedAction.Move
        // which the ListView handles without clearing selection.
        MoveNoteToTop(note);

        _localStore.Save(_folders, _notes);
        _ = _syncService?.SendAsync(new WsMessage
        {
            Type      = "update_note",
            NoteId    = id,
            Title     = title,
            Content   = content,
            UpdatedAt = now
        });
    }

    private void MoveNoteToTop(Note note)
    {
        for (int i = 0; i < FilteredNotes.Count; i++)
        {
            if (FilteredNotes[i].Id == note.Id)
            {
                if (i > 0) FilteredNotes.Move(i, 0);
                return;
            }
        }
        // Not in FilteredNotes (e.g. viewing a different folder) — nothing to do.
    }

    // ── Private: sync ─────────────────────────────────────────────────────────

    private void StartSync()
    {
        _syncService = new SyncService(
            _serverAddress,
            onMessage:    msg => _dispatcher.TryEnqueue(() => HandleMessage(msg)),
            onConnection: ok  => _dispatcher.TryEnqueue(() => _onConnectionChanged(ok)));
    }

    private void HandleMessage(WsMessage msg)
    {
        switch (msg.Type)
        {
            case "init":
                HandleInit(msg.Folders ?? Array.Empty<Folder>(),
                           msg.Notes   ?? Array.Empty<Note>());
                return; // HandleInit calls Save

            case "folder_created":
                if (msg.Folder is { } fc && !_folders.Any(f => f.Id == fc.Id))
                {
                    _folders.Add(fc);
                    FolderItems.Add(FolderItem.FromFolder(fc));
                }
                break;

            case "folder_renamed":
                if (msg.Folder is { } fr)
                    UpdateFolderName(fr.Id, fr.Name);
                break;

            case "folder_deleted":
                if (msg.FolderId is { } fdId)
                {
                    _folders.RemoveAll(f => f.Id == fdId);
                    _notes.RemoveAll(n => n.FolderId == fdId);
                    var item = FolderItems.FirstOrDefault(f => f.Id == fdId);
                    if (item is not null) FolderItems.Remove(item);
                    if (_selectedFolderId == fdId) _selectedFolderId = AllNotesId;
                    DiffFilteredNotes();
                }
                break;

            case "note_created":
                if (msg.Note is { } nc && !_notes.Any(n => n.Id == nc.Id))
                {
                    _notes.Add(nc);
                    DiffFilteredNotes();
                    _onNoteAutoSelected(nc.Id, nc.Title, nc.Content);
                    _editingNoteId = nc.Id;
                }
                break;

            case "note_updated":
                if (msg.Note is { } nu)
                {
                    var existing = _notes.FirstOrDefault(n => n.Id == nu.Id);
                    if (existing is not null && nu.UpdatedAt > existing.UpdatedAt)
                    {
                        // Update in-place — INotifyPropertyChanged refreshes the row.
                        existing.Title     = nu.Title;
                        existing.Content   = nu.Content;
                        existing.UpdatedAt = nu.UpdatedAt;
                        MoveNoteToTop(existing);
                        if (_editingNoteId == nu.Id)
                            _onEditorChanged(nu.Title, nu.Content);
                    }
                }
                break;

            case "note_deleted":
                if (msg.NoteId is { } ndId)
                {
                    _notes.RemoveAll(n => n.Id == ndId);
                    DiffFilteredNotes();
                    if (_editingNoteId == ndId)
                    {
                        _editingNoteId = null;
                        _onEditorChanged("", "");
                    }
                }
                break;
        }

        _localStore.Save(_folders, _notes);
    }

    private void HandleInit(Folder[] serverFolders, Note[] serverNotes)
    {
        var savedEditingId = _editingNoteId;

        _folders.Clear();
        _folders.AddRange(serverFolders);
        RebuildFolderItems();

        if (serverFolders.Length == 0)
            _ = _syncService?.SendAsync(new WsMessage { Type = "create_folder", Name = "Notes" });

        var serverMap = serverNotes.ToDictionary(n => n.Id);
        var merged    = new List<Note>();

        foreach (var local in _notes)
        {
            if (serverMap.TryGetValue(local.Id, out var server))
            {
                serverMap.Remove(local.Id);
                if (local.UpdatedAt > server.UpdatedAt)
                {
                    merged.Add(local);
                    _ = _syncService?.SendAsync(new WsMessage
                    {
                        Type      = "update_note",
                        NoteId    = local.Id,
                        Title     = local.Title,
                        Content   = local.Content,
                        UpdatedAt = local.UpdatedAt
                    });
                }
                else
                {
                    // Update the existing Note object in-place so that if the object
                    // is already in FilteredNotes, the row refreshes via INotifyPropertyChanged.
                    local.Title     = server.Title;
                    local.Content   = server.Content;
                    local.UpdatedAt = server.UpdatedAt;
                    merged.Add(local);
                }
            }
            // Local-only (offline-created, no server ID): dropped.
        }
        merged.AddRange(serverMap.Values);

        _notes.Clear();
        _notes.AddRange(merged);

        // Refresh editor if the active note was updated by the server.
        if (savedEditingId is not null)
        {
            var n = _notes.FirstOrDefault(n => n.Id == savedEditingId);
            if (n is not null)
            {
                _editingNoteId = savedEditingId;
                _onEditorChanged(n.Title, n.Content);
            }
        }

        // Update the visible list using diff (preserves selection if note still exists).
        DiffFilteredNotes();
        _localStore.Save(_folders, _notes);
    }

    // ── Private: collection helpers ───────────────────────────────────────────

    private void RebuildFolderItems()
    {
        FolderItems.Clear();
        FolderItems.Add(new FolderItem { Id = AllNotesId, Name = "All Notes" });
        foreach (var f in _folders)
            FolderItems.Add(FolderItem.FromFolder(f));
    }

    /// Updates FilteredNotes via a diff so the ListView never loses selection:
    ///   • Items no longer visible are removed (RemoveAt)
    ///   • Existing items are moved to their sorted positions (Move — preserves selection)
    ///   • New items are inserted at the correct position
    private void DiffFilteredNotes()
    {
        var target = _notes
            .Where(n => _selectedFolderId == AllNotesId || n.FolderId == _selectedFolderId)
            .OrderByDescending(n => n.UpdatedAt)
            .ToList();

        // Pass 1: remove items that are no longer visible.
        for (int i = FilteredNotes.Count - 1; i >= 0; i--)
        {
            if (!target.Any(n => n.Id == FilteredNotes[i].Id))
                FilteredNotes.RemoveAt(i);
        }

        // Pass 2: insert new items and move existing items to their correct positions.
        for (int ti = 0; ti < target.Count; ti++)
        {
            var targetNote = target[ti];

            // Find the note in the current collection starting from index ti.
            int ci = -1;
            for (int j = ti; j < FilteredNotes.Count; j++)
            {
                if (FilteredNotes[j].Id == targetNote.Id) { ci = j; break; }
            }

            if (ci == -1)
                FilteredNotes.Insert(ti, targetNote);   // new item — insert
            else if (ci != ti)
                FilteredNotes.Move(ci, ti);             // wrong position — move (preserves selection)
            // else: already at correct position, no-op
        }
    }

    private void UpdateFolderName(string id, string name)
    {
        var folder = _folders.FirstOrDefault(f => f.Id == id);
        if (folder is not null) folder.Name = name;

        for (int i = 0; i < FolderItems.Count; i++)
        {
            if (FolderItems[i].Id == id)
            {
                FolderItems[i] = new FolderItem { Id = id, Name = name };
                break;
            }
        }
    }

    // ── Private: settings ─────────────────────────────────────────────────────

    private static string LoadSettings()
    {
        try
        {
            if (!File.Exists(SettingsFile)) return DefaultServer;
            var doc = System.Text.Json.JsonDocument.Parse(File.ReadAllText(SettingsFile));
            return doc.RootElement.GetProperty("serverAddress").GetString() ?? DefaultServer;
        }
        catch { return DefaultServer; }
    }

    private static void SaveSettings(string address)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(SettingsFile)!);
            File.WriteAllText(SettingsFile,
                System.Text.Json.JsonSerializer.Serialize(new { serverAddress = address }));
        }
        catch { }
    }

    public void Dispose() => _syncService?.Dispose();
}
