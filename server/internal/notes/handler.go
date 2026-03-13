package notes

import (
	"database/sql"
	"encoding/json"
	"log"
	"net/http"
	"time"

	"amadeuz/server/internal/auth"
	"amadeuz/server/internal/db"

	"github.com/go-chi/chi/v5"
	"github.com/gorilla/websocket"
)

const maxNoteBodyBytes = 10 << 20 // 10 MB

type Note struct {
	ID        string  `json:"id"`
	FolderID  string  `json:"folder_id"`
	Title     string  `json:"title"`
	Content   string  `json:"content"`
	UpdatedAt int64   `json:"updated_at"`
	CreatedAt int64   `json:"created_at"`
	DeletedAt *int64  `json:"deleted_at,omitempty"`
}

type Handler struct {
	db  *db.DB
	hub *hub
}

func NewHandler(database *db.DB) *Handler {
	return &Handler{db: database, hub: newHub()}
}

// List handles GET /notes
// Returns all notes for the authenticated user, including soft-deleted ones.
// Clients use the deleted_at field to place notes in the wastebasket.
func (h *Handler) List(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	rows, err := h.db.Query(
		`SELECT id, COALESCE(folder_id, ''), title, content, updated_at, created_at, deleted_at
		 FROM notes WHERE user_id = ? ORDER BY updated_at DESC`,
		userID,
	)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	defer rows.Close()

	notes := []Note{}
	for rows.Next() {
		var n Note
		if err := rows.Scan(&n.ID, &n.FolderID, &n.Title, &n.Content, &n.UpdatedAt, &n.CreatedAt, &n.DeletedAt); err != nil {
			http.Error(w, "db error", http.StatusInternalServerError)
			return
		}
		notes = append(notes, n)
	}
	if err := rows.Err(); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"notes": notes})
}

// Create handles POST /notes
// Body: { "id": "...", "folder_id": "...", "title": "...", "content": "...", "updated_at": 123, "created_at": 123 }
// All fields except title are optional. id and timestamps can be client-provided for offline sync.
func (h *Handler) Create(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, maxNoteBodyBytes)
	userID := auth.UserID(r)
	var body struct {
		ID        string `json:"id"`
		FolderID  string `json:"folder_id"`
		Title     string `json:"title"`
		Content   string `json:"content"`
		UpdatedAt int64  `json:"updated_at"`
		CreatedAt int64  `json:"created_at"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "invalid request", http.StatusBadRequest)
		return
	}

	id := body.ID
	if id == "" {
		id = db.NewID()
	}
	now := time.Now().UnixMilli()
	if body.UpdatedAt == 0 {
		body.UpdatedAt = now
	}
	if body.CreatedAt == 0 {
		body.CreatedAt = now
	}

	var folderID sql.NullString
	if body.FolderID != "" {
		folderID = sql.NullString{String: body.FolderID, Valid: true}
	}

	if _, err := h.db.Exec(
		`INSERT INTO notes (id, user_id, folder_id, title, content, updated_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)`,
		id, userID, folderID, body.Title, body.Content, body.UpdatedAt, body.CreatedAt,
	); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	n := Note{
		ID:        id,
		FolderID:  body.FolderID,
		Title:     body.Title,
		Content:   body.Content,
		UpdatedAt: body.UpdatedAt,
		CreatedAt: body.CreatedAt,
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]any{"note": n})
}

// Update handles PATCH /notes/:id
// Body: { "title": "...", "content": "...", "updated_at": 123 }
// Returns 409 if updated_at is not strictly greater than the stored value (last-write-wins).
func (h *Handler) Update(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, maxNoteBodyBytes)
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")
	var body struct {
		Title     string `json:"title"`
		Content   string `json:"content"`
		UpdatedAt int64  `json:"updated_at"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.UpdatedAt == 0 {
		http.Error(w, "updated_at required", http.StatusBadRequest)
		return
	}

	var current int64
	if err := h.db.QueryRow(
		`SELECT updated_at FROM notes WHERE id = ? AND user_id = ?`, id, userID,
	).Scan(&current); err != nil {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}
	if body.UpdatedAt <= current {
		http.Error(w, "stale update", http.StatusConflict)
		return
	}

	if _, err := h.db.Exec(
		`UPDATE notes SET title = ?, content = ?, updated_at = ? WHERE id = ? AND user_id = ?`,
		body.Title, body.Content, body.UpdatedAt, id, userID,
	); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	// Broadcast to any other clients watching this note via WebSocket.
	h.hub.broadcastExcept(id, nil, wsMsg{Type: "update", Title: body.Title, Content: body.Content, UpdatedAt: body.UpdatedAt})

	var n Note
	if err := h.db.QueryRow(
		`SELECT id, COALESCE(folder_id, ''), title, content, updated_at, created_at, deleted_at FROM notes WHERE id = ?`, id,
	).Scan(&n.ID, &n.FolderID, &n.Title, &n.Content, &n.UpdatedAt, &n.CreatedAt, &n.DeletedAt); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"note": n})
}

