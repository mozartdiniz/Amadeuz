// All DOM manipulation — called once to wire event handlers, then on every
// state change to update only the parts that differ.

import {
  state, ALL_NOTES, TRASH,
  login, register, recover, logout,
  setSelectedFolder, createFolder, renameFolder, deleteFolder,
  selectNote, createNote, trashNote, restoreNote, permanentlyDeleteNote, moveNote,
  flushNow, setSearchQuery, updateServerAddress,
  notesForFolder, noteSections, folderName,
} from './state.js';

import { setEditorContent, setEditorEditable, focusEditor } from './editor.js';

// ── One-time DOM wiring ───────────────────────────────────────────────────────

export function wireEvents() {
  // Auth tabs
  document.querySelectorAll('.auth-tab').forEach(btn => {
    btn.addEventListener('click', () => _switchAuthTab(btn.dataset.tab));
  });

  // Auth forms
  document.getElementById('form-login').addEventListener('submit', async (e) => {
    e.preventDefault();
    const server = document.getElementById('login-server').value.trim();
    if (server) updateServerAddress(server);
    await _authAction(() => login(
      document.getElementById('login-email').value.trim(),
      document.getElementById('login-password').value,
    ));
  });

  document.getElementById('form-register').addEventListener('submit', async (e) => {
    e.preventDefault();
    const server = document.getElementById('reg-server').value.trim();
    if (server) updateServerAddress(server);
    await _authAction(() => register(
      document.getElementById('reg-email').value.trim(),
      document.getElementById('reg-password').value,
    ));
  });

  document.getElementById('form-recover').addEventListener('submit', async (e) => {
    e.preventDefault();
    const server = document.getElementById('rec-server').value.trim();
    if (server) updateServerAddress(server);
    await _authAction(() => recover(
      document.getElementById('rec-email').value.trim(),
      document.getElementById('rec-code').value.trim(),
      document.getElementById('rec-password').value,
    ));
  });

  // Sidebar buttons
  document.getElementById('btn-new-folder').addEventListener('click', () => {
    _promptDialog('New Folder', '', name => { if (name?.trim()) createFolder(name.trim()); });
  });

  document.getElementById('btn-sign-out').addEventListener('click', () => logout());

  document.getElementById('btn-settings').addEventListener('click', () => {
    document.getElementById('settings-server-url').value = state.serverAddress;
    _show('dlg-settings');
  });

  // New note
  document.getElementById('btn-new-note').addEventListener('click', () => createNote());

  // Search
  document.getElementById('search-input').addEventListener('input', (e) => {
    setSearchQuery(e.target.value);
  });

  // Settings dialog
  document.getElementById('btn-settings-save').addEventListener('click', () => {
    const url = document.getElementById('settings-server-url').value.trim();
    if (url) updateServerAddress(url);
    _hide('dlg-settings');
  });
  document.getElementById('btn-settings-cancel').addEventListener('click', () => _hide('dlg-settings'));

  // Recovery code dialog
  document.getElementById('btn-copy-recovery').addEventListener('click', () => {
    const code = document.getElementById('recovery-code-text').textContent;
    navigator.clipboard.writeText(code);
  });
  document.getElementById('btn-recovery-done').addEventListener('click', () => {
    state.pendingRecovery = null;
    _hide('dlg-recovery');
  });

  // Move note dialog
  document.getElementById('btn-move-cancel').addEventListener('click', () => _hide('dlg-move'));

  // Prompt dialog (folder rename)
  document.getElementById('btn-prompt-cancel').addEventListener('click', () => _hide('dlg-prompt'));
  document.getElementById('dlg-prompt-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') document.getElementById('btn-prompt-ok').click();
  });

  // Hide dialogs on overlay click
  document.querySelectorAll('.dialog-overlay').forEach(el => {
    el.addEventListener('click', (e) => { if (e.target === el) _hide(el.id); });
  });

  // Hide context menu on any click
  document.addEventListener('click', () => _hideContextMenu());
}

// ── Render (called on every state change) ────────────────────────────────────

let _prevEditorNoteId = null;

export function render(s) {
  _renderAuthGate(s);
  if (!s.isAuthenticated) return;

  _renderConnectionDot(s);
  _renderFolderList(s);
  _renderFolderTitle(s);
  _renderNoteList(s);
  _renderEditor(s);
  _renderRecoveryDialog(s);
}

// ── Auth gate ─────────────────────────────────────────────────────────────────

function _renderAuthGate(s) {
  if (s.isAuthenticated) {
    _hide('auth-view');
    _show('main-view');
  } else {
    _show('auth-view');
    _hide('main-view');
    // Pre-fill server address
    const inputs = ['login-server', 'reg-server', 'rec-server'];
    inputs.forEach(id => {
      const el = document.getElementById(id);
      if (!el.value) el.value = s.serverAddress;
    });
  }
}

// ── Connection dot ────────────────────────────────────────────────────────────

function _renderConnectionDot(s) {
  const dot = document.getElementById('conn-dot');
  dot.className = 'conn-dot ' + (s.isConnected ? 'online' : 'offline');
  dot.title     = s.isConnected ? 'Connected' : 'Offline';
}

