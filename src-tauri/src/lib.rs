mod notes;
mod sticky;
mod watch;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use notes::NoteMeta;
use serde::{Deserialize, Serialize};
use sticky::NoteState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_opener::OpenerExt;

/// System-wide shortcut for a new sticky note, unless config.json sets `hotkey`.
const DEFAULT_HOTKEY: &str = "CommandOrControl+Alt+N";
/// Passed by the login item so the app starts with just the tray and stickies.
const HIDDEN_ARG: &str = "--hidden";

/// Persisted in the OS config dir (e.g. ~/.config/dev.vaultpad.app/config.json).
#[derive(Default, Serialize, Deserialize)]
struct Config {
    vault: Option<PathBuf>,
    /// Global shortcut for a new sticky, in Tauri accelerator syntax
    /// (e.g. "CommandOrControl+Alt+N"). Hand-edited; there's no UI for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hotkey: Option<String>,
}

fn read_config(path: &PathBuf) -> Config {
    fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

struct AppState {
    config_path: PathBuf,
    vault: Mutex<Option<PathBuf>>,
    /// Serializes read-modify-write of `.sticky/state.json` across windows.
    ui_lock: Mutex<()>,
    /// While quitting: labels of windows that haven't confirmed their final save.
    quitting: Mutex<Option<HashSet<String>>>,
    /// Watches the current vault; replacing it stops the old watcher.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl AppState {
    fn vault(&self) -> Result<PathBuf, String> {
        self.vault.lock().unwrap().clone().ok_or_else(|| "No notes folder selected".into())
    }

    fn watch(&self, app: &AppHandle, vault: &PathBuf) {
        let watcher = watch::start(app.clone(), vault.clone())
            .map_err(|e| eprintln!("file watcher unavailable for {}: {e}", vault.display()))
            .ok();
        *self.watcher.lock().unwrap() = watcher;
    }

    /// Updates one note's UI state and tells every window about it.
    fn update_ui(&self, app: &AppHandle, id: &str, patch: sticky::Patch) -> Result<NoteState, String> {
        let vault = self.vault()?;
        let result = {
            let _guard = self.ui_lock.lock().unwrap();
            sticky::update(&vault, id, patch)?
        };
        let _ = app.emit("note-states-changed", ());
        Ok(result)
    }

    /// Drops the UI state of a note that no longer exists.
    fn forget(&self, app: &AppHandle, vault: &PathBuf, id: Option<String>) -> Result<(), String> {
        let Some(id) = id else { return Ok(()) };
        {
            let _guard = self.ui_lock.lock().unwrap();
            sticky::remove(vault, &id)?;
        }
        let _ = app.emit("note-states-changed", ());
        Ok(())
    }
}

const STICKY_PREFIX: &str = "sticky-";

/// Window label for a note's sticky window. Ids come from frontmatter and may
/// contain characters labels don't allow, so they're hex-encoded
/// (decoded again by stickyIdFromLabel in api.js).
fn sticky_label(id: &str) -> String {
    let hex: String = id.bytes().map(|b| format!("{b:02x}")).collect();
    format!("{STICKY_PREFIX}{hex}")
}

fn open_sticky_window(app: &AppHandle, id: &str, st: &NoteState) -> Result<(), String> {
    let label = sticky_label(id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.unminimize();
        return win.set_focus().map_err(|e| e.to_string());
    }
    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::default())
        .title("Vault Pad note")
        .decorations(false)
        .min_inner_size(180.0, 120.0)
        .always_on_top(st.on_top);
    if let (Some(x), Some(y)) = (st.x, st.y) {
        builder = builder.position(x, y);
    }
    let win = builder.build().map_err(|e| e.to_string())?;
    // Sized after creation: on GTK the builder's inner_size on a frameless
    // window comes out taller by a phantom title bar, which then compounds
    // every time the saved size is restored.
    let size = tauri::LogicalSize::new(st.width.unwrap_or(300.0), st.height.unwrap_or(300.0));
    win.set_size(size).map_err(|e| e.to_string())
}

/// Upper bound on waiting for windows to save before the app exits anyway.
const QUIT_TIMEOUT: Duration = Duration::from_secs(2);

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Asks every window to save (each answers with `quit_ready`), then exits.
fn request_quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let labels: HashSet<String> = app.webview_windows().into_keys().collect();
    {
        let mut quitting = state.quitting.lock().unwrap();
        if quitting.is_some() {
            return;
        }
        *quitting = Some(labels);
    }
    let _ = app.emit("app-quitting", ());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(QUIT_TIMEOUT);
        app.exit(0);
    });
}

fn pop_out_path(app: &AppHandle, state: &AppState, path: &str) -> Result<NoteMeta, String> {
    let (note, id) = ensure_note_id(&state.vault()?, path)?;
    let st = state.update_ui(app, &id, sticky::Patch { open: Some(true), ..Default::default() })?;
    open_sticky_window(app, &id, &st)?;
    Ok(note)
}

