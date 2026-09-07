//! Run with `cargo run --offline --example terminal_perf` (same profile before/after).
use agentdock::{model::Settings, terminal::Engine};
use std::{hint::black_box, time::Instant};

fn main() {
    for mode in ["cursor", "single_line", "full_screen", "selection"] {
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
                "selection" => String::new(),
                _ => screen.clone(),
            };
            let start = Instant::now();
            if mode == "selection" {
                engine.select(frame % 100, 20, agentdock::terminal::engine::SelectionMode::Character);
            } else { engine.advance(update); }
            parse += start.elapsed();
            let start = Instant::now();
            black_box(engine.snapshot());
            snapshot += start.elapsed();
        }
        println!("{mode}: parse_ms={:.2} snapshot_ms={:.2} (1000 updates)", parse.as_secs_f64()*1000.0, snapshot.as_secs_f64()*1000.0);
    }
    for count in [1000, 10000] {
        let records: Vec<_> = (0..count).map(|i| agentdock::omp::Record {
            id: uuid::Uuid::new_v4(), file: format!("session-{i}.jsonl").into(),
            cwd: "benchmark-project".into(), title: format!("历史会话 {i}"), modified: i as u64,
        }).collect();
        let mut store = agentdock::model::Store::default();
        let start = Instant::now();
        assert!(store.import_omp(&records, None));
        let initial = start.elapsed();
        let start = Instant::now();
        assert!(!store.import_omp(&records, None));
        println!("import_{count}: initial_ms={:.2} unchanged_ms={:.2}", initial.as_secs_f64()*1000.0, start.elapsed().as_secs_f64()*1000.0);
    }
}
