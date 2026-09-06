//! Read only OMP session headers, never transcripts or authentication files.
use crate::{model::Id, paths};
use anyhow::Result;
use std::{collections::HashMap, fs, io::{BufRead, BufReader, Read}, path::{Path, PathBuf}, time::SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record { pub id: Id, pub file: PathBuf, pub cwd: PathBuf, pub title: String, pub modified: u64 }

pub fn normalize_profile(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None); };
    let value = value.trim();
    if value.is_empty() || value == "default" { return Ok(None); }
    let base = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON"|"PRN"|"AUX"|"NUL") || (base.len() == 4 && (base.starts_with("COM") || base.starts_with("LPT")) && base.as_bytes()[3].is_ascii_digit());
    anyhow::ensure!(value.len() <= 64 && value.as_bytes()[0].is_ascii_alphanumeric() && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b)) && !value.ends_with('.') && !reserved, "无效的 OMP profile 名称");
    Ok(Some(value.to_owned()))
}
pub fn session_root(profile: Option<&str>) -> PathBuf {
    if let Some(root) = std::env::var_os("PI_CODING_AGENT_SESSION_DIR").filter(|v| !v.is_empty()) { return PathBuf::from(root); }
    let config = paths::home().join(std::env::var_os("PI_CONFIG_DIR").filter(|v| !v.is_empty()).unwrap_or_else(|| ".omp".into()));
    let config = if let Some(profile) = profile { config.join("profiles").join(profile) } else { config };
    if profile.is_none() {
        if let Some(root) = std::env::var_os("PI_CODING_AGENT_DIR").filter(|v| !v.is_empty()) { return PathBuf::from(root).join("sessions"); }
    }
    #[cfg(unix)]
    if let Some(root) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        let root = PathBuf::from(root).join("omp");
        let root = if let Some(profile) = profile { root.join("profiles").join(profile) } else { root };
        if root.is_dir() { return root.join("sessions"); }
    }
    config.join("agent/sessions")
}
pub fn read_record(file: &Path) -> Result<Option<Record>> {
    let mut title = None;
    // The fork may prefix a title record before the upstream session header.
    // Stop at the session header, before any messages or credential pins.
    for line in BufReader::new(fs::File::open(file)?.take(64 * 1024)).lines().take(8) {
        let line = line?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
        match value.get("type").and_then(|v| v.as_str()) {
            Some("title") => { title = value.get("title").and_then(|v| v.as_str()).map(|s| s.chars().filter(|c| !c.is_control()).take(120).collect::<String>()); }
            Some("session") => {
                let Some(id) = value.get("id").and_then(|v| v.as_str()).and_then(|v| Id::parse_str(v).ok()) else { return Ok(None); };
                let Some(cwd) = value.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from) else { return Ok(None); };
                if !cwd.is_absolute() { return Ok(None); }
                let modified = fs::metadata(file)?.modified()?.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
                return Ok(Some(Record { id, file: file.to_owned(), cwd, title: title.filter(|t| !t.is_empty()).unwrap_or_else(|| id.to_string()[..8].to_owned()), modified }));
            }
            Some("message") => return Ok(None),
            _ => {}
        }
    }
    Ok(None)
}

#[derive(Default)]
pub struct Scanner { cache: HashMap<PathBuf, (SystemTime, u64, Option<Record>)> }
impl Scanner {
    pub fn scan(&mut self, root: &Path) -> Result<Vec<Record>> {
        if !root.exists() { self.cache.clear(); return Ok(vec![]); }
        let mut files = Vec::new();
        for item in fs::read_dir(root)? {
            let item = item?; let kind = item.file_type()?;
            if kind.is_file() { files.push(item.path()); }
            else if kind.is_dir() {
                for child in fs::read_dir(item.path())? { let child = child?; if child.file_type()?.is_file() { files.push(child.path()); } }
            }
        }
        files.retain(|p| p.extension().is_some_and(|e| e == "jsonl"));
        let mut cache = HashMap::new(); let mut records = Vec::new();
        for file in files {
            let Ok(meta) = fs::metadata(&file) else { continue; };
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let entry = match self.cache.remove(&file) {
                Some(entry) if entry.0 == modified && entry.1 == meta.len() => entry,
                _ => (modified, meta.len(), read_record(&file).ok().flatten()),
            };
            if let Some(record) = &entry.2 { records.push(record.clone()); }
            cache.insert(file, entry);
        }
        self.cache = cache;
        records.sort_by(|a,b| b.modified.cmp(&a.modified).then_with(|| a.file.cmp(&b.file)));
        Ok(records)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn title_prefix_and_precise_file_identity() {
        let d = tempfile::tempdir().unwrap(); let p = d.path().join("one.jsonl"); let id = Id::new_v4();
        fs::write(&p, format!("{}\n{}\nnot a transcript to parse", serde_json::json!({"type":"title","title":"Agent work"}), serde_json::json!({"type":"session","id":id,"cwd":d.path()}))).unwrap();
        let record = read_record(&p).unwrap().unwrap(); assert_eq!(record.id,id); assert_eq!(record.file,p); assert_eq!(record.title,"Agent work");
        let mut scanner = Scanner::default(); assert_eq!(scanner.scan(d.path()).unwrap().len(),1);
        fs::remove_file(p).unwrap(); assert!(scanner.scan(d.path()).unwrap().is_empty());
    }
    #[test] fn profile_validation() {
        assert_eq!(normalize_profile(Some("work".into())).unwrap().as_deref(),Some("work"));
        assert!(normalize_profile(Some("../work".into())).is_err());
        assert!(normalize_profile(Some("CON".into())).is_err());
        assert_eq!(normalize_profile(Some("default".into())).unwrap(),None);
    }
}
