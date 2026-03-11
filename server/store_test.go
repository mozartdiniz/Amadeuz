package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// tempStore creates a store backed by a temp file that is cleaned up after the test.
func tempStore(t *testing.T) *store {
	t.Helper()
	path := filepath.Join(t.TempDir(), "data.json")
	return newStore(path)
}

// --- Folder tests ---

func TestCreateFolder(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)

	if f.ID == "" {
		t.Fatal("expected non-empty ID")
	}
	if f.Name != "Work" {
		t.Fatalf("name: got %q, want %q", f.Name, "Work")
	}
	if f.CreatedAt != 1000 {
		t.Fatalf("created_at: got %d, want 1000", f.CreatedAt)
	}

	folders := s.GetFolders()
	if len(folders) != 1 {
		t.Fatalf("GetFolders: got %d folders, want 1", len(folders))
	}
}

func TestCreateFolderWithClientID(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("client-generated-id", "Work", 1000)

	if f.ID != "client-generated-id" {
		t.Fatalf("expected client ID to be preserved, got %q", f.ID)
	}
}

func TestCreateMultipleFolders(t *testing.T) {
	s := tempStore(t)
	s.CreateFolder("", "Work", 1000)
	s.CreateFolder("", "Personal", 2000)
	s.CreateFolder("", "Archive", 3000)

	folders := s.GetFolders()
	if len(folders) != 3 {
		t.Fatalf("got %d folders, want 3", len(folders))
	}
}

func TestRenameFolder(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)

	updated, ok := s.RenameFolder(f.ID, "Work Projects")
	if !ok {
		t.Fatal("RenameFolder returned false for existing folder")
	}
	if updated.Name != "Work Projects" {
		t.Fatalf("name: got %q, want %q", updated.Name, "Work Projects")
	}
	if updated.ID != f.ID {
		t.Fatal("ID changed after rename")
	}

	// Verify via GetFolders
	folders := s.GetFolders()
	if folders[0].Name != "Work Projects" {
		t.Fatalf("GetFolders: name not persisted in memory")
	}
}

func TestRenameFolderNotFound(t *testing.T) {
	s := tempStore(t)
	_, ok := s.RenameFolder("nonexistent-id", "whatever")
	if ok {
		t.Fatal("expected false for nonexistent folder")
	}
}

func TestDeleteFolder(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Temp", 1000)

	ok := s.DeleteFolder(f.ID)
	if !ok {
		t.Fatal("DeleteFolder returned false for existing folder")
	}

	folders := s.GetFolders()
	if len(folders) != 0 {
		t.Fatalf("expected 0 folders after delete, got %d", len(folders))
	}
}

func TestDeleteFolderNotFound(t *testing.T) {
	s := tempStore(t)
	ok := s.DeleteFolder("nonexistent-id")
	if ok {
		t.Fatal("expected false for nonexistent folder")
	}
}

func TestDeleteFolderCascadesNotes(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)
	s.CreateNote("", f.ID, "Note 1", "", 0, 2000)
	s.CreateNote("", f.ID, "Note 2", "", 0, 3000)

	// Create a second folder with its own note — should survive the delete.
	f2 := s.CreateFolder("", "Personal", 1000)
	s.CreateNote("", f2.ID, "Personal Note", "", 0, 4000)

	s.DeleteFolder(f.ID)

	notes := s.GetNotes()
	if len(notes) != 1 {
		t.Fatalf("expected 1 note after cascade delete, got %d", len(notes))
	}
	if notes[0].FolderID != f2.ID {
		t.Fatalf("surviving note belongs to wrong folder")
	}
}

// --- Note tests ---

