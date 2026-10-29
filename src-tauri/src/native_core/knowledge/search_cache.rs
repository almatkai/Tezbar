//! Exact, disposable source-search acceleration; never an authority or an ANN index.
//! One model/dimension/root intersection at a time. All construction/reconciliation
//! is bounded worker work; an incomplete snapshot ALWAYS uses the streaming lane.
use super::{err, text, Store};
use rusqlite::{params, Connection};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const BATCH: usize = 1024;
const _: () = assert!(BATCH % 4 == 0); // Packed dot-product groups cannot cross slabs.
const IDLE: Duration = Duration::from_secs(120);
const ROW_ALLOWANCE: usize = 176;

#[derive(Clone)]
pub(crate) struct SearchToken {
    generation: Arc<AtomicU64>,
    expected: u64,
    alive: Arc<AtomicBool>,
    receiver: Option<Arc<AtomicBool>>,
}
pub(crate) struct SearchGuard(pub SearchToken);
impl SearchGuard {
    pub fn new(generation: Arc<AtomicU64>) -> Self {
        let expected = generation.fetch_add(1, Ordering::SeqCst) + 1;
        Self(SearchToken {
            generation,
            expected,
            alive: Arc::new(AtomicBool::new(true)),
            receiver: None,
        })
    }
}
impl Drop for SearchGuard {
    fn drop(&mut self) {
        self.0.alive.store(false, Ordering::SeqCst);
        let _ = self.0.generation.compare_exchange(
            self.0.expected,
            self.0.expected.wrapping_add(1),
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}
pub(crate) struct ReceiverGuard(pub Arc<AtomicBool>);
impl Drop for ReceiverGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl SearchToken {
    /// Background read tied to the existing semantic selection/lifecycle epoch.
    /// Does not supersede searches or consume/increment their generations.
    pub fn background(cancel: super::super::extract::Cancellation) -> Self {
        Self {
            generation: cancel.generation,
            expected: cancel.expected,
            alive: Arc::new(AtomicBool::new(true)),
            receiver: None,
        }
    }
    pub fn with_receiver(mut self, alive: Arc<AtomicBool>) -> Self {
        self.receiver = Some(alive);
        self
    }
    fn current(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
            && self
                .receiver
                .as_ref()
                .map(|v| v.load(Ordering::SeqCst))
                .unwrap_or(true)
            && self.generation.load(Ordering::SeqCst) == self.expected
    }
    pub fn check(&self) -> Result<(), String> {
        if self.current() {
            Ok(())
        } else {
            Err("Superseded knowledge search".into())
        }
    }
    pub fn cancellation(&self) -> super::super::extract::Cancellation {
        super::super::extract::Cancellation {
            generation: self.generation.clone(),
            expected: self.expected,
        }
    }
}

/// Worker-local SQLite VM cancellation. Unlike connection-wide interrupt(), a
/// progress callback cannot fire after this read-only search job leaves scope.
/// It is uninstalled before any prewarm batch or indexing/mutation job executes.
/// Native vec0 computation can still run until it next yields to SQLite's VM.
pub(crate) struct SqlSearchGuard {
    connection: *mut rusqlite::ffi::sqlite3,
    _token: Box<SearchToken>,
}
impl SqlSearchGuard {
    pub fn new(connection: &Connection, token: SearchToken) -> Self {
        let token = Box::new(token);
        // SAFETY: SQLite is confined to this worker. The boxed token's address
        // is stable until Drop uninstalls the callback, before dropping it. The
        // callback only reads atomics, never calls SQLite or unwinds through C.
        let db = unsafe { connection.handle() };
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(
                db,
                1000,
                Some(search_progress),
                (&*token as *const SearchToken).cast_mut().cast(),
            );
        }
        Self {
            connection: db,
            _token: token,
        }
    }
}
unsafe extern "C" fn search_progress(context: *mut std::ffi::c_void) -> std::ffi::c_int {
    let token = unsafe { &*context.cast::<SearchToken>() };
    i32::from(!token.current())
}
impl Drop for SqlSearchGuard {
    fn drop(&mut self) {
        // SAFETY: the same worker still owns the live Connection; no other job
        // can execute until this guard has removed its callback/context.
        unsafe {
            rusqlite::ffi::sqlite3_progress_handler(self.connection, 0, None, std::ptr::null_mut());
        }
    }
}

#[derive(Default)]
pub(crate) struct Control {
    pub paused: AtomicBool,
    pub stopped: AtomicBool,
}
impl Control {
    fn check(&self) -> Result<(), String> {
        if self.paused.load(Ordering::Relaxed) || self.stopped.load(Ordering::Relaxed) {
            Err("Search prewarming suspended".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub model: Option<String>,
    pub dimensions: usize,
    // SQL literals generated only from canonical, escaped root IDs by Store.
    pub semantic_roots: String,
    pub requested_roots: String,
}
impl Key {
    pub fn embedding(&self) -> &'static str {
        if self.model.is_some() {
            "g.vector"
        } else {
            "c.embedding_json"
        }
    }
    pub fn gemma_join(&self) -> &'static str {
        if self.model.is_some() {
            "CROSS JOIN knowledge_gemma_embeddings g ON g.chunk_id=c.id"
        } else {
            ""
        }
    }
    pub fn eligible(&self) -> String {
        let model = self
            .model
            .as_ref()
            .map(|m| format!(" AND g.model_key='{}'", m.replace('\'', "''")))
            .unwrap_or_default();
        format!("FROM knowledge_chunks c CROSS JOIN knowledge_sources s ON s.id=c.source_id CROSS JOIN knowledge_roots r ON r.id=s.root_id {} WHERE s.status='indexed' AND r.enabled=1 AND r.id IN ({}) AND r.id IN ({}) AND {} IS NOT NULL{}", self.gemma_join(), self.requested_roots, self.semantic_roots, self.embedding(), model)
    }
    fn select(&self) -> String {
        format!(
            "SELECT c.rowid,c.id,c.source_id,{} {}",
            self.embedding(),
            self.eligible()
        )
    }
}
struct Row {
    rowid: i64,
    id: String,
    source: usize,
    norm: f64,
    // Non-F32 JSON and unusual lengths retain the original F64/zip semantics.
    legacy: Option<Vec<f64>>,
}
struct Packed {
    rows: Vec<Row>,
    // Fixed-size dense slabs avoid reallocating/copying a GB-sized buffer as
    // indexing grows. Each slab remains contiguous for exact bounded scoring.
    values: Vec<Vec<f32>>,
    dense_bytes: usize,
    positions: HashMap<i64, usize>,
    sources: Vec<String>,
    source_ids: HashMap<String, usize>,
    string_bytes: usize,
    legacy_bytes: usize,
    dimensions: usize,
    allowance: usize,
}
impl Packed {
    fn new(dimensions: usize, allowance: usize) -> Self {
        Self {
            rows: vec![],
            values: vec![],
            dense_bytes: 0,
            positions: HashMap::new(),
            sources: vec![],
            source_ids: HashMap::new(),
            string_bytes: 0,
            legacy_bytes: 0,
            dimensions,
            allowance,
        }
    }
    fn estimate(rows: usize, dimensions: usize) -> usize {
        rows.saturating_mul(dimensions.saturating_mul(4).saturating_add(ROW_ALLOWANCE))
    }
    fn allocated(&self) -> usize {
        // HashMap exposes usable capacity, not bucket bytes. A factor of two
        // deliberately overestimates bucket/control storage on supported std.
        self.dense_bytes
            .saturating_add(self.values.capacity() * std::mem::size_of::<Vec<f32>>())
            .saturating_add(self.rows.capacity() * std::mem::size_of::<Row>())
            .saturating_add(
                self.positions
                    .capacity()
                    .saturating_mul(2 * (std::mem::size_of::<(i64, usize)>() + 1)),
            )
            .saturating_add(self.sources.capacity() * std::mem::size_of::<String>())
            .saturating_add(
                self.source_ids
                    .capacity()
                    .saturating_mul(2 * (std::mem::size_of::<(String, usize)>() + 1)),
            )
            .saturating_add(self.string_bytes)
            .saturating_add(self.legacy_bytes)
    }
    fn reserve(&mut self, rows: usize) -> Result<(), String> {
        if Self::estimate(rows, self.dimensions)
            .saturating_add(self.string_bytes)
            .saturating_add(self.legacy_bytes)
            > self.allowance
        {
            return Err("Exact search cache exceeds memory allowance".into());
        }
        // Include old+new metadata during growth. Dense vectors never realloc:
        // only a bounded new slab is added, after its own allowance check.
        let row_growth = if rows > self.rows.capacity() {
            rows.saturating_mul(std::mem::size_of::<Row>())
        } else {
            0
        };
        let map_growth = if rows > self.positions.capacity() {
            rows.saturating_mul(2)
                .checked_next_power_of_two()
                .unwrap_or(usize::MAX)
                .saturating_mul(2 * (std::mem::size_of::<(i64, usize)>() + 1))
        } else {
            0
        };
        if self
            .allocated()
            .saturating_add(row_growth)
            .saturating_add(map_growth)
            > self.allowance
        {
            return Err("Exact search cache metadata exceeds memory allowance".into());
        }
        let missing = rows.saturating_sub(self.rows.len());
        self.rows
            .try_reserve_exact(missing)
            .map_err(|e| e.to_string())?;
        self.positions
            .try_reserve(missing)
            .map_err(|e| e.to_string())?;
        if self.allocated() > self.allowance {
            return Err("Exact search cache exceeds memory allowance".into());
        }
        Ok(())
    }
    fn remove(&mut self, rowid: i64) {
        if let Some(i) = self.positions.remove(&rowid) {
            let last = self.rows.len() - 1;
            let (block, offset) = (i / BATCH, (i % BATCH) * self.dimensions);
            let (last_block, last_offset) = (last / BATCH, (last % BATCH) * self.dimensions);
            if block == last_block {
                self.values[block].copy_within(last_offset..last_offset + self.dimensions, offset);
            } else {
                let (before, tail) = self.values.split_at_mut(last_block);
                before[block][offset..offset + self.dimensions]
                    .copy_from_slice(&tail[0][last_offset..last_offset + self.dimensions]);
            }
            self.values[last_block].truncate(last_offset);
            if last_offset == 0 {
                self.dense_bytes -= self.values.pop().unwrap().capacity() * 4;
            }
            let row = self.rows.swap_remove(i);
            self.string_bytes -= row.id.capacity();
            self.legacy_bytes -= row.legacy.as_ref().map(|v| v.capacity() * 8).unwrap_or(0);
            if i != last {
                self.positions.insert(self.rows[i].rowid, i);
            }
        }
    }
    fn insert(&mut self, row: &rusqlite::Row<'_>) -> Result<(), String> {
        let rowid = row.get(0).map_err(err)?;
        self.remove(rowid);
        let id: String = row.get(1).map_err(err)?;
        let source: String = row.get(2).map_err(err)?;
        let data = row.get_ref(3).map_err(err)?;
        let raw_bytes = match data {
            rusqlite::types::ValueRef::Blob(b) | rusqlite::types::ValueRef::Text(b) => b.len(),
            _ => 0,
        };
        // Include both source-map copies, dead source slots, and legacy allocations.
        let extra = id
            .capacity()
            .saturating_add(source.capacity().saturating_mul(2));
        let source_growth = if !self.source_ids.contains_key(&source) {
            let array = if self.sources.len() == self.sources.capacity() {
                self.sources
                    .capacity()
                    .saturating_mul(2)
                    .saturating_add(8)
                    .saturating_mul(std::mem::size_of::<String>())
            } else {
                0
            };
            let map = if self.source_ids.len() == self.source_ids.capacity() {
                self.source_ids
                    .capacity()
                    .saturating_mul(3)
                    .saturating_add(8)
                    .saturating_mul(2 * (std::mem::size_of::<(String, usize)>() + 1))
            } else {
                0
            };
            array.saturating_add(map)
        } else {
            0
        };
        let slab_growth = if self.rows.len() % BATCH == 0 {
            BATCH
                .saturating_mul(self.dimensions)
                .saturating_mul(4)
                .saturating_add(128)
        } else {
            0
        };
        if self
            .allocated()
            .saturating_add(extra)
            .saturating_add(source_growth)
            .saturating_add(slab_growth)
            .saturating_add(raw_bytes.saturating_mul(8))
            > self.allowance
        {
            return Err("Exact search cache exceeds memory allowance".into());
        }
        if self.rows.len() == self.rows.capacity() {
            let row_bytes = self
                .dimensions
                .saturating_mul(4)
                .saturating_add(ROW_ALLOWANCE);
            let max_rows = self
                .allowance
                .saturating_sub(self.string_bytes)
                .saturating_sub(self.legacy_bytes)
                / row_bytes.max(1);
            let target = self.rows.len().saturating_add(BATCH).min(max_rows);
            if target <= self.rows.len() {
                return Err("Exact search cache exceeds memory allowance".into());
            }
            self.reserve(target)?;
        }
        if self
            .allocated()
            .saturating_add(extra)
            .saturating_add(source_growth)
            .saturating_add(slab_growth)
            .saturating_add(raw_bytes.saturating_mul(8))
            > self.allowance
        {
            return Err("Exact search cache exceeds memory allowance".into());
        }
        if self.rows.len() % BATCH == 0 {
            let mut slab = Vec::new();
            slab.try_reserve_exact(BATCH.saturating_mul(self.dimensions))
                .map_err(|e| e.to_string())?;
            self.values.try_reserve(1).map_err(|e| e.to_string())?;
            self.dense_bytes += slab.capacity() * 4;
            self.values.push(slab);
        }
        let source = if let Some(i) = self.source_ids.get(&source) {
            *i
        } else {
            self.sources.try_reserve(1).map_err(|e| e.to_string())?;
            self.source_ids.try_reserve(1).map_err(|e| e.to_string())?;
            let i = self.sources.len();
            let copy = source.clone();
            self.string_bytes += source.capacity() + copy.capacity();
            self.sources.push(copy);
            self.source_ids.insert(source, i);
            i
        };
        let slab = self.values.last_mut().unwrap();
        let dense = match data {
            rusqlite::types::ValueRef::Blob(b) if b.len() == self.dimensions * 4 => Some(b),
            _ => None,
        };
        let (norm, legacy) = if let Some(bytes) = dense {
            let mut norm = 0.;
            for bytes in bytes.chunks_exact(4) {
                let v = f32::from_le_bytes(bytes.try_into().unwrap());
                slab.push(v);
                norm += (v as f64) * (v as f64);
            }
            (norm, None)
        } else {
            let decoded = text::decode(data);
            // Keep exact legacy values, including nonfinite values/odd lengths.
            slab.resize(slab.len() + self.dimensions, 0.);
            (0., Some(decoded))
        };
        self.string_bytes += id.capacity();
        self.legacy_bytes += legacy.as_ref().map(|v| v.capacity() * 8).unwrap_or(0);
        self.positions.insert(rowid, self.rows.len());
        self.rows.push(Row {
            rowid,
            id,
            source,
            norm,
            legacy,
        });
        if self.allocated() > self.allowance {
            return Err("Exact search cache exceeds memory allowance".into());
        }
        Ok(())
    }
    fn winners(
        &self,
        query: &[f64],
        budget: usize,
        mut checkpoint: impl FnMut() -> Result<(), String>,
    ) -> Result<Vec<(i64, String, f64)>, String> {
        let mut norm = 0.;
        for v in query {
            norm += v * v;
        }
        let mut best = Vec::new();
        best.try_reserve_exact(self.sources.len())
            .map_err(|e| e.to_string())?;
        best.resize(self.sources.len(), None::<(usize, f64)>);
        // Four independent accumulators expose instruction/SIMD parallelism
        // without reordering a single vector's additions (or using F32 sums).
        // BATCH is divisible by four, so each group stays inside one dense slab.
        for (group, rows) in self.rows.chunks(4).enumerate() {
            let base = group * 4;
            if base % 256 == 0 {
                checkpoint()?;
            }
            let slab = &self.values[base / BATCH];
            let offset = (base % BATCH) * self.dimensions;
            let mut dots = [0.; 4];
            if rows.len() == 4 {
                let a = &slab[offset..offset + self.dimensions];
                let b = &slab[offset + self.dimensions..offset + 2 * self.dimensions];
                let c = &slab[offset + 2 * self.dimensions..offset + 3 * self.dimensions];
                let d = &slab[offset + 3 * self.dimensions..offset + 4 * self.dimensions];
                for ((((q, a), b), c), d) in query.iter().zip(a).zip(b).zip(c).zip(d) {
                    dots[0] += q * (*a as f64);
                    dots[1] += q * (*b as f64);
                    dots[2] += q * (*c as f64);
                    dots[3] += q * (*d as f64);
                }
            } else {
                for (j, dot) in dots.iter_mut().take(rows.len()).enumerate() {
                    for (q, v) in query.iter().zip(
                        &slab[offset + j * self.dimensions..offset + (j + 1) * self.dimensions],
                    ) {
                        *dot += q * (*v as f64);
                    }
                }
            }
            for (j, row) in rows.iter().enumerate() {
                let i = base + j;
                let score = if let Some(v) = &row.legacy {
                    text::cosine(query, v)
                } else if norm == 0. || row.norm == 0. {
                    0.
                } else {
                    dots[j] / (norm * row.norm).sqrt()
                }
                .max(0.);
                if best[row.source]
                    .map(|(old, score_old)| {
                        score > score_old || (score == score_old && row.id < self.rows[old].id)
                    })
                    .unwrap_or(true)
                {
                    best[row.source] = Some((i, score));
                }
            }
        }
        let mut winners = best.into_iter().flatten().collect::<Vec<_>>();
        let compare = |a: &(usize, f64), b: &(usize, f64)| {
            b.1.total_cmp(&a.1)
                .then_with(|| self.rows[a.0].id.cmp(&self.rows[b.0].id))
        };
        if winners.len() > budget {
            winners.select_nth_unstable_by(budget, compare);
            winners.truncate(budget);
        }
        winners.sort_unstable_by(compare);
        Ok(winners
            .into_iter()
            .map(|(i, score)| {
                let r = &self.rows[i];
                (r.rowid, r.id.clone(), score)
            })
            .collect())
    }
}

pub(crate) struct Cache {
    key: Option<Key>,
    packed: Option<Packed>,
    cursor: i64,
    end: Option<i64>,
    scan_done: bool,
    ready: bool,
    rejected: bool,
    generation: i64,
    data_version: i64,
    last_use: Instant,
    pub control: Arc<Control>,
    pub budget: usize,
    pub loaded_rows: usize,
    journal: bool,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            key: None,
            packed: None,
            cursor: i64::MIN,
            end: None,
            scan_done: false,
            ready: false,
            rejected: false,
            generation: 0,
            data_version: 0,
            last_use: Instant::now(),
            control: Arc::default(),
            budget: memory_budget(),
            loaded_rows: 0,
            journal: false,
        }
    }
}
impl Cache {
    pub(super) fn has_requested_cache(&self) -> bool {
        self.key.is_some()
    }
    pub fn clear(&mut self) {
        self.key = None;
        self.packed = None;
        self.end = None;
        self.ready = false;
        self.rejected = false;
        self.scan_done = false;
        self.cursor = i64::MIN;
    }
    pub(super) fn journal(&mut self, db: &Connection) -> Result<(), String> {
        if !self.journal {
            db.execute_batch(include_str!("search_journal.sql"))
                .map_err(err)?;
            self.journal = true;
        }
        Ok(())
    }
    fn version(db: &Connection) -> Result<i64, String> {
        db.query_row("PRAGMA data_version", [], |r| r.get(0))
            .map_err(err)
    }
    fn generation(db: &Connection) -> Result<i64, String> {
        db.query_row(
            "SELECT generation FROM temp.knowledge_search_generation",
            [],
            |r| r.get(0),
        )
        .map_err(err)
    }
    fn request(&mut self, db: &Connection, key: &Key) -> Result<(), String> {
        self.journal(db)?;
        let version = Self::version(db)?;
        if self.key.as_ref() != Some(key)
            || version != self.data_version
            || self.last_use.elapsed() >= IDLE
        {
            // Release old buffers BEFORE any replacement allocation.
            self.clear();
            self.key = Some(key.clone());
            self.data_version = version;
            self.generation = Self::generation(db)?;
            self.loaded_rows = 0;
            db.execute("DELETE FROM temp.knowledge_search_dirty", [])
                .map_err(err)?;
        }
        self.last_use = Instant::now();
        if self.generation != Self::generation(db)? {
            self.ready = false;
        }
        Ok(())
    }
    pub fn needs_work(&self) -> bool {
        self.key.is_some() && !self.ready && !self.rejected && self.control.check().is_ok()
    }
    pub fn maintain(&mut self, db: &Connection) -> Result<bool, String> {
        if self.control.check().is_err() || self.last_use.elapsed() >= IDLE || memory_pressure() {
            self.clear();
            return Ok(false);
        }
        if self.key.is_none() {
            if self.journal {
                db.execute("DELETE FROM temp.knowledge_search_dirty", [])
                    .map_err(err)?;
            }
            return Ok(false);
        }
        if Self::version(db)? != self.data_version {
            let key = self.key.clone().unwrap();
            self.clear();
            self.request(db, &key)?;
        }
        if self.generation != Self::generation(db)? {
            self.ready = false;
        }
        if !self.needs_work() {
            return Ok(false);
        }
        let result = self.batch(db);
        if let Err(error) = result {
            // Allocation, cap, malformed legacy input, SQL failure or suspension:
            // drop partial buffers. Correct streaming remains available.
            self.packed = None;
            self.rejected = true;
            self.ready = false;
            log::debug!("Exact search cache unavailable: {error}");
            return Ok(false);
        }
        Ok(self.needs_work())
    }
    fn batch(&mut self, db: &Connection) -> Result<(), String> {
        self.control.check()?;
        let key = self.key.as_ref().unwrap();
        if self.end.is_none() {
            // A cheap upper bound for allocation ONLY, never a coverage proof.
            // Sparse rowids may conservatively reject a cache; streaming is exact.
            let (lo,hi):(i64,i64)=db.query_row("SELECT COALESCE((SELECT rowid FROM knowledge_chunks ORDER BY rowid LIMIT 1),0),COALESCE((SELECT rowid FROM knowledge_chunks ORDER BY rowid DESC LIMIT 1),0)",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
            let rows = hi.saturating_sub(lo).saturating_add(1).max(0) as usize;
            let mut packed = Packed::new(key.dimensions, self.budget);
            packed.reserve(rows)?;
            self.packed = Some(packed);
            self.end = Some(hi);
        }
        let packed = self.packed.as_mut().unwrap();
        if !self.scan_done {
            // Bound visited chunks, not returned eligible vectors: a sparse
            // model/root lane must not scan 600k rejected chunks in one batch.
            let mut boundary=db.prepare(&format!("SELECT rowid FROM knowledge_chunks WHERE rowid>? AND rowid<=? ORDER BY rowid LIMIT {BATCH}")).map_err(err)?;
            let ids = boundary
                .query_map(params![self.cursor, self.end], |r| r.get::<_, i64>(0))
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            if let Some(last) = ids.last() {
                let mut stmt = db
                    .prepare(&format!(
                        "{} AND c.rowid>? AND c.rowid<=? ORDER BY c.rowid",
                        key.select()
                    ))
                    .map_err(err)?;
                let mut rows = stmt.query(params![self.cursor, last]).map_err(err)?;
                let mut count = 0;
                while let Some(row) = rows.next().map_err(err)? {
                    if count % 256 == 0 {
                        self.control.check()?;
                    }
                    packed.insert(row)?;
                    count += 1;
                    self.loaded_rows += 1;
                }
                self.cursor = *last;
            }
            self.scan_done = ids.len() < BATCH;
            return Ok(());
        }
        // The TEMP dirty set is transactional and deduplicated. No count-based
        // coverage assumptions, no rebuild on progress/status metadata writes.
        let mut stmt = db
            .prepare(&format!(
                "SELECT rowid FROM temp.knowledge_search_dirty ORDER BY rowid LIMIT {BATCH}"
            ))
            .map_err(err)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        let mut stmt = db
            .prepare(&format!("{} AND c.rowid=?", key.select()))
            .map_err(err)?;
        for (i, id) in ids.iter().enumerate() {
            if i % 256 == 0 {
                self.control.check()?;
            }
            packed.remove(*id);
            let mut rows = stmt.query([id]).map_err(err)?;
            if let Some(row) = rows.next().map_err(err)? {
                packed.insert(row)?;
                self.loaded_rows += 1;
            }
            db.execute(
                "DELETE FROM temp.knowledge_search_dirty WHERE rowid=?",
                [id],
            )
            .map_err(err)?;
        }
        if ids.len() < BATCH {
            self.generation = Self::generation(db)?;
            self.ready = true;
        }
        Ok(())
    }
}

impl Store {
    pub(crate) fn search_checkpoint(&self) -> Result<(), String> {
        self.search_cancel
            .as_ref()
            .map(SearchToken::check)
            .unwrap_or(Ok(()))
    }
    pub(crate) fn search_cache_maintenance(&self) -> Result<bool, String> {
        self.search_cache.borrow_mut().maintain(&self.db)
    }
    /// Queue context only; allocation and reads happen in bounded worker batches.
    pub(crate) fn prewarm_source_cache(
        &self,
        roots: &[String],
        semantic_roots: &[String],
        model: Option<&str>,
        dimensions: usize,
    ) -> Result<(), String> {
        let canonical = |ids: &[String]| {
            let mut ids = ids.to_vec();
            ids.sort();
            ids.dedup();
            ids.iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",")
        };
        if roots.is_empty() || semantic_roots.is_empty() {
            self.clear_search_cache();
            return Ok(());
        }
        let key = Key {
            model: model.map(str::to_owned),
            dimensions,
            semantic_roots: canonical(semantic_roots),
            requested_roots: canonical(roots),
        };
        self.search_cache.borrow_mut().request(&self.db, &key)
    }
    pub(crate) fn clear_search_cache(&self) {
        self.search_cache.borrow_mut().clear();
    }
    pub(crate) fn exact_source_winners(
        &self,
        key: &Key,
        vector: &[f64],
        budget: usize,
    ) -> Result<Vec<(i64, String, f64)>, String> {
        self.search_checkpoint()?;
        {
            let mut cache = self.search_cache.borrow_mut();
            if memory_pressure() || cache.request(&self.db, key).is_err() {
                cache.clear();
            }
            if cache.ready && cache.control.check().is_ok() {
                match cache
                    .packed
                    .as_ref()
                    .unwrap()
                    .winners(vector, budget, || self.search_checkpoint())
                {
                    Ok(winners) => return Ok(winners),
                    Err(_) => {
                        self.search_checkpoint()?;
                        // Free dense buffers before streaming winner metadata
                        // if a fallible query allocation was rejected.
                        cache.clear();
                    }
                }
            }
        }
        // Correct cold/cap/dirty path: score every authoritative eligible chunk,
        // retaining source representatives, and hydrate only the final winners.
        let mut best = HashMap::<String, (i64, String, f64)>::new();
        let mut stmt = self.db.prepare(&key.select()).map_err(err)?;
        let mut rows = stmt.query([]).map_err(err)?;
        let mut count = 0;
        let query = text::CosineQuery::new(vector);
        while let Some(row) = rows.next().map_err(err)? {
            if count % 256 == 0 {
                self.search_checkpoint()?;
            }
            count += 1;
            let rowid = row.get(0).map_err(err)?;
            let id = row
                .get_ref(1)
                .map_err(err)?
                .as_str()
                .map_err(|e| e.to_string())?;
            let source = row
                .get_ref(2)
                .map_err(err)?
                .as_str()
                .map_err(|e| e.to_string())?;
            let score = query.score(row.get_ref(3).map_err(err)?).max(0.);
            if let Some(winner) = best.get_mut(source) {
                if score > winner.2 || (score == winner.2 && id < winner.1.as_str()) {
                    *winner = (rowid, id.to_owned(), score);
                }
            } else {
                best.insert(source.to_owned(), (rowid, id.to_owned(), score));
            }
        }
        self.search_checkpoint()?;
        let mut winners = best.into_values().collect::<Vec<_>>();
        let compare = |a: &(i64, String, f64), b: &(i64, String, f64)| {
            b.2.total_cmp(&a.2).then_with(|| a.1.cmp(&b.1))
        };
        if winners.len() > budget {
            winners.select_nth_unstable_by(budget, compare);
            winners.truncate(budget);
        }
        winners.sort_unstable_by(compare);
        Ok(winners)
    }
}

