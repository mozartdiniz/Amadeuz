import Foundation

/// WebSocket connection for a single note's live sync.
///
/// Receives `init` and `update` messages from the server.
/// Content updates are sent via REST (PATCH /notes/:id), not WebSocket;
/// this connection only exists to receive changes from other clients.
final class NoteSync {
    private let url: URL
    private var task: URLSessionWebSocketTask?
    private var alive = true

    private let onInit:             (NoteWsMsg) -> Void
    private let onUpdate:           (NoteWsMsg) -> Void
    private let onConnectionChange: (Bool) -> Void

    private let decoder = JSONDecoder()

    init(
        url: URL,
        onInit:             @escaping (NoteWsMsg) -> Void,
        onUpdate:           @escaping (NoteWsMsg) -> Void,
        onConnectionChange: @escaping (Bool) -> Void
    ) {
        self.url               = url
        self.onInit            = onInit
        self.onUpdate          = onUpdate
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
                   let msg = try? self.decoder.decode(NoteWsMsg.self, from: data)
                {
                    self.onConnectionChange(true)
                    switch msg.type {
                    case "init":   self.onInit(msg)
                    case "update": self.onUpdate(msg)
                    default: break
                    }
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
}
