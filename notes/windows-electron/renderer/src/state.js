// App state — in-memory store with IPC-backed persistence.
// All actions are here; UI only calls exported functions and subscribes to state.

import { ApiClient, ApiError, NoteSync } from './api.js';

export const ALL_NOTES = '__all__';
export const TRASH     = '__wastebasket__';

// Incremented each time a remote update arrives — lets main.js detect when
// to push new content into TipTap without false-positives from local flushes.
export let remoteSeq = 0;

// ── State object ──────────────────────────────────────────────────────────────

export const state = {
  isAuthenticated:   false,
  pendingRecovery:   null,   // recovery code to show once after register/recover

  folders:           [],
  notes:             [],

  selectedFolderId:  ALL_NOTES,
  selectedNoteId:    null,

  // editingContent holds the combined "title\n\nbody" that lives in TipTap.
  // title and body are split out again in _flush() before saving / sending.
  editingContent:    '',
  editingNoteId:     null,

  isConnected:       false,
  serverAddress:     'http://localhost:8080',
};

let _api       = null;
let _noteSync  = null;
let _debounce  = null;
let _syncTimer = null;
let _syncing   = false;
let _listeners = [];

// ── Subscriptions ─────────────────────────────────────────────────────────────

export function subscribe(fn) {
  _listeners.push(fn);
  return () => { _listeners = _listeners.filter(l => l !== fn); };
}

function _notify() { _listeners.forEach(fn => fn(state)); }

// ── Bootstrap ─────────────────────────────────────────────────────────────────

export async function init() {
  const [settings, token] = await Promise.all([
    window.api.readSettings(),
    window.api.getToken(),
  ]);

  state.serverAddress = _normalize(settings?.serverAddress ?? 'http://localhost:8080');
  _api = new ApiClient(state.serverAddress, token);

  const local = await window.api.readData();
  state.folders = local.folders ?? [];
  state.notes   = local.notes   ?? [];

  if (token) {
    state.isAuthenticated = true;
    fullSync(); // background
  }

  _startPeriodicSync();

  // Auto-select the most recently edited note in All Notes on startup.
  const first = state.notes
    .filter(n => !n.deleted_at)
    .sort((a, b) => b.updated_at - a.updated_at)[0];
  if (first) selectNote(first.id);

  _notify();
}

// ── Auth ──────────────────────────────────────────────────────────────────────

export async function login(email, password) {
  const resp = await _api.login(email, password);
  await _applyToken(resp.token);
  await fullSync();
}

export async function register(email, password) {
  const resp = await _api.register(email, password);
  state.pendingRecovery = resp.recovery_code;
  await _applyToken(resp.token);
  await fullSync();
}

export async function recover(email, code, newPassword) {
  const resp = await _api.recover(email, code, newPassword);
  state.pendingRecovery = resp.recovery_code;
  await _applyToken(resp.token);
  await fullSync();
}

export async function logout() {
  await window.api.deleteToken();
  _noteSync?.destroy();
  _noteSync = null;
  _api.token = null;
  Object.assign(state, {
    isAuthenticated: false,
    folders: [], notes: [],
    selectedFolderId: ALL_NOTES,
    selectedNoteId: null,
    editingContent: '', editingNoteId: null,
    isConnected: false,
  });
  await window.api.writeData({ folders: [], notes: [] });
  _notify();
}

async function _applyToken(token) {
  await window.api.setToken(token);
  _api.token = token;
  Object.assign(state, {
    isAuthenticated: true,
    folders: [], notes: [],
    editingContent: '', editingNoteId: null,
    selectedNoteId: null,
  });
  await window.api.writeData({ folders: [], notes: [] });
}

// ── Full sync ─────────────────────────────────────────────────────────────────

