use unicode_normalization::UnicodeNormalization;

pub fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
pub fn tokens(s: &str) -> Vec<String> {
    // ASCII is already NFKC; avoid the Unicode iterator/buffer on common paths.
    if s.is_ascii() {
        return s
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
    }
    s.nfkc()
        .collect::<String>()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
pub fn variants(s: &str) -> Vec<String> {
    let s = normalize(s);
    if s.is_empty() {
        return vec![];
    }
    let en: Vec<char> = "`qwertyuiop[]asdfghjkl;'zxcvbnm,.".chars().collect();
    let ru: Vec<char> = "ёйцукенгшщзхъфывапролджэячсмитьбю".chars().collect();
    let mut out = vec![s.clone()];
    for (from, to, yes) in [
        (
            &ru,
            &en,
            s.chars().any(|c| ('а'..='я').contains(&c) || c == 'ё'),
        ),
        (&en, &ru, s.chars().any(|c| c.is_ascii_lowercase())),
    ] {
        if yes {
            let mapped: String = s
                .chars()
                .map(|c| {
                    from.iter()
                        .position(|x| *x == c)
                        .map(|i| to[i])
                        .unwrap_or(c)
                })
                .collect();
            if !out.contains(&mapped) {
                out.push(mapped)
            }
        }
    }
    out
}
#[cfg(test)]
pub fn distance(a: &str, b: &str) -> usize {
    // Match JS UTF-16 edit distances, including non-BMP text.
    let a: Vec<_> = a.to_lowercase().encode_utf16().collect();
    let b: Vec<_> = b.to_lowercase().encode_utf16().collect();
    let mut prev: Vec<_> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, x) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(x != y));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
struct MatchToken {
    text: String,
    units: Vec<u16>,
    chars: usize,
    length: usize,
}
impl MatchToken {
    fn new(text: String) -> Self {
        Self {
            units: if text.is_ascii() {
                text.encode_utf16().collect()
            } else {
                text.to_lowercase().encode_utf16().collect()
            },
            chars: text.chars().count(),
            length: text.encode_utf16().count(),
            text,
        }
    }
}

/// Query-owned preparation, shared by all candidates and scoring passes. Never
/// retained across searches, so index/learning mutations need no invalidation.
pub struct Query {
    normalized: String,
    lexical: String,
    terms: Vec<MatchToken>,
    app_variants: Vec<Vec<String>>,
}
impl Query {
    pub fn new(query: &str) -> Self {
        let normalized = normalize(query);
        let terms: Vec<_> = tokens(query).into_iter().map(MatchToken::new).collect();
        let app_variants = tokens(&normalized).iter().map(|t| variants(t)).collect();
        Self {
            normalized,
            lexical: query.trim().to_lowercase(),
            terms,
            app_variants,
        }
    }
    fn lexical(&self, lower: &str) -> f64 {
        let q = &self.lexical;
        if q.is_empty() {
            return 0.;
        }
        if lower == q {
            return 1.;
        }
        if lower.starts_with(q) {
            return 0.9;
        }
        if lower.contains(q) {
            return 0.75;
        }
        if self.terms.is_empty() {
            return 0.;
        }
        self.terms
            .iter()
            .filter(|t| lower.contains(&t.text))
            .count() as f64
            / self.terms.len() as f64
            / 1.5
    }
    fn fuzzy(&self, candidate: &[MatchToken], scratch: &mut DistanceScratch) -> f64 {
        if self.terms.is_empty() {
            return 0.;
        }
        let mut sum = 0.;
        for q in &self.terms {
            let mut best: f64 = 0.;
            for a in candidate {
                best = best.max(scratch.similarity(a, q));
                if best == 1. {
                    break;
                }
            }
            sum += best;
        }
        let score = sum / self.terms.len() as f64;
        if score >= 0.45 {
            score
        } else {
            0.
        }
    }
    pub fn app_match(&self, name: &str) -> f64 {
        app_matches(name, std::slice::from_ref(self))
    }
    pub fn full_tokens(&self, title: &str, subtitle: &str) -> f64 {
        if self.terms.len() < 2 {
            return 0.;
        }
        let s = format!("{title} {subtitle}").to_lowercase();
        if self.terms.iter().all(|t| s.contains(&t.text)) {
            200.
        } else {
            0.
        }
    }
}

