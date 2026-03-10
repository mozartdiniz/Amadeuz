package com.amadeuz.notes

import android.app.Application
import android.content.Context
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class NoteViewModel(application: Application) : AndroidViewModel(application) {

    companion object {
        const val ALL_NOTES_ID = "__all__"
    }

    private val prefs = application.getSharedPreferences("amadeuz", Context.MODE_PRIVATE)
    private val localStore = LocalStore(application)

    private val _folders = MutableStateFlow<List<Folder>>(emptyList())
    val folders = _folders.asStateFlow()

    private val _notes = MutableStateFlow<List<Note>>(emptyList())
    val notes = _notes.asStateFlow()

    private val _selectedFolderID = MutableStateFlow<String?>(null)
    val selectedFolderID = _selectedFolderID.asStateFlow()

    private val _selectedNoteID = MutableStateFlow<String?>(null)
    val selectedNoteID = _selectedNoteID.asStateFlow()

    private val _editingTitle = MutableStateFlow("")
    val editingTitle = _editingTitle.asStateFlow()

    private val _editingContent = MutableStateFlow("")
    val editingContent = _editingContent.asStateFlow()

    private val _isConnected = MutableStateFlow(false)
    val isConnected = _isConnected.asStateFlow()

    private val _serverAddress = MutableStateFlow(
        prefs.getString("serverAddress", "ws://localhost:8080/ws") ?: "ws://localhost:8080/ws"
    )
    val serverAddress = _serverAddress.asStateFlow()

    private var editingNoteID: String? = null
    private var syncService: SyncService? = null
    private var debounceJob: Job? = null

    init {
        val (savedFolders, savedNotes) = localStore.load()
        _folders.value = savedFolders
        _notes.value = savedNotes
        _selectedFolderID.value = ALL_NOTES_ID
        startSync()
    }

    // MARK: - Editor input (called from UI)

    fun onTitleChanged(title: String) {
        _editingTitle.value = title
        scheduleFlush()
    }

    fun onContentChanged(content: String) {
        _editingContent.value = content
        scheduleFlush()
    }

    private fun scheduleFlush() {
        debounceJob?.cancel()
        debounceJob = viewModelScope.launch {
            delay(500)
            val id = editingNoteID ?: return@launch
            flushNote(id, _editingTitle.value, _editingContent.value)
        }
    }

    private fun flushNote(id: String, title: String, content: String) {
        val list = _notes.value.toMutableList()
        val idx = list.indexOfFirst { it.id == id }
        if (idx == -1) return
        val stored = list[idx]
        if (stored.title == title && stored.content == content) return
        val now = System.currentTimeMillis()
        list[idx] = stored.copy(title = title, content = content, updated_at = now)
        _notes.value = list
        localStore.save(_folders.value, list)
        syncService?.send(WSMsg(type = "update_note", note_id = id, title = title, content = content, updated_at = now))
    }

    // MARK: - WebSocket sync

    private fun startSync() {
        syncService = SyncService(
            url = _serverAddress.value,
            onMessage = { msg -> handleMessage(msg) },
            onConnectionChange = { connected ->
                viewModelScope.launch(Dispatchers.Main) { _isConnected.value = connected }
            }
        )
    }

    private fun handleMessage(msg: WSMsg) {
        viewModelScope.launch(Dispatchers.Main) {
            when (msg.type) {
                "init" -> handleInit(msg.folders ?: emptyList(), msg.notes ?: emptyList())

                "folder_created" -> {
                    val f = msg.folder ?: return@launch
                    if (_folders.value.none { it.id == f.id }) {
                        _folders.value = _folders.value + f
                    }
                }

                "folder_renamed" -> {
                    val f = msg.folder ?: return@launch
                    _folders.value = _folders.value.map { if (it.id == f.id) it.copy(name = f.name) else it }
                }

                "folder_deleted" -> {
                    val fid = msg.folder_id ?: return@launch
                    _folders.value = _folders.value.filter { it.id != fid }
                    _notes.value = _notes.value.filter { it.folder_id != fid }
                    if (_selectedFolderID.value == fid) {
                        _selectedFolderID.value = null
                        clearEditor()
                    }
                }

                "note_created" -> {
                    val n = msg.note ?: return@launch
                    if (_notes.value.none { it.id == n.id }) {
                        _notes.value = _notes.value + n
                        val visible = n.folder_id == _selectedFolderID.value
                                || _selectedFolderID.value == ALL_NOTES_ID
                        if (visible) selectNote(n.id)
                    }
                }

                "note_updated" -> {
                    val n = msg.note ?: return@launch
                    val list = _notes.value.toMutableList()
                    val idx = list.indexOfFirst { it.id == n.id }
                    if (idx != -1 && n.updated_at > list[idx].updated_at) {
                        list[idx] = n
                        _notes.value = list
                        if (_selectedNoteID.value == n.id) {
                            _editingTitle.value = n.title
                            _editingContent.value = n.content
                        }
                    }
                }

                "note_deleted" -> {
                    val nid = msg.note_id ?: return@launch
                    _notes.value = _notes.value.filter { it.id != nid }
                    if (_selectedNoteID.value == nid) clearEditor()
                }
            }
            localStore.save(_folders.value, _notes.value)
        }
    }

    private fun handleInit(serverFolders: List<Folder>, serverNotes: List<Note>) {
        _folders.value = serverFolders
        if (serverFolders.isEmpty()) {
            syncService?.send(WSMsg(type = "create_folder", name = "Notes"))
        }

        val serverMap = serverNotes.associateBy { it.id }.toMutableMap()
        val merged = mutableListOf<Note>()
        for (local in _notes.value) {
            val server = serverMap.remove(local.id)
            if (server != null) {
                if (local.updated_at > server.updated_at) {
                    merged.add(local)
                    syncService?.send(WSMsg(type = "update_note", note_id = local.id,
                        title = local.title, content = local.content, updated_at = local.updated_at))
                } else {
                    merged.add(server)
                }
            }
            // Local-only notes created offline are dropped on reconnect.
        }
        merged.addAll(serverMap.values)
        _notes.value = merged

        val id = _selectedNoteID.value
        if (id != null) {
            val n = merged.firstOrNull { it.id == id }
            if (n != null) {
                _editingTitle.value = n.title
                _editingContent.value = n.content
            }
        }
        localStore.save(_folders.value, merged)
    }

    // MARK: - Selection

    fun selectFolder(id: String?) {
        val old = _selectedNoteID.value
        if (old != null) flushNote(old, _editingTitle.value, _editingContent.value)
        _selectedFolderID.value = id
        _selectedNoteID.value = null
        clearEditor()
    }

    fun selectNote(id: String?) {
        val old = _selectedNoteID.value
        if (old != null) flushNote(old, _editingTitle.value, _editingContent.value)
        _selectedNoteID.value = id
        val note = _notes.value.firstOrNull { it.id == id }
        editingNoteID = note?.id
        _editingTitle.value = note?.title ?: ""
        _editingContent.value = note?.content ?: ""
    }

    // MARK: - Folder actions

    fun createFolder(name: String) {
        syncService?.send(WSMsg(type = "create_folder", name = name))
    }

    fun renameFolder(id: String, name: String) {
        _folders.value = _folders.value.map { if (it.id == id) it.copy(name = name) else it }
        localStore.save(_folders.value, _notes.value)
        syncService?.send(WSMsg(type = "rename_folder", folder_id = id, name = name))
    }

    fun deleteFolder(id: String) {
        _folders.value = _folders.value.filter { it.id != id }
        _notes.value = _notes.value.filter { it.folder_id != id }
        if (_selectedFolderID.value == id) { _selectedFolderID.value = null; clearEditor() }
        localStore.save(_folders.value, _notes.value)
        syncService?.send(WSMsg(type = "delete_folder", folder_id = id))
    }

    // MARK: - Note actions

    fun createNote() {
        val fid = _selectedFolderID.value ?: return
        if (fid == ALL_NOTES_ID) return
        syncService?.send(WSMsg(type = "create_note", folder_id = fid, title = ""))
    }

    fun deleteNote(id: String) {
        _notes.value = _notes.value.filter { it.id != id }
        if (_selectedNoteID.value == id) clearEditor()
        localStore.save(_folders.value, _notes.value)
        syncService?.send(WSMsg(type = "delete_note", note_id = id))
    }

    // MARK: - Settings

    fun updateServerAddress(address: String) {
        prefs.edit().putString("serverAddress", address).apply()
        _serverAddress.value = address
        syncService?.close()
        _isConnected.value = false
        startSync()
    }

    // MARK: - Helpers

    fun notesInFolder(folderID: String?): List<Note> {
        val sorted = _notes.value.sortedByDescending { it.updated_at }
        if (folderID == null || folderID == ALL_NOTES_ID) return sorted
        return sorted.filter { it.folder_id == folderID }
    }

    private fun clearEditor() {
        _selectedNoteID.value = null
        editingNoteID = null
        _editingTitle.value = ""
        _editingContent.value = ""
    }

    override fun onCleared() {
        super.onCleared()
        syncService?.close()
    }
}
