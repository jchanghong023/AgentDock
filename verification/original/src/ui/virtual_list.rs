//! A fixed-height virtual list. A directory can contain millions of entries,
//! but neither this widget nor the directory model instantiates all of them.
use super::paint;
use crate::files::visible_range;
use iced::advanced::{layout, renderer, widget::{self, Tree}, Clipboard, Layout, Shell, Widget};
use iced::{Color, Element, Event, Font, Length, Point, Rectangle, Renderer, Size, Theme, keyboard, mouse};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Row<K> {
    pub key: K,
    pub label: String,
    pub depth: usize,
    pub folder: bool,
    pub expanded: bool,
    pub selected: bool,
    pub star: Option<bool>,
    pub add: bool,
    pub muted: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum Click { Select, Double, Star, New }

pub struct VirtualList<'a, K, M> {
    rows: &'a [Row<K>],
    on: Box<dyn Fn(K, Click) -> M + 'a>,
}
impl<'a, K, M> VirtualList<'a, K, M> {
    pub fn new(rows: &'a [Row<K>], on: impl Fn(K, Click) -> M + 'a) -> Self {
        Self { rows, on: Box::new(on) }
    }
}
#[derive(Default)]
struct State {
    offset: f32,
    focused: bool,
    hover: Option<usize>,
    keyboard: usize,
    click: Option<(usize, Instant)>,
    scrollbar_drag: bool,
}
const HEIGHT: f32 = 28.0;
fn max_offset(count: usize, viewport: f32) -> f32 { (count as f32 * HEIGHT - viewport).max(0.0) }
impl<K: Clone, M> Widget<M, Theme, Renderer> for VirtualList<'_, K, M> {
    fn size(&self) -> Size<Length> { Size::new(Length::Fill, Length::Fill) }
    fn tag(&self) -> widget::tree::Tag { widget::tree::Tag::of::<State>() }
    fn state(&self) -> widget::tree::State { widget::tree::State::new(State::default()) }
    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let state = tree.state.downcast_mut::<State>();
        state.offset = state.offset.clamp(0.0, max_offset(self.rows.len(), size.height));
        layout::Node::new(size)
    }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor,
        _renderer: &Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, _viewport: &Rectangle) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let index = |point: Point, offset: f32| {
            let row = ((point.y - bounds.y + offset) / HEIGHT).floor().max(0.0) as usize;
            (row < self.rows.len()).then_some(row)
        };
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let amount = match delta { mouse::ScrollDelta::Lines { y, .. } => *y * HEIGHT * 3.0, mouse::ScrollDelta::Pixels { y, .. } => *y };
                state.offset = (state.offset - amount).clamp(0.0, max_offset(self.rows.len(), bounds.height));
                state.click = None; shell.capture_event(); shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.focused = cursor.is_over(bounds);
                if let Some(point) = cursor.position_over(bounds) {
                    if point.x >= bounds.x + bounds.width - 8.0 && max_offset(self.rows.len(), bounds.height) > 0.0 {
                        state.scrollbar_drag = true;
                        state.offset = ((point.y - bounds.y) / bounds.height * max_offset(self.rows.len(), bounds.height)).clamp(0.0, max_offset(self.rows.len(), bounds.height));
                    } else if let Some(i) = index(point, state.offset) {
                        let row = &self.rows[i];
                        state.keyboard = i;
                        let now = Instant::now();
                        let click = if row.star.is_some() && point.x > bounds.x + bounds.width - 35.0 { Click::Star }
                            else if row.add && point.x > bounds.x + bounds.width - 60.0 { Click::New }
                            else if state.click.is_some_and(|(last, t)| last == i && now.duration_since(t) < Duration::from_millis(450)) { Click::Double }
                            else { Click::Select };
                        state.click = if matches!(click, Click::Select) { Some((i, now)) } else { None };
                        shell.publish((self.on)(row.key.clone(), click));
                    }
                    shell.capture_event(); shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => state.scrollbar_drag = false,
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.scrollbar_drag {
                    state.offset = ((position.y - bounds.y) / bounds.height * max_offset(self.rows.len(), bounds.height)).clamp(0.0, max_offset(self.rows.len(), bounds.height));
                    shell.capture_event(); shell.request_redraw();
                }
                let next = cursor.position_over(bounds).and_then(|p| index(p, state.offset));
                if state.hover != next { state.hover = next; shell.request_redraw(); }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.focused && !self.rows.is_empty() => {
                use keyboard::{Key, key::Named};
                match key {
                    Key::Named(Named::ArrowDown) => state.keyboard = (state.keyboard + 1).min(self.rows.len() - 1),
                    Key::Named(Named::ArrowUp) => state.keyboard = state.keyboard.saturating_sub(1),
                    Key::Named(Named::Home) => state.keyboard = 0,
                    Key::Named(Named::End) => state.keyboard = self.rows.len() - 1,
                    Key::Named(Named::Enter) => {
                        state.keyboard = state.keyboard.min(self.rows.len() - 1);
                        // Enter selects a file; opening remains an explicit double click.
                        shell.publish((self.on)(self.rows[state.keyboard].key.clone(), Click::Select));
                    }
                    _ => return,
                }
                let top = state.keyboard as f32 * HEIGHT;
                if top < state.offset { state.offset = top; }
                if top + HEIGHT > state.offset + bounds.height { state.offset = top + HEIGHT - bounds.height; }
                shell.capture_event(); shell.request_redraw();
            }
            _ => {},
        }
    }
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, _theme: &Theme, _style: &renderer::Style,
        layout: Layout<'_>, _cursor: mouse::Cursor, viewport: &Rectangle) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else { return; };
        paint::rect(renderer, bounds, Color::from_rgb8(246, 248, 251));
        for i in visible_range(self.rows.len(), state.offset, bounds.height, HEIGHT) {
            let row = &self.rows[i];
            let y = bounds.y + i as f32 * HEIGHT - state.offset;
            let rect = Rectangle { x: bounds.x + 5.0, y, width: (bounds.width - 13.0).max(0.0), height: HEIGHT };
            let Some(row_clip) = rect.intersection(&clip) else { continue; };
            if row.selected { paint::rect(renderer, row_clip, Color::from_rgb8(212, 232, 251)); }
            else if state.hover == Some(i) || (state.focused && state.keyboard == i) { paint::rect(renderer, row_clip, Color::from_rgb8(231, 237, 245)); }
            let x = bounds.x + 10.0 + row.depth.min(40) as f32 * 15.0;
            let color = if row.muted { Color::from_rgb8(128, 137, 151) } else { Color::from_rgb8(36, 48, 67) };
            let icon = if row.folder { if row.expanded { "▾" } else { "▸" } } else { "·" };
            paint::label(renderer, icon, Point::new(x, y), Size::new(16.0, HEIGHT), Font::DEFAULT, 14.0, color, row_clip);
            let right = if row.star.is_some() { 62.0 } else { 10.0 };
            let text_clip = Rectangle { x: x + 16.0, y, width: (bounds.x + bounds.width - right - x - 16.0).max(0.0), height: HEIGHT };
            if let Some(text_clip) = text_clip.intersection(&row_clip) {
                paint::label(renderer, &row.label, Point::new(x + 16.0, y), Size::new(10000.0, HEIGHT), Font::DEFAULT, 13.0, color, text_clip);
            }
            if let Some(starred) = row.star {
                paint::label(renderer, if starred { "★" } else { "☆" }, Point::new(bounds.x + bounds.width - 31.0, y), Size::new(22.0, HEIGHT), Font::DEFAULT, 17.0,
                    if starred { Color::from_rgb8(210, 150, 9) } else { Color::from_rgb8(145, 160, 181) }, row_clip);
            }
            if row.add && (state.hover == Some(i) || row.selected) {
                paint::label(renderer, "+", Point::new(bounds.x + bounds.width - 56.0, y), Size::new(20.0, HEIGHT), Font::DEFAULT, 17.0, color, row_clip);
            }
        }
        let maximum = max_offset(self.rows.len(), bounds.height);
        if maximum > 0.0 {
            let height = (bounds.height * bounds.height / (self.rows.len() as f32 * HEIGHT)).max(20.0).min(bounds.height);
            paint::rect(renderer, Rectangle { x: bounds.x + bounds.width - 6.0, y: bounds.y + (bounds.height - height) * state.offset / maximum, width: 3.0, height }, Color::from_rgb8(183, 193, 207));
        }
    }
    fn mouse_interaction(&self, _tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &Renderer) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) { mouse::Interaction::Pointer } else { mouse::Interaction::None }
    }
}
impl<'a, K: Clone + 'a, M: 'a> From<VirtualList<'a, K, M>> for Element<'a, M> {
    fn from(value: VirtualList<'a, K, M>) -> Self { Element::new(value) }
}
