use super::{app::Message, style};
use iced::{widget::{button, column, container, markdown, row, text, Space}, Border, Element, Length};

pub struct Viewer;
impl<'a> markdown::Viewer<'a, Message> for Viewer {
    fn on_link_click(url: markdown::Uri) -> Message { Message::Link(url) }

    fn heading(&self, mut settings: markdown::Settings, level: &'a markdown::HeadingLevel, content: &'a markdown::Text, index: usize) -> Element<'a, Message> {
        settings.style.font = style::bold();
        let heading = markdown::heading(settings, level, content, index, Message::Link);
        if matches!(level, markdown::HeadingLevel::H2) {
            column![container(Space::new().height(1)).width(Length::Fill).style(|_| container::Style { background: Some(style::LINE.into()), ..Default::default() }), heading].spacing(6).into()
        } else { heading }
    }

    fn code_block(&self, settings: markdown::Settings, language: Option<&'a str>, code: &'a str, _lines: &'a [markdown::Text]) -> Element<'a, Message> {
        container(column![
            row![text(language.unwrap_or("text")).size(12).color(style::MUTED), Space::new().width(Length::Fill), button(text("复制").size(11)).style(style::subtle).padding([0, 5]).on_press(Message::CopyCode(code.to_owned()))],
            text(code.trim_end()).font(settings.style.code_block_font).size(settings.code_size),
        ].spacing(3)).padding([7, 12]).width(Length::Fill).style(|_| container::Style {
            background: Some(iced::Color::from_rgb8(239, 245, 250).into()), text_color: Some(style::INK),
            border: Border { color: style::LINE, width: 1.0, radius: 5.0.into() }, ..Default::default()
        }).into()
    }
}

pub fn view(items: &[markdown::Item], font: iced::Font) -> Element<'_, Message> {
    let mut settings = markdown::Settings::with_text_size(15, style::theme());
    settings.h1_size = 36.into(); settings.h2_size = 22.into(); settings.h3_size = 17.into();
    settings.code_size = 13.into(); settings.spacing = 10.into();
    settings.style.font = style::font();
    settings.style.code_block_font = font;
    settings.style.inline_code_font = font;
    settings.style.inline_code_color = style::INK;
    settings.style.inline_code_highlight.background = iced::Color::from_rgb8(232, 241, 250).into();
    markdown::view_with(items, settings, &Viewer)
}
