package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"amadeuz/server/internal/auth"
	"amadeuz/server/internal/blobs"
	"amadeuz/server/internal/db"
	"amadeuz/server/internal/folders"
	"amadeuz/server/internal/notes"

	"github.com/go-chi/chi/v5"
)

// testServer builds a fully wired test server backed by a temp SQLite DB.
func testServer(t *testing.T) (http.Handler, func()) {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := db.Open(dbPath)
	if err != nil {
		t.Fatalf("open test db: %v", err)
	}

	jwtSecret, err := database.JWTSecret()
	if err != nil {
		t.Fatalf("jwt secret: %v", err)
	}

	authHandler := auth.NewHandler(database, jwtSecret)
	foldersHandler := folders.NewHandler(database)
	notesHandler := notes.NewHandler(database)
	blobsHandler := blobs.NewHandler(filepath.Join(t.TempDir(), "blobs"))
	authMiddleware := auth.Middleware(jwtSecret)

	r := chi.NewRouter()
	r.Post("/auth/register", authHandler.Register)
	r.Post("/auth/login", authHandler.Login)
	r.Post("/auth/recover", authHandler.Recover)
	r.Get("/blobs/{id}", blobsHandler.Download)

	r.Group(func(r chi.Router) {
		r.Use(authMiddleware)
		r.Get("/folders", foldersHandler.List)
		r.Post("/folders", foldersHandler.Create)
		r.Patch("/folders/{id}", foldersHandler.Rename)
		r.Delete("/folders/{id}", foldersHandler.Delete)
		r.Get("/notes", notesHandler.List)
		r.Post("/notes", notesHandler.Create)
		r.Patch("/notes/{id}", notesHandler.Update)
		r.Patch("/notes/{id}/move", notesHandler.Move)
		r.Delete("/notes/{id}", notesHandler.Delete)
		r.Put("/blobs/{id}", blobsHandler.Upload)
	})

	return r, func() { database.Close() }
}

// helpers

func do(t *testing.T, handler http.Handler, method, path, token string, body any) *httptest.ResponseRecorder {
	t.Helper()
	var buf bytes.Buffer
	if body != nil {
		json.NewEncoder(&buf).Encode(body)
	}
	req := httptest.NewRequest(method, path, &buf)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	return w
}

func mustDecode(t *testing.T, w *httptest.ResponseRecorder, v any) {
	t.Helper()
	if err := json.NewDecoder(w.Body).Decode(v); err != nil {
		t.Fatalf("decode response: %v\nbody: %s", err, w.Body.String())
	}
}

func register(t *testing.T, h http.Handler, email, password string) (token, recoveryCode string) {
	t.Helper()
	w := do(t, h, "POST", "/auth/register", "", map[string]string{"email": email, "password": password})
	if w.Code != http.StatusCreated {
		t.Fatalf("register: expected 201, got %d: %s", w.Code, w.Body.String())
	}
	var resp map[string]string
	mustDecode(t, w, &resp)
	return resp["token"], resp["recovery_code"]
}

// --- Auth tests ---

func TestRegister(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, recoveryCode := register(t, h, "alice@example.com", "password123")

	if token == "" {
		t.Fatal("expected non-empty token")
	}
	if len(recoveryCode) != 32 {
		t.Fatalf("expected 32-char recovery code, got %d chars", len(recoveryCode))
	}
}

func TestRegisterDuplicateEmail(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	register(t, h, "alice@example.com", "password123")
	w := do(t, h, "POST", "/auth/register", "", map[string]string{"email": "alice@example.com", "password": "other"})
	if w.Code != http.StatusConflict {
		t.Fatalf("expected 409, got %d", w.Code)
	}
}

func TestRegisterEmailNormalisation(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	register(t, h, "Alice@Example.COM", "password123")
	// Login with lowercase should work because register normalises to lowercase.
	w := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "password123"})
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200 after normalised login, got %d", w.Code)
	}
}

func TestLogin(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	register(t, h, "alice@example.com", "password123")

	w := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "password123"})
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var resp map[string]string
	mustDecode(t, w, &resp)
	if resp["token"] == "" {
		t.Fatal("expected non-empty token")
	}
}

func TestLoginWrongPassword(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	register(t, h, "alice@example.com", "password123")
	w := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "wrong"})
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", w.Code)
	}
}

func TestLoginUnknownEmail(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	w := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "nobody@example.com", "password": "x"})
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", w.Code)
	}
}