// ── Folder list ───────────────────────────────────────────────────────────────

function _renderFolderList(s) {
  const list = document.getElementById('folder-list');
  const searchQuery = document.getElementById('search-input').value;

  // Build items: All Notes, user folders, Trash
  const items = [
    { id: ALL_NOTES, name: 'All Notes', count: s.notes.filter(n => !n.deleted_at).length, special: true },
    ...s.folders.map(f => ({
      id: f.id, name: f.name,
      count: s.notes.filter(n => !n.deleted_at && n.folder_id === f.id).length,
      special: false,
    })),
    { id: TRASH, name: 'Recently Deleted', count: s.notes.filter(n => n.deleted_at).length, special: true },
  ];

  const active = searchQuery ? ALL_NOTES : s.selectedFolderId;

  list.innerHTML = '';
  items.forEach(item => {
    const li = document.createElement('li');
    li.className = 'folder-item' + (item.id === active ? ' selected' : '') + (item.special ? ' sentinel' : '');
    li.dataset.id = item.id;

    const nameSpan = document.createElement('span');
    nameSpan.className = 'folder-name';
    nameSpan.textContent = item.name;

    const badge = document.createElement('span');
    badge.className = 'folder-badge';
    if (item.count > 0) badge.textContent = item.count;

    li.append(nameSpan, badge);

    li.addEventListener('click', () => {
      document.getElementById('search-input').value = '';
      setSelectedFolder(item.id);
    });

    if (!item.special) {
      li.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        _showContextMenu(e.clientX, e.clientY, [
          { label: 'Rename', action: () => _promptDialog('Rename Folder', item.name, name => {
            if (name?.trim()) renameFolder(item.id, name.trim());
          })},
          { label: 'Delete', danger: true, action: () => deleteFolder(item.id) },
        ]);
      });
    }

    list.appendChild(li);
  });
}

// ── Folder title ──────────────────────────────────────────────────────────────

function _renderFolderTitle(s) {
  const label = document.getElementById('folder-title-label');
  const newBtn = document.getElementById('btn-new-note');
  if      (s.selectedFolderId === ALL_NOTES) { label.textContent = 'All Notes'; }
  else if (s.selectedFolderId === TRASH)     { label.textContent = 'Recently Deleted'; }
  else {
    label.textContent = s.folders.find(f => f.id === s.selectedFolderId)?.name ?? 'Notes';
  }
  newBtn.style.visibility = s.selectedFolderId === TRASH ? 'hidden' : 'visible';
}

// ── Note list ─────────────────────────────────────────────────────────────────

function _renderNoteList(s) {
  const container = document.getElementById('note-list');
  const searchQuery = document.getElementById('search-input').value;
  const isTrash = s.selectedFolderId === TRASH;
  const notes = notesForFolder(s.selectedFolderId, searchQuery);

  if (notes.length === 0) {
    container.innerHTML = '<p class="note-list-empty">No notes</p>';
    return;
  }

  const sections = noteSections(notes);
  const frag = document.createDocumentFragment();

  sections.forEach(sec => {
    const header = document.createElement('div');
    header.className = 'note-section-header';
    header.textContent = sec.title;
    frag.appendChild(header);

    sec.notes.forEach(note => {
      const item = _makeNoteRow(note, s.selectedNoteId === note.id, isTrash, s);
      frag.appendChild(item);
    });
  });

  container.innerHTML = '';
  container.appendChild(frag);
}

function _makeNoteRow(note, selected, isTrash, s) {
  const item = document.createElement('div');
  item.className = 'note-item' + (selected ? ' selected' : '');
  item.dataset.id = note.id;

  const title = document.createElement('div');
  title.className = 'note-item-title';
  title.textContent = note.title || 'Untitled';

  const preview = document.createElement('div');
  preview.className = 'note-item-preview';
  preview.textContent = _previewText(note.content);

  const meta = document.createElement('div');
  meta.className = 'note-item-meta';

  const dateSpan = document.createElement('span');
  dateSpan.className = 'note-item-date';
  dateSpan.textContent = _formatDate(note.updated_at);

  const folderSpan = document.createElement('span');
  folderSpan.className = 'note-item-folder';
  if (note.folder_id) folderSpan.textContent = folderName(note.folder_id);

  meta.append(folderSpan, dateSpan);
  item.append(title, preview, meta);

  item.addEventListener('click', () => { if (!isTrash) selectNote(note.id); });

  item.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (isTrash) {
      _showContextMenu(e.clientX, e.clientY, [
        { label: 'Restore',           action: () => restoreNote(note.id) },
        { label: 'Delete Permanently', danger: true, action: () => permanentlyDeleteNote(note.id) },
      ]);
    } else {
      const moveItems = [
        { label: '— No folder —', action: () => moveNote(note.id, '') },
        ...s.folders.map(f => ({ label: f.name, action: () => moveNote(note.id, f.id) })),
      ];
      _showContextMenu(e.clientX, e.clientY, [
        { label: 'Move to Folder…', action: () => _showMoveDialog(note.id, s) },
        { label: 'Move to Trash', danger: true, action: () => trashNote(note.id) },
      ]);
    }
  });

  return item;
}

