//! Exact bundled-FTS5 BM25 with a document-size blob supplied by the outer join.
//!
//! SQLite's built-in BM25 opens/steps/resets a nested docsize statement per hit.
//! Joining the same authoritative row in the search snapshot avoids that VM and
//! cursor lifecycle. There is no cache, schema change, extra index or stale state.
//! The formula and floating-point operation order are copied from SQLite 3.46.0
//! (public domain), including phrase-wide IDF, weights and the 1e-6 IDF floor.
//!
//! Shadow-table access is deliberately version-gated: SQLite describes the
//! encoding but discourages application access. An engine upgrade uses built-in
//! BM25 until its format/arithmetic and exact-order tests are requalified.
use rusqlite::{ffi, Connection};
use std::{ffi::c_void, ptr};

extern "C" {
    fn tezbar_bm25_sum(n: i32, idf: *const f64, freq: *const f64, length: f64, avgdl: f64) -> f64;
}

pub(super) fn supported() -> bool {
    rusqlite::version_number() == 3_046_000
}

pub(super) fn register(db: &Connection) -> Result<(), String> {
    if !supported() {
        return Ok(());
    }
    // SAFETY: the connection is exclusively used by its owning DB thread. The
    // pointer binding is synchronous; FTS5 writes only the local API pointer.
    unsafe {
        let mut stmt = ptr::null_mut();
        let mut api: *mut ffi::fts5_api = ptr::null_mut();
        let handle = db.handle();
        let mut rc = ffi::sqlite3_prepare_v2(
            handle,
            c"SELECT fts5(?1)".as_ptr(),
            -1,
            &mut stmt,
            ptr::null_mut(),
        );
        if rc == ffi::SQLITE_OK {
            rc = ffi::sqlite3_bind_pointer(
                stmt,
                1,
                (&mut api as *mut *mut ffi::fts5_api).cast(),
                c"fts5_api_ptr".as_ptr(),
                None,
            );
            if rc == ffi::SQLITE_OK {
                rc = ffi::sqlite3_step(stmt);
                if rc == ffi::SQLITE_ROW {
                    rc = ffi::SQLITE_OK;
                }
            }
        }
        ffi::sqlite3_finalize(stmt);
        if rc == ffi::SQLITE_OK && !api.is_null() {
            if let Some(create) = (*api).xCreateFunction {
                rc = create(
                    api,
                    c"tezbar_bm25".as_ptr(),
                    ptr::null_mut(),
                    Some(rank),
                    None,
                );
            } else {
                rc = ffi::SQLITE_MISUSE;
            }
        } else if rc == ffi::SQLITE_OK {
            rc = ffi::SQLITE_MISUSE;
        }
        if rc == ffi::SQLITE_OK {
            Ok(())
        } else {
            Err(format!("FTS5 rank registration failed: {rc}"))
        }
    }
}

#[repr(C)]
struct Data {
    phrases: usize,
    avgdl: f64,
    // Immediately followed by phrases IDF doubles, then phrases frequency doubles.
}

unsafe extern "C" fn count_phrase(
    _: *const ffi::Fts5ExtensionApi,
    _: *mut ffi::Fts5Context,
    count: *mut c_void,
) -> i32 {
    // SAFETY: xQueryPhrase synchronously invokes this with our live i64 counter.
    unsafe {
        *count.cast::<i64>() += 1;
    }
    ffi::SQLITE_OK
}

unsafe extern "C" fn rank(
    api: *const ffi::Fts5ExtensionApi,
    fts: *mut ffi::Fts5Context,
    ctx: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    // SAFETY: FTS5 owns all callback arguments until this invocation returns.
    // The implementation has no panicking indexing/allocation or Rust unwind
    // across the C boundary; errors go back through SQLite's result API.
    unsafe {
        match calculate(&*api, fts, argc, argv) {
            Ok(score) => ffi::sqlite3_result_double(ctx, score),
            Err(rc) => ffi::sqlite3_result_error_code(ctx, rc),
        }
    }
}

