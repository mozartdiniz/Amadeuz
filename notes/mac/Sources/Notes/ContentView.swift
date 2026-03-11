import SwiftUI

// MARK: - Root

struct ContentView: View {
    @StateObject private var vm = NotesViewModel()
    @State private var showSettings = false

    var body: some View {
        NavigationSplitView {
            FolderSidebar(vm: vm)
        } content: {
            NoteList(vm: vm)
        } detail: {
            NoteEditor(vm: vm, showSettings: $showSettings)
        }
        .searchable(text: $vm.searchText, placement: .toolbar, prompt: "Search notes")
        .sheet(isPresented: $showSettings) {
            SettingsView(serverAddress: $vm.serverAddress)
        }
    }
}

// MARK: - Folder sidebar (left column)

private struct FolderSidebar: View {
    @ObservedObject var vm: NotesViewModel
    @State private var showNewFolder = false
    @State private var newFolderName = ""
    @State private var renamingFolder: Folder?
    @State private var renameText = ""

    var body: some View {
        List(selection: $vm.selectedFolderID) {
            Label("All Notes", systemImage: "tray.2.fill")
                .tag(NotesViewModel.allNotesID)

            if !vm.folders.isEmpty {
                Section("Folders") {
                    ForEach(vm.folders) { folder in
                        Text(folder.name)
                            .tag(folder.id)
                            .contextMenu {
                                Button("Rename…") {
                                    renamingFolder = folder
                                    renameText = folder.name
                                }
                                Divider()
                                Button("Delete Folder", role: .destructive) {
                                    vm.deleteFolder(id: folder.id)
                                }
                            }
                    }
                }
            }
        }
        .onChange(of: vm.selectedFolderID) { _, _ in
            vm.folderSelectionChanged()
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Button {
                showNewFolder = true
            } label: {
                Label("New Folder", systemImage: "folder.badge.plus")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
            .buttonStyle(.plain)
            .background(.bar)
        }
        .navigationTitle("Folders")
        .alert("New Folder", isPresented: $showNewFolder) {
            TextField("Folder name", text: $newFolderName)
            Button("Create") {
                let name = newFolderName.trimmingCharacters(in: .whitespaces)
                if !name.isEmpty { vm.createFolder(name: name) }
                newFolderName = ""
            }
            Button("Cancel", role: .cancel) { newFolderName = "" }
        }
        .alert("Rename Folder", isPresented: Binding(
            get: { renamingFolder != nil },
            set: { if !$0 { renamingFolder = nil } }
        )) {
            TextField("Folder name", text: $renameText)
            Button("Rename") {
                if let f = renamingFolder {
                    let name = renameText.trimmingCharacters(in: .whitespaces)
                    if !name.isEmpty { vm.renameFolder(id: f.id, name: name) }
                }
                renamingFolder = nil
            }
            Button("Cancel", role: .cancel) { renamingFolder = nil }
        }
    }
}

// MARK: - Note list (middle column)

private struct NoteList: View {
    @ObservedObject var vm: NotesViewModel

    var body: some View {
        Group {
            if vm.notesInSelectedFolder.isEmpty {
                ContentUnavailableView("No Notes", systemImage: "note.text")
            } else {
                List(vm.notesInSelectedFolder, selection: $vm.selectedNoteID) { note in
                    NoteRow(note: note, folderName: vm.folders.first { $0.id == note.folderID }?.name)
                        .contextMenu {
                            Menu("Move to Folder") {
                                Button("No Folder") {
                                    vm.moveNote(id: note.id, toFolderID: nil)
                                }
                                if !vm.folders.isEmpty {
                                    Divider()
                                    ForEach(vm.folders) { folder in
                                        Button(folder.name) {
                                            vm.moveNote(id: note.id, toFolderID: folder.id)
                                        }
                                    }
                                }
                            }
                            Divider()
                            Button("Delete Note", role: .destructive) {
                                vm.deleteNote(id: note.id)
                            }
                        }
                }
                .onChange(of: vm.selectedNoteID) { old, new in
                    vm.noteSelectionChanged(from: old, to: new)
                }
            }
        }
        .navigationTitle(vm.selectedFolderID == NotesViewModel.allNotesID
            ? "All Notes"
            : (vm.selectedFolder?.name ?? "Notes"))
        .toolbar {
            ToolbarItem {
                Button(role: .destructive) {
                    if let id = vm.selectedNoteID { vm.deleteNote(id: id) }
                } label: {
                    Label("Delete Note", systemImage: "trash")
                }
                .disabled(vm.selectedNoteID == nil)
            }
            ToolbarItem {
                Button { vm.createNote() } label: {
                    Label("New Note", systemImage: "square.and.pencil")
                }
                .disabled(vm.selectedFolderID == nil)
            }
        }
    }
}

// MARK: - Note row

private struct NoteRow: View {
    let note: Note
    let folderName: String?

    private var displayTitle: String {
        note.title.trimmingCharacters(in: .whitespaces).isEmpty ? "Untitled" : note.title
    }

    private var preview: String {
        let trimmed = note.content.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "No additional text" : trimmed
    }

    private var dateString: String {
        let date = Date(timeIntervalSince1970: Double(note.updatedAt) / 1000)
        let cal  = Calendar.current
        if cal.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        } else if cal.isDateInYesterday(date) {
            return "Yesterday"
        } else {
            return date.formatted(.dateTime.month(.abbreviated).day().year())
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(displayTitle)
                    .font(.headline)
                    .lineLimit(1)
                Spacer()
                Text(dateString)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Text(preview)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            if let folderName {
                Label(folderName, systemImage: "folder")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 3)
    }
}

// MARK: - Note editor (right column)

private struct NoteEditor: View {
    @ObservedObject var vm: NotesViewModel
    @Binding var showSettings: Bool

    var body: some View {
        Group {
            if vm.selectedNoteID == nil {
                ContentUnavailableView("Select a Note", systemImage: "note.text")
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    TextField("Title", text: $vm.editingTitle)
                        .font(.title2.bold())
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 20)
                        .padding(.top, 20)
                        .padding(.bottom, 10)

                    Divider()

                    TextEditor(text: $vm.editingContent)
                        .font(.body)
                        .padding(16)
                        .scrollContentBackground(.hidden)
                }
                .background(.windowBackground)
            }
        }
        .toolbar {
            ToolbarItem(placement: .automatic) {
                HStack(spacing: 8) {
                    Circle()
                        .fill(vm.isConnected ? Color.green : Color.red)
                        .frame(width: 7, height: 7)
                    Text(vm.isConnected ? "Synced" : "Offline")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Button("Settings") { showSettings = true }
                        .font(.caption)
                }
            }
        }
    }
}

// MARK: - Settings sheet

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
