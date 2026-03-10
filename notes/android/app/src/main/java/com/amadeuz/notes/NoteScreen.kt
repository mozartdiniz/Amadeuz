package com.amadeuz.notes

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import java.text.SimpleDateFormat
import java.util.*

// MARK: - Root

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotesApp(vm: NoteViewModel) {
    val folders by vm.folders.collectAsState()
    val allNotes by vm.notes.collectAsState()
    val selectedFolderID by vm.selectedFolderID.collectAsState()
    val selectedNoteID by vm.selectedNoteID.collectAsState()
    val isConnected by vm.isConnected.collectAsState()
    val editingTitle by vm.editingTitle.collectAsState()
    val editingContent by vm.editingContent.collectAsState()

    val notesInFolder = remember(allNotes, selectedFolderID) {
        vm.notesInFolder(selectedFolderID)
    }

    var showEditor by remember { mutableStateOf(false) }
    var showSettings by remember { mutableStateOf(false) }
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()

    // Open editor when a note is selected, close when deselected
    LaunchedEffect(selectedNoteID) {
        showEditor = selectedNoteID != null
    }

    if (showSettings) {
        SettingsDialog(
            currentAddress = vm.serverAddress.collectAsState().value,
            onSave = { vm.updateServerAddress(it) },
            onDismiss = { showSettings = false }
        )
    }

    if (showEditor) {
        NoteEditorScreen(
            title = editingTitle,
            content = editingContent,
            isConnected = isConnected,
            onTitleChange = { vm.onTitleChanged(it) },
            onContentChange = { vm.onContentChanged(it) },
            onBack = { vm.selectNote(null) }
        )
        return
    }

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            ModalDrawerSheet {
                FolderDrawer(
                    folders = folders,
                    selectedFolderID = selectedFolderID,
                    onSelectFolder = {
                        vm.selectFolder(it)
                        scope.launch { drawerState.close() }
                    },
                    onCreateFolder = { vm.createFolder(it) },
                    onRenameFolder = { id, name -> vm.renameFolder(id, name) },
                    onDeleteFolder = { vm.deleteFolder(it) }
                )
            }
        }
    ) {
        val folderName = when (selectedFolderID) {
            NoteViewModel.ALL_NOTES_ID -> "All Notes"
            null -> "Notes"
            else -> folders.firstOrNull { it.id == selectedFolderID }?.name ?: "Notes"
        }

        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(folderName) },
                    navigationIcon = {
                        IconButton(onClick = { scope.launch { drawerState.open() } }) {
                            Icon(Icons.Default.Menu, contentDescription = "Folders")
                        }
                    },
                    actions = {
                        // Connection indicator dot
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(
                                    color = if (isConnected) Color(0xFF34C759) else Color(0xFFFF3B30),
                                    shape = CircleShape
                                )
                        )
                        Spacer(Modifier.width(4.dp))
                        IconButton(onClick = { showSettings = true }) {
                            Icon(Icons.Default.Settings, contentDescription = "Settings")
                        }
                    }
                )
            },
            floatingActionButton = {
                val canCreate = selectedFolderID != null && selectedFolderID != NoteViewModel.ALL_NOTES_ID
                if (canCreate) {
                    FloatingActionButton(onClick = { vm.createNote() }) {
                        Icon(Icons.Default.Add, contentDescription = "New Note")
                    }
                }
            }
        ) { padding ->
            NoteList(
                notes = notesInFolder,
                selectedNoteID = selectedNoteID,
                onSelectNote = { vm.selectNote(it) },
                onDeleteNote = { vm.deleteNote(it) },
                modifier = Modifier.padding(padding)
            )
        }
    }
}

// MARK: - Folder drawer

@Composable
private fun FolderDrawer(
    folders: List<Folder>,
    selectedFolderID: String?,
    onSelectFolder: (String?) -> Unit,
    onCreateFolder: (String) -> Unit,
    onRenameFolder: (String, String) -> Unit,
    onDeleteFolder: (String) -> Unit
) {
    var showNewFolderDialog by remember { mutableStateOf(false) }
    var renamingFolder by remember { mutableStateOf<Folder?>(null) }
    var renameText by remember { mutableStateOf("") }

    Column(modifier = Modifier.fillMaxHeight()) {
        Spacer(Modifier.height(16.dp))
        Text(
            "Folders",
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)
        )

        NavigationDrawerItem(
            label = { Text("All Notes") },
            selected = selectedFolderID == NoteViewModel.ALL_NOTES_ID,
            onClick = { onSelectFolder(NoteViewModel.ALL_NOTES_ID) },
            modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding)
        )

        folders.forEach { folder ->
            var showMenu by remember { mutableStateOf(false) }
            NavigationDrawerItem(
                label = { Text(folder.name) },
                selected = selectedFolderID == folder.id,
                onClick = { onSelectFolder(folder.id) },
                badge = {
                    Box {
                        IconButton(
                            onClick = { showMenu = true },
                            modifier = Modifier.size(24.dp)
                        ) {
                            Icon(Icons.Default.MoreVert, contentDescription = "Options", modifier = Modifier.size(16.dp))
                        }
                        DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
                            DropdownMenuItem(
                                text = { Text("Rename") },
                                onClick = {
                                    showMenu = false
                                    renamingFolder = folder
                                    renameText = folder.name
                                }
                            )
                            DropdownMenuItem(
                                text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                                onClick = {
                                    showMenu = false
                                    onDeleteFolder(folder.id)
                                }
                            )
                        }
                    }
                },
                modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding)
            )
        }

        Spacer(Modifier.weight(1f))

        TextButton(
            onClick = { showNewFolderDialog = true },
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 8.dp)
        ) {
            Icon(Icons.Default.Add, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text("New Folder")
        }
    }

    // New folder dialog
    if (showNewFolderDialog) {
        var newName by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { showNewFolderDialog = false; newName = "" },
            title = { Text("New Folder") },
            text = {
                OutlinedTextField(
                    value = newName,
                    onValueChange = { newName = it },
                    label = { Text("Folder name") },
                    singleLine = true
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val n = newName.trim()
                    if (n.isNotEmpty()) onCreateFolder(n)
                    showNewFolderDialog = false
                    newName = ""
                }) { Text("Create") }
            },
            dismissButton = {
                TextButton(onClick = { showNewFolderDialog = false; newName = "" }) { Text("Cancel") }
            }
        )
    }

    // Rename folder dialog
    renamingFolder?.let { folder ->
        AlertDialog(
            onDismissRequest = { renamingFolder = null },
            title = { Text("Rename Folder") },
            text = {
                OutlinedTextField(
                    value = renameText,
                    onValueChange = { renameText = it },
                    label = { Text("Folder name") },
                    singleLine = true
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    val n = renameText.trim()
                    if (n.isNotEmpty()) onRenameFolder(folder.id, n)
                    renamingFolder = null
                }) { Text("Rename") }
            },
            dismissButton = {
                TextButton(onClick = { renamingFolder = null }) { Text("Cancel") }
            }
        )
    }
}

