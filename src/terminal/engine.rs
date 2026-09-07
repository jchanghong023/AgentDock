//! Mature WezTerm VT parser, with an application-owned software view.
use crate::model::Settings;
use std::{cell::RefCell,io::Write,sync::Arc};
use termwiz::{cell::{Intensity,Underline},surface::{CursorVisibility,CursorShape}};
use wezterm_term::{Terminal,TerminalConfiguration,TerminalSize,KeyCode,KeyModifiers};
use wezterm_term::color::{ColorPalette,RgbColor,SrgbaTuple};
#[derive(Debug)]struct Config{scrollback:usize}
impl TerminalConfiguration for Config{
    fn scrollback_size(&self)->usize{self.scrollback}
    fn color_palette(&self)->ColorPalette{let mut p=ColorPalette::default();p.background=RgbColor::new_8bpc(17,21,29).into();p.foreground=RgbColor::new_8bpc(220,227,236).into();p}
    fn enable_kitty_keyboard(&self)->bool{true}
    fn enable_kitty_graphics(&self)->bool{false}
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]struct Position{row:isize,column:usize}
#[derive(Debug,Clone,Copy)]pub enum SelectionMode{Character,Word,Line}
#[derive(Debug,Clone,PartialEq,Eq)]struct Selection{anchor:Position,extent:Position}
impl Selection{
    fn columns(&self,row:isize,cols:usize)->Option<(usize,usize)>{
        let(a,b)=self.ordered();
        if row<a.row||row>b.row{return None;}
        Some((if row==a.row{a.column}else{0},if row==b.row{b.column}else{cols.saturating_sub(1)}))
    }
    fn ordered(&self)->(Position,Position){if self.anchor<=self.extent{(self.anchor,self.extent)}else{(self.extent,self.anchor)}}
    fn contains(&self,p:Position)->bool{let(a,b)=self.ordered();p>=a&&p<=b}
}
#[derive(Debug,Clone,PartialEq)]pub struct Cell{
    pub column:usize,pub width:usize,pub text:String,pub fg:[f32;4],pub bg:[f32;4],
    pub bold:bool,pub italic:bool,pub underline:bool,pub strike:bool,pub selected:bool,
}
#[derive(Debug,Clone)]pub struct Snapshot{
    pub lines:Vec<Arc<Vec<Cell>>>,pub rows:usize,pub columns:usize,pub cursor_column:usize,pub cursor_row:usize,
    pub cursor_visible:bool,pub cursor_shape:CursorShape,pub background:[f32;4],pub mouse_grabbed:bool,
    pub alternate:bool,pub scroll_offset:usize,pub history:usize,pub generation:usize,
}
fn rgba(c:SrgbaTuple)->[f32;4]{[c.0 as f32,c.1 as f32,c.2 as f32,c.3 as f32]}
#[derive(Default)]struct SnapshotCache {
    palette:Option<ColorPalette>,columns:usize,alternate:bool,reverse:bool,
    selection:Option<Selection>,rows:Vec<(isize,usize,Arc<Vec<Cell>>)>,
}
pub struct Engine{pub terminal:Terminal,offset:usize,selection:Option<Selection>,generation:usize,cache:RefCell<SnapshotCache>}
impl Engine{
    pub fn new(settings:&Settings,writer:Box<dyn Write+Send>)->Self{
        let mut terminal=Terminal::new(TerminalSize::default(),Arc::new(Config{scrollback:settings.scrollback_lines}),"AgentDock",env!("CARGO_PKG_VERSION"),writer);
        #[cfg(windows)]terminal.enable_conpty_quirks();
        terminal.focus_changed(true);Self{terminal,offset:0,selection:None,generation:0,cache:RefCell::default()}
    }
    pub fn advance(&mut self,bytes:impl AsRef<[u8]>){
        let before=self.terminal.screen().visible_row_to_stable_row(0);self.terminal.advance_bytes(bytes);
        let after=self.terminal.screen().visible_row_to_stable_row(0);
        if self.offset>0{self.offset=self.offset.saturating_add(after.saturating_sub(before).max(0)as usize);}
        if self.terminal.is_alt_screen_active(){self.offset=0;}self.clamp();self.changed();
    }
    fn changed(&mut self){self.generation=self.generation.wrapping_add(1);}
    pub fn generation(&self)->usize{self.generation}
    fn clamp(&mut self){self.offset=self.offset.min(self.terminal.screen().scrollback_rows().saturating_sub(self.terminal.get_size().rows));}
    pub fn resize(&mut self,cols:usize,rows:usize,width:usize,height:usize){self.terminal.resize(TerminalSize{rows:rows.clamp(1,500),cols:cols.clamp(2,1000),pixel_width:width,pixel_height:height,dpi:96});self.selection=None;self.clamp();self.changed();}
    pub fn scroll(&mut self,rows:i32){if self.terminal.is_alt_screen_active(){return;}self.offset=if rows>=0{self.offset.saturating_add(rows as usize)}else{self.offset.saturating_sub(rows.unsigned_abs()as usize)};self.clamp();self.changed();}
    pub fn bottom(&mut self){if self.offset!=0||self.selection.is_some(){self.offset=0;self.selection=None;self.changed();}}
    pub fn text(&mut self,text:&str)->anyhow::Result<()>{self.bottom();for c in text.chars(){self.terminal.key_down(KeyCode::Char(c),KeyModifiers::NONE)?;}Ok(())}
    pub fn paste(&mut self,text:&str)->anyhow::Result<()>{anyhow::ensure!(text.len()<=1024*1024,"单次粘贴上限 1 MiB");self.bottom();self.terminal.send_paste(text)}
    fn first(&self)->usize{self.terminal.screen().scrollback_rows().saturating_sub(self.terminal.get_size().rows).saturating_sub(self.offset)}
    fn position(&self,col:usize,row:usize)->Position{
        let size=self.terminal.get_size();Position{row:self.terminal.screen().phys_to_stable_row_index(self.first()+row.min(size.rows.saturating_sub(1))),column:col.min(size.cols.saturating_sub(1))}
    }
    pub fn select(&mut self,col:usize,row:usize,mode:SelectionMode){
        let mut a=self.position(col,row);let mut b=a;
        match mode{
            SelectionMode::Character=>{},
            SelectionMode::Line=>{a.column=0;b.column=self.terminal.get_size().cols.saturating_sub(1);},
            SelectionMode::Word=>{
                if let Some(phys)=self.terminal.screen().stable_row_to_phys(a.row){
                    for line in self.terminal.screen().lines_in_phys_range(phys..phys+1){
                        let cells:Vec<_>=line.visible_cells().collect();
                        if let Some(i)=cells.iter().position(|c|c.cell_index()<=col&&col<c.cell_index()+c.width()){
                            let category=|s:&str|if s.chars().all(char::is_whitespace){0}else if s.chars().all(|c|c.is_alphanumeric()||c=='_'){1}else{2};
                            let kind=category(cells[i].str());let mut first=i;let mut last=i;
                            while first>0&&category(cells[first-1].str())==kind{first-=1;}
                            while last+1<cells.len()&&category(cells[last+1].str())==kind{last+=1;}
                            a.column=cells[first].cell_index();b.column=cells[last].cell_index()+cells[last].width().saturating_sub(1);
                        }
                    }
                }
            }
        }
        self.selection=Some(Selection{anchor:a,extent:b});self.changed();
    }
    pub fn extend(&mut self,col:usize,row:usize){let p=self.position(col,row);if let Some(s)=&mut self.selection{s.extent=p;}self.changed();}
    pub fn selection_text(&self)->String{
        let Some(s)=&self.selection else{return String::new()};let(a,b)=s.ordered();let screen=self.terminal.screen();
        let(Some(first),Some(last))=(screen.stable_row_to_phys(a.row),screen.stable_row_to_phys(b.row))else{return String::new()};
        let mut out=String::new();
        for(i,line)in screen.lines_in_phys_range(first..last+1).iter().enumerate(){
            let row=screen.phys_to_stable_row_index(first+i);let mut text=String::new();
            for c in line.visible_cells(){if(row>a.row||c.cell_index()+c.width().saturating_sub(1)>=a.column)&&(row<b.row||c.cell_index()<=b.column){text.push_str(c.str());}}
            out.push_str(text.trim_end_matches(' '));if i<last-first&&!line.last_cell_was_wrapped(){out.push('\n');}
            if out.len()>1024*1024{break;}
        }out
    }
    pub fn snapshot(&self)->Snapshot{
        let t=&self.terminal;let size=t.get_size();let palette=t.palette();let start=self.first();
        let mut cache=self.cache.borrow_mut();
        let alternate=t.is_alt_screen_active();let reverse=t.get_reverse_video();
        if cache.palette.as_ref()!=Some(&palette)||cache.columns!=size.cols||cache.alternate!=alternate||cache.reverse!=reverse {
            cache.rows.clear();cache.palette=Some(palette.clone());cache.columns=size.cols;
            cache.alternate=alternate;cache.reverse=reverse;
        }
        let mut next=Vec::with_capacity(size.rows);
        // Borrow core lines instead of cloning the entire screen on every update.
        t.screen().with_phys_lines(start..start+size.rows,|source|{
        for (i,line) in source.iter().enumerate(){
            let row=t.screen().phys_to_stable_row_index(start+i);
            let seqno=line.current_seqno();
            if let Ok(index)=cache.rows.binary_search_by_key(&row,|(r,_,_)|*r){
                let (_,previous,cells)=&cache.rows[index];
                let old_selection=cache.selection.as_ref().and_then(|s|s.columns(row,size.cols));
                let new_selection=self.selection.as_ref().and_then(|s|s.columns(row,size.cols));
                if *previous==seqno&&old_selection==new_selection{next.push((row,seqno,Arc::clone(cells)));continue;}
            }
            let cells=line.visible_cells().filter(|c|c.cell_index()<size.cols).map(|c|{
                let a=c.attrs();let mut fg=rgba(palette.resolve_fg(a.foreground()));let mut bg=rgba(palette.resolve_bg(a.background()));
                if a.reverse()^t.get_reverse_video(){std::mem::swap(&mut fg,&mut bg);}
                if a.intensity()==Intensity::Half{for j in 0..3{fg[j]=(fg[j]+bg[j])*0.5;}}
                let selected=self.selection.as_ref().is_some_and(|s|(0..c.width()).any(|n|s.contains(Position{row,column:c.cell_index()+n})));
                Cell{column:c.cell_index(),width:c.width().min(size.cols-c.cell_index()),text:if a.invisible(){String::new()}else{c.str().into()},fg,bg,bold:a.intensity()==Intensity::Bold,italic:a.italic(),underline:a.underline()!=Underline::None,strike:a.strikethrough(),selected}
            }).collect();
            next.push((row,seqno,Arc::new(cells)));
        }});
        let lines=next.iter().map(|(_,_,cells)|Arc::clone(cells)).collect();cache.rows=next;cache.selection=self.selection.clone();
        let cursor=t.cursor_pos();
        Snapshot{lines,rows:size.rows,columns:size.cols,cursor_column:cursor.x,cursor_row:cursor.y.max(0)as usize,cursor_visible:cursor.visibility==CursorVisibility::Visible&&self.offset==0,cursor_shape:cursor.shape,background:rgba(palette.background),mouse_grabbed:t.is_mouse_grabbed(),alternate:t.is_alt_screen_active(),scroll_offset:self.offset,history:t.screen().scrollback_rows().saturating_sub(size.rows),generation:self.generation}
    }
    pub fn visible_text(&self)->String{self.snapshot().lines.iter().map(|line|line.iter().map(|c|c.text.as_str()).collect::<String>().trim_end().to_owned()).collect::<Vec<_>>().join("\n")}
}
#[cfg(test)]mod tests{
    use super::*;use std::sync::Mutex;
    #[derive(Clone)]struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture{fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0.lock().unwrap().extend_from_slice(b);Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
    fn engine()->(Engine,Arc<Mutex<Vec<u8>>>){let b=Arc::new(Mutex::new(vec![]));(Engine::new(&Settings::default(),Box::new(Capture(b.clone()))),b)}
    #[test]fn parse_ansi(){let(mut e,_)=engine();e.advance(b"\x1b[31mred\x1b[0m\r\nnext");assert!(e.visible_text().starts_with("red\nnext"));}
    #[test]fn fragmented_chinese(){let(mut e,_)=engine();for b in "\x1b[32m中文\x1b[0m".as_bytes(){e.advance([*b]);}assert!(e.visible_text().starts_with("中文"));assert_eq!(e.snapshot().lines[0][0].width,2);}
    #[test]fn alternate_screen(){let(mut e,_)=engine();e.advance(b"main\x1b[?1049hother");assert!(e.snapshot().alternate);e.advance(b"\x1b[?1049l");assert!(!e.snapshot().alternate);assert!(e.visible_text().starts_with("main"));}
    // WezTerm forwards writes through its own worker thread. Await delivery,
    // retaining a bounded deadline and the exact protocol assertion.
    fn await_bytes(b: &Arc<Mutex<Vec<u8>>>, expected: &[u8]) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let actual = b.lock().unwrap().clone();
            if actual.len() >= expected.len() || std::time::Instant::now() >= deadline {
                assert_eq!(actual, expected); return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]fn device_reply(){let(mut e,b)=engine();e.advance(b"\x1b[6n");await_bytes(&b,b"\x1b[1;1R");}
    #[test]fn ctrl_t_is_sent_on_press_without_waiting_for_release(){
        let(mut e,b)=engine();
        let key=crate::terminal::input::code(&iced::keyboard::Key::Character("t".into())).unwrap();
        e.terminal.key_down(key,KeyModifiers::CTRL).unwrap();await_bytes(&b,b"\x14");
    }
    #[test]fn bracketed_paste(){let(mut e,b)=engine();e.advance(b"\x1b[?2004h");e.paste("abc").unwrap();await_bytes(&b,b"\x1b[200~abc\x1b[201~");}
    #[test]fn unicode_selection(){let(mut e,_)=engine();e.advance("abc中文");e.select(3,0,SelectionMode::Character);e.extend(6,0);assert_eq!(e.selection_text(),"中文");}
    #[test]fn selection_only_invalidates_changed_rows(){
        let(mut e,_)=engine();e.resize(20,4,200,80);e.advance("中文abc\r\nsecond\r\nthird");
        let before=e.snapshot();e.select(1,0,SelectionMode::Character);let selected=e.snapshot();
        assert!(selected.lines[0][0].selected);
        assert!(!Arc::ptr_eq(&before.lines[0],&selected.lines[0]));
        assert!(Arc::ptr_eq(&before.lines[1],&selected.lines[1]));
        e.extend(2,1);let extended=e.snapshot();
        assert!(Arc::ptr_eq(&selected.lines[2],&extended.lines[2]));check_cached(&e);
        e.bottom();check_cached(&e);
    }
    #[test]fn bounded_history(){let(mut e,_)=engine();e.resize(40,12,400,240);for _ in 0..20_000{e.advance(b"line\r\n");}assert!(e.snapshot().history<=10_000);}
    #[test]fn mouse_reporting(){let(mut e,_)=engine();e.advance(b"\x1b[?1000h");assert!(e.snapshot().mouse_grabbed);}
    #[test]fn snapshot_reuses_only_unchanged_rows(){
        let(mut e,_)=engine();e.resize(40,4,400,80);e.advance("first\r\nsecond");
        let before=e.snapshot();e.advance("\x1b[1;2H");let cursor=e.snapshot();
        assert!(Arc::ptr_eq(&before.lines[0],&cursor.lines[0]));assert_eq!(cursor.cursor_column,1);
        e.advance("X");let changed=e.snapshot();
        assert!(!Arc::ptr_eq(&cursor.lines[0],&changed.lines[0]));
        assert!(Arc::ptr_eq(&cursor.lines[1],&changed.lines[1]));
        assert_eq!(before.lines[0][1].text,"i"); // Published snapshots remain immutable.
    }
    fn check_cached(e:&Engine){
        let cached=e.snapshot();e.cache.borrow_mut().rows.clear();let fresh=e.snapshot();
        assert_eq!(cached.lines,fresh.lines);
    }
    #[test]fn snapshot_cache_tracks_terminal_and_view_changes(){
        let(mut e,_)=engine();e.resize(30,4,300,80);
        for bytes in ["\x1b[31m中文 red\r\nsecond", "\x1b[2;1H\x1b[Knew", "\x1b[?5h", "\x1b[?5l", "\x1b]4;1;#123456\x07", "\x1b[?1049hother", "\x1b[?1049l", "\r\na\r\nb\r\nc\r\nd"] {
            e.advance(bytes);check_cached(&e);
        }
        e.scroll(2);check_cached(&e);e.select(0,0,SelectionMode::Character);check_cached(&e);
        e.extend(3,2);check_cached(&e);e.bottom();check_cached(&e);
        e.resize(15,6,150,120);check_cached(&e);e.advance("\x1bcreset");check_cached(&e);
    }
}
