# Sticky Vault — notes for Claude

Lightweight cross-platform (Linux/macOS/Windows) quick-capture notes app. Replaces the habit of leaving
unsaved text-editor windows around: every note is a plain `.md` file in a user-chosen folder ("vault"),
typically an Obsidian vault or a subfolder of one. The folder is the only source of truth — no database.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** (runes, plain JS, no TypeScript) + **Vite** + **CodeMirror 6**.
Icons: **Lucide** (`@lucide/svelte` in the UI; app icon built from Lucide `sticky-note`). Don't hand-draw icons.

## Commands

```sh
npm run tauri dev                              # run app (opens native window)
npm run build                                  # frontend only — quick check that Svelte compiles
cd src-tauri && cargo test                     # Rust unit tests (notes.rs)
npm run tauri build -- --bundles appimage      # AppImage → src-tauri/target/release/bundle/appimage/
```

- `localhost:1420` in a normal browser shows a "only works inside the app window" message by design
  (`isTauri()` guard in App.svelte). Test in the Tauri window, not a browser.
- Blank Tauri window on Linux → `WEBKIT_DISABLE_DMABUF_RENDERER=1` (WebKitGTK GPU issue).
- AppImage: needs `libfuse2t64` to run on Ubuntu 24.04; if linuxdeploy fails on strip, use `NO_STRIP=true`.
- Dev machine: Ubuntu 24.04, zsh. User has built and run the app successfully.

## Layout

```
src/App.svelte          All UI state + logic: list, search, save queue, keyboard shortcuts
src/lib/Editor.svelte   CodeMirror wrapper; replaces doc only when `loadKey` changes
src/lib/api.js          invoke() wrappers for every Rust command + titleOf() (mirrors Rust title_of)
src/app.css             Global styles + color tokens (light/dark via prefers-color-scheme)
src-tauri/src/lib.rs    Tauri commands, AppState (vault path), config.json in OS config dir
src-tauri/src/notes.rs  File format, naming, list/read/create/save/finalize/delete + tests
src-tauri/capabilities/default.json   Frontend permissions
app-icon.svg            Icon source
```

## Note file format

```markdown
---
id: 01J...ULID
<any other existing frontmatter, preserved verbatim>
---
body text
```

- Backend strips frontmatter; the frontend only ever sees/edits `body`.
- `save` adds `id:` if missing and keeps every other frontmatter line untouched (Obsidian properties).
- Frontmatter parsing is hand-rolled line splitting (`split`, `frontmatter_id`) — no YAML crate on purpose.

## Invariants — don't break these

- **Paths crossing the IPC boundary are vault-relative with `/` separators.** Always go through
  `notes::resolve()`, which rejects `..`, absolute paths and non-`.md` files.
- **Auto-rename only for auto-named notes.** New notes are `YYYY-MM-DD HHMM[ N].md`; `finalize`
  renames them after their first line. Notes with any other name are never renamed — opening an
  existing Obsidian note must not change it.
- **Auto-delete only blank auto-named notes whose frontmatter holds nothing but `id`.**
- **Writes are atomic** (`.<name>.tmp` + rename). Temp files are dotfiles so listing skips them.
- **Listing skips dot-directories** (`.obsidian`, `.trash`, `.sticky`).
- **User delete → OS trash** (`trash` crate), never a hard delete.
- File names go through `safe_name` (Windows-reserved names, illegal chars, `#^[]` for Obsidian links, 60 chars max).

## Frontend flow (App.svelte)

- Typing → `onEdit` → 500 ms debounce → `flush()`. Saves are serialized through the `chain` promise;
  read `note.path` inside the chained callback, not before.
- Leaving a note (switch, new, close, change vault) → `leave()` = flush + `finalize_note`.
- Window blur → flush; focus → `syncFromDisk()` (reloads list; reloads editor if file changed and not dirty).
- Window close → `onCloseRequested(leave)`; requires `core:window:allow-destroy` in capabilities.
- Bump `loadSeq` whenever the editor content must be replaced from outside; never on normal typing.
- Shortcuts: Ctrl/Cmd+N new, Ctrl/Cmd+K or F search, Ctrl/Cmd+S force save.

## Adding things

- New Rust command: write fn in `lib.rs` → add to `generate_handler![]` → wrapper in `api.js`.
  Commands return `Result<T, String>`; NoteMeta serializes camelCase.
- New Tauri plugin used from JS: add Rust crate + `.plugin(...)` + npm package + permission in
  `capabilities/default.json`. Rust-side plugin use (e.g. opener in `open_vault_folder`) needs no permission.
- Svelte styles: `:global(...)` only inside component `<style>`, never in `app.css`.
- Icons: `import { Name } from '@lucide/svelte'`, `size={16}` in buttons; `button.icon` for icon-only.
- App icon change: edit `app-icon.svg`, rasterize to 1024px PNG (e.g. `@resvg/resvg-js` in a scratch dir),
  run `npx tauri icon <png>`, then delete `src-tauri/icons/android` and `ios`.

## Roadmap / agreed design decisions

1. ~~MVP: vault picker, list, editor, autosave, search, delete~~ (done)
2. **Pinned notes** as their own small windows (only selected notes, not all). Position/size/color go in
   a sidecar `<vault>/.sticky/state.json` keyed by note `id` — NOT in note frontmatter (keeps notes
   clean, dragging doesn't touch mtime). Missing state file = notes simply unpinned.
3. Tray icon, global hotkey for new note, autostart.
4. File watcher (`notify` crate) with self-write suppression + conflict copies.
5. CI builds (tauri-action) for all three OSes.

User preferences: no always-on-top needed; plain Markdown editor with syntax highlighting (no live
preview); exact Obsidian format matching is not a priority beyond not breaking their files.
