'use strict';

const { app, BrowserWindow, ipcMain, protocol, nativeTheme, net, session, safeStorage } = require('electron');
const path = require('path');
const fs = require('fs');

// ── Paths ─────────────────────────────────────────────────────────────────────

const DATA_DIR   = path.join(app.getPath('userData'), 'amadeuz');
const DATA_FILE  = path.join(DATA_DIR, 'data.json');
const SETTINGS_FILE = path.join(DATA_DIR, 'settings.json');
const TOKEN_FILE = path.join(DATA_DIR, 'token.dat');
const BLOBS_DIR  = path.join(DATA_DIR, 'blobs');
const PENDING_FILE = path.join(DATA_DIR, 'pending_blobs.json');

function ensureDirs() {
  fs.mkdirSync(DATA_DIR,  { recursive: true });
  fs.mkdirSync(BLOBS_DIR, { recursive: true });
}

// Register amadeuz:// as a trusted scheme before app is ready.
protocol.registerSchemesAsPrivileged([
  { scheme: 'amadeuz', privileges: { secure: true, standard: true, supportFetchAPI: true } },
]);

// ── Window ────────────────────────────────────────────────────────────────────

let mainWindow = null;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#1c1c1e' : '#f2f2f7',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  const isDev = process.env.NODE_ENV === 'development';
  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools();
  } else {
    mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'));
  }
}

// ── App lifecycle ─────────────────────────────────────────────────────────────

app.whenReady().then(() => {
  ensureDirs();

  // Serve amadeuz://blob/<id> from local blobs directory.
  protocol.handle('amadeuz', (request) => {
    const url = new URL(request.url);
    const blobId = url.pathname.replace(/^\//, '');
    const filePath = path.join(BLOBS_DIR, blobId);
    return net.fetch(require('url').pathToFileURL(filePath).toString());
  });

  // Inject CORS headers into all server responses so fetch() works from the
  // renderer regardless of what Origin Chromium sends.
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Access-Control-Allow-Origin':  ['*'],
        'Access-Control-Allow-Headers': ['*'],
        'Access-Control-Allow-Methods': ['GET, POST, PATCH, PUT, DELETE, OPTIONS'],
      },
    });
  });

  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

// ── IPC: file system ──────────────────────────────────────────────────────────

ipcMain.handle('fs:read-data', () => {
  try { return JSON.parse(fs.readFileSync(DATA_FILE, 'utf8')); }
  catch { return { folders: [], notes: [] }; }
});

ipcMain.handle('fs:write-data', (_, data) => {
  fs.writeFileSync(DATA_FILE, JSON.stringify(data), 'utf8');
});

ipcMain.handle('fs:read-settings', () => {
  try { return JSON.parse(fs.readFileSync(SETTINGS_FILE, 'utf8')); }
  catch { return { serverAddress: 'http://localhost:8080' }; }
});

ipcMain.handle('fs:write-settings', (_, settings) => {
  fs.writeFileSync(SETTINGS_FILE, JSON.stringify(settings), 'utf8');
});

// ── IPC: credentials (safeStorage = DPAPI on Windows) ────────────────────────

ipcMain.handle('auth:get-token', () => {
  try {
    const encrypted = fs.readFileSync(TOKEN_FILE);
    return safeStorage.decryptString(encrypted);
  } catch { return null; }
});

ipcMain.handle('auth:set-token', (_, token) => {
  const encrypted = safeStorage.encryptString(token);
  fs.writeFileSync(TOKEN_FILE, encrypted);
});

ipcMain.handle('auth:delete-token', () => {
  try { fs.unlinkSync(TOKEN_FILE); } catch {}
});

// ── IPC: blobs ────────────────────────────────────────────────────────────────

ipcMain.handle('blob:save', (_, { id, data }) => {
  fs.writeFileSync(path.join(BLOBS_DIR, id), Buffer.from(data));
  let pending = [];
  try { pending = JSON.parse(fs.readFileSync(PENDING_FILE, 'utf8')); } catch {}
  if (!pending.includes(id)) {
    pending.push(id);
    fs.writeFileSync(PENDING_FILE, JSON.stringify(pending));
  }
});

ipcMain.handle('blob:get-pending', () => {
  try { return JSON.parse(fs.readFileSync(PENDING_FILE, 'utf8')); }
  catch { return []; }
});

ipcMain.handle('blob:mark-uploaded', (_, id) => {
  try {
    let pending = JSON.parse(fs.readFileSync(PENDING_FILE, 'utf8'));
    fs.writeFileSync(PENDING_FILE, JSON.stringify(pending.filter(p => p !== id)));
  } catch {}
});

ipcMain.handle('blob:read', (_, id) => {
  try { return fs.readFileSync(path.join(BLOBS_DIR, id)); }
  catch { return null; }
});

// ── IPC: theme ────────────────────────────────────────────────────────────────

ipcMain.handle('theme:get', () => (nativeTheme.shouldUseDarkColors ? 'dark' : 'light'));

nativeTheme.on('updated', () => {
  mainWindow?.webContents.send('theme:changed', nativeTheme.shouldUseDarkColors ? 'dark' : 'light');
});
