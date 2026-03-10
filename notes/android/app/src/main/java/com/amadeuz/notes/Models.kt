package com.amadeuz.notes

data class Folder(
    val id: String = "",
    val name: String = "",
    val created_at: Long = 0L
)

data class Note(
    val id: String = "",
    val folder_id: String = "",
    val title: String = "",
    val content: String = "",
    val updated_at: Long = 0L,
    val created_at: Long = 0L
)

// Generic WebSocket message. Null fields are omitted by Gson (default behaviour).
data class WSMsg(
    val type: String = "",
    val folders: List<Folder>? = null,
    val notes: List<Note>? = null,
    val folder: Folder? = null,
    val note: Note? = null,
    val folder_id: String? = null,
    val note_id: String? = null,
    val name: String? = null,
    val title: String? = null,
    val content: String? = null,
    val updated_at: Long? = null
)
