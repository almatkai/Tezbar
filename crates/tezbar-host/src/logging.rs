//! Explicitly opened, bounded file logger. Constructing the native UI opens no log.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
const LIMIT: u64 = 5_000_000;
pub struct FileLog {
    path: PathBuf,
    file: Mutex<File>,
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
            path,
            file: Mutex::new(file),
        })
    }
    pub fn write(&self, line: &[u8]) -> Result<(), String> {
        let mut file = self.file.lock().map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() + line.len() as u64 > LIMIT {
            file.flush().map_err(|e| e.to_string())?;
            let backup = self.path.with_extension("log.1");
            if backup.exists() {
                std::fs::remove_file(&backup).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&self.path, &backup).map_err(|e| e.to_string())?;
            *file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
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
        assert_eq!(
            std::fs::metadata(d.path().join("tezbar.log.1"))
                .unwrap()
                .len(),
            LIMIT
        );
        assert_eq!(
            std::fs::read(d.path().join("tezbar.log")).unwrap(),
            b"second"
        );
    }
}
