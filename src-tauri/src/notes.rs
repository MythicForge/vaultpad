//! Note storage: one Markdown file per note inside the vault folder.
//!
//! Files keep a tiny YAML frontmatter block holding a stable `id`, which
//! later features (pinned windows in `.sticky/state.json`) key on so that
//! renames done in Obsidian don't lose track of a note. Any other
//! frontmatter already in a file is preserved verbatim.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use walkdir::WalkDir;

const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H%M";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMeta {
    /// Path relative to the vault, always with `/` separators.
    pub path: String,
    pub id: Option<String>,
    pub title: String,
    /// Note text without the frontmatter block.
    pub body: String,
    /// Last modified time, milliseconds since the Unix epoch.
    pub modified: u64,
}

struct Parsed {
    /// Raw frontmatter lines between the `---` fences, each ending in `\n`.
    frontmatter: Option<String>,
    body: String,
}

fn split(raw: &str) -> Parsed {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let rest = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n"));
    if let Some(rest) = rest {
        let mut offset = 0;
        for line in rest.split_inclusive('\n') {
            if line.trim_end() == "---" {
                return Parsed {
                    frontmatter: Some(rest[..offset].to_string()),
                    body: rest[offset + line.len()..].to_string(),
                };
            }
            offset += line.len();
        }
    }
    Parsed { frontmatter: None, body: raw.to_string() }
}

fn frontmatter_id(fm: &str) -> Option<String> {
    fm.lines().find_map(|line| {
        let value = line.strip_prefix("id:")?.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Frontmatter is "ours" when it holds nothing but the id we added.
fn only_has_id(fm: Option<&str>) -> bool {
    fm.map_or(true, |fm| fm.lines().all(|l| l.trim().is_empty() || l.starts_with("id:")))
}

fn with_id(fm: Option<String>) -> String {
    match fm {
        Some(fm) if frontmatter_id(&fm).is_some() => fm,
        Some(fm) => format!("id: {}\n{fm}", ulid::Ulid::new()),
        None => format!("id: {}\n", ulid::Ulid::new()),
    }
}

pub fn title_of(body: &str, path: &Path) -> String {
    body.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| stem(path))
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Names produced by `create`, e.g. `2026-10-07 0915` or `2026-10-07 0915 2`.
fn is_auto_name(stem: &str) -> bool {
    let (stamp, suffix) = match stem.get(15..) {
        Some(rest) => (&stem[..15], rest),
        None => (stem, ""),
    };
    let suffix_ok = suffix.is_empty()
        || suffix.strip_prefix(' ').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    chrono::NaiveDateTime::parse_from_str(stamp, TIMESTAMP_FORMAT).is_ok() && suffix_ok
}

/// First free `<stem>.md`, `<stem> 2.md`, ... in `dir`.
fn unique_path(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.md"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} {n}.md"));
        n += 1;
    }
    candidate
}

/// Write via a temp file + rename so Obsidian and sync tools never see a half-written note.
pub(crate) fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    let name = path.file_name().ok_or("invalid note path")?.to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, contents).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("save {}: {e}", path.display())
    })
}

fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

fn rel_path(vault: &Path, abs: &Path) -> String {
    abs.strip_prefix(vault)
        .unwrap_or(abs)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Resolves a vault-relative path, refusing anything that could escape the vault.
pub fn resolve(vault: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    let safe = !rel.is_empty() && rel_path.components().all(|c| matches!(c, Component::Normal(_)));
    let is_md = rel_path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if !safe || !is_md {
        return Err(format!("invalid note path: {rel}"));
    }
    Ok(vault.join(rel_path))
}

fn meta(vault: &Path, abs: &Path, parsed: Parsed) -> NoteMeta {
    NoteMeta {
        path: rel_path(vault, abs),
        id: parsed.frontmatter.as_deref().and_then(frontmatter_id),
        title: title_of(&parsed.body, abs),
        body: parsed.body,
        modified: modified_ms(abs),
    }
}

fn load(vault: &Path, abs: &Path) -> Result<NoteMeta, String> {
    let raw = fs::read_to_string(abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
    Ok(meta(vault, abs, split(&raw)))
}

/// All `.md` files under the vault, newest first. Dot folders such as
/// `.obsidian`, `.trash` and `.sticky` are skipped.
pub fn list(vault: &Path) -> Vec<NoteMeta> {
    let mut notes: Vec<NoteMeta> = WalkDir::new(vault)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))
        })
        .filter_map(|e| load(vault, e.path()).ok())
        .collect();
    notes.sort_by(|a, b| b.modified.cmp(&a.modified));
    notes
}

pub fn read(vault: &Path, rel: &str) -> Result<NoteMeta, String> {
    load(vault, &resolve(vault, rel)?)
}