// Static physical-memory allowance plus macOS pressure fence. This is not a
// portable allocator/OS-pressure guarantee; unknown platforms use only 128 MiB.
fn memory_budget() -> usize {
    #[cfg(target_os = "macos")]
    {
        let mut bytes = 0u64;
        let mut len = std::mem::size_of::<u64>();
        let ok = unsafe {
            libc::sysctlbyname(
                b"hw.memsize\0".as_ptr().cast(),
                (&mut bytes as *mut u64).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        if ok {
            return ((bytes / 24) as usize).min(1024 * 1024 * 1024);
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
        let kb = |key: &str| {
            info.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0)
        };
        let allowance = (kb("MemTotal:") / 24)
            .min(kb("MemAvailable:") / 4)
            .saturating_mul(1024);
        if allowance > 0 {
            return allowance.min(1024 * 1024 * 1024);
        }
    }
    128 * 1024 * 1024
}
#[cfg(not(test))]
fn memory_pressure() -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut level = 0i32;
        let mut len = std::mem::size_of::<i32>();
        let ok = unsafe {
            libc::sysctlbyname(
                b"kern.memorystatus_vm_pressure_level\0".as_ptr().cast(),
                (&mut level as *mut i32).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        return ok && level > 1;
    }
    #[cfg(not(target_os = "macos"))]
    false
}

// Algorithm tests control pressure independently of unrelated desktop load.
// Production continues to read the OS fence above, without an override.
#[cfg(test)]
std::thread_local! { static TEST_MEMORY_PRESSURE: std::cell::Cell<bool> = std::cell::Cell::new(false); }
#[cfg(test)]
fn memory_pressure() -> bool {
    TEST_MEMORY_PRESSURE.with(|level| level.get())
}

#[cfg(test)]
mod tests {
    use super::super::super::runtime::MODEL_KEY;
    use super::*;
    use serde_json::Value;

    fn fixture(count: usize) -> Store {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("schema.sql")).unwrap();
        c.execute_batch("INSERT INTO knowledge_roots VALUES('r','/r','smart','local',1,0,0); INSERT INTO knowledge_roots VALUES('other','/other','smart','local',1,0,0);").unwrap();
        let s = Store::from_connection(c, false, "/scratch".into());
        for i in 1..=count {
            insert(&s, i);
        }
        s
    }
    fn vector(near: bool) -> Vec<f64> {
        let mut v = vec![0.; 256];
        v[usize::from(!near)] = 1.;
        v
    }
    fn insert(s: &Store, i: usize) {
        let id = format!("c{i:05}");
        let source = format!("s{i:05}");
        s.db.execute("INSERT INTO knowledge_sources(id,root_id,path,content_hash,byte_size,modified_at,status) VALUES(?,'r',?,'',0,0,'indexed')",params![source,format!("/r/{source}")]).unwrap();
        s.db.execute("INSERT INTO knowledge_chunks(rowid,id,source_id,text,embedding_json) VALUES(?,?,?,'unrelated archive',?)",params![i as i64,id,source,text::encode(&vec![0.;384])]).unwrap();
        s.db.execute(
            "INSERT INTO knowledge_gemma_embeddings VALUES(?,?,?)",
            params![id, MODEL_KEY, text::encode(&vector(i == 1))],
        )
        .unwrap();
    }
    fn query(s: &Store) -> Vec<Value> {
        s.search_sources(
            "needle",
            16,
            &["r".into()],
            false,
            Some((&vector(true), &["r".into()])),
        )
        .unwrap()
    }
    fn warm(s: &Store) {
        for _ in 0..100 {
            s.search_cache_maintenance().unwrap();
            if s.search_cache.borrow().ready {
                return;
            }
        }
        panic!("cache did not become ready");
    }
    #[test]
    #[ignore = "release-only deterministic 10k-vector search, excluding inference"]
    fn search_matrix() {
        use crate::native_core::performance::measure;
        use serde_json::json;
        let s = fixture(10_000);
        s.db.execute_batch("INSERT INTO knowledge_chunks_fts(rowid,id,source_id,text) SELECT rowid,id,source_id,text FROM knowledge_chunks;").unwrap();
        let roots = ["r".to_owned()];
        assert!(!query(&s).is_empty(), "semantic fixture must return hits");
        assert!(
            !s.search_sources("unrelated archive", 16, &roots, true, None)
                .unwrap()
                .is_empty(),
            "lexical fixture must return hits"
        );
        // Include cold scans, then measure the exact same sources with the cache.
        // Learned vectors are fixtures: this does not time a model or claim to.
        let cold = measure("deep.semantic.cold", || {
            s.clear_search_cache();
            Value::Array(query(&s))
        });
        warm(&s);
        let warm_result = measure("deep.semantic.warm", || Value::Array(query(&s)));
        assert_eq!(cold["results"], warm_result["results"]);
        let cache = s.search_cache.borrow();
        eprintln!(
            "TEZBAR_PERF {}",
            json!({"name":"deep.cache", "rows":cache.loaded_rows,
            "estimated_allocated_bytes":cache.packed.as_ref().unwrap().allocated()})
        );
        drop(cache);
        measure("deep.lexical", || {
            Value::Array(
                s.search_sources("unrelated archive", 16, &roots, true, None)
                    .unwrap(),
            )
        });
    }
    #[test]
    fn source_winners_match_full_sort_for_dense_legacy_ties_and_updates() {
        let s = fixture(360);
        let mut seed = 93u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) as i32 as f64 / i32::MAX as f64) * 0.7
        };
        for i in 1..=360 {
            let v: Vec<_> = (0..256).map(|_| next()).collect();
            let data = if i % 13 == 0 {
                serde_json::to_string(&v[..i % 17]).unwrap().into_bytes()
            } else {
                text::encode(&v)
            };
            if i % 13 == 0 {
                s.db.execute(
                    "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id=?",
                    params![std::str::from_utf8(&data).unwrap(), format!("c{i:05}")],
                )
                .unwrap();
            } else {
                s.db.execute(
                    "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id=?",
                    params![data, format!("c{i:05}")],
                )
                .unwrap();
            }
            // Multiple chunks/source, not a top-chunk shortcut. Include exact ties.
            s.db.execute(
                "UPDATE knowledge_chunks SET source_id=? WHERE id=?",
                params![format!("s{:05}", (i - 1) / 3 + 1), format!("c{i:05}")],
            )
            .unwrap();
        }
        query(&s);
        warm(&s);
        let key = s.search_cache.borrow().key.clone().unwrap();
        let mut checkpoints = 0;
        assert!(s
            .search_cache
            .borrow()
            .packed
            .as_ref()
            .unwrap()
            .winners(&vector(true), 80, || {
                checkpoints += 1;
                if checkpoints == 2 {
                    Err("cancel during packed scan".into())
                } else {
                    Ok(())
                }
            })
            .is_err());
        assert_eq!(checkpoints, 2);
        for query in [
            vec![0.; 256],
            (0..256).map(|_| next()).collect(),
            vector(true),
        ] {
            let mut best = HashMap::<String, (i64, String, f64)>::new();
            let mut stmt = s.db.prepare(&key.select()).unwrap();
            let mut rows = stmt.query([]).unwrap();
            while let Some(r) = rows.next().unwrap() {
                let rowid = r.get(0).unwrap();
                let id: String = r.get(1).unwrap();
                let source: String = r.get(2).unwrap();
                let score = text::cosine(&query, &text::decode(r.get_ref(3).unwrap())).max(0.);
                let winner = best.entry(source).or_insert((rowid, id.clone(), score));
                if score > winner.2 || (score == winner.2 && id < winner.1) {
                    *winner = (rowid, id, score);
                }
            }
            let mut reference: Vec<_> = best.into_values().collect();
            reference.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
            for budget in [0, 1, 16, 80, 1000] {
                let expected = reference.iter().take(budget).cloned().collect::<Vec<_>>();
                s.clear_search_cache();
                assert_eq!(
                    s.exact_source_winners(&key, &query, budget).unwrap(),
                    expected,
                    "authoritative cold lane, budget={budget}"
                );
                warm(&s);
                assert_eq!(
                    s.search_cache
                        .borrow()
                        .packed
                        .as_ref()
                        .unwrap()
                        .winners(&query, budget, || Ok(()))
                        .unwrap(),
                    expected
                );
            }
        }
    }
    #[test]
    fn lexical_source_selection_matches_beta17_window_with_starvation_and_filters() {
        let s = fixture(180);
        // A large source must not consume the unchanged 80-source window.
        s.db.execute(
            "UPDATE knowledge_chunks SET source_id='s00001' WHERE rowid<=100",
            [],
        )
        .unwrap();
        s.db.execute(
            "UPDATE knowledge_sources SET status='pending' WHERE id='s00105'",
            [],
        )
        .unwrap();
        s.db.execute(
            "UPDATE knowledge_sources SET root_id='other' WHERE id='s00106'",
            [],
        )
        .unwrap();
        s.db.execute(
            "UPDATE knowledge_chunks SET text='unrelated unrelated archive extra' WHERE rowid%7=0",
            [],
        )
        .unwrap();
        s.db.execute_batch("INSERT INTO knowledge_chunks_fts(rowid,id,source_id,text) SELECT rowid,id,source_id,text FROM knowledge_chunks;").unwrap();
        for query in ["unrelated", "unrelated archive", "archive extra", "absent"] {
            let matches = "FROM knowledge_chunks_fts f JOIN knowledge_chunks c ON c.rowid=f.rowid JOIN knowledge_sources s ON s.id=c.source_id JOIN knowledge_roots r ON r.id=s.root_id WHERE knowledge_chunks_fts MATCH ? AND s.status='indexed' AND r.enabled=1 AND r.id IN ('r')";
            let sql = format!("WITH scored AS MATERIALIZED (SELECT c.id,c.source_id,bm25(knowledge_chunks_fts,1.0) AS rank {matches}), ranked AS (SELECT *,ROW_NUMBER() OVER (PARTITION BY source_id ORDER BY rank,id) AS source_rank FROM scored) SELECT id FROM ranked WHERE source_rank=1 ORDER BY rank,id LIMIT ?");
            let mut lexical = vec![];
            let mut exact = std::collections::HashSet::new();
            for (fts, phrase) in [
                (super::super::content_fts_phrase(query), true),
                (super::super::content_fts_query(query), false),
            ] {
                if fts.is_empty() {
                    continue;
                }
                let ids =
                    s.db.prepare(&sql)
                        .unwrap()
                        .query_map(params![fts, 80], |r| r.get::<_, String>(0))
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                for id in ids {
                    if phrase {
                        exact.insert(id.clone());
                    }
                    if !lexical.contains(&id) {
                        lexical.push(id);
                    }
                }
            }
            let mut reference: Vec<_> = lexical
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let ls = (1. - i as f64 / lexical.len().max(1) as f64).max(0.15);
                    let score = ls * 0.72;
                    (
                        id.clone(),
                        if exact.is_empty() {
                            score
                        } else if exact.contains(id) {
                            0.75 + score * 0.25
                        } else {
                            score * 0.74
                        },
                    )
                })
                .collect();
            // sort_hits places phrase matches first; within each group score/id.
            reference.sort_by(|a, b| {
                exact
                    .contains(&b.0)
                    .cmp(&exact.contains(&a.0))
                    .then_with(|| b.1.total_cmp(&a.1))
                    .then_with(|| a.0.cmp(&b.0))
            });
            reference.truncate(16);
            let hits = s
                .search_sources(query, 16, &["r".into()], true, None)
                .unwrap();
            assert_eq!(
                hits.iter()
                    .map(|h| h["chunkId"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                reference.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(),
                "{query}"
            );
            for (hit, (_, score)) in hits.iter().zip(reference) {
                assert_eq!(hit["score"].as_f64().unwrap(), score, "{query}");
            }
        }
    }
    #[test]
    fn cache_same_count_deltas_status_and_rollback() {
        let s = fixture(3);
        let cold = query(&s);
        warm(&s);
        assert_eq!(query(&s), cold);
        let loaded = s.search_cache.borrow().loaded_rows;
        s.set_meta("status", &serde_json::json!({"progress":0.5}))
            .unwrap();
        s.db.execute("UPDATE knowledge_sources SET indexed_at=123", [])
            .unwrap();
        s.search_cache_maintenance().unwrap();
        assert!(
            s.search_cache.borrow().ready,
            "progress/status writes must not evict the lane"
        );
        assert_eq!(s.search_cache.borrow().loaded_rows, loaded);
        // Deferred FK violation makes the actual commit fail. Transaction Drop
        // rolls back both authoritative rows and their TEMP journal entries.
        s.db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE cache_parent(id INTEGER PRIMARY KEY); CREATE TABLE cache_child(id INTEGER REFERENCES cache_parent(id) DEFERRABLE INITIALLY DEFERRED);").unwrap();
        let tx = s.db.unchecked_transaction().unwrap();
        tx.execute_batch("UPDATE knowledge_gemma_embeddings SET model_key='bad'; INSERT INTO cache_child VALUES(99);").unwrap();
        assert!(tx.commit().is_err());
        assert_eq!(query(&s), cold);
        s.search_cache_maintenance().unwrap();
        assert_eq!(s.search_cache.borrow().loaded_rows, loaded);
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id='c00001'",
            [text::encode(&vector(false))],
        )
        .unwrap();
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id='c00002'",
            [text::encode(&vector(true))],
        )
        .unwrap();
        assert_eq!(query(&s)[0]["sourceId"], "s00002");
        warm(&s);
        assert_eq!(query(&s)[0]["sourceId"], "s00002");
        assert_eq!(
            s.search_cache.borrow().loaded_rows,
            loaded + 2,
            "only changed rows reloaded"
        );
        s.db.execute(
            "UPDATE knowledge_sources SET status='failed' WHERE id='s00002'",
            [],
        )
        .unwrap();
        warm(&s);
        assert!(query(&s).iter().all(|h| h["sourceId"] != "s00002"));
        s.db.execute(
            "UPDATE knowledge_sources SET status='pending' WHERE id='s00001'",
            [],
        )
        .unwrap();
        warm(&s);
        assert!(query(&s).iter().all(|h| h["sourceId"] != "s00001"));
        s.db.execute(
            "UPDATE knowledge_sources SET status='indexed',root_id='other' WHERE id='s00002'",
            [],
        )
        .unwrap();
        warm(&s);
        assert!(query(&s).iter().all(|h| h["sourceId"] != "s00002"));
        s.db.execute("UPDATE knowledge_roots SET enabled=0 WHERE id='r'", [])
            .unwrap();
        warm(&s);
        assert!(query(&s).is_empty());
        s.db.execute("UPDATE knowledge_roots SET enabled=1 WHERE id='r'", [])
            .unwrap();
        warm(&s);
        assert_eq!(query(&s).len(), 1);
        s.db.execute("DELETE FROM knowledge_chunks WHERE id='c00003'", [])
            .unwrap();
        warm(&s);
        assert!(query(&s).is_empty());
    }
    #[test]
    fn cache_bounded_loading_reconciles_inserts_and_replacements() {
        let s = fixture(BATCH + 2);
        query(&s);
        s.search_cache_maintenance().unwrap();
        assert!(!s.search_cache.borrow().ready);
        assert_eq!(s.search_cache.borrow().loaded_rows, BATCH);
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id='c00001'",
            [text::encode(&vector(false))],
        )
        .unwrap();
        insert(&s, BATCH + 3);
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id=?",
            params![text::encode(&vector(true)), format!("c{:05}", BATCH + 3)],
        )
        .unwrap();
        // Incomplete caches cannot claim global recall.
        assert_eq!(query(&s)[0]["sourceId"], format!("s{:05}", BATCH + 3));
        warm(&s);
        assert_eq!(query(&s)[0]["sourceId"], format!("s{:05}", BATCH + 3));
        assert_eq!(
            s.search_cache.borrow().packed.as_ref().unwrap().rows.len(),
            BATCH + 3
        );
        assert!(
            s.search_cache.borrow().loaded_rows <= BATCH + 6,
            "no needless global reload"
        );
    }
    #[test]
    fn cache_source_root_and_model_deletes_follow_store_transactions() {
        let mut s = fixture(3);
        let original = query(&s);
        warm(&s);
        let loaded = s.search_cache.borrow().loaded_rows;
        // Fail after delete_content has already removed chunks and fired their
        // journal triggers. Store must roll back both content and invalidation.
        s.db.execute_batch("CREATE TEMP TRIGGER reject_source_delete BEFORE DELETE ON main.knowledge_sources WHEN OLD.id='s00001' BEGIN SELECT RAISE(ABORT,'fixture deletion failure'); END;").unwrap();
        assert!(s.remove_source("s00001").is_err());
        assert_eq!(query(&s), original);
        s.search_cache_maintenance().unwrap();
        assert_eq!(s.search_cache.borrow().loaded_rows, loaded);
        s.db.execute_batch("DROP TRIGGER reject_source_delete")
            .unwrap();
        s.remove_source("s00001").unwrap();
        let remaining = query(&s);
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|h| h["sourceId"] != "s00001"));
        warm(&s);
        assert_eq!(query(&s), remaining);
        assert_eq!(s.search_cache.borrow().loaded_rows, loaded);
        s.gemma_clear().unwrap();
        assert!(query(&s).is_empty());
        warm(&s);
        assert!(query(&s).is_empty());
        for i in 2..=3 {
            s.db.execute(
                "INSERT INTO knowledge_gemma_embeddings VALUES(?,?,?)",
                params![format!("c{i:05}"), MODEL_KEY, text::encode(&vector(false))],
            )
            .unwrap();
        }
        assert_eq!(query(&s), remaining);
        warm(&s);
        assert_eq!(query(&s), remaining);
        s.root_remove("r").unwrap();
        assert!(query(&s).is_empty());
        warm(&s);
        assert!(query(&s).is_empty());
        assert_eq!(
            s.search_cache.borrow().packed.as_ref().unwrap().rows.len(),
            0
        );
    }
    #[test]
    fn cache_sparse_lane_still_visits_only_a_bounded_raw_batch() {
        let s = fixture(BATCH + 2);
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET model_key='other-model'",
            [],
        )
        .unwrap();
        assert!(query(&s).is_empty());
        s.search_cache_maintenance().unwrap();
        let cache = s.search_cache.borrow();
        assert_eq!(cache.loaded_rows, 0);
        assert!(
            !cache.scan_done,
            "batch bounds apply to visited chunks, not eligible output"
        );
        assert_eq!(cache.cursor, BATCH as i64);
        drop(cache);
        warm(&s);
        assert!(query(&s).is_empty());
    }

    #[test]
    fn cache_cap_idle_and_exact_model_switch() {
        let s = fixture(2);
        s.search_cache.borrow_mut().budget = 1;
        let expected = query(&s);
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().rejected);
        assert!(s.search_cache.borrow().packed.is_none());
        assert_eq!(query(&s), expected);
        s.clear_search_cache();
        s.search_cache.borrow_mut().budget = 4 * 1024 * 1024;
        query(&s);
        warm(&s);
        let key = s.search_cache.borrow().key.clone().unwrap();
        let mut other = key.clone();
        other.model = Some("scratch-other-exact-key".into());
        let winners = s.exact_source_winners(&other, &vector(true), 80).unwrap();
        assert!(winners.is_empty());
        assert!(
            s.search_cache.borrow().packed.is_none(),
            "old lane released before replacement"
        );
        warm(&s);
        assert!(s
            .search_cache
            .borrow()
            .packed
            .as_ref()
            .unwrap()
            .rows
            .is_empty());
        query(&s);
        warm(&s);
        s.search_cache.borrow_mut().last_use = Instant::now() - IDLE;
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().packed.is_none());
    }
    #[test]
    fn cache_growth_cap_and_cancelled_loading_release_buffers() {
        let s = fixture(2);
        query(&s);
        warm(&s);
        s.search_cache.borrow_mut().budget = 1;
        // Existing lane's allowance is fixed; inject pressure-like budget into it.
        s.search_cache
            .borrow_mut()
            .packed
            .as_mut()
            .unwrap()
            .allowance = 1;
        insert(&s, 3);
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().packed.is_none());
        assert_eq!(query(&s).len(), 3);
        s.clear_search_cache();
        s.search_cache.borrow_mut().budget = 4 * 1024 * 1024;
        query(&s);
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().packed.is_some());
        s.search_cache
            .borrow()
            .control
            .paused
            .store(true, Ordering::SeqCst);
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().packed.is_none());
        s.search_cache
            .borrow()
            .control
            .paused
            .store(false, Ordering::SeqCst);
        query(&s);
        warm(&s);
        s.search_cache
            .borrow()
            .control
            .stopped
            .store(true, Ordering::SeqCst);
        s.search_cache_maintenance().unwrap();
        assert!(s.search_cache.borrow().packed.is_none());
    }
    #[test]
    fn cache_preserves_legacy_f64_and_short_vector_zip_norms() {
        let s = fixture(2);
        let legacy = vec![0.12345678901234567f64, 0.7];
        s.db.execute(
            "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id='c00001'",
            [serde_json::to_string(&legacy).unwrap()],
        )
        .unwrap();
        let cold = query(&s);
        warm(&s);
        assert_eq!(query(&s), cold);
        assert_eq!(
            cold.iter().find(|h| h["sourceId"] == "s00001").unwrap()["semanticScore"]
                .as_f64()
                .unwrap(),
            text::cosine(&vector(true), &legacy)
        );
    }
    #[test]
    fn cache_external_writer_invalidates_even_same_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.sqlite3");
        let s = fixture(2);
        // VACUUM INTO copies this tiny scratch fixture only, not any frozen DB.
        s.db.execute("VACUUM INTO ?", [path.to_string_lossy().as_ref()])
            .unwrap();
        let s = Store::from_connection(Connection::open(&path).unwrap(), false, path.clone());
        query(&s);
        warm(&s);
        let external = Connection::open(&path).unwrap();
        external
            .execute(
                "UPDATE knowledge_gemma_embeddings SET vector=? WHERE chunk_id='c00002'",
                [text::encode(&vector(true))],
            )
            .unwrap();
        external
            .execute(
                "UPDATE knowledge_sources SET status='failed' WHERE id='s00001'",
                [],
            )
            .unwrap();
        assert_eq!(query(&s)[0]["sourceId"], "s00002");
        assert!(!s.search_cache.borrow().ready);
        warm(&s);
        assert_eq!(query(&s)[0]["sourceId"], "s00002");
    }
    #[test]
    fn cache_cancelled_authoritative_and_packed_query() {
        let mut s = fixture(2);
        query(&s);
        warm(&s);
        let epoch = Arc::new(AtomicU64::new(0));
        let old = SearchGuard::new(epoch.clone());
        s.search_cancel = Some(old.0.clone());
        let _new = SearchGuard::new(epoch);
        assert!(s
            .search_sources(
                "needle",
                16,
                &["r".into()],
                false,
                Some((&vector(true), &["r".into()]))
            )
            .is_err());
        s.clear_search_cache();
        assert!(s
            .search_sources("needle", 16, &["r".into()], false, None)
            .is_err());
    }

    #[test]
    fn cache_pressure_releases_buffers_and_retains_authoritative_results() {
        let s = fixture(4);
        let cold = query(&s);
        warm(&s);
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                TEST_MEMORY_PRESSURE.with(|level| level.set(false));
            }
        }
        let _reset = Reset;
        TEST_MEMORY_PRESSURE.with(|level| level.set(true));
        assert!(!s.search_cache_maintenance().unwrap());
        assert!(s.search_cache.borrow().packed.is_none());
        assert_eq!(query(&s), cold);
        assert!(s.search_cache.borrow().packed.is_none());
        TEST_MEMORY_PRESSURE.with(|level| level.set(false));
        assert_eq!(query(&s), cold);
        warm(&s);
        assert_eq!(query(&s), cold);
    }
}
