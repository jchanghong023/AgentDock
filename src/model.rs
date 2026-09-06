use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
pub type Id = uuid::Uuid;
pub fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Launch {
    /// Empty selects the current platform's interactive shell. No shell concatenation.
    pub program: String,
    #[serde(default)] pub args: Vec<String>,
}
impl Launch {
    pub fn omp(profile: Option<&str>, continue_session: bool) -> Self {
        let mut args = Vec::new();
        if continue_session { args.push("-c".into()); }
        if let Some(profile) = profile { args.extend(["--profile".into(), profile.into()]); }
        Self { program: "omp".into(), args }
    }
    pub fn omp_profile(&self) -> Option<&str> {
        self.args.iter().enumerate().find_map(|(i, arg)| {
            if arg == "--profile" { self.args.get(i + 1).map(String::as_str) }
            else { arg.strip_prefix("--profile=") }
        })
    }
    pub fn resolved(&self) -> Self {
        if !self.program.is_empty() { return self.clone(); }
        #[cfg(windows)] { Self { program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()), args: vec![] } }
        #[cfg(unix)] { Self { program: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()), args: vec!["-i".into()] } }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: Id, pub title: String, pub last_used: u64, pub launch: Launch,
    /// Explicit command only: saving metadata does not magically resume an agent.
    #[serde(default)] pub resume: Option<Launch>,
    #[serde(default)] pub omp_session: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Id,
    #[serde(with = "paths")] pub path: PathBuf,
    pub pinned: bool, pub last_used: u64,
    #[serde(default = "yes")] pub expanded: bool,
    #[serde(default)] pub sessions: Vec<SessionInfo>,
}
fn yes() -> bool { true }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub font_family: String, pub font_size: f32, pub scrollback_lines: usize,
    pub sidebar_width: f32, pub recent_fraction: f32, pub default_launch: Launch,
    pub directory_page_size: usize, pub preview_limit_bytes: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self { font_family: if cfg!(windows) { "Consolas".into() } else { "monospace".into() },
            font_size: 15.0, scrollback_lines: 10_000, sidebar_width: 278.0,
            recent_fraction: 0.45, default_launch: Launch::omp(None, false),
            directory_page_size: 256, preview_limit_bytes: 2 * 1024 * 1024 }
    }
}
impl Settings {
    pub fn sanitize(&mut self) {
        if !self.font_size.is_finite() { self.font_size = 15.0; }
        if !self.sidebar_width.is_finite() { self.sidebar_width = 278.0; }
        if !self.recent_fraction.is_finite() { self.recent_fraction = 0.45; }
        self.font_size = self.font_size.clamp(9.0, 40.0);
        self.sidebar_width = self.sidebar_width.clamp(190.0, 520.0);
        self.recent_fraction = self.recent_fraction.clamp(0.2, 0.8);
        self.scrollback_lines = self.scrollback_lines.clamp(100, 100_000);
        self.directory_page_size = self.directory_page_size.clamp(32, 1024);
        self.preview_limit_bytes = self.preview_limit_bytes.clamp(4096, 8 * 1024 * 1024);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    pub schema: u32,
    #[serde(default)] pub settings: Settings,
    #[serde(default)] pub projects: Vec<Project>,
}
impl Default for Store { fn default() -> Self { Self { schema: 1, settings: Settings::default(), projects: vec![] } } }
impl Store {
    pub fn validate(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema == 1, "未知状态版本 {}；不会覆盖文件", self.schema);
        self.settings.sanitize();
        let mut ids = std::collections::HashSet::new();
        for p in &self.projects {
            anyhow::ensure!(ids.insert(p.id), "重复项目 ID");
            for s in &p.sessions { anyhow::ensure!(ids.insert(s.id), "重复会话 ID"); }
        }
        Ok(())
    }
    pub fn ensure_project(&mut self, path: PathBuf) -> Id {
        if let Some(p) = self.projects.iter().find(|p| p.path == path) { return p.id; }
        let id = Id::new_v4();
        self.projects.push(Project { id, path, pinned: false, last_used: now(), expanded: true, sessions: vec![] });
        id
    }
    pub fn project(&self, id: Id) -> Option<&Project> { self.projects.iter().find(|p| p.id == id) }
    pub fn project_mut(&mut self, id: Id) -> Option<&mut Project> { self.projects.iter_mut().find(|p| p.id == id) }
    pub fn sorted(&self) -> Vec<&Project> {
        let mut result: Vec<_> = self.projects.iter().collect();
        result.sort_by(|a,b| b.pinned.cmp(&a.pinned).then_with(|| b.last_used.cmp(&a.last_used)).then_with(|| a.path.cmp(&b.path)));
        result
    }
    pub fn toggle_pin(&mut self, id: Id) { if let Some(p) = self.project_mut(id) { p.pinned = !p.pinned; } }
    pub fn new_session(&mut self, id: Id, launch: Launch) -> Option<Id> {
        let p = self.project_mut(id)?; let session = Id::new_v4();
        p.expanded = true; p.last_used = now();
        p.sessions.insert(0, SessionInfo { id: session, title: "OMP · 新会话".into(), last_used: now(), launch, resume: None, omp_session: None });
        Some(session)
    }
    pub fn session(&self, id: Id) -> Option<(&Project, &SessionInfo)> {
        self.projects.iter().find_map(|p| p.sessions.iter().find(|s| s.id == id).map(|s| (p,s)))
    }
    pub fn touch(&mut self, id: Id) {
        for p in &mut self.projects { if let Some(s) = p.sessions.iter_mut().find(|s| s.id == id) { s.last_used=now(); p.last_used=s.last_used; break; } }
    }
    pub fn title(&mut self, id: Id, value: &str) {
        let value: String = value.chars().filter(|c| !c.is_control()).take(120).collect();
        if value.is_empty() { return; }
        for p in &mut self.projects { if let Some(s) = p.sessions.iter_mut().find(|s| s.id == id) { s.title=value; break; } }
    }
    pub fn continue_omp(&mut self, id: Id, profile: Option<&str>) {
        for project in &mut self.projects {
            if let Some(session) = project.sessions.iter_mut().find(|s| s.id == id) {
                let profile = profile.or_else(|| session.launch.omp_profile()).map(str::to_owned);
                session.launch = Launch::omp(profile.as_deref(), session.omp_session.is_none());
                if let Some(file) = &session.omp_session { session.launch.args.extend(["--resume".into(), file.to_string_lossy().into_owned()]); }
                session.resume = None;
                break;
            }
        }
    }
    pub fn import_omp(&mut self, records: &[crate::omp::Record], profile: Option<&str>) -> bool {
        let mut changed = false;
        for record in records {
            let project = self.ensure_project(record.cwd.clone());
            let p = self.project_mut(project).unwrap();
            p.last_used = p.last_used.max(record.modified);
            if let Some(s) = p.sessions.iter_mut().find(|s| s.omp_session.as_ref() == Some(&record.file)) {
                if s.title != record.title || s.last_used != record.modified { s.title = record.title.clone(); s.last_used = record.modified; changed = true; }
            } else {
                // DevHub IDs identify tabs; the exact OMP identity is the file path.
                p.sessions.push(SessionInfo { id: Id::new_v4(), title: record.title.clone(), last_used: record.modified,
                    launch: Launch::omp(profile, false), resume: None, omp_session: Some(record.file.clone()) });
                changed = true;
            }
            p.sessions.sort_by(|a,b| b.last_used.cmp(&a.last_used));
        }
        changed
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn pinned_before_recent() {
        let mut s=Store::default(); let a=s.ensure_project("a".into()); let b=s.ensure_project("b".into());
        s.project_mut(a).unwrap().last_used=1; s.project_mut(b).unwrap().last_used=2;
        s.toggle_pin(a); assert_eq!(s.sorted()[0].id,a); s.toggle_pin(a); assert_eq!(s.sorted()[0].id,b);
    }
    #[test] fn dedup_projects_not_sessions() {
        let mut s=Store::default(); let a=s.ensure_project("a".into()); assert_eq!(a,s.ensure_project("a".into()));
        assert_ne!(s.new_session(a,Launch::default()),s.new_session(a,Launch::default()));
    }
    #[test] fn future_schema_rejected() { let mut s=Store { schema: 42,..Store::default() }; assert!(s.validate().is_err()); }
    #[test] fn sanitize_limits() { let mut s=Settings { font_size:f32::NAN, directory_page_size:usize::MAX,..Settings::default() }; s.sanitize(); assert_eq!(s.font_size,15.0); assert_eq!(s.directory_page_size,1024); }
    #[test] fn title_removes_control_characters() {
        let mut s=Store::default();let p=s.ensure_project("a".into());let id=s.new_session(p,Launch::default()).unwrap();s.title(id,"\x1b\n中文");assert_eq!(s.session(id).unwrap().1.title,"中文");
    }
    #[test] fn command_args_remain_separate() { let x=Launch { program:"tool".into(),args:vec!["a b".into(),"; echo x".into()] };assert_eq!(x,x.resolved()); }
    #[test] fn omp_continue_preserves_profile_as_one_argument() {
        let mut store = Store::default(); let project = store.ensure_project("project".into());
        let id = store.new_session(project, Launch::omp(Some("work space"), false)).unwrap();
        store.continue_omp(id, None);
        assert_eq!(store.session(id).unwrap().1.launch, Launch { program: "omp".into(), args: vec!["-c".into(), "--profile".into(), "work space".into()] });
        store.continue_omp(id, Some("other"));
        assert_eq!(store.session(id).unwrap().1.launch.args, ["-c", "--profile", "other"]);
    }
}