func TestCreateNote(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)

	n := s.CreateNote("", f.ID, "My Note", "", 0, 2000)
	if n.ID == "" {
		t.Fatal("expected non-empty ID")
	}
	if n.Title != "My Note" {
		t.Fatalf("title: got %q, want %q", n.Title, "My Note")
	}
	if n.FolderID != f.ID {
		t.Fatalf("folder_id mismatch")
	}
	if n.Content != "" {
		t.Fatalf("new note should have empty content, got %q", n.Content)
	}
	if n.CreatedAt != 2000 || n.UpdatedAt != 2000 {
		t.Fatalf("timestamps: got created=%d updated=%d, want both 2000", n.CreatedAt, n.UpdatedAt)
	}
}

func TestCreateNoteWithClientID(t *testing.T) {
	s := tempStore(t)
	n := s.CreateNote("client-note-id", "", "Offline note", "Written offline", 5000, 6000)

	if n.ID != "client-note-id" {
		t.Fatalf("expected client ID to be preserved, got %q", n.ID)
	}
	if n.Content != "Written offline" {
		t.Fatalf("content: got %q, want %q", n.Content, "Written offline")
	}
	if n.UpdatedAt != 5000 {
		t.Fatalf("updated_at: got %d, want 5000 (client timestamp)", n.UpdatedAt)
	}
}

func TestCreateNoteWithoutFolder(t *testing.T) {
	s := tempStore(t)
	n := s.CreateNote("", "", "Quick thought", "", 0, 1000)

	if n.ID == "" {
		t.Fatal("expected non-empty ID")
	}
	if n.FolderID != "" {
		t.Fatalf("expected empty folder_id, got %q", n.FolderID)
	}

	notes := s.GetNotes()
	if len(notes) != 1 {
		t.Fatalf("expected 1 note, got %d", len(notes))
	}
}

func TestUpdateNote(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)
	n := s.CreateNote("", f.ID, "Draft", "", 0, 2000)

	updated, ok := s.UpdateNote(n.ID, "Final Title", "Hello, world!", 3000)
	if !ok {
		t.Fatal("UpdateNote returned false for valid update")
	}
	if updated.Title != "Final Title" {
		t.Fatalf("title: got %q, want %q", updated.Title, "Final Title")
	}
	if updated.Content != "Hello, world!" {
		t.Fatalf("content: got %q, want %q", updated.Content, "Hello, world!")
	}
	if updated.UpdatedAt != 3000 {
		t.Fatalf("updated_at: got %d, want 3000", updated.UpdatedAt)
	}
	if updated.CreatedAt != 2000 {
		t.Fatalf("created_at should not change: got %d, want 2000", updated.CreatedAt)
	}
}

func TestUpdateNoteRejectsStalerTimestamp(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)
	n := s.CreateNote("", f.ID, "Note", "", 0, 5000)

	// Attempt to update with an older timestamp — should be rejected.
	_, ok := s.UpdateNote(n.ID, "Overwrite", "Should not stick", 4000)
	if ok {
		t.Fatal("expected update to be rejected (stale timestamp)")
	}

	// Verify content unchanged via a fresh update with a newer timestamp.
	// We update with ts=5001 to confirm the content is still empty.
	fresh, ok := s.UpdateNote(n.ID, "Note", "still empty check", 5001)
	if !ok {
		t.Fatal("fresh update should succeed")
	}
	if fresh.Content != "still empty check" {
		t.Fatal("content mismatch after rejected stale update")
	}
}

func TestUpdateNoteRejectsSameTimestamp(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)
	n := s.CreateNote("", f.ID, "Note", "", 0, 2000)

	// First update succeeds.
	s.UpdateNote(n.ID, "Title", "Content", 3000)

	// Same timestamp should be rejected (strictly greater required).
	_, ok := s.UpdateNote(n.ID, "Title", "Different content", 3000)
	if ok {
		t.Fatal("expected same-timestamp update to be rejected")
	}
}

func TestUpdateNoteNotFound(t *testing.T) {
	s := tempStore(t)
	_, ok := s.UpdateNote("nonexistent-id", "Title", "Content", 1000)
	if ok {
		t.Fatal("expected false for nonexistent note")
	}
}

