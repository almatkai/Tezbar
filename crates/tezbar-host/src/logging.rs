//! Explicitly opened, bounded file logger. Constructing the native UI opens no log.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
const LIMIT: u64 = 5_000_000;
pub struct FileLog {
    file: Mutex<File>,
    path: PathBuf,
}
impl FileLog {
    pub fn open(directory: &Path, name: &str) -> Result<Self, String> {
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err("Invalid log name".into());
        }
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!("{name}.log"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            file: Mutex::new(file),
            path,
        })
    }
    pub fn write(&self, line: &[u8]) -> Result<(), String> {
        let mut file = self.file.lock().map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() + line.len() as u64 > LIMIT {
            file.flush().map_err(|e| e.to_string())?;
            // The locked legacy logger defaults to KeepOne: retain only the
            // active 5 MB file, rather than introducing a second log history.
            // Windows append handles lack FILE_WRITE_DATA, which set_len needs.
            // Keep the append handle for writes and truncate through a short-lived
            // write handle under the same mutex.
            OpenOptions::new()
                .write(true)
                .open(&self.path)
                .and_then(|truncate| truncate.set_len(0))
                .map_err(|e| e.to_string())?;
        }
        file.write_all(&line[..line.len().min(LIMIT as usize)])
            .map_err(|e| e.to_string())
    }
}
impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            let _ =
                self.write(format!("[{}] {}: {}\n", r.level(), r.target(), r.args()).as_bytes());
        }
    }
    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotates_without_retaining_unbounded_history() {
        let d = tempfile::tempdir().unwrap();
        let l = FileLog::open(d.path(), "tezbar").unwrap();
        l.write(&vec![b'a'; LIMIT as usize]).unwrap();
        l.write(b"second").unwrap();
        assert!(!d.path().join("tezbar.log.1").exists());
        assert_eq!(
            std::fs::read(d.path().join("tezbar.log")).unwrap(),
            b"second"
        );
    }
}