/// Normalize/tokenize each application name/alias at most once for all keyboard
/// variants. Direct name matches still avoid tokenization entirely.
pub fn app_matches(name: &str, queries: &[Query]) -> f64 {
    let name = normalize(name);
    let mut best: f64 = 0.;
    for name in std::iter::once(name.as_str()).chain(normalized_aliases(&name).iter().copied()) {
        let mut name_tokens = None;
        for (i, q) in queries.iter().enumerate() {
            if q.app_variants.is_empty() {
                continue;
            }
            let score = if name == q.normalized {
                1.
            } else if name.starts_with(&q.normalized) {
                0.92
            } else {
                let terms = name_tokens.get_or_insert_with(|| tokens(name));
                if q.app_variants.iter().all(|variants| {
                    variants
                        .iter()
                        .any(|v| terms.iter().any(|n| n.starts_with(v)))
                }) {
                    0.9
                } else {
                    0.
                }
            };
            best = best.max(score * if i == 0 { 1. } else { 0.9 });
            if best == 1. {
                return best;
            }
        }
    }
    best
}
#[derive(Default)]
pub struct DistanceScratch {
    prev: Vec<usize>,
    cur: Vec<usize>,
}
impl DistanceScratch {
    /// Ranking distinguishes only distances 0, 1 and 2. Do not compute a full
    /// title/query matrix merely to discard every larger distance at the caller.
    pub fn title_distance(&mut self, title: &str, query: &str) -> usize {
        let a: Vec<_> = title.to_lowercase().encode_utf16().collect();
        let b: Vec<_> = query.to_lowercase().encode_utf16().collect();
        self.bounded(&a, &b, 2).min(3)
    }
    // Exact within the caller's threshold; values outside it are irrelevant to
    // fuzzy scores. UTF-16 (not UTF-8/chars) preserves the legacy JS distances.
    fn bounded(&mut self, a: &[u16], b: &[u16], k: usize) -> usize {
        if a.len().abs_diff(b.len()) > k {
            return k + 1;
        }
        self.prev.resize(b.len() + 1, k + 1);
        self.cur.resize(b.len() + 1, k + 1);
        for (j, v) in self.prev.iter_mut().enumerate() {
            *v = j.min(k + 1);
        }
        for (i, x) in a.iter().enumerate() {
            self.cur[0] = (i + 1).min(k + 1);
            let start = (i + 1).saturating_sub(k).max(1);
            let end = (i + 1 + k).min(b.len());
            if start > 1 {
                self.cur[start - 1] = k + 1;
            }
            let mut minimum = self.cur[0];
            for j in start..=end {
                self.cur[j] = (self.prev[j] + 1)
                    .min(self.cur[j - 1] + 1)
                    .min(self.prev[j - 1] + usize::from(*x != b[j - 1]));
                minimum = minimum.min(self.cur[j]);
            }
            if end < b.len() {
                self.cur[end + 1] = k + 1;
            }
            if minimum > k {
                return k + 1;
            }
            std::mem::swap(&mut self.prev, &mut self.cur);
        }
        self.prev[b.len()]
    }
    fn similarity(&mut self, a: &MatchToken, b: &MatchToken) -> f64 {
        if a.text == b.text {
            return 1.;
        }
        if a.text.starts_with(&b.text) {
            return 0.92;
        }
        if b.text.starts_with(&a.text) && a.chars >= 3 {
            return 0.82;
        }
        if b.chars >= 3 && a.text.contains(&b.text) {
            return 0.74;
        }
        let n = a.length.max(b.length);
        let k = 1.max((n as f64 * 0.38).floor() as usize);
        let d = self.bounded(&a.units, &b.units, k);
        if d > k {
            0.
        } else {
            1. - d as f64 / n.max(1) as f64
        }
    }
}

