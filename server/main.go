package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"sync"

	"github.com/gorilla/websocket"
)

// Message is the wire format shared between server and clients.
type Message struct {
	Type      string `json:"type"`
	Content   string `json:"content"`
	UpdatedAt int64  `json:"updated_at"` // Unix milliseconds
}

// store holds the latest note and persists it to disk.
type store struct {
	mu        sync.RWMutex
	content   string
	updatedAt int64
	path      string
}

func newStore(path string) *store {
	s := &store{path: path}
	s.load()
	return s
}

func (s *store) load() {
	data, err := os.ReadFile(s.path)
	if err != nil {
		return
	}
	var m Message
	if err := json.Unmarshal(data, &m); err == nil {
		s.content = m.Content
		s.updatedAt = m.UpdatedAt
	}
}

func (s *store) get() Message {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return Message{Type: "init", Content: s.content, UpdatedAt: s.updatedAt}
}

// update stores new content only if updatedAt is strictly newer.
// Returns true if the store was updated.
func (s *store) update(content string, updatedAt int64) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if updatedAt <= s.updatedAt {
		return false
	}
	s.content = content
	s.updatedAt = updatedAt
	go s.persist()
	return true
}

func (s *store) persist() {
	s.mu.RLock()
	defer s.mu.RUnlock()
	data, _ := json.Marshal(Message{Content: s.content, UpdatedAt: s.updatedAt})
	if err := os.WriteFile(s.path, data, 0644); err != nil {
		log.Println("persist error:", err)
	}
}

// hub manages connected WebSocket clients.
type hub struct {
	mu      sync.Mutex
	clients map[*websocket.Conn]bool
}

func newHub() *hub {
	return &hub{clients: make(map[*websocket.Conn]bool)}
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

// broadcast sends msg to every client except the sender.
func (h *hub) broadcast(msg Message, sender *websocket.Conn) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for c := range h.clients {
		if c == sender {
			continue
		}
		if err := c.WriteJSON(msg); err != nil {
			log.Println("broadcast write error:", err)
		}
	}
}

var upgrader = websocket.Upgrader{
	// Allow any origin since clients are native apps, not browsers.
	CheckOrigin: func(r *http.Request) bool { return true },
}

func main() {
	s := newStore("note.json")
	h := newHub()

	http.HandleFunc("/ws", func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			log.Println("upgrade error:", err)
			return
		}
		defer func() {
			h.remove(conn)
			conn.Close()
			log.Println("client disconnected, total:", len(h.clients))
		}()

		h.add(conn)
		log.Println("client connected, total:", len(h.clients))

		// Send current note state to the new client.
		if err := conn.WriteJSON(s.get()); err != nil {
			return
		}

		for {
			var msg Message
			if err := conn.ReadJSON(&msg); err != nil {
				break
			}
			if msg.Type == "update" && s.update(msg.Content, msg.UpdatedAt) {
				log.Printf("note updated (ts=%d, len=%d)", msg.UpdatedAt, len(msg.Content))
				h.broadcast(msg, conn)
			}
		}
	})

	addr := ":8080"
	log.Printf("amadeuz server listening on %s", addr)
	log.Fatal(http.ListenAndServe(addr, nil))
}
