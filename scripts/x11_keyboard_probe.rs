use winit::{application::ApplicationHandler, event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop}, window::{Window, WindowId},
    raw_window_handle::{HasWindowHandle, RawWindowHandle}};

#[derive(Default)]
struct Probe { window: Option<Window> }
impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop.create_window(Window::default_attributes()
            .with_title("AgentDock X11 keyboard probe")).expect("create window");
        window.focus_window();
        let handle = window.window_handle().unwrap();
        match handle.as_raw() {
            RawWindowHandle::Xlib(h) => println!("WINDOW {}", h.window),
            RawWindowHandle::Xcb(h) => println!("WINDOW {}", h.window),
            _ => panic!("expected X11"),
        }
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => println!("KEY {:?} {:?} {:?} repeat={}", event.state, event.logical_key, event.text, event.repeat),
            WindowEvent::ModifiersChanged(mods) => println!("MOD {:?}", mods.state()),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {},
        }
    }
}
fn main() {
    let event_loop = match EventLoop::new() {
        Ok(value) => value,
        Err(error) => { eprintln!("INIT_ERROR {error}"); std::process::exit(2); }
    };
    event_loop.run_app(&mut Probe::default()).unwrap();
}