// Move handles PATCH /notes/:id/move
// Body: { "folder_id": "..." }  (empty string = remove from all folders)
func (h *Handler) Move(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")
	var body struct {
		FolderID string `json:"folder_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "invalid request", http.StatusBadRequest)
		return
	}

	var folderID sql.NullString
	if body.FolderID != "" {
		folderID = sql.NullString{String: body.FolderID, Valid: true}
	}

	res, err := h.db.Exec(
		`UPDATE notes SET folder_id = ? WHERE id = ? AND user_id = ?`, folderID, id, userID,
	)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	var n Note
	if err := h.db.QueryRow(
		`SELECT id, COALESCE(folder_id, ''), title, content, updated_at, created_at, deleted_at FROM notes WHERE id = ?`, id,
	).Scan(&n.ID, &n.FolderID, &n.Title, &n.Content, &n.UpdatedAt, &n.CreatedAt, &n.DeletedAt); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"note": n})
}

// Trash handles PATCH /notes/:id/trash
// Soft-deletes the note by setting deleted_at. Idempotent.
func (h *Handler) Trash(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")
	now := time.Now().UnixMilli()

	res, err := h.db.Exec(
		`UPDATE notes SET deleted_at = ?, updated_at = ? WHERE id = ? AND user_id = ?`,
		now, now, id, userID,
	)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	w.WriteHeader(http.StatusNoContent)
}

// Restore handles PATCH /notes/:id/restore
// Clears deleted_at, moving the note out of the wastebasket. Idempotent.
func (h *Handler) Restore(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")
	now := time.Now().UnixMilli()

	res, err := h.db.Exec(
		`UPDATE notes SET deleted_at = NULL, updated_at = ? WHERE id = ? AND user_id = ?`,
		now, id, userID,
	)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	w.WriteHeader(http.StatusNoContent)
}

// Delete handles DELETE /notes/:id
func (h *Handler) Delete(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")

	res, err := h.db.Exec(`DELETE FROM notes WHERE id = ? AND user_id = ?`, id, userID)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	w.WriteHeader(http.StatusNoContent)
}

const (
	wsPingInterval = 30 * time.Second
	wsReadTimeout  = 60 * time.Second
)

// ServeWS handles GET /notes/:id/ws?token=<jwt>
// Upgrades to WebSocket for live content sync on a single note.
func (h *Handler) ServeWS(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	noteID := chi.URLParam(r, "id")

	var title, content string
	var updatedAt int64
	if err := h.db.QueryRow(
		`SELECT title, content, updated_at FROM notes WHERE id = ? AND user_id = ?`, noteID, userID,
	).Scan(&title, &content, &updatedAt); err != nil {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	rawConn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return
	}

	c := &conn{Conn: rawConn}

	// Send current state BEFORE adding to hub.
	// This eliminates the race between the init write and a concurrent broadcast
	// that could occur if we added to the hub first.
	if err := c.writeJSON(wsMsg{Type: "init", Title: title, Content: content, UpdatedAt: updatedAt}); err != nil {
		rawConn.Close()
		return
	}

	h.hub.add(noteID, c)
	defer func() {
		h.hub.remove(noteID, c)
		rawConn.Close()
		log.Printf("ws: client disconnected from note %s", noteID)
	}()
	log.Printf("ws: client connected to note %s", noteID)

	// Keep the connection alive with periodic pings; drop zombie connections.
	rawConn.SetPongHandler(func(string) error {
		rawConn.SetReadDeadline(time.Now().Add(wsReadTimeout))
		return nil
	})

	go func() {
		ticker := time.NewTicker(wsPingInterval)
		defer ticker.Stop()
		for range ticker.C {
			c.mu.Lock()
			err := rawConn.WriteMessage(websocket.PingMessage, nil)
			c.mu.Unlock()
			if err != nil {
				return
			}
		}
	}()

	for {
		rawConn.SetReadDeadline(time.Now().Add(wsReadTimeout))
		var msg wsMsg
		if err := rawConn.ReadJSON(&msg); err != nil {
			break
		}
		if msg.Type != "update" || msg.UpdatedAt == 0 {
			continue
		}

		// Last-write-wins: verify timestamp and ownership atomically.
		var current int64
		if err := h.db.QueryRow(
			`SELECT updated_at FROM notes WHERE id = ? AND user_id = ?`, noteID, userID,
		).Scan(&current); err != nil {
			break
		}
		if msg.UpdatedAt <= current {
			continue // stale — discard silently
		}

		if _, err := h.db.Exec(
			`UPDATE notes SET title = ?, content = ?, updated_at = ? WHERE id = ? AND user_id = ?`,
			msg.Title, msg.Content, msg.UpdatedAt, noteID, userID,
		); err != nil {
			log.Printf("ws: db update error for note %s: %v", noteID, err)
			break
		}

		h.hub.broadcastExcept(noteID, c, wsMsg{
			Type:      "update",
			Title:     msg.Title,
			Content:   msg.Content,
			UpdatedAt: msg.UpdatedAt,
		})
	}
}
