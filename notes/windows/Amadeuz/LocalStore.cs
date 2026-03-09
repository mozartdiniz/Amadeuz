using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Amadeuz;

/// <summary>
/// Persists folders and notes to %APPDATA%\amadeuz\data.json.
/// Same JSON schema as the macOS and server.
/// Writes are atomic (temp file + rename) so a crash mid-write can't corrupt the file.
/// </summary>
public sealed class LocalStore
{
    private readonly string _filePath;

    public LocalStore()
    {
        var dir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "amadeuz");
        Directory.CreateDirectory(dir);
        _filePath = Path.Combine(dir, "data.json");
    }

    public (List<Folder> folders, List<Note> notes) Load()
    {
        try
        {
            if (!File.Exists(_filePath)) return (new(), new());
            var doc = JsonSerializer.Deserialize<LocalData>(File.ReadAllText(_filePath));
            return (doc?.Folders ?? new(), doc?.Notes ?? new());
        }
        catch { return (new(), new()); }
    }

    public void Save(IEnumerable<Folder> folders, IEnumerable<Note> notes)
    {
        try
        {
            var tmp = _filePath + ".tmp";
            File.WriteAllText(tmp, JsonSerializer.Serialize(
                new LocalData { Folders = new(folders), Notes = new(notes) }));
            File.Move(tmp, _filePath, overwrite: true);
        }
        catch { }
    }

    private sealed class LocalData
    {
        [JsonPropertyName("folders")] public List<Folder> Folders { get; init; } = new();
        [JsonPropertyName("notes")]   public List<Note>   Notes   { get; init; } = new();
    }
}
