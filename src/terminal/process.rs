use super::Engine;
use crate::model::{Launch,Settings};
use anyhow::{Context,Result};
use crossbeam_channel::{bounded,Receiver,Sender};
use portable_pty::{native_pty_system,Child,CommandBuilder,MasterPty,PtySize};
use std::{io::{Read,Write},path::Path,time::{Duration,Instant}};
use std::{pin::Pin,sync::{Arc,atomic::{AtomicBool,Ordering}},task::{Context as TaskContext,Poll}};
use iced::futures::{Stream,task::AtomicWaker};
#[derive(Debug)]enum Output{Bytes(Vec<u8>),Written(usize),Eof,Error(String)}
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
        // Called by WezTerm's dedicated ThreadedWriter, never by the UI.
        // Temporary backpressure must not terminate that upstream writer.
        self.0.send(b.to_vec()).map_err(|_|std::io::Error::new(std::io::ErrorKind::BrokenPipe,"PTY writer disconnected"))?;Ok(b.len())
    }
    fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
pub struct Session{
    pub engine:Engine,master:Box<dyn MasterPty+Send>,child:Option<Box<dyn Child+Send+Sync>>,
    output:Receiver<Output>,size:(usize,usize,usize,usize),watch_id:uuid::Uuid,signal:Arc<OutputSignal>,pub exited:Option<String>,pub error:Option<String>,pub unread:bool,
    pub raw:bool,pub raw_waiting:bool,pub raw_output:Vec<u8>,pub input_written:usize,input:Sender<Vec<u8>>,
}
impl Session{
    pub fn spawn(cwd:&Path,launch:&Launch,settings:&Settings)->Result<Self>{
        Self::spawn_mode(cwd,launch,settings,false)
    }
    pub fn spawn_mode(cwd:&Path,launch:&Launch,settings:&Settings,raw:bool)->Result<Self>{
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
            while let Ok(bytes)=input_rx.recv(){if let Err(e)=writer.write_all(&bytes).and_then(|_|writer.flush()){publish(&errors_tx,&writer_signal,Output::Error(e.to_string()));break;}
                if raw&&!publish(&errors_tx,&writer_signal,Output::Written(bytes.len())){break;}
            }
        });
        if let Err(e)=writer_thread{let _=child.kill();let _=child.wait();return Err(e.into());}
        Ok(Self{engine:Engine::new(settings,Box::new(Input(input_tx.clone()))),master:pair.master,child:Some(child),output,size:(80,24,0,0),watch_id:uuid::Uuid::new_v4(),signal,exited:None,error:None,unread:false,raw,raw_waiting:raw,raw_output:vec![],input_written:0,input:input_tx})
    }
    pub fn instance_id(&self)->uuid::Uuid{self.watch_id}
    pub fn raw_input(&self,bytes:Vec<u8>)->Result<()>{
        anyhow::ensure!(self.raw&&bytes.len()<=32768,"无效的终端输入批次");
        self.input.try_send(bytes).map_err(|_|anyhow::anyhow!("终端输入队列不可用"))
    }
    pub fn running(&self)->bool{self.exited.is_none()}
    pub(crate) fn watch(&self) -> OutputWatch { OutputWatch { id:self.watch_id,signal:self.signal.clone() } }
    pub fn poll(&mut self,budget:usize)->bool{
        self.signal.scheduled.store(false,Ordering::Release);
        let mut used=0;let mut changed=false;let deadline=Instant::now()+Duration::from_millis(3);
        while used<budget&&Instant::now()<deadline&&(!self.raw||!self.raw_waiting){match self.output.try_recv(){
            Ok(Output::Bytes(b))=>{used+=b.len();if self.raw{self.raw_output.extend(b);}else{self.engine.advance(b);}changed=true;self.unread=true;},
            Ok(Output::Written(n))=>{self.input_written+=n;changed=true;},
            Ok(Output::Eof)=>break,
            Ok(Output::Error(e))=>{self.error=Some(e);changed=true;},Err(_)=>break,
        }}
        if self.exited.is_none(){if let Some(child)=&mut self.child{match child.try_wait(){
            Ok(Some(status))=>{self.exited=Some(status.to_string());changed=true;},Ok(None)=>{},
            Err(e)=>{self.error=Some(e.to_string());self.exited=Some("无法读取退出状态".into());changed=true;}
        }}}
        if !self.output.is_empty()&&(!self.raw||!self.raw_waiting){self.signal.notify();}
        changed
    }
    pub fn resize(&mut self,cols:usize,rows:usize,width:usize,height:usize)->Result<()>{
        let(cols,rows,width,height)=(cols.clamp(2,1000),rows.clamp(1,500),width.min(u16::MAX as usize),height.min(u16::MAX as usize));
        if self.size==(cols,rows,width,height){return Ok(());}
        self.master.resize(PtySize{cols:cols.clamp(2,1000)as u16,rows:rows.clamp(1,500)as u16,pixel_width:width.min(u16::MAX as usize)as u16,pixel_height:height.min(u16::MAX as usize)as u16})?;
        if !self.raw{self.engine.resize(cols,rows,width,height);}self.size=(cols,rows,width,height);Ok(())
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
    #[test]fn input_backpressure_recovers_without_losing_bytes(){
        let(tx,rx)=bounded(2);let mut input=Input(tx);
        input.write_all(b"first").unwrap();input.write_all(b"second").unwrap();
        let(done_tx,done_rx)=bounded(1);
        let worker=std::thread::spawn(move||{
            input.write_all(b"third").unwrap();input.write_all(b"fourth").unwrap();done_tx.send(()).unwrap();
        });
        assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
        for expected in [b"first".as_slice(),b"second",b"third",b"fourth"]{
            assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(),expected);
        }
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();worker.join().unwrap();
    }
    #[test]fn disconnected_input_reports_broken_pipe(){
        let(tx,rx)=bounded(1);drop(rx);
        assert_eq!(Input(tx).write(b"x").unwrap_err().kind(),std::io::ErrorKind::BrokenPipe);
    }
    #[test]fn core_writer_continues_after_full_queue(){
        let(tx,rx)=bounded(2);let mut engine=Engine::new(&Settings::default(),Box::new(Input(tx)));
        for _ in 0..100{engine.paste("中文-input").unwrap();}
        let deadline=Instant::now()+Duration::from_secs(2);
        while rx.len()<2&&Instant::now()<deadline{std::thread::yield_now();}
        assert_eq!(rx.len(),2);
        std::thread::sleep(Duration::from_millis(50));
        let expected="中文-input".repeat(100).into_bytes();let mut actual=Vec::new();
        while actual.len()<expected.len(){actual.extend(rx.recv_timeout(Duration::from_secs(2)).unwrap());}
        assert_eq!(actual,expected);
        engine.paste("still-alive").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(),b"still-alive");
    }
    #[test]fn repeated_resize_preserves_generation_and_selection(){
        let mut s=Session::spawn(&std::env::current_dir().unwrap(),&Launch::default(),&Settings::default()).unwrap();
        s.resize(90,20,900,400).unwrap();s.engine.advance("中文");
        s.engine.select(0,0,super::super::engine::SelectionMode::Character);
        let generation=s.engine.generation();let selection=s.engine.selection_text();
        s.resize(90,20,900,400).unwrap();assert_eq!(s.engine.generation(),generation);
        assert_eq!(s.engine.selection_text(),selection);
    }
    #[test]fn raw_pty_waits_for_frontend_and_preserves_bytes(){
        #[cfg(windows)]let launch=Launch{program:"cmd.exe".into(),args:vec!["/Q".into(),"/V:ON".into(),"/C".into(),"echo RAW_READY & set /p token= & echo RAW_ECHO:!token!".into()]};
        #[cfg(unix)]let launch=Launch{program:"/bin/sh".into(),args:vec!["-c".into(),"printf 'RAW_READY\\n'; read line; printf 'RAW_ECHO:%s\\n' \"$line\"".into()]};
        let mut session=Session::spawn_mode(&std::env::current_dir().unwrap(),&launch,&Settings::default(),true).unwrap();
        let generation=session.engine.generation();let deadline=Instant::now()+Duration::from_secs(8);
        while session.output.is_empty()&&Instant::now()<deadline{std::thread::sleep(Duration::from_millis(5));}
        assert!(!session.output.is_empty());session.poll(65536);assert!(session.raw_output.is_empty());
        session.raw_waiting=false;let mut received=Vec::new();let mut sent=false;let mut written=0;
        while Instant::now()<deadline{
            session.poll(65536);received.append(&mut session.raw_output);written+=std::mem::take(&mut session.input_written);
            let text=String::from_utf8_lossy(&received);
            if !sent&&text.contains("RAW_READY"){session.raw_input(b"roundtrip-123\r".to_vec()).unwrap();sent=true;}
            if text.contains("RAW_ECHO:roundtrip-123")&&written>0{break;}
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(String::from_utf8_lossy(&received).contains("RAW_ECHO:roundtrip-123"));assert!(written>0);
        assert_eq!(session.engine.generation(),generation,"raw mode must bypass the native parser");
        assert!(session.raw_input(vec![0;32769]).is_err());
    }
}
