import { invoke } from '@tauri-apps/api/core';

// Thin wrappers over the Rust commands in src-tauri/src/lib.rs.
// Note paths are always relative to the vault, using forward slashes.

export const getVault = () => invoke('get_vault');
export const setVault = (path) => invoke('set_vault', { path });
export const openVaultFolder = () => invoke('open_vault_folder');

export const listNotes = () => invoke('list_notes');
export const readNote = (path) => invoke('read_note', { path });
export const createNote = () => invoke('create_note');
export const saveNote = (path, body) => invoke('save_note', { path, body });
export const finalizeNote = (path) => invoke('finalize_note', { path });
export const deleteNote = (path) => invoke('delete_note', { path });

// Mirrors notes::title_of on the Rust side so the list updates while typing.
export function titleOf(body, path) {
  for (const line of body.split('\n')) {
    const t = line.trim().replace(/^#+\s*/, '').trim();
    if (t) return t;
  }
  const file = path.split('/').pop();
  return file.replace(/\.md$/i, '');
}
