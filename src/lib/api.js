import { invoke } from '@tauri-apps/api/core';

// Thin wrappers over the Rust commands in src-tauri/src/lib.rs.
// Note paths are always relative to the vault, using forward slashes.

export const getVault = () => invoke('get_vault');
export const setVault = (path) => invoke('set_vault', { path });
export const openVaultFolder = () => invoke('open_vault_folder');

export const listNotes = () => invoke('list_notes');
export const readNote = (path) => invoke('read_note', { path });
export const createNote = () => invoke('create_note');
// `base` is the body this window last loaded/saved; if the file changed on disk
// since, the backend keeps that version as a conflict copy (`note-conflict` event).
export const saveNote = (path, body, base) => invoke('save_note', { path, body, base });
export const finalizeNote = (path) => invoke('finalize_note', { path });
export const deleteNote = (path) => invoke('delete_note', { path });
export const findNote = (id) => invoke('find_note', { id });
// Renames the file (same folder); resolves to the note's new NoteMeta.
export const renameNote = (path, name) => invoke('rename_note', { path, name });

// Per-note UI state from <vault>/.sticky/state.json, keyed by note id:
// { pinned, open, onTop, color, x, y, width, height }
export const getNoteStates = () => invoke('get_note_states');
export const updateNoteState = (id, patch) => invoke('update_note_state', { id, patch });
export const setPinned = (path, pinned) => invoke('set_pinned', { path, pinned });
export const popOut = (path) => invoke('pop_out', { path });
export const closeSticky = (id) => invoke('close_sticky', { id });
export const restoreStickies = () => invoke('restore_stickies');

// Quitting: the backend emits `app-quitting`; every window saves, then calls quitReady().
export const quit = () => invoke('quit');
export const quitReady = () => invoke('quit_ready');

// Sticky windows are labelled `sticky-<hex of note id>` (see sticky_label in lib.rs).
export function stickyIdFromLabel(label) {
  if (!label.startsWith('sticky-')) return null;
  const bytes = label.slice(7).match(/../g) ?? [];
  return new TextDecoder().decode(Uint8Array.from(bytes, (h) => parseInt(h, 16)));
}

// Timestamp names given by notes::create, e.g. `2026-10-07 0915` or `2026-10-07 0915 2`.
const AUTO_NAME = /^\d{4}-\d{2}-\d{2} \d{4}( \d+)?$/;

export const stemOf = (path) => path.split('/').pop().replace(/\.md$/i, '');

// Mirrors notes::title_of on the Rust side so the list updates while typing:
// named notes show their name, timestamp-named ones their first line.
export function titleOf(body, path) {
  const stem = stemOf(path);
  if (!AUTO_NAME.test(stem)) return stem;
  for (const line of body.split('\n')) {
    const t = line.trim().replace(/^#+\s*/, '').trim();
    if (t) return t;
  }
  return stem;
}
