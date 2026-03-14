import SwiftUI
import PhotosUI

// MARK: - Root

struct ContentView: View {
    @EnvironmentObject private var vm: NotesViewModel
    @State private var showSettings = false

    var body: some View {
        Group {
            if vm.isAuthenticated {
                NavigationSplitView {
                    FolderSidebar(vm: vm, showSettings: $showSettings)
                } content: {
                    NoteList(vm: vm)
                } detail: {
                    NoteEditor(vm: vm)
                }
                .searchable(text: $vm.searchText, placement: .toolbar, prompt: "Search")
                .sheet(isPresented: $showSettings) {
                    SettingsView(vm: vm)
                }
            } else {
                AuthView(vm: vm)
            }
        }
        .sheet(item: Binding(
            get: { vm.pendingRecoveryCode.map { RecoveryCodePresentation(code: $0) } },
            set: { if $0 == nil { vm.pendingRecoveryCode = nil } }
        )) { p in
            RecoveryCodeView(code: p.code)
        }
    }
}

// MARK: - Folder sidebar (left column)

private struct FolderSidebar: View {
    @ObservedObject var vm: NotesViewModel
    @Binding var showSettings: Bool
    @State private var showNewFolder  = false
    @State private var newFolderName  = ""
    @State private var renamingFolder: Folder?
    @State private var renameText     = ""

