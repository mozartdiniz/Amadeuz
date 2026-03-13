# Amadeuz Notes — Server

Go sync server for the Amadeuz Notes app. Handles authentication, note storage, folder management, and real-time WebSocket sync.

## Requirements

- Go 1.23+

### Install Go

**Ubuntu / Debian:**

```bash
sudo apt install golang-go
```

**Fedora / RHEL:**

```bash
sudo dnf install golang
```

**Arch Linux:**

```bash
sudo pacman -S go
```

**macOS:**

```bash
brew install go
```

Or download directly from [go.dev/dl](https://go.dev/dl/).

## Build & Run

```bash
cd server
go mod tidy
go run .
```

The server listens on port `8080` on all interfaces, accepting connections from any device on the network.

For a compiled binary:

```bash
go build -o amadeuz-server .
./amadeuz-server
```

## Configuration

| Environment variable | Default        | Description                     |
|----------------------|----------------|---------------------------------|
| `AMADEUZ_DB`         | `amadeuz.db`   | Path to the SQLite database file |

Example:

```bash
AMADEUZ_DB=/var/lib/amadeuz/data.db ./amadeuz-server
```

## Data Storage

All data is stored in a single SQLite file (`amadeuz.db` by default, created automatically on first run). Uploaded blobs (images, attachments) are stored in a `blobs/` directory next to the binary.

## User Management

### Register

Users self-register through the client app, or via the API:

```bash
curl -X POST http://localhost:8080/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email": "you@example.com", "password": "yourpassword"}'
```

The response includes a `recovery_code` — save it somewhere safe. It's the only way to recover access if the password is lost.

### Reset a password (admin)

```bash
./amadeuz-server reset-password you@example.com newpassword
```

## API Overview

### Auth (unauthenticated)

| Method | Path             | Description                              |
|--------|------------------|------------------------------------------|
| POST   | `/auth/register` | Register a new account                   |
| POST   | `/auth/login`    | Log in, receive a JWT token              |
| POST   | `/auth/recover`  | Reset password using a recovery code     |

### Notes & Folders (JWT required)

| Method | Path                  | Description                    |
|--------|-----------------------|--------------------------------|
| GET    | `/folders`            | List all folders               |
| POST   | `/folders`            | Create a folder                |
| PATCH  | `/folders/{id}`       | Rename a folder                |
| DELETE | `/folders/{id}`       | Delete a folder                |
| GET    | `/notes`              | List all notes                 |
| POST   | `/notes`              | Create a note                  |
| PATCH  | `/notes/{id}`         | Update a note                  |
| PATCH  | `/notes/{id}/move`    | Move a note to a folder        |
| DELETE | `/notes/{id}`         | Delete a note                  |
| GET    | `/notes/{id}/ws`      | WebSocket — real-time sync     |
| GET    | `/blobs/{id}`         | Download a blob (public)       |
| PUT    | `/blobs/{id}`         | Upload a blob (auth required)  |

All authenticated requests require an `Authorization: Bearer <token>` header. Tokens are valid for 30 days.

## Project Structure

```
server/
├── main.go                    # Entry point, router setup, graceful shutdown
├── go.mod / go.sum
└── internal/
    ├── auth/
    │   ├── handler.go         # Register, login, recover endpoints
    │   └── middleware.go      # JWT validation middleware
    ├── db/
    │   └── db.go              # SQLite setup, schema, helpers
    ├── folders/
    │   └── handler.go         # Folder CRUD
    ├── notes/
    │   ├── handler.go         # Note CRUD + WebSocket upgrade
    │   └── hub.go             # Per-note WebSocket hub, broadcast logic
    └── blobs/
        └── handler.go         # File upload/download
```
