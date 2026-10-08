# Vault Pad

Quick-capture notes and desktop sticky notes that live as plain `.md` files in a folder you choose. Point it at an Obsidian vault (or a subfolder of one) and every note shows up there.

## Features

**Notes**

- One note = one Markdown file. The folder is the only source of truth; there is no database.
- New notes are named by timestamp (`2026-10-07 0915.md`) and keep that name until you rename them. Blank new notes are discarded when you leave them.
- Rename any note from the toolbar (click the file name or the pencil), with `F2` or `Ctrl/Cmd+Shift+R`. Names are made safe for Windows, macOS, Linux and Obsidian links, and never overwrite another note.
- The list shows a note's name, or its first line while it still has a timestamp name.
- Autosaves 500 ms after you stop typing (atomic temp file + rename).
- Existing frontmatter / Obsidian properties are preserved; the app only adds a single `id:` line.
- Edits made elsewhere (e.g. in Obsidian) are picked up when a window regains focus.
- Delete moves files to the OS trash.

**Sticky notes**

- Pop any note out into its own small, frameless window, or create one directly with **+** on a sticky or from the tray.
- Six colors (with dark-mode variants) and a per-sticky **keep on top** toggle.
- Drag by the title bar; double-click the title to rename.
- Position, size, color and on-top are remembered, and open stickies come back on the next launch.
- Window state lives in `<vault>/.sticky/state.json`, keyed by note id, never in the notes themselves, so moving a sticky doesn't touch the file.
- **Pin to top** keeps a note at the top of the list.

**Tray**

- Closing the main window hides it to the tray; stickies stay on screen.
- Tray menu: Show Vault Pad · New sticky note · Quit. Quitting saves every open window first.
- Only one instance runs; launching it again brings the existing window back.

## Shortcuts

| Keys | Action |
| --- | --- |
| `Ctrl/Cmd+N` | New note |
| `Ctrl/Cmd+K` or `Ctrl/Cmd+F` | Search (`Enter` opens the top hit, `Esc` clears) |
| `Ctrl/Cmd+S` | Save now |
| `F2` or `Ctrl/Cmd+Shift+R` | Rename (main window and stickies) |
| `Ctrl/Cmd+Q` | Quit |

## Develop

Prereqs: Rust, Node 20+, plus the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

Ubuntu/Debian:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

```sh
npm install
npm run tauri dev                            # run with hot reload
npm run build                                # frontend only: quick compile check
cd src-tauri && cargo test                   # Rust unit tests
npm run tauri build                          # installers in src-tauri/target/release/bundle
npm run tauri build -- --bundles appimage    # just the AppImage
```

The UI only works inside the app window; `localhost:1420` in a browser shows a notice by design.

### Linux notes

- Blank window: run with `WEBKIT_DISABLE_DMABUF_RENDERER=1` (WebKitGTK GPU issue).
- The AppImage needs `libfuse2t64` on Ubuntu 24.04. If linuxdeploy fails while stripping, build with `NO_STRIP=true`.
- The tray icon needs an AppIndicator-capable panel. Ubuntu's GNOME has one built in; stock GNOME needs the AppIndicator extension.
- Keep on top works on X11. GNOME on Wayland doesn't let apps keep windows on top.

## Layout

```
src/App.svelte             Main window: list, search, editor, pin / pop-out / rename
src/lib/Sticky.svelte      A sticky note window
src/lib/Editor.svelte      CodeMirror 6 wrapper
src/lib/NameInput.svelte   Inline rename field
src/lib/api.js             Wrappers for the Rust commands
src-tauri/src/lib.rs       Tauri commands, config, sticky windows, tray, quit handling
src-tauri/src/notes.rs     File format, naming, save/rename/delete (unit-tested)
src-tauri/src/sticky.rs    .sticky/state.json storage (unit-tested)
app-icon.svg               App icon source
src-tauri/icons/vaultpad-blank.svg   Grey stand-in icon (placeholders, tray)
```

Regenerate app icons after editing `app-icon.svg`: rasterize to a 1024px PNG, run `npx tauri icon <png>`, then delete `src-tauri/icons/android` and `src-tauri/icons/ios`. The tray icon is `src-tauri/icons/tray.png`, the blank icon at 64px.

## Roadmap

1. ~~MVP: vault, list, editor, autosave, search~~
2. ~~Sticky note windows, colors, keep on top, pin to top~~
3. ~~Tray icon~~ · global hotkey for a new note · autostart
4. File watcher + conflict copies
5. CI builds for Windows / macOS / Linux
