package main

import (
	"crypto/rand"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
)

// store holds all folders and notes in memory and persists them to disk.
type store struct {
	mu      sync.RWMutex
	folders map[string]*Folder
	notes   map[string]*Note
	path    string
}

// diskData is the on-disk JSON structure.
type diskData struct {
	Folders []*Folder `json:"folders"`
	Notes   []*Note   `json:"notes"`
}

// newID returns a random UUID v4-like identifier.
func newID() string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		panic(fmt.Sprintf("newID: %v", err))
	}
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

func newStore(path string) *store {
	s := &store{
		folders: make(map[string]*Folder),
		notes:   make(map[string]*Note),
		path:    path,
	}
	s.load()
	return s
}

func (s *store) load() {
	data, err := os.ReadFile(s.path)
	if err != nil {
		return // file not found is fine on first run
	}
	var d diskData
	if err := json.Unmarshal(data, &d); err != nil {
		log.Println("store: load error:", err)
		return
	}
	for _, f := range d.Folders {
		s.folders[f.ID] = f
	}
	for _, n := range d.Notes {
		s.notes[n.ID] = n
	}
}

// snapshot returns a copy of all data, safe to use outside the lock.
func (s *store) snapshot() diskData {
	s.mu.RLock()
	defer s.mu.RUnlock()
	d := diskData{
		Folders: make([]*Folder, 0, len(s.folders)),
		Notes:   make([]*Note, 0, len(s.notes)),
	}
	for _, f := range s.folders {
		cp := *f
		d.Folders = append(d.Folders, &cp)
	}
	for _, n := range s.notes {
		cp := *n
		d.Notes = append(d.Notes, &cp)
	}
	return d
}

// persist writes current state to disk atomically (write to temp file, then rename).
// This prevents partial writes from corrupting the data file.
// Called synchronously after every mutation — fast enough for a notes app.
func (s *store) persist() {
	d := s.snapshot()
	data, err := json.Marshal(d)
	if err != nil {
		log.Println("store: persist: marshal error:", err)
		return
	}

	dir := filepath.Dir(s.path)
	tmp, err := os.CreateTemp(dir, ".data-*.json.tmp")
	if err != nil {
		log.Println("store: persist: create temp:", err)
		return
	}
	tmpPath := tmp.Name()

	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		os.Remove(tmpPath)
		log.Println("store: persist: write:", err)
		return
	}
	tmp.Close()

	if err := os.Rename(tmpPath, s.path); err != nil {
		os.Remove(tmpPath)
		log.Println("store: persist: rename:", err)
	}
}

// --- Folder operations ---

// GetFolders returns all folders.
func (s *store) GetFolders() []Folder {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]Folder, 0, len(s.folders))
	for _, f := range s.folders {
		result = append(result, *f)
	}
	return result
}

// CreateFolder adds a new folder and returns it.
func (s *store) CreateFolder(name string, now int64) Folder {
	f := &Folder{ID: newID(), Name: name, CreatedAt: now}
	s.mu.Lock()
	s.folders[f.ID] = f
	s.mu.Unlock()
	s.persist()
	return *f
}

// RenameFolder changes a folder's name.
// Returns the updated folder and true, or zero value and false if not found.
func (s *store) RenameFolder(id, name string) (Folder, bool) {
	s.mu.Lock()
	f, ok := s.folders[id]
	if !ok {
		s.mu.Unlock()
		return Folder{}, false
	}
	f.Name = name
	cp := *f
	s.mu.Unlock()
	s.persist()
	return cp, true
}

// DeleteFolder removes a folder and all its notes (cascade).
// Returns true if a folder was deleted.
func (s *store) DeleteFolder(id string) bool {
	s.mu.Lock()
	if _, ok := s.folders[id]; !ok {
		s.mu.Unlock()
		return false
	}
	delete(s.folders, id)
	for nid, n := range s.notes {
		if n.FolderID == id {
			delete(s.notes, nid)
		}
	}
	s.mu.Unlock()
	s.persist()
	return true
}

// --- Note operations ---

// GetNotes returns all notes across all folders.
func (s *store) GetNotes() []Note {
	s.mu.RLock()
	defer s.mu.RUnlock()
	result := make([]Note, 0, len(s.notes))
	for _, n := range s.notes {
		result = append(result, *n)
	}
	return result
}

// CreateNote adds a new empty note. folderID may be empty for an unfoldered note.
func (s *store) CreateNote(folderID, title string, now int64) Note {
	n := &Note{
		ID:        newID(),
		FolderID:  folderID,
		Title:     title,
		Content:   "",
		UpdatedAt: now,
		CreatedAt: now,
	}
	s.mu.Lock()
	s.notes[n.ID] = n
	s.mu.Unlock()
	s.persist()
	return *n
}

// MoveNote changes the folder of an existing note.
// Returns the updated note and true, or zero value and false if the note doesn't exist.
func (s *store) MoveNote(id, folderID string, now int64) (Note, bool) {
	s.mu.Lock()
	n, ok := s.notes[id]
	if !ok {
		s.mu.Unlock()
		return Note{}, false
	}
	n.FolderID = folderID
	n.UpdatedAt = now
	cp := *n
	s.mu.Unlock()
	s.persist()
	return cp, true
}

// UpdateNote updates title and content of an existing note using last-write-wins.
// Returns the updated note and true only if the note exists and updatedAt is strictly newer.
func (s *store) UpdateNote(id, title, content string, updatedAt int64) (Note, bool) {
	s.mu.Lock()
	n, ok := s.notes[id]
	if !ok {
		s.mu.Unlock()
		return Note{}, false
	}
	if updatedAt <= n.UpdatedAt {
		s.mu.Unlock()
		return Note{}, false
	}
	n.Title = title
	n.Content = content
	n.UpdatedAt = updatedAt
	cp := *n
	s.mu.Unlock()
	s.persist()
	return cp, true
}

// DeleteNote removes a note by ID.
// Returns true if a note was deleted.
func (s *store) DeleteNote(id string) bool {
	s.mu.Lock()
	if _, ok := s.notes[id]; !ok {
		s.mu.Unlock()
		return false
	}
	delete(s.notes, id)
	s.mu.Unlock()
	s.persist()
	return true
}
