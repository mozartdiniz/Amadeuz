'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('api', {
  // File system
  readData:      ()          => ipcRenderer.invoke('fs:read-data'),
  writeData:     (data)      => ipcRenderer.invoke('fs:write-data', data),
  readSettings:  ()          => ipcRenderer.invoke('fs:read-settings'),
  writeSettings: (s)         => ipcRenderer.invoke('fs:write-settings', s),

  // Credentials
  getToken:    ()      => ipcRenderer.invoke('auth:get-token'),
  setToken:    (token) => ipcRenderer.invoke('auth:set-token', token),
  deleteToken: ()      => ipcRenderer.invoke('auth:delete-token'),

  // Blobs
  saveBlob:         (id, data) => ipcRenderer.invoke('blob:save', { id, data }),
  getPendingBlobs:  ()         => ipcRenderer.invoke('blob:get-pending'),
  markBlobUploaded: (id)       => ipcRenderer.invoke('blob:mark-uploaded', id),
  readBlob:         (id)       => ipcRenderer.invoke('blob:read', id),

  // Theme
  getTheme:       ()   => ipcRenderer.invoke('theme:get'),
  onThemeChanged: (cb) => ipcRenderer.on('theme:changed', (_, t) => cb(t)),
});
