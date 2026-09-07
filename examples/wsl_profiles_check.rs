//! Windows integration probe: installed distributions, real ConPTY, input and cwd.
use agentdock::{model::Settings,terminal::{profiles,Session}};
use std::time::{Duration,Instant};
fn main()->anyhow::Result<()> {
    let root=std::env::current_dir()?;
    let profiles=profiles::discover();
    if let Some(warning)=profiles.warning { eprintln!("{warning}"); }
    let mut checked=0;
    for profile in profiles.items.iter().filter(|p|p.distribution.is_some()) {
        let mut launch=profile.launch(&root);
        launch.args.extend(["--exec".into(),"/bin/sh".into(),"-c".into(),"printf 'WSL_READY\\n'; IFS= read -r token; printf 'WSL_ECHO:%s\\n' \"$token\"; uname -s; pwd; printf 'WSL_DONE\\n'".into()]);
        let mut session=Session::spawn(&root,&launch,&Settings::default())?;
        session.resize(180,30,0,0)?;
        let deadline=Instant::now()+Duration::from_secs(30);let mut sent=false;let mut result=String::new();
        while Instant::now()<deadline {
            session.poll(65536);result=session.engine.visible_text();
            if !sent&&result.contains("WSL_READY") { session.engine.text("中文-roundtrip-123")?;session.engine.terminal.key_down(wezterm_term::KeyCode::Enter,wezterm_term::KeyModifiers::NONE)?;sent=true; }
            if result.contains("WSL_DONE") { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        anyhow::ensure!(result.contains("WSL_ECHO:中文-roundtrip-123")&&result.contains("Linux")&&result.contains("WSL_DONE"),"{} failed: {result}",profile.title);
        let expected=format!("/mnt/{}/{}",root.to_string_lossy()[..1].to_lowercase(),root.to_string_lossy()[3..].replace('\\',"/"));
        anyhow::ensure!(result.contains(&expected),"{} cwd mismatch: {result}",profile.title);
        println!("{}: ConPTY Chinese roundtrip and project cwd OK",profile.title);checked+=1;
    }
    anyhow::ensure!(checked>0,"No WSL distributions available to test");
    Ok(())
}
