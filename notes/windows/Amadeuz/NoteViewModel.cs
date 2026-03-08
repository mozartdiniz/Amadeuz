using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;

namespace Amadeuz;

/// <summary>
/// All business logic: offline-first load, 500 ms debounce, and timestamp-based merge.
/// Mirror of the macOS NoteViewModel. UI-thread-safe to call; callbacks come from background threads.
/// </summary>
public sealed class NoteViewModel : IDisposable
{
    private readonly Action<string> _onContentChanged;
    private readonly Action<bool>   _onConnectionChanged;
    private readonly LocalStore     _localStore = new();
    private SyncService?            _syncService;

    private string _content             = "";
    private string _lastReceivedContent = "";
    private long   _lastUpdatedAt;
    private string _serverAddress;

    private CancellationTokenSource _debounceCts = new();

    private static readonly string SettingsFile = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "amadeuz", "settings.json");
    private const string DefaultServer = "ws://localhost:8080/ws";

    public string Content       => _content;
    public string ServerAddress
    {
        get => _serverAddress;
        set
        {
            _serverAddress = value;
            SaveServerAddress(value);
            _syncService?.Dispose();
            _onConnectionChanged(false);
            StartSync();
        }
    }

    public NoteViewModel(Action<string> onContentChanged, Action<bool> onConnectionChanged)
    {
        _onContentChanged    = onContentChanged;
        _onConnectionChanged = onConnectionChanged;

        // Restore saved server address.
        _serverAddress = LoadServerAddress();

        // Offline-first: show locally-stored note immediately.
        var note             = _localStore.Load();
        _content             = note.Content;
        _lastReceivedContent = note.Content;
        _lastUpdatedAt       = note.UpdatedAt;

        StartSync();
    }

    // Called from the UI thread every time the text changes.
    public void OnTextChanged(string content)
    {
        _content = content;

        // Cancel any pending debounce and start a fresh 500 ms window.
        _debounceCts.Cancel();
        _debounceCts = new CancellationTokenSource();
        var ct = _debounceCts.Token;

        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(500, ct);

                // Echo from server — don't loop it back.
                if (_content == _lastReceivedContent) return;

                var now        = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
                _lastUpdatedAt = now;
                _localStore.Save(_content, now);
                if (_syncService is not null)
                    await _syncService.SendAsync(_content, now);
            }
            catch (OperationCanceledException) { }
        }, ct);
    }

    private void StartSync()
    {
        _syncService = new SyncService(
            _serverAddress,
            onMessage:    (content, updatedAt) => HandleServerMessage(content, updatedAt),
            onConnection: connected => _onConnectionChanged(connected));
    }

    // Called from background thread (network).
    private void HandleServerMessage(string content, long updatedAt)
    {
        if (updatedAt > _lastUpdatedAt)
        {
            // Server is newer — accept it and notify the UI.
            _lastReceivedContent = content;
            _content             = content;
            _lastUpdatedAt       = updatedAt;
            _localStore.Save(content, updatedAt);
            _onContentChanged(content);
        }
        else if (_lastUpdatedAt > updatedAt)
        {
            // We wrote offline and are ahead — push our version.
            _ = _syncService?.SendAsync(_content, _lastUpdatedAt);
        }
        // Equal timestamps → already in sync.
    }

    public void Dispose() => _syncService?.Dispose();

    private static string LoadServerAddress()
    {
        try
        {
            if (!File.Exists(SettingsFile)) return DefaultServer;
            var json = File.ReadAllText(SettingsFile);
            var doc  = System.Text.Json.JsonDocument.Parse(json);
            return doc.RootElement.GetProperty("serverAddress").GetString() ?? DefaultServer;
        }
        catch { return DefaultServer; }
    }

    private static void SaveServerAddress(string address)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(SettingsFile)!);
            File.WriteAllText(SettingsFile,
                System.Text.Json.JsonSerializer.Serialize(new { serverAddress = address }));
        }
        catch { }
    }
}
