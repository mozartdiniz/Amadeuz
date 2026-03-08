import SwiftUI

struct ContentView: View {
    @StateObject private var vm = NoteViewModel()
    @State private var showSettings = false

    var body: some View {
        VStack(spacing: 0) {
            TextEditor(text: $vm.content)
                .font(.system(size: 16))
                .padding(12)

            Divider()

            StatusBar(isConnected: vm.isConnected) {
                showSettings = true
            }
        }
        .sheet(isPresented: $showSettings) {
            SettingsView(serverAddress: $vm.serverAddress)
        }
    }
}

// MARK: - Status Bar

private struct StatusBar: View {
    let isConnected: Bool
    let onSettings: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(isConnected ? Color.green : Color.red)
                .frame(width: 7, height: 7)

            Text(isConnected ? "Synced" : "Offline")
                .font(.caption)
                .foregroundStyle(.secondary)

            Spacer()

            Button("Settings", action: onSettings)
                .buttonStyle(.plain)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

// MARK: - Settings

struct SettingsView: View {
    @Binding var serverAddress: String
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Server")
                .font(.headline)

            LabeledContent("WebSocket URL") {
                TextField("ws://hostname:8080/ws", text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 280)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Connect") {
                    serverAddress = draft
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(24)
        .frame(width: 420)
        .onAppear { draft = serverAddress }
    }
}
