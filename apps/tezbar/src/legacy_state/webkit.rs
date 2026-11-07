//! Read ONLY a quiescent, origin-attested SQLite candidate supplied by the launch owner.
//! Modern hashed WKWebsiteData origins use the limited Persistence decoder below;
//! a directory name/hash alone is never origin evidence. No speculative profile scan.
use super::*;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::path::Path;

pub const ORIGIN: &str = "tauri://localhost";
/// Finite external seam: decoder must verify BOTH top and client origins, not substring-match.
/// ModernOrigin implements the traced format; parent must verify staged OS variants before export.
pub trait OriginVerifier {
    fn verify(&self, database: &Path, exact_origin: &str) -> Result<(), Diagnostic>;
}
pub fn read(path: &Path, origin: &dyn OriginVerifier) -> ReadReport {
    match read_inner(path, origin) {
        Ok(v) => v,
        Err(e) => ReadReport::unavailable(e),
    }
}

/// Connection URI helper, not a path-discovery API. Canonical Windows paths have a
/// verbatim prefix; normalize drive paths to file:/C:/... and refuse UNC variants.
fn immutable_uri(canonical: &Path) -> Result<String, Diagnostic> {
    let s = canonical.to_str().ok_or(Diagnostic::UnsupportedEncoding)?;
    #[cfg(windows)]
    let s = {
        let s = s.strip_prefix(r"\\?\").unwrap_or(s).replace('\\', "/");
        if s.starts_with("//") || s.starts_with("UNC/") {
            return Err(Diagnostic::UnsupportedEncoding);
        }
        if s.as_bytes().get(1) == Some(&b':') {
            format!("/{s}")
        } else {
            s
        }
    };
    let encoded: String = s
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/:".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    Ok(format!("file:{encoded}?mode=ro&immutable=1"))
}
fn read_inner(path: &Path, origin: &dyn OriginVerifier) -> Result<ReadReport, Diagnostic> {
    origin.verify(path, ORIGIN)?;
    // immutable=1 deliberately cannot replay WAL. Refuse even a zero-length sidecar rather
    // than ignore a transaction or create/repair SHM/WAL. Retry once the old app is closed.
    let sidecar = |suffix: &str| {
        let mut p = path.as_os_str().to_os_string();
        p.push(suffix);
        std::path::PathBuf::from(p)
    };
    if sidecar("-wal").exists() || sidecar("-shm").exists() || sidecar("-journal").exists() {
        return Err(Diagnostic::WalPresent);
    }
    let before = std::fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Diagnostic::Missing
        } else {
            Diagnostic::Corrupt
        }
    })?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(Diagnostic::Corrupt);
    }
    if before.len() > TOTAL_LIMIT as u64 {
        return Err(Diagnostic::TooLarge);
    }
    // Build a URI without interpreting ?, #, % or non-ASCII filename bytes as URI syntax.
    let canonical = path.canonicalize().map_err(|_| Diagnostic::Corrupt)?;
    let uri = immutable_uri(&canonical)?;
    let db = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| Diagnostic::Corrupt)?;
    // Hard SQLite VM/allocation limits, using the same locked engine through rusqlite's
    // public ffi (no additional crate feature or second SQLite). Callback context is boxed
    // for a stable address and removed before it is freed, including every error return.
    struct VmBudget {
        handle: *mut rusqlite::ffi::sqlite3,
        remaining: Box<std::sync::atomic::AtomicUsize>,
    }
    unsafe extern "C" fn progress(context: *mut std::ffi::c_void) -> std::ffi::c_int {
        // SAFETY: installed only while VmBudget owns the pointed-to AtomicUsize.
        let remaining = unsafe { &*context.cast::<std::sync::atomic::AtomicUsize>() };
        i32::from(
            remaining
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |n| n.checked_sub(1),
                )
                .is_err(),
        )
    }
    impl Drop for VmBudget {
        fn drop(&mut self) {
            // SAFETY: db outlives this guard; no statement is concurrently executing.
            unsafe {
                rusqlite::ffi::sqlite3_progress_handler(self.handle, 0, None, std::ptr::null_mut());
            }
        }
    }
    let mut vm = VmBudget {
        handle: unsafe { db.handle() },
        remaining: Box::new(std::sync::atomic::AtomicUsize::new(1000)),
    };
    // SAFETY: connection is private to this synchronous reader. Limits only affect it;
    // progress executes synchronously and never dereferences storage after guard drop.
    unsafe {
        rusqlite::ffi::sqlite3_limit(
            vm.handle,
            rusqlite::ffi::SQLITE_LIMIT_LENGTH,
            (ITEM_LIMIT + 1024) as i32,
        );
        rusqlite::ffi::sqlite3_limit(vm.handle, rusqlite::ffi::SQLITE_LIMIT_SQL_LENGTH, 16384);
        rusqlite::ffi::sqlite3_limit(vm.handle, rusqlite::ffi::SQLITE_LIMIT_COLUMN, 64);
        rusqlite::ffi::sqlite3_limit(vm.handle, rusqlite::ffi::SQLITE_LIMIT_EXPR_DEPTH, 32);
        rusqlite::ffi::sqlite3_limit(vm.handle, rusqlite::ffi::SQLITE_LIMIT_VDBE_OP, 10000);
        rusqlite::ffi::sqlite3_progress_handler(
            vm.handle,
            1000,
            Some(progress),
            std::ptr::from_mut(vm.remaining.as_mut()).cast(),
        );
    }
    // Do not enable or load SQLite extensions, change journal mode or issue checkpoints.
    db.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=MEMORY;")
        .map_err(|_| Diagnostic::Corrupt)?;
    let schema: String = db
        .query_row(
            "SELECT type FROM sqlite_schema WHERE name='ItemTable'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| Diagnostic::Corrupt)?;
    if schema != "table" {
        return Err(Diagnostic::Corrupt);
    }
    let mut statement = db
        .prepare("PRAGMA table_info(ItemTable)")
        .map_err(|_| Diagnostic::Corrupt)?;
    let columns: Vec<(String, String)> = statement
        .query_map([], |row| Ok((row.get(1)?, row.get(2)?)))
        .map_err(|_| Diagnostic::Corrupt)?
        .collect::<rusqlite::Result<_>>()
        .map_err(|_| Diagnostic::Corrupt)?;
    drop(statement);
    if !columns
        .iter()
        .any(|(name, kind)| name == "key" && kind.eq_ignore_ascii_case("TEXT"))
        || !columns
            .iter()
            .any(|(name, kind)| name == "value" && kind.eq_ignore_ascii_case("BLOB"))
    {
        return Err(Diagnostic::Corrupt);
    }
    let mut out = ReadReport {
        complete: true,
        ..ReadReport::default()
    };
    let mut bytes = 0;
    for key in KEYS {
        if cache(key) {
            out.diagnostics.push((Some(key), Diagnostic::CacheRebuilt));
            continue;
        }
        // Ask SQLite for length before materializing a potentially large cell.
        let mut q = db
            .prepare("SELECT typeof(value), length(value) FROM ItemTable WHERE key=?1 LIMIT 2")
            .map_err(|_| Diagnostic::Corrupt)?;
        let mut rows = q.query([key]).map_err(|_| Diagnostic::Corrupt)?;
        let Some(row) = rows.next().map_err(|_| Diagnostic::Corrupt)? else {
            continue;
        };
        let kind: String = row.get(0).map_err(|_| Diagnostic::Corrupt)?;
        let length: i64 = row.get(1).map_err(|_| Diagnostic::Corrupt)?;
        if rows.next().map_err(|_| Diagnostic::Corrupt)?.is_some() {
            return Err(Diagnostic::Corrupt);
        }
        if kind != "blob" {
            out.complete = false;
            out.diagnostics
                .push((Some(key), Diagnostic::UnsupportedEncoding));
            continue;
        }
        if length < 0 || length as usize > ITEM_LIMIT {
            out.complete = false;
            out.diagnostics.push((Some(key), Diagnostic::TooLarge));
            continue;
        }
        bytes += length as usize;
        if bytes > TOTAL_LIMIT {
            return Err(Diagnostic::TooLarge);
        }
        let mut q = db
            .prepare("SELECT value FROM ItemTable WHERE key=?1")
            .map_err(|_| Diagnostic::Corrupt)?;
        let value = q
            .query_row([key], |row| match row.get_ref(0)? {
                ValueRef::Blob(b) => Ok(b.to_vec()),
                _ => Err(rusqlite::Error::InvalidQuery),
            })
            .map_err(|_| Diagnostic::Corrupt)?;
        match utf16le(&value) {
            Ok(v) => {
                out.values.insert(key.into(), v);
            }
            Err(e) => {
                out.complete = false;
                out.diagnostics.push((Some(key), e));
            }
        }
    }
    drop(vm);
    drop(db);
    let after = std::fs::symlink_metadata(path).map_err(|_| Diagnostic::Changed)?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || sidecar("-wal").exists()
        || sidecar("-shm").exists()
        || sidecar("-journal").exists()
    {
        return Err(Diagnostic::Changed);
    }
    origin.verify(path, ORIGIN)?;
    Ok(out)
}

