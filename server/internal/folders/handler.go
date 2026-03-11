package folders

import (
	"encoding/json"
	"net/http"
	"time"

	"amadeuz/server/internal/auth"
	"amadeuz/server/internal/db"

	"github.com/go-chi/chi/v5"
)

type Folder struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	CreatedAt int64  `json:"created_at"`
}

type Handler struct {
	db *db.DB
}

func NewHandler(database *db.DB) *Handler {
	return &Handler{db: database}
}

// List handles GET /folders
func (h *Handler) List(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	rows, err := h.db.Query(
		`SELECT id, name, created_at FROM folders WHERE user_id = ? ORDER BY created_at`, userID,
	)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	defer rows.Close()

	folders := []Folder{}
	for rows.Next() {
		var f Folder
		if err := rows.Scan(&f.ID, &f.Name, &f.CreatedAt); err != nil {
			http.Error(w, "db error", http.StatusInternalServerError)
			return
		}
		folders = append(folders, f)
	}
	if err := rows.Err(); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"folders": folders})
}

// Create handles POST /folders
// Body: { "id": "...", "name": "...", "created_at": 123 }  (id and created_at optional)
func (h *Handler) Create(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	var body struct {
		ID        string `json:"id"`
		Name      string `json:"name"`
		CreatedAt int64  `json:"created_at"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Name == "" {
		http.Error(w, "name required", http.StatusBadRequest)
		return
	}

	id := body.ID
	if id == "" {
		id = db.NewID()
	}
	createdAt := body.CreatedAt
	if createdAt == 0 {
		createdAt = time.Now().UnixMilli()
	}

	if _, err := h.db.Exec(
		`INSERT INTO folders (id, user_id, name, created_at) VALUES (?, ?, ?, ?)`,
		id, userID, body.Name, createdAt,
	); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]any{"folder": Folder{ID: id, Name: body.Name, CreatedAt: createdAt}})
}

// Rename handles PATCH /folders/:id
// Body: { "name": "..." }
func (h *Handler) Rename(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")
	var body struct {
		Name string `json:"name"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Name == "" {
		http.Error(w, "name required", http.StatusBadRequest)
		return
	}

	res, err := h.db.Exec(`UPDATE folders SET name = ? WHERE id = ? AND user_id = ?`, body.Name, id, userID)
	if err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}
	if n, _ := res.RowsAffected(); n == 0 {
		http.Error(w, "not found", http.StatusNotFound)
		return
	}

	var f Folder
	if err := h.db.QueryRow(
		`SELECT id, name, created_at FROM folders WHERE id = ?`, id,
	).Scan(&f.ID, &f.Name, &f.CreatedAt); err != nil {
		http.Error(w, "db error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"folder": f})
}

// Delete handles DELETE /folders/:id
// Notes inside the folder are cascade-deleted by the DB (ON DELETE CASCADE).
func (h *Handler) Delete(w http.ResponseWriter, r *http.Request) {
	userID := auth.UserID(r)
	id := chi.URLParam(r, "id")

	res, err := h.db.Exec(`DELETE FROM folders WHERE id = ? AND user_id = ?`, id, userID)
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
