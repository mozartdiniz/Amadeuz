package blobs

import (
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"

	"github.com/go-chi/chi/v5"
)

const maxBlobBytes = 20 << 20 // 20 MB

type Handler struct {
	dir string
}

func NewHandler(dir string) *Handler {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		log.Fatalf("blobs: mkdir: %v", err)
	}
	return &Handler{dir: dir}
}

// Upload handles PUT /blobs/:id (authenticated).
// Idempotent: if the blob already exists, returns 200 without re-writing.
func (h *Handler) Upload(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "id")
	if !validID(id) {
		http.Error(w, "invalid id", http.StatusBadRequest)
		return
	}

	path := filepath.Join(h.dir, id)
	if _, err := os.Stat(path); err == nil {
		w.WriteHeader(http.StatusOK)
		return
	}

	r.Body = http.MaxBytesReader(w, r.Body, maxBlobBytes)
	data, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "request too large or read error", http.StatusBadRequest)
		return
	}
	if len(data) == 0 {
		http.Error(w, "empty body", http.StatusBadRequest)
		return
	}

	if err := os.WriteFile(path, data, 0o644); err != nil {
		log.Printf("blobs: write %s: %v", id, err)
		http.Error(w, "storage error", http.StatusInternalServerError)
		return
	}
	log.Printf("blob stored: %s (%d bytes)", id, len(data))
	w.WriteHeader(http.StatusCreated)
}

// Download handles GET /blobs/:id (unauthenticated — blob ID acts as a capability).
func (h *Handler) Download(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "id")
	if !validID(id) {
		http.Error(w, "invalid id", http.StatusBadRequest)
		return
	}

	data, err := os.ReadFile(filepath.Join(h.dir, id))
	if err != nil {
		if os.IsNotExist(err) {
			http.Error(w, "not found", http.StatusNotFound)
		} else {
			http.Error(w, "storage error", http.StatusInternalServerError)
		}
		return
	}

	w.Header().Set("Content-Type", http.DetectContentType(data))
	w.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
	if _, err := w.Write(data); err != nil {
		log.Printf("blobs: serve %s: %v", id, err)
	}
}

func validID(s string) bool {
	if s == "" || len(s) > 40 {
		return false
	}
	for _, c := range s {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || c == '-') {
			return false
		}
	}
	return true
}