/// Modern WebKit ClientOrigin persistence format on the shipped little-endian targets.
/// Source: StorageUtilities.cpp + WebCorePersistentCoders.cpp + PersistentCoders.{h,cpp}.
/// Unknown/trailing data is unavailable, never guessed. This is not a general WebKit decoder.
pub fn verify_origin_bytes(mut bytes: &[u8]) -> Result<(), Diagnostic> {
    fn take<'a>(s: &mut &'a [u8], n: usize) -> Result<&'a [u8], Diagnostic> {
        if n > s.len() {
            return Err(Diagnostic::OriginUnverified);
        }
        let (a, b) = s.split_at(n);
        *s = b;
        Ok(a)
    }
    fn string(s: &mut &[u8]) -> Result<String, Diagnostic> {
        let len = u32::from_le_bytes(
            take(s, 4)?
                .try_into()
                .map_err(|_| Diagnostic::OriginUnverified)?,
        ) as usize;
        if len > 1024 {
            return Err(Diagnostic::OriginUnverified);
        }
        match take(s, 1)?[0] {
            1 => Ok(take(s, len)?.iter().map(|b| char::from(*b)).collect()),
            0 => utf16le(take(s, len * 2)?),
            _ => Err(Diagnostic::OriginUnverified),
        }
    }
    if bytes.len() > 16 * 1024 {
        return Err(Diagnostic::TooLarge);
    }
    for _ in 0..2 {
        let protocol = string(&mut bytes)?;
        let host = string(&mut bytes)?;
        // No explicit port on tauri://localhost. Do not accept a different client/top origin.
        if take(&mut bytes, 1)?[0] != 0 || protocol != "tauri" || host != "localhost" {
            return Err(Diagnostic::OriginUnverified);
        }
    }
    if !bytes.is_empty() {
        return Err(Diagnostic::UnsupportedEncoding);
    }
    Ok(())
}
/// The database candidate lives in <top hash>/<client hash>/LocalStorage/localstorage.sqlite3;
/// WebKit writes ClientOrigin into the sibling `origin` file at the client-hash level.
pub struct ModernOrigin;
impl OriginVerifier for ModernOrigin {
    fn verify(&self, path: &Path, exact_origin: &str) -> Result<(), Diagnostic> {
        if exact_origin != ORIGIN
            || path.file_name().and_then(|n| n.to_str()) != Some("localstorage.sqlite3")
        {
            return Err(Diagnostic::OriginUnverified);
        }
        let local = path.parent().ok_or(Diagnostic::OriginUnverified)?;
        if local.file_name().and_then(|n| n.to_str()) != Some("LocalStorage") {
            return Err(Diagnostic::OriginUnverified);
        }
        let directory =
            std::fs::symlink_metadata(local).map_err(|_| Diagnostic::OriginUnverified)?;
        if !directory.is_dir() || directory.file_type().is_symlink() {
            return Err(Diagnostic::OriginUnverified);
        }
        let origin = local
            .parent()
            .ok_or(Diagnostic::OriginUnverified)?
            .join("origin");
        let meta = std::fs::symlink_metadata(&origin).map_err(|_| Diagnostic::OriginUnverified)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16384 {
            return Err(Diagnostic::OriginUnverified);
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(&origin)
            .map_err(|_| Diagnostic::OriginUnverified)?
            .take(16385)
            .read_to_end(&mut bytes)
            .map_err(|_| Diagnostic::OriginUnverified)?;
        verify_origin_bytes(&bytes)
    }
}
/// Bounded discovery ONLY under an explicit WKWebsiteData/Default root. Returns one proven
/// candidate or an unavailable diagnostic. Never choose arbitrarily among multiple origins.
pub fn discover_modern(root: &Path) -> Result<std::path::PathBuf, Diagnostic> {
    fn dirs(path: &Path, count: &mut usize) -> Result<Vec<std::path::PathBuf>, Diagnostic> {
        let m = std::fs::symlink_metadata(path).map_err(|_| Diagnostic::Missing)?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(Diagnostic::Corrupt);
        }
        let mut paths = vec![];
        for e in std::fs::read_dir(path).map_err(|_| Diagnostic::Corrupt)? {
            *count += 1;
            if *count > 256 {
                return Err(Diagnostic::TooLarge);
            }
            let e = e.map_err(|_| Diagnostic::Corrupt)?;
            let kind = e.file_type().map_err(|_| Diagnostic::Corrupt)?;
            if kind.is_symlink() {
                return Err(Diagnostic::Corrupt);
            }
            if kind.is_dir() {
                paths.push(e.path());
            }
        }
        Ok(paths)
    }
    let mut count = 0;
    let mut candidates = vec![];
    for top in dirs(root, &mut count)? {
        for client in dirs(&top, &mut count)? {
            let db = client.join("LocalStorage/localstorage.sqlite3");
            if db.exists() && ModernOrigin.verify(&db, ORIGIN).is_ok() {
                candidates.push(db);
            }
        }
    }
    match candidates.len() {
        0 => Err(Diagnostic::OriginUnverified),
        1 => Ok(candidates.remove(0)),
        _ => Err(Diagnostic::AmbiguousOrigin),
    }
}
#[cfg(test)]
mod origin_tests {
    use super::*;
    fn text(s: &str) -> Vec<u8> {
        [
            (s.len() as u32).to_le_bytes().to_vec(),
            vec![1],
            s.as_bytes().to_vec(),
        ]
        .concat()
    }
    fn origin(protocol: &str, host: &str) -> Vec<u8> {
        [text(protocol), text(host), vec![0]].concat()
    }
    #[test]
    fn binary_client_origin_checks_top_and_client_no_substrings_or_extra_data() {
        let good = [origin("tauri", "localhost"), origin("tauri", "localhost")].concat();
        assert!(verify_origin_bytes(&good).is_ok());
        for bad in [
            &good[..good.len() - 1],
            &[good.clone(), vec![0]].concat(),
            &[origin("https", "localhost"), origin("tauri", "localhost")].concat(),
            &[
                origin("tauri", "localhost"),
                origin("tauri", "localhost.evil"),
            ]
            .concat(),
        ] {
            assert!(verify_origin_bytes(bad).is_err());
        }
    }
}

