# GNOME Refactor Recommendation for Amadeuz Notes

This document provides a comprehensive strategy for refactoring the Amadeuz Notes Linux application to align with modern GNOME Circle standards as of 2026.

## 1. Core Technology Stack (The "2026 Standard")

The most recommended stack for high-quality, performant, and memory-safe GNOME applications is **Rust** paired with **GTK4** and **Libadwaita**.

- **Language:** **Rust**. It offers memory safety without a garbage collector, excellent concurrency for sync operations, and a first-class ecosystem for GNOME development via `gtk4-rs`.
- **UI Toolkit:** **GTK4** + **Libadwaita**. Libadwaita is essential for the modern GNOME look and feel (adaptive widgets, dark mode support, and HIG-compliant styling).
- **UI Definition:** **Blueprint**. A concise, human-readable language that compiles to GTK `.ui` XML files. It is significantly more maintainable than raw XML.
- **Asynchronous Runtime:** **Tokio** or **GLib Main Loop**. For WebSocket synchronization, Rust's `async/await` pattern is vastly superior to C++'s callback-heavy approach.

## 2. GNOME UI Guidelines (HIG) Compliance

To be accepted into the GNOME Circle or even the official GNOME Apps, the UI must follow the **Human Interface Guidelines (HIG)**:

- **Adaptive Design:** Use `AdwBreakpoint` and `AdwNavigationSplitView` to ensure the app works perfectly on both desktops and mobile devices (GNOME Mobile/Phosh).
- **Window Management:** Use `AdwWindow` and `AdwHeaderBar`. The header bar should contain the primary actions (Add Note, Search, Menu).
- **Typography & Spacing:** Adhere to the standard Adwaita spacing (usually 6px or 12px increments) and use standard typography classes.
- **Dark Mode:** Support `AdwStyleManager` for automatic dark/light theme switching based on system settings.
- **Feedback:** Use `AdwToast` for non-intrusive notifications (e.g., "Note saved" or "Syncing...").

## 3. Architecture & Design Patterns

Follow the **GObject-oriented** approach even in Rust:

- **Model-View:** Separate your data models (Notes) from the UI. Use `gio::ListModel` for the list of notes to leverage GTK's efficient list widgets (`GtkListView`).
- **Composite Templates:** Define UI in Blueprint files and bind them to Rust structs using the `CompositeTemplate` derive macro.
- **State Management:** Use GObject properties and signals for reactive UI updates.

## 4. Internationalization (I18n)

GNOME is a global project. Every app should be translatable from day one.

- **System:** Use **gettext**.
- **Integration:** Include a `po/` directory with a `POTFILES.in` tracking translatable source and UI files.
- **Workflow:** Use Meson's `i18n` module to automate the generation of `.mo` files and merging of translations into desktop/metainfo files.

## 5. GNOME Ecosystem Integration

To feel like a "first-class citizen," the app should integrate with system services:

- **GSettings:** Store user preferences (server URL, font size, etc.) in GSettings. This allows them to be managed via `dconf-editor` and ensures they follow system-wide patterns.
- **Secret Service:** Use `libsecret` (via `ashpd` or `libsecret-rs`) to securely store authentication tokens for the sync server.
- **Search Provider:** Implement a GNOME Shell Search Provider. This allows users to search for their notes directly from the GNOME Activities overview (refer to `iotas` for implementation).
- **Desktop Portal:** Use `XDG Desktop Portals` for file picking and notifications to ensure compatibility with Flatpak sandboxing.

## 6. Build & Distribution

- **Build System:** **Meson**. It is the standard for GNOME projects. It handles Rust compilation (via `cargo`), Blueprint compilation, GResource bundling, and installation of desktop/icon files.
- **Packaging:** **Flatpak**. The primary way to distribute GNOME apps. Use `flatpak-builder` with a manifest file (e.g., `com.amadeuz.Notes.json`).
- **App ID:** Use a reverse-DNS identifier: `com.amadeuz.Notes` (or `org.gnome.World.Amadeuz` if aiming for GNOME World).

## 7. GNOME Community & Project Standards

Beyond the code, GNOME Circle apps are expected to follow specific community practices:

- **Licensing:** Use a standard Open Source license, typically **GPL-3.0-or-later**. Ensure every file has a proper license header (SPDX format is preferred).
- **CI/CD:** Use **GitLab CI** (standard for GNOME projects). Your `.gitlab-ci.yml` should include stages for:
  - Linting (e.g., `cargo fmt --check`).
  - Building (via Meson).
  - Flatpak bundling and testing.
- **Documentation:** Include a `CONTRIBUTING.md` and a `HACKING.md` to help newcomers set up their development environment.
- **App Icons:** Follow the [GNOME Icon Design Guidelines](https://developer.gnome.org/hig/reference/icon-design.html). Use high-quality SVG icons.

## 8. Comparison Table: Current vs. Recommended

| Feature | Current (C++) | Recommended (Rust/GNOME 2026) |
| :--- | :--- | :--- |
| **Language** | C++20 | Rust 2021+ |
| **UI Toolkit** | GTK4 | GTK4 + Libadwaita |
| **Build System** | CMake | Meson + Cargo |
| **UI Files** | Hardcoded/XML | Blueprint (`.blp`) |
| **Sync Logic** | libsoup (Manual) | Tokio + reqwest/tokio-tungstenite |
| **Storage** | Manual JSON in `~/.local` | GSettings (config) + SQLite/Flat Files (data) |
| **Packaging** | Native Binaries | Flatpak |
| **I18n** | None | gettext (`.po` files) |
| **Accessibility** | Basic | High (Libadwaita + AdwPropertyAnimation) |

## 8. Specific Recommendations for Amadeuz

1. **Move from Single-Note to Multiple-Notes:** Modern note apps (like `iotas`) support multiple notes. Use an `AdwNavigationSplitView` with a sidebar for the note list and a main area for the editor.
2. **Markdown Support:** Use `gtksourceview-5` for markdown syntax highlighting in the editor.
3. **Drafting a Roadmap:**
   - **Phase 1:** Set up the Meson + Rust skeleton with a basic Libadwaita window.
   - **Phase 2:** Implement the Note model and local storage (SQLite is recommended for multiple notes).
   - **Phase 3:** Port the WebSocket sync logic to Rust.
   - **Phase 4:** Design the UI using Blueprint, following the `iotas` or `Fragments` layout.
   - **Phase 5:** Add translations and a Flatpak manifest.

By following this path, Amadeuz Notes will not just be a tool, but a polished part of the GNOME ecosystem that users will love to use and contribute to.
