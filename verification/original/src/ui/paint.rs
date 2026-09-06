use iced::advanced::{renderer,text};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::text::Renderer as _;
use iced::{Color,Font,Pixels,Point,Rectangle,Renderer,Size};
pub fn rect(r:&mut Renderer,b:Rectangle,c:Color){if b.width>0.0&&b.height>0.0{r.fill_quad(renderer::Quad{bounds:b,..Default::default()},c);}}
pub fn label(r:&mut Renderer,s:&str,p:Point,b:Size,font:Font,size:f32,c:Color,clip:Rectangle){
    r.fill_text(text::Text{content:s.into(),bounds:b,size:Pixels(size),font,line_height:text::LineHeight::Absolute(Pixels(b.height)),align_x:text::Alignment::Left,align_y:iced::alignment::Vertical::Top,shaping:text::Shaping::Advanced,wrapping:text::Wrapping::None},p,c,clip);
}
pub fn rgba(c:[f32;4])->Color{Color::from_rgba(c[0],c[1],c[2],c[3])}
