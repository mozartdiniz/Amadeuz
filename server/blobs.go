package main

import (
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

const maxBlobBytes = 20 << 20 // 20 MB

type blobStore struct {
	dir string
}

func newBlobStore(dir string) *blobStore {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		log.Fatalf("blobStore: mkdir: %v", err)
	}
	return &blobStore{dir: dir}
}

// validBlobID returns true if s contains only lowercase hex digits and hyphens.
// Matches both newID() output and standard UUID format (client-generated IDs).
func validBlobID(s string) bool {
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

// HandleBlob dispatches GET (download) and PUT (upload) for /blobs/:id.
func (bs *blobStore) HandleBlob(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		bs.download(w, r)
	case http.MethodPut:
		bs.upload(w, r)
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

// upload handles PUT /blobs/:id — client provides the blob ID.
// Idempotent: if the blob already exists, returns 200 without re-writing.
func (bs *blobStore) upload(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimPrefix(r.URL.Path, "/blobs/")
	if !validBlobID(id) {
		http.Error(w, "invalid id", http.StatusBadRequest)
		return
	}

	path := filepath.Join(bs.dir, id)

	// Already exists — idempotent success (handles retry after partial failure).
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
		log.Printf("blobStore: write %s: %v", id, err)
		http.Error(w, "storage error", http.StatusInternalServerError)
		return
	}
	log.Printf("blob stored: %s (%d bytes)", id, len(data))
	w.WriteHeader(http.StatusCreated)
}

// download handles GET /blobs/:id — serves the blob with detected content-type.
func (bs *blobStore) download(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimPrefix(r.URL.Path, "/blobs/")
	if !validBlobID(id) {
		http.Error(w, "invalid id", http.StatusBadRequest)
		return
	}
	data, err := os.ReadFile(filepath.Join(bs.dir, id))
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
		log.Printf("blobStore: serve %s: %v", id, err)
	}
}