unsafe fn data(api: &ffi::Fts5ExtensionApi, fts: *mut ffi::Fts5Context) -> Result<*mut Data, i32> {
    unsafe {
        let p = (api.xGetAuxdata.ok_or(ffi::SQLITE_MISUSE)?)(fts, 0).cast::<Data>();
        if !p.is_null() {
            return Ok(p);
        }
        let n = (api.xPhraseCount.ok_or(ffi::SQLITE_MISUSE)?)(fts);
        if n < 0 {
            return Err(ffi::SQLITE_CORRUPT);
        }
        let size = (n as usize)
            .checked_mul(16)
            .and_then(|v| v.checked_add(size_of::<Data>()))
            .ok_or(ffi::SQLITE_TOOBIG)?;
        let p = ffi::sqlite3_malloc64(size as u64).cast::<Data>();
        if p.is_null() {
            return Err(ffi::SQLITE_NOMEM);
        }
        // SQLite allocates at least 8-byte aligned memory; Data/doubles use 8.
        ptr::write(
            p,
            Data {
                phrases: n as usize,
                avgdl: 0.,
            },
        );
        let init = (|| {
            let mut rows = 0;
            let mut tokens = 0;
            check((api.xRowCount.ok_or(ffi::SQLITE_MISUSE)?)(fts, &mut rows))?;
            check((api.xColumnTotalSize.ok_or(ffi::SQLITE_MISUSE)?)(
                fts,
                -1,
                &mut tokens,
            ))?;
            (*p).avgdl = tokens as f64 / rows as f64;
            let idf = p.add(1).cast::<f64>();
            for i in 0..n {
                let mut hits = 0i64;
                check((api.xQueryPhrase.ok_or(ffi::SQLITE_MISUSE)?)(
                    fts,
                    i,
                    (&mut hits as *mut i64).cast(),
                    Some(count_phrase),
                ))?;
                let mut value = (((rows - hits) as f64 + 0.5) / (hits as f64 + 0.5)).ln();
                if value <= 0. {
                    value = 1e-6;
                }
                *idf.add(i as usize) = value;
            }
            Ok(())
        })();
        if let Err(rc) = init {
            ffi::sqlite3_free(p.cast());
            return Err(rc);
        }
        let Some(set) = api.xSetAuxdata else {
            ffi::sqlite3_free(p.cast());
            return Err(ffi::SQLITE_MISUSE);
        };
        // FTS5 takes ownership and calls the destructor even if setting fails.
        check(set(fts, p.cast(), Some(ffi::sqlite3_free)))?;
        Ok(p)
    }
}

fn check(rc: i32) -> Result<(), i32> {
    if rc == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(rc)
    }
}

/// Decode only the documented, nonnegative int-sized docsize vector. Unknown,
/// malformed, overflowing or missing data falls back to FTS5 xColumnSize, which
/// preserves the engine's own corruption/error behavior. No unchecked blob reads.
fn document_size(blob: &[u8], columns: usize) -> Option<i32> {
    let mut off = 0;
    let mut total = 0i32;
    for _ in 0..columns {
        let mut value = 0u32;
        let mut complete = false;
        for _ in 0..5 {
            let byte = *blob.get(off)?;
            off += 1;
            value = value.checked_mul(128)?.checked_add((byte & 127) as u32)?;
            if byte & 128 == 0 {
                complete = true;
                break;
            }
        }
        if !complete {
            return None;
        }
        total = total.checked_add(i32::try_from(value).ok()?)?;
    }
    (off == blob.len()).then_some(total)
}

