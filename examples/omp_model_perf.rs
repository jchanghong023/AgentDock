//! Local /model latency probe. Never submits an agent prompt or changes a model.
use agentdock::{model::{Launch,Settings},terminal::Session};
use std::time::{Duration,Instant};
use wezterm_term::{KeyCode,KeyModifiers};
fn main()->anyhow::Result<()>{
    let root=std::env::current_dir()?;
    anyhow::ensure!(std::env::var_os("PI_CODING_AGENT_SESSION_DIR").map(std::path::PathBuf::from).is_some_and(|p|p.starts_with(root.join(".tmp"))),"Set PI_CODING_AGENT_SESSION_DIR to a directory under .tmp before running this probe");
    let mut session=Session::spawn(&root,&Launch::omp(Some("default"),false),&Settings::default())?;
    session.resize(140,40,1400,800)?;
    let started=Instant::now();
    while started.elapsed()<Duration::from_secs(8){session.poll(65536);std::thread::sleep(Duration::from_millis(1));}
    std::fs::write(root.join(".tmp/omp-before-model.txt"),session.engine.visible_text())?;
    for trial in 0..3 {
        let start=Instant::now();session.engine.text("/model")?;
        session.engine.terminal.key_down(KeyCode::Enter,KeyModifiers::NONE)?;
        let mut first=None;let mut last=Duration::ZERO;let mut max_poll=Duration::ZERO;let mut updates=0;
        while start.elapsed()<Duration::from_secs(5){
            let tick=Instant::now();let changed=session.poll(65536);max_poll=max_poll.max(tick.elapsed());
            if changed{
                first.get_or_insert(start.elapsed());last=start.elapsed();updates+=1;
                let snapshot_start=Instant::now();let visible=session.engine.visible_text();
                println!("event trial={trial} elapsed_ms={:.2} snapshot_ms={:.2} model_menu={}",last.as_secs_f64()*1000.,snapshot_start.elapsed().as_secs_f64()*1000.,visible.contains("Enter assign roles"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::fs::write(root.join(format!(".tmp/omp-model-{trial}.txt")),session.engine.visible_text())?;
        println!("trial={trial} first_output_ms={:.2} last_output_ms={:.2} max_poll_ms={:.2} updates={updates} error={:?}",first.unwrap_or_default().as_secs_f64()*1000.,last.as_secs_f64()*1000.,max_poll.as_secs_f64()*1000.,session.error);
        session.engine.terminal.key_down(KeyCode::Escape,KeyModifiers::NONE)?;
        let settle=Instant::now();while settle.elapsed()<Duration::from_millis(500){session.poll(65536);std::thread::sleep(Duration::from_millis(1));}
    }
    Ok(())
}