/// Tray / global hotkey "New sticky note": a fresh note straight into its own
/// window. Runs off the event loop (window creation there can deadlock on
/// Windows); with no vault chosen yet it shows the main window instead.
fn spawn_new_sticky(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let created = state
            .vault()
            .and_then(|vault| notes::create(&vault))
            .and_then(|note| pop_out_path(&app, &state, &note.path));
        if created.is_err() {
            show_main(&app);
        }
    });
}

/// "CommandOrControl+Alt+N" → "Ctrl+Alt+N" (or "Cmd+…" on macOS) for menus.
fn pretty_hotkey(hotkey: &str) -> String {
    let primary = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
    hotkey.replace("CommandOrControl", primary).replace("CmdOrCtrl", primary)
}

fn register_hotkey(app: &tauri::App, hotkey: &str) -> tauri::Result<()> {
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    spawn_new_sticky(app);
                }
            })
            .build(),
    )?;
    // Not fatal: the combination may be taken by another app or the desktop.
    if let Err(e) = app.global_shortcut().register(hotkey) {
        eprintln!("couldn't register global shortcut {hotkey}: {e}");
    }
    Ok(())
}

fn build_tray(app: &tauri::App, hotkey: &str) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Vault Pad", true, None::<&str>)?;
    let new_label = format!("New sticky note ({})", pretty_hotkey(hotkey));
    let new = MenuItem::with_id(app, "new-sticky", new_label, true, None::<&str>)?;
    let at_login = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start at login", true, at_login, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&show, &new, &sep()?, &autostart, &sep()?, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("Vault Pad")
        .menu(&menu)
        // Left click shows the main window on Windows/macOS; Linux trays
        // only ever open the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => request_quit(app),
            "new-sticky" => spawn_new_sticky(app),
            "autostart" => {
                let launcher = app.autolaunch();
                let enable = !launcher.is_enabled().unwrap_or(false);
                let result = if enable { launcher.enable() } else { launcher.disable() };
                if let Err(e) = result {
                    eprintln!("couldn't change start at login: {e}");
                }
                // Reflect what actually happened, whatever the click toggled.
                let _ = autostart.set_checked(launcher.is_enabled().unwrap_or(false));
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn ensure_note_id(vault: &PathBuf, path: &str) -> Result<(NoteMeta, String), String> {
    let note = notes::ensure_id(vault, path)?;
    let id = note.id.clone().ok_or("note has no id")?;
    Ok((note, id))
}

#[tauri::command]
fn get_vault(state: State<'_, AppState>) -> Option<String> {
    state.vault.lock().unwrap().as_ref().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn set_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(format!("not a folder: {}", path.display()));
    }
    // Stickies belong to the old vault. They already saved on blur, and
    // destroy (unlike close) skips their close handler, so nothing they do
    // can land in the new vault. Their state stays `open` for next time.
    for (label, win) in app.webview_windows() {
        if label.starts_with(STICKY_PREFIX) {
            let _ = win.destroy();
        }
    }
    let mut config = read_config(&state.config_path);
    config.vault = Some(path.clone());
    if let Some(dir) = state.config_path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&state.config_path, json).map_err(|e| e.to_string())?;
    *state.vault.lock().unwrap() = Some(path.clone());
    state.watch(&app, &path);
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn open_vault_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let vault = state.vault()?;
    app.opener().open_path(vault.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteMeta>, String> {
    Ok(notes::list(&state.vault()?))
}

#[tauri::command]
fn read_note(state: State<'_, AppState>, path: String) -> Result<NoteMeta, String> {
    notes::read(&state.vault()?, &path)
}

#[tauri::command]
fn create_note(state: State<'_, AppState>) -> Result<NoteMeta, String> {
    notes::create(&state.vault()?)
}

#[derive(Clone, Serialize)]
struct NoteChanged<'a> {
    note: &'a NoteMeta,
    source: &'a str,
}

#[derive(Clone, Serialize)]
struct NoteConflict<'a> {
    path: &'a str,
    copy: &'a str,
}

/// `base`: the body this window last loaded or saved, for conflict detection
/// (see notes::save_checked). A conflict is announced as `note-conflict`.
#[tauri::command]
fn save_note(
    window: tauri::Window,
    state: State<'_, AppState>,
    path: String,
    body: String,
    base: Option<String>,
) -> Result<NoteMeta, String> {
    let (note, conflict) = notes::save_checked(&state.vault()?, &path, &body, base.as_deref())?;
    if let Some(copy) = &conflict {
        let _ = window.emit("note-conflict", NoteConflict { path: &note.path, copy });
    }
    let _ = window.emit("note-changed", NoteChanged { note: &note, source: window.label() });
    Ok(note)
}

