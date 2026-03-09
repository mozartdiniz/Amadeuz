package main

// Folder is a named group of notes.
type Folder struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	CreatedAt int64  `json:"created_at"`
}

// Note is a single note document.
type Note struct {
	ID        string `json:"id"`
	FolderID  string `json:"folder_id"`
	Title     string `json:"title"`
	Content   string `json:"content"`
	UpdatedAt int64  `json:"updated_at"`
	CreatedAt int64  `json:"created_at"`
}

// Msg is the wire format for all WebSocket messages.
// Fields irrelevant to a given message type are omitted from JSON.
//
// Client → Server message types:
//
//	create_folder  { name }
//	rename_folder  { folder_id, name }
//	delete_folder  { folder_id }
//	create_note    { folder_id, title }
//	update_note    { note_id, title, content, updated_at }
//	delete_note    { note_id }
//
// Server → Client message types:
//
//	init            { folders, notes }          — sent to every new client on connect
//	folder_created  { folder }                  — broadcast after create_folder
//	folder_renamed  { folder }                  — broadcast after rename_folder
//	folder_deleted  { folder_id }               — broadcast after delete_folder
//	note_created    { note }                    — sent to creator + broadcast
//	note_updated    { note }                    — broadcast after accepted update_note
//	note_deleted    { note_id }                 — broadcast after delete_note
type Msg struct {
	Type string `json:"type"`

	// Collections (init)
	Folders []Folder `json:"folders,omitempty"`
	Notes   []Note   `json:"notes,omitempty"`

	// Single entity (create/update responses)
	Folder *Folder `json:"folder,omitempty"`
	Note   *Note   `json:"note,omitempty"`

	// Scalar params (client requests + delete broadcasts)
	FolderID  string `json:"folder_id,omitempty"`
	NoteID    string `json:"note_id,omitempty"`
	Name      string `json:"name,omitempty"`
	Title     string `json:"title,omitempty"`
	Content   string `json:"content,omitempty"`
	UpdatedAt int64  `json:"updated_at,omitempty"`
}
