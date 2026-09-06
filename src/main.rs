#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]
use agentdock::{cli::{HELP, Options}, terminal, ui::App};

fn main() {
    if let Err(error) = run() {
        eprintln!("AgentDock: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let options = Options::parse(std::env::args_os().skip(1))?;
    if options.help { println!("{HELP}"); return Ok(()); }
    if options.version { println!("AgentDock {}", env!("CARGO_PKG_VERSION")); return Ok(()); }
    if options.self_test {
        terminal::process::self_test()?;
        println!("AGENTDOCK_PTY_SELF_TEST_OK");
        return Ok(());
    }
    let app = App::create(options)?;
    // iced accepts an Fn boot function. Ownership is moved exactly once into it.
    let state = std::cell::RefCell::new(Some(app));
    iced::application(move || state.borrow_mut().take().expect("application boots once"), App::update, App::view)
        .title(App::title)
        .theme(agentdock::ui::style::theme())
        .default_font(agentdock::ui::style::font())
        .subscription(App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1240.0, 820.0),
            min_size: Some(iced::Size::new(760.0, 480.0)),
            exit_on_close_request: false,
            icon: agentdock::ui::style::icon(),
            ..Default::default()
        })
        .centered()
        .run().map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(())
}
