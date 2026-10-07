# Sticky Vault

Quick-capture notes that land as plain `.md` files in a folder you choose — point it at an Obsidian vault (or a subfolder of one) and every note shows up there.

- One note = one Markdown file. The folder is the only source of truth.
- New notes start as `2026-10-07 0915.md` and are renamed after their first line when you leave them. Blank new notes are discarded.
- Autosaves 500 ms after you stop typing (atomic temp-file + rename).
- Existing frontmatter / Obsidian properties are preserved; the app adds a single `id:` line on save.
- Edits made elsewhere are picked up when the window regains focus.
- Delete moves files to the OS trash.

Shortcuts: `Ctrl/Cmd+N` new note · `Ctrl/Cmd+K` search · `Enter` in search opens top hit.

## Develop

Prereqs: Rust, Node 20+, plus the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

Ubuntu/Debian:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

```sh
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # installers in src-tauri/target/release/bundle
```

## Layout

```
src/                  Svelte UI (App.svelte, lib/Editor.svelte = CodeMirror 6)
src-tauri/src/lib.rs   Tauri commands + config (vault path in OS config dir)
src-tauri/src/notes.rs File format, naming, save/rename/delete (unit-tested)
app-icon.svg          Source for app icons (Lucide "sticky-note", ISC)
```

Regenerate icons after editing `app-icon.svg`: rasterize to a 1024px PNG, then `npx tauri icon <png>`.

## Roadmap

1. ~~MVP: vault, list, editor, autosave, search~~
2. Pinned notes as their own windows; positions in `<vault>/.sticky/state.json`
3. Tray icon, global hotkey for a new note, autostart
4. File watcher + conflict copies
5. CI builds for Windows / macOS / Linux