pub fn best_match(text: &str, queries: &[Query], scratch: &mut DistanceScratch) -> (f64, usize) {
    let lower = text.to_lowercase();
    let mut candidate = None;
    queries
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let lex = q.lexical(&lower);
            let score = if lex == 1. {
                lex
            } else {
                let tokens = candidate.get_or_insert_with(|| {
                    tokens(text)
                        .into_iter()
                        .map(MatchToken::new)
                        .collect::<Vec<_>>()
                });
                lex.max(q.fuzzy(tokens, scratch))
            };
            (score * if i == 0 { 1. } else { 0.9 }, i)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0., 0))
}
#[cfg(test)]
pub fn fuzzy(text: &str, q: &str) -> f64 {
    Query::new(q).fuzzy(
        &tokens(text)
            .into_iter()
            .map(MatchToken::new)
            .collect::<Vec<_>>(),
        &mut DistanceScratch::default(),
    )
}
pub fn fts(q: &str) -> String {
    tokens(q)
        .iter()
        .map(|t| {
            if t.chars().count() == 1 {
                format!("\"{t}\"")
            } else {
                format!("{t}*")
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}
pub fn prior(c: &str) -> f64 {
    match c {
        "applications" => 0.72,
        "files" => 0.6,
        "knowledge" => 0.64,
        "clipboard" => 0.45,
        "quick-notes" | "extensions" => 0.68,
        "store" => 0.25,
        "mac-cli" => 0.46,
        "native-command" => 0.7,
        "commands" => 0.66,
        "snippets" => 0.58,
        "quick-links" => 0.55,
        "calculator" | "color-converter" => 0.9,
        _ => 0.35,
    }
}
pub fn weighted(lex: f64, age: u64, category: &str, pop: f64) -> f64 {
    ((lex.clamp(0., 1.) * 0.6
        + 0.1 / (1. + age as f64 / 86_400_000.)
        + prior(category) * 0.05
        + if pop > 0. {
            ((pop + 1.).log10() / 7.).min(1.) * 0.1
        } else {
            0.
        })
        * 1000.)
        .round()
}
pub fn query_boost(freq: f64, success: f64, last: u64, now: u64) -> f64 {
    if freq <= 0. || last == 0 {
        return 0.;
    }
    let days = now.saturating_sub(last) / 86_400_000;
    let r = if days < 1 {
        1250.
    } else if days < 7 {
        900.
    } else if days < 30 {
        550.
    } else {
        250.
    };
    (r + ((freq + 1.).log2() * 350.).min(1200.) + if success >= 0.5 { 150. } else { 0. }).round()
}
pub fn learned(freq: f64, success: f64, last: u64, now: u64) -> f64 {
    if freq <= 0. || last == 0 {
        return 0.;
    }
    let age = now.saturating_sub(last);
    let r = if age < 300_000 {
        500.
    } else if age < 1_800_000 {
        450.
    } else if age < 3_600_000 {
        400.
    } else if age < 86_400_000 {
        360.
    } else if age < 604_800_000 {
        220.
    } else if age < 2_592_000_000 {
        100.
    } else {
        0.
    };
    (r + ((freq + 1.).log2() * 220.).min(900.) + if success >= 0.5 { 120. } else { 0. }).round()
}
pub fn hot(n: u64) -> f64 {
    match n {
        0 => 0.,
        1 => 180.,
        2 => 600.,
        _ => 3600f64.min(2600. + (n - 3) as f64 * 250.),
    }
}
pub fn aliases(name: &str) -> &'static [&'static str] {
    normalized_aliases(&normalize(name))
}
fn normalized_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "visual studio code" => &["vscode", "vsc", "code", "vs code", "вс код", "вскод"],
        "google chrome" | "chrome" => &["chrome", "хром", "гугл хром"],
        "telegram" => &["телеграм", "телеграмм"],
        "discord" => &["дискорд"],
        "docker" => &["докер", "доккер"],
        "safari" => &["сафари"],
        "raycast" => &["рейкаст"],
        "claude" => &["клод", "клауд"],
        "xcode" => &["икскод"],
        "finder" => &["files", "filemanager", "файндер"],
        "terminal" => &["term", "shell", "терминал"],
        "quicktime player" => &["quicktime", "qt", "qtp"],
        "activity monitor" => &[
            "activity",
            "taskmanager",
            "task monitor",
            "мониторинг системы",
        ],
        _ => &[],
    }
}
pub fn app_match(name: &str, q: &str) -> f64 {
    Query::new(q).app_match(name)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distance_matches_reference() {
        fn reference(a: &str, b: &str) -> usize {
            let a: Vec<_> = a.to_lowercase().encode_utf16().collect();
            let b: Vec<_> = b.to_lowercase().encode_utf16().collect();
            let mut matrix = vec![vec![0; b.len() + 1]; a.len() + 1];
            for i in 0..=a.len() {
                matrix[i][0] = i;
            }
            for j in 0..=b.len() {
                matrix[0][j] = j;
            }
            for i in 1..=a.len() {
                for j in 1..=b.len() {
                    matrix[i][j] = (matrix[i - 1][j] + 1)
                        .min(matrix[i][j - 1] + 1)
                        .min(matrix[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
                }
            }
            matrix[a.len()][b.len()]
        }
        let mut inputs = vec![
            String::new(),
            "İΣҚазақ".into(),
            "😀𐐀".into(),
            "A".repeat(90),
            format!("{}b", "a".repeat(89)),
            "é e\u{301}".into(),
        ];
        let alphabet = ['A', 'b', 'İ', 'Σ', 'қ', '😀', '𐐀', '\u{301}'];
        let mut seed = 17u64;
        for n in 0..48 {
            inputs.push(
                (0..n % 17)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        alphabet[(seed >> 32) as usize % alphabet.len()]
                    })
                    .collect(),
            );
        }
        for a in &inputs {
            for b in &inputs {
                let expected = reference(a, b);
                assert_eq!(distance(a, b), expected, "{a:?} / {b:?}");
                let mut scratch = DistanceScratch::default();
                assert_eq!(
                    scratch.title_distance(a, b),
                    expected.min(3),
                    "{a:?} / {b:?}"
                );
                let a: Vec<_> = a.to_lowercase().encode_utf16().collect();
                let b: Vec<_> = b.to_lowercase().encode_utf16().collect();
                for k in 0..=8 {
                    assert_eq!(scratch.bounded(&a, &b, k).min(k + 1), expected.min(k + 1));
                }
            }
        }
    }
    // Frozen beta.17 scoring oracle: optimizations must retain its Unicode,
    // token, alias and keyboard-layout behavior, not just English typo recall.
    fn reference_tokens(s: &str) -> Vec<String> {
        s.nfkc()
            .collect::<String>()
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
    fn reference_lexical(text: &str, query: &str) -> f64 {
        let t = text.to_lowercase();
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return 0.;
        }
        if t == q {
            return 1.;
        }
        if t.starts_with(&q) {
            return 0.9;
        }
        if t.contains(&q) {
            return 0.75;
        }
        let terms = reference_tokens(&q);
        if terms.is_empty() {
            return 0.;
        }
        terms.iter().filter(|x| t.contains(x.as_str())).count() as f64 / terms.len() as f64 / 1.5
    }
    fn reference_similarity(a: &str, b: &str) -> f64 {
        if a == b {
            return 1.;
        }
        if a.starts_with(b) {
            return 0.92;
        }
        if b.starts_with(a) && a.chars().count() >= 3 {
            return 0.82;
        }
        if b.chars().count() >= 3 && a.contains(b) {
            return 0.74;
        }
        let d = distance(a, b);
        let n = a.encode_utf16().count().max(b.encode_utf16().count());
        if d > 1.max((n as f64 * 0.38).floor() as usize) {
            0.
        } else {
            1. - d as f64 / n.max(1) as f64
        }
    }
    fn reference_fuzzy(text: &str, query: &str) -> f64 {
        let a = reference_tokens(text);
        let b = reference_tokens(query);
        if b.is_empty() {
            return 0.;
        }
        let score = b
            .iter()
            .map(|q| {
                a.iter()
                    .map(|a| reference_similarity(a, q))
                    .fold(0., f64::max)
            })
            .sum::<f64>()
            / b.len() as f64;
        if score >= 0.45 {
            score
        } else {
            0.
        }
    }
    fn reference_app(name: &str, q: &str) -> f64 {
        let q = normalize(q);
        let qt = reference_tokens(&q);
        if qt.is_empty() {
            return 0.;
        }
        let name = normalize(name);
        std::iter::once(name.as_str())
            .chain(aliases(&name).iter().copied())
            .map(|name| {
                if name == q {
                    1.
                } else if name.starts_with(&q) {
                    0.92
                } else if qt.iter().all(|t| {
                    variants(t)
                        .iter()
                        .any(|v| reference_tokens(name).iter().any(|n| n.starts_with(v)))
                }) {
                    0.9
                } else {
                    0.
                }
            })
            .fold(0., f64::max)
    }
    #[test]
    fn prepared_scoring_matches_beta17_reference() {
        let mut inputs: Vec<String> = [
            "",
            "Calculator 100001",
            "calclator",
            "сфдсгдфещк",
            "Google Chrome",
            "хром",
            "Visual Studio Code",
            "vs code",
            "вс код",
            "İΣ Қазақ",
            "café",
            "cafe\u{301}",
            "１２ ﬁle",
            "𐐀𐐨 𐐩",
            "😀",
            "---",
            "a",
            "abc",
            "abcdef",
            "terminal",
            "терминал",
            "мониторинг системы",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let alphabet = ['a', 'z', 'İ', 'Σ', 'қ', '𐐀', '１', ' ', '-', '\u{301}'];
        let mut seed = 71u64;
        for n in 0..70 {
            inputs.push(
                (0..n % 19)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        alphabet[(seed >> 32) as usize % alphabet.len()]
                    })
                    .collect(),
            );
        }
        inputs.push((0u8..=127).map(char::from).collect());
        for text in &inputs {
            assert_eq!(tokens(text), reference_tokens(text));
            for query in &inputs {
                assert_eq!(
                    fuzzy(text, query),
                    reference_fuzzy(text, query),
                    "{text:?}/{query:?}"
                );
                assert_eq!(
                    app_match(text, query),
                    reference_app(text, query),
                    "{text:?}/{query:?}"
                );
                let variants = variants(query);
                let expected = variants
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        (
                            reference_lexical(text, q).max(reference_fuzzy(text, q))
                                * if i == 0 { 1. } else { 0.9 },
                            i,
                        )
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap_or((0., 0));
                let prepared: Vec<_> = variants.iter().map(|q| Query::new(q)).collect();
                let expected_app = variants
                    .iter()
                    .enumerate()
                    .map(|(i, q)| reference_app(text, q) * if i == 0 { 1. } else { 0.9 })
                    .fold(0., f64::max);
                assert_eq!(
                    app_matches(text, &prepared),
                    expected_app,
                    "{text:?}/{query:?}"
                );
                assert_eq!(
                    best_match(text, &prepared, &mut DistanceScratch::default()),
                    expected,
                    "{text:?}/{query:?}"
                );
            }
        }
    }
    #[test]
    fn multilingual() {
        assert!(variants("сркщьу").contains(&"chrome".into()));
        assert_eq!(tokens("Қазақ café １２"), vec!["қазақ", "café", "12"]);
        assert_eq!(distance("😀", ""), 2);
        assert_eq!(fuzzy("in min bin", "gemini"), 0.);
        assert_eq!(fts("n note"), "\"n\" OR note*");
        assert_eq!(app_match("Google Chrome", "хром"), 1.);
        assert!(hot(3) > hot(2));
    }
}

