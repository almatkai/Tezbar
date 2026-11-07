//! Bounded replay capabilities over the file owned by the managed PTY.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const CHUNK: usize = 64 * 1024;
const CURSORS: usize = 32;
const IDLE: Duration = Duration::from_secs(60);
pub(super) struct Stream {
    pub generation: String,
    file: File,
    path: PathBuf,
    pub position: u64,
    origin: u64,
    durable: bool,
}
impl Stream {
    pub fn new(file: File, path: PathBuf) -> Result<Self, String> {
        let position = file.metadata().map_err(|e| e.to_string())?.len();
        Ok(Self {
            generation: uuid::Uuid::new_v4().to_string(),
            file,
            path,
            position,
            origin: position,
            durable: true,
        })
    }
    pub fn append(&mut self, bytes: &[u8]) -> Value {
        let start = self.position;
        self.position += bytes.len() as u64;
        if self.durable
            && self
                .file
                .write_all(bytes)
                .and_then(|_| self.file.flush())
                .is_err()
        {
            self.durable = false;
        }
        json!({"generation":self.generation,"start":start,"end":self.position,"durable":self.durable,"bytes":STANDARD.encode(bytes)})
    }
    fn validate(&self) -> Result<(), String> {
        if !self.durable {
            return Err("Terminal history persistence failed; replay unavailable".into());
        }
        let fd = self.file.metadata().map_err(|e| e.to_string())?;
        let path = std::fs::metadata(&self.path)
            .map_err(|e| format!("Terminal history unavailable: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (fd.dev(), fd.ino()) != (path.dev(), path.ino()) {
                return Err("Terminal history was replaced".into());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };
            fn identity(f: &File) -> Result<(u32, u32, u32), String> {
                let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
                if unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) } == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok((
                    info.dwVolumeSerialNumber,
                    info.nFileIndexHigh,
                    info.nFileIndexLow,
                ))
            }
            let current = File::open(&self.path).map_err(|e| e.to_string())?;
            if identity(&current)? != identity(&self.file)? {
                return Err("Terminal history was replaced".into());
            }
        }
        if fd.len() != self.position || path.len() != self.position {
            return Err("Terminal history was truncated or modified".into());
        }
        Ok(())
    }
}
pub(super) type SharedStream = Arc<Mutex<Stream>>;
struct Cursor {
    stream: SharedStream,
    touched: Instant,
}
#[derive(Default)]
pub(super) struct ReplayRegistry {
    cursors: HashMap<String, Cursor>,
}
impl ReplayRegistry {
    pub fn expire(&mut self) {
        self.cursors.retain(|_, c| c.touched.elapsed() < IDLE);
    }
    pub fn open(&mut self, stream: SharedStream) -> Result<Value, String> {
        self.expire();
        if self.cursors.len() >= CURSORS {
            return Err("Terminal replay capacity reached".into());
        }
        let s = stream.lock().unwrap();
        s.validate()?;
        let token = uuid::Uuid::new_v4().to_string();
        let result = json!({"generation":s.generation,"replay":token,"watermark":s.position,"origin":s.origin});
        drop(s);
        self.cursors.insert(
            token,
            Cursor {
                stream,
                touched: Instant::now(),
            },
        );
        Ok(result)
    }
    pub fn release(&mut self, token: &str) -> bool {
        self.expire();
        self.cursors.remove(token).is_some()
    }
    pub fn read(&mut self, token: &str, position: u64, limit: usize) -> Result<Value, String> {
        self.expire();
        if limit == 0 || limit > CHUNK {
            return Err("Invalid terminal replay chunk limit".into());
        }
        let c = self
            .cursors
            .get_mut(token)
            .ok_or("Terminal replay expired or released")?;
        c.touched = Instant::now();
        let s = c.stream.lock().unwrap();
        s.validate()?;
        if position > s.position {
            return Err("Terminal replay offset exceeds watermark".into());
        }
        let n = limit.min((s.position - position) as usize);
        let mut bytes = vec![0; n];
        let mut read = 0;
        while read < n {
            #[cfg(unix)]
            let count = {
                use std::os::unix::fs::FileExt;
                s.file.read_at(&mut bytes[read..], position + read as u64)
            };
            #[cfg(windows)]
            let count = {
                use std::os::windows::fs::FileExt;
                s.file.seek_read(&mut bytes[read..], position + read as u64)
            };
            let count = count.map_err(|e| format!("Terminal replay read failed: {e}"))?;
            if count == 0 {
                return Err("Terminal history truncated during replay".into());
            }
            read += count;
        }
        Ok(
            json!({"generation":s.generation,"start":position,"end":position+n as u64,"watermark":s.position,"bytes":STANDARD.encode(bytes)}),
        )
    }
    pub fn is_empty(&self) -> bool {
        self.cursors.is_empty()
    }
    pub fn clear(&mut self) {
        self.cursors.clear();
    }
}
pub(super) fn decode_input(encoded: &str) -> Result<Vec<u8>, String> {
    if encoded.is_empty() || encoded.len() > super::INPUT_QUEUE_LIMIT_BYTES.div_ceil(3) * 4 {
        return Err("Terminal input size is invalid".into());
    }
    let data = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid terminal input base64")?;
    if data.is_empty() || data.len() > super::INPUT_QUEUE_LIMIT_BYTES {
        return Err("Terminal input exceeds 1 MiB".into());
    }
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn stream() -> (tempfile::TempDir, SharedStream) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("history");
        let f = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(&p)
            .unwrap();
        (d, Arc::new(Mutex::new(Stream::new(f, p).unwrap())))
    }
    #[test]
    fn review_m10_f6_origin_distinguishes_prior_history_from_this_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        std::fs::write(&path, b"old\x1b[6n").unwrap();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .unwrap();
        let stream = Arc::new(Mutex::new(Stream::new(file, path).unwrap()));
        stream.lock().unwrap().append(b"\x1b[6nnew");
        let mut registry = ReplayRegistry::default();
        let attached = registry.open(stream.clone()).unwrap();
        assert_eq!(attached["origin"], 7);
        assert_eq!(attached["watermark"], 14);
        stream.lock().unwrap().append(b"later");
        let again = registry.open(stream).unwrap();
        assert_eq!(again["origin"], 7);
        assert_eq!(again["watermark"], 19);
    }
    #[test]
    fn bounded_history_repeated_bytes_watermark_release_expiry_and_truncation() {
        let (d, s) = stream();
        let mut r = ReplayRegistry::default();
        s.lock().unwrap().append(&vec![b'x'; 3 * 1024 * 1024]);
        let a = r.open(s.clone()).unwrap();
        let token = a["replay"].as_str().unwrap();
        let mut at = 0;
        while at < 3 * 1024 * 1024 {
            let v = r.read(token, at, CHUNK).unwrap();
            let b = STANDARD.decode(v["bytes"].as_str().unwrap()).unwrap();
            assert!(b.len() <= CHUNK);
            assert_eq!(b, vec![b'x'; b.len()]);
            at = v["end"].as_u64().unwrap();
        }
        s.lock().unwrap().append(b"xx");
        let v = r.read(token, at, CHUNK).unwrap();
        assert_eq!(v["end"], at + 2);
        assert!(r.release(token));
        assert!(r.read(token, 0, CHUNK).is_err());
        let a = r.open(s.clone()).unwrap();
        let t = a["replay"].as_str().unwrap();
        r.cursors.get_mut(t).unwrap().touched = Instant::now() - IDLE;
        assert!(r.read(t, 0, 1).is_err());
        let a = r.open(s.clone()).unwrap();
        std::fs::write(d.path().join("history"), b"short").unwrap();
        assert!(r.read(a["replay"].as_str().unwrap(), 0, 1).is_err());
    }
    #[test]
    fn deletion_replacement_generation_and_disk_failure_are_truthful() {
        let (d, s) = stream();
        let mut r = ReplayRegistry::default();
        let a = r.open(s.clone()).unwrap();
        std::fs::remove_file(d.path().join("history")).unwrap();
        assert!(r.read(a["replay"].as_str().unwrap(), 0, 1).is_err());
        std::fs::write(d.path().join("history"), b"").unwrap();
        assert!(r.open(s.clone()).is_err());
        let (_, other) = stream();
        assert_ne!(
            s.lock().unwrap().generation,
            other.lock().unwrap().generation
        );
        let readonly = File::open(d.path().join("history")).unwrap();
        let mut failure = Stream::new(readonly, d.path().join("history")).unwrap();
        let event = failure.append(b"not persisted");
        assert_eq!(event["durable"], false);
        assert!(failure.validate().is_err());
    }
    #[test]
    fn registry_capacity_and_cancel_are_bounded() {
        let (_d, s) = stream();
        let mut r = ReplayRegistry::default();
        for _ in 0..CURSORS {
            r.open(s.clone()).unwrap();
        }
        assert!(r.open(s.clone()).is_err());
        r.clear();
        assert!(r.is_empty());
        assert!(r.open(s).is_ok());
    }
    #[test]
    fn binary_limits_and_invalid_encoding() {
        assert_eq!(
            decode_input(&STANDARD.encode([0x1b, 0x5b, 0x4d, 0xff])).unwrap(),
            [0x1b, 0x5b, 0x4d, 0xff]
        );
        assert!(decode_input("!!!!").is_err());
        assert!(
            decode_input(&STANDARD.encode(vec![0; super::super::INPUT_QUEUE_LIMIT_BYTES + 1]))
                .is_err()
        );
    }
}
