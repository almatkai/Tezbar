//! Disposable synthetic byte fixtures; NOT RUN in the M11 source lane.
use super::*;
use std::io::Write;
fn vint(mut n: u64) -> Vec<u8> {
    let mut v = vec![];
    while n >= 128 {
        v.push(n as u8 | 128);
        n >>= 7;
    }
    v.push(n as u8);
    v
}
fn string(v: &[u8]) -> Vec<u8> {
    [vint(v.len() as u64), v.to_vec()].concat()
}
fn record(value: &[u8], kind: u8) -> Vec<u8> {
    let crc = masked(crc(std::iter::once(kind).chain(value.iter().copied())));
    [
        crc.to_le_bytes().to_vec(),
        (value.len() as u16).to_le_bytes().to_vec(),
        vec![kind],
        value.to_vec(),
    ]
    .concat()
}
fn internal(key: &[u8], seq: u64, kind: u8) -> Vec<u8> {
    [
        key.to_vec(),
        ((seq << 8) | u64::from(kind)).to_le_bytes().to_vec(),
    ]
    .concat()
}
fn user(key: &str) -> Vec<u8> {
    [ORIGIN_PREFIX.to_vec(), vec![1], key.as_bytes().to_vec()].concat()
}
fn block(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![];
    for (k, v) in items {
        b.extend(vint(0));
        b.extend(vint(k.len() as u64));
        b.extend(vint(v.len() as u64));
        b.extend(k);
        b.extend(v);
    }
    b.extend(0u32.to_le_bytes());
    b.extend(1u32.to_le_bytes());
    b
}
fn physical(b: &[u8], compressed: bool) -> Vec<u8> {
    let bytes = if compressed {
        snap::raw::Encoder::new().compress_vec(b).unwrap()
    } else {
        b.to_vec()
    };
    let kind = u8::from(compressed);
    let crc = masked(crc(bytes.iter().copied().chain(Some(kind))));
    [bytes, vec![kind], crc.to_le_bytes().to_vec()].concat()
}
fn sst(items: &[(Vec<u8>, Vec<u8>)], compressed: bool) -> Vec<u8> {
    let data = physical(&block(items), compressed);
    let index_handle = [vint(0), vint((data.len() - 5) as u64)].concat();
    let index = physical(&block(&[(internal(b"z", 99, 1), index_handle)]), false);
    let mut footer = [
        vint(0),
        vint(0),
        vint(data.len() as u64),
        vint((index.len() - 5) as u64),
    ]
    .concat();
    footer.resize(40, 0);
    footer.extend(0xdb4775248b80fb57u64.to_le_bytes());
    [data, index, footer].concat()
}
fn manifest(table: Option<(u64, u64)>) -> Vec<u8> {
    let mut m = vec![1];
    m.extend(string(b"leveldb.BytewiseComparator"));
    m.extend([2, 3, 3, 5, 4, 10]); // log_number=3, next_file=5, last_sequence=10
    if let Some((id, size)) = table {
        m.push(7);
        m.extend(vint(0));
        m.extend(vint(id));
        m.extend(vint(size));
        m.extend(string(&internal(b"a", 1, 1)));
        m.extend(string(&internal(b"z", 10, 1)));
    }
    record(&m, 1)
}
fn batch(seq: u64, operations: &[(u8, Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut b = seq.to_le_bytes().to_vec();
    b.extend((operations.len() as u32).to_le_bytes());
    for (kind, key, value) in operations {
        b.push(*kind);
        b.extend(string(key));
        if *kind == 1 {
            b.extend(string(value));
        }
    }
    record(&b, 1)
}
fn seed(root: &Path, table: Option<&[u8]>, log_bytes: &[u8]) {
    std::fs::write(root.join("CURRENT"), b"MANIFEST-000001\n").unwrap();
    std::fs::write(
        root.join("MANIFEST-000001"),
        manifest(table.map(|t| (2, t.len() as u64))),
    )
    .unwrap();
    if let Some(t) = table {
        std::fs::write(root.join("000002.ldb"), t).unwrap();
    }
    std::fs::write(root.join("000003.log"), log_bytes).unwrap();
}
#[test]
fn current_manifest_live_tables_log_sequence_tombstones_and_immutability() {
    let d = tempfile::tempdir().unwrap();
    let key = user("tezbar:theme");
    let table = sst(&[(internal(&key, 2, 1), b"\x01dark".to_vec())], true);
    seed(
        d.path(),
        Some(&table),
        &batch(3, &[(0, key.clone(), vec![])]),
    );
    // Obsolete table/log must never resurrect the deleted theme.
    std::fs::write(
        d.path().join("000099.ldb"),
        sst(&[(internal(&key, 99, 1), b"\x01white".to_vec())], false),
    )
    .unwrap();
    std::fs::write(
        d.path().join("000001.log"),
        batch(100, &[(1, key, b"\x01white".to_vec())]),
    )
    .unwrap();
    let before = inventory(d.path()).unwrap();
    let bytes = std::fs::read(d.path().join("000003.log")).unwrap();
    let result = read(d.path());
    assert!(result.complete);
    assert!(!result.values.contains_key("tezbar:theme"));
    assert!(before == inventory(d.path()).unwrap());
    assert_eq!(bytes, std::fs::read(d.path().join("000003.log")).unwrap());
}
#[test]
fn newer_table_beats_older_log_and_other_origin_is_never_imported() {
    let d = tempfile::tempdir().unwrap();
    let key = user("tezbar:theme");
    seed(
        d.path(),
        Some(&sst(&[(internal(&key, 9, 1), b"\x01dark".to_vec())], false)),
        &batch(
            2,
            &[
                (1, key, b"\x01white".to_vec()),
                (
                    1,
                    b"_https://example.com\0\x01tezbar:theme".to_vec(),
                    b"\x01white".to_vec(),
                ),
            ],
        ),
    );
    assert_eq!(read(d.path()).values["tezbar:theme"], "dark");
}
#[test]
fn checksum_torn_log_missing_live_table_and_unknown_compression_fail_closed() {
    let d = tempfile::tempdir().unwrap();
    seed(
        d.path(),
        None,
        &batch(1, &[(1, user("tezbar:theme"), b"\x01dark".to_vec())]),
    );
    let p = d.path().join("000003.log");
    let mut bytes = std::fs::read(&p).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&p, &bytes).unwrap();
    assert!(!read(d.path()).complete);
    bytes.truncate(8);
    std::fs::write(&p, &bytes).unwrap();
    let recovered = read(d.path());
    assert!(recovered.complete);
    assert!(
        recovered
            .diagnostics
            .contains(&(None, Diagnostic::RecoveredLogTail))
    );
    seed(d.path(), Some(&sst(&[], false)), b"");
    std::fs::remove_file(d.path().join("000002.ldb")).unwrap();
    assert!(!read(d.path()).complete);
    let mut file = tempfile::tempfile().unwrap();
    let mut raw = b"bad".to_vec();
    raw.push(2);
    let crc = masked(crc(raw.iter().copied()));
    raw.extend(crc.to_le_bytes());
    file.write_all(&raw).unwrap();
    file.set_len(56).unwrap();
    assert!(matches!(
        table_block(&mut file, (0, 3), 56, &mut Budget::default()),
        Err(Diagnostic::UnsupportedEncoding)
    ));
}
#[test]
fn oversized_snappy_declaration_is_rejected_before_decompression() {
    let raw = vint((BLOCK + 1) as u64);
    let mut bytes = raw.clone();
    bytes.push(1);
    let crc = masked(crc(bytes.iter().copied()));
    bytes.extend(crc.to_le_bytes());
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let len = bytes.len() as u64 + 48;
    file.set_len(len).unwrap();
    assert!(matches!(
        table_block(&mut file, (0, raw.len()), len, &mut Budget::default()),
        Err(Diagnostic::TooLarge)
    ));
}
#[test]
fn encoding_tags_utf16_latin1_invalid_and_deleted_invalid_value() {
    assert_eq!(chromium_string(&[1, 0xe9]).unwrap(), "é");
    let bytes = [
        vec![0],
        "қазақша 😀"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect(),
    ]
    .concat();
    assert_eq!(chromium_string(&bytes).unwrap(), "қазақша 😀");
    for value in [vec![2], vec![0, 1], vec![0, 0, 0xd8]] {
        assert!(chromium_string(&value).is_err());
    }
    let mut values = Values::default();
    let key = user("tezbar:theme");
    values.insert(&key, Some(&[2]), 1).unwrap();
    values.insert(&key, None, 2).unwrap();
    assert!(values.report().unwrap().complete);
}
#[test]
fn manifest_deleted_tables_and_unknown_tags_are_not_salvaged() {
    let mut m = Manifest::default();
    m.tables.insert((0, 2), 100);
    m.edit(&[6, 0, 2]).unwrap();
    assert!(m.tables.is_empty());
    assert!(m.edit(&[100]).is_err());
}

#[test]
fn logical_key_encoding_variants_cannot_resurrect_a_newer_tombstone() {
    let mut values = Values::default();
    let latin = user("tezbar:theme");
    let utf16 = [
        ORIGIN_PREFIX.to_vec(),
        vec![0],
        "tezbar:theme"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect(),
    ]
    .concat();
    values.insert(&utf16, None, 9).unwrap();
    values.insert(&latin, Some(b"\x01dark"), 2).unwrap();
    assert!(!values.report().unwrap().values.contains_key("tezbar:theme"));
}

#[test]
fn review_m11_torn_newest_tail_preserves_committed_values_not_corruption() {
    let d = tempfile::tempdir().unwrap();
    seed(
        d.path(),
        None,
        &batch(1, &[(1, user("tezbar:theme"), b"\x01dark".to_vec())]),
    );
    let file = d.path().join("000003.log");
    let committed = std::fs::read(&file).unwrap();
    for tail in [vec![1, 2, 3], vec![0, 0, 0, 0, 50, 0, 1, 2]] {
        let mut bytes = committed.clone();
        bytes.extend(tail);
        std::fs::write(&file, &bytes).unwrap();
        let report = read(d.path());
        assert!(report.complete);
        assert_eq!(report.values["tezbar:theme"], "dark");
        assert!(
            report
                .diagnostics
                .contains(&(None, Diagnostic::RecoveredLogTail))
        );
        // The same truncated record is fatal if a newer live log exists.
        std::fs::write(d.path().join("000004.log"), b"").unwrap();
        assert!(!read(d.path()).complete);
        std::fs::remove_file(d.path().join("000004.log")).unwrap();
    }
    let mut invalid_block = committed;
    invalid_block.extend([0, 0, 0, 0, 255, 255, 1]);
    invalid_block.resize(32768, 0);
    std::fs::write(&file, invalid_block).unwrap();
    assert!(
        !read(d.path()).complete,
        "a record cannot cross a complete physical block; this is corruption, not EOF truncation"
    );
}

#[test]
fn review_m11_webview_unique_nested_profile_and_ambiguous_pair() {
    let d = tempfile::tempdir().unwrap();
    let nested = d.path().join("EBWebView/Default/Local Storage/leveldb");
    std::fs::create_dir_all(&nested).unwrap();
    seed(
        &nested,
        None,
        &batch(1, &[(1, user("tezbar:theme"), b"\x01dark".to_vec())]),
    );
    let source = Source::WebView2(d.path().into());
    let report = source.read();
    assert!(report.complete);
    assert_eq!(report.values["tezbar:theme"], "dark");
    std::fs::create_dir_all(d.path().join("Default/Local Storage/leveldb")).unwrap();
    assert!(
        source
            .read()
            .diagnostics
            .contains(&(None, Diagnostic::AmbiguousOrigin))
    );
}
#[test]
fn review_m11_value_surrogate_is_diagnosed_but_identity_stays_strict() {
    let raw = [0, 0xd8];
    assert!(utf16le(&raw).is_err());
    assert_eq!(super::super::utf16_value(&raw).unwrap(), ("�".into(), true));
}
