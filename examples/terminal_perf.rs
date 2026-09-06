//! Run with `cargo run --offline --example terminal_perf` (same profile before/after).
use agentdock::{model::Settings, terminal::Engine};
use std::{hint::black_box, time::Instant};

fn main() {
    for mode in ["cursor", "single_line", "full_screen"] {
        let mut engine = Engine::new(&Settings::default(), Box::new(std::io::sink()));
        engine.resize(140, 40, 1400, 800);
        let screen = (0..40).map(|row| format!("\x1b[{};1H\x1b[32m{}", row + 1, "terminal中文0123456789 ".repeat(6))).collect::<String>();
        engine.advance(&screen);
        black_box(engine.snapshot());
        let mut parse = std::time::Duration::ZERO;
        let mut snapshot = std::time::Duration::ZERO;
        for frame in 0..1000 {
            let update = match mode {
                "cursor" => format!("\x1b[1;{}H", frame % 100 + 1),
                "single_line" => format!("\x1b[20;1Hframe {frame:04} 中文"),
                _ => screen.clone(),
            };
            let start = Instant::now();
            engine.advance(update);
            parse += start.elapsed();
            let start = Instant::now();
            black_box(engine.snapshot());
            snapshot += start.elapsed();
        }
        println!("{mode}: parse_ms={:.2} snapshot_ms={:.2} (1000 updates)", parse.as_secs_f64()*1000.0, snapshot.as_secs_f64()*1000.0);
    }
}
