// REST API client + per-note WebSocket

export class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.status = status;
  }
  get isUnauthorized() { return this.status === 401; }
}

export class ApiClient {
  constructor(baseUrl, token) {
    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.token = token;
  }

  get _headers() {
    const h = { 'Content-Type': 'application/json' };
    if (this.token) h['Authorization'] = `Bearer ${this.token}`;
    return h;
  }

  async _req(method, path, body) {
    const resp = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: this._headers,
      body: body != null ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 401) throw new ApiError('Unauthorized', 401);
    if (!resp.ok) {
      const msg = await resp.text().catch(() => resp.statusText);
      throw new ApiError(msg, resp.status);
    }
    if (resp.status === 204) return null;
    return resp.json();
  }

  // Auth
  login(email, password)                             { return this._req('POST', '/auth/login',   { email, password }); }
  register(email, password)                          { return this._req('POST', '/auth/register', { email, password }); }
  recover(email, recovery_code, new_password)        { return this._req('POST', '/auth/recover',  { email, recovery_code, new_password }); }

  // Folders
  listFolders()            { return this._req('GET',    '/folders').then(r => r.folders); }
  createFolder(id, name, created_at) { return this._req('POST',   '/folders',       { id, name, created_at }).then(r => r.folder); }
  renameFolder(id, name)   { return this._req('PATCH',  `/folders/${id}`, { name }).then(r => r.folder); }
  deleteFolder(id)         { return this._req('DELETE', `/folders/${id}`); }

  // Notes
  listNotes() { return this._req('GET', '/notes').then(r => r.notes); }
  createNote(id, folder_id, title, content, updated_at, created_at) {
    return this._req('POST', '/notes', { id, folder_id, title, content, updated_at, created_at }).then(r => r.note);
  }
  updateNote(id, title, content, updated_at) {
    return this._req('PATCH', `/notes/${id}`, { title, content, updated_at }).then(r => r.note);
  }
  moveNote(id, folder_id)  { return this._req('PATCH',  `/notes/${id}/move`,    { folder_id }).then(r => r.note); }
  trashNote(id)            { return this._req('PATCH',  `/notes/${id}/trash`); }
  restoreNote(id)          { return this._req('PATCH',  `/notes/${id}/restore`); }
  deleteNote(id)           { return this._req('DELETE', `/notes/${id}`); }

  // Blobs
  async uploadBlob(id, data) {
    const resp = await fetch(`${this.baseUrl}/blobs/${id}`, {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${this.token}`,
        'Content-Type': 'application/octet-stream',
      },
      body: data,
    });
    if (!resp.ok && resp.status !== 200) throw new ApiError(`Blob upload failed`, resp.status);
  }

  wsUrl(noteId) {
    const base = this.baseUrl.replace(/^https?:\/\//, (m) => m.startsWith('https') ? 'wss://' : 'ws://');
    return `${base}/notes/${noteId}/ws?token=${this.token}`;
  }
}

// Per-note WebSocket with 3-second auto-reconnect.
export class NoteSync {
  constructor(url, { onInit, onUpdate, onStatusChange }) {
    this.url = url;
    this._onInit = onInit;
    this._onUpdate = onUpdate;
    this._onStatusChange = onStatusChange;
    this._ws = null;
    this._destroyed = false;
    this._timer = null;
    this._connect();
  }

  _connect() {
    if (this._destroyed) return;
    this._ws = new WebSocket(this.url);

    this._ws.onopen = () => this._onStatusChange?.(true);

    this._ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data);
        if      (msg.type === 'init')   this._onInit?.(msg);
        else if (msg.type === 'update') this._onUpdate?.(msg);
      } catch {}
    };

    this._ws.onclose = () => {
      this._onStatusChange?.(false);
      if (!this._destroyed) this._timer = setTimeout(() => this._connect(), 3000);
    };

    this._ws.onerror = () => this._ws.close();
  }

  send(msg) {
    if (this._ws?.readyState === WebSocket.OPEN) this._ws.send(JSON.stringify(msg));
  }

  destroy() {
    this._destroyed = true;
    clearTimeout(this._timer);
    this._ws?.close();
    this._ws = null;
  }
}