func TestRecover(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	_, recoveryCode := register(t, h, "alice@example.com", "password123")

	w := do(t, h, "POST", "/auth/recover", "", map[string]string{
		"email":         "alice@example.com",
		"recovery_code": recoveryCode,
		"new_password":  "newpassword456",
	})
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var resp map[string]string
	mustDecode(t, w, &resp)
	if resp["token"] == "" {
		t.Fatal("expected token in recover response")
	}
	if resp["recovery_code"] == recoveryCode {
		t.Fatal("recovery code should be rotated after use")
	}

	// Old password no longer works.
	w2 := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "password123"})
	if w2.Code != http.StatusUnauthorized {
		t.Fatalf("old password should be rejected after recovery, got %d", w2.Code)
	}

	// New password works.
	w3 := do(t, h, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "newpassword456"})
	if w3.Code != http.StatusOK {
		t.Fatalf("new password should work after recovery, got %d", w3.Code)
	}
}

func TestRecoverWrongCode(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	register(t, h, "alice@example.com", "password123")

	w := do(t, h, "POST", "/auth/recover", "", map[string]string{
		"email":         "alice@example.com",
		"recovery_code": "0000000000000000000000000000000000000000",
		"new_password":  "newpassword456",
	})
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", w.Code)
	}
}

func TestUnauthenticated(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	paths := []string{"/folders", "/notes"}
	for _, p := range paths {
		w := do(t, h, "GET", p, "", nil)
		if w.Code != http.StatusUnauthorized {
			t.Fatalf("GET %s without token: expected 401, got %d", p, w.Code)
		}
	}
}

// --- Folder tests ---

func TestFolderCRUD(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")

	// Create
	w := do(t, h, "POST", "/folders", token, map[string]string{"name": "Work"})
	if w.Code != http.StatusCreated {
		t.Fatalf("create folder: expected 201, got %d: %s", w.Code, w.Body.String())
	}
	var createResp map[string]map[string]any
	mustDecode(t, w, &createResp)
	folderID := createResp["folder"]["id"].(string)
	if folderID == "" {
		t.Fatal("expected folder id")
	}

	// List
	w = do(t, h, "GET", "/folders", token, nil)
	if w.Code != http.StatusOK {
		t.Fatalf("list folders: expected 200, got %d", w.Code)
	}
	var listResp map[string][]any
	mustDecode(t, w, &listResp)
	if len(listResp["folders"]) != 1 {
		t.Fatalf("expected 1 folder, got %d", len(listResp["folders"]))
	}

	// Rename
	w = do(t, h, "PATCH", "/folders/"+folderID, token, map[string]string{"name": "Work Projects"})
	if w.Code != http.StatusOK {
		t.Fatalf("rename folder: expected 200, got %d: %s", w.Code, w.Body.String())
	}

	// Delete
	w = do(t, h, "DELETE", "/folders/"+folderID, token, nil)
	if w.Code != http.StatusNoContent {
		t.Fatalf("delete folder: expected 204, got %d", w.Code)
	}

	// List again — should be empty
	w = do(t, h, "GET", "/folders", token, nil)
	mustDecode(t, w, &listResp)
	if len(listResp["folders"]) != 0 {
		t.Fatalf("expected 0 folders after delete, got %d", len(listResp["folders"]))
	}
}

func TestFolderDeleteCascadesNotes(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")

	// Create folder.
	w := do(t, h, "POST", "/folders", token, map[string]string{"name": "Work"})
	var fr map[string]map[string]any
	mustDecode(t, w, &fr)
	folderID := fr["folder"]["id"].(string)

	// Create notes inside the folder.
	for i := 0; i < 3; i++ {
		do(t, h, "POST", "/notes", token, map[string]any{
			"folder_id":  folderID,
			"title":      fmt.Sprintf("Note %d", i),
			"updated_at": time.Now().UnixMilli(),
		})
	}

	// Create an unfoldered note — should survive the folder delete.
	do(t, h, "POST", "/notes", token, map[string]any{"title": "Unfoldered", "updated_at": time.Now().UnixMilli()})

	// Delete the folder.
	do(t, h, "DELETE", "/folders/"+folderID, token, nil)

	// Only the unfoldered note should remain.
	w = do(t, h, "GET", "/notes", token, nil)
	var nr map[string][]any
	mustDecode(t, w, &nr)
	if len(nr["notes"]) != 1 {
		t.Fatalf("expected 1 note after folder delete cascade, got %d", len(nr["notes"]))
	}
}