/// Adds an `id` to a note's frontmatter if it lacks one; the body is untouched.
pub fn ensure_id(vault: &Path, rel: &str) -> Result<NoteMeta, String> {
    let note = read(vault, rel)?;
    if note.id.is_some() {
        return Ok(note);
    }
    save(vault, rel, &note.body)
}

pub fn create(vault: &Path) -> Result<NoteMeta, String> {
    let stamp = chrono::Local::now().format(TIMESTAMP_FORMAT).to_string();
    let abs = unique_path(vault, &stamp);
    atomic_write(&abs, &format!("---\n{}---\n", with_id(None)))?;
    load(vault, &abs)
}

pub fn save(vault: &Path, rel: &str, body: &str) -> Result<NoteMeta, String> {
    let abs = resolve(vault, rel)?;
    if let Some(dir) = abs.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let existing = fs::read_to_string(&abs).unwrap_or_default();
    let fm = with_id(split(&existing).frontmatter);
    atomic_write(&abs, &format!("---\n{fm}---\n{body}"))?;
    Ok(meta(vault, &abs, Parsed { frontmatter: Some(fm), body: body.to_string() }))
}

/// Runs when the user leaves a note. A brand-new note (still carrying its
/// timestamp name) that was left blank is removed. File names never change.
pub fn finalize(vault: &Path, rel: &str) -> Result<Option<NoteMeta>, String> {
    let abs = resolve(vault, rel)?;
    if !abs.exists() {
        return Ok(None);
    }
    let parsed = split(&fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?);
    if is_auto_name(&stem(&abs)) && parsed.body.trim().is_empty() && only_has_id(parsed.frontmatter.as_deref()) {
        fs::remove_file(&abs).map_err(|e| format!("remove {}: {e}", abs.display()))?;
        return Ok(None);
    }
    Ok(Some(meta(vault, &abs, parsed)))
}

pub fn delete(vault: &Path, rel: &str) -> Result<(), String> {
    let abs = resolve(vault, rel)?;
    trash::delete(&abs).map_err(|e| format!("move {} to trash: {e}", abs.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_preserves_frontmatter() {
        let p = split("---\nid: abc\ntags: [x]\n---\nHello\n");
        assert_eq!(p.frontmatter.as_deref(), Some("id: abc\ntags: [x]\n"));
        assert_eq!(p.body, "Hello\n");
        assert_eq!(frontmatter_id(p.frontmatter.as_deref().unwrap()).as_deref(), Some("abc"));

        let p = split("No frontmatter\n---\n");
        assert!(p.frontmatter.is_none());
    }

    #[test]
    fn auto_names() {
        assert!(is_auto_name("2026-10-07 0915"));
        assert!(is_auto_name("2026-10-07 0915 3"));
        assert!(!is_auto_name("Meeting notes"));
        assert!(!is_auto_name("2026-10-07 0915 notes"));
    }

    #[test]
    fn rejects_escaping_paths() {
        let v = Path::new("/vault");
        assert!(resolve(v, "../x.md").is_err());
        assert!(resolve(v, "/etc/x.md").is_err());
        assert!(resolve(v, "a.txt").is_err());
        assert!(resolve(v, "sub/a.md").is_ok());
    }

    #[test]
    fn note_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();

        let note = create(vault).unwrap();
        assert!(note.id.is_some());
        let saved = save(vault, &note.path, "# Buy milk\nand eggs\n").unwrap();
        assert_eq!(saved.id, note.id);
        assert_eq!(saved.title, "Buy milk");

        // Notes keep their timestamp name; the title comes from the first line.
        let done = finalize(vault, &note.path).unwrap().unwrap();
        assert_eq!(done.path, note.path);
        assert_eq!(done.title, "Buy milk");
        assert_eq!(done.id, note.id);

        let blank = create(vault).unwrap();
        assert!(finalize(vault, &blank.path).unwrap().is_none());
        assert!(!vault.join(&blank.path).exists());

        assert_eq!(list(vault).len(), 1);
    }

    #[test]
    fn ensure_id_keeps_body() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Plain.md"), "hello\n").unwrap();
        let note = ensure_id(dir.path(), "Plain.md").unwrap();
        assert!(note.id.is_some());
        assert_eq!(note.body, "hello\n");
        assert_eq!(ensure_id(dir.path(), "Plain.md").unwrap().id, note.id);
    }

    #[test]
    fn keeps_obsidian_properties() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Existing.md"), "---\ntags: [work]\n---\nold\n").unwrap();
        save(dir.path(), "Existing.md", "new\n").unwrap();
        let raw = fs::read_to_string(dir.path().join("Existing.md")).unwrap();
        assert!(raw.contains("tags: [work]\n") && raw.contains("id: ") && raw.ends_with("---\nnew\n"));
    }
}
