using System;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace Amadeuz;

/// <summary>
/// WebSocket client for the amadeuz sync server.
/// Automatically reconnects every 3 seconds on disconnect.
/// All callbacks are invoked from background threads.
/// </summary>
public sealed class SyncService : IDisposable
{
    private readonly string                _url;
    private readonly Action<string, long>  _onMessage;
    private readonly Action<bool>          _onConnection;
    private readonly CancellationTokenSource _cts = new();
    private ClientWebSocket?               _socket;

    public SyncService(string url, Action<string, long> onMessage, Action<bool> onConnection)
    {
        _url          = url;
        _onMessage    = onMessage;
        _onConnection = onConnection;
        _ = ConnectLoopAsync(_cts.Token);
    }

    private async Task ConnectLoopAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            _socket = new ClientWebSocket();
            try
            {
                await _socket.ConnectAsync(new Uri(_url), ct);
                _onConnection(true);
                await ReceiveLoopAsync(ct);
            }
            catch (OperationCanceledException) { return; }
            catch { /* connection failed — will retry */ }
            finally
            {
                _onConnection(false);
                _socket.Dispose();
                _socket = null;
            }

            try { await Task.Delay(3_000, ct); }
            catch (OperationCanceledException) { return; }
        }
    }

    private async Task ReceiveLoopAsync(CancellationToken ct)
    {
        var buffer = new byte[65_536];
        while (_socket?.State == WebSocketState.Open && !ct.IsCancellationRequested)
        {
            var sb = new StringBuilder();
            WebSocketReceiveResult result;
            do
            {
                result = await _socket.ReceiveAsync(buffer, ct);
                sb.Append(Encoding.UTF8.GetString(buffer, 0, result.Count));
            } while (!result.EndOfMessage);

            if (result.MessageType == WebSocketMessageType.Close) break;

            try
            {
                var msg = JsonSerializer.Deserialize<WsMessage>(sb.ToString());
                if (msg is not null)
                    _onMessage(msg.Content, msg.UpdatedAt);
            }
            catch { }
        }
    }

    public async Task SendAsync(string content, long updatedAt)
    {
        var socket = _socket;
        if (socket?.State != WebSocketState.Open) return;
        try
        {
            var json  = JsonSerializer.Serialize(new WsMessage("update", content, updatedAt));
            var bytes = Encoding.UTF8.GetBytes(json);
            await socket.SendAsync(bytes, WebSocketMessageType.Text, true, CancellationToken.None);
        }
        catch { }
    }

    public void Dispose()
    {
        _cts.Cancel();
        _cts.Dispose();
    }

    // Wire format — matches the Go server protocol exactly.
    private record WsMessage(
        [property: JsonPropertyName("type")]       string Type,
        [property: JsonPropertyName("content")]    string Content,
        [property: JsonPropertyName("updated_at")] long   UpdatedAt);
}
