//! Single writer and atomic replacement; corrupt state is never silently reset.
use crate::model::Store;
use anyhow::{Context,Result};
use crossbeam_channel::{Sender,Receiver,bounded};
use fs2::FileExt;
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf}};
const MAX: u64=8*1024*1024;
pub fn load(directory:&Path)->Result<Store> {
    let file=match File::open(directory.join("state.json")) { Ok(f)=>f,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Store::default()),Err(e)=>return Err(e.into()) };
    let mut data=Vec::new();file.take(MAX+1).read_to_end(&mut data)?;
    anyhow::ensure!(data.len() as u64<=MAX,"状态超过 8 MiB");
    let mut state:Store=serde_json::from_slice(&data).context("状态文件损坏；请备份后处理，不会覆盖原文件")?;
    state.validate()?;Ok(state)
}
pub fn save_atomic(directory:&Path,state:&Store)->Result<()> {
    let bytes=serde_json::to_vec_pretty(state)?;
    anyhow::ensure!(bytes.len() as u64<=MAX,"状态超过 8 MiB，未覆盖旧文件");
    let path=directory.join(format!(".state-{}.tmp",uuid::Uuid::new_v4()));
    let result=(||->Result<()> {
        let mut options=OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt;options.mode(0o600); }
        let mut f=options.open(&path)?;f.write_all(&bytes)?;f.sync_all()?;drop(f);
        fs::rename(&path,directory.join("state.json"))?;
        #[cfg(unix)] File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() { let _=fs::remove_file(&path); }result
}
pub struct Persistence {
    _lock:File, tx:Option<Sender<Store>>,pub errors:Receiver<String>,worker:Option<std::thread::JoinHandle<()>>,
}
impl Persistence {
    pub fn open(directory:PathBuf)->Result<(Self,Store)> {
        fs::create_dir_all(&directory)?;
        let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(directory.join("instance.lock"))?;
        lock.try_lock_exclusive().context("同一状态目录已有实例；使用 --state-dir 创建独立实例")?;
        let state=load(&directory)?;
        // One pending snapshot; the app retries while dirty if the queue is busy.
        let (tx,rx)=bounded::<Store>(1);let (error_tx,errors)=bounded(8);
        let worker=std::thread::Builder::new().name("agentdock-state".into()).spawn(move || {
            while let Ok(state)=rx.recv() { if let Err(e)=save_atomic(&directory,&state) { let _=error_tx.try_send(format!("保存失败：{e:#}")); } }
        })?;
        Ok((Self { _lock:lock,tx:Some(tx),errors,worker:Some(worker) },state))
    }
    /// Called from the app only when dirty. A busy writer requests a later retry.
    pub fn save(&self,state:&Store)->bool { self.tx.as_ref().is_some_and(|tx| tx.try_send(state.clone()).is_ok()) }
    pub fn flush_final(&self,state:&Store)->Result<()> {
        if let Some(tx)=&self.tx { tx.send(state.clone()).context("保存线程已退出")?; }Ok(())
    }
}
impl Drop for Persistence {
    fn drop(&mut self) { self.tx.take();if let Some(worker)=self.worker.take(){let _=worker.join();} }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn atomic_roundtrip(){let d=tempfile::tempdir().unwrap();let mut s=Store::default();let id=s.ensure_project(d.path().join("中文"));s.toggle_pin(id);save_atomic(d.path(),&s).unwrap();assert!(load(d.path()).unwrap().project(id).unwrap().pinned);assert_eq!(fs::read_dir(d.path()).unwrap().count(),1);}
    #[test]fn corruption_preserved(){let d=tempfile::tempdir().unwrap();fs::write(d.path().join("state.json"),"broken").unwrap();assert!(load(d.path()).is_err());assert_eq!(fs::read_to_string(d.path().join("state.json")).unwrap(),"broken");}
    #[test]fn exclusive_writer(){let d=tempfile::tempdir().unwrap();let (p,_)=Persistence::open(d.path().into()).unwrap();assert!(Persistence::open(d.path().into()).is_err());drop(p);assert!(Persistence::open(d.path().into()).is_ok());}
    #[test]fn final_flush(){let d=tempfile::tempdir().unwrap();let(p,mut s)=Persistence::open(d.path().into()).unwrap();s.ensure_project("final".into());p.flush_final(&s).unwrap();drop(p);assert_eq!(load(d.path()).unwrap().projects.len(),1);}
}
