//! Native paths are never round-tripped through lossy UI labels.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Component, Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Encoded {
    Text(String),
    Unix { unix_bytes: Vec<u8> },
    Windows { windows_wide: Vec<u16> },
}
pub fn serialize<S: Serializer>(path: &Path, s: S) -> Result<S::Ok, S::Error> {
    if let Some(text) = path.to_str() { return Encoded::Text(text.into()).serialize(s); }
    #[cfg(unix)] {
        use std::os::unix::ffi::OsStrExt;
        Encoded::Unix { unix_bytes: path.as_os_str().as_bytes().to_vec() }.serialize(s)
    }
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        Encoded::Windows { windows_wide: path.as_os_str().encode_wide().collect() }.serialize(s)
    }
}
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
    match Encoded::deserialize(d)? {
        Encoded::Text(text) => Ok(text.into()),
        #[cfg(unix)]
        Encoded::Unix { unix_bytes } => {
            use std::os::unix::ffi::OsStringExt;
            Ok(std::ffi::OsString::from_vec(unix_bytes).into())
        }
        #[cfg(windows)]
        Encoded::Windows { windows_wide } => {
            use std::os::windows::ffi::OsStringExt;
            Ok(std::ffi::OsString::from_wide(&windows_wide).into())
        }
        #[allow(unreachable_patterns)]
        _ => Err(serde::de::Error::custom("state contains a foreign-platform path")),
    }
}
pub fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    let p = if path.is_absolute() { path.to_owned() } else { std::env::current_dir()?.join(path) };
    // Do NOT collapse '..' across symlinks and do not stat network paths here.
    Ok(p.components().filter(|c| !matches!(c, Component::CurDir)).collect())
}
pub fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}
pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" }))
}
pub fn state_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("DEVHUB_HOME") { return p.into(); }
    #[cfg(windows)] {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(home).join("DevHub")
    }
    #[cfg(unix)] {
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local/state")).join("devhub")
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn parent_is_not_collapsed() { assert!(absolute(Path::new("a/./b/../c")).unwrap().ends_with("a/b/../c")); }
    #[cfg(unix)]
    #[test] fn non_utf8_roundtrip() {
        use std::os::unix::ffi::OsStringExt;
        #[derive(Serialize, Deserialize)] struct Wrapper { #[serde(with = "super")] path: PathBuf }
        let p = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 255]));
        let encoded = serde_json::to_string(&Wrapper { path: p.clone() }).unwrap();
        assert_eq!(serde_json::from_str::<Wrapper>(&encoded).unwrap().path, p);
    }
}
