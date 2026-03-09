package main

import (
	"log"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

// hub manages all connected WebSocket clients and routes messages.
type hub struct {
	mu      sync.Mutex
	clients map[*websocket.Conn]bool
	store   *store
}

func newHub(s *store) *hub {
	return &hub{
		clients: make(map[*websocket.Conn]bool),
		store:   s,
	}
}

func (h *hub) add(c *websocket.Conn) {
	h.mu.Lock()
	h.clients[c] = true
	h.mu.Unlock()
}

func (h *hub) remove(c *websocket.Conn) {
	h.mu.Lock()
	delete(h.clients, c)
	h.mu.Unlock()
}

// send sends msg to a single connection, logging errors.
func send(c *websocket.Conn, msg Msg) {
	if err := c.WriteJSON(msg); err != nil {
		log.Println("send error:", err)
	}
}

// broadcast sends msg to every client except sender.
// Pass nil as sender to broadcast to everyone.
func (h *hub) broadcast(msg Msg, sender *websocket.Conn) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for c := range h.clients {
		if c == sender {
			continue
		}
		if err := c.WriteJSON(msg); err != nil {
			log.Println("broadcast error:", err)
		}
	}
}

// sendInit sends the full current state to a newly connected client.
func (h *hub) sendInit(conn *websocket.Conn) {
	msg := Msg{
		Type:    "init",
		Folders: h.store.GetFolders(),
		Notes:   h.store.GetNotes(),
	}
	send(conn, msg)
}

// handle processes one inbound message from a client.
func (h *hub) handle(conn *websocket.Conn, msg Msg) {
	now := time.Now().UnixMilli()

	switch msg.Type {

	case "create_folder":
		if msg.Name == "" {
			return
		}
		f := h.store.CreateFolder(msg.Name, now)
		resp := Msg{Type: "folder_created", Folder: &f}
		log.Printf("folder created: %q (%s)", f.Name, f.ID)
		send(conn, resp)
		h.broadcast(resp, conn)

	case "rename_folder":
		if msg.FolderID == "" || msg.Name == "" {
			return
		}
		f, ok := h.store.RenameFolder(msg.FolderID, msg.Name)
		if !ok {
			return
		}
		resp := Msg{Type: "folder_renamed", Folder: &f}
		log.Printf("folder renamed: %q (%s)", f.Name, f.ID)
		send(conn, resp)
		h.broadcast(resp, conn)

	case "delete_folder":
		if msg.FolderID == "" {
			return
		}
		if !h.store.DeleteFolder(msg.FolderID) {
			return
		}
		resp := Msg{Type: "folder_deleted", FolderID: msg.FolderID}
		log.Printf("folder deleted: %s", msg.FolderID)
		send(conn, resp)
		h.broadcast(resp, conn)

	case "create_note":
		if msg.FolderID == "" {
			return
		}
		n, ok := h.store.CreateNote(msg.FolderID, msg.Title, now)
		if !ok {
			return
		}
		resp := Msg{Type: "note_created", Note: &n}
		log.Printf("note created: %q (%s) in folder %s", n.Title, n.ID, n.FolderID)
		send(conn, resp)
		h.broadcast(resp, conn)

	case "update_note":
		if msg.NoteID == "" || msg.UpdatedAt == 0 {
			return
		}
		n, ok := h.store.UpdateNote(msg.NoteID, msg.Title, msg.Content, msg.UpdatedAt)
		if !ok {
			return // rejected (stale timestamp or note not found)
		}
		resp := Msg{Type: "note_updated", Note: &n}
		log.Printf("note updated: %s (ts=%d, len=%d)", n.ID, n.UpdatedAt, len(n.Content))
		h.broadcast(resp, conn)

	case "delete_note":
		if msg.NoteID == "" {
			return
		}
		if !h.store.DeleteNote(msg.NoteID) {
			return
		}
		resp := Msg{Type: "note_deleted", NoteID: msg.NoteID}
		log.Printf("note deleted: %s", msg.NoteID)
		send(conn, resp)
		h.broadcast(resp, conn)
	}
}
