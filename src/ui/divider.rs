//! Draggable split handles without adding toolbars or menus.
use super::paint;
use iced::advanced::{layout, renderer, widget::{self, Tree}, Clipboard, Layout, Shell, Widget};
use iced::{Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse};
#[derive(Clone, Copy)] pub enum Axis { Vertical, Horizontal }
pub struct Divider<'a, M> { axis: Axis, on: Box<dyn Fn(f32) -> M + 'a> }
impl<'a, M> Divider<'a, M> { pub fn new(axis: Axis, on: impl Fn(f32) -> M + 'a) -> Self { Self { axis, on: Box::new(on) } } }
#[derive(Default)] struct State { anchor: Option<Point> }
impl<M> Widget<M, Theme, Renderer> for Divider<'_, M> {
    fn size(&self) -> Size<Length> { match self.axis { Axis::Vertical => Size::new(Length::Fixed(5.0), Length::Fill), Axis::Horizontal => Size::new(Length::Fill, Length::Fixed(5.0)) } }
    fn tag(&self) -> widget::tree::Tag { widget::tree::Tag::of::<State>() }
    fn state(&self) -> widget::tree::State { widget::tree::State::new(State::default()) }
    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node { let size = self.size(); layout::Node::new(limits.resolve(size.width, size.height, Size::ZERO)) }
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, _viewport: &Rectangle) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if cursor.is_over(layout.bounds()) => { state.anchor = cursor.position(); shell.capture_event(); }
            Event::Mouse(mouse::Event::CursorMoved { position }) => if let Some(previous) = state.anchor {
                let delta = match self.axis { Axis::Vertical => position.x - previous.x, Axis::Horizontal => position.y - previous.y };
                state.anchor = Some(*position); shell.publish((self.on)(delta)); shell.capture_event();
            },
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => state.anchor = None,
            _ => {},
        }
    }
    fn draw(&self, _tree: &Tree, renderer: &mut Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, _cursor: mouse::Cursor, _viewport: &Rectangle) { paint::rect(renderer, layout.bounds(), Color::from_rgb8(220, 228, 238)); }
    fn mouse_interaction(&self, _tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &Renderer) -> mouse::Interaction { if cursor.is_over(layout.bounds()) { mouse::Interaction::Grab } else { mouse::Interaction::None } }
}
impl<'a, M: 'a> From<Divider<'a, M>> for Element<'a, M> { fn from(value: Divider<'a, M>) -> Self { Element::new(value) } }
