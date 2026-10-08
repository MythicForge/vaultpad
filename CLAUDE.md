# Sticky Vault — notes for Claude

Lightweight cross-platform (Linux/macOS/Windows) quick-capture notes app. Replaces the habit of leaving
unsaved text-editor windows around: every note is a plain `.md` file in a user-chosen folder ("vault"),
typically an Obsidian vault or a subfolder of one. The folder is the only source of truth — no database.

Stack: **Tauri 2** (Rust backend) + **Svelte 5** (runes, plain JS, no TypeScript) + **Vite** + **CodeMirror 6**.
Icons: **Lucide** (`@lucide/svelte` in the UI; app icon built from Lucide `sticky-note`). Don't hand-draw icons.

> Project has now been renamed to VaultPad - N.F

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
src/main.js             Mounts App (label "main") or Sticky (label "sticky-<hex id>")
src/App.svelte          Main window: list, search, save queue, keyboard shortcuts, pin/pop-out
src/lib/Sticky.svelte   One sticky-note window: frameless, color, on-top, + new sticky, own save queue
src/lib/Editor.svelte   CodeMirror wrapper; replaces doc only when `loadKey` changes
src/lib/api.js          invoke() wrappers for every Rust command + titleOf() (mirrors Rust title_of)
src/app.css             Global styles + color tokens (light/dark via prefers-color-scheme)
src-tauri/src/lib.rs    Tauri commands, AppState (vault path), config.json in OS config dir, sticky windows
src-tauri/src/notes.rs  File format, naming, list/read/create/save/finalize/delete + tests
src-tauri/src/sticky.rs <vault>/.sticky/state.json: per-note pinned/open/onTop/color/geometry + tests
src-tauri/capabilities/default.json   Frontend permissions
app-icon.svg            App icon source (yellow note with #)
src-tauri/icons/vaultpad-blank.svg   Grey stand-in: in-app empty/welcome placeholders, future tray icon
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
- **Files are never renamed.** New notes are `YYYY-MM-DD HHMM[ N].md` and keep that name (user
  choice; first-line renaming was removed). The list *displays* the first line as the title.
- **Auto-delete only blank auto-named notes whose frontmatter holds nothing but `id`.**
- **Writes are atomic** (`.<name>.tmp` + rename). Temp files are dotfiles so listing skips them.
- **Listing skips dot-directories** (`.obsidian`, `.trash`, `.sticky`).
- **User delete → OS trash** (`trash` crate), never a hard delete.
- **UI state lives in `.sticky/state.json` keyed by note `id`, never in frontmatter.** Entries equal to
  the default are dropped; deleting a note drops its entry. All writes go through `AppState::update_ui`
  / `forget` (hold `ui_lock`, emit `note-states-changed`). Corrupt file → error, never overwritten.
- **A note is edited in one window at a time.** Popping out releases it from the main editor; clicking
  a popped-out note in the list focuses its sticky instead of opening it.

## Frontend flow (App.svelte)

- Typing → `onEdit` → 500 ms debounce → `flush()`. Saves are serialized through the `chain` promise;
  read `note.path` inside the chained callback, not before.
- Leaving a note (switch, new, close, change vault) → `leave()` = flush + `finalize_note`.
- Window blur → flush; focus → `syncFromDisk()` (reloads list; reloads editor if file changed and not dirty).
- Window close → `onCloseRequested(leave)`; requires `core:window:allow-destroy` in capabilities.
- Bump `loadSeq` whenever the editor content must be replaced from outside; never on normal typing.
- Shortcuts: Ctrl/Cmd+N new, Ctrl/Cmd+K or F search, Ctrl/Cmd+S force save.
- `save_note` emits `note-changed {note, source: label}`; main applies saves from other windows to the list.
- `refreshStates()` reloads the full list whenever a sticky opens or closes (new note from a sticky's +,
  blank note dropped on close).

## Sticky windows

- Label `sticky-<hex(id)>` (ids are arbitrary frontmatter strings; hex keeps labels valid).
  `stickyIdFromLabel` in api.js decodes. Created in Rust (`open_sticky_window`) from async commands
  (sync command + window creation deadlocks on Windows).
- Lifecycle: `pop_out` sets `open` and creates/focuses. Close (X, Alt+F4, `close_sticky` from main) →
  sticky's onCloseRequested = flush + finalize + `open: false`. App quit and vault switch *destroy*
  stickies without the handler, so `open` stays true and `restore_stickies` reopens them next time.
  Stickies flush on blur, which is what keeps quit/vault-switch from losing edits.
- Closing the main window exits the app (`on_window_event` Destroyed → `exit(0)`).
- GTK/X11 frameless quirks (measured): `outerPosition` is off by a phantom ~37px title bar → save
  `innerPosition`; builder `inner_size` comes out ~37px too tall → size via `set_size` after build.
  Breaking either makes stickies creep a little on every restart.
- Testing without touching the user's vault: `XDG_CONFIG_HOME=<scratch>/cfg npm run tauri dev` with
  `<scratch>/cfg/dev.vaultpad.app/config.json` pointing at a scratch vault; drive with `wmctrl`/`xdotool`.

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
2. ~~Sticky windows (pop-out, color, always-on-top, + new sticky) and pin-to-top-of-list~~ (done)
3. Tray icon (use `src-tauri/icons/vaultpad-blank.svg`), global hotkey for new note, autostart.
4. File watcher (`notify` crate) with self-write suppression + conflict copies.
5. CI builds (tauri-action) for all three OSes.

User preferences: per-sticky always-on-top toggle wanted (default off); plain Markdown editor with syntax highlighting (no live
preview); exact Obsidian format matching is not a priority beyond not breaking their files.
