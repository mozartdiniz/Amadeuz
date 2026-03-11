package notes

import (
	"log"
	"net/http"
	"sync"

	"github.com/gorilla/websocket"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

// wsMsg is the wire format for per-note WebSocket messages.
//
// Server → client on connect:
//
//	{ "type": "init", "title": "...", "content": "...", "updated_at": 123 }
//
// Client → server on user edit:
//
//	{ "type": "update", "title": "...", "content": "...", "updated_at": 123 }
//
// Server → all other clients after accepted update:
//
//	{ "type": "update", "title": "...", "content": "...", "updated_at": 123 }
type wsMsg struct {
	Type      string `json:"type"`
	Title     string `json:"title,omitempty"`
	Content   string `json:"content,omitempty"`
	UpdatedAt int64  `json:"updated_at,omitempty"`
}

// conn wraps a WebSocket connection with a write mutex.
// gorilla/websocket connections are not safe for concurrent writes;
// the mutex serialises all writes to a given connection.
type conn struct {
	mu sync.Mutex
	*websocket.Conn
}

func (c *conn) writeJSON(v any) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.Conn.WriteJSON(v)
}

// hub manages per-note WebSocket rooms.
// Each note ID maps to the set of connections currently watching it.
type hub struct {
	mu    sync.Mutex
	rooms map[string]map[*conn]bool
}

func newHub() *hub {
	return &hub{rooms: make(map[string]map[*conn]bool)}
}

func (h *hub) add(noteID string, c *conn) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.rooms[noteID] == nil {
		h.rooms[noteID] = make(map[*conn]bool)
	}
	h.rooms[noteID][c] = true
}

func (h *hub) remove(noteID string, c *conn) {
	h.mu.Lock()
	defer h.mu.Unlock()
	delete(h.rooms[noteID], c)
	if len(h.rooms[noteID]) == 0 {
		delete(h.rooms, noteID)
	}
}

func (h *hub) broadcastExcept(noteID string, except *conn, msg wsMsg) {
	h.mu.Lock()
	conns := make([]*conn, 0, len(h.rooms[noteID]))
	for c := range h.rooms[noteID] {
		if c != except {
			conns = append(conns, c)
		}
	}
	h.mu.Unlock()

	// Write outside the hub lock so a slow client cannot block other broadcasts.
	for _, c := range conns {
		if err := c.writeJSON(msg); err != nil {
			log.Println("ws broadcast error:", err)
		}
	}
}
