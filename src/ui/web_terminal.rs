//! Native child WebView + bounded PTY/xterm.js bridge. No terminal parsing here.
use crate::model::{Id,Settings};
use crossbeam_channel::{Receiver,Sender,unbounded};
use iced::{Rectangle,Task,window};
use serde::Deserialize;
use serde_json::{Value,json};
use std::collections::HashMap;

#[derive(Debug,Clone,Deserialize)]
pub struct Event {
    pub kind:String,
    pub id:Option<Id>,pub epoch:Option<Id>,pub data:Option<String>,
    pub cols:Option<usize>,pub rows:Option<usize>,pub title:Option<String>,
}

pub struct Bridge {
    pub enabled:bool,pub ready:bool,pub events:Receiver<Event>,
    sender:Sender<Event>,created:bool,failed:bool,
    pub bounds:Option<Rectangle>,last_bounds:Option<Rectangle>,
    visible:bool,configuration:String,known:HashMap<Id,Id>,
    pub commands:Vec<Value>,
}
impl Bridge {
    pub fn new(enabled:bool)->Self{
        // PTY data never travels on this queue. Frontend input is one 32 KiB
        // batch in flight per session; output acknowledgments are also one per
        // session. Resize/title notifications are coalesced by the frontend.
        let(sender,events)=unbounded();
        Self{enabled,ready:false,events,sender,created:false,failed:false,bounds:None,last_bounds:None,
            visible:false,configuration:String::new(),known:HashMap::new(),commands:vec![]}
    }
    pub fn failed(&mut self){self.failed=true;}
    pub fn flush(&mut self,id:Option<window::Id>,active:Option<Id>,sessions:Vec<(Id,Id)>,settings:&Settings,focus:u64)->Task<Result<(),String>>{
        if !self.enabled||self.failed{return Task::none();}
        let Some(id)=id else{return Task::none()};
        if !self.created{
            let Some(bounds)=self.bounds else{return Task::none()};
            self.created=true;self.last_bounds=Some(bounds);
            let sender=self.sender.clone();
            return window::run(id,move|window|platform::create(window,bounds,sender));
        }
        let visible=active.is_some()&&self.ready;
        let visibility=(visible!=self.visible).then_some(visible);self.visible=visible;
        let bounds=if self.bounds!=self.last_bounds{self.last_bounds=self.bounds;self.bounds}else{None};
        if self.ready {
            for (session,epoch) in &sessions{
                if self.known.get(session)!=Some(epoch){
                    self.commands.insert(0,json!({"kind":"create","id":session,"epoch":epoch,"font":settings.font_family,"size":settings.font_size,"scrollback":settings.scrollback_lines}));
                    self.known.insert(*session,*epoch);
                    self.configuration.clear();
                }
            }
            self.known.retain(|session,_|{
                let keep=sessions.iter().any(|(s,_)|s==session);
                if !keep{self.commands.push(json!({"kind":"dispose","id":session}));}keep
            });
            let configuration=json!({"kind":"activate","id":active,"font":settings.font_family,"size":settings.font_size,"focus":focus});
            let signature=configuration.to_string();
            if self.configuration!=signature{self.configuration=signature;self.commands.push(configuration);}
        }
        if visibility.is_none()&&bounds.is_none()&&self.commands.is_empty(){return Task::none();}
        let commands=std::mem::take(&mut self.commands);
        window::run(id,move|_|platform::update(bounds,visibility,commands))
    }
}

