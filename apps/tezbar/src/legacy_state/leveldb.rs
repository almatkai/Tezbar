//! Read-only LevelDB 1.x physical format. No database engine, LOCK, recovery or compaction.
//! Caller must hold the old app quiescent. CURRENT/manifest select live tables; highest
//! sequence wins, including deletion records. Unknown formats stop the entire import.
use super::*;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
const BLOCK: usize = 4 * 1024 * 1024;
const RECORDS: usize = 100_000;
const ORIGIN_PREFIX: &[u8] = b"_http://tauri.localhost\0";
type Result<T> = std::result::Result<T, Diagnostic>;
#[derive(Default)]
struct Budget {
    bytes: usize,
    records: usize,
}
impl Budget {
    fn bytes(&mut self, n: usize) -> Result<()> {
        self.bytes = self.bytes.checked_add(n).ok_or(Diagnostic::TooLarge)?;
        if self.bytes > TOTAL_LIMIT {
            Err(Diagnostic::TooLarge)
        } else {
            Ok(())
        }
    }
    fn record(&mut self) -> Result<()> {
        self.records += 1;
        if self.records > RECORDS {
            Err(Diagnostic::TooLarge)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}
fn stamp(path: &Path) -> Result<Stamp> {
    let m = std::fs::symlink_metadata(path).map_err(|_| Diagnostic::Missing)?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err(Diagnostic::Corrupt);
    }
    Ok(Stamp {
        len: m.len(),
        modified: m.modified().ok(),
    })
}
fn inventory(root: &Path) -> Result<BTreeMap<String, Stamp>> {
    if std::fs::symlink_metadata(root)
        .map_err(|_| Diagnostic::Missing)?
        .file_type()
        .is_symlink()
    {
        return Err(Diagnostic::Corrupt);
    }
    let mut result = BTreeMap::new();
    for (i, e) in std::fs::read_dir(root)
        .map_err(|_| Diagnostic::Missing)?
        .enumerate()
    {
        if i >= 256 {
            return Err(Diagnostic::TooLarge);
        }
        let e = e.map_err(|_| Diagnostic::Corrupt)?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| Diagnostic::UnsupportedEncoding)?;
        if name == "CURRENT"
            || name.starts_with("MANIFEST-")
            || name.ends_with(".log")
            || name.ends_with(".ldb")
            || name.ends_with(".sst")
        {
            result.insert(name, stamp(&e.path())?);
        }
    }
    Ok(result)
}
fn read_at(f: &mut File, offset: u64, n: usize, budget: &mut Budget) -> Result<Vec<u8>> {
    if n > BLOCK {
        return Err(Diagnostic::TooLarge);
    }
    budget.bytes(n)?;
    f.seek(SeekFrom::Start(offset))
        .map_err(|_| Diagnostic::Corrupt)?;
    let mut bytes = vec![0; n];
    f.read_exact(&mut bytes).map_err(|_| Diagnostic::Corrupt)?;
    Ok(bytes)
}
fn open(path: &Path) -> Result<File> {
    File::open(path).map_err(|_| Diagnostic::Missing)
}
fn u32le(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| Diagnostic::Corrupt)?,
    ))
}
fn u64le(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| Diagnostic::Corrupt)?,
    ))
}
fn take<'a>(s: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if n > s.len() {
        return Err(Diagnostic::Corrupt);
    }
    let (a, b) = s.split_at(n);
    *s = b;
    Ok(a)
}
fn var(s: &mut &[u8]) -> Result<u64> {
    let mut n = 0;
    for shift in (0..70).step_by(7) {
        let b = take(s, 1)?[0];
        if shift == 63 && b > 1 {
            return Err(Diagnostic::Corrupt);
        }
        n |= u64::from(b & 127) << shift;
        if b < 128 {
            return Ok(n);
        }
    }
    Err(Diagnostic::Corrupt)
}
fn slice<'a>(s: &mut &'a [u8]) -> Result<&'a [u8]> {
    let n = usize::try_from(var(s)?).map_err(|_| Diagnostic::TooLarge)?;
    if n > BLOCK {
        return Err(Diagnostic::TooLarge);
    }
    take(s, n)
}
/// Castagnoli CRC; LevelDB stores Mask(CRC32C(type || payload)).
fn crc(bytes: impl IntoIterator<Item = u8>) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0x82f63b78 } else { 0 };
        }
    }
    !c
}
fn masked(c: u32) -> u32 {
    c.rotate_right(15).wrapping_add(0xa282ead8)
}
fn checksum(expected: u32, bytes: impl IntoIterator<Item = u8>) -> Result<()> {
    if masked(crc(bytes)) != expected {
        Err(Diagnostic::Corrupt)
    } else {
        Ok(())
    }
}
/// Physical log block size 32768; fragmented logical records are capped before append.
fn log(path: &Path, budget: &mut Budget, mut visit: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
    let mut f = open(path)?;
    let len = f.metadata().map_err(|_| Diagnostic::Corrupt)?.len();
    if len > TOTAL_LIMIT as u64 {
        return Err(Diagnostic::TooLarge);
    }
    let mut offset = 0;
    let mut pending: Option<Vec<u8>> = None;
    while offset < len {
        let data = read_at(&mut f, offset, ((len - offset).min(32768)) as usize, budget)?;
        offset += data.len() as u64;
        let mut s = data.as_slice();
        while s.len() >= 7 {
            if s.iter().all(|b| *b == 0) {
                break;
            }
            let h = take(&mut s, 7)?;
            let size = usize::from(u16::from_le_bytes([h[4], h[5]]));
            let kind = h[6];
            let payload = take(&mut s, size)?;
            checksum(
                u32le(&h[..4])?,
                std::iter::once(kind).chain(payload.iter().copied()),
            )?;
            match kind {
                1 if pending.is_none() => {
                    budget.record()?;
                    visit(payload)?;
                }
                2 if pending.is_none() => pending = Some(payload.to_vec()),
                3 | 4 if pending.is_some() => {
                    let p = pending.as_mut().unwrap();
                    if p.len() + payload.len() > BLOCK {
                        return Err(Diagnostic::TooLarge);
                    }
                    p.extend_from_slice(payload);
                    if kind == 4 {
                        budget.record()?;
                        visit(&pending.take().unwrap())?;
                    }
                }
                _ => return Err(Diagnostic::Corrupt),
            }
        }
        if s.len() < 7 && s.iter().any(|b| *b != 0) {
            return Err(Diagnostic::Corrupt);
        }
    }
    if pending.is_some() {
        return Err(Diagnostic::Corrupt);
    }
    Ok(())
}
#[derive(Default)]
struct Manifest {
    log: u64,
    prev: u64,
    tables: BTreeMap<(u64, u64), u64>,
    comparator: bool,
    has_log: bool,
}
impl Manifest {
    fn edit(&mut self, mut s: &[u8]) -> Result<()> {
        // Apply deleted files before added files, as VersionSet::Builder does.
        let mut deleted = vec![];
        let mut added = vec![];
        while !s.is_empty() {
            match var(&mut s)? {
                1 => {
                    if slice(&mut s)? != b"leveldb.BytewiseComparator" {
                        return Err(Diagnostic::UnsupportedEncoding);
                    }
                    self.comparator = true;
                }
                2 => {
                    self.log = var(&mut s)?;
                    self.has_log = true;
                }
                3 | 4 => {
                    var(&mut s)?;
                }
                9 => self.prev = var(&mut s)?,
                5 => {
                    let level = var(&mut s)?;
                    if level > 6 {
                        return Err(Diagnostic::Corrupt);
                    }
                    slice(&mut s)?;
                }
                6 => {
                    let l = var(&mut s)?;
                    let n = var(&mut s)?;
                    if l > 6 {
                        return Err(Diagnostic::Corrupt);
                    }
                    deleted.push((l, n));
                }
                7 => {
                    let l = var(&mut s)?;
                    let n = var(&mut s)?;
                    let size = var(&mut s)?;
                    if l > 6 {
                        return Err(Diagnostic::Corrupt);
                    }
                    if slice(&mut s)?.len() < 8 || slice(&mut s)?.len() < 8 {
                        return Err(Diagnostic::Corrupt);
                    }
                    added.push(((l, n), size));
                }
                _ => return Err(Diagnostic::UnsupportedEncoding),
            }
        }
        for k in deleted {
            self.tables.remove(&k);
        }
        for (k, size) in added {
            self.tables.insert(k, size);
        }
        if self.tables.len() > 256 {
            return Err(Diagnostic::TooLarge);
        }
        Ok(())
    }
}
fn handle(s: &mut &[u8]) -> Result<(u64, usize)> {
    let off = var(s)?;
    let n = usize::try_from(var(s)?).map_err(|_| Diagnostic::TooLarge)?;
    if n > BLOCK {
        return Err(Diagnostic::TooLarge);
    }
    Ok((off, n))
}
fn table_block(f: &mut File, h: (u64, usize), len: u64, b: &mut Budget) -> Result<Vec<u8>> {
    if h.0
        .checked_add(h.1 as u64 + 5)
        .is_none_or(|end| end > len.saturating_sub(48))
    {
        return Err(Diagnostic::Corrupt);
    }
    let bytes = read_at(f, h.0, h.1.checked_add(5).ok_or(Diagnostic::TooLarge)?, b)?;
    let (raw, trailer) = bytes.split_at(h.1);
    let compression = trailer[0];
    checksum(
        u32le(&trailer[1..])?,
        raw.iter().copied().chain(Some(compression)),
    )?;
    match compression {
        0 => Ok(raw.to_vec()),
        1 => {
            // Mandatory: inspect declared length BEFORE allocation/decompression.
            let n = snap::raw::decompress_len(raw).map_err(|_| Diagnostic::Corrupt)?;
            if n > BLOCK {
                return Err(Diagnostic::TooLarge);
            }
            b.bytes(n)?;
            let mut out = vec![0; n];
            let written = snap::raw::Decoder::new()
                .decompress(raw, &mut out)
                .map_err(|_| Diagnostic::Corrupt)?;
            if written != n {
                return Err(Diagnostic::Corrupt);
            }
            Ok(out)
        }
        _ => Err(Diagnostic::UnsupportedEncoding),
    }
}
fn entries(
    data: &[u8],
    b: &mut Budget,
    mut visit: impl FnMut(&[u8], &[u8]) -> Result<()>,
) -> Result<()> {
    if data.len() < 8 {
        return Err(Diagnostic::Corrupt);
    }
    let n = u32le(&data[data.len() - 4..])? as usize;
    let restart_bytes = n
        .checked_mul(4)
        .and_then(|n| n.checked_add(4))
        .ok_or(Diagnostic::TooLarge)?;
    if n == 0 || restart_bytes > data.len() {
        return Err(Diagnostic::Corrupt);
    }
    let end = data.len() - restart_bytes;
    let restarts = &data[end..data.len() - 4];
    let mut previous_restart = None;
    for r in restarts.chunks_exact(4) {
        let off = u32le(r)? as usize;
        if off > end || previous_restart.is_some_and(|p| off <= p) {
            return Err(Diagnostic::Corrupt);
        }
        previous_restart = Some(off);
    }
    if u32le(&restarts[..4])? != 0 {
        return Err(Diagnostic::Corrupt);
    }
    let mut s = &data[..end];
    let mut key = Vec::new();
    let mut restart_index = 0;
    while !s.is_empty() {
        b.record()?;
        let offset = end - s.len();
        let shared = var(&mut s)? as usize;
        let new = var(&mut s)? as usize;
        let value = var(&mut s)? as usize;
        if shared > key.len()
            || shared.checked_add(new).is_none_or(|n| n > ITEM_LIMIT)
            || value > BLOCK
        {
            return Err(Diagnostic::TooLarge);
        }
        if restart_index < n {
            let r = u32le(&restarts[restart_index * 4..restart_index * 4 + 4])? as usize;
            if r < offset {
                return Err(Diagnostic::Corrupt);
            }
            if r == offset {
                if shared != 0 {
                    return Err(Diagnostic::Corrupt);
                }
                restart_index += 1;
            }
        }
        key.truncate(shared);
        key.extend_from_slice(take(&mut s, new)?);
        visit(&key, take(&mut s, value)?)?;
    }
    if restart_index != n && !(end == 0 && n == 1) {
        return Err(Diagnostic::Corrupt);
    }
    Ok(())
}
#[derive(Default)]
struct Values(BTreeMap<String, (u64, Option<Vec<u8>>)>, usize);
impl Values {
    fn insert(&mut self, key: &[u8], value: Option<&[u8]>, seq: u64) -> Result<()> {
        if seq > ((1u64 << 56) - 1) {
            return Err(Diagnostic::Corrupt);
        }
        // Only retain known keys + database format version, never arbitrary credential keys.
        let canonical = if key == b"VERSION" {
            "VERSION".to_string()
        } else {
            let Some(encoded) = key.strip_prefix(ORIGIN_PREFIX) else {
                return Ok(());
            };
            let decoded = chromium_string(encoded)?;
            if known_key(&decoded).is_none() || cache(&decoded) {
                return Ok(());
            }
            decoded
        };
        if value.is_some_and(|v| v.len() > ITEM_LIMIT) {
            return Err(Diagnostic::TooLarge);
        }
        if let Some((old, previous)) = self.0.get(&canonical) {
            if *old > seq {
                return Ok(());
            }
            if *old == seq && previous.as_deref() != value {
                return Err(Diagnostic::Corrupt);
            }
        }
        self.0.insert(canonical, (seq, value.map(Vec::from)));
        Ok(())
    }
    fn batch(&mut self, mut s: &[u8]) -> Result<()> {
        let seq = u64le(take(&mut s, 8)?)?;
        let count = u32le(take(&mut s, 4)?)?;
        self.1 = self
            .1
            .checked_add(count as usize)
            .ok_or(Diagnostic::TooLarge)?;
        if self.1 > RECORDS {
            return Err(Diagnostic::TooLarge);
        }
        for i in 0..count {
            let kind = take(&mut s, 1)?[0];
            let key = slice(&mut s)?;
            let value = match kind {
                0 => None,
                1 => Some(slice(&mut s)?),
                _ => return Err(Diagnostic::UnsupportedEncoding),
            };
            self.insert(
                key,
                value,
                seq.checked_add(u64::from(i)).ok_or(Diagnostic::Corrupt)?,
            )?;
        }
        if !s.is_empty() {
            return Err(Diagnostic::Corrupt);
        }
        Ok(())
    }
    fn report(self) -> Result<ReadReport> {
        if self
            .0
            .get("VERSION")
            .is_some_and(|(_, v)| v.as_deref() != Some(b"1"))
        {
            return Err(Diagnostic::UnsupportedEncoding);
        }
        let mut out = ReadReport {
            complete: true,
            ..ReadReport::default()
        };
        for (key, (_, value)) in self.0 {
            let Some(key) = known_key(&key) else {
                continue;
            };
            if cache(key) {
                out.diagnostics.push((Some(key), Diagnostic::CacheRebuilt));
                continue;
            }
            let Some(raw) = value else { continue }; // Tombstones never fall back to older values.
            match chromium_string(&raw) {
                Ok(v) => {
                    out.values.insert(key.into(), v);
                }
                Err(e) => {
                    out.complete = false;
                    out.diagnostics.push((Some(key), e));
                }
            }
        }
        Ok(out)
    }
}
fn table(path: &Path, expected: u64, b: &mut Budget, values: &mut Values) -> Result<()> {
    let mut f = open(path)?;
    let len = f.metadata().map_err(|_| Diagnostic::Corrupt)?.len();
    if len != expected || len < 48 || len > TOTAL_LIMIT as u64 {
        return Err(Diagnostic::Corrupt);
    }
    let footer = read_at(&mut f, len - 48, 48, b)?;
    if u64le(&footer[40..])? != 0xdb4775248b80fb57 {
        return Err(Diagnostic::UnsupportedEncoding);
    }
    let mut s = &footer[..40];
    handle(&mut s)?;
    let index = handle(&mut s)?;
    if s.iter().any(|b| *b != 0) {
        return Err(Diagnostic::Corrupt);
    }
    let index = table_block(&mut f, index, len, b)?;
    let mut blocks = Vec::new();
    entries(&index, b, |_, mut value| {
        blocks.push(handle(&mut value)?);
        if !value.is_empty() {
            return Err(Diagnostic::Corrupt);
        }
        Ok(())
    })?;
    let mut end = 0;
    for h in blocks {
        if h.0 < end {
            return Err(Diagnostic::Corrupt);
        }
        end = h.0 + h.1 as u64 + 5;
        let bytes = table_block(&mut f, h, len, b)?;
        entries(&bytes, b, |key, value| {
            if key.len() < 8 {
                return Err(Diagnostic::Corrupt);
            }
            let (user, trailer) = key.split_at(key.len() - 8);
            let tag = u64le(trailer)?;
            match tag & 255 {
                0 => values.insert(user, None, tag >> 8),
                1 => values.insert(user, Some(value), tag >> 8),
                _ => Err(Diagnostic::UnsupportedEncoding),
            }
        })?;
    }
    Ok(())
}
pub fn read(root: &Path) -> ReadReport {
    match read_inner(root) {
        Ok(r) => r,
        Err(e) => ReadReport::unavailable(e),
    }
}
fn read_inner(root: &Path) -> Result<ReadReport> {
    let before = inventory(root)?;
    let mut budget = Budget::default();
    let mut current = open(&root.join("CURRENT"))?;
    let n = current.metadata().map_err(|_| Diagnostic::Corrupt)?.len();
    if n > 128 {
        return Err(Diagnostic::TooLarge);
    }
    let bytes = read_at(&mut current, 0, n as usize, &mut budget)?;
    let name = std::str::from_utf8(&bytes)
        .map_err(|_| Diagnostic::Corrupt)?
        .strip_suffix('\n')
        .ok_or(Diagnostic::Corrupt)?;
    if !name
        .strip_prefix("MANIFEST-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(Diagnostic::Corrupt);
    }
    let mut manifest = Manifest::default();
    log(&root.join(name), &mut budget, |r| manifest.edit(r))?;
    if !manifest.comparator || !manifest.has_log {
        return Err(Diagnostic::UnsupportedEncoding);
    }
    if manifest.log > 0 && !before.contains_key(&format!("{:06}.log", manifest.log)) {
        return Err(Diagnostic::Missing);
    }
    let mut values = Values::default();
    for ((_, number), size) in &manifest.tables {
        let ldb = format!("{number:06}.ldb");
        let sst = format!("{number:06}.sst");
        let file = match (before.contains_key(&ldb), before.contains_key(&sst)) {
            (true, false) => ldb,
            (false, true) => sst,
            _ => return Err(Diagnostic::Corrupt),
        };
        table(&root.join(file), *size, &mut budget, &mut values)?;
    }
    // LevelDB DBImpl::Recover: logs >= manifest log_number OR == prev_log_number.
    // Obsolete logs/tables are never scanned for salvage.
    let mut logs: Vec<(u64, PathBuf)> = before
        .keys()
        .filter_map(|name| {
            name.strip_suffix(".log")
                .and_then(|n| n.parse::<u64>().ok())
                .map(|n| (n, root.join(name)))
        })
        .filter(|(n, _)| *n >= manifest.log || *n == manifest.prev)
        .collect();
    logs.sort_by_key(|(n, _)| *n);
    for (_, path) in logs {
        log(&path, &mut budget, |r| values.batch(r))?;
    }
    if before != inventory(root)? {
        return Err(Diagnostic::Changed);
    }
    values.report()
}
#[cfg(test)]
#[path = "leveldb_tests.rs"]
mod tests;
