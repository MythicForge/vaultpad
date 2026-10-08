<script>
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import * as api from './api.js';
  import Editor from './Editor.svelte';
  import { Palette, Pin, PinOff, Plus, X } from '@lucide/svelte';

  // One popped-out note in its own frameless window. Placement, color and
  // on-top live in .sticky/state.json via updateNoteState; text is saved
  // exactly like the main editor (debounced, serialized through `chain`).
  let { id } = $props();

  const SAVE_DELAY_MS = 500;
  const GEOMETRY_DELAY_MS = 400;
  const COLORS = ['yellow', 'pink', 'green', 'blue', 'purple', 'gray'];
  const win = getCurrentWindow();

  let note = $state(null); // { path, id, title, body, modified }
  let missing = $state(false);
  let color = $state('yellow');
  let onTop = $state(false);
  let showColors = $state(false);
  let loadSeq = $state(0);
  let error = $state('');

  let dirty = false;
  let timer;
  let geometryTimer;
  let chain = Promise.resolve();

  const title = $derived(note ? api.titleOf(note.body, note.path) : '');

  $effect(() => {
    win.setTitle(title || 'Vault Pad note').catch(() => {});
  });

  function report(e) {
    error = String(e);
  }

  function flush() {
    clearTimeout(timer);
    if (!dirty || !note) return chain;
    dirty = false;
    const n = note;
    const body = n.body;
    chain = chain
      .then(async () => {
        const meta = await api.saveNote(n.path, body);
        n.id = meta.id;
        n.modified = meta.modified;
      })
      .catch(report);
    return chain;
  }

  function onEdit(text) {
    note.body = text;
    dirty = true;
    clearTimeout(timer);
    timer = setTimeout(flush, SAVE_DELAY_MS);
  }

  // Re-read the note (it may have been edited in Obsidian or renamed there).
  async function syncFromDisk() {
    await chain;
    if (dirty) return;
    try {
      let fresh = note ? await api.readNote(note.path).catch(() => null) : null;
      if (fresh?.id !== id) fresh = await api.findNote(id);
      if (!fresh) {
        note = null;
        missing = true;
        return;
      }
      missing = false;
      if (!note || fresh.body !== note.body) {
        note = fresh;
        loadSeq++;
      } else {
        note.path = fresh.path;
      }
    } catch (e) {
      report(e);
    }
  }

  function saveGeometrySoon() {
    clearTimeout(geometryTimer);
    geometryTimer = setTimeout(async () => {
      try {
        const scale = await win.scaleFactor();
        // innerPosition, not outerPosition: on GTK/X11 frameless windows the
        // latter is off by a phantom title bar, so stickies would creep upward.
        const pos = (await win.innerPosition()).toLogical(scale);
        const size = (await win.innerSize()).toLogical(scale);
        await api.updateNoteState(id, { x: pos.x, y: pos.y, width: size.width, height: size.height });
      } catch (e) {
        report(e);
      }
    }, GEOMETRY_DELAY_MS);
  }

  function pickColor(c) {
    color = c;
    showColors = false;
    api.updateNoteState(id, { color: c }).catch(report);
  }

  // New note in a new sticky, cascaded from this one and in the same color.
  async function newSticky() {
    try {
      const meta = await api.createNote();
      const scale = await win.scaleFactor();
      const pos = (await win.innerPosition()).toLogical(scale);
      await api.updateNoteState(meta.id, { color, x: pos.x + 30, y: pos.y + 30 });
      await api.popOut(meta.path);
    } catch (e) {
      report(e);
    }
  }

  async function toggleOnTop() {
    onTop = !onTop;
    try {
      await win.setAlwaysOnTop(onTop);
      await api.updateNoteState(id, { onTop });
    } catch (e) {
      report(e);
    }
  }

  onMount(() => {
    const unlisten = [
      win.onFocusChanged(({ payload: focused }) => (focused ? syncFromDisk() : flush())),
      win.onMoved(saveGeometrySoon),
      win.onResized(saveGeometrySoon),
      // Closing a sticky (its X, Alt+F4, or "dock" from the main window) puts
      // the note back in the list only; blank new notes are dropped as usual.
      win.onCloseRequested(async () => {
        await flush();
        if (note) await api.finalizeNote(note.path).catch(report);
        await api.updateNoteState(id, { open: false }).catch(report);
      }),
    ];

    (async () => {
      try {
        const st = (await api.getNoteStates())[id];
        if (st?.color && COLORS.includes(st.color)) color = st.color;
        onTop = !!st?.onTop;
      } catch (e) {
        report(e);
      }
      await syncFromDisk();
    })();

    return () => unlisten.forEach((p) => p.then((f) => f()));
  });
