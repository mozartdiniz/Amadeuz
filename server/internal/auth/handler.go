package auth

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"log"
	"net/http"
	"strings"
	"time"

	"amadeuz/server/internal/db"

	"github.com/golang-jwt/jwt/v5"
	"golang.org/x/crypto/bcrypt"
)

// dummyHash is used in constant-time comparisons when the user does not exist.
// This prevents timing attacks that distinguish "unknown email" from "wrong password".
var dummyHash, _ = bcrypt.GenerateFromPassword([]byte("dummy-constant-time-placeholder"), bcrypt.DefaultCost)

type Handler struct {
	db        *db.DB
	jwtSecret []byte
}

func NewHandler(database *db.DB, jwtSecret []byte) *Handler {
	return &Handler{db: database, jwtSecret: jwtSecret}
}

// Register handles POST /auth/register
// Body: { "email": "...", "password": "..." }
// Response: { "token": "...", "recovery_code": "..." }
func (h *Handler) Register(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Email    string `json:"email"`
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Email == "" || req.Password == "" {
		http.Error(w, "email and password required", http.StatusBadRequest)
		return
	}
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))

	passwordHash, err := bcrypt.GenerateFromPassword([]byte(req.Password), bcrypt.DefaultCost)
	if err != nil {
		log.Printf("auth: register: bcrypt error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	recoveryCode, err := newRecoveryCode()
	if err != nil {
		log.Printf("auth: register: recovery code error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}
	recoveryHash, err := bcrypt.GenerateFromPassword([]byte(recoveryCode), bcrypt.DefaultCost)
	if err != nil {
		log.Printf("auth: register: bcrypt recovery error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	id := db.NewID()
	_, err = h.db.Exec(
		`INSERT INTO users (id, email, password_hash, recovery_code_hash, created_at) VALUES (?, ?, ?, ?, ?)`,
		id, req.Email, string(passwordHash), string(recoveryHash), time.Now().UnixMilli(),
	)
	if err != nil {
		// Most likely cause: UNIQUE constraint on email.
		log.Printf("auth: register: db error for %q: %v", req.Email, err)
		http.Error(w, "email already registered", http.StatusConflict)
		return
	}
	log.Printf("auth: registered user %q (%s)", req.Email, id)

	token, err := h.signToken(id)
	if err != nil {
		log.Printf("auth: register: sign token error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{"token": token, "recovery_code": recoveryCode})
}

// Login handles POST /auth/login
// Body: { "email": "...", "password": "..." }
// Response: { "token": "..." }
func (h *Handler) Login(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Email    string `json:"email"`
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Email == "" || req.Password == "" {
		http.Error(w, "email and password required", http.StatusBadRequest)
		return
	}
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))

	var id, hash string
	err := h.db.QueryRow(`SELECT id, password_hash FROM users WHERE email = ?`, req.Email).Scan(&id, &hash)
	if err != nil {
		// User not found: run bcrypt against the dummy hash to equalise response time
		// and prevent timing attacks that enumerate valid email addresses.
		bcrypt.CompareHashAndPassword(dummyHash, []byte(req.Password)) //nolint:errcheck
		log.Printf("auth: login failed (unknown email): %q", req.Email)
		http.Error(w, "invalid credentials", http.StatusUnauthorized)
		return
	}
	if err := bcrypt.CompareHashAndPassword([]byte(hash), []byte(req.Password)); err != nil {
		log.Printf("auth: login failed (wrong password): %q", req.Email)
		http.Error(w, "invalid credentials", http.StatusUnauthorized)
		return
	}

	token, err := h.signToken(id)
	if err != nil {
		log.Printf("auth: login: sign token error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	log.Printf("auth: login OK: %q (%s)", req.Email, id)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"token": token})
}

// Recover handles POST /auth/recover
// Body: { "email": "...", "recovery_code": "...", "new_password": "..." }
// Response: { "token": "...", "recovery_code": "..." }
// The old recovery code is consumed and a new one is issued — the client must show it to the user.
func (h *Handler) Recover(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Email        string `json:"email"`
		RecoveryCode string `json:"recovery_code"`
		NewPassword  string `json:"new_password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Email == "" || req.RecoveryCode == "" || req.NewPassword == "" {
		http.Error(w, "email, recovery_code, and new_password required", http.StatusBadRequest)
		return
	}
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))

	var id, recoveryHash string
	err := h.db.QueryRow(`SELECT id, recovery_code_hash FROM users WHERE email = ?`, req.Email).Scan(&id, &recoveryHash)
	if err != nil {
		bcrypt.CompareHashAndPassword(dummyHash, []byte(req.RecoveryCode)) //nolint:errcheck
		log.Printf("auth: recover failed (unknown email): %q", req.Email)
		http.Error(w, "invalid credentials", http.StatusUnauthorized)
		return
	}
	if err := bcrypt.CompareHashAndPassword([]byte(recoveryHash), []byte(req.RecoveryCode)); err != nil {
		log.Printf("auth: recover failed (wrong code): %q (%s)", req.Email, id)
		http.Error(w, "invalid recovery code", http.StatusUnauthorized)
		return
	}

	newPasswordHash, err := bcrypt.GenerateFromPassword([]byte(req.NewPassword), bcrypt.DefaultCost)
	if err != nil {
		log.Printf("auth: recover: bcrypt error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	// Rotate the recovery code — the old one is consumed on use.
	newRecovery, err := newRecoveryCode()
	if err != nil {
		log.Printf("auth: recover: recovery code error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}
	newRecoveryHash, err := bcrypt.GenerateFromPassword([]byte(newRecovery), bcrypt.DefaultCost)
	if err != nil {
		log.Printf("auth: recover: bcrypt recovery error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	if _, err := h.db.Exec(
		`UPDATE users SET password_hash = ?, recovery_code_hash = ? WHERE id = ?`,
		string(newPasswordHash), string(newRecoveryHash), id,
	); err != nil {
		log.Printf("auth: recover: db error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	log.Printf("auth: password recovered for %q (%s)", req.Email, id)

	token, err := h.signToken(id)
	if err != nil {
		log.Printf("auth: recover: sign token error: %v", err)
		http.Error(w, "internal error", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"token": token, "recovery_code": newRecovery})
}

func (h *Handler) signToken(userID string) (string, error) {
	claims := jwt.MapClaims{
		"sub": userID,
		"exp": time.Now().Add(30 * 24 * time.Hour).Unix(),
	}
	return jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString(h.jwtSecret)
}

// newRecoveryCode generates a cryptographically random 32-character hex string.
func newRecoveryCode() (string, error) {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}
