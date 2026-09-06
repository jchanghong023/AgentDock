use iced::advanced::{renderer,text};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::text::Renderer as _;
use iced::{Color,Font,Pixels,Point,Rectangle,Renderer,Size};
pub fn rect(r:&mut Renderer,b:Rectangle,c:Color){if b.width>0.0&&b.height>0.0{r.fill_quad(renderer::Quad{bounds:b,..Default::default()},c);}}
pub fn label(r:&mut Renderer,s:&str,p:Point,b:Size,font:Font,size:f32,c:Color,clip:Rectangle){
    r.with_layer(clip, |r| r.fill_text(text::Text{content:s.into(),bounds:b,size:Pixels(size),font,line_height:text::LineHeight::Absolute(Pixels(b.height)),align_x:text::Alignment::Left,align_y:iced::alignment::Vertical::Top,shaping:text::Shaping::Advanced,wrapping:text::Wrapping::None},p,c,clip));
}
pub fn rgba(c:[f32;4])->Color{Color::from_rgba(c[0],c[1],c[2],c[3])}
// The terminal owns one clip layer for the complete grid, not one per glyph.
pub fn terminal_label(r:&mut Renderer,s:&str,p:Point,b:Size,font:Font,size:f32,c:Color,clip:Rectangle){
    r.fill_text(text::Text{content:s.into(),bounds:b,size:Pixels(size),font,line_height:text::LineHeight::Absolute(Pixels(b.height)),align_x:text::Alignment::Left,align_y:iced::alignment::Vertical::Top,shaping:if s.is_ascii(){text::Shaping::Basic}else{text::Shaping::Advanced},wrapping:text::Wrapping::None},p,c,clip);
}
pub fn file_icon(r: &mut Renderer, x: f32, y: f32, folder: bool, pinned: bool) {
    let blue = if pinned { Color::from_rgb8(255, 181, 0) } else { Color::from_rgb8(0, 132, 255) };
    if folder {
        rect(r, Rectangle { x, y: y + 1.0, width: 7.0, height: 4.0 }, blue);
        r.fill_quad(renderer::Quad { bounds: Rectangle { x, y: y + 3.0, width: 14.0, height: 10.0 }, border: iced::Border { color: blue, width: 1.0, radius: 1.5.into() }, ..Default::default() }, if pinned { Color::from_rgb8(255, 209, 74) } else { Color::from_rgb8(74, 176, 255) });
    } else {
        r.fill_quad(renderer::Quad { bounds: Rectangle { x: x + 2.0, y, width: 10.0, height: 14.0 }, border: iced::Border { color: blue, width: 1.0, radius: 1.0.into() }, ..Default::default() }, Color::WHITE);
        for offset in [5.0, 8.0, 11.0] { rect(r, Rectangle { x: x + 4.0, y: y + offset, width: 6.0, height: 1.0 }, blue); }
    }
}