// ── Editor ────────────────────────────────────────────────────────────────────

function _renderEditor(s) {
  if (!s.selectedNoteId || s.selectedFolderId === TRASH) {
    _show('editor-empty');
    _hide('editor-active');
    setEditorEditable(false);
    _prevEditorNoteId = null;
    return;
  }

  _hide('editor-empty');
  _show('editor-active');
  setEditorEditable(true);

  const note = s.notes.find(n => n.id === s.selectedNoteId);
  if (!note) return;

  // Date stamp
  document.getElementById('editor-date').textContent = _formatDateFull(note.updated_at);

  // Focus TipTap when a brand new (empty) note is opened.
  if (s.selectedNoteId !== _prevEditorNoteId) {
    _prevEditorNoteId = s.selectedNoteId;
    if (!note.title && !note.content) {
      setTimeout(() => focusEditor(), 50);
    }
  }
  // Content is pushed into TipTap by main.js subscriber (note change or remote update).
}

// ── Recovery code dialog ──────────────────────────────────────────────────────

function _renderRecoveryDialog(s) {
  if (s.pendingRecovery) {
    document.getElementById('recovery-code-text').textContent = s.pendingRecovery;
    _show('dlg-recovery');
  }
}

// ── Auth action helper ────────────────────────────────────────────────────────

async function _authAction(fn) {
  const errorEl = document.getElementById('auth-error');
  errorEl.classList.add('hidden');
  errorEl.textContent = '';
  // Disable all submit buttons
  document.querySelectorAll('.auth-form button[type="submit"]').forEach(b => b.disabled = true);
  try {
    await fn();
  } catch (e) {
    errorEl.textContent = e.message ?? 'Unknown error';
    errorEl.classList.remove('hidden');
  } finally {
    document.querySelectorAll('.auth-form button[type="submit"]').forEach(b => b.disabled = false);
  }
}

function _switchAuthTab(tab) {
  document.querySelectorAll('.auth-tab').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  document.querySelectorAll('.auth-form').forEach(f => f.classList.toggle('hidden', f.dataset.tab !== tab));
  document.getElementById('auth-error').classList.add('hidden');
}

// ── Context menu ──────────────────────────────────────────────────────────────

function _showContextMenu(x, y, items) {
  const menu = document.getElementById('context-menu');
  menu.innerHTML = '';
  items.forEach(item => {
    const div = document.createElement('div');
    div.className = 'ctx-item' + (item.danger ? ' danger' : '');
    div.textContent = item.label;
    div.addEventListener('click', (e) => { e.stopPropagation(); item.action(); _hideContextMenu(); });
    menu.appendChild(div);
  });
  // Clamp to viewport
  menu.classList.remove('hidden');
  const mw = menu.offsetWidth, mh = menu.offsetHeight;
  menu.style.left = Math.min(x, window.innerWidth  - mw - 8) + 'px';
  menu.style.top  = Math.min(y, window.innerHeight - mh - 8) + 'px';
}

function _hideContextMenu() {
  document.getElementById('context-menu').classList.add('hidden');
}

// ── Move dialog ───────────────────────────────────────────────────────────────

function _showMoveDialog(noteId, s) {
  const list = document.getElementById('move-folder-list');
  list.innerHTML = '';

  const options = [
    { id: '', name: '— No folder —' },
    ...s.folders.map(f => ({ id: f.id, name: f.name })),
  ];

  options.forEach(opt => {
    const li = document.createElement('li');
    li.className = 'move-folder-item';
    li.textContent = opt.name;
    li.addEventListener('click', () => { moveNote(noteId, opt.id); _hide('dlg-move'); });
    list.appendChild(li);
  });

  _show('dlg-move');
}

// ── Prompt dialog ─────────────────────────────────────────────────────────────

function _promptDialog(title, defaultValue, onOk) {
  document.getElementById('dlg-prompt-title').textContent = title;
  const input = document.getElementById('dlg-prompt-input');
  input.value = defaultValue ?? '';
  _show('dlg-prompt');
  setTimeout(() => { input.focus(); input.select(); }, 50);

  const okBtn = document.getElementById('btn-prompt-ok');
  const handler = () => {
    okBtn.removeEventListener('click', handler);
    _hide('dlg-prompt');
    onOk(input.value);
  };
  okBtn.addEventListener('click', handler);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function _show(id) { document.getElementById(id)?.classList.remove('hidden'); }
function _hide(id) { document.getElementById(id)?.classList.add('hidden'); }

function _previewText(content) {
  if (!content) return '';
  // Strip markdown syntax characters for a clean preview
  return content
    .replace(/!\[.*?\]\(.*?\)/g, '[image]')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/#{1,6}\s*/g, '')
    .replace(/[*_~`]/g, '')
    .replace(/\n+/g, ' ')
    .trim()
    .slice(0, 100);
}

function _formatDate(ms) {
  if (!ms) return '';
  const d   = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

function _formatDateFull(ms) {
  if (!ms) return '';
  return new Date(ms).toLocaleString([], {
    day: 'numeric', month: 'long', year: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}