/// WAL recovery is confined to a bounded private snapshot; originals are never opened by SQLite.
pub fn read_consistent(path: &Path, origin: &dyn OriginVerifier) -> ReadReport {
    fn recover(path: &Path, origin: &dyn OriginVerifier) -> Result<ReadReport, Diagnostic> {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        origin.verify(path, ORIGIN)?;
        struct Private(std::path::PathBuf);
        impl Drop for Private {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let temp = std::env::temp_dir().join(format!(
            "tezbar-legacy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&temp).map_err(|_| Diagnostic::Corrupt)?;
        let private = Private(temp);
        fn capture(
            path: &Path,
            target: Option<&Path>,
            budget: &mut u64,
        ) -> Result<Option<Vec<u8>>, Diagnostic> {
            let m = match std::fs::symlink_metadata(path) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(Diagnostic::Corrupt),
            };
            if !m.is_file() || m.file_type().is_symlink() {
                return Err(Diagnostic::Corrupt);
            }
            *budget = budget
                .checked_add(m.len())
                .filter(|n| *n <= TOTAL_LIMIT as u64)
                .ok_or(Diagnostic::TooLarge)?;
            let mut input = std::fs::File::open(path).map_err(|_| Diagnostic::Corrupt)?;
            let mut output = target
                .map(|p| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(p)
                })
                .transpose()
                .map_err(|_| Diagnostic::Corrupt)?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buf = [0u8; 65536];
            loop {
                let n = input.read(&mut buf).map_err(|_| Diagnostic::Corrupt)?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                if bytes > m.len() {
                    return Err(Diagnostic::Changed);
                }
                hash.update(&buf[..n]);
                if let Some(out) = &mut output {
                    out.write_all(&buf[..n]).map_err(|_| Diagnostic::Corrupt)?;
                }
            }
            if bytes != m.len() {
                return Err(Diagnostic::Changed);
            }
            Ok(Some(hash.finalize().to_vec()))
        }
        let suffixes = ["", "-wal", "-shm", "-journal"];
        let original = |suffix: &str| {
            let mut p = path.as_os_str().to_os_string();
            p.push(suffix);
            std::path::PathBuf::from(p)
        };
        let copied = |suffix: &str| private.0.join(format!("localstorage.sqlite3{suffix}"));
        let mut budget = 0;
        let mut pins = vec![];
        for suffix in suffixes {
            pins.push(capture(
                &original(suffix),
                Some(&copied(suffix)),
                &mut budget,
            )?);
        }
        if pins[0].is_none() {
            return Err(Diagnostic::Missing);
        }
        let db = Connection::open_with_flags(
            copied(""),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| Diagnostic::Corrupt)?;
        db.busy_timeout(std::time::Duration::from_millis(50))
            .map_err(|_| Diagnostic::Corrupt)?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA wal_checkpoint(TRUNCATE);").map_err(|_|Diagnostic::Corrupt)?;
        db.close().map_err(|_| Diagnostic::Corrupt)?;
        // Only private, closed sidecars are removed after successful recovery/checkpoint.
        for suffix in &suffixes[1..] {
            let p = copied(suffix);
            if p.exists() {
                std::fs::remove_file(p).map_err(|_| Diagnostic::Corrupt)?;
            }
        }
        struct Attested;
        impl OriginVerifier for Attested {
            fn verify(&self, _: &Path, exact: &str) -> Result<(), Diagnostic> {
                if exact == ORIGIN {
                    Ok(())
                } else {
                    Err(Diagnostic::OriginUnverified)
                }
            }
        }
        let report = read_inner(&copied(""), &Attested)?;
        budget = 0;
        for (suffix, pin) in suffixes.into_iter().zip(pins) {
            if capture(&original(suffix), None, &mut budget)? != pin {
                return Err(Diagnostic::Changed);
            }
        }
        origin.verify(path, ORIGIN)?;
        Ok(report)
    }
    let report = read(path, origin);
    if report
        .diagnostics
        .iter()
        .any(|(_, d)| *d == Diagnostic::WalPresent)
    {
        recover(path, origin).unwrap_or_else(ReadReport::unavailable)
    } else {
        report
    }
}
