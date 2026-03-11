package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"amadeuz/server/internal/auth"
	"amadeuz/server/internal/blobs"
	"amadeuz/server/internal/db"
	"amadeuz/server/internal/folders"
	"amadeuz/server/internal/notes"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"golang.org/x/crypto/bcrypt"
)

func main() {
	dbPath := "amadeuz.db"
	if p := os.Getenv("AMADEUZ_DB"); p != "" {
		dbPath = p
	}

	database, err := db.Open(dbPath)
	if err != nil {
		log.Fatalf("open db: %v", err)
	}
	defer database.Close()

	// Handle: amadeuz-server reset-password <email> <new-password>
	if len(os.Args) > 1 && os.Args[1] == "reset-password" {
		if len(os.Args) != 4 {
			fmt.Fprintln(os.Stderr, "usage: amadeuz-server reset-password <email> <new-password>")
			os.Exit(1)
		}
		resetPassword(database, os.Args[2], os.Args[3])
		return
	}

	jwtSecret, err := database.JWTSecret()
	if err != nil {
		log.Fatalf("jwt secret: %v", err)
	}

	authHandler := auth.NewHandler(database, jwtSecret)
	foldersHandler := folders.NewHandler(database)
	notesHandler := notes.NewHandler(database)
	blobsHandler := blobs.NewHandler("blobs")
	authMiddleware := auth.Middleware(jwtSecret)

	r := chi.NewRouter()
	r.Use(middleware.Logger)
	r.Use(middleware.Recoverer)

	// Unauthenticated REST routes (with request timeout)
	r.Group(func(r chi.Router) {
		r.Use(middleware.Timeout(30 * time.Second))
		r.Post("/auth/register", authHandler.Register)
		r.Post("/auth/login", authHandler.Login)
		r.Post("/auth/recover", authHandler.Recover)
		r.Get("/blobs/{id}", blobsHandler.Download)
	})

	// Authenticated REST routes (with request timeout)
	r.Group(func(r chi.Router) {
		r.Use(authMiddleware)
		r.Use(middleware.Timeout(30 * time.Second))

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

	// WebSocket route — no timeout middleware (long-lived connections).
	r.With(authMiddleware).Get("/notes/{id}/ws", notesHandler.ServeWS)

	addr := ":8080"
	srv := &http.Server{Addr: addr, Handler: r}

	go func() {
		log.Printf("amadeuz server listening on %s", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("listen: %v", err)
		}
	}()

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("shutting down...")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := srv.Shutdown(ctx); err != nil {
		log.Fatalf("shutdown: %v", err)
	}
	log.Println("stopped")
}

func resetPassword(database *db.DB, email, newPassword string) {
	hash, err := bcrypt.GenerateFromPassword([]byte(newPassword), bcrypt.DefaultCost)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
	res, err := database.Exec(`UPDATE users SET password_hash = ? WHERE email = ?`, string(hash), email)
	if err != nil {
		fmt.Fprintf(os.Stderr, "db error: %v\n", err)
		os.Exit(1)
	}
	n, _ := res.RowsAffected()
	if n == 0 {
		fmt.Fprintf(os.Stderr, "no user found with email %q\n", email)
		os.Exit(1)
	}
	fmt.Printf("password reset for %s\n", email)
}