    var body: some View {
        List(selection: $vm.selectedFolderID) {

            // All Notes — always at top
            Label {
                Text("All Notes")
            } icon: {
                Image(systemName: "tray.2.fill")
                    .foregroundStyle(.yellow)
            }
            .badge(vm.allNotesCount)
            .tag(NotesViewModel.allNotesID)

            // User folders
            if !vm.folders.isEmpty {
                Section("Folders") {
                    ForEach(vm.folders) { folder in
                        Label {
                            Text(folder.name)
                        } icon: {
                            Image(systemName: "folder")
                        }
                        .badge(vm.noteCount(for: folder.id))
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

            // Trash — always at bottom
            Section {
                Label {
                    Text("Recently Deleted")
                } icon: {
                    Image(systemName: "trash")
                }
                .badge(vm.trashCount)
                .tag(NotesViewModel.trashID)
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
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.plain)
            .background(.bar)
        }
        .navigationTitle("Notes")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showSettings = true
                } label: {
                    Image(systemName: "gear")
                }
            }
        }
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
            if vm.noteSections.isEmpty {
                ContentUnavailableView(
                    vm.isTrashView ? "No Deleted Notes" : "No Notes",
                    systemImage: vm.isTrashView ? "trash" : "note.text"
                )
            } else {
                List(selection: $vm.selectedNoteID) {
                    ForEach(vm.noteSections) { section in
                        Section(section.title) {
                            ForEach(section.notes) { note in
                                NoteRow(
                                    note: note,
                                    folderName: vm.folders.first { $0.id == note.folderID }?.name,
                                    isTrashView: vm.isTrashView
                                )
                                .tag(note.id)
                                .contextMenu {
                                    if vm.isTrashView {
                                        Button("Restore") {
                                            vm.restoreNote(id: note.id)
                                        }
                                        Divider()
                                        Button("Delete Permanently", role: .destructive) {
                                            vm.permanentlyDeleteNote(id: note.id)
                                        }
                                    } else {
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
                                        Button("Move to Trash", role: .destructive) {
                                            vm.trashNote(id: note.id)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                .onChange(of: vm.selectedNoteID) { old, new in
                    vm.noteSelectionChanged(from: old, to: new)
                }
            }
        }
        .navigationTitle(vm.currentFolderTitle)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    vm.createNote()
                } label: {
                    Label("New Note", systemImage: "square.and.pencil")
                }
                .disabled(vm.selectedFolderID == nil || vm.isTrashView)
            }

            if vm.isTrashView && !vm.noteSections.isEmpty {
                ToolbarItem {
                    Button("Empty Trash", role: .destructive) {
                        for section in vm.noteSections {
                            for note in section.notes {
                                vm.permanentlyDeleteNote(id: note.id)
                            }
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Note row

private struct NoteRow: View {
    let note: Note
    let folderName: String?
    let isTrashView: Bool

    private var displayTitle: String {
        note.title.trimmingCharacters(in: .whitespaces).isEmpty ? "New Note" : note.title
    }

    private var preview: String {
        let text = note.content
            .replacingOccurrences(of: #"!\[[^\]]*\]\(amadeuz://blob/[a-f0-9\-]+\)"#,
                                  with: "",
                                  options: .regularExpression)
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? "No additional text" : text
    }

    private var dateString: String {
        let date = Date(timeIntervalSince1970: Double(note.updatedAt) / 1000)
        let cal  = Calendar.current

        if cal.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }

        let startOfWeek = cal.date(from: cal.dateComponents([.yearForWeekOfYear, .weekOfYear], from: Date()))
        if let sow = startOfWeek, date >= sow {
            let fmt = DateFormatter()
            fmt.dateFormat = "EEEE"
            return fmt.string(from: date)
        }

        return date.formatted(.dateTime.day(.twoDigits).month(.twoDigits).year())
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(displayTitle)
                .font(.headline)
                .lineLimit(1)

            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(dateString)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .layoutPriority(1)
                Text(preview)
                    .font(.subheadline)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }

            Label(folderName ?? "Notes", systemImage: "folder")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
        }
        .padding(.vertical, 3)
    }
}

// MARK: - Note editor (detail column)

private struct NoteEditor: View {
    @ObservedObject var vm: NotesViewModel
    @State private var pendingImageData: Data? = nil
    @State private var photoPickerItems: [PhotosPickerItem] = []

    private var selectedNote: Note? {
        vm.notes.first { $0.id == vm.selectedNoteID }
    }

    private var noteDateString: String? {
        guard let note = selectedNote else { return nil }
        let date = Date(timeIntervalSince1970: Double(note.updatedAt) / 1000)
        let fmt  = DateFormatter()
        fmt.dateStyle = .long
        fmt.timeStyle = .short
        return fmt.string(from: date)
    }

    var body: some View {
        Group {
            if vm.selectedNoteID == nil {
                ContentUnavailableView("Select a Note", systemImage: "note.text")
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    if let dateStr = noteDateString {
                        Text(dateStr)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .frame(maxWidth: .infinity, alignment: .center)
                            .padding(.top, 14)
                            .padding(.bottom, 4)
                    }

                    MarkdownEditor(
                        markdown: $vm.editingBody,
                        blobStore: vm.blobStore,
                        pendingImageData: $pendingImageData
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                .background(.background)
            }
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                PhotosPicker(
                    selection: $photoPickerItems,
                    maxSelectionCount: 1,
                    matching: .images
                ) {
                    Image(systemName: "photo.badge.plus")
                }
                .onChange(of: photoPickerItems) { _, new in
                    guard let item = new.first else { return }
                    Task {
                        if let data = try? await item.loadTransferable(type: Data.self) {
                            pendingImageData = data
                        }
                        photoPickerItems = []
                    }
                }
                .disabled(vm.selectedNoteID == nil)
            }

            ToolbarItem(placement: .topBarTrailing) {
                HStack(spacing: 6) {
                    Circle()
                        .fill(vm.isConnected ? Color.green : Color.red)
                        .frame(width: 7, height: 7)
                    Text(vm.isConnected ? "Synced" : "Offline")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

// MARK: - Settings sheet

struct SettingsView: View {
    @ObservedObject var vm: NotesViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Server") {
                    TextField("http://hostname:8080", text: $draft)
                        .keyboardType(.URL)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                }

                Section {
                    Button("Sign Out", role: .destructive) {
                        vm.logout()
                        dismiss()
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        let normalized = NotesViewModel.normalizeServerAddress(draft)
                        vm.serverAddress = normalized
                        dismiss()
                    }
                    .bold()
                }
            }
        }
        .onAppear { draft = vm.serverAddress }
    }
}
