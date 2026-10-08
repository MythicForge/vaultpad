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
src-tauri/src/watch.rs  Vault file watcher (notify) + own-write suppression (`mark`)
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
- **Files are only renamed when the user explicitly renames them** (`rename_note`; never automatically —
  first-line renaming was removed by user choice). New notes are `YYYY-MM-DD HHMM[ N].md`.
- **Title rule** (`title_of` in Rust, `titleOf` in api.js — keep in sync): user-named notes show their
  file name; timestamp-named notes show their first non-blank line.
- Renames go through `safe_name` (Windows-reserved names, illegal chars, `#^[]` for Obsidian links,
  60 chars max), stay in the same folder, never overwrite (case-only change allowed), and run inside
  the save `chain`. `rename_note` emits `note-renamed {from, note}`.
- **Auto-delete only blank auto-named notes whose frontmatter holds nothing but `id`.**
- **Writes are atomic** (`.<name>.tmp` + rename). Temp files are dotfiles so listing skips them.
- **Every app-side create/write/rename/remove of a note calls `watch::mark(path)` first**
  (`atomic_write` does it for writes). Unmarked writes make every window reload on each autosave.
- **Saves never silently clobber outside edits.** Windows pass `base` (body they last loaded/saved) to
  `save_note`; if disk differs from both `base` and the new body, `save_checked` first writes the disk
  version to `<name> (conflict YYYY-MM-DD HHMM).md` (id line stripped so ids stay unique), then saves.
  Keep `base` updated: set on load/reload, set to the saved body inside the chain after each save.
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
- Window blur → flush; focus or `vault-changed` (watcher) → `syncFromDisk()` (reloads list; reloads editor
  if file changed and not dirty; follows the open note by id if renamed elsewhere).
- Main window close → `leave()` then **hide** (to tray); never quits. Quit = tray menu or Ctrl/Cmd+Q →
  `request_quit` emits `app-quitting`; every window saves and calls `quit_ready`; exit when all have
  answered (2 s fallback). Any new window type must join this handshake.
- Bump `loadSeq` whenever the editor content must be replaced from outside; never on normal typing.
- Shortcuts: Ctrl/Cmd+N new, Ctrl/Cmd+K or F search, Ctrl/Cmd+S force save, Ctrl/Cmd+Q quit, F2 or
  Ctrl/Cmd+Shift+R rename (main and stickies).
- Rename UI: `NameInput.svelte` (Enter/blur commit, Escape cancel) in the main toolbar (click path,
  pencil, F2) and in the sticky title bar (double-click title).
- `save_note` emits `note-changed {note, source: label}`; main applies saves from other windows to the list.
- `refreshStates()` reloads the full list whenever a sticky opens or closes (new note from a sticky's +,
  blank note dropped on close).

## Sticky windows

- Label `sticky-<hex(id)>` (ids are arbitrary frontmatter strings; hex keeps labels valid).
  `stickyIdFromLabel` in api.js decodes. Created in Rust (`open_sticky_window`) from async commands
  (sync command + window creation deadlocks on Windows).
- Lifecycle: `pop_out` sets `open` and creates/focuses. Close (X, Alt+F4, `close_sticky` from main) →
  sticky's onCloseRequested = flush + finalize + `open: false`. Quit (handshake above) and vault
  switch (`destroy`, relies on flush-on-blur) leave `open` true, so `restore_stickies` reopens them.
- Sticky title-bar dragging is hand-rolled (`onHeaderDown` → `startDragging`), NOT
  `data-tauri-drag-region`: Tauri's version maximizes on double-click, which blocked rename.
- GTK/X11 frameless quirks (measured): `outerPosition` is off by a phantom ~37px title bar → save
  `innerPosition`; builder `inner_size` comes out ~37px too tall → size via `set_size` after build.
  Breaking either makes stickies creep a little on every restart.
## Tray / app lifecycle

- Main window is `visible: false` in tauri.conf.json; setup shows it unless launched with `--hidden`
  (what the login item passes) — a login start shows only the tray + restored stickies.
- Global hotkey (`tauri-plugin-global-shortcut`, Rust-side only): default `CommandOrControl+Alt+N` →
  new sticky; override with `"hotkey"` in config.json (no UI). Registration failure is logged, not fatal.
  Doesn't work under Wayland. `set_vault` must keep other config fields (read-modify-write).
- Start at login: `tauri-plugin-autostart`, tray CheckMenuItem. Uses the AppImage path when run as an
  AppImage; enabling it from `tauri dev` points the login item at the debug binary.

- Tray (`build_tray` in lib.rs, icon `src-tauri/icons/tray.png` = `vaultpad-blank.svg` at 64px):
  Show Vault Pad / New sticky note / Quit. Left click shows main on Windows/macOS; Linux only opens the menu.
  Needs tauri features `tray-icon` + `image-png`. Tray actions creating windows run via `async_runtime::spawn`.
- `tauri-plugin-single-instance` (registered first): a second launch calls `show_main` instead of
  starting another process writing the same files.
- Ubuntu GNOME shows the tray via its built-in AppIndicator extension; on GNOME without it, a hidden
  main window can only be brought back by launching the app again (single-instance shows it).

## Testing

- Testing without touching the user's vault: `XDG_CONFIG_HOME=<scratch>/cfg npm run tauri dev` with
  `<scratch>/cfg/dev.vaultpad.app/config.json` pointing at a scratch vault; drive with `wmctrl`/`xdotool`.
  Use `npm run tauri dev`, not the bare debug binary (it loads `devUrl`, blank without Vite).
  Don't `pkill -f` a pattern that appears in your own command line; use `pkill -x vault-pad`.

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
3. ~~Tray icon, global hotkey for new note, autostart~~ (done)
4. ~~File watcher (`notify` crate) with self-write suppression + conflict copies~~ (done)
5. CI builds (tauri-action) for all three OSes.

User preferences: per-sticky always-on-top toggle wanted (default off); plain Markdown editor with syntax highlighting (no live
preview); exact Obsidian format matching is not a priority beyond not breaking their files.
