using System;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Text.Json.Serialization;

namespace Amadeuz;

// ── Domain models ─────────────────────────────────────────────────────────────

public sealed class Folder
{
    [JsonPropertyName("id")]         public string Id        { get; set; } = "";
    [JsonPropertyName("name")]       public string Name      { get; set; } = "";
    [JsonPropertyName("created_at")] public long   CreatedAt { get; set; }
}

/// <summary>
/// Implements INotifyPropertyChanged so the ListView can update rows in-place
/// without the collection being rebuilt (which would clear selection).
/// </summary>
public sealed class Note : INotifyPropertyChanged
{
    [JsonPropertyName("id")]         public string Id        { get; set; } = "";
    [JsonPropertyName("folder_id")]  public string FolderId  { get; set; } = "";
    [JsonPropertyName("created_at")] public long   CreatedAt { get; set; }

    private string _title = "";
    [JsonPropertyName("title")]
    public string Title
    {
        get => _title;
        set
        {
            if (_title == value) return;
            _title = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(DisplayTitle));
        }
    }

    private string _content = "";
    [JsonPropertyName("content")]
    public string Content
    {
        get => _content;
        set
        {
            if (_content == value) return;
            _content = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(ContentPreview));
        }
    }

    private long _updatedAt;
    [JsonPropertyName("updated_at")]
    public long UpdatedAt
    {
        get => _updatedAt;
        set
        {
            if (_updatedAt == value) return;
            _updatedAt = value;
            OnPropertyChanged();
            OnPropertyChanged(nameof(FormattedDate));
        }
    }

    // Computed display properties for the note list DataTemplate (x:Bind Mode=OneWay).
    [JsonIgnore] public string DisplayTitle   => string.IsNullOrWhiteSpace(Title)   ? "Untitled"           : Title.Trim();
    [JsonIgnore] public string ContentPreview => string.IsNullOrWhiteSpace(Content) ? "No additional text" : Content.Trim();
    [JsonIgnore] public string FormattedDate
    {
        get
        {
            var dt    = DateTimeOffset.FromUnixTimeMilliseconds(UpdatedAt).LocalDateTime;
            var today = DateTime.Today;
            if (dt.Date == today)             return dt.ToString("h:mm tt");
            if (dt.Date == today.AddDays(-1)) return "Yesterday";
            return dt.ToString("MMM d, yyyy");
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;
    private void OnPropertyChanged([CallerMemberName] string? name = null) =>
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}

/// <summary>
/// Flat item used in the folder sidebar ListView.
/// The sentinel "All Notes" item always appears first.
/// </summary>
public sealed class FolderItem
{
    public string Id   { get; init; } = "";
    public string Name { get; init; } = "";

    [JsonIgnore] public bool IsAllNotes => Id == NotesViewModel.AllNotesId;

    public static FolderItem FromFolder(Folder f) => new() { Id = f.Id, Name = f.Name };
}

// ── Wire message ──────────────────────────────────────────────────────────────

public sealed class WsMessage
{
    [JsonPropertyName("type")]       public string   Type      { get; set; } = "";
    [JsonPropertyName("folders")]    public Folder[]? Folders  { get; set; }
    [JsonPropertyName("notes")]      public Note[]?   Notes    { get; set; }
    [JsonPropertyName("folder")]     public Folder?   Folder   { get; set; }
    [JsonPropertyName("note")]       public Note?     Note     { get; set; }
    [JsonPropertyName("folder_id")]  public string?   FolderId { get; set; }
    [JsonPropertyName("note_id")]    public string?   NoteId   { get; set; }
    [JsonPropertyName("name")]       public string?   Name     { get; set; }
    [JsonPropertyName("title")]      public string?   Title    { get; set; }
    [JsonPropertyName("content")]    public string?   Content  { get; set; }
    [JsonPropertyName("updated_at")] public long?     UpdatedAt { get; set; }
}
