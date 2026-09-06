//! Paged, non-recursive filesystem access. At most one page per expanded node.
use crossbeam_channel::{bounded,Receiver,Sender};
use std::{collections::HashMap,fs::{self,File,ReadDir},io::Read,path::{Path,PathBuf},sync::Arc};
#[derive(Debug,Clone)]
pub struct Entry { pub path:PathBuf,pub name:String,pub directory:bool,pub symlink:bool }
#[derive(Debug,Clone)]
pub struct Page { pub entries:Vec<Entry>,pub index:usize,pub more:bool }
pub struct DirectoryReader { cursor:ReadDir,pending:Option<std::io::Result<fs::DirEntry>>,pub next_index:usize }
impl DirectoryReader {
    pub fn open(path:&Path)->std::io::Result<Self>{Ok(Self {cursor:fs::read_dir(path)?,pending:None,next_index:0})}
    pub fn next_page(&mut self,size:usize)->std::io::Result<Page>{
        let mut entries=Vec::with_capacity(size);
        for _ in 0..size {
            let Some(next)=self.pending.take().or_else(||self.cursor.next())else{break};
            let e=next?;let kind=e.file_type()?;let path=e.path();
            let directory=kind.is_dir()||(kind.is_symlink()&&fs::metadata(&path).is_ok_and(|m|m.is_dir()));
            entries.push(Entry {name:e.file_name().to_string_lossy().into_owned(),path,directory,symlink:kind.is_symlink()});
        }
        self.pending=self.cursor.next();
        // This is only page-local sorting: it does not promise a globally sorted million-file directory.
        entries.sort_by(|a,b|b.directory.cmp(&a.directory).then_with(||a.name.cmp(&b.name)));
        let page=Page{entries,index:self.next_index,more:self.pending.is_some()};self.next_index+=1;Ok(page)
    }
}
#[derive(Debug,Clone)]
pub struct Preview {pub path:PathBuf,pub text:String,pub markdown:bool,pub truncated:bool,pub lossy:bool}
pub fn read_preview(path:&Path,limit:usize)->anyhow::Result<Preview>{
    let metadata=fs::metadata(path)?;
    anyhow::ensure!(metadata.is_file(),"只预览普通文件，不读取目录、设备或管道");
    let mut bytes=Vec::new();File::open(path)?.take((limit+1)as u64).read_to_end(&mut bytes)?;
    let truncated=bytes.len()>limit;bytes.truncate(limit);
    let(text,lossy)=if bytes.starts_with(&[255,254])||bytes.starts_with(&[254,255]) {
        let le=bytes[0]==255;
        let words:Vec<_>=bytes[2..].chunks_exact(2).map(|x|if le{u16::from_le_bytes([x[0],x[1]])}else{u16::from_be_bytes([x[0],x[1]])}).collect();
        let decoded=String::from_utf16(&words);let lossy=decoded.is_err();(decoded.unwrap_or_else(|_|String::from_utf16_lossy(&words)),lossy)
    }else{
        anyhow::ensure!(!bytes.iter().take(8192).any(|&b|b==0),"文件看起来是二进制，未作为文本打开");
        let bytes=bytes.strip_prefix(&[239,187,191]).unwrap_or(&bytes);
        let text=String::from_utf8_lossy(bytes);let lossy=matches!(text,std::borrow::Cow::Owned(_));(text.into_owned(),lossy)
    };
    let markdown=path.extension().and_then(|e|e.to_str()).is_some_and(|e|e.eq_ignore_ascii_case("md")||e.eq_ignore_ascii_case("markdown"));
    Ok(Preview{path:path.into(),text,markdown,truncated,lossy})
}
#[derive(Debug)]pub enum Request{Page{path:PathBuf,index:usize,generation:u64},Forget(PathBuf),Roots}
#[derive(Debug,Clone)]pub enum Completed{
    Omp(Result<Vec<crate::omp::Record>,String>),
    Page{path:PathBuf,generation:u64,result:Result<Page,String>},
    Preview{token:u64,result:Result<Arc<Preview>,String>},Roots(Vec<PathBuf>),
}
pub struct FileService{tx:Sender<Request>,preview_tx:Sender<(PathBuf,u64)>,omp_tx:Sender<PathBuf>,pub rx:Receiver<Completed>}
impl FileService{
    pub fn new(page_size:usize,preview_limit:usize)->std::io::Result<Self>{
        let(tx,requests)=bounded::<Request>(32);let(out,rx)=bounded(32);
        let (omp_tx, omp_rx) = bounded::<PathBuf>(1); let omp_out = out.clone();
        std::thread::Builder::new().name("agentdock-omp-sessions".into()).spawn(move || {
            let mut scanner = crate::omp::Scanner::default();
            let mut previous = None;
            while let Ok(root) = omp_rx.recv() {
                let result = scanner.scan(&root).map_err(|e| e.to_string());
                if previous.as_ref() != Some(&result) {
                    previous = Some(result.clone());
                    if omp_out.send(Completed::Omp(result)).is_err() { break; }
                }
            }
        })?;
        let(preview_tx,preview_rx)=bounded::<(PathBuf,u64)>(8);let preview_out=out.clone();
        // A slow directory read must not block the separate preview worker.
        std::thread::Builder::new().name("agentdock-preview".into()).spawn(move||{
            while let Ok((mut path,mut token))=preview_rx.recv(){
                for newer in preview_rx.try_iter(){(path,token)=newer;}
                let result=read_preview(&path,preview_limit).map(Arc::new).map_err(|e|e.to_string());
                if preview_out.send(Completed::Preview{token,result}).is_err(){break;}
            }
        })?;
        std::thread::Builder::new().name("agentdock-files".into()).spawn(move||{
            let mut cursors:HashMap<PathBuf,DirectoryReader>=HashMap::new();
            while let Ok(request)=requests.recv(){
                let result=match request{
                    Request::Page{path,index,generation}=>{
                        let value=(||->std::io::Result<Page>{
                            if cursors.get(&path).is_none_or(|r|r.next_index!=index){
                                if cursors.len()>=64{cursors.clear();}
                                let mut reader=DirectoryReader::open(&path)?;
                                for _ in 0..index{if !reader.next_page(page_size)?.more{break;}}
                                cursors.insert(path.clone(),reader);
                            }
                            cursors.get_mut(&path).expect("reader inserted").next_page(page_size)
                        })().map_err(|e|e.to_string());Completed::Page{path,generation,result:value}
                    }
                    Request::Forget(path)=>{cursors.retain(|p,_|!p.starts_with(&path));continue;}
                    Request::Roots=>{
                        #[cfg(windows)]let roots=('A'..='Z').map(|x|PathBuf::from(format!("{x}:\\"))).filter(|p|p.is_dir()).collect();
                        #[cfg(unix)]let roots=vec![PathBuf::from("/")];
                        Completed::Roots(roots)
                    }
                };if out.send(result).is_err(){break;}
            }
        })?;Ok(Self{tx,preview_tx,omp_tx,rx})
    }
    pub fn request(&self,request:Request)->Result<(),String>{self.tx.try_send(request).map_err(|_|"文件请求队列繁忙，请重试".into())}
    pub fn scan_omp(&self, root:PathBuf) { let _ = self.omp_tx.try_send(root); }
    pub fn preview(&self,path:PathBuf,token:u64)->Result<(),String>{self.preview_tx.try_send((path,token)).map_err(|_|"预览队列繁忙，请重试".into())}
}
#[derive(Debug,Clone)]pub struct Listing{pub generation:u64,pub loading:bool,pub error:Option<String>,pub page:Option<Page>}
#[derive(Debug,Clone)]pub enum Kind{Entry{directory:bool},Previous(usize),Next(usize),Notice}
#[derive(Debug,Clone)]pub struct TreeRow{pub path:PathBuf,pub label:String,pub depth:usize,pub kind:Kind}
#[derive(Default)]pub struct FileTree{pub roots:Vec<PathBuf>,pub listings:HashMap<PathBuf,Listing>,pub rows:Vec<TreeRow>,generation:u64}
impl FileTree{
    pub fn rebuild(&mut self){let mut rows=Vec::new();for root in &self.roots{self.append(root,root.display().to_string(),true,0,&mut rows);}self.rows=rows;}
    fn append(&self,path:&Path,label:String,directory:bool,depth:usize,rows:&mut Vec<TreeRow>){
        rows.push(TreeRow{path:path.into(),label,depth,kind:Kind::Entry{directory}});
        let Some(listing)=self.listings.get(path)else{return};
        if let Some(page)=&listing.page{
            if page.index>0{rows.push(TreeRow{path:path.into(),label:"‹ 上一页".into(),depth:depth+1,kind:Kind::Previous(page.index-1)});}
            for e in &page.entries{self.append(&e.path,format!("{}{}",e.name,if e.symlink{" ↗"}else{""}),e.directory,depth+1,rows);}
            if page.more{rows.push(TreeRow{path:path.into(),label:format!("下一页 ›  ·  第 {} 页",page.index+1),depth:depth+1,kind:Kind::Next(page.index+1)});}
            if page.entries.is_empty(){rows.push(TreeRow{path:path.into(),label:"空目录".into(),depth:depth+1,kind:Kind::Notice});}
        }
        if listing.loading||listing.error.is_some(){rows.push(TreeRow{path:path.into(),label:listing.error.clone().unwrap_or_else(||"读取中…".into()),depth:depth+1,kind:Kind::Notice});}
    }
    pub fn collapse(&mut self,path:&Path){self.listings.retain(|p,_|!p.starts_with(path));self.rebuild();}
    pub fn begin(&mut self,path:PathBuf,index:usize)->Result<Request,String>{
        if path.components().count()>64{return Err("已达到 64 层展开上限；请检查符号链接循环".into());}
        if !self.listings.contains_key(&path)&&self.listings.len()>=64{return Err("已展开 64 个目录；收起一些目录再继续".into());}
        self.listings.retain(|p,_|p==&path||!p.starts_with(&path));self.generation=self.generation.wrapping_add(1);
        self.listings.insert(path.clone(),Listing{generation:self.generation,loading:true,error:None,page:None});self.rebuild();
        Ok(Request::Page{path,index,generation:self.generation})
    }
    pub fn complete(&mut self,path:&Path,generation:u64,result:Result<Page,String>){
        if let Some(l)=self.listings.get_mut(path).filter(|l|l.generation==generation){l.loading=false;match result{Ok(p)=>l.page=Some(p),Err(e)=>l.error=Some(e)}}self.rebuild();
    }
}
pub fn visible_range(total:usize,offset:f32,height:f32,row:f32)->std::ops::Range<usize>{
    let start=(offset.max(0.0)/row.max(1.0)).floor()as usize;
    let end=((offset.max(0.0)+height.max(0.0))/row.max(1.0)).ceil()as usize+1;
    start.min(total)..end.min(total)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn viewport_is_constant_size(){assert!(visible_range(10_000_000,3_000_000.0,600.0,26.0).len()<=25);}
    #[test]fn enumerate_in_pages(){let d=tempfile::tempdir().unwrap();for i in 0..700{fs::write(d.path().join(i.to_string()),"").unwrap();}let mut r=DirectoryReader::open(d.path()).unwrap();let a=r.next_page(256).unwrap();let b=r.next_page(256).unwrap();let c=r.next_page(256).unwrap();assert_eq!((a.entries.len(),b.entries.len(),c.entries.len()),(256,256,188));assert!(a.more&&b.more&&!c.more);let set:std::collections::HashSet<_>=a.entries.into_iter().chain(b.entries).chain(c.entries).map(|e|e.path).collect();assert_eq!(set.len(),700);}
    #[test]fn not_recursive(){let d=tempfile::tempdir().unwrap();fs::create_dir(d.path().join("child")).unwrap();fs::write(d.path().join("child/file"),"").unwrap();assert_eq!(DirectoryReader::open(d.path()).unwrap().next_page(256).unwrap().entries.len(),1);}
    #[test]fn late_response_not_reopened(){let mut tree=FileTree::default();let p=PathBuf::from("/x");let Request::Page{generation,..}=tree.begin(p.clone(),0).unwrap()else{panic!()};tree.collapse(&p);tree.complete(&p,generation,Ok(Page{entries:vec![],index:0,more:false}));assert!(tree.listings.is_empty());}
    #[test]fn preview_capped(){let d=tempfile::tempdir().unwrap();let p=d.path().join("README.MD");fs::write(&p,"a".repeat(100000)).unwrap();let x=read_preview(&p,1024).unwrap();assert!(x.truncated&&x.markdown);assert_eq!(x.text.len(),1024);}
    #[test]fn binary_rejected(){let d=tempfile::tempdir().unwrap();let p=d.path().join("x");fs::write(&p,[1,0,2]).unwrap();assert!(read_preview(&p,100).is_err());}
    #[test]fn utf16_decoded(){let d=tempfile::tempdir().unwrap();let p=d.path().join("x");let mut b=vec![255,254];for w in "中文".encode_utf16(){b.extend(w.to_le_bytes());}fs::write(&p,b).unwrap();assert_eq!(read_preview(&p,100).unwrap().text,"中文");}
    #[test]fn devices_not_read(){assert!(read_preview(Path::new("/"),100).is_err());}
}
