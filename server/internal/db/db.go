package db

import (
	"crypto/rand"
	"database/sql"
	"encoding/base64"
	"fmt"
	"log"

	_ "modernc.org/sqlite"
)

// DB wraps *sql.DB with app-specific helpers.
type DB struct {
	*sql.DB
}

// Open opens (or creates) the SQLite database at path and runs migrations.
func Open(path string) (*DB, error) {
	raw, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, fmt.Errorf("db open: %w", err)
	}
	// modernc.org/sqlite requires PRAGMAs to be set explicitly via separate Exec calls —
	// DSN query parameters are not reliably applied, and multi-statement Exec
	// only executes the first statement on some drivers.
	if _, err := raw.Exec(`PRAGMA foreign_keys = ON`); err != nil {
		return nil, fmt.Errorf("db pragma foreign_keys: %w", err)
	}
	if _, err := raw.Exec(`PRAGMA journal_mode = WAL`); err != nil {
		return nil, fmt.Errorf("db pragma journal_mode: %w", err)
	}
	if err := migrate(raw); err != nil {
		return nil, fmt.Errorf("db migrate: %w", err)
	}
	return &DB{raw}, nil
}

// JWTSecret returns the persistent JWT signing secret, generating one on first call.
func (db *DB) JWTSecret() ([]byte, error) {
	var encoded string
	err := db.QueryRow(`SELECT value FROM config WHERE key = 'jwt_secret'`).Scan(&encoded)
	if err == nil {
		return base64.StdEncoding.DecodeString(encoded)
	}
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return nil, fmt.Errorf("generate jwt secret: %w", err)
	}
	encoded = base64.StdEncoding.EncodeToString(b)
	if _, err := db.Exec(`INSERT INTO config (key, value) VALUES ('jwt_secret', ?)`, encoded); err != nil {
		return nil, fmt.Errorf("store jwt secret: %w", err)
	}
	log.Println("db: generated new JWT secret")
	return b, nil
}

// NewID returns a random UUID v4-like identifier.
// Calls log.Fatal if the OS cannot provide random bytes — this indicates
// a fatal system condition from which the server cannot safely continue.
func NewID() string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		log.Fatalf("newID: entropy source unavailable: %v", err)
	}
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

func migrate(db *sql.DB) error {
	_, err := db.Exec(`
		CREATE TABLE IF NOT EXISTS config (
			key   TEXT PRIMARY KEY,
			value TEXT NOT NULL
		);

		CREATE TABLE IF NOT EXISTS users (
			id                 TEXT PRIMARY KEY,
			email              TEXT UNIQUE NOT NULL,
			password_hash      TEXT NOT NULL,
			recovery_code_hash TEXT NOT NULL,
			created_at         INTEGER NOT NULL
		);

		CREATE TABLE IF NOT EXISTS folders (
			id         TEXT PRIMARY KEY,
			user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			name       TEXT NOT NULL,
			created_at INTEGER NOT NULL
		);

		CREATE TABLE IF NOT EXISTS notes (
			id         TEXT PRIMARY KEY,
			user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			folder_id  TEXT REFERENCES folders(id) ON DELETE CASCADE,
			title      TEXT NOT NULL DEFAULT '',
			content    TEXT NOT NULL DEFAULT '',
			updated_at INTEGER NOT NULL,
			created_at INTEGER NOT NULL
		);
	`)
	return err
}