// MARK: - Note list

@Composable
private fun NoteList(
    notes: List<Note>,
    selectedNoteID: String?,
    onSelectNote: (String) -> Unit,
    onDeleteNote: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    if (notes.isEmpty()) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("No Notes", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }

    LazyColumn(modifier) {
        items(notes, key = { it.id }) { note ->
            NoteRow(
                note = note,
                onClick = { onSelectNote(note.id) },
                onDelete = { onDeleteNote(note.id) }
            )
            HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
        }
    }
}

@Composable
private fun NoteRow(note: Note, onClick: () -> Unit, onDelete: () -> Unit) {
    var showMenu by remember { mutableStateOf(false) }

    val displayTitle = note.title.trim().ifEmpty { "Untitled" }
    val preview = note.content.trim().ifEmpty { "No additional text" }
    val dateStr = remember(note.updated_at) { formatDate(note.updated_at) }

    Box {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.Top
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        displayTitle,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f)
                    )
                    Text(
                        dateStr,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
                Spacer(Modifier.height(2.dp))
                Text(
                    preview,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis
                )
            }
            IconButton(onClick = { showMenu = true }, modifier = Modifier.size(32.dp)) {
                Icon(Icons.Default.MoreVert, contentDescription = "More", modifier = Modifier.size(16.dp))
            }
        }
        DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
            DropdownMenuItem(
                text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                onClick = { showMenu = false; onDelete() }
            )
        }
    }
}

private fun formatDate(updatedAt: Long): String {
    val date = Date(updatedAt)
    val now = Calendar.getInstance()
    val cal = Calendar.getInstance().also { it.time = date }
    return when {
        now.get(Calendar.DATE) == cal.get(Calendar.DATE) &&
                now.get(Calendar.YEAR) == cal.get(Calendar.YEAR) ->
            SimpleDateFormat("HH:mm", Locale.getDefault()).format(date)
        now.get(Calendar.DATE) - cal.get(Calendar.DATE) == 1 &&
                now.get(Calendar.YEAR) == cal.get(Calendar.YEAR) ->
            "Yesterday"
        else ->
            SimpleDateFormat("MMM d, yyyy", Locale.getDefault()).format(date)
    }
}

// MARK: - Note editor

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NoteEditorScreen(
    title: String,
    content: String,
    isConnected: Boolean,
    onTitleChange: (String) -> Unit,
    onContentChange: (String) -> Unit,
    onBack: () -> Unit
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    Box(
                        modifier = Modifier
                            .size(10.dp)
                            .background(
                                color = if (isConnected) Color(0xFF34C759) else Color(0xFFFF3B30),
                                shape = CircleShape
                            )
                    )
                    Spacer(Modifier.width(12.dp))
                }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
        ) {
            TextField(
                value = title,
                onValueChange = onTitleChange,
                placeholder = { Text("Title", style = MaterialTheme.typography.titleLarge) },
                textStyle = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent
                ),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 4.dp)
            )

            HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))

            TextField(
                value = content,
                onValueChange = onContentChange,
                placeholder = { Text("Start writing…") },
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = Color.Transparent,
                    unfocusedContainerColor = Color.Transparent,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent
                ),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 4.dp)
            )
        }
    }
}

// MARK: - Settings dialog

@Composable
fun SettingsDialog(
    currentAddress: String,
    onSave: (String) -> Unit,
    onDismiss: () -> Unit
) {
    var draft by remember(currentAddress) { mutableStateOf(currentAddress) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Server Settings") },
        text = {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                label = { Text("WebSocket URL") },
                placeholder = { Text("ws://192.168.x.x:8080/ws") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth()
            )
        },
        confirmButton = {
            TextButton(onClick = { onSave(draft.trim()); onDismiss() }) { Text("Connect") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        }
    )
}
