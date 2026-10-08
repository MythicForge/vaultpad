//! Vault file watcher. Changes made outside the app (Obsidian, sync tools,
//! editors) are batched and announced to every window as `vault-changed`,
//! which then re-syncs from disk. The app's own writes are recorded via
//! `mark` and ignored, so autosave doesn't make every window reload.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// How long after our own write events for that path are ignored.
const OWN_WRITE_WINDOW: Duration = Duration::from_secs(2);
/// Events arriving within this window are reported as one change.
const DEBOUNCE: Duration = Duration::from_millis(300);

static OWN_WRITES: LazyLock<Mutex<HashMap<PathBuf, Instant>>> = LazyLock::new(Default::default);

/// Records that the app itself is about to create/change/remove `path`.
pub fn mark(path: &Path) {
    let mut writes = OWN_WRITES.lock().unwrap();
    let now = Instant::now();
    writes.retain(|_, t| now.duration_since(*t) < OWN_WRITE_WINDOW);
    writes.insert(path.to_path_buf(), now);
}

fn is_own(path: &Path) -> bool {
    OWN_WRITES.lock().unwrap().get(path).is_some_and(|t| t.elapsed() < OWN_WRITE_WINDOW)
}

/// Same rules as `notes::list`: `.md` files outside dot folders. Temp files
/// from `atomic_write` are dotfiles, so they never count.
fn is_note_path(vault: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(vault) else { return false };
    let is_md = rel.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    is_md && !rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

fn is_external_change(vault: &Path, event: notify::Result<Event>) -> bool {
    let Ok(event) = event else { return false };
    !matches!(event.kind, EventKind::Access(_))
        && event.paths.iter().any(|p| is_note_path(vault, p) && !is_own(p))
}

fn run(app: AppHandle, vault: PathBuf, rx: Receiver<notify::Result<Event>>) {
    // Ends when the watcher (and with it the sender) is dropped.
    while let Ok(first) = rx.recv() {
        let mut changed = is_external_change(&vault, first);
        let deadline = Instant::now() + DEBOUNCE;
        while let Ok(event) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            changed |= is_external_change(&vault, event);
        }
        if changed {
            let _ = app.emit("vault-changed", ());
        }
    }
}

/// Starts watching `vault`; dropping the returned watcher stops it.
pub fn start(app: AppHandle, vault: PathBuf) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&vault, RecursiveMode::Recursive)?;
    std::thread::spawn(move || run(app, vault, rx));
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_paths() {
        let v = Path::new("/vault");
        assert!(is_note_path(v, Path::new("/vault/a.md")));
        assert!(is_note_path(v, Path::new("/vault/sub/B.MD")));
        assert!(!is_note_path(v, Path::new("/vault/.a.md.tmp")));
        assert!(!is_note_path(v, Path::new("/vault/.obsidian/x.md")));
        assert!(!is_note_path(v, Path::new("/vault/.sticky/state.json")));
        assert!(!is_note_path(v, Path::new("/elsewhere/a.md")));
    }

    #[test]
    fn own_writes_expire() {
        let p = Path::new("/vault/mine.md");
        assert!(!is_own(p));
        mark(p);
        assert!(is_own(p));
    }
}
