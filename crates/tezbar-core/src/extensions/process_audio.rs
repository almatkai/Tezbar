//! Session-owned native playback. Uses the same MCI/afplay backends as Translate.
use std::{path::{Path,PathBuf},io::Read};
pub(super) struct Playback {
    file: PathBuf,
    #[cfg(windows)] alias: String,
    #[cfg(target_os="macos")] child: std::process::Child,
}
impl Playback {
    pub fn open(root:&Path, source:&str)->Result<Self,String> {
        let path=Path::new(source);
        let rel=path.strip_prefix(root).map_err(|_|"Audio must be in private temp directory")?;
        if rel.components().count()!=1 || !matches!(rel.components().next(),Some(std::path::Component::Normal(_))) || source.contains(['"','\0','\n','\r']) {return Err("Invalid audio path".into());}
        let meta=std::fs::symlink_metadata(path).map_err(|_|"Cannot inspect audio")?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len()==0 || meta.len()>8*1024*1024 {return Err("Invalid audio file bounds".into());}
        let mut bytes=Vec::new();std::fs::File::open(path).map_err(|_|"Cannot read audio")?.take(8*1024*1024+1).read_to_end(&mut bytes).map_err(|_|"Cannot read audio")?;
        if bytes.len()>8*1024*1024 || !(bytes.starts_with(b"ID3") || bytes.get(0)==Some(&255)&&bytes.get(1).is_some_and(|b|b&224==224)) {return Err("Expected bounded MP3 audio".into());}
        let file=root.join(format!("playback-{}.mp3",uuid::Uuid::new_v4()));
        std::fs::write(&file,bytes).map_err(|_|"Cannot stage audio")?;
        let result=Self::start(file.clone());
        if result.is_err(){let _=std::fs::remove_file(file);} result
    }
    #[cfg(windows)]
    fn start(file:PathBuf)->Result<Self,String>{
        let alias=format!("tezbar{}",uuid::Uuid::new_v4().simple());
        send(&format!("open \"{}\" type mpegvideo alias {alias}",file.display()))?;
        if let Err(e)=send(&format!("play {alias}")){let _=send(&format!("close {alias}"));return Err(e);}
        Ok(Self{file,alias})
    }
    #[cfg(target_os="macos")]
    fn start(file:PathBuf)->Result<Self,String>{
        let child=std::process::Command::new("/usr/bin/afplay").arg(&file).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map_err(|_|"Cannot start native audio")?;
        Ok(Self{file,child})
    }
    #[cfg(not(any(windows,target_os="macos")))]
    fn start(_:PathBuf)->Result<Self,String>{Err("Audio unsupported on this platform".into())}
}
impl Drop for Playback {
    fn drop(&mut self){
        #[cfg(windows)] if !self.alias.is_empty() {let _=send(&format!("stop {}",self.alias));let _=send(&format!("close {}",self.alias));}
        #[cfg(target_os="macos")] {let _=self.child.kill();let _=self.child.wait();}
        let _=std::fs::remove_file(&self.file);
    }
}
#[cfg(windows)]
fn send(command:&str)->Result<(),String>{
    #[link(name="winmm")] extern "system" {fn mciSendStringW(command:*const u16,result:*mut u16,length:u32,callback:isize)->u32;}
    let wide:Vec<u16>=command.encode_utf16().chain(Some(0)).collect();
    let code=unsafe{mciSendStringW(wide.as_ptr(),std::ptr::null_mut(),0,0)};
    if code==0{Ok(())}else{Err(format!("Native audio failed ({code})"))}
}
#[test]
fn rejects_unscoped_and_invalid_audio_without_playing(){
    let root=std::env::temp_dir().join(format!("audio-test-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
    assert!(Playback::open(&root,"outside.mp3").is_err());
    for bytes in [vec![],b"not mp3".to_vec(),vec![0;8*1024*1024+1]] {let p=root.join("test.mp3");std::fs::write(&p,bytes).unwrap();assert!(Playback::open(&root,p.to_str().unwrap()).is_err());}
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(all(test,windows))]
pub(super) fn fixture(file:PathBuf)->Playback { Playback{file,alias:String::new()} }
