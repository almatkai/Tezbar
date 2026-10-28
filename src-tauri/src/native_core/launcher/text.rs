use unicode_normalization::UnicodeNormalization;

pub fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
pub fn tokens(s: &str) -> Vec<String> {
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
pub fn lexical(text: &str, query: &str) -> f64 {
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
    let terms = tokens(&q);
    if terms.is_empty() {
        return 0.;
    }
    terms.iter().filter(|x| t.contains(x.as_str())).count() as f64 / terms.len() as f64 / 1.5
}
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
fn similarity(a: &str, b: &str) -> f64 {
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
    if d > (1.max((n as f64 * 0.38).floor() as usize)) {
        0.
    } else {
        1. - d as f64 / n.max(1) as f64
    }
}
pub fn fuzzy(text: &str, q: &str) -> f64 {
    let a = tokens(text);
    let b = tokens(q);
    if b.is_empty() {
        return 0.;
    }
    let score = b
        .iter()
        .map(|q| a.iter().map(|a| similarity(a, q)).fold(0., f64::max))
        .sum::<f64>()
        / b.len() as f64;
    if score >= 0.45 {
        score
    } else {
        0.
    }
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
    match normalize(name).as_str() {
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
    let q = normalize(q);
    let qt = tokens(&q);
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
                    .any(|variant| tokens(name).iter().any(|n| n.starts_with(variant)))
            }) {
                0.9
            } else {
                0.
            }
        })
        .fold(0., f64::max)
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
                assert_eq!(distance(a, b), reference(a, b), "{a:?} / {b:?}");
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

pub fn surface_boost(category: &str, title: &str, subtitle: &str, q: &str) -> f64 {
    if category == "applications" {
        return app_match(title, q) * 600.;
    }
    if !["native-command", "commands", "extensions", "quick-notes"].contains(&category) {
        return 0.;
    }
    let title = title.to_lowercase();
    let q = normalize(q);
    if q.is_empty() {
        return 0.;
    }
    if category == "commands"
        && ((title == "extensions" && q == "extensions")
            || (title == "extensions store"
                && ["store", "extension store", "extensions store"].contains(&q.as_str())))
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
pub fn full_tokens(title: &str, subtitle: &str, q: &str) -> f64 {
    let t = tokens(q);
    let s = format!("{title} {subtitle}").to_lowercase();
    if t.len() >= 2 && t.iter().all(|t| s.contains(t)) {
        200.
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