export async function fullSync() {
  if (_syncing || !state.isAuthenticated) return;
  // Commit any in-flight edit before merging so we don't lose keystrokes.
  flushNow();
  _syncing = true;
  try {
    const [serverFolders, serverNotes] = await Promise.all([
      _api.listFolders(),
      _api.listNotes(),
    ]);

    const serverFolderIds = new Set(serverFolders.map(f => f.id));
    const serverNoteMap   = new Map(serverNotes.map(n => [n.id, n]));
    const localNoteIds    = new Set(state.notes.map(n => n.id));

    const localOnlyFolders = state.folders.filter(f => !serverFolderIds.has(f.id));
    const localOnlyNotes   = state.notes.filter(n => !serverNoteMap.has(n.id) && !n.deleted_at);
    const newerLocalNotes  = state.notes.filter(n => {
      const sv = serverNoteMap.get(n.id);
      return sv && n.updated_at > sv.updated_at && !n.deleted_at;
    });

    await Promise.allSettled([
      ...localOnlyFolders.map(f => _api.createFolder(f.id, f.name, f.created_at)),
      ...localOnlyNotes.map(n => _api.createNote(n.id, n.folder_id || null, n.title, n.content, n.updated_at, n.created_at)),
      ...newerLocalNotes.map(n => _api.updateNote(n.id, n.title, n.content, n.updated_at)),
    ]);

    // Merge folders
    state.folders = [...serverFolders, ...localOnlyFolders];

    // Merge notes (last-write-wins per note)
    const merged = [];
    for (const local of state.notes) {
      const sv = serverNoteMap.get(local.id);
      merged.push(sv ? (local.updated_at > sv.updated_at ? local : sv) : local);
    }
    for (const sv of serverNotes) {
      if (!localNoteIds.has(sv.id)) merged.push(sv);
    }
    state.notes = merged;

    // Refresh editor if the currently-open note was updated from the server.
    if (state.editingNoteId) {
      const n = state.notes.find(n => n.id === state.editingNoteId);
      if (n) {
        const combined = _combinedContent(n);
        if (combined !== state.editingContent) {
          state.editingContent = combined;
          remoteSeq++;
        }
      }
    }

    await window.api.writeData({ folders: state.folders, notes: state.notes });
    state.isConnected = true;
    _uploadPendingBlobs();
    _connectNoteSync();
  } catch (e) {
    if (e instanceof ApiError && e.isUnauthorized) await logout();
    else state.isConnected = false;
  } finally {
    _syncing = false;
  }
  _notify();
}

function _startPeriodicSync() {
  clearInterval(_syncTimer);
  _syncTimer = setInterval(() => fullSync(), 30_000);
}

// ── Per-note WebSocket ────────────────────────────────────────────────────────

function _connectNoteSync() {
  _noteSync?.destroy();
  _noteSync = null;
  if (!state.selectedNoteId) return;
  _noteSync = new NoteSync(_api.wsUrl(state.selectedNoteId), {
    onInit:           (msg) => _handleRemote(msg),
    onUpdate:         (msg) => _handleRemote(msg),
    onStatusChange:   (ok)  => { state.isConnected = ok; _notify(); },
  });
}

function _handleRemote(msg) {
  const idx = state.notes.findIndex(n => n.id === state.selectedNoteId);
  if (idx === -1 || !msg.updated_at || msg.updated_at <= state.notes[idx].updated_at) return;
  state.notes[idx] = {
    ...state.notes[idx],
    title:      msg.title   ?? state.notes[idx].title,
    content:    msg.content ?? state.notes[idx].content,
    updated_at: msg.updated_at,
  };
  state.editingContent = _combinedContent(state.notes[idx]);
  remoteSeq++;
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _notify();
}

// ── Debounce + flush ──────────────────────────────────────────────────────────

export function onContentChange(content) {
  state.editingContent = content;
  _scheduleFlush();
}

export function flushNow() {
  clearTimeout(_debounce);
  _flush();
}

function _scheduleFlush() {
  clearTimeout(_debounce);
  _debounce = setTimeout(_flush, 500);
}

function _flush() {
  if (!state.editingNoteId) return;
  const title   = _titleFromCombined(state.editingContent);
  const content = _bodyFromCombined(state.editingContent);
  const idx = state.notes.findIndex(n => n.id === state.editingNoteId);
  if (idx === -1) return;
  if (state.notes[idx].title === title && state.notes[idx].content === content) return;

  const now = Date.now();
  state.notes[idx] = { ...state.notes[idx], title, content, updated_at: now };
  window.api.writeData({ folders: state.folders, notes: state.notes });

  const wsMsg = { type: 'update', title, content, updated_at: now };
  _noteSync?.send(wsMsg);
  _api.updateNote(state.editingNoteId, title, content, now).catch(() => {});
  _notify();
}

