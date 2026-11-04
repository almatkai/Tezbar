//! Retain a bounded suffix so a credential split across deltas cannot escape
//! through individually harmless-looking text fragments.
pub struct Redactor {
    secrets: Vec<String>,
    pending: String,
    longest: usize,
}
impl Redactor {
    pub fn new(secrets: Vec<String>) -> Self {
        let secrets = secrets
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let longest = secrets.iter().map(String::len).max().unwrap_or(1);
        Self {
            secrets,
            pending: String::new(),
            longest,
        }
    }
    pub fn clean(&self, text: &str) -> String {
        let mut text = super::super::redact_text(text);
        for secret in &self.secrets {
            text = text.replace(secret, "[redacted]");
        }
        text
    }
    pub fn contains_secret(&self, text: &str) -> bool {
        self.secrets.iter().any(|s| text.contains(s))
    }
    pub fn push(&mut self, text: &str, final_chunk: bool) -> String {
        self.pending.push_str(text);
        let mut end = if final_chunk {
            self.pending.len()
        } else {
            self.pending
                .len()
                .saturating_sub(self.longest.saturating_sub(1))
        };
        while !self.pending.is_char_boundary(end) {
            end -= 1
        }
        for secret in &self.secrets {
            for (start, _) in self.pending.match_indices(secret) {
                if start < end && start + secret.len() > end {
                    end = start;
                }
            }
        }
        let raw = self.pending[..end].to_string();
        self.pending.drain(..end);
        self.clean(&raw)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_split_at_every_byte_never_reach_visible_output() {
        let mut r = Redactor::new(vec!["fixture-private-key".into()]);
        let mut out = String::new();
        for c in "🦀 before fixture-private-key after".chars() {
            out.push_str(&r.push(&c.to_string(), false));
        }
        out.push_str(&r.push("", true));
        assert_eq!(out, "🦀 before [redacted] after");
    }
}