pub fn surface_boost(category: &str, title: &str, subtitle: &str, query: &Query) -> f64 {
    if category == "applications" {
        return query.app_match(title) * 600.;
    }
    if !["native-command", "commands", "extensions", "quick-notes"].contains(&category) {
        return 0.;
    }
    let title = title.to_lowercase();
    let q = query.normalized.as_str();
    if q.is_empty() {
        return 0.;
    }
    if category == "commands"
        && ((title == "extensions" && q == "extensions")
            || (title == "extensions store"
                && ["store", "extension store", "extensions store"].contains(&q)))
    {
        return 1400.;
    }
    let mut b: f64 = if title == q {
        600.
    } else if title.starts_with(&q) {
        420.
    } else if title.split_whitespace().any(|w| w.starts_with(&q)) {
        300.
    } else if title.contains(&q) {
        150.
    } else {
        0.
    };
    if category == "extensions" {
        let name = subtitle.split(" · ").next().unwrap_or("").to_lowercase();
        let slug = name.replace(' ', "");
        if name == q || slug == q {
            b = b.max(1200.)
        } else if name.starts_with(&q) || slug.starts_with(&q) {
            b = b.max(800.)
        } else if name.contains(&q) || slug.contains(&q) {
            b = b.max(400.)
        }
    }
    b
}
pub fn note_recency(category: &str, age: u64) -> f64 {
    if category != "quick-notes" {
        0.
    } else if age < 90_000 {
        1100.
    } else if age < 300_000 {
        520.
    } else if age < 1_800_000 {
        140.
    } else {
        0.
    }
}
pub fn exact_note(category: &str, title: &str, q: &str, age: u64) -> f64 {
    if category != "quick-notes" || age > 300_000 {
        return 0.;
    }
    let t = normalize(title);
    let q = normalize(q);
    if q.is_empty() {
        0.
    } else if t == q {
        1800.
    } else if t.starts_with(&q) {
        1400.
    } else if t.contains(&q) {
        900.
    } else {
        0.
    }
}
pub fn recommendation(id: &str) -> f64 {
    match id {
        "native:open-clipboard-history" => 900.,
        "native:open-snippets" => 880.,
        "extcmd:raycast.kill-process:index" => 860.,
        _ => 0.,
    }
}

pub fn intent(query: &str) -> serde_json::Value {
    let q = query.trim().to_lowercase();
    let (kind, confidence) = if q.is_empty() {
        ("general", 0.)
    } else if q.starts_with('/') {
        ("extension-command", 0.95)
    } else if regex::Regex::new(r"\b(file|folder|path|open\s+~|open\s+/)")
        .unwrap()
        .is_match(&q)
    {
        ("file", 0.78)
    } else if regex::Regex::new(r"\b(app|application|launch|open\s+[a-z])")
        .unwrap()
        .is_match(&q)
    {
        ("app", 0.72)
    } else if regex::Regex::new(r"\b(turn|enable|disable|toggle|run|kill|stop|start|mute|unmute)\b")
        .unwrap()
        .is_match(&q)
    {
        ("command", 0.8)
    } else if q.ends_with('?')
        || regex::Regex::new(r"^(ask|explain|summarize|write|draft|generate)\b")
            .unwrap()
            .is_match(&q)
    {
        ("ai", 0.65)
    } else {
        ("general", 0.45)
    };
    serde_json::json!({"type":kind,"normalizedQuery":q,"confidence":confidence})
}

pub fn utf16_prefix(s: &str, length: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(length).collect::<Vec<_>>())
}