// ── Folder actions ────────────────────────────────────────────────────────────

export function setSelectedFolder(id) {
  if (state.selectedFolderId === id) return;
  flushNow();
  _noteSync?.destroy();
  _noteSync = null;
  Object.assign(state, {
    selectedFolderId: id,
    selectedNoteId: null,
    editingContent: '', editingNoteId: null,
  });
  _notify();
}

export function createFolder(name) {
  const now = Date.now();
  const folder = { id: crypto.randomUUID(), name, created_at: now };
  state.folders.push(folder);
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.createFolder(folder.id, folder.name, folder.created_at).catch(() => {});
  _notify();
}

export function renameFolder(id, name) {
  const f = state.folders.find(f => f.id === id);
  if (!f) return;
  f.name = name;
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.renameFolder(id, name).catch(() => {});
  _notify();
}

export function deleteFolder(id) {
  state.folders = state.folders.filter(f => f.id !== id);
  state.notes   = state.notes.map(n => n.folder_id === id ? { ...n, folder_id: '' } : n);
  if (state.selectedFolderId === id) {
    Object.assign(state, {
      selectedFolderId: ALL_NOTES,
      selectedNoteId: null,
      editingContent: '', editingNoteId: null,
    });
  }
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.deleteFolder(id).catch(() => {});
  _notify();
}

// ── Note actions ──────────────────────────────────────────────────────────────

export function selectNote(id) {
  if (state.selectedNoteId === id) return;
  flushNow();
  _noteSync?.destroy();
  _noteSync = null;
  state.selectedNoteId = id;
  const n = state.notes.find(n => n.id === id);
  if (n) {
    state.editingContent = _combinedContent(n);
    state.editingNoteId  = n.id;
  } else {
    state.editingContent = '';
    state.editingNoteId  = null;
  }
  _connectNoteSync();
  _notify();
}

export function createNote() {
  if (state.selectedFolderId === TRASH) return;
  const now      = Date.now();
  const folderId = state.selectedFolderId === ALL_NOTES ? '' : (state.selectedFolderId ?? '');
  const note = {
    id: crypto.randomUUID(),
    folder_id: folderId,
    title: '', content: '',
    updated_at: now, created_at: now,
    deleted_at: null,
  };
  flushNow();
  _noteSync?.destroy();
  _noteSync = null;
  state.notes.push(note);
  Object.assign(state, {
    selectedNoteId: note.id,
    editingContent: '', editingNoteId: note.id,
  });
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.createNote(note.id, folderId || null, '', '', now, now).catch(() => {});
  _connectNoteSync();
  _notify();
}

export function trashNote(id) {
  const idx = state.notes.findIndex(n => n.id === id);
  if (idx === -1) return;
  state.notes[idx] = { ...state.notes[idx], deleted_at: Date.now() };
  if (state.selectedNoteId === id) {
    Object.assign(state, { selectedNoteId: null, editingContent: '', editingNoteId: null });
    _noteSync?.destroy(); _noteSync = null;
  }
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.trashNote(id).catch(() => {});
  _notify();
}

export function restoreNote(id) {
  const idx = state.notes.findIndex(n => n.id === id);
  if (idx !== -1) state.notes[idx] = { ...state.notes[idx], deleted_at: null };
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.restoreNote(id).catch(() => {});
  _notify();
}

export function permanentlyDeleteNote(id) {
  state.notes = state.notes.filter(n => n.id !== id);
  if (state.selectedNoteId === id) {
    Object.assign(state, { selectedNoteId: null, editingContent: '', editingNoteId: null });
    _noteSync?.destroy(); _noteSync = null;
  }
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.deleteNote(id).catch(() => {});
  _notify();
}