unsafe fn calculate(
    api: &ffi::Fts5ExtensionApi,
    fts: *mut ffi::Fts5Context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) -> Result<f64, i32> {
    unsafe {
        if argc < 1 {
            return Err(ffi::SQLITE_MISUSE);
        }
        let p = data(api, fts)?;
        let n = (*p).phrases;
        let idf = p.add(1).cast::<f64>();
        let freq = idf.add(n);
        ptr::write_bytes(freq, 0, n);
        let mut instances = 0;
        check((api.xInstCount.ok_or(ffi::SQLITE_MISUSE)?)(
            fts,
            &mut instances,
        ))?;
        for i in 0..instances {
            let (mut phrase, mut col, mut offset) = (0, 0, 0);
            check((api.xInst.ok_or(ffi::SQLITE_MISUSE)?)(
                fts,
                i,
                &mut phrase,
                &mut col,
                &mut offset,
            ))?;
            if phrase < 0 || phrase as usize >= n || col < 0 {
                return Err(ffi::SQLITE_CORRUPT);
            }
            let weight = if col < argc - 1 {
                ffi::sqlite3_value_double(*argv.add(col as usize + 1))
            } else {
                1.
            };
            *freq.add(phrase as usize) += weight;
        }
        let columns = (api.xColumnCount.ok_or(ffi::SQLITE_MISUSE)?)(fts);
        let bytes = ffi::sqlite3_value_bytes(*argv);
        let blob = ffi::sqlite3_value_blob(*argv).cast::<u8>();
        let supplied = if !blob.is_null() && bytes >= 0 && columns >= 0 {
            document_size(
                std::slice::from_raw_parts(blob, bytes as usize),
                columns as usize,
            )
        } else {
            None
        };
        let total = if let Some(total) = supplied {
            total
        } else {
            let mut total = 0;
            check((api.xColumnSize.ok_or(ffi::SQLITE_MISUSE)?)(
                fts, -1, &mut total,
            ))?;
            total
        };
        // Same C expression/compiler defaults as bundled SQLite, preserving
        // its contracted scalar and vector-prefix arithmetic bit for bit.
        Ok(tezbar_bm25_sum(
            n as i32,
            idf,
            freq,
            total as f64,
            (*p).avgdl,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scores_match_bundled_bm25_bits_across_phrase_counts_and_lengths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fts.sqlite3");
        let mut db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE VIRTUAL TABLE entries USING fts5(title,subtitle,tokens,tokenize='unicode61');",
        )
        .unwrap();
        let tx = db.transaction().unwrap();
        for n in 0..240 {
            let terms = (0..20)
                .filter(|i| (n * 17 + i * 7) % 13 < 4)
                .map(|i| format!("term{i}"))
                .collect::<Vec<_>>()
                .join(" ");
            let title = if n % 3 == 0 {
                format!("archive café {terms}")
            } else {
                format!("other {terms}")
            };
            let subtitle = format!("東京 e\u{301} emoji 😀 {}", "archive ".repeat(n % 11));
            let tokens = format!(
                "{terms} {}",
                "common ".repeat(if n == 0 { 17_000 } else { n % 137 })
            );
            tx.execute(
                "INSERT INTO entries VALUES(?,?,?)",
                rusqlite::params![title, subtitle, tokens],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let queries = [
            "archive*",
            "archive* OR common*",
            "cafe*",
            "\"archive café\"",
            "東京",
            "NEAR(archive cafe, 4)",
            "archive NOT common",
            "absent*",
            "title:archive",
            "{title subtitle}:archive",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain((1..=20).map(|n| {
            (0..n)
                .map(|i| format!("term{i}*"))
                .collect::<Vec<_>>()
                .join(" OR ")
        }))
        .collect::<Vec<_>>();
        for phase in 0..4 {
            if phase == 1 {
                db.execute_batch("BEGIN; UPDATE entries SET title='archive term0 term1 term2 term3', tokens='common' WHERE rowid%7=0; DELETE FROM entries WHERE rowid%11=0; COMMIT;").unwrap();
            }
            if phase == 2 {
                db.execute_batch(
                    "BEGIN; UPDATE entries SET title='rollback archive' WHERE rowid%3=0; ROLLBACK;",
                )
                .unwrap();
            }
            if phase == 3 {
                drop(db);
                db = Connection::open(&path).unwrap();
            }
            register(&db).unwrap();
            for weights in ["", ",5.0,2.0,1.0", ",0.0,1.0,3.0", ",1.0"] {
                let sql = format!("SELECT f.rowid,bm25(entries{weights}),tezbar_bm25(entries,sz.sz{weights}),tezbar_bm25(entries,NULL{weights}),tezbar_bm25(entries,X'80'{weights}) FROM entries f LEFT JOIN entries_docsize sz ON sz.id=f.rowid WHERE entries MATCH ?");
                let mut stmt = db.prepare(&sql).unwrap();
                for query in &queries {
                    let mut rows = stmt.query([query]).unwrap();
                    while let Some(row) = rows.next().unwrap() {
                        let expected = row.get::<_, f64>(1).unwrap().to_bits();
                        for column in 2..5 {
                            assert_eq!(row.get::<_,f64>(column).unwrap().to_bits(),expected,"phase={phase} query={query} weights={weights} column={column} row={}",row.get::<_,i64>(0).unwrap());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn size_decoder_is_bounded_and_falls_back_on_invalid_input() {
        assert_eq!(document_size(&[1, 2, 3], 3), Some(6));
        assert_eq!(document_size(&[0x81, 0, 2, 0], 3), Some(130));
        for blob in [
            &[][..],
            &[0x80],
            &[1, 2],
            &[1, 2, 3, 4],
            &[0xff, 0xff, 0xff, 0xff, 0xff, 2, 3],
        ] {
            assert_eq!(document_size(blob, 3), None, "{blob:?}");
        }
    }
}
