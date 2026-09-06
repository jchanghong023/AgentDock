use iced::keyboard::{Key,Modifiers,key::Named};
use wezterm_term::{KeyCode,KeyModifiers,MouseEvent};
use super::engine::SelectionMode;
#[derive(Debug,Clone)]pub enum Action{
    Key{key:KeyCode,modifiers:KeyModifiers,pressed:bool},Text(String),Copy,RequestPaste,
    Scroll(i32),Focus(bool),Zoom(i8),Mouse(MouseEvent),
    Resize{cols:usize,rows:usize,width:usize,height:usize},
    Select{col:usize,row:usize,mode:SelectionMode},Extend{col:usize,row:usize},
}
pub fn modifiers(m:Modifiers)->KeyModifiers{
    let mut result=KeyModifiers::NONE;if m.shift(){result|=KeyModifiers::SHIFT;}if m.control(){result|=KeyModifiers::CTRL;}if m.alt(){result|=KeyModifiers::ALT;}if m.logo(){result|=KeyModifiers::SUPER;}result
}
pub fn code(key:&Key)->Option<KeyCode>{Some(match key.as_ref(){
    Key::Character(s)=>KeyCode::Char(s.chars().next()?),
    Key::Named(n)=>match n{
        Named::Enter=>KeyCode::Enter,Named::Escape=>KeyCode::Escape,Named::Backspace=>KeyCode::Backspace,Named::Tab=>KeyCode::Tab,Named::Space=>KeyCode::Char(' '),
        Named::ArrowUp=>KeyCode::UpArrow,Named::ArrowDown=>KeyCode::DownArrow,Named::ArrowLeft=>KeyCode::LeftArrow,Named::ArrowRight=>KeyCode::RightArrow,
        Named::Home=>KeyCode::Home,Named::End=>KeyCode::End,Named::PageUp=>KeyCode::PageUp,Named::PageDown=>KeyCode::PageDown,Named::Insert=>KeyCode::Insert,Named::Delete=>KeyCode::Delete,
        Named::F1=>KeyCode::Function(1),Named::F2=>KeyCode::Function(2),Named::F3=>KeyCode::Function(3),Named::F4=>KeyCode::Function(4),Named::F5=>KeyCode::Function(5),Named::F6=>KeyCode::Function(6),
        Named::F7=>KeyCode::Function(7),Named::F8=>KeyCode::Function(8),Named::F9=>KeyCode::Function(9),Named::F10=>KeyCode::Function(10),Named::F11=>KeyCode::Function(11),Named::F12=>KeyCode::Function(12),
        _=>return None,
    },_=>return None,
})}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn nav_not_text(){assert_eq!(code(&Key::Named(Named::ArrowUp)),Some(KeyCode::UpArrow));}
    #[test]fn modifiers_retained(){assert!(modifiers(Modifiers::CTRL).contains(KeyModifiers::CTRL));assert!(modifiers(Modifiers::SHIFT).contains(KeyModifiers::SHIFT));}
}
