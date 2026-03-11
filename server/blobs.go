package main

import (
	"fmt"
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

// validBlobID returns true if s contains only lowercase hex digits and hyphens,
// matching the output format of newID(). Prevents path traversal attacks.
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

// Upload handles POST /blobs — stores the raw body as a new blob and returns its ID.
func (bs *blobStore) Upload(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
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
	id := newID()
	if err := os.WriteFile(filepath.Join(bs.dir, id), data, 0o644); err != nil {
		log.Printf("blobStore: write %s: %v", id, err)
		http.Error(w, "storage error", http.StatusInternalServerError)
		return
	}
	log.Printf("blob stored: %s (%d bytes)", id, len(data))
	w.Header().Set("Content-Type", "application/json")
	fmt.Fprintf(w, `{"id":%q}`, id)
}

// Download handles GET /blobs/:id — serves the blob with detected content-type.
func (bs *blobStore) Download(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
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