func TestFolderIsolation(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	tokenA, _ := register(t, h, "alice@example.com", "pass")
	tokenB, _ := register(t, h, "bob@example.com", "pass")

	// Alice creates a folder.
	w := do(t, h, "POST", "/folders", tokenA, map[string]string{"name": "Alice's folder"})
	var fr map[string]map[string]any
	mustDecode(t, w, &fr)
	folderID := fr["folder"]["id"].(string)

	// Bob cannot see it.
	w = do(t, h, "GET", "/folders", tokenB, nil)
	var lr map[string][]any
	mustDecode(t, w, &lr)
	if len(lr["folders"]) != 0 {
		t.Fatalf("bob should see 0 folders, got %d", len(lr["folders"]))
	}

	// Bob cannot delete it.
	w = do(t, h, "DELETE", "/folders/"+folderID, tokenB, nil)
	if w.Code != http.StatusNotFound {
		t.Fatalf("bob deleting alice's folder: expected 404, got %d", w.Code)
	}
}

// --- Note tests ---

func TestNoteCRUD(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	now := time.Now().UnixMilli()

	// Create
	w := do(t, h, "POST", "/notes", token, map[string]any{
		"title":      "My Note",
		"content":    "Hello",
		"updated_at": now,
	})
	if w.Code != http.StatusCreated {
		t.Fatalf("create note: expected 201, got %d: %s", w.Code, w.Body.String())
	}
	var createResp map[string]map[string]any
	mustDecode(t, w, &createResp)
	noteID := createResp["note"]["id"].(string)
	if noteID == "" {
		t.Fatal("expected note id")
	}

	// List
	w = do(t, h, "GET", "/notes", token, nil)
	var listResp map[string][]any
	mustDecode(t, w, &listResp)
	if len(listResp["notes"]) != 1 {
		t.Fatalf("expected 1 note, got %d", len(listResp["notes"]))
	}

	// Update
	w = do(t, h, "PATCH", "/notes/"+noteID, token, map[string]any{
		"title":      "Updated",
		"content":    "World",
		"updated_at": now + 1000,
	})
	if w.Code != http.StatusOK {
		t.Fatalf("update note: expected 200, got %d: %s", w.Code, w.Body.String())
	}

	// Delete
	w = do(t, h, "DELETE", "/notes/"+noteID, token, nil)
	if w.Code != http.StatusNoContent {
		t.Fatalf("delete note: expected 204, got %d", w.Code)
	}

	w = do(t, h, "GET", "/notes", token, nil)
	mustDecode(t, w, &listResp)
	if len(listResp["notes"]) != 0 {
		t.Fatalf("expected 0 notes after delete, got %d", len(listResp["notes"]))
	}
}

func TestNoteLastWriteWins(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	now := time.Now().UnixMilli()

	// Create note at ts=1000.
	w := do(t, h, "POST", "/notes", token, map[string]any{"title": "Note", "content": "v1", "updated_at": now})
	var cr map[string]map[string]any
	mustDecode(t, w, &cr)
	noteID := cr["note"]["id"].(string)

	// Update with newer timestamp — should be accepted.
	w = do(t, h, "PATCH", "/notes/"+noteID, token, map[string]any{"title": "Note", "content": "v2", "updated_at": now + 1000})
	if w.Code != http.StatusOK {
		t.Fatalf("newer update: expected 200, got %d", w.Code)
	}

	// Update with older timestamp — should be rejected with 409.
	w = do(t, h, "PATCH", "/notes/"+noteID, token, map[string]any{"title": "Note", "content": "v0", "updated_at": now - 1})
	if w.Code != http.StatusConflict {
		t.Fatalf("stale update: expected 409, got %d", w.Code)
	}

	// Update with same timestamp — should be rejected with 409.
	w = do(t, h, "PATCH", "/notes/"+noteID, token, map[string]any{"title": "Note", "content": "v2b", "updated_at": now + 1000})
	if w.Code != http.StatusConflict {
		t.Fatalf("same-timestamp update: expected 409, got %d", w.Code)
	}

	// Verify content was not clobbered.
	w = do(t, h, "GET", "/notes", token, nil)
	var lr map[string][]map[string]any
	mustDecode(t, w, &lr)
	if content := lr["notes"][0]["content"].(string); content != "v2" {
		t.Fatalf("content should be v2 after rejected updates, got %q", content)
	}
}

