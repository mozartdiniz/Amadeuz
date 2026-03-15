// TipTap editor — content-only (title lives in a separate <input>).

import { Editor }   from '@tiptap/core';
import StarterKit   from '@tiptap/starter-kit';
import Image        from '@tiptap/extension-image';
import TaskList     from '@tiptap/extension-task-list';
import TaskItem     from '@tiptap/extension-task-item';
import { Markdown } from 'tiptap-markdown';

import { onContentChange, saveBlob } from './state.js';

let _editor   = null;
let _suppress = false; // block onUpdate during programmatic setContent

export function createEditor(mountEl) {
  _editor = new Editor({
    element: mountEl,
    extensions: [
      StarterKit,
      Markdown.configure({
        html:                 false,
        tightLists:           true,
        bulletListMarker:     '-',
        linkify:              false,
        breaks:               false,
        transformPastedText:  true,
        transformCopiedText:  false,
      }),
      Image.configure({ inline: true, allowBase64: false }),
      TaskList,
      TaskItem.configure({ nested: false }),
    ],
    editable: false,
    editorProps: {
      attributes: { class: 'tiptap-body', spellcheck: 'true' },
    },
    onUpdate: ({ editor }) => {
      if (_suppress) return;
      onContentChange(editor.storage.markdown.getMarkdown());
    },
  });

  // Image paste handler — intercepts before TipTap sees the event.
  mountEl.addEventListener('paste', _handleImagePaste, true);

  // Image drag-and-drop handler.
  mountEl.addEventListener('dragover', (e) => {
    if ([...e.dataTransfer.items].some(i => i.type.startsWith('image/'))) {
      e.preventDefault();
    }
  });
  mountEl.addEventListener('drop', _handleImageDrop);

  return _editor;
}

async function _handleImagePaste(e) {
  if (!_editor?.isEditable) return;
  const items = e.clipboardData?.items;
  if (!items) return;
  for (const item of items) {
    if (item.type.startsWith('image/')) {
      e.preventDefault();
      e.stopImmediatePropagation();
      const file = item.getAsFile();
      if (!file) continue;
      await _insertImageFile(file);
      break;
    }
  }
}

async function _handleImageDrop(e) {
  if (!_editor?.isEditable) return;
  const files = [...(e.dataTransfer?.files ?? [])].filter(f => f.type.startsWith('image/'));
  if (!files.length) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  for (const file of files) {
    await _insertImageFile(file);
  }
}

async function _insertImageFile(file) {
  const blobId = crypto.randomUUID();
  const buf    = await file.arrayBuffer();
  await saveBlob(blobId, new Uint8Array(buf));
  _editor.commands.setImage({ src: `amadeuz://blob/${blobId}` });
}

// Load markdown into the editor (does not trigger onContentChange).
export function setEditorContent(markdown) {
  if (!_editor) return;
  _suppress = true;
  if (!markdown) {
    _editor.commands.clearContent();
  } else {
    _editor.commands.setContent(markdown);
  }
  _suppress = false;
}

export function setEditorEditable(editable) {
  _editor?.setEditable(editable);
}

export function focusEditor() {
  _editor?.commands.focus('end');
}

export function destroyEditor() {
  _editor?.destroy();
  _editor = null;
}