func TestDeleteNote(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)
	n := s.CreateNote("", f.ID, "Note", "", 0, 2000)

	ok := s.DeleteNote(n.ID)
	if !ok {
		t.Fatal("DeleteNote returned false for existing note")
	}

	notes := s.GetNotes()
	if len(notes) != 0 {
		t.Fatalf("expected 0 notes after delete, got %d", len(notes))
	}
}

func TestDeleteNoteNotFound(t *testing.T) {
	s := tempStore(t)
	ok := s.DeleteNote("nonexistent-id")
	if ok {
		t.Fatal("expected false for nonexistent note")
	}
}

func TestGetNotesAcrossFolders(t *testing.T) {
	s := tempStore(t)
	f1 := s.CreateFolder("", "Work", 1000)
	f2 := s.CreateFolder("", "Personal", 1000)

	s.CreateNote("", f1.ID, "Work Note 1", "", 0, 2000)
	s.CreateNote("", f1.ID, "Work Note 2", "", 0, 3000)
	s.CreateNote("", f2.ID, "Personal Note", "", 0, 4000)

	notes := s.GetNotes()
	if len(notes) != 3 {
		t.Fatalf("expected 3 notes, got %d", len(notes))
	}
}

// --- Persistence tests ---

func TestPersistAndReload(t *testing.T) {
	path := filepath.Join(t.TempDir(), "data.json")

	// Build initial state.
	s1 := newStore(path)
	f := s1.CreateFolder("", "Work", 1000)
	n := s1.CreateNote("", f.ID, "Meeting notes", "", 0, 2000)
	s1.UpdateNote(n.ID, "Meeting notes", "Action items: ...", 3000)

	// Force synchronous persist to ensure file is written before reload.
	s1.persist()

	// Reload from same file.
	s2 := newStore(path)

	folders := s2.GetFolders()
	if len(folders) != 1 {
		t.Fatalf("reloaded: expected 1 folder, got %d", len(folders))
	}
	if folders[0].Name != "Work" {
		t.Fatalf("reloaded: folder name %q, want %q", folders[0].Name, "Work")
	}

	notes := s2.GetNotes()
	if len(notes) != 1 {
		t.Fatalf("reloaded: expected 1 note, got %d", len(notes))
	}
	if notes[0].Title != "Meeting notes" {
		t.Fatalf("reloaded: note title %q", notes[0].Title)
	}
	if notes[0].Content != "Action items: ..." {
		t.Fatalf("reloaded: note content %q", notes[0].Content)
	}
	if notes[0].UpdatedAt != 3000 {
		t.Fatalf("reloaded: updated_at %d, want 3000", notes[0].UpdatedAt)
	}
}

func TestPersistFileFormat(t *testing.T) {
	path := filepath.Join(t.TempDir(), "data.json")
	s := newStore(path)

	f := s.CreateFolder("", "Books", 1000)
	s.CreateNote("", f.ID, "Dune", "", 0, 2000)
	s.persist()

	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var d diskData
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatalf("invalid JSON on disk: %v", err)
	}
	if len(d.Folders) != 1 {
		t.Fatalf("disk: expected 1 folder, got %d", len(d.Folders))
	}
	if len(d.Notes) != 1 {
		t.Fatalf("disk: expected 1 note, got %d", len(d.Notes))
	}
}

func TestEmptyStoreOnNewFile(t *testing.T) {
	s := tempStore(t)
	if len(s.GetFolders()) != 0 {
		t.Fatal("new store should have no folders")
	}
	if len(s.GetNotes()) != 0 {
		t.Fatal("new store should have no notes")
	}
}

func TestUniqueIDs(t *testing.T) {
	s := tempStore(t)
	f := s.CreateFolder("", "Work", 1000)

	n1 := s.CreateNote("", f.ID, "Note 1", "", 0, 2000)
	n2 := s.CreateNote("", f.ID, "Note 2", "", 0, 3000)

	if n1.ID == n2.ID {
		t.Fatal("two notes should not share the same ID")
	}
	if n1.ID == f.ID {
		t.Fatal("note ID should not equal folder ID")
	}
}
