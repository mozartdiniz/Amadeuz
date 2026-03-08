using System;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Amadeuz;

public record NoteData
{
    [JsonPropertyName("content")]  public string Content   { get; init; } = "";
    [JsonPropertyName("updatedAt")] public long  UpdatedAt { get; init; }

    public static NoteData Empty => new();
}

/// <summary>
/// Persists note content to the app's local data folder as note.json.
/// Same JSON schema as the macOS client.
/// </summary>
public class LocalStore
{
    private readonly string _filePath;

    public LocalStore()
    {
        // %APPDATA%\amadeuz\note.json  — matches the spec and survives reinstalls.
        var folder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "amadeuz");
        Directory.CreateDirectory(folder);
        _filePath = Path.Combine(folder, "note.json");
    }

    public NoteData Load()
    {
        try
        {
            if (!File.Exists(_filePath)) return NoteData.Empty;
            var json = File.ReadAllText(_filePath);
            return JsonSerializer.Deserialize<NoteData>(json) ?? NoteData.Empty;
        }
        catch { return NoteData.Empty; }
    }

    public void Save(string content, long updatedAt)
    {
        try { File.WriteAllText(_filePath, JsonSerializer.Serialize(new NoteData { Content = content, UpdatedAt = updatedAt })); }
        catch { }
    }
}
