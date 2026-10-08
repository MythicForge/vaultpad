//! Per-vault UI state kept in `<vault>/.sticky/state.json`, keyed by note `id`:
//! list pins and sticky-window placement/color. Lives outside the notes so
//! dragging a sticky around never touches a note's contents or mtime.
//! A missing file simply means nothing is pinned or popped out.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::notes::atomic_write;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoteState {
    /// Shown at the top of the note list.
    pub pinned: bool,
    /// Open as its own sticky window.
    pub open: bool,
    /// Sticky window stays above other windows.
    pub on_top: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Sticky window geometry in logical pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

/// Fields to change; anything left out keeps its current value.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub pinned: Option<bool>,
    pub open: Option<bool>,
    pub on_top: Option<bool>,
    pub color: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

impl NoteState {
    fn apply(&mut self, p: Patch) {
        if let Some(v) = p.pinned { self.pinned = v; }
        if let Some(v) = p.open { self.open = v; }
        if let Some(v) = p.on_top { self.on_top = v; }
        if p.color.is_some() { self.color = p.color; }
        if p.x.is_some() { self.x = p.x; }
        if p.y.is_some() { self.y = p.y; }
        if p.width.is_some() { self.width = p.width; }
        if p.height.is_some() { self.height = p.height; }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    notes: BTreeMap<String, NoteState>,
}

fn state_path(vault: &Path) -> PathBuf {
    vault.join(".sticky").join("state.json")
}

fn read_file(vault: &Path) -> Result<StateFile, String> {
    let path = state_path(vault);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StateFile::default()),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

/// All note states; an unreadable file reads as empty.
pub fn load(vault: &Path) -> BTreeMap<String, NoteState> {
    read_file(vault).map(|f| f.notes).unwrap_or_default()
}

/// Read-modify-write of one note's state. Refuses to overwrite a file it
/// can't parse. Callers must serialize calls (AppState::ui_lock).
pub fn update(vault: &Path, id: &str, patch: Patch) -> Result<NoteState, String> {
    let mut file = read_file(vault)?;
    let entry = file.notes.entry(id.to_string()).or_default();
    entry.apply(patch);
    let result = entry.clone();
    if result == NoteState::default() {
        file.notes.remove(id);
    }
    let path = state_path(vault);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    atomic_write(&path, &json)?;
    Ok(result)
}

/// Forgets a deleted note. No-op (and no write) if it had no state.
pub fn remove(vault: &Path, id: &str) -> Result<(), String> {
    let mut file = read_file(vault)?;
    if file.notes.remove(id).is_none() {
        return Ok(());
    }
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    atomic_write(&state_path(vault), &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_merge_and_defaults_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let v = dir.path();
        assert!(load(v).is_empty());

        update(v, "a", Patch { pinned: Some(true), ..Default::default() }).unwrap();
        let s = update(v, "a", Patch { x: Some(10.0), color: Some("pink".into()), ..Default::default() }).unwrap();
        assert!(s.pinned);
        assert_eq!(s.x, Some(10.0));
        assert_eq!(load(v)["a"].color.as_deref(), Some("pink"));

        update(v, "b", Patch { open: Some(true), ..Default::default() }).unwrap();
        update(v, "b", Patch { open: Some(false), ..Default::default() }).unwrap();
        assert!(!load(v).contains_key("b"));

        remove(v, "a").unwrap();
        assert!(load(v).is_empty());
    }

    #[test]
    fn refuses_to_clobber_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".sticky")).unwrap();
        fs::write(state_path(dir.path()), "{ nope").unwrap();
        assert!(update(dir.path(), "a", Patch::default()).is_err());
        assert_eq!(fs::read_to_string(state_path(dir.path())).unwrap(), "{ nope");
    }
}