#[derive(Clone, Serialize)]
struct NoteRenamed<'a> {
    from: &'a str,
    note: &'a NoteMeta,
}

#[tauri::command]
fn rename_note(app: AppHandle, state: State<'_, AppState>, path: String, name: String) -> Result<NoteMeta, String> {
    let note = notes::rename(&state.vault()?, &path, &name)?;
    let _ = app.emit("note-renamed", NoteRenamed { from: &path, note: &note });
    Ok(note)
}

#[tauri::command]
fn find_note(state: State<'_, AppState>, id: String) -> Result<Option<NoteMeta>, String> {
    Ok(notes::list(&state.vault()?).into_iter().find(|n| n.id.as_deref() == Some(id.as_str())))
}

#[tauri::command]
fn get_note_states(state: State<'_, AppState>) -> Result<BTreeMap<String, NoteState>, String> {
    Ok(sticky::load(&state.vault()?))
}

#[tauri::command]
fn update_note_state(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: sticky::Patch,
) -> Result<NoteState, String> {
    state.update_ui(&app, &id, patch)
}

#[tauri::command]
fn set_pinned(app: AppHandle, state: State<'_, AppState>, path: String, pinned: bool) -> Result<NoteMeta, String> {
    let (note, id) = ensure_note_id(&state.vault()?, &path)?;
    state.update_ui(&app, &id, sticky::Patch { pinned: Some(pinned), ..Default::default() })?;
    Ok(note)
}

/// Opens a note as a sticky window, or focuses it if already open.
/// Async because creating windows from a sync command deadlocks on Windows.
#[tauri::command]
async fn pop_out(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<NoteMeta, String> {
    pop_out_path(&app, &state, &path)
}

#[tauri::command]
fn quit(app: AppHandle) {
    request_quit(&app);
}

/// A window finished its final save; exit once every window has.
#[tauri::command]
fn quit_ready(window: tauri::Window, app: AppHandle, state: State<'_, AppState>) {
    let mut quitting = state.quitting.lock().unwrap();
    if let Some(pending) = quitting.as_mut() {
        pending.remove(window.label());
        if pending.is_empty() {
            app.exit(0);
        }
    }
}

/// Closes a sticky window; its own close handler saves and marks it closed.
#[tauri::command]
fn close_sticky(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    match app.get_webview_window(&sticky_label(&id)) {
        Some(win) => win.close().map_err(|e| e.to_string()),
        None => state.update_ui(&app, &id, sticky::Patch { open: Some(false), ..Default::default() }).map(|_| ()),
    }
}

/// Reopens the sticky windows that were open last time. Notes that no longer
/// exist are skipped (their state is kept in case the file comes back).
#[tauri::command]
async fn restore_stickies(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let vault = state.vault()?;
    let existing: std::collections::HashSet<String> = notes::list(&vault).into_iter().filter_map(|n| n.id).collect();
    for (id, st) in sticky::load(&vault) {
        if st.open && existing.contains(&id) && app.get_webview_window(&sticky_label(&id)).is_none() {
            open_sticky_window(&app, &id, &st)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn finalize_note(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<Option<NoteMeta>, String> {
    let vault = state.vault()?;
    let id = notes::read(&vault, &path).ok().and_then(|n| n.id);
    let result = notes::finalize(&vault, &path)?;
    if result.is_none() {
        state.forget(&app, &vault, id)?;
    }
    Ok(result)
}

#[tauri::command]
fn delete_note(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let vault = state.vault()?;
    let id = notes::read(&vault, &path).ok().and_then(|n| n.id);
    notes::delete(&vault, &path)?;
    state.forget(&app, &vault, id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch just surfaces the running app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::Builder::new().app_name("Vault Pad").arg(HIDDEN_ARG).build())
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let config = read_config(&config_path);
            let vault = config.vault.filter(|p| p.is_dir());
            let hotkey = config.hotkey.unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
            app.manage(AppState {
                config_path,
                vault: Mutex::new(vault.clone()),
                ui_lock: Mutex::new(()),
                quitting: Mutex::new(None),
                watcher: Mutex::new(None),
            });
            if let Some(vault) = &vault {
                app.state::<AppState>().watch(app.handle(), vault);
            }
            register_hotkey(app, &hotkey)?;
            build_tray(app, &hotkey)?;
            // The main window starts hidden (tauri.conf.json) so a login
            // start shows only the tray and stickies.
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        // The main window only hides on close (App.svelte); quitting goes
        // through request_quit. This is a backstop if it's destroyed anyway.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_vault,
            set_vault,
            open_vault_folder,
            list_notes,
            read_note,
            create_note,
            save_note,
            finalize_note,
            delete_note,
            find_note,
            rename_note,
            get_note_states,
            update_note_state,
            set_pinned,
            pop_out,
            close_sticky,
            restore_stickies,
            quit,
            quit_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