</script>

<div class="sticky {color}">
  <header data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>{title}</span>
    <button class="icon" title="New sticky note" onclick={newSticky}><Plus size={14} /></button>
    <button class="icon" title="Color" onclick={() => (showColors = !showColors)}><Palette size={14} /></button>
    <button class="icon" class:on={onTop} title={onTop ? 'Stop keeping on top' : 'Keep on top'} onclick={toggleOnTop}>
      {#if onTop}<Pin size={14} />{:else}<PinOff size={14} />{/if}
    </button>
    <button class="icon" title="Close sticky (note stays in your list)" onclick={() => win.close()}>
      <X size={14} />
    </button>
  </header>

  {#if showColors}
    <div class="swatches">
      {#each COLORS as c (c)}
        <button class="swatch {c}" class:current={c === color} title={c} aria-label={c} onclick={() => pickColor(c)}
        ></button>
      {/each}
    </div>
  {/if}

  <main>
    {#if note}
      <Editor doc={note.body} loadKey={loadSeq} onchange={onEdit} />
    {:else if missing}
      <p class="missing">This note was moved out of the vault or deleted.</p>
    {/if}
  </main>

  {#if error}
    <button class="error" onclick={() => (error = '')} title="Dismiss">{error}</button>
  {/if}

  <!-- Fallback resize handle; frameless windows don't always get edge resizing. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" onpointerdown={() => win.startResizeDragging('SouthEast')}></div>
</div>

<style>
  .sticky {
    --paper: var(--note);
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--note);
  }
  /* Light colors first; dark variants below keep text readable in dark mode. */
  .yellow { --note: #fff4a8; --bar: #f8e67c; }
  .pink { --note: #ffd9e4; --bar: #fbbfd1; }
  .green { --note: #d6f5cf; --bar: #bbeab0; }
  .blue { --note: #d4e9ff; --bar: #b6d8fb; }
  .purple { --note: #e7dcff; --bar: #d4c3fb; }
  .gray { --note: #ecebe7; --bar: #dcdad3; }
  @media (prefers-color-scheme: dark) {
    .yellow { --note: #3b361c; --bar: #4a4322; }
    .pink { --note: #3d2730; --bar: #4c303b; }
    .green { --note: #24362a; --bar: #2d4433; }
    .blue { --note: #1f2f40; --bar: #283b50; }
    .purple { --note: #2f2840; --bar: #3a3150; }
    .gray { --note: #2a2926; --bar: #35332f; }
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 30px;
    padding: 0 4px 0 10px;
    background: var(--bar);
    cursor: grab;
    user-select: none;
  }
  .title {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  header button {
    padding: 4px;
    border: none;
    background: transparent;
    opacity: 0.6;
  }
  header button:hover,
  header button.on {
    opacity: 1;
    background: color-mix(in srgb, var(--ink) 10%, transparent);
  }
  .swatches {
    display: flex;
    gap: 6px;
    padding: 6px 10px;
    background: var(--bar);
    border-top: 1px solid color-mix(in srgb, var(--ink) 10%, transparent);
  }
  .swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--ink) 25%, transparent);
    background: var(--note);
  }
  .swatch.current {
    outline: 2px solid var(--ink);
    outline-offset: 1px;
  }
  main {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .sticky main :global(.cm-scroller) {
    padding: 10px 12px;
  }
  .sticky main :global(.cm-editor) {
    font-size: 14px;
  }
  .missing {
    margin: auto;
    padding: 16px;
    color: var(--muted);
    text-align: center;
  }
  .grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 14px;
    height: 14px;
    cursor: nwse-resize;
    background: linear-gradient(135deg, transparent 50%, color-mix(in srgb, var(--ink) 20%, transparent) 50%);
  }
  .error {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 8px;
    background: var(--danger);
    color: white;
    border: none;
    text-align: left;
    font-size: 12px;
  }
</style>
