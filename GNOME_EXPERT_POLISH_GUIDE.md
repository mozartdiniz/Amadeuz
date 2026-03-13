# GNOME Expert Polish Guide: The "Secret Ingredients"

Beyond the architecture and UI guidelines, successful GNOME projects like **Fragments** and **iotas** share several professional-grade practices that make them feel like "first-class" software.

## 1. GResource: The "One Binary" Rule
GNOME apps bundle everything into the executable. This avoids broken paths for icons and UI files.

- **Practice:** Use a `gresource.xml` to compile all your `.blp` (Blueprint), `.css`, and `.svg` files into a binary resource.
- **Why:** It ensures the app always looks correct, even when running uninstalled from the build directory.
- **Blueprint Tip:** Set your resource prefix to your App ID (e.g., `/com/amadeuz/Notes/`) to avoid collisions with other apps.

## 2. Accessibility (a11y) as a Feature
Successful GNOME apps are usable by everyone.

- **Practice:** Every interactive element MUST have an `accessible-role` and an `accessible-name` (usually via an `AdwHeaderBar` or `GtkButton` label).
- **Mnemonic:** "If you can't use it with a screen reader, it's not finished."
- **Shortcut:** Use `AdwStatusPage` for empty states—it's pre-configured for accessibility.

## 3. High-Quality Empty States
Apps like `iotas` don't just show a blank screen when there are no notes.

- **Practice:** Use `AdwStatusPage` to explain how to get started.
- **Content:** Include an icon, a title ("No Notes Yet"), and a primary action button ("Create Note").

## 4. Adaptive Styles with CSS
While Libadwaita provides most styles, you will eventually need custom CSS.

- **Practice:** Use `style.css` for general tweaks and `style-dark.css` for dark-mode specific overrides (as seen in **Fragments**).
- **Tip:** Avoid hardcoding colors. Use CSS variables like `@window_bg_color` and `@accent_color` so your app follows system theme changes.

## 5. Keyboard-Centric Design
Power users love GNOME for its keyboard workflow.

- **Practice:** 
  - Implement a "Keyboard Shortcuts" dialog (use `AdwAboutDialog`'s shortcuts feature).
  - Bind common actions: `Ctrl+N` (New), `Ctrl+F` (Search), `Ctrl+W` (Close/Pop).
  - Use `GtkEventControllerKey` to handle complex key events in the editor.

## 6. The Developer Experience (DevEx)
How do other people contribute to your app?

- **Practice:** 
  - Provide a **Flatpak Manifest** in the root directory (`com.amadeuz.Notes.json`).
  - Use **Meson `devenv`**. This allows contributors to run `meson compile && meson devenv` to test the app in a controlled environment without installing anything.
  - **Linting:** Include a `.rustfmt.toml` or `.editorconfig` to enforce style.

## 7. Testing Strategy
- **Unit Tests:** Test your business logic (sync, storage, markdown parsing) independently of GTK.
- **UI Integration Tests:** Use `xvfb-run` in CI to run tests that require a display (creating a window, clicking a button).

---
### Final Thought: The "Aura" of a GNOME App
A successful GNOME app feels "quiet." It doesn't have flashy animations, it doesn't pop up unnecessary windows, and it uses standard system dialogs. When in doubt, **less is more**.
