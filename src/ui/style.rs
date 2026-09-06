use iced::{Color, Font, Theme, Border, widget::{button, container}};

pub const INK: Color = Color::from_rgb8(13, 32, 78);
pub const BLUE: Color = Color::from_rgb8(0, 126, 255);
pub const MUTED: Color = Color::from_rgb8(105, 128, 165);
pub const LINE: Color = Color::from_rgb8(218, 231, 244);
pub const SURFACE: Color = Color::from_rgb8(247, 251, 255);

pub fn font() -> Font {
    if cfg!(windows) { Font::with_name("Microsoft YaHei UI") } else { Font::DEFAULT }
}
pub fn bold() -> Font { Font { weight: iced::font::Weight::Bold, ..font() } }
pub fn icon() -> Option<iced::window::Icon> {
    let mut pixels = vec![0u8; 32 * 32 * 4];
    for y in 0..32usize {
        for x in 0..32usize {
            let dx = x as f32 - 15.5; let dy = y as f32 - 15.5;
            if dx * dx + dy * dy < 225.0 {
                let mark = (8..=23).contains(&y) && ((dx.abs() + (y as f32 - 13.0).abs() * 0.65 - 7.0).abs() < 1.5);
                pixels[(y * 32 + x) * 4..(y * 32 + x + 1) * 4].copy_from_slice(if mark { &[255, 255, 255, 255] } else { &[0, 126, 255, 255] });
            }
        }
    }
    iced::window::icon::from_rgba(pixels, 32, 32).ok()
}
pub fn theme() -> Theme {
    Theme::custom("AgentDock", iced::theme::Palette {
        background: Color::WHITE, text: INK, primary: BLUE,
        success: Color::from_rgb8(33, 158, 103), danger: Color::from_rgb8(207, 59, 69),
        warning: Color::from_rgb8(255, 185, 0),
    })
}
pub fn panel(_: &Theme) -> container::Style {
    container::Style { background: Some(SURFACE.into()), text_color: Some(INK), ..Default::default() }
}
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(if active { Color::WHITE } else if matches!(status, button::Status::Hovered) { Color::from_rgb8(226, 240, 255) } else { SURFACE }.into()),
        text_color: INK,
        border: Border { color: LINE, width: 1.0, radius: 5.0.into() },
        ..Default::default()
    }
}
pub fn subtle(_: &Theme, status: button::Status) -> button::Style {
    button::Style { text_color: MUTED,
        background: matches!(status, button::Status::Hovered).then_some(Color::from_rgb8(225, 239, 253).into()),
        border: Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() }
}
pub fn tab_group(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(if active { Color::WHITE } else { SURFACE }.into()),
        text_color: Some(INK),
        border: Border { color: LINE, width: 1.0, radius: 5.0.into() },
        ..Default::default()
    }
}
pub fn tab_title(theme: &Theme, status: button::Status) -> button::Style {
    button::Style { text_color: INK, ..subtle(theme, status) }
}
