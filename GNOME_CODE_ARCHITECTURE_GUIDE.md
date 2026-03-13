# GNOME Code Architecture Guide: Amadeuz Notes

This guide defines the "Mantra" for the Amadeuz Notes refactor, based on proven patterns from GNOME Circle projects like **iotas** and **Fragments**.

## 1. The Core Architecture: GObject-Driven MVVM

Modern GNOME apps are built on **GObject**. Even in Rust, we treat our data models as GObjects to leverage properties, signals, and efficient list binding.

- **Model (GObject):** Each Note should be a GObject with properties (`title`, `content`, `modified_at`).
- **View (Composite Templates):** UI is defined in `.blp` (Blueprint) and bound to a Rust/Python struct via `CompositeTemplate`.
- **ViewModel/Manager:** A central `NoteManager` (singleton or app-scoped) that holds the `Gio.ListModel` of notes.

### Mantra: "The UI is a reflection of the Model."
Never manually update a list row. Update the property on the GObject model, and let GTK's property bindings and `GtkListView` handle the rest.

## 2. Directory Structure (The "Standard Layout")

Organize your code by domain, not by "type" (don't put all models in one file).

```text
src/
├── main.rs            # Entry point
├── app.rs             # Application subclass, handles global actions
├── model/             # GObject Data Models
│   ├── mod.rs
│   ├── note.rs        # The Note GObject
│   └── folder.rs      # (Future) Folder/Category GObject
├── ui/                # UI Components (Views)
│   ├── mod.rs
│   ├── window.rs      # Main AdwWindow
│   ├── sidebar.rs     # The Note List (Master)
│   ├── editor.rs      # The Note Editor (Detail)
│   └── widgets/       # Reusable small widgets (NoteRow, StatusBadge)
├── backend/           # Business Logic & Sync
│   ├── mod.rs
│   ├── database.rs    # SQLite/Storage logic
│   ├── sync_worker.rs # WebSocket/API logic
│   └── keyring.rs     # Secret Service integration
└── utils.rs           # Helpers (Date formatting, Markdown parsing)
```

## 3. Master-Detail Pattern (Adaptive UI)

For a document-based app, use `AdwNavigationSplitView` (or `AdwOverlaySplitView`).

- **Master:** A `GtkListView` inside a sidebar.
- **Detail:** An editor component (using `GtkSourceView` for Markdown).
- **Communication:**
  - The Sidebar emits a `note-selected(Note)` signal.
  - The Window listens and calls `editor.set_note(note)`.
  - The Editor binds its text buffer to the `note.content` property.

## 4. Sync & Persistence Strategy

Follow the **"Persistence-First"** pattern seen in `iotas`:

1.  **Local First:** Every keystroke is debounced and saved to the local database immediately.
2.  **Manager-Mediated Sync:** The `NoteManager` observes changes to the local database and triggers the `SyncWorker`.
3.  **Background Threading:** Sync operations MUST happen on a background thread (Tokio in Rust) to keep the UI at 60fps.
4.  **Conflicts:** Use a `modified_at` timestamp. If a remote change is newer, show an `AdwBanner` or a "Conflict" badge on the note.

## 5. Coding Guidelines & Best Practices

- **Actions over Callbacks:** Use `Gio.Action` for everything (win.save, win.delete). It makes keyboard shortcuts and menus work automatically.
- **Strong Typing:** In Rust, use `glib::Properties` to derive GObject properties. This reduces boilerplate.
- **State via Signals:** If the `SyncWorker` finishes, it emits a signal. The UI (Window/Sidebar) connects to this signal to show "Synced just now".
- **GSettings for Config:** Never use a custom JSON for settings. Use `gio.Settings` for things like `server-url` and `auto-sync-enabled`.

## 6. The "Mantra" for Claude (Implementation Instructions)

When evolving Amadeuz, follow these rules:
1.  **Always use Libadwaita widgets.** If there's an `Adw` version of a widget, use it.
2.  **Define UI in Blueprint.** Do not build complex UIs in code.
3.  **Prefer `GtkListView` + `Gio.ListModel`** over `GtkListBox` for the note list. It's more performant.
4.  **Handle Errors Gracefully.** Use `AdwToast` for errors, never print to stderr and expect the user to see it.
5.  **Stay "GNOME-y".** Use standard icons (`edit-symbolic`, `document-save-symbolic`) and follow the HIG spacing.

---
*Inspired by iotas, Fragments, and the GNOME Human Interface Guidelines.*
