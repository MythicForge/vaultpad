mod notes;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use notes::NoteMeta;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Persisted in the OS config dir (e.g. ~/.config/dev.stickyvault.app/config.json).
#[derive(Default, Serialize, Deserialize)]
struct Config {
    vault: Option<PathBuf>,
}

struct AppState {
    config_path: PathBuf,
    vault: Mutex<Option<PathBuf>>,
}

impl AppState {
    fn vault(&self) -> Result<PathBuf, String> {
        self.vault.lock().unwrap().clone().ok_or_else(|| "No notes folder selected".into())
    }
}

#[tauri::command]
fn get_vault(state: State<'_, AppState>) -> Option<String> {
    state.vault.lock().unwrap().as_ref().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn set_vault(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(format!("not a folder: {}", path.display()));
    }
    let config = Config { vault: Some(path.clone()) };
    if let Some(dir) = state.config_path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&state.config_path, json).map_err(|e| e.to_string())?;
    *state.vault.lock().unwrap() = Some(path.clone());
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

#[tauri::command]
fn save_note(state: State<'_, AppState>, path: String, body: String) -> Result<NoteMeta, String> {
    notes::save(&state.vault()?, &path, &body)
}

#[tauri::command]
fn finalize_note(state: State<'_, AppState>, path: String) -> Result<Option<NoteMeta>, String> {
    notes::finalize(&state.vault()?, &path)
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, path: String) -> Result<(), String> {
    notes::delete(&state.vault()?, &path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let config: Config = fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let vault = config.vault.filter(|p| p.is_dir());
            app.manage(AppState { config_path, vault: Mutex::new(vault) });
            Ok(())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
