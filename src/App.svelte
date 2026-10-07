<script>
  import { onMount } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open } from '@tauri-apps/plugin-dialog';
  import * as api from './lib/api.js';
  import Editor from './lib/Editor.svelte';
  import { FolderOpen, Library, Plus, StickyNote, Trash2 } from '@lucide/svelte';

  const SAVE_DELAY_MS = 500;
  // Vite serves this UI on localhost:1420 for the Tauri window; a plain browser has no backend.
  const inTauri = isTauri();

  let vault = $state(null);
  let ready = $state(false);
  let notes = $state([]);
  let query = $state('');
  let current = $state(null); // note open in the editor: { path, id, title, body, modified }
  let loadSeq = $state(0); // bump to make the editor replace its document
  let error = $state('');
  let searchEl = $state();

  // Saves are serialized through `chain` so they never overlap or reorder.
  let dirty = false;
  let timer;
  let chain = Promise.resolve();

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return notes;
    return notes.filter(
      (n) => n.title.toLowerCase().includes(q) || n.body.toLowerCase().includes(q) || n.path.toLowerCase().includes(q),
    );
  });

  const vaultName = $derived(vault ? vault.split(/[\\/]/).filter(Boolean).pop() : '');

  function report(e) {
    error = String(e);
  }

  // Replace a list entry in place, or remove it when `meta` is null.
  function replaceNote(oldPath, meta) {
    const i = notes.findIndex((n) => n.path === oldPath);
    if (i < 0) return;
    if (meta) notes[i] = meta;
    else notes.splice(i, 1);
  }

  function moveToTop(oldPath, meta) {
    replaceNote(oldPath, null);
    notes.unshift(meta);
  }

  async function refresh() {
    if (!vault) return;
    try {
      notes = await api.listNotes();
    } catch (e) {
      report(e);
    }
  }

  // Picks up edits made in Obsidian (or anywhere) when the window regains focus.
  async function syncFromDisk() {
    await chain;
    await refresh();
    if (!current || dirty) return;
    const fresh = notes.find((n) => n.path === current.path);
    if (!fresh) {
      current = null;
    } else if (fresh.body !== current.body) {
      current = { ...fresh };
      loadSeq++;
    }
  }

  function flush() {
    clearTimeout(timer);
    if (!dirty || !current) return chain;
    dirty = false;
    const note = current;
    const body = note.body;
    chain = chain
      .then(async () => {
        const meta = await api.saveNote(note.path, body);
        moveToTop(note.path, meta);
        note.id = meta.id;
        note.modified = meta.modified;
      })
      .catch(report);
    return chain;
  }

  // Called when navigating away from a note: final save, then let the backend
  // name it from its first line or drop it if it was left empty.
  async function leave() {
    if (!current) return;
    const note = current;
    await flush();
    try {
      replaceNote(note.path, await api.finalizeNote(note.path));
    } catch (e) {
      report(e);
    }
  }

  function onEdit(text) {
    current.body = text;
    current.title = api.titleOf(text, current.path);
    const entry = notes.find((n) => n.path === current.path);
    if (entry) {
      entry.body = text;
      entry.title = current.title;
    }
    dirty = true;
    clearTimeout(timer);
    timer = setTimeout(flush, SAVE_DELAY_MS);
  }

  async function openNote(meta) {
    if (!meta || current?.path === meta.path) return;
    await leave();
    try {
      current = await api.readNote(meta.path);
      loadSeq++;
    } catch (e) {
      report(e);
      await refresh();
    }
  }

  async function newNote() {
    await leave();
    try {
      const meta = await api.createNote();
      notes.unshift(meta);
      current = { ...meta };
      query = '';
      loadSeq++;
    } catch (e) {
      report(e);
    }
  }

  async function deleteCurrent() {
    if (!current) return;
    clearTimeout(timer);
    dirty = false;
    const path = current.path;
    await chain;
    try {
      await api.deleteNote(path);
      replaceNote(path, null);
      current = null;
    } catch (e) {
      report(e);
    }
  }

  async function pickVault() {
    const dir = await open({ directory: true, title: 'Choose a folder for your notes' });
    if (!dir) return;
    await leave();
    current = null;
    try {
      vault = await api.setVault(dir);
      await refresh();
    } catch (e) {
      report(e);
    }
  }

  function onKey(e) {
    if (!(e.ctrlKey || e.metaKey) || !vault) return;
    const key = e.key.toLowerCase();
    if (key === 'n') {
      e.preventDefault();
      newNote();
    } else if (key === 'k' || (key === 'f' && !e.shiftKey)) {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    } else if (key === 's') {
      e.preventDefault();
      flush();
    }
  }

  function onSearchKey(e) {
    if (e.key === 'Enter') openNote(filtered[0]);
    else if (e.key === 'Escape') query = '';
  }

  function ago(ms) {
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)}m`;
    if (s < 86400) return `${Math.floor(s / 3600)}h`;
    if (s < 86400 * 7) return `${Math.floor(s / 86400)}d`;
    return new Date(ms).toLocaleDateString();
  }

  function preview(body) {
    const lines = body.split('\n').map((l) => l.trim()).filter(Boolean);
    return lines.slice(1, 3).join(' · ');
  }

  onMount(() => {
    if (!inTauri) return;
    const win = getCurrentWindow();
    const unlisten = [
      win.onFocusChanged(({ payload: focused }) => (focused ? syncFromDisk() : flush())),
      // The window closes once this handler resolves, so pending edits land first.
      win.onCloseRequested(() => leave()),
    ];

    (async () => {
      try {
        vault = await api.getVault();
        await refresh();
      } catch (e) {
        report(e);
      }
      ready = true;
    })();

    return () => unlisten.forEach((p) => p.then((f) => f()));
  });
</script>

<svelte:window onkeydown={onKey} />

{#if !inTauri}
  <div class="welcome">
    <StickyNote size={48} strokeWidth={1.5} />
    <h1>Sticky Vault</h1>
    <p>This page only works inside the app window. Run <code>npm run tauri dev</code> and use the window it opens.</p>
  </div>
{:else if !ready}
  <div class="empty"></div>
{:else if !vault}
  <div class="welcome">
    <StickyNote size={48} strokeWidth={1.5} />
    <h1>Sticky Vault</h1>
    <p>Pick a folder. Every note becomes a <code>.md</code> file inside it, so an Obsidian vault works too.</p>
    <button class="primary" onclick={pickVault}><FolderOpen size={16} /> Choose folder…</button>
  </div>
{:else}
  <div class="app">
    <aside>
      <header>
        <button class="vault" title="{vault} (click to change)" onclick={pickVault}>
          <Library size={16} /> <span>{vaultName}</span>
        </button>
        <button class="primary" title="New note (Ctrl/Cmd+N)" onclick={newNote}><Plus size={16} /> New</button>
      </header>
      <input
        class="search"
        type="search"
        placeholder="Search notes  (Ctrl/Cmd+K)"
        bind:this={searchEl}
        bind:value={query}
        onkeydown={onSearchKey}
      />
      <ul>
        {#each filtered as note (note.path)}
          <li>
            <button class:active={current?.path === note.path} onclick={() => openNote(note)}>
              <span class="row">
                <span class="title">{note.title}</span>
                <span class="when">{ago(note.modified)}</span>
              </span>
              {#if preview(note.body)}<span class="preview">{preview(note.body)}</span>{/if}
            </button>
          </li>
        {:else}
          <li class="none">{query ? 'No matches' : 'No notes yet. Press Ctrl/Cmd+N'}</li>
        {/each}
      </ul>
    </aside>

    <main>
      {#if current}
        <div class="toolbar">
          <span class="path" title={current.path}>{current.path}</span>
          <button class="icon" title="Open notes folder" onclick={() => api.openVaultFolder().catch(report)}>
            <FolderOpen size={16} />
          </button>
          <button class="icon danger" title="Move to trash" onclick={deleteCurrent}>
            <Trash2 size={16} />
          </button>
        </div>
        <Editor doc={current.body} loadKey={loadSeq} onchange={onEdit} />
      {:else}
        <div class="placeholder">
          <StickyNote size={40} strokeWidth={1.25} />
          <p>Select a note or press <kbd>Ctrl/Cmd</kbd>+<kbd>N</kbd></p>
        </div>
      {/if}
    </main>
  </div>
{/if}

{#if error}
  <button class="error" onclick={() => (error = '')} title="Dismiss">{error}</button>
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100vh;
  }
  aside {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--line);
    background: var(--side);
    min-width: 0;
  }
  header {
    display: flex;
    gap: 8px;
    padding: 12px;
  }
  .vault {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    justify-content: flex-start;
    background: transparent;
    border-color: transparent;
  }
  .vault span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vault:hover {
    border-color: var(--line);
  }
  .search {
    margin: 0 12px 8px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 6px 12px;
    overflow-y: auto;
    flex: 1;
  }
  li button {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    padding: 8px 10px;
    border-radius: 6px;
  }
  li button:hover {
    background: var(--hover);
  }
  li button.active {
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    font-size: 11px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .preview {
    display: block;
    margin-top: 2px;
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    padding: 16px 10px;
    color: var(--muted);
    font-size: 13px;
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--paper);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  .path {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--muted);
    font-family: var(--mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .placeholder,
  .welcome {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    text-align: center;
    padding: 24px;
  }
  .welcome {
    height: 100vh;
    color: var(--ink);
  }
  .welcome h1 {
    margin: 0 0 8px;
  }
  .welcome p {
    color: var(--muted);
    max-width: 420px;
    margin: 0 0 20px;
  }
  .error {
    position: fixed;
    right: 16px;
    bottom: 16px;
    max-width: 420px;
    background: var(--danger);
    color: white;
    border: none;
    text-align: left;
  }
</style>
