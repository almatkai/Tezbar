//! Application display metadata only: never use this cache for integrity decisions.
use core_foundation::{
    base::{CFType, TCFType},
    data::CFData,
    dictionary::CFDictionary,
    propertylist::{create_with_data, kCFPropertyListImmutable},
    string::CFString,
};
use std::{
    collections::HashMap,
    fs::{Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const CAPACITY: usize = 1024;
// Bundle identifiers are optional display metadata. Bound memory for malformed
// bundles, including a file that grows while being read.
const MAX_PLIST_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Metadata> for Stamp {
    fn from(m: Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
#[derive(Default)]
struct Cache {
    entries: HashMap<PathBuf, (Stamp, Option<String>)>,
}
impl Cache {
    fn read(
        &mut self,
        path: &Path,
        parse: impl FnOnce(&[u8]) -> Result<Option<String>, ()>,
    ) -> Option<String> {
        let result = (|| {
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
                .ok()?;
            let metadata = file.metadata().ok()?;
            if !metadata.is_file() || metadata.len() > MAX_PLIST_BYTES {
                return None;
            }
            let before = Stamp::from(metadata);
            if let Some((stamp, value)) = self.entries.get(path) {
                if *stamp == before {
                    return Some(value.clone());
                }
            }
            self.entries.remove(path);
            let mut bytes = Vec::new();
            file.by_ref()
                .take(MAX_PLIST_BYTES + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() as u64 > MAX_PLIST_BYTES {
                return None;
            }
            let value = parse(&bytes).ok()?;
            // Do not publish a read raced by an edit or atomic replacement.
            if before != Stamp::from(file.metadata().ok()?)
                || before != Stamp::from(std::fs::metadata(path).ok()?)
            {
                return None;
            }
            if self.entries.len() >= CAPACITY {
                self.entries.clear();
            }
            self.entries
                .insert(path.to_owned(), (before, value.clone()));
            Some(value)
        })();
        if result.is_none() {
            self.entries.remove(path);
        }
        result.flatten()
    }
}
fn parse(bytes: &[u8]) -> Result<Option<String>, ()> {
    let (raw, _) =
        create_with_data(CFData::from_buffer(bytes), kCFPropertyListImmutable).map_err(|_| ())?;
    // create_with_data transfers an owned CF object; wrapping balances its release.
    let value = unsafe { CFType::wrap_under_create_rule(raw) };
    let Some(dict) = value.downcast::<CFDictionary>() else {
        return Ok(None);
    };
    let Some(value) = dict.find(CFString::new("CFBundleIdentifier").as_CFTypeRef()) else {
        return Ok(None);
    };
    let value = unsafe { CFType::wrap_under_get_rule(*value) };
    Ok(value
        .downcast::<CFString>()
        .map(|s| s.to_string().trim().to_owned())
        .filter(|s| !s.is_empty()))
}
pub(super) fn read(path: &Path) -> Option<String> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .read(path, parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    fn xml(value: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key>{value}</dict></plist>"#
        )
    }
    #[test]
    fn fixtures_and_cache_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir.path().join("Fixture.app/Contents");
        std::fs::create_dir_all(&contents).unwrap();
        let path = contents.join("Info.plist");
        let mut cache = Cache::default();
        let calls = std::cell::Cell::new(0);
        let parser = |bytes: &[u8]| {
            calls.set(calls.get() + 1);
            parse(bytes)
        };
        std::fs::write(
            &path,
            xml("<string>  com.Қазақ.😀&amp;&lt;&quot;  </string>"),
        )
        .unwrap();
        assert_eq!(
            cache.read(&path, parser).as_deref(),
            Some("com.Қазақ.😀&<\"")
        );
        for _ in 0..8 {
            assert!(cache.read(&path, parser).is_some());
        }
        assert_eq!(calls.get(), 1, "unchanged files must not be parsed again");
        for value in ["<integer>42</integer>", "<string> </string>", "<array/>"] {
            std::fs::write(&path, xml(value)).unwrap();
            assert_eq!(cache.read(&path, parser), None);
        }
        std::fs::write(&path, "<plist version=\"1.0\"><dict/></plist>").unwrap();
        assert_eq!(cache.read(&path, parser), None);
        std::fs::write(&path, "corrupt plist").unwrap();
        assert_eq!(cache.read(&path, parser), None);
        let before = calls.get();
        assert_eq!(cache.read(&path, parser), None);
        assert_eq!(calls.get(), before + 1, "parse failures are retried");
        let replacement = contents.join("replacement");
        std::fs::write(&replacement, xml("<string>new</string>")).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert_eq!(cache.read(&path, parser).as_deref(), Some("new"));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(cache.read(&path, parser), None);
        std::fs::write(&path, xml("<string>back</string>")).unwrap();
        assert_eq!(cache.read(&path, parser).as_deref(), Some("back"));
        // Same-size rewrite with preserved mtime must still invalidate via ctime.
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, xml("<string>edit</string>")).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(cache.read(&path, parser).as_deref(), Some("edit"));
        // An edit during parsing must not enter the cache.
        cache.entries.clear();
        assert_eq!(
            cache.read(&path, |bytes| {
                std::fs::write(&path, xml("<string>raced</string>")).unwrap();
                parse(bytes)
            }),
            None
        );
        assert_eq!(cache.read(&path, parser).as_deref(), Some("raced"));
        // Generate binary data using the same platform serializer, without a process.
        use core_foundation::propertylist::{create_data, kCFPropertyListBinaryFormat_v1_0};
        let dict = CFDictionary::from_CFType_pairs(&[(
            CFString::new("CFBundleIdentifier"),
            CFString::new(" binary.😀 "),
        )]);
        let data = create_data(dict.as_CFTypeRef(), kCFPropertyListBinaryFormat_v1_0).unwrap();
        std::fs::write(&path, data.bytes()).unwrap();
        assert_eq!(cache.read(&path, parser).as_deref(), Some("binary.😀"));
    }
    #[test]
    fn oversized_metadata_is_not_read_or_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Info.plist");
        File::create(&path)
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        let mut cache = Cache::default();
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            cache.read(&path, |_| {
                calls.set(calls.get() + 1);
                Ok(Some("unexpected".into()))
            }),
            None
        );
        assert_eq!(calls.get(), 0);
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn cache_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::default();
        for n in 0..=CAPACITY {
            let path = dir.path().join(format!("{n}.app/Contents/Info.plist"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, xml("<string>id</string>")).unwrap();
            assert!(cache.read(&path, parse).is_some());
            assert!(cache.entries.len() <= CAPACITY);
        }
    }
}