export function moveNote(id, folderId) {
  const idx = state.notes.findIndex(n => n.id === id);
  if (idx === -1) return;
  const now = Date.now();
  state.notes[idx] = { ...state.notes[idx], folder_id: folderId || '', updated_at: now };
  if (state.selectedFolderId !== ALL_NOTES && state.selectedNoteId === id && folderId !== state.selectedFolderId) {
    Object.assign(state, { selectedNoteId: null, editingContent: '', editingNoteId: null });
  }
  window.api.writeData({ folders: state.folders, notes: state.notes });
  _api.moveNote(id, folderId || null).catch(() => {});
  _notify();
}

export function setSearchQuery(q) {
  if (q && state.selectedFolderId !== ALL_NOTES) state.selectedFolderId = ALL_NOTES;
  _notify();
}

export function updateServerAddress(addr) {
  state.serverAddress = _normalize(addr);
  _api = new ApiClient(state.serverAddress, _api.token);
  window.api.writeSettings({ serverAddress: state.serverAddress });
  if (state.isAuthenticated) fullSync();
  _notify();
}

// ── Blobs ─────────────────────────────────────────────────────────────────────

export async function saveBlob(id, data) {
  await window.api.saveBlob(id, data);
}

async function _uploadPendingBlobs() {
  const pending = await window.api.getPendingBlobs();
  for (const id of pending) {
    try {
      const data = await window.api.readBlob(id);
      if (data) {
        await _api.uploadBlob(id, Buffer.from(data));
        await window.api.markBlobUploaded(id);
      }
    } catch {}
  }
}

// ── Computed ──────────────────────────────────────────────────────────────────

export function notesForFolder(folderId, searchQuery) {
  if (folderId === TRASH) {
    return state.notes.filter(n => n.deleted_at).sort((a, b) => b.updated_at - a.updated_at);
  }
  let notes = state.notes.filter(n => !n.deleted_at);
  if (folderId !== ALL_NOTES) notes = notes.filter(n => (n.folder_id || '') === folderId);
  if (searchQuery) {
    const q = searchQuery.toLowerCase();
    notes = notes.filter(n => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q));
  }
  return notes.sort((a, b) => b.updated_at - a.updated_at);
}

export function noteSections(notes) {
  const now = Date.now();
  const DAY = 86_400_000;
  const buckets = [
    { id: 'today', title: 'Today',            notes: [] },
    { id: 'week',  title: 'Previous 7 Days',  notes: [] },
    { id: 'month', title: 'Previous 30 Days', notes: [] },
    { id: 'older', title: 'Older',            notes: [] },
  ];
  for (const n of notes) {
    const age = now - n.updated_at;
    if      (age < DAY)       buckets[0].notes.push(n);
    else if (age < 7 * DAY)   buckets[1].notes.push(n);
    else if (age < 30 * DAY)  buckets[2].notes.push(n);
    else                      buckets[3].notes.push(n);
  }
  return buckets.filter(b => b.notes.length > 0);
}

export function folderName(folderId) {
  if (!folderId) return '';
  return state.folders.find(f => f.id === folderId)?.name ?? '';
}

// ── Content helpers ───────────────────────────────────────────────────────────

// Build the combined "title\n\nbody" string that goes into TipTap.
function _combinedContent(note) {
  const t = note?.title   ?? '';
  const c = note?.content ?? '';
  if (!t && !c) return '';
  if (!c) return t;
  if (!t) return c;
  return t + '\n\n' + c;
}

// Extract the title from the first line of the combined editor content.
function _titleFromCombined(combined) {
  const first = (combined ?? '').split('\n')[0];
  return first.replace(/^#{1,6}\s*/, '').trim();
}

// Everything after the first line, with leading blank lines stripped.
function _bodyFromCombined(combined) {
  const lines = (combined ?? '').split('\n');
  if (lines.length <= 1) return '';
  return lines.slice(1).join('\n').replace(/^\n+/, '');
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function _normalize(addr) {
  if (!addr) return 'http://localhost:8080';
  addr = addr.trim();
  if (addr.startsWith('ws://') || addr.startsWith('wss://')) {
    addr = addr.replace(/^ws:\/\//, 'http://').replace(/^wss:\/\//, 'https://');
    const wsIdx = addr.indexOf('/ws');
    if (wsIdx !== -1) addr = addr.slice(0, wsIdx);
  }
  return addr.replace(/\/$/, '');
}
