//! Application state and routing. Previewing a file does not own, replace or
//! terminate the PTY. Process ownership stays in `sessions` until explicit close.
use super::{divider::{Axis, Divider}, terminal_view::TerminalView, virtual_list::{Click, Row, VirtualList}};
use crate::{cli::Options, files::{self, Completed, FileService, FileTree, Kind, Preview, Request, TreeRow}, model::{Id, Launch, Store}, paths, persistence::Persistence, terminal::{input::Action, Session, Snapshot}};
use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use iced::{clipboard, widget::{button, column, container, markdown, row, scrollable, text, Space}, window, Element, Font, Length, Size, Subscription, Task, Theme};
use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::Arc, time::{Duration, Instant}};

#[derive(Debug, Clone)]
pub enum RecentKey { Project(Id), Session(Id) }
#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Window(window::Id, window::Event),
    Recent(RecentKey, Click),
    File(TreeRow, Click),
    NewSelected,
    Activate(Id),
    CloseSession(Id),
    Restart(Id),
    Terminal(Id, Action),
    Clipboard(Id, Option<String>),
    ShowPreview,
    ClosePreview,
    PreviewPage(i32),
    ToggleRaw,
    Link(String),
    CopyPreview,
    ResizeSidebar(f32),
    ResizeSections(f32),
    Confirm,
    Cancel,
    Dismiss,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content { Empty, Terminal(Id), Preview }
#[derive(Debug, Clone, Copy)]
enum Confirmation { Quit(window::Id), Stop(Id), Restart(Id) }
struct Document {
    preview: Arc<Preview>,
    pages: Vec<String>,
    index: usize,
    raw: bool,
    items: Vec<markdown::Item>,
}
impl Document {
    fn new(preview: Arc<Preview>) -> Self {
        let pages = text_pages(&preview.text);
        let items = if preview.markdown { markdown::parse(&pages[0]).collect() } else { vec![] };
        Self { preview, pages, index: 0, raw: false, items }
    }
    fn change_page(&mut self, delta: i32) {
        self.index = self.index.saturating_add_signed(delta as isize).min(self.pages.len() - 1);
        self.items = if self.preview.markdown { markdown::parse(&self.pages[self.index]).collect() } else { vec![] };
    }
}
/// Bound one preview layout even for a giant line or a large generated Markdown.
/// Pages preserve every decoded character, but Markdown constructs may span pages.
pub fn text_pages(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut lines = 0;
    for (index, c) in input.char_indices() {
        if index - start >= 64 * 1024 || lines >= 600 {
            result.push(input[start..index].to_owned());
            start = index;
            lines = 0;
        }
        if c == '\n' { lines += 1; }
    }
    result.push(input[start..].to_owned());
    result
}