// Reports actual Iced layout bounds; the child view follows splitters, resizing,
// tab/preview changes and modal visibility instead of assuming a fixed sidebar.
pub struct Bounds<'a,M>{on:Box<dyn Fn(Rectangle)->M+'a>}
impl<'a,M>Bounds<'a,M>{pub fn new(on:impl Fn(Rectangle)->M+'a)->Self{Self{on:Box::new(on)}}}
#[derive(Default)]struct BoundsState(Option<Rectangle>);
impl<M>iced::advanced::Widget<M,iced::Theme,iced::Renderer> for Bounds<'_,M>{
    fn size(&self)->iced::Size<iced::Length>{iced::Size::new(iced::Length::Fill,iced::Length::Fill)}
    fn tag(&self)->iced::advanced::widget::tree::Tag{iced::advanced::widget::tree::Tag::of::<BoundsState>()}
    fn state(&self)->iced::advanced::widget::tree::State{iced::advanced::widget::tree::State::new(BoundsState::default())}
    fn layout(&mut self,_:&mut iced::advanced::widget::Tree,_:&iced::Renderer,limits:&iced::advanced::layout::Limits)->iced::advanced::layout::Node{
        iced::advanced::layout::Node::new(limits.resolve(iced::Length::Fill,iced::Length::Fill,iced::Size::ZERO))
    }
    fn update(&mut self,tree:&mut iced::advanced::widget::Tree,_:&iced::Event,layout:iced::advanced::Layout<'_>,_:iced::mouse::Cursor,_:&iced::Renderer,_:&mut dyn iced::advanced::Clipboard,shell:&mut iced::advanced::Shell<'_,M>,_:&Rectangle){
        let state=tree.state.downcast_mut::<BoundsState>();let bounds=layout.bounds();
        if state.0!=Some(bounds){state.0=Some(bounds);shell.publish((self.on)(bounds));}
    }
    fn draw(&self,_:&iced::advanced::widget::Tree,_:&mut iced::Renderer,_:&iced::Theme,_:&iced::advanced::renderer::Style,_:iced::advanced::Layout<'_>,_:iced::mouse::Cursor,_:&Rectangle){}
}
impl<'a,M:'a>From<Bounds<'a,M>>for iced::Element<'a,M>{fn from(value:Bounds<'a,M>)->Self{Self::new(value)}}

#[cfg(windows)]mod platform {
    use super::*;
    use std::{borrow::Cow,cell::RefCell};
    use wry::{WebContext,WebView,WebViewBuilder,WebViewBuilderExtWindows};
    struct Host{view:WebView,_context:WebContext}
    thread_local!{static HOST:RefCell<Option<Host>>=const{RefCell::new(None)};}
    fn rect(bounds:Rectangle)->wry::Rect{wry::Rect{position:wry::dpi::LogicalPosition::new(bounds.x as f64,bounds.y as f64).into(),size:wry::dpi::LogicalSize::new(bounds.width as f64,bounds.height as f64).into()}}
    fn asset(path:&str)->Option<(&'static str,&'static [u8])>{
        Some(match path{
            "/"|"/index.html"=>("text/html; charset=utf-8",include_bytes!("../../assets/xterm/index.html")),
            "/terminal.js"=>("text/javascript; charset=utf-8",include_bytes!("../../assets/xterm/terminal.js")),
            "/xterm.js"=>("text/javascript; charset=utf-8",include_bytes!("../../assets/xterm/xterm.js")),
            "/addon-fit.js"=>("text/javascript; charset=utf-8",include_bytes!("../../assets/xterm/addon-fit.js")),
            "/addon-unicode11.js"=>("text/javascript; charset=utf-8",include_bytes!("../../assets/xterm/addon-unicode11.js")),
            "/xterm.css"=>("text/css; charset=utf-8",include_bytes!("../../assets/xterm/xterm.css")),
            _=>return None,
        })
    }
    pub fn create(window:&dyn iced::window::Window,bounds:Rectangle,sender:Sender<Event>)->Result<(),String>{
        let path=std::env::current_dir().map_err(|e|e.to_string())?.join(".tmp/webview").join(std::process::id().to_string());
        std::fs::create_dir_all(&path).map_err(|e|e.to_string())?;
        let mut context=WebContext::new(Some(path));
        let mut args="--disable-gpu".to_owned();
        if let Some(port)=std::env::var("AGENTDOCK_WEBVIEW_DEBUG_PORT").ok().and_then(|p|p.parse::<u16>().ok()).filter(|p|*p>1024){args.push_str(&format!(" --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"));}
        let view=WebViewBuilder::new_with_web_context(&mut context)
            .with_bounds(rect(bounds)).with_visible(false).with_focused(false)
            .with_background_color((17,21,29,255)).with_additional_browser_args(args)
            .with_custom_protocol("agentdock".into(),|_,request|{
                let resource=asset(request.uri().path());let status=if resource.is_some(){200}else{404};
                let(content_type,body)=resource.unwrap_or(("text/plain",b"Not found"));
                wry::http::Response::builder().status(status).header("Content-Type",content_type)
                    .header("Content-Security-Policy","default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src data:; connect-src 'none'")
                    .body(Cow::Borrowed(body)).unwrap()
            })
            .with_navigation_handler(|url|url=="http://agentdock.localhost/"||url=="agentdock://localhost/")
            .with_new_window_req_handler(|_,_|wry::NewWindowResponse::Deny)
            .with_ipc_handler(move|request|{
                if request.body().len()<=128*1024{if let Ok(event)=serde_json::from_str::<Event>(request.body()){let _=sender.send(event);}}
            })
            .with_url("agentdock://localhost/").build_as_child(&window).map_err(|e|e.to_string())?;
        HOST.with(|host|*host.borrow_mut()=Some(Host{view,_context:context}));Ok(())
    }
    pub fn update(bounds:Option<Rectangle>,visible:Option<bool>,commands:Vec<Value>)->Result<(),String>{
        HOST.with(|host|{
            let host=host.borrow();let Some(host)=host.as_ref()else{return Err("WebView 未初始化".into())};
            if let Some(bounds)=bounds{host.view.set_bounds(rect(bounds)).map_err(|e|e.to_string())?;}
            if let Some(visible)=visible{host.view.set_visible(visible).map_err(|e|e.to_string())?;if visible{host.view.focus().map_err(|e|e.to_string())?;}else{host.view.focus_parent().map_err(|e|e.to_string())?;}}
            if !commands.is_empty(){host.view.evaluate_script(&format!("window.agentdock.receive({});",serde_json::to_string(&commands).unwrap())).map_err(|e|e.to_string())?;}
            Ok(())
        })
    }
}
#[cfg(not(windows))]mod platform{
    use super::*;
    pub fn create(_: &dyn iced::window::Window,_:Rectangle,_:Sender<Event>)->Result<(),String>{Err("此平台使用原生终端".into())}
    pub fn update(_:Option<Rectangle>,_:Option<bool>,_:Vec<Value>)->Result<(),String>{Ok(())}
}
