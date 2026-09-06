//! A real Iced widget: keyboard/IME/mouse -> VT core, screen cells -> software renderer.
use super::paint;
use crate::{model::Id,terminal::{Snapshot,input::{self,Action},engine::SelectionMode}};
use iced::advanced::{layout,renderer,text,widget::{self,Tree},Clipboard,Layout,Shell,Widget};
use iced::advanced::text::Paragraph as _;
use iced::advanced::renderer::Renderer as _;
use iced::{Color,Element,Event,Font,Length,Pixels,Point,Rectangle,Renderer,Size,Theme,keyboard,mouse,window};
use iced::advanced::input_method::{self,InputMethod,Preedit,Purpose};
use std::time::{Duration,Instant};
use wezterm_term::{MouseButton,MouseEvent,MouseEventKind,KeyCode};

pub struct TerminalView<'a,M>{id:Id,snapshot:&'a Snapshot,font:Font,size:f32,serial:u64,allow_input:bool,on:Box<dyn Fn(Action)->M+'a>}
impl<'a,M>TerminalView<'a,M>{pub fn new(id:Id,snapshot:&'a Snapshot,font:Font,size:f32,serial:u64,allow_input:bool,on:impl Fn(Action)->M+'a)->Self{Self{id,snapshot,font,size,serial,allow_input,on:Box::new(on)}}}
struct State{
    id:Option<Id>,serial:u64,focused:bool,modifiers:keyboard::Modifiers,width:f32,height:f32,
    font_key:(Font,u32),grid:(usize,usize),generation:usize,preedit:Option<Preedit>,composing:bool,
    dragging:bool,button:MouseButton,click:Option<(Instant,usize,usize,u8)>,
    blink_at:Instant,blink:bool,suppressed:Vec<KeyCode>,
}
impl Default for State{fn default()->Self{Self{id:None,serial:0,focused:true,modifiers:Default::default(),width:9.0,height:21.0,font_key:(Font::MONOSPACE,0),grid:(0,0),generation:usize::MAX,preedit:None,composing:false,dragging:false,button:MouseButton::None,click:None,blink_at:Instant::now(),blink:true,suppressed:vec![]}}}
fn area(b:Rectangle)->Rectangle{Rectangle{x:b.x+10.0,y:b.y+10.0,width:(b.width-28.0).max(1.0),height:(b.height-20.0).max(1.0)}}
fn location(s:&State,a:Rectangle,p:Point,v:&Snapshot)->(usize,usize){
    ((((p.x-a.x).max(0.0)/s.width)as usize).min(v.columns.saturating_sub(1)),(((p.y-a.y).max(0.0)/s.height)as usize).min(v.rows.saturating_sub(1)))
}
fn mouse_action(s:&State,a:Rectangle,p:Point,v:&Snapshot,kind:MouseEventKind,button:MouseButton)->Action{
    let(col,row)=location(s,a,p,v);Action::Mouse(MouseEvent{kind,button,x:col,y:row as i64,x_pixel_offset:0,y_pixel_offset:0,modifiers:input::modifiers(s.modifiers)})
}
fn run_end(cells: &[crate::terminal::engine::Cell], start: usize) -> usize {
    let first = &cells[start];
    let ascii = |cell: &crate::terminal::engine::Cell| cell.width == 1 && cell.text.len() == 1 && cell.text.is_ascii();
    let mut end = start + 1;
    if ascii(first) {
        while end < cells.len() {
            let next = &cells[end];
            if !ascii(next) || next.column != cells[end - 1].column + 1 || next.fg != first.fg || next.bg != first.bg
                || next.bold != first.bold || next.italic != first.italic || next.underline != first.underline
                || next.strike != first.strike || next.selected != first.selected { break; }
            end += 1;
        }
    }
    end
}
impl<M>Widget<M,Theme,Renderer> for TerminalView<'_,M>{
    fn size(&self)->Size<Length>{Size::new(Length::Fill,Length::Fill)}
    fn tag(&self)->widget::tree::Tag{widget::tree::Tag::of::<State>()}
    fn state(&self)->widget::tree::State{widget::tree::State::new(State::default())}
    fn layout(&mut self,tree:&mut Tree,_renderer:&Renderer,limits:&layout::Limits)->layout::Node{
        let s=tree.state.downcast_mut::<State>();
        if s.font_key!=(self.font,self.size.to_bits()){
            let paragraph=<Renderer as text::Renderer>::Paragraph::with_text(text::Text{content:"M",bounds:Size::new(1000.0,100.0),font:self.font,size:Pixels(self.size),line_height:text::LineHeight::Relative(1.4),align_x:text::Alignment::Left,align_y:iced::alignment::Vertical::Top,shaping:text::Shaping::Advanced,wrapping:text::Wrapping::None});
            s.width=paragraph.min_bounds().width.max(1.0);s.height=(self.size*1.4).ceil();s.font_key=(self.font,self.size.to_bits());
        }
        layout::Node::new(limits.resolve(Length::Fill,Length::Fill,Size::ZERO))
    }
    fn update(&mut self,tree:&mut Tree,event:&Event,layout:Layout<'_>,cursor:mouse::Cursor,_renderer:&Renderer,_clipboard:&mut dyn Clipboard,shell:&mut Shell<'_,M>,_viewport:&Rectangle){
        let s=tree.state.downcast_mut::<State>();
        macro_rules! send{($a:expr)=>{{shell.publish((self.on)($a));shell.capture_event();}}}
        if s.id!=Some(self.id)||s.serial!=self.serial{s.id=Some(self.id);s.serial=self.serial;s.focused=true;s.grid=(0,0);s.preedit=None;s.composing=false;s.dragging=false;s.suppressed.clear();s.generation=usize::MAX;shell.publish((self.on)(Action::Focus(true)));shell.request_redraw();}
        if !self.allow_input{s.focused=false;}
        let bounds=layout.bounds();let a=area(bounds);
        match event{
            Event::Window(window::Event::RedrawRequested(now))=>{
                let grid=((a.width/s.width).floor().clamp(2.0,1000.0)as usize,(a.height/s.height).floor().clamp(1.0,500.0)as usize);
                if s.grid!=grid{s.grid=grid;send!(Action::Resize{cols:grid.0,rows:grid.1,width:a.width as usize,height:a.height as usize});}
                if now.duration_since(s.blink_at)>=Duration::from_millis(600){s.blink=!s.blink;s.blink_at=*now;}
                if s.focused{
                    let ime:InputMethod=InputMethod::Enabled{cursor:Rectangle{x:a.x+self.snapshot.cursor_column as f32*s.width,y:a.y+self.snapshot.cursor_row as f32*s.height,width:s.width,height:s.height},purpose:Purpose::Terminal,preedit:s.preedit.clone()};
                    shell.request_input_method(&ime);shell.request_redraw_at(*now+Duration::from_millis(600));
                }
            }
            Event::Window(window::Event::Unfocused)=>{s.focused=false;s.preedit=None;s.composing=false;s.dragging=false;shell.publish((self.on)(Action::Focus(false)));}
            Event::Window(window::Event::Focused)=>{s.focused=self.allow_input;shell.publish((self.on)(Action::Focus(s.focused)));}
            Event::Keyboard(keyboard::Event::ModifiersChanged(m))=>s.modifiers=*m,
            Event::InputMethod(ime)if s.focused=>match ime{
                input_method::Event::Preedit(value,selection)=>{s.composing=!value.is_empty();s.preedit=if value.is_empty(){None}else{Some(Preedit{content:value.clone(),selection:selection.clone(),text_size:Some(Pixels(self.size))})};shell.request_redraw();shell.capture_event();}
                input_method::Event::Commit(value)=>{s.preedit=None;s.composing=false;send!(Action::Text(value.clone()));}
                input_method::Event::Closed=>{s.preedit=None;s.composing=false;}
                _=>{},
            },
            Event::Keyboard(keyboard::Event::KeyPressed{key,modified_key,modifiers,text,..})if s.focused&&!s.composing=>{
                s.modifiers=*modifiers;s.blink=true;s.blink_at=Instant::now();
                let letter=match key.as_ref(){keyboard::Key::Character(c)=>c.to_ascii_lowercase(),_=>String::new()};
                let local=if modifiers.control()&&modifiers.shift()&&letter=="c"{Some(Action::Copy)}
                else if(modifiers.control()&&modifiers.shift()&&letter=="v")||(modifiers.shift()&&*key==keyboard::Key::Named(keyboard::key::Named::Insert)){Some(Action::RequestPaste)}
                else if modifiers.shift()&&!modifiers.control()&&*key==keyboard::Key::Named(keyboard::key::Named::PageUp){Some(Action::Scroll(self.snapshot.rows as i32-1))}
                else if modifiers.shift()&&!modifiers.control()&&*key==keyboard::Key::Named(keyboard::key::Named::PageDown){Some(Action::Scroll(-(self.snapshot.rows as i32-1)))}
                else if modifiers.control()&&matches!(letter.as_str(),"="|"+"|"-"){Some(Action::Zoom(if letter=="-"{-1}else{1}))}else{None};
                if let Some(action)=local{if let Some(code)=input::code(key){if !s.suppressed.contains(&code){s.suppressed.push(code);}}send!(action);}
                else{
                    let chosen=if modifiers.control(){key}else{modified_key};
                    let altgr=modifiers.control()&&modifiers.alt()&&text.as_ref().is_some_and(|t|t.chars().any(|c|!c.is_ascii()));
                    if altgr||(matches!(chosen,keyboard::Key::Character(_))&&!modifiers.control()&&!modifiers.alt()&&text.as_ref().is_some_and(|t|t.chars().count()>1)){
                        if let Some(value)=text{send!(Action::Text(value.to_string()));}
                    }else if let Some(code)=input::code(chosen){send!(Action::Key{key:code,modifiers:input::modifiers(*modifiers),pressed:true});}
                    else if let Some(value)=text{if !modifiers.control()&&!modifiers.alt(){send!(Action::Text(value.to_string()));}}
                }
            }
            Event::Keyboard(keyboard::Event::KeyReleased{key,modified_key,modifiers,..})if s.focused&&!s.composing=>{
                if let Some(code)=input::code(key){if let Some(i)=s.suppressed.iter().position(|c|*c==code){s.suppressed.remove(i);shell.capture_event();return;}}
                if let Some(code)=input::code(if modifiers.control(){key}else{modified_key}){send!(Action::Key{key:code,modifiers:input::modifiers(*modifiers),pressed:false});}
            }
            Event::Mouse(mouse::Event::ButtonPressed(button))=>{
                if let Some(point)=cursor.position_over(bounds){
                    s.focused=true;shell.publish((self.on)(Action::Focus(true)));
                    let(col,row)=location(s,a,point,self.snapshot);
                    let button=match button{mouse::Button::Left=>MouseButton::Left,mouse::Button::Middle=>MouseButton::Middle,mouse::Button::Right=>MouseButton::Right,_=>MouseButton::None};s.button=button;
                    if self.snapshot.mouse_grabbed&&!s.modifiers.shift(){send!(mouse_action(s,a,point,self.snapshot,MouseEventKind::Press,button));}
                    else if button==MouseButton::Left{
                        let now=Instant::now();let n=if let Some((t,c,r,n))=s.click{if now.duration_since(t)<Duration::from_millis(450)&&c==col&&r==row{n%3+1}else{1}}else{1};s.click=Some((now,col,row,n));s.dragging=n==1;
                        send!(Action::Select{col,row,mode:match n{2=>SelectionMode::Word,3=>SelectionMode::Line,_=>SelectionMode::Character}});
                    }else if button==MouseButton::Right{send!(Action::RequestPaste);}
                }else if s.focused{s.focused=false;shell.publish((self.on)(Action::Focus(false)));}
            }
            Event::Mouse(mouse::Event::CursorMoved{position})=>{
                if s.dragging{let(col,row)=location(s,a,*position,self.snapshot);send!(Action::Extend{col,row});}
                else if cursor.is_over(bounds)&&self.snapshot.mouse_grabbed&&!s.modifiers.shift(){send!(mouse_action(s,a,*position,self.snapshot,MouseEventKind::Move,s.button));}
            }
            Event::Mouse(mouse::Event::ButtonReleased(_))=>{
                s.dragging=false;
                if s.button!=MouseButton::None&&self.snapshot.mouse_grabbed&&!s.modifiers.shift(){if let Some(p)=cursor.position(){send!(mouse_action(s,a,p,self.snapshot,MouseEventKind::Release,s.button));}}
                s.button=MouseButton::None;
            }
            Event::Mouse(mouse::Event::WheelScrolled{delta})if cursor.is_over(bounds)=>{
                let value=match delta{mouse::ScrollDelta::Lines{y,..}=>*y*3.0,mouse::ScrollDelta::Pixels{y,..}=>*y/s.height};
                let rows=if value.abs()<1.0{value.signum()as i32}else{value as i32};
                if(self.snapshot.mouse_grabbed||self.snapshot.alternate)&&!s.modifiers.shift(){if let Some(p)=cursor.position(){let button=if rows>=0{MouseButton::WheelUp(rows.unsigned_abs()as usize)}else{MouseButton::WheelDown(rows.unsigned_abs()as usize)};send!(mouse_action(s,a,p,self.snapshot,MouseEventKind::Press,button));}}
                else{send!(Action::Scroll(rows));}
            }
            _=>{},
        }
        if s.generation!=self.snapshot.generation{s.generation=self.snapshot.generation;shell.request_redraw();}
    }
    fn draw(&self,tree:&Tree,r:&mut Renderer,_theme:&Theme,_style:&renderer::Style,layout:Layout<'_>,_cursor:mouse::Cursor,viewport:&Rectangle){
        let s=tree.state.downcast_ref::<State>();let b=layout.bounds();let a=area(b);let Some(clip)=a.intersection(viewport)else{return};
        paint::rect(r,b,paint::rgba(self.snapshot.background));
        r.start_layer(clip);
        for(row,cells)in self.snapshot.lines.iter().enumerate(){
            let y=a.y+row as f32*s.height;if y>=a.y+a.height{break;}
            let mut index = 0;
            while index < cells.len() {
                let cell = &cells[index];
                let end = run_end(cells, index);
                let last = &cells[end - 1];
                let content: String = cells[index..end].iter().map(|cell| cell.text.as_str()).collect();
                index = end;
                let bounds=Rectangle{x:a.x+cell.column as f32*s.width,y,width:(last.column + last.width - cell.column) as f32*s.width,height:s.height};
                let Some(crop)=bounds.intersection(&clip)else{continue};
                if cell.selected || cell.bg != self.snapshot.background { paint::rect(r,crop,if cell.selected{Color::from_rgb8(51,78,111)}else{paint::rgba(cell.bg)}); }
                let font=Font{weight:if cell.bold{iced::font::Weight::Bold}else{iced::font::Weight::Normal},style:if cell.italic{iced::font::Style::Italic}else{iced::font::Style::Normal},..self.font};
                let fg=paint::rgba(cell.fg);
                if !content.trim().is_empty(){paint::terminal_label(r,&content,bounds.position(),bounds.size(),font,self.size,fg,crop);}
                if cell.underline{paint::rect(r,Rectangle{y:y+s.height-2.0,height:1.0,..crop},fg);}
                if cell.strike{paint::rect(r,Rectangle{y:y+s.height*0.55,height:1.0,..crop},fg);}
            }
        }
        r.end_layer();
        if self.snapshot.cursor_visible&&self.snapshot.cursor_row<self.snapshot.rows&&s.preedit.is_none(){
            let shape=format!("{:?}",self.snapshot.cursor_shape);let blinking=shape.contains("Blinking")||shape=="Default";
            if !blinking||s.blink||!s.focused{
                let mut cursor=Rectangle{x:a.x+self.snapshot.cursor_column.min(self.snapshot.columns.saturating_sub(1))as f32*s.width,y:a.y+self.snapshot.cursor_row as f32*s.height,width:s.width,height:s.height};
                if shape.contains("Underline"){cursor.y+=s.height-2.0;cursor.height=2.0;}else if shape.contains("Bar")||!s.focused{cursor.width=2.0;}
                if let Some(cursor)=cursor.intersection(&clip){paint::rect(r,cursor,Color::from_rgba8(153,210,255,0.55));}
            }
        }
        if self.snapshot.history>0{
            let h=(a.height*self.snapshot.rows as f32/(self.snapshot.history+self.snapshot.rows)as f32).max(20.0).min(a.height);
            let y=a.y+(a.height-h)*(1.0-self.snapshot.scroll_offset as f32/self.snapshot.history as f32);
            paint::rect(r,Rectangle{x:b.x+b.width-6.0,y,width:3.0,height:h},Color::from_rgb8(67,78,96));
        }
    }
    fn mouse_interaction(&self,_tree:&Tree,layout:Layout<'_>,cursor:mouse::Cursor,_viewport:&Rectangle,_renderer:&Renderer)->mouse::Interaction{if cursor.is_over(layout.bounds()){mouse::Interaction::Text}else{mouse::Interaction::None}}
}
impl<'a,M:'a>From<TerminalView<'a,M>>for Element<'a,M>{fn from(value:TerminalView<'a,M>)->Self{Element::new(value)}}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Settings, terminal::Engine};
    #[test]
    fn batches_ascii_but_preserves_color_selection_and_wide_cells() {
        let mut engine = Engine::new(&Settings::default(), Box::new(std::io::sink()));
        engine.advance("abc\x1b[31mde\x1b[0m中文!");
        let snapshot = engine.snapshot(); let cells = &snapshot.lines[0];
        assert_eq!(run_end(cells, 0), 3);
        assert_eq!(run_end(cells, 3), 5);
        assert_eq!(run_end(cells, 5), 6);
        engine.select(1, 0, SelectionMode::Character);
        let snapshot = engine.snapshot();
        assert_eq!(run_end(&snapshot.lines[0], 0), 1);
    }
}
