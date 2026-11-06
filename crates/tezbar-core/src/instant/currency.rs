//! Natural currency intents and deterministic preferences. Transport remains in core.
use regex::Regex;
macro_rules! cached_regex {
    ($pattern:literal) => {{
        static RE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};
static SYNONYMS: LazyLock<Vec<(String, String)>> =
    LazyLock::new(|| serde_json::from_str(include_str!("currency_synonyms.json")).unwrap());
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub amount: f64,
    pub from: String,
    pub to: String,
    pub target_explicit: bool,
}
pub fn valid_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}
pub fn fuzzy_currency(token: &str) -> Option<String> {
    let t = token.trim().to_lowercase();
    if t.chars().count() < 2 || t.len() > 128 {
        return None;
    }
    if t.len() == 3 && t.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Some(t.to_uppercase());
    }
    SYNONYMS
        .iter()
        .enumerate()
        .filter_map(|(i, (code, term))| {
            super::fuzzy::score(term, &t, 0.42, 2)
                .map(|s| (s.powf(super::fuzzy::field_norm(term)), i, code))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .filter(|(s, _, _)| *s <= 0.38)
        .map(|(_, _, c)| c.clone())
}
pub fn looks_like_pure_arithmetic(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace() || ".+-*/^(),%".contains(c))
}
fn symbol(c: char) -> Option<&'static str> {
    match c {
        '$' => Some("USD"),
        '€' => Some("EUR"),
        '£' => Some("GBP"),
        '¥' => Some("JPY"),
        '₽' => Some("RUB"),
        '₸' => Some("KZT"),
        '₹' => Some("INR"),
        '฿' => Some("THB"),
        _ => None,
    }
}
fn tokens(s: &str) -> Vec<String> {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric()
                || ('а'..='я').contains(&c)
                || ('А'..='Я').contains(&c)
                || matches!(c, 'ё' | 'Ё')
                || c.is_whitespace()
                || c == '.'
                || symbol(c).is_some()
            {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|s| !s.is_empty() && !['\0'].contains(&s.chars().next().unwrap()))
        .filter(|s| {
            ![
                "to", "in", "into", "at", "the", "a", "an", "of", "for", "and", "or", "в", "к",
                "на", "из", "по", "от", "до", "и",
            ]
            .contains(&s.to_lowercase().as_str())
        })
        .map(String::from)
        .collect()
}
fn first(s: &str) -> Option<String> {
    tokens(s)
        .iter()
        .find_map(|t| fuzzy_currency(t))
        .or_else(|| {
            let compact = s.split_whitespace().collect::<String>();
            (compact.encode_utf16().count() >= 3)
                .then(|| fuzzy_currency(&compact))
                .flatten()
        })
}
fn extract(s: &str) -> Option<(f64, Option<String>, String)> {
    let s = s.trim();
    let (prefix, s) = if s
        .chars()
        .next()
        .filter(|c| *c != '฿')
        .and_then(symbol)
        .is_some()
    {
        let c = s.chars().next()?;
        (symbol(c).map(String::from), s[c.len_utf8()..].trim_start())
    } else {
        (None, s)
    };
    let m = cached_regex!(r"^\d+(?:[.,]\d+)?\s*").find(s)?;
    let mut n = m.as_str().trim().to_string();
    if n.contains(',') {
        let tail = n.split(',').next_back()?;
        n = if tail.len() <= 2 {
            n.replace(',', ".")
        } else {
            n.replace(',', "")
        };
    }
    let amount = n.parse::<f64>().ok()?;
    if !amount.is_finite() || amount <= 0. {
        return None;
    }
    let rest = s[m.end()..].trim_start();
    let (sym, rest) = if let Some(c) = rest
        .chars()
        .next()
        .filter(|c| *c != '฿' && symbol(*c).is_some())
    {
        (
            symbol(c).map(String::from),
            rest[c.len_utf8()..].trim_start(),
        )
    } else {
        (None, rest)
    };
    Some((amount, prefix.or(sym), rest.into()))
}
pub fn parse_currency_query(s: &str, default_to: &str) -> Option<Intent> {
    if s.len() > 4096 || looks_like_pure_arithmetic(s) {
        return None;
    }
    let s = s.trim();
    let parts: Vec<_> = cached_regex!(r"(?i)\s*(?:into|to|in|→|=>|➝|в|к|на|из)\s+")
        .split(s)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let left = parts.first().copied().unwrap_or(s);
    if cached_regex!(r"\d\s*%").is_match(left) {
        return None;
    }
    let (amount, sym, rest) = extract(left)?;
    let (from, to, explicit) = if parts.len() >= 2 {
        (
            sym.or_else(|| first(&rest))
                .or_else(|| first(left))
                .unwrap_or_else(|| default_to.into()),
            first(&parts[1..].join(" "))?,
            true,
        )
    } else {
        let mut found = vec![];
        for t in tokens(&rest) {
            if let Some(c) = fuzzy_currency(&t) {
                if !found.contains(&c) {
                    found.push(c);
                }
            }
        }
        let from = sym.or_else(|| found.first().cloned())?;
        let target = if found.len() >= 2 {
            Some(found[1].clone())
        } else {
            found.first().filter(|c| **c != from).cloned()
        };
        (
            from,
            target.clone().unwrap_or_else(|| default_to.into()),
            target.is_some(),
        )
    };
    if from == to {
        return None;
    }
    Some(Intent {
        amount,
        from,
        to,
        target_explicit: explicit,
    })
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub count: u64,
    pub last_used_at: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Preferences {
    pub pinned: Option<String>,
    pub usage: BTreeMap<String, Usage>,
}
impl Preferences {
    pub fn validate(&mut self) {
        self.pinned = self.pinned.take().filter(|s| valid_code(s));
        self.usage.retain(|k, _| valid_code(k));
    }
    pub fn record(&mut self, code: &str, now: u64) {
        let c = code.trim().to_uppercase();
        if !valid_code(&c) {
            return;
        }
        let u = self.usage.entry(c).or_default();
        u.count = u.count.saturating_add(1);
        u.last_used_at = now;
    }
    pub fn pin(&mut self, c: Option<&str>) {
        self.pinned = c.map(str::to_uppercase).filter(|c| valid_code(c));
    }
    pub fn ranked(&self) -> Vec<(&str, &Usage)> {
        let mut v: Vec<_> = self.usage.iter().map(|(k, v)| (k.as_str(), v)).collect();
        v.sort_by(|a, b| {
            b.1.count
                .cmp(&a.1.count)
                .then(b.1.last_used_at.cmp(&a.1.last_used_at))
        });
        v
    }
    pub fn preferred(&self) -> &str {
        self.pinned
            .as_deref()
            .or_else(|| self.ranked().first().map(|(k, _)| *k))
            .unwrap_or("EUR")
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConversionEffect {
    FetchRates { generation: u64, from: String },
    RecordTarget(String),
}
#[derive(Default, Debug)]
pub struct Conversion {
    pub generation: u64,
    pub intent: Option<Intent>,
    pub value: Option<f64>,
    pub error: Option<String>,
}
impl Conversion {
    pub fn input(&mut self, s: &str, p: &Preferences) -> Vec<ConversionEffect> {
        self.generation += 1;
        self.value = None;
        self.error = None;
        self.intent = parse_currency_query(s, p.preferred());
        self.intent
            .as_ref()
            .map(|i| {
                vec![ConversionEffect::FetchRates {
                    generation: self.generation,
                    from: i.from.clone(),
                }]
            })
            .unwrap_or_default()
    }
    pub fn complete(
        &mut self,
        g: u64,
        rates: Result<&BTreeMap<String, f64>, String>,
    ) -> Vec<ConversionEffect> {
        if g != self.generation {
            return vec![];
        }
        let Some(i) = &self.intent else {
            return vec![];
        };
        match rates.and_then(|r| {
            r.get(&i.to)
                .copied()
                .filter(|r| r.is_finite() && *r > 0.)
                .ok_or_else(|| format!("No rate {} → {}", i.from, i.to))
        }) {
            Ok(r) if (i.amount * r).is_finite() => {
                self.value = Some(i.amount * r);
                self.error = None;
                if i.target_explicit {
                    return vec![ConversionEffect::RecordTarget(i.to.clone())];
                }
            }
            Ok(_) => self.error = Some("Converted amount is not finite".into()),
            Err(e) => self.error = Some(e),
        }
        vec![]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(s: &str, amount: f64, from: &str, to: &str, explicit: bool) {
        assert_eq!(
            parse_currency_query(s, "KZT"),
            Some(Intent {
                amount,
                from: from.into(),
                to: to.into(),
                target_explicit: explicit
            })
        );
    }
    #[test]
    fn parses_amount_dollar_default() {
        check("1$", 1., "USD", "KZT", false);
        check("$100", 100., "USD", "KZT", false);
    }
    #[test]
    fn parses_russian_preposition_fuzzy_ruble() {
        check("100$ в рублях", 100., "USD", "RUB", true);
    }
    #[test]
    fn parses_english_in_code() {
        check("100$ in eur", 100., "USD", "EUR", true);
    }
    #[test]
    fn parses_100_dollar_to_rub() {
        check("100$ to rub", 100., "USD", "RUB", true);
    }
    #[test]
    fn parses_explicit_usd_to_eur() {
        check("100 usd to eur", 100., "USD", "EUR", true);
    }
    #[test]
    fn parses_amount_eur_default() {
        check("50 eur", 50., "EUR", "KZT", false);
    }
    #[test]
    fn rejects_pure_arithmetic() {
        for s in ["1+1", "sqrt(4)"] {
            assert!(parse_currency_query(s, "KZT").is_none());
        }
    }
    #[test]
    fn rejects_percentage_phrases() {
        for s in ["100% в тенге", "15% of 200", "100%"] {
            assert!(parse_currency_query(s, "KZT").is_none());
        }
    }
    #[test]
    fn detects_math_only_buffers() {
        for s in ["1+1", " 2 * 3 "] {
            assert!(looks_like_pure_arithmetic(s));
        }
    }
    #[test]
    fn allows_currency_hints_through() {
        for s in ["1$", "100 usd"] {
            assert!(!looks_like_pure_arithmetic(s));
        }
    }
    #[test]
    fn starts_with_initial_default_no_usage() {
        let p = Preferences::default();
        assert_eq!(p.preferred(), "EUR");
        assert!(p.ranked().is_empty());
        assert!(p.pinned.is_none());
    }
    #[test]
    fn ranks_targets_by_usage_frequency() {
        let mut p = Preferences::default();
        for s in ["KZT", "KZT", "EUR", "KZT"] {
            p.record(s, 1);
        }
        assert_eq!(p.preferred(), "KZT");
        let r = p.ranked();
        assert_eq!((r[0].0, r[0].1.count), ("KZT", 3));
        assert_eq!((r[1].0, r[1].1.count), ("EUR", 1));
    }
    #[test]
    fn pinned_override_wins() {
        let mut p = Preferences::default();
        p.record("KZT", 1);
        p.record("KZT", 2);
        p.pin(Some("USD"));
        assert_eq!(p.preferred(), "USD");
        p.pin(None);
        assert_eq!(p.preferred(), "KZT");
    }
    #[test]
    fn ignores_invalid_codes() {
        let mut p = Preferences::default();
        for s in ["zz", "123"] {
            p.record(s, 0);
        }
        p.pin(Some("!!"));
        assert!(p.ranked().is_empty());
        assert!(p.pinned.is_none());
    }
    #[test]
    fn conversion_generation_error_retry_and_explicit_usage() {
        let p = Preferences::default();
        let mut c = Conversion::default();
        c.input("100 usd to eur", &p);
        let stale = c.generation;
        c.input("1$", &p);
        assert!(c.complete(stale, Err("late".into())).is_empty());
        assert!(c.error.is_none());
        c.complete(c.generation, Err("offline".into()));
        assert_eq!(c.error.as_deref(), Some("offline"));
        c.input("100 usd to eur", &p);
        let rates = BTreeMap::from([("EUR".into(), 0.9)]);
        assert_eq!(
            c.complete(c.generation, Ok(&rates)),
            vec![ConversionEffect::RecordTarget("EUR".into())]
        );
        assert_eq!(c.value, Some(90.));
    }
}
/// Locale formatting is injected by the host; numeric conversion stays portable and testable.
pub trait AmountFormatter {
    fn currency(&self, value: f64, code: &str, max_fraction_digits: u8) -> String;
}
#[derive(Clone, Debug, PartialEq)]
pub struct FormattedConversion {
    pub amount_formatted: String,
    pub formatted: String,
    pub clipboard: String,
    pub amount: f64,
    pub from: String,
    pub to: String,
}
impl Conversion {
    pub fn formatted(&self, formatter: &impl AmountFormatter) -> Option<FormattedConversion> {
        let i = self.intent.as_ref()?;
        let value = self.value?;
        let formatted = formatter.currency(value, &i.to, 2);
        Some(FormattedConversion {
            amount_formatted: formatter.currency(i.amount, &i.from, 4),
            clipboard: formatted.clone(),
            formatted,
            amount: i.amount,
            from: i.from.clone(),
            to: i.to.clone(),
        })
    }
}
/// Hook-equivalent 110ms replaceable timer. No rates are fetched until the adapter executes FetchRates.
#[derive(Debug, Default)]
pub struct ScheduledConversion {
    pub conversion: Conversion,
    due: Option<u64>,
}
impl ScheduledConversion {
    pub fn input(
        &mut self,
        s: &str,
        completion: bool,
        prefs: &Preferences,
        now: u64,
    ) -> Option<u64> {
        self.conversion
            .input(if completion { "" } else { s }, prefs);
        self.due = self
            .conversion
            .intent
            .as_ref()
            .map(|_| now.saturating_add(110));
        self.due
    }
    pub fn tick(&mut self, now: u64) -> Option<ConversionEffect> {
        if self.due.is_some_and(|t| now >= t) {
            self.due = None;
            Some(ConversionEffect::FetchRates {
                generation: self.conversion.generation,
                from: self.conversion.intent.as_ref()?.from.clone(),
            })
        } else {
            None
        }
    }
}
#[cfg(test)]
mod scheduled_tests {
    use super::*;
    #[test]
    fn debounce_is_110_ms_and_completion_input_cancels() {
        let mut s = ScheduledConversion::default();
        assert_eq!(s.input("1$", false, &Preferences::default(), 0), Some(110));
        assert!(s.tick(109).is_none());
        assert!(matches!(s.tick(110),Some(ConversionEffect::FetchRates{from,..})if from=="USD"));
        s.input("2$", false, &Preferences::default(), 111);
        s.input("/1$", true, &Preferences::default(), 112);
        assert!(s.tick(1000).is_none());
    }
}

#[cfg(test)]
mod opus_repairs {
    use super::*;
    #[test]
    fn opus_currency_compact_fallback_script_and_percent_side() {
        assert_eq!(first("u s d"), Some("USD".into()));
        assert_eq!(tokens("ңusd ң eur"), vec!["usd", "eur"]);
        let c = parse_currency_query("100 usd to eur 20%", "KZT").unwrap();
        assert_eq!((c.from, c.to), ("USD".into(), "EUR".into()));
        assert!(parse_currency_query("100% usd to eur", "KZT").is_none());
        assert!(parse_currency_query("฿100 to usd", "EUR").is_none());
    }
}
