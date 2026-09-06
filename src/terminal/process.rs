use super::Engine;
use crate::model::{Launch,Settings};
use anyhow::{Context,Result};
use crossbeam_channel::{bounded,Receiver,Sender};
use portable_pty::{native_pty_system,Child,CommandBuilder,MasterPty,PtySize};
use std::{io::{Read,Write},path::Path,time::{Duration,Instant}};
use std::{pin::Pin,sync::{Arc,atomic::{AtomicBool,Ordering}},task::{Context as TaskContext,Poll}};
use iced::futures::{Stream,task::AtomicWaker};
#[derive(Debug)]enum Output{Bytes(Vec<u8>),Eof,Error(String)}
// One outstanding UI notification per session. Keep it outstanding until the
// UI drains output, so a busy PTY cannot flood the application message queue.
#[derive(Default)]struct OutputSignal{scheduled:AtomicBool,ready:AtomicBool,waker:AtomicWaker}
impl OutputSignal{
    fn notify(&self){
        if !self.scheduled.swap(true,Ordering::AcqRel){
            self.ready.store(true,Ordering::Release);self.waker.wake();
        }
    }
}
#[derive(Clone)]
pub(crate) struct OutputWatch { id: uuid::Uuid, signal:Arc<OutputSignal> }
impl std::hash::Hash for OutputWatch {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
impl Stream for OutputWatch{
    type Item=();
    fn poll_next(self:Pin<&mut Self>,cx:&mut TaskContext<'_>)->Poll<Option<()>>{
        self.signal.waker.register(cx.waker());
        if self.signal.ready.swap(false,Ordering::AcqRel){Poll::Ready(Some(()))}else{Poll::Pending}
    }
}
fn publish(tx:&Sender<Output>,signal:&OutputSignal,output:Output)->bool{
    if tx.send(output).is_err(){return false;}signal.notify();true
}
#[derive(Clone)]struct Input(Sender<Vec<u8>>);
impl Write for Input{
    fn write(&mut self,b:&[u8])->std::io::Result<usize>{
        self.0.try_send(b.to_vec()).map_err(|e|std::io::Error::new(std::io::ErrorKind::WouldBlock,e.to_string()))?;Ok(b.len())
    }
    fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
pub struct Session{
    pub engine:Engine,master:Box<dyn MasterPty+Send>,child:Option<Box<dyn Child+Send+Sync>>,
    output:Receiver<Output>,watch_id:uuid::Uuid,signal:Arc<OutputSignal>,pub exited:Option<String>,pub error:Option<String>,pub unread:bool,
}
impl Session{
    pub fn spawn(cwd:&Path,launch:&Launch,settings:&Settings)->Result<Self>{
        anyhow::ensure!(cwd.is_dir(),"工作目录不可访问：{}",cwd.display());
        let pair=native_pty_system().openpty(PtySize{rows:24,cols:80,pixel_width:0,pixel_height:0})?;
        let launch=launch.resolved();let mut cmd=CommandBuilder::new(&launch.program);cmd.args(&launch.args);cmd.cwd(cwd);
        cmd.env("TERM","xterm-256color");cmd.env("COLORTERM","truecolor");cmd.env("TERM_PROGRAM","AgentDock");cmd.env("TERM_PROGRAM_VERSION",env!("CARGO_PKG_VERSION"));
        let mut reader=pair.master.try_clone_reader()?;let mut writer=pair.master.take_writer()?;
        let mut child=pair.slave.spawn_command(cmd).with_context(||format!("启动 {}",launch.program))?;drop(pair.slave);
        let(tx,output)=bounded::<Output>(128);let errors_tx=tx.clone();let(input_tx,input_rx)=bounded::<Vec<u8>>(256);
        let signal=Arc::new(OutputSignal::default());let reader_signal=signal.clone();let writer_signal=signal.clone();
        let reader_thread=std::thread::Builder::new().name("agentdock-pty-read".into()).spawn(move||{
            let mut bytes=[0u8;8192];loop{match reader.read(&mut bytes){
                Ok(0)=>{publish(&tx,&reader_signal,Output::Eof);break;},
                Ok(n)=>{if !publish(&tx,&reader_signal,Output::Bytes(bytes[..n].to_vec())){break;}},
                Err(e)if e.kind()==std::io::ErrorKind::Interrupted=>continue,
                Err(e)if cfg!(target_os="linux")&&e.raw_os_error()==Some(5)=>{publish(&tx,&reader_signal,Output::Eof);break;},
                Err(e)=>{publish(&tx,&reader_signal,Output::Error(e.to_string()));break;},
            }}
        });
        if let Err(e)=reader_thread{let _=child.kill();let _=child.wait();return Err(e.into());}
        let writer_thread=std::thread::Builder::new().name("agentdock-pty-write".into()).spawn(move||{
            while let Ok(bytes)=input_rx.recv(){if let Err(e)=writer.write_all(&bytes).and_then(|_|writer.flush()){publish(&errors_tx,&writer_signal,Output::Error(e.to_string()));break;}}
        });
        if let Err(e)=writer_thread{let _=child.kill();let _=child.wait();return Err(e.into());}
        Ok(Self{engine:Engine::new(settings,Box::new(Input(input_tx))),master:pair.master,child:Some(child),output,watch_id:uuid::Uuid::new_v4(),signal,exited:None,error:None,unread:false})
    }
    pub fn running(&self)->bool{self.exited.is_none()}
    pub(crate) fn watch(&self) -> OutputWatch { OutputWatch { id:self.watch_id,signal:self.signal.clone() } }
    pub fn poll(&mut self,budget:usize)->bool{
        self.signal.scheduled.store(false,Ordering::Release);
        let mut used=0;let mut changed=false;let deadline=Instant::now()+Duration::from_millis(3);
        while used<budget&&Instant::now()<deadline{match self.output.try_recv(){
            Ok(Output::Bytes(b))=>{used+=b.len();self.engine.advance(b);changed=true;self.unread=true;},
            Ok(Output::Eof)=>break,
            Ok(Output::Error(e))=>{self.error=Some(e);changed=true;},Err(_)=>break,
        }}
        if self.exited.is_none(){if let Some(child)=&mut self.child{match child.try_wait(){
            Ok(Some(status))=>{self.exited=Some(status.to_string());changed=true;},Ok(None)=>{},
            Err(e)=>{self.error=Some(e.to_string());self.exited=Some("无法读取退出状态".into());changed=true;}
        }}}
        if !self.output.is_empty(){self.signal.notify();}
        changed
    }
    pub fn resize(&mut self,cols:usize,rows:usize,width:usize,height:usize)->Result<()>{
        self.master.resize(PtySize{cols:cols.clamp(2,1000)as u16,rows:rows.clamp(1,500)as u16,pixel_width:width.min(u16::MAX as usize)as u16,pixel_height:height.min(u16::MAX as usize)as u16})?;
        self.engine.resize(cols,rows,width,height);Ok(())
    }
}
impl Drop for Session{
    fn drop(&mut self){if let Some(mut child)=self.child.take(){if !matches!(child.try_wait(),Ok(Some(_))){let _=child.kill();let _=std::thread::Builder::new().name("agentdock-reap".into()).spawn(move||{let _=child.wait();});}}}
}
pub fn self_test()->Result<()>{
    #[cfg(unix)]let command=Launch{program:"/bin/sh".into(),args:vec!["-c".into(),"printf 'AGENTDOCK_READY\\n'; read line; printf 'AGENTDOCK_ECHO:%s\\n' \"$line\"".into()]};
    #[cfg(windows)]let command=Launch{program:"cmd.exe".into(),args:vec!["/Q".into(),"/V:ON".into(),"/C".into(),"echo AGENTDOCK_READY & set /p token= & echo AGENTDOCK_ECHO:!token!".into()]};
    let mut s=Session::spawn(&std::env::current_dir()?,&command,&Settings::default())?;s.resize(100,30,1000,600)?;
    let start=Instant::now();let mut sent=false;
    while start.elapsed()<Duration::from_secs(8){s.poll(65536);let text=s.engine.visible_text();
        if !sent&&text.contains("AGENTDOCK_READY"){s.engine.text("roundtrip-123")?;s.engine.terminal.key_down(wezterm_term::KeyCode::Enter,wezterm_term::KeyModifiers::NONE)?;sent=true;}
        if text.contains("AGENTDOCK_ECHO:roundtrip-123"){return Ok(());}std::thread::sleep(Duration::from_millis(20));
    }anyhow::bail!("PTY 自测超时：{:?}; error={:?}",s.engine.visible_text(),s.error)
}
#[cfg(test)]mod tests{
    use super::*;
    use iced::futures::task::{ArcWake,waker};
    use std::sync::atomic::AtomicUsize;
    #[derive(Default)]struct WakeCount(AtomicUsize);
    impl ArcWake for WakeCount{fn wake_by_ref(this:&Arc<Self>){this.0.fetch_add(1,Ordering::Relaxed);}}
    #[test]fn output_wakes_immediately_and_coalesces_until_drained(){
        let signal=Arc::new(OutputSignal::default());
        let mut watch=OutputWatch{id:uuid::Uuid::new_v4(),signal:signal.clone()};
        let count=Arc::new(WakeCount::default());let waker=waker(count.clone());
        let mut cx=TaskContext::from_waker(&waker);
        assert!(Pin::new(&mut watch).poll_next(&mut cx).is_pending());
        for _ in 0..1000{signal.notify();}
        assert_eq!(count.0.load(Ordering::Relaxed),1);
        assert_eq!(Pin::new(&mut watch).poll_next(&mut cx),Poll::Ready(Some(())));
        signal.notify(); // A delivered UI message is still outstanding.
        assert!(Pin::new(&mut watch).poll_next(&mut cx).is_pending());
        signal.scheduled.store(false,Ordering::Release); // UI begins draining.
        signal.notify(); // New output, or output left over after the byte budget.
        assert_eq!(Pin::new(&mut watch).poll_next(&mut cx),Poll::Ready(Some(())));
    }
    #[test]fn output_before_subscription_is_not_lost(){
        let signal=Arc::new(OutputSignal::default());signal.notify();
        let mut watch=OutputWatch{id:uuid::Uuid::new_v4(),signal};
        let waker=iced::futures::task::noop_waker();let mut cx=TaskContext::from_waker(&waker);
        assert_eq!(Pin::new(&mut watch).poll_next(&mut cx),Poll::Ready(Some(())));
    }
    #[test]fn real_pty_io_resize(){super::self_test().unwrap();}
}
