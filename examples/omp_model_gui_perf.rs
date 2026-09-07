//! Runs /model in an isolated GUI session and measures event-loop stalls.
use agentdock::{cli::Options,terminal::input::Action,ui::{App,app::Message}};
use iced::{Element,Subscription,Task,window};
use std::time::{Instant,Duration};
#[derive(Debug,Clone)]enum Event{App(Message),Pulse(Instant)}
struct Probe{app:App,start:Instant,last:Instant,id:Option<uuid::Uuid>,phase:usize,menu_max_gap:Duration}
impl Probe{
    fn update(&mut self,event:Event)->Task<Event>{
        match event{
            Event::App(message)=>{
                if let Message::Terminal(id,_)=&message{self.id=Some(*id);}
                let output=matches!(&message,Message::TerminalOutput(_));
                let start=Instant::now();let task=self.app.update(message).map(Event::App);
                if output||start.elapsed()>Duration::from_millis(8){eprintln!("update elapsed_ms={:.2} cost_ms={:.2} output={output}",self.start.elapsed().as_secs_f64()*1000.,start.elapsed().as_secs_f64()*1000.);}
                task
            }
            Event::Pulse(scheduled)=>{
                let now=Instant::now();
                let gap=now.duration_since(self.last);self.last=now;
                if (1..=2).contains(&self.phase){self.menu_max_gap=self.menu_max_gap.max(gap);}
                if gap>Duration::from_millis(40){eprintln!("ui_gap elapsed_ms={:.2} gap_ms={:.2} delivery_ms={:.2}",self.start.elapsed().as_secs_f64()*1000.,gap.as_secs_f64()*1000.,now.saturating_duration_since(scheduled).as_secs_f64()*1000.);}
                if self.start.elapsed()>Duration::from_secs(16){eprintln!("menu_max_gap_ms={:.2}",self.menu_max_gap.as_secs_f64()*1000.);return window::oldest().and_then(window::close);}
                if let Some(id)=self.id{
                    let seconds=self.start.elapsed().as_secs_f64();
                    let action=match self.phase{
                        0 if seconds>=8.0=>Some(Action::Text("/model".into())),
                        1 if seconds>=8.3=>Some(Action::Key{key:wezterm_term::KeyCode::Enter,modifiers:wezterm_term::KeyModifiers::NONE,pressed:true}),
                        2 if seconds>=12.0=>Some(Action::Key{key:wezterm_term::KeyCode::Escape,modifiers:wezterm_term::KeyModifiers::NONE,pressed:true}),
                        _=>None,
                    };
                    if let Some(action)=action{eprintln!("input phase={} elapsed_ms={:.2}",self.phase,seconds*1000.);self.phase+=1;return self.app.update(Message::Terminal(id,action)).map(Event::App);}
                }
                Task::none()
            }
        }
    }
    fn view(&self)->Element<'_,Event>{self.app.view().map(Event::App)}
    fn subscription(&self)->Subscription<Event>{Subscription::batch([self.app.subscription().map(Event::App),iced::time::every(Duration::from_millis(16)).map(Event::Pulse)])}
}
fn main()->anyhow::Result<()>{
    let root=std::env::current_dir()?;
    anyhow::ensure!(std::env::var_os("PI_CODING_AGENT_SESSION_DIR").map(std::path::PathBuf::from).is_some_and(|p|p.starts_with(root.join(".tmp"))),"Set PI_CODING_AGENT_SESSION_DIR to a directory under .tmp before running this probe");
    let args=vec!["--state-dir".to_owned(),root.join(".tmp/omp-gui-state").to_string_lossy().into_owned(),"--command".into(),"omp".into(),"--arg".into(),"--profile".into(),"--arg".into(),"default".into()];
    let mut options=Options::parse(args.into_iter().map(Into::into))?;
    options.native_terminal=true; // This probe measures the original Iced renderer.
    let app=App::create(options)?;
    let state=std::cell::RefCell::new(Some(app));
    iced::application(move||Probe{app:state.borrow_mut().take().unwrap(),start:Instant::now(),last:Instant::now(),id:None,phase:0,menu_max_gap:Duration::ZERO},Probe::update,Probe::view)
        .title("AgentDock /model performance probe").theme(agentdock::ui::style::theme()).default_font(agentdock::ui::style::font())
        .subscription(Probe::subscription).window_size((1240.,820.)).run()?;
    Ok(())
}
