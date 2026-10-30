use super::*;
use crate::native_core::performance_alloc::volume;
fn legacy(value: &str) -> Vec<f64> {
    let normalized: String = value.to_lowercase().nfkc().collect();
    let mut vector = vec![0f32; 384];
    let hash = |v: &[u16], seed: u32| {
        v.iter()
            .fold(seed, |h, c| (h ^ *c as u32).wrapping_mul(16777619))
    };
    let mut add = |p: &str, w: &[u16]| {
        let mut f: Vec<_> = p.encode_utf16().collect();
        f.extend_from_slice(w);
        let bucket = hash(&f, 2166136261) as usize % 384;
        vector[bucket] += if hash(&f, 0x9e3779b9) & 1 == 0 {
            1.
        } else {
            -1.
        };
    };
    for w in normalized.split(js_whitespace).filter(|w| !w.is_empty()) {
        let units: Vec<_> = w.encode_utf16().collect();
        add("w:", &units);
        for n in 2..=4 {
            for part in units.windows(n) {
                add("c:", part);
            }
        }
    }
    let magnitude = vector
        .iter()
        .map(|x| (*x as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    let magnitude = if magnitude == 0. { 1. } else { magnitude };
    vector.into_iter().map(|x| x as f64 / magnitude).collect()
}
#[test]
fn built_in_embeddings_preserve_bits_without_per_ngram_allocation() {
    for input in [
        "",
        "a",
        "ＡＢ ﬃ K e\u{301} İ",
        "қазақша Русский 😀 𐐀",
        "a\u{a0}b\u{feff}c\u{200b}d",
        "nul\0 bytes",
        &"longword".repeat(200),
    ] {
        assert_eq!(
            embed(input).iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            legacy(input)
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
    }
    let input = "Runtime indexing preserves apples, pears and Unicode Қазақша. ".repeat(20);
    let (_, n, bytes) = volume(|| embed(&input));
    eprintln!("built-in embedding: {n} allocations, {bytes} requested bytes");
    assert!(
        n <= 32,
        "one 1200-character chunk must not allocate once per feature: {n}"
    );
}
#[test]
fn page_normalization_reuses_compiled_patterns_and_preserves_chunk_payloads() {
    let text = "A paragraph.  \t\n\n\n\nSecond paragraph.\0 End.";
    let expected = "A paragraph.\n\nSecond paragraph. End.";
    let pages = vec![json!({"pageNumber":1,"extractedText":text}); 64];
    let _ = chunks("warmup", &pages[..1], false);
    let (actual, n, bytes) = volume(|| chunks("fixture", &pages, false));
    assert_eq!(actual.len(), 64);
    for c in actual {
        assert_eq!(c["text"], expected);
        assert_eq!(c["id"], hash(format!("fixture:1:0:{expected}")));
        assert_eq!(c["startOffset"], 0);
        assert_eq!(c["endOffset"], expected.encode_utf16().count());
    }
    eprintln!("page normalization: {n} allocations, {bytes} requested bytes");
    assert!(
        n < 2000,
        "normalization of 64 pages must not recompile two regexes per page: {n}"
    );
}
#[test]
#[ignore = "release allocation/time probe"]
fn runtime_text_allocations() {
    let input = "Runtime indexing preserves apples, pears and Unicode Қазақша. ".repeat(20);
    let pages = vec![json!({"pageNumber":1,"extractedText":input}); 64];
    let _ = chunks("warmup", &pages[..1], false);
    for (name, run) in [("embedding", 0), ("pages", 1)] {
        let mut times = vec![];
        let mut n = 0;
        let mut bytes = 0;
        for _ in 0..40 {
            let start = std::time::Instant::now();
            let (_, a, b) = volume(|| {
                if run == 0 {
                    std::hint::black_box(embed(&input));
                } else {
                    std::hint::black_box(chunks("fixture", &pages, false));
                }
            });
            times.push(start.elapsed().as_secs_f64() * 1000.);
            n = a;
            bytes = b;
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "TEZBAR_RUNTIME_TEXT {}",
            json!({"name":name,"p50_ms":times[19],"p95_ms":times[37],"allocations":n,"requested_bytes":bytes})
        );
    }
}