func TestNoteMove(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")

	// Create folder and note.
	w := do(t, h, "POST", "/folders", token, map[string]string{"name": "Work"})
	var fr map[string]map[string]any
	mustDecode(t, w, &fr)
	folderID := fr["folder"]["id"].(string)

	w = do(t, h, "POST", "/notes", token, map[string]any{"title": "Note", "updated_at": time.Now().UnixMilli()})
	var nr map[string]map[string]any
	mustDecode(t, w, &nr)
	noteID := nr["note"]["id"].(string)

	// Move into folder.
	w = do(t, h, "PATCH", "/notes/"+noteID+"/move", token, map[string]string{"folder_id": folderID})
	if w.Code != http.StatusOK {
		t.Fatalf("move note: expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var mr map[string]map[string]any
	mustDecode(t, w, &mr)
	if mr["note"]["folder_id"] != folderID {
		t.Fatalf("note should be in folder %s, got %v", folderID, mr["note"]["folder_id"])
	}

	// Move out of folder (empty folder_id).
	w = do(t, h, "PATCH", "/notes/"+noteID+"/move", token, map[string]string{"folder_id": ""})
	if w.Code != http.StatusOK {
		t.Fatalf("unfoldr note: expected 200, got %d", w.Code)
	}
	mustDecode(t, w, &mr)
	if mr["note"]["folder_id"] != "" {
		t.Fatalf("note should have no folder, got %v", mr["note"]["folder_id"])
	}
}

func TestNoteIsolation(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	tokenA, _ := register(t, h, "alice@example.com", "pass")
	tokenB, _ := register(t, h, "bob@example.com", "pass")

	// Alice creates a note.
	w := do(t, h, "POST", "/notes", tokenA, map[string]any{"title": "Secret", "content": "private", "updated_at": time.Now().UnixMilli()})
	var nr map[string]map[string]any
	mustDecode(t, w, &nr)
	noteID := nr["note"]["id"].(string)

	// Bob cannot see it.
	w = do(t, h, "GET", "/notes", tokenB, nil)
	var lr map[string][]any
	mustDecode(t, w, &lr)
	if len(lr["notes"]) != 0 {
		t.Fatalf("bob should see 0 notes, got %d", len(lr["notes"]))
	}

	// Bob cannot update it.
	w = do(t, h, "PATCH", "/notes/"+noteID, tokenB, map[string]any{"title": "Hacked", "content": "x", "updated_at": time.Now().UnixMilli() + 9999})
	if w.Code != http.StatusNotFound {
		t.Fatalf("bob updating alice's note: expected 404, got %d", w.Code)
	}

	// Bob cannot delete it.
	w = do(t, h, "DELETE", "/notes/"+noteID, tokenB, nil)
	if w.Code != http.StatusNotFound {
		t.Fatalf("bob deleting alice's note: expected 404, got %d", w.Code)
	}
}

func TestNoteClientProvidedID(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	clientID := "aabbccdd-1122-3344-5566-778899aabbcc"

	w := do(t, h, "POST", "/notes", token, map[string]any{
		"id":         clientID,
		"title":      "Offline note",
		"content":    "Created while offline",
		"updated_at": time.Now().UnixMilli(),
	})
	if w.Code != http.StatusCreated {
		t.Fatalf("expected 201, got %d: %s", w.Code, w.Body.String())
	}
	var nr map[string]map[string]any
	mustDecode(t, w, &nr)
	if nr["note"]["id"] != clientID {
		t.Fatalf("expected client-provided ID %q, got %v", clientID, nr["note"]["id"])
	}
}

// --- Blob tests ---

func TestBlobUploadAndDownload(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	blobID := "aabbccdd-1122-3344-5566-778899001122"
	data := []byte("fake image bytes")

	// Upload (authenticated)
	req := httptest.NewRequest("PUT", "/blobs/"+blobID, bytes.NewReader(data))
	req.Header.Set("Authorization", "Bearer "+token)
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != http.StatusCreated {
		t.Fatalf("blob upload: expected 201, got %d: %s", w.Code, w.Body.String())
	}

	// Download (unauthenticated)
	req2 := httptest.NewRequest("GET", "/blobs/"+blobID, nil)
	w2 := httptest.NewRecorder()
	h.ServeHTTP(w2, req2)
	if w2.Code != http.StatusOK {
		t.Fatalf("blob download: expected 200, got %d", w2.Code)
	}
	if got := w2.Body.Bytes(); string(got) != string(data) {
		t.Fatalf("blob content mismatch: got %q, want %q", got, data)
	}
}

func TestBlobUploadIdempotent(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	blobID := "aabbccdd-1122-3344-5566-778899001122"
	data := []byte("original")

	upload := func(body []byte) int {
		req := httptest.NewRequest("PUT", "/blobs/"+blobID, bytes.NewReader(body))
		req.Header.Set("Authorization", "Bearer "+token)
		w := httptest.NewRecorder()
		h.ServeHTTP(w, req)
		return w.Code
	}

	if code := upload(data); code != http.StatusCreated {
		t.Fatalf("first upload: expected 201, got %d", code)
	}
	if code := upload([]byte("different")); code != http.StatusOK {
		t.Fatalf("idempotent re-upload: expected 200, got %d", code)
	}

	// Content should still be the original.
	req := httptest.NewRequest("GET", "/blobs/"+blobID, nil)
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Body.String() != string(data) {
		t.Fatalf("content changed after idempotent re-upload: got %q", w.Body.String())
	}
}

func TestBlobUploadRequiresAuth(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	req := httptest.NewRequest("PUT", "/blobs/aabbccdd-1122-3344-5566-778899001122", bytes.NewReader([]byte("data")))
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated blob upload: expected 401, got %d", w.Code)
	}
}

// --- reset-password ---

func TestResetPassword(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := db.Open(dbPath)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer database.Close()

	jwtSecret, _ := database.JWTSecret()
	authHandler := auth.NewHandler(database, jwtSecret)
	authMiddleware := auth.Middleware(jwtSecret)

	r := chi.NewRouter()
	r.Post("/auth/register", authHandler.Register)
	r.Post("/auth/login", authHandler.Login)
	r.Group(func(r chi.Router) {
		r.Use(authMiddleware)
		r.Get("/notes", notes.NewHandler(database).List)
	})

	// Register.
	w := do(t, r, "POST", "/auth/register", "", map[string]string{"email": "alice@example.com", "password": "old"})
	if w.Code != http.StatusCreated {
		t.Fatalf("register: %d", w.Code)
	}

	// Reset password via CLI function.
	origStdout := os.Stdout
	os.Stdout, _ = os.Open(os.DevNull)
	resetPassword(database, "alice@example.com", "newpassword")
	os.Stdout = origStdout

	// Old password rejected.
	w = do(t, r, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "old"})
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("old password should be rejected after reset, got %d", w.Code)
	}

	// New password accepted.
	w = do(t, r, "POST", "/auth/login", "", map[string]string{"email": "alice@example.com", "password": "newpassword"})
	if w.Code != http.StatusOK {
		t.Fatalf("new password should work after reset, got %d", w.Code)
	}
}