type SpawnResult = (Id, std::result::Result<Session, String>);
pub struct App {
    store: Store,
    persistence: Persistence,
    files: FileService,
    tree: FileTree,
    sessions: HashMap<Id, Session>,
    spawning: HashSet<Id>,
    spawned_tx: Sender<SpawnResult>,
    spawned_rx: Receiver<SpawnResult>,
    tabs: Vec<Id>,
    active: Content,
    last_terminal: Option<Id>,
    snapshot: Option<(Id, Snapshot)>,
    document: Option<Document>,
    preview_token: u64,
    preview_loading: bool,
    selected_path: Option<PathBuf>,
    selected_project: Option<Id>,
    recent_rows: Vec<Row<RecentKey>>,
    file_rows: Vec<Row<TreeRow>>,
    confirmation: Option<Confirmation>,
    notice: Option<String>,
    dirty: bool,
    font: Font,
    focus_serial: u64,
    terminal_focus: bool,
    window: Option<window::Id>,
    window_size: Size,
    poll_cursor: usize,
    started: Instant,
    smoke_ms: Option<u64>,
    closing: bool,
}
impl App {
    pub fn create(options: Options) -> Result<Self> {
        let (persistence, mut store) = Persistence::open(options.state_dir.clone().unwrap_or_else(paths::state_dir))?;
        let files = FileService::new(store.settings.directory_page_size, store.settings.preview_limit_bytes)?;
        files.request(Request::Roots).map_err(anyhow::Error::msg)?;
        let font = if store.settings.font_family == "monospace" { Font::MONOSPACE }
            else { Font::with_name(Box::leak(store.settings.font_family.clone().into_boxed_str())) };
        let mut initial = None;
        if let Some(path) = options.project.clone() {
            let project = store.ensure_project(path);
            let launch = options.launch.clone().unwrap_or_else(|| store.settings.default_launch.clone());
            initial = store.new_session(project, launch);
        } else if options.launch.is_some() {
            let project = store.ensure_project(std::env::current_dir()?);
            initial = store.new_session(project, options.launch.clone().unwrap());
        } else if store.projects.is_empty() {
            store.ensure_project(std::env::current_dir()?);
        }
        let (spawned_tx, spawned_rx) = bounded(16);
        let mut app = Self {
            store, persistence, files, tree: FileTree::default(), sessions: HashMap::new(),
            spawning: HashSet::new(), spawned_tx, spawned_rx, tabs: vec![], active: Content::Empty,
            last_terminal: None, snapshot: None, document: None, preview_token: 0,
            preview_loading: false, selected_path: None, selected_project: None,
            recent_rows: vec![], file_rows: vec![], confirmation: None, notice: None, dirty: true,
            font, focus_serial: 0, terminal_focus: false, window: None, window_size: Size::new(1240.0, 820.0), poll_cursor: 0,
            started: Instant::now(), smoke_ms: options.smoke_ui_ms, closing: false,
        };
        if let Some(id) = initial { app.start_session(id); }
        if let Some(path) = options.open { app.open_preview(path); }
        app.rebuild_recent(); app.rebuild_files();
        Ok(app)
    }
    pub fn title(&self) -> String {
        match self.active {
            Content::Terminal(id) => self.store.session(id).map(|(p, s)| format!("{} · {} — DevHub", s.title, paths::name(&p.path))).unwrap_or_else(|| "DevHub".into()),
            Content::Preview => self.document.as_ref().map(|d| format!("{} · 只读 — DevHub", paths::name(&d.preview.path))).unwrap_or_else(|| "DevHub".into()),
            Content::Empty => "DevHub · 原生终端工作台".into(),
        }
    }
    pub fn subscription(&self) -> Subscription<Message> {
        let period = if self.sessions.values().any(Session::running) || !self.spawning.is_empty() { 32 } else { 100 };
        Subscription::batch([
            iced::time::every(Duration::from_millis(period)).map(|_| Message::Tick),
            window::events().map(|(id, event)| Message::Window(id, event)),
        ])
    }
    fn notice(&mut self, text: impl Into<String>) { self.notice = Some(text.into()); }
    fn rebuild_recent(&mut self) {
        let mut rows = vec![];
        for p in self.store.sorted() {
            rows.push(Row { key: RecentKey::Project(p.id), label: paths::name(&p.path), depth: 0,
                folder: true, expanded: p.expanded, selected: self.selected_project == Some(p.id),
                star: Some(p.pinned), add: true, muted: false });
            if p.expanded {
                for s in &p.sessions {
                    let prefix = if self.spawning.contains(&s.id) { "◌ " }
                        else if let Some(session) = self.sessions.get(&s.id) {
                            if session.unread && self.active != Content::Terminal(s.id) { "● " }
                            else if session.running() { "› " } else { "○ " }
                        } else { "  " };
                    rows.push(Row { key: RecentKey::Session(s.id), label: format!("{prefix}{}", s.title),
                        depth: 1, folder: false, expanded: false, selected: self.active == Content::Terminal(s.id),
                        star: None, add: false, muted: !self.sessions.contains_key(&s.id) });
                }
            }
        }
        self.recent_rows = rows;
    }
    fn rebuild_files(&mut self) {
        self.file_rows = self.tree.rows.iter().map(|r| {
            let directory = matches!(r.kind, Kind::Entry { directory: true });
            let pinned = self.store.projects.iter().find(|p| p.path == r.path).is_some_and(|p| p.pinned);
            Row { key: r.clone(), label: r.label.clone(), depth: r.depth, folder: directory,
                expanded: self.tree.listings.contains_key(&r.path), selected: self.selected_path.as_ref() == Some(&r.path),
                star: directory.then_some(pinned), add: directory, muted: matches!(r.kind, Kind::Notice) }
        }).collect();
    }
    fn request_page(&mut self, path: PathBuf, index: usize) {
        match self.tree.begin(path, index) {
            Ok(request) => {
                if let Request::Page { path, generation, .. } = &request {
                    let path = path.clone(); let generation = *generation;
                    if let Err(error) = self.files.request(request) { self.tree.complete(&path, generation, Err(error)); }
                }
            }
            Err(error) => self.notice(error),
        }
        self.rebuild_files();
    }
    fn focus(&mut self, next: Content) {
        if let Content::Terminal(previous) = self.active {
            if let Some(session) = self.sessions.get_mut(&previous) { session.engine.terminal.focus_changed(false); }
        }
        self.active = next;
        self.terminal_focus = matches!(next, Content::Terminal(_));
        self.focus_serial = self.focus_serial.wrapping_add(1);
        if let Content::Terminal(id) = next {
            self.last_terminal = Some(id);
            if let Some((p, _)) = self.store.session(id) { self.selected_project = Some(p.id); }
            if let Some(session) = self.sessions.get_mut(&id) {
                session.unread = false; session.engine.terminal.focus_changed(true);
                self.snapshot = Some((id, session.engine.snapshot()));
            } else { self.snapshot = None; }
            self.store.touch(id); self.dirty = true;
        }
        self.rebuild_recent();
    }
    fn start_session(&mut self, id: Id) {
        if self.spawning.contains(&id) { self.focus(Content::Terminal(id)); return; }
        if self.sessions.get(&id).is_some_and(Session::running) { self.focus(Content::Terminal(id)); return; }
        if self.sessions.len() + self.spawning.len() >= 16 && !self.sessions.contains_key(&id) {
            self.notice("最多同时保留 16 个终端；关闭不用的终端标签后再打开。"); return;
        }
        let Some((project, info)) = self.store.session(id) else { return; };
        let path = project.path.clone();
        let launch = info.resume.clone().unwrap_or_else(|| info.launch.clone());
        let settings = self.store.settings.clone();
        let output = self.spawned_tx.clone();
        self.sessions.remove(&id);
        self.spawning.insert(id);
        if !self.tabs.contains(&id) { self.tabs.push(id); }
        let result = std::thread::Builder::new().name("devhub-spawn".into()).spawn(move || {
            let session = Session::spawn(&path, &launch, &settings).map_err(|e| format!("{e:#}"));
            let _ = output.send((id, session));
        });
        if let Err(error) = result { self.spawning.remove(&id); self.notice(format!("无法启动后台线程：{error}")); }
        self.focus(Content::Terminal(id));
    }
    fn new_session(&mut self, project: Id) {
        if self.sessions.len() + self.spawning.len() >= 16 { self.notice("同时保留终端的上限为 16；先关闭不用的标签。"); return; }
        if let Some(id) = self.store.new_session(project, self.store.settings.default_launch.clone()) {
            self.dirty = true; self.start_session(id);
        }
    }
    fn close_session(&mut self, id: Id) {
        self.spawning.remove(&id);
        self.sessions.remove(&id); // Drop closes PTY and reaps the direct child.
        self.tabs.retain(|tab| *tab != id);
        if self.active == Content::Terminal(id) {
            self.snapshot = None;
            let next = self.tabs.last().copied().map(Content::Terminal).unwrap_or(if self.document.is_some() { Content::Preview } else { Content::Empty });
            self.focus(next);
        }
        if self.last_terminal == Some(id) { self.last_terminal = self.tabs.last().copied(); }
        self.rebuild_recent();
    }
    fn open_preview(&mut self, path: PathBuf) {
        self.preview_token = self.preview_token.wrapping_add(1);
        self.preview_loading = true;
        match self.files.preview(path, self.preview_token) {
            Ok(()) => self.focus(Content::Preview),
            Err(error) => { self.preview_loading = false; self.notice(error); }
        }
    }
    fn finish(&mut self, id: window::Id) -> Task<Message> {
        if let Err(error) = self.persistence.flush_final(&self.store) { self.notice(format!("退出前保存失败：{error:#}")); return Task::none(); }
        self.closing = true;
        // No unsupported promise that process trees or shell-disowned jobs are killed.
        self.sessions.clear(); self.spawning.clear();
        window::close(id)
    }
    fn tick(&mut self) -> Task<Message> {
        let completed: Vec<_> = self.files.rx.try_iter().collect();
        for result in completed {
            match result {
                Completed::Roots(roots) => { self.tree.roots = roots; self.tree.rebuild(); self.rebuild_files(); }
                Completed::Page { path, generation, result } => { self.tree.complete(&path, generation, result); self.rebuild_files(); }
                Completed::Preview { token, result } if token == self.preview_token => {
                    self.preview_loading = false;
                    match result { Ok(preview) => self.document = Some(Document::new(preview)), Err(error) => self.notice(error) }
                }
                _ => {},
            }
        }
        for (id, result) in self.spawned_rx.try_iter().collect::<Vec<_>>() {
            if !self.spawning.remove(&id) || self.closing { continue; } // Dropping a cancelled result closes its PTY.
            match result {
                Ok(session) => {
                    if self.active == Content::Terminal(id) { self.snapshot = Some((id, session.engine.snapshot())); }
                    self.sessions.insert(id, session);
                }
                Err(error) => self.notice(format!("终端启动失败：{error}")),
            }
            self.rebuild_recent();
        }
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        let mut changed = false;
        let deadline = Instant::now() + Duration::from_millis(7);
        for n in 0..ids.len() {
            let index = (self.poll_cursor + n) % ids.len();
            let id = ids[index];
            let session = self.sessions.get_mut(&id).expect("session key collected");
            let previous_running = session.running(); let previous_unread = session.unread;
            let output = session.poll(if self.active == Content::Terminal(id) { 64 * 1024 } else { 16 * 1024 });
            if self.active == Content::Terminal(id) {
                session.unread = false;
                if output { self.snapshot = Some((id, session.engine.snapshot())); }
            }
            let title: String = session.engine.terminal.get_title().chars().filter(|c| !c.is_control()).take(120).collect();
            if !title.is_empty() && title != "DevHub" && self.store.session(id).is_some_and(|(_, s)| s.title != title) {
                self.store.title(id, &title); self.dirty = true; changed = true;
            }
            if previous_running != session.running() || previous_unread != session.unread { changed = true; }
            if let Some(error) = session.error.take() { self.notice = Some(format!("终端 I/O：{error}")); }
            if Instant::now() >= deadline { self.poll_cursor = (index + 1) % ids.len(); break; }
        }
        if changed { self.rebuild_recent(); }
        for error in self.persistence.errors.try_iter() { self.notice = Some(error); self.dirty = true; }
        if self.dirty && self.persistence.save(&self.store) { self.dirty = false; }
        if self.smoke_ms.is_some_and(|ms| self.started.elapsed() >= Duration::from_millis(ms)) {
            if let Some(id) = self.window { eprintln!("DEVHUB_GUI_SMOKE_EVENT_LOOP_OK"); return self.finish(id); }
        }
        Task::none()
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => return self.tick(),
            Message::Window(id, event) => {
                self.window = Some(id);
                match event {
                    window::Event::Resized(size) => self.window_size = size,
                    window::Event::CloseRequested => {
                        if self.sessions.values().any(Session::running) || !self.spawning.is_empty() { self.confirmation = Some(Confirmation::Quit(id)); }
                        else { return self.finish(id); }
                    }
                    _ => {},
                }
            }
            Message::Recent(key, click) => match key {
                RecentKey::Project(id) => {
                    self.selected_project = Some(id);
                    self.terminal_focus = false;
                    match click {
                        Click::Star => { self.store.toggle_pin(id); self.dirty = true; self.rebuild_files(); }
                        Click::New => self.new_session(id),
                        Click::Select => if let Some(p) = self.store.project_mut(id) { p.expanded = !p.expanded; self.dirty = true; },
                        Click::Double => {},
                    }
                    self.rebuild_recent();
                }
                RecentKey::Session(id) => return self.update(Message::Activate(id)),
            },
            Message::File(entry, click) => {
                self.selected_path = Some(entry.path.clone());
                self.terminal_focus = false;
                match entry.kind {
                    Kind::Entry { directory: true } => {
                        if matches!(click, Click::Star | Click::New) {
                            let project = self.store.ensure_project(entry.path.clone());
                            self.selected_project = Some(project);
                            if matches!(click, Click::Star) { self.store.toggle_pin(project); } else { self.new_session(project); }
                            self.dirty = true; self.rebuild_recent();
                        } else if matches!(click, Click::Select) {
                            if self.tree.listings.contains_key(&entry.path) {
                                self.tree.collapse(&entry.path); let _ = self.files.request(Request::Forget(entry.path));
                            } else { self.request_page(entry.path, 0); }
                        }
                    }
                    Kind::Entry { directory: false } if matches!(click, Click::Double) => self.open_preview(entry.path),
                    Kind::Previous(index) | Kind::Next(index) if matches!(click, Click::Select | Click::Double) => self.request_page(entry.path, index),
                    _ => {},
                }
                self.rebuild_files();
            }
            Message::NewSelected => {
                if let Some(project) = self.selected_project.or_else(|| self.store.sorted().first().map(|p| p.id)) { self.new_session(project); }
                else { self.notice("在下方系统目录树中选择一个目录，然后点击目录旁的 +。"); }
            }
            Message::Activate(id) => {
                if self.sessions.contains_key(&id) || self.spawning.contains(&id) { self.focus(Content::Terminal(id)); }
                else { self.confirmation = Some(Confirmation::Restart(id)); }
            }
            Message::Restart(id) => self.confirmation = Some(Confirmation::Restart(id)),
            Message::CloseSession(id) => {
                if self.sessions.get(&id).is_some_and(Session::running) || self.spawning.contains(&id) { self.confirmation = Some(Confirmation::Stop(id)); }
                else { self.close_session(id); }
            }
            Message::Terminal(id, action) => {
                if self.confirmation.is_some() { return Task::none(); }
                if let Action::Focus(value) = &action { self.terminal_focus = *value; }
                if matches!(action, Action::RequestPaste) { return clipboard::read().map(move |value| Message::Clipboard(id, value)); }
                if let Action::Zoom(delta) = action {
                    self.store.settings.font_size = (self.store.settings.font_size + delta as f32).clamp(9.0, 40.0);
                    self.dirty = true; return Task::none();
                }
                if let Some(session) = self.sessions.get_mut(&id) {
                    if matches!(action, Action::Copy) { return clipboard::write(session.engine.selection_text()); }
                    let result: Result<()> = match action {
                        Action::Key { key, modifiers, pressed } => {
                            if session.running() { if pressed { session.engine.bottom(); session.engine.terminal.key_down(key, modifiers) } else { session.engine.terminal.key_up(key, modifiers) } } else { Ok(()) }
                        }
                        Action::Text(value) => if session.running() { session.engine.text(&value) } else { Ok(()) },
                        Action::Mouse(event) => session.engine.terminal.mouse_event(event),
                        Action::Scroll(rows) => { session.engine.scroll(rows); Ok(()) }
                        Action::Focus(focused) => { session.engine.terminal.focus_changed(focused); Ok(()) }
                        Action::Resize { cols, rows, width, height } => session.resize(cols, rows, width, height),
                        Action::Select { col, row, mode } => { session.engine.select(col, row, mode); Ok(()) }
                        Action::Extend { col, row } => { session.engine.extend(col, row); Ok(()) }
                        _ => Ok(()),
                    };
                    if let Err(error) = result { self.notice = Some(format!("终端：{error:#}")); }
                    if self.active == Content::Terminal(id) { self.snapshot = Some((id, session.engine.snapshot())); }
                }
            }
            Message::Clipboard(id, value) => {
                if let (Some(session), Some(value)) = (self.sessions.get_mut(&id), value) {
                    if session.running() { if let Err(error) = session.engine.paste(&value) { self.notice = Some(format!("粘贴失败：{error:#}")); } }
                }
            }
            Message::ShowPreview => self.focus(Content::Preview),
            Message::ClosePreview => {
                self.preview_token = self.preview_token.wrapping_add(1);
                self.preview_loading = false; self.document = None;
                self.focus(self.last_terminal.filter(|id| self.sessions.contains_key(id)).map(Content::Terminal).unwrap_or(Content::Empty));
            }
            Message::PreviewPage(delta) => if let Some(document) = &mut self.document { document.change_page(delta); },
            Message::ToggleRaw => if let Some(document) = &mut self.document { document.raw = !document.raw; },
            Message::Link(link) => { self.notice("链接已复制；本程序不会自动联网、执行命令或打开外部链接。"); return clipboard::write(link); }
            Message::CopyPreview => if let Some(document) = &self.document { return clipboard::write(document.pages[document.index].clone()); },
            Message::ResizeSidebar(delta) => {
                self.store.settings.sidebar_width = (self.store.settings.sidebar_width + delta).clamp(190.0, (self.window_size.width - 350.0).clamp(190.0, 520.0)); self.dirty = true;
            }
            Message::ResizeSections(delta) => {
                self.store.settings.recent_fraction = (self.store.settings.recent_fraction + delta / self.window_size.height.max(1.0)).clamp(0.2, 0.8); self.dirty = true;
            }
            Message::Confirm => if let Some(confirmation) = self.confirmation.take() {
                match confirmation {
                    Confirmation::Quit(id) => return self.finish(id),
                    Confirmation::Stop(id) => self.close_session(id),
                    Confirmation::Restart(id) => self.start_session(id),
                }
            },
            Message::Cancel => { self.confirmation = None; self.focus_serial = self.focus_serial.wrapping_add(1); }
            Message::Dismiss => self.notice = None,
        }
        Task::none()
    }
    fn confirmation_view(&self, confirmation: Confirmation) -> Element<'_, Message> {
        let (title, detail, accept) = match confirmation {
            Confirmation::Quit(_) => ("退出并关闭终端？", "仍有运行中的进程。退出不会把它们保活到后台；关闭当前 PTY 可能中断任务。", "退出并关闭"),
            Confirmation::Stop(_) => ("关闭此终端？", "关闭标签会关闭 PTY 并终止直接子进程；任务可能被中断。目录和会话元数据仍保留。", "关闭终端"),
            Confirmation::Restart(id) => {
                let resume = self.store.session(id).is_some_and(|(_, s)| s.resume.is_some());
                (if resume { "执行配置的恢复命令？" } else { "重新启动这个会话？" },
                 if resume { "将运行 state.json 中显式配置的 resume 命令。是否恢复原聊天由 Agent 自身决定。" }
                 else { "旧进程已不在运行，且没有配置 Agent 恢复命令。继续会启动新进程，不会恢复旧聊天内容。" }, "继续")
            }
        };
        container(column![text(title).size(23), text(detail).size(15), row![button("取消").on_press(Message::Cancel), button(accept).on_press(Message::Confirm)].spacing(12)].spacing(20).max_width(540))
            .center_x(Length::Fill).center_y(Length::Fill).padding(30).into()
    }
    fn content_view(&self) -> Element<'_, Message> {
        if let Some(confirmation) = self.confirmation { return self.confirmation_view(confirmation); }
        match self.active {
            Content::Terminal(id) => {
                if let Some((snapshot_id, snapshot)) = &self.snapshot {
                    if *snapshot_id == id {
                        let terminal: Element<'_, Message> = TerminalView::new(id, snapshot, self.font, self.store.settings.font_size, self.focus_serial, self.terminal_focus, move |action| Message::Terminal(id, action)).into();
                        if let Some(status) = self.sessions.get(&id).and_then(|s| s.exited.as_ref()) {
                            return column![terminal, row![text(format!("进程已退出 · {status}")).size(12), button("重新启动").on_press(Message::Restart(id))].padding(6).spacing(12)].into();
                        }
                        return terminal;
                    }
                }
                container(column![text(if self.spawning.contains(&id) { "正在启动终端…" } else { "终端未运行" }).size(20), button("重新启动").on_press(Message::Restart(id))].spacing(15)).center_x(Length::Fill).center_y(Length::Fill).into()
            }
            Content::Preview => {
                if self.preview_loading { return container(text("正在读取文件…")).center_x(Length::Fill).center_y(Length::Fill).into(); }
                let Some(document) = &self.document else { return container(text("没有可显示的文件；从下方目录树双击文件打开。")).padding(24).into(); };
                let view: Element<'_, Message> = if document.preview.markdown && !document.raw {
                    markdown::view(&document.items, Theme::Light).map(Message::Link)
                } else { text(&document.pages[document.index]).font(self.font).size(14).into() };
                let label = format!("只读 · {} / {} 页{}{}", document.index + 1, document.pages.len(), if document.preview.truncated { " · 超出读取上限，内容已截断" } else { "" }, if document.preview.lossy { " · 编码含替代字符" } else { "" });
                let mut controls = row![text(label).size(12), Space::new().width(Length::Fill), button("复制本页").on_press(Message::CopyPreview)].spacing(8);
                if document.preview.markdown { controls = controls.push(button(if document.raw { "预览" } else { "源码" }).on_press(Message::ToggleRaw)); }
                if document.pages.len() > 1 { controls = controls.push(button("‹").on_press(Message::PreviewPage(-1))).push(button("›").on_press(Message::PreviewPage(1))); }
                column![container(controls).padding(8), scrollable(container(view).padding(24).width(Length::Fill)).height(Length::Fill)].into()
            }
            Content::Empty => container(column![text("DevHub").size(30), text("从左侧选择会话，或点击目录旁的 + 新建终端。"), text("双击文件只读预览。没有菜单、编辑器或工作树。")].spacing(14)).center_x(Length::Fill).center_y(Length::Fill).padding(24).into(),
        }
    }
    pub fn view(&self) -> Element<'_, Message> {
        let recent = column![
            container(row![text("最近工作目录 / 会话").size(14), Space::new().width(Length::Fill), button("+").on_press(Message::NewSelected).style(button::text)].spacing(4)).padding(8),
            VirtualList::new(&self.recent_rows, Message::Recent),
        ].height(Length::FillPortion((self.store.settings.recent_fraction * 1000.0) as u16));
        let system = column![
            container(text("系统目录树").size(14)).padding(10),
            VirtualList::new(&self.file_rows, Message::File),
            container(text("双击文件只读预览 · 目录按页读取").size(11)).padding(8),
        ].height(Length::FillPortion(((1.0 - self.store.settings.recent_fraction) * 1000.0) as u16));
        let sidebar = column![recent, Divider::new(Axis::Horizontal, Message::ResizeSections), system].width(self.store.settings.sidebar_width).height(Length::Fill);
        let mut tabs = iced::widget::Row::new().spacing(3);
        for id in &self.tabs {
            let title = self.store.session(*id).map(|(_, s)| s.title.chars().take(20).collect::<String>()).unwrap_or_else(|| "终端".into());
            let label = if self.active == Content::Terminal(*id) { format!("▣ {title}") } else { title };
            tabs = tabs.push(row![button(text(label).size(13)).on_press(Message::Activate(*id)), button(text("×").size(14)).on_press(Message::CloseSession(*id)).style(button::text)].spacing(0));
        }
        if let Some(document) = &self.document {
            tabs = tabs.push(row![button(text(paths::name(&document.preview.path)).size(13)).on_press(Message::ShowPreview), button("×").on_press(Message::ClosePreview).style(button::text)]);
        }
        if self.tabs.is_empty() && self.document.is_none() { tabs = tabs.push(text("终端 / 文件预览").size(13)); }
        let tabbar = scrollable(container(tabs).padding(5)).direction(scrollable::Direction::Horizontal(Default::default()));
        let mut right = column![tabbar, self.content_view()].width(Length::Fill).height(Length::Fill);
        if let Some(notice) = &self.notice {
            right = right.push(container(row![text(notice).size(12).width(Length::Fill), button("×").on_press(Message::Dismiss).style(button::text)].spacing(8)).padding(8));
        }
        row![sidebar, Divider::new(Axis::Vertical, Message::ResizeSidebar), right].height(Length::Fill).into()
    }
}
impl Drop for App {
    fn drop(&mut self) {
        // Flush has bounded metadata only, never a terminal transcript.
        let _ = self.persistence.flush_final(&self.store);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn preview_pages_preserve_every_character() {
        let input = format!("{}{}", "中文\n".repeat(1200), "x".repeat(200_000));
        let pages = text_pages(&input);
        assert_eq!(pages.concat(), input);
        assert!(pages.iter().all(|p| p.len() <= 64 * 1024 + 4));
    }
    #[test] fn empty_file_has_one_page() { assert_eq!(text_pages(""), vec![String::new()]); }
}
