import Foundation

/// Manages a WebSocket connection to the sync server.
/// Automatically reconnects when the connection drops.
final class SyncService {
    private let url: URL
    private var task: URLSessionWebSocketTask?
    private var alive = true

    private let onMessage: (WSMsg) -> Void
    private let onConnectionChange: (Bool) -> Void

    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(
        url: URL,
        onMessage: @escaping (WSMsg) -> Void,
        onConnectionChange: @escaping (Bool) -> Void
    ) {
        self.url = url
        self.onMessage = onMessage
        self.onConnectionChange = onConnectionChange
        connect()
    }

    deinit {
        alive = false
        task?.cancel(with: .goingAway, reason: nil)
    }

    // MARK: - Private

    private func connect() {
        guard alive else { return }
        task = URLSession.shared.webSocketTask(with: url)
        task?.resume()
        receive()
    }

    private func receive() {
        task?.receive { [weak self] result in
            guard let self, self.alive else { return }
            switch result {
            case .success(let message):
                if case .string(let text) = message,
                   let data = text.data(using: .utf8),
                   let msg = try? self.decoder.decode(WSMsg.self, from: data)
                {
                    self.onConnectionChange(true)
                    self.onMessage(msg)
                }
                self.receive()

            case .failure:
                self.onConnectionChange(false)
                Task {
                    try? await Task.sleep(for: .seconds(3))
                    self.connect()
                }
            }
        }
    }

    // MARK: - Public

    func send(_ msg: WSMsg) {
        guard
            let data = try? encoder.encode(msg),
            let text = String(data: data, encoding: .utf8)
        else { return }
        task?.send(.string(text)) { _ in }
    }
}