func TestRecoverCodeIsConsumed(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	_, recoveryCode := register(t, h, "alice@example.com", "password123")

	// First use: succeeds and issues a new recovery code.
	w := do(t, h, "POST", "/auth/recover", "", map[string]string{
		"email":         "alice@example.com",
		"recovery_code": recoveryCode,
		"new_password":  "newpassword456",
	})
	if w.Code != http.StatusOK {
		t.Fatalf("first recover: expected 200, got %d", w.Code)
	}

	// Second use of the same code: must be rejected.
	w = do(t, h, "POST", "/auth/recover", "", map[string]string{
		"email":         "alice@example.com",
		"recovery_code": recoveryCode,
		"new_password":  "anotherpassword",
	})
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("reused recovery code: expected 401, got %d", w.Code)
	}
}

func TestNoteUpdateMissingUpdatedAt(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	now := time.Now().UnixMilli()

	w := do(t, h, "POST", "/notes", token, map[string]any{"title": "Note", "updated_at": now})
	var cr map[string]map[string]any
	mustDecode(t, w, &cr)
	noteID := cr["note"]["id"].(string)

	// Omitting updated_at (zero value) should be rejected.
	w = do(t, h, "PATCH", "/notes/"+noteID, token, map[string]any{"title": "Updated"})
	if w.Code != http.StatusBadRequest {
		t.Fatalf("update without updated_at: expected 400, got %d", w.Code)
	}
}

func TestFolderClientProvidedID(t *testing.T) {
	h, cleanup := testServer(t)
	defer cleanup()

	token, _ := register(t, h, "alice@example.com", "pass")
	clientID := "aabbccdd-1122-3344-5566-778899aabbcc"
	createdAt := time.Now().UnixMilli()

	w := do(t, h, "POST", "/folders", token, map[string]any{
		"id":         clientID,
		"name":       "Offline Folder",
		"created_at": createdAt,
	})
	if w.Code != http.StatusCreated {
		t.Fatalf("expected 201, got %d: %s", w.Code, w.Body.String())
	}
	var fr map[string]map[string]any
	mustDecode(t, w, &fr)
	if fr["folder"]["id"] != clientID {
		t.Fatalf("expected client-provided ID %q, got %v", clientID, fr["folder"]["id"])
	}
	if int64(fr["folder"]["created_at"].(float64)) != createdAt {
		t.Fatalf("expected client-provided created_at %d, got %v", createdAt, fr["folder"]["created_at"])
	}
}
