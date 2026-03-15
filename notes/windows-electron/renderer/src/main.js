// Renderer entry point — bootstraps the app.

import { init, subscribe, state, flushNow, remoteSeq } from './state.js';
import { wireEvents, render } from './ui.js';
import { createEditor, setEditorContent } from './editor.js';

// Track which note is currently loaded in TipTap so we push content only when:
//  (a) the selected note changes, or
//  (b) a remote update arrives (remoteSeq increments).
// We never push on local flushes — that would reset the cursor.
let _lastPushedNoteId  = null;
let _lastRemoteSeq     = 0;

async function bootstrap() {
  // Apply system theme before rendering anything
  const theme = await window.api.getTheme();
  document.documentElement.dataset.theme = theme;
  window.api.onThemeChanged((t) => { document.documentElement.dataset.theme = t; });

  // Mount TipTap — the mount element exists in static HTML
  createEditor(document.getElementById('tiptap-mount'));

  // Wire all event handlers (once)
  wireEvents();

  // Initialize state — loads local data + token, triggers fullSync if authenticated
  await init();

  // Subscribe: re-render UI and sync TipTap when state changes
  subscribe((s) => {
    render(s);

    if (s.selectedNoteId !== _lastPushedNoteId) {
      // Note selection changed — load the new note into TipTap.
      setEditorContent(s.editingContent);
      _lastPushedNoteId = s.selectedNoteId;
      _lastRemoteSeq    = remoteSeq;
    } else if (remoteSeq !== _lastRemoteSeq) {
      // A remote update arrived — push the new content.
      setEditorContent(s.editingContent);
      _lastRemoteSeq = remoteSeq;
    }
    // Local flush notifications (debounce fired) are intentionally ignored here
    // so we don't reset TipTap's cursor position.
  });

  // Initial render
  render(state);

  // Flush any pending edit when the window is about to close.
  window.addEventListener('beforeunload', () => flushNow());
}

bootstrap();
