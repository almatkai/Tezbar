//! Recursive descent arithmetic parser; identifiers are data, never executable code.
use super::number;
use regex::Regex;
use std::sync::LazyLock;
#[derive(Clone, Debug, PartialEq)]
pub struct CalcResult {
    pub expression: String,
    pub formatted: String,
    pub clipboard: String,
}
fn result(expression: &str, value: f64) -> Option<CalcResult> {
    if !value.is_finite() {
        return None;
    }
    let formatted = number(value);
    Some(CalcResult {
        expression: expression.into(),
        clipboard: formatted.clone(),
        formatted,
    })
}
static PERCENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(\d+(?:\.\d+)?)\s*%\s+of\s+(\d+(?:\.\d+)?)$").unwrap());
static UNITS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(-?\d+(?:\.\d+)?(?:e[+-]?\d+)?)\s*([a-z°]+)\s+(?:to|in)\s+([a-z°]+)$")
        .unwrap()
});
fn unit(s: &str) -> Option<(u8, f64)> {
    Some(match s {
        "m" => (0, 1.),
        "km" => (0, 1000.),
        "cm" => (0, 0.01),
        "mm" => (0, 0.001),
        "mi" => (0, 1609.344),
        "yd" => (0, 0.9144),
        "ft" => (0, 0.3048),
        "in" => (0, 0.0254),
        "g" => (1, 1.),
        "kg" => (1, 1000.),
        "mg" => (1, 0.001),
        "lb" => (1, 453.59237),
        "oz" => (1, 28.34952),
        _ => return None,
    })
}
fn temperature(s: &str) -> Option<u8> {
    match s.trim_start_matches('°') {
        "c" | "celsius" => Some(0),
        "f" | "fahrenheit" => Some(1),
        "k" | "kelvin" => Some(2),
        _ => None,
    }
}
pub fn evaluate_expression(input: &str) -> Option<CalcResult> {
    let s = input.trim();
    if s.is_empty() || s.len() > 4096 {
        return None;
    }
    if let Some(c) = PERCENT.captures(s) {
        return result(
            s,
            c[1].parse::<f64>().ok()? / 100. * c[2].parse::<f64>().ok()?,
        );
    }
    if let Some(c) = UNITS.captures(s) {
        let v = c[1].parse::<f64>().ok()?;
        let f = c[2].to_lowercase();
        let t = c[3].to_lowercase();
        if let (Some(f), Some(t)) = (temperature(&f), temperature(&t)) {
            let v = match f {
                0 => v,
                1 => (v - 32.) * (5. / 9.),
                _ => v - 273.15,
            };
            return result(
                s,
                match t {
                    0 => v,
                    1 => v * (9. / 5.) + 32.,
                    _ => v + 273.15,
                },
            );
        }
        let (a, f) = unit(&f)?;
        let (b, t) = unit(&t)?;
        return if a == b { result(s, v * f / t) } else { None };
    }
    if !s.chars().any(|c| "+-*/^%()".contains(c)) {
        return None;
    }
    let mut parser = Parser {
        chars: s.chars().collect(),
        pos: 0,
        depth: 0,
    };
    let value = parser.expr(0)?;
    parser.space();
    if parser.pos != parser.chars.len() || !value.is_finite() || number(value) == s {
        return None;
    }
    result(s, value)
}
struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}
impl Parser {
    fn space(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }
    fn take(&mut self, c: char) -> bool {
        self.space();
        if self.chars.get(self.pos) == Some(&c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self, min: u8) -> Option<f64> {
        self.depth += 1;
        if self.depth > 128 {
            return None;
        }
        let mut lhs = self.atom()?;
        loop {
            self.space();
            let Some(&op) = self.chars.get(self.pos) else {
                break;
            };
            let (prec, right) = match op {
                '+' | '-' => (1, false),
                '*' | '/' | '%' => (2, false),
                '^' => (3, true),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.pos += 1;
            if op == '*' && self.chars.get(self.pos) == Some(&'*') {
                return None;
            }
            let rhs = self.expr(if right { prec } else { prec + 1 })?;
            lhs = match op {
                '+' => lhs + rhs,
                '-' => lhs - rhs,
                '*' => lhs * rhs,
                '/' => lhs / rhs,
                '%' => lhs % rhs,
                _ => lhs.powf(rhs),
            };
        }
        self.depth -= 1;
        Some(lhs)
    }
    fn atom(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > 128 {
            return None;
        }
        self.space();
        let mut v = if self.take('+') {
            self.atom()?
        } else if self.take('-') {
            -self.atom()?
        } else if self.take('(') {
            let v = self.expr(0)?;
            if !self.take(')') {
                return None;
            }
            v
        } else {
            self.space();
            let start = self.pos;
            if self
                .chars
                .get(start)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
            {
                while self
                    .chars
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    self.pos += 1;
                }
                let name = self.chars[start..self.pos]
                    .iter()
                    .collect::<String>()
                    .to_lowercase();
                match name.as_str() {
                    "pi" => std::f64::consts::PI,
                    "e" => std::f64::consts::E,
                    _ => {
                        if !self.take('(') {
                            return None;
                        }
                        let mut args = vec![];
                        if !self.take(')') {
                            loop {
                                args.push(self.expr(0)?);
                                if self.take(')') {
                                    break;
                                }
                                if !self.take(',') {
                                    return None;
                                }
                            }
                        }
                        function(&name, &args)?
                    }
                }
            } else {
                while self
                    .chars
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                {
                    self.pos += 1;
                }
                if start == self.pos {
                    return None;
                }
                self.chars[start..self.pos]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .ok()?
            }
        };
        if self.take('°') {
            v *= std::f64::consts::PI / 180.;
        }
        self.depth -= 1;
        Some(v)
    }
}
fn function(n: &str, a: &[f64]) -> Option<f64> {
    if n == "min" {
        return Some(a.iter().copied().fold(f64::INFINITY, f64::min));
    }
    if n == "max" {
        return Some(a.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    }
    if n == "pow" {
        return if a.len() >= 2 {
            Some(a[0].powf(a[1]))
        } else {
            None
        };
    }
    let x = *a.first()?;
    Some(match n {
        "sqrt" => x.sqrt(),
        "cbrt" => x.cbrt(),
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "log" | "ln" => x.ln(),
        "log10" => x.log10(),
        "log2" => x.log2(),
        "abs" => x.abs(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => (x + 0.5).floor(),
        "trunc" => x.trunc(),
        "exp" => x.exp(),
        "sign" => {
            if x == 0. {
                x
            } else {
                x.signum()
            }
        }
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evaluates_basic_arithmetic() {
        for (s, v) in [
            ("42 * 3.14", "131.88"),
            ("2 + 3 * 4", "14"),
            ("(2 + 3) * 4", "20"),
        ] {
            assert_eq!(evaluate_expression(s).unwrap().formatted, v);
        }
    }
    #[test]
    fn evaluates_functions() {
        for (s, v) in [
            ("sqrt(144)", "12"),
            ("sin(90°)", "1"),
            ("abs(-5)", "5"),
            ("ln(e)", "1"),
            ("max(1, 2)", "2"),
            ("pow(2, 3)", "8"),
        ] {
            assert_eq!(evaluate_expression(s).unwrap().formatted, v);
        }
    }
    #[test]
    fn evaluates_percent_of() {
        for (s, v) in [("5% of 100", "5"), ("12.5% of 80", "10")] {
            assert_eq!(evaluate_expression(s).unwrap().formatted, v);
        }
    }
    #[test]
    fn evaluates_unit_conversions() {
        for (s, v) in [
            ("1 km to m", "1000"),
            ("1 kg to g", "1000"),
            ("32 f to c", "0"),
        ] {
            assert_eq!(evaluate_expression(s).unwrap().formatted, v);
        }
    }
    #[test]
    fn rejects_plain_words_and_non_math_inputs() {
        for s in ["hello", "pictures", "5"] {
            assert!(evaluate_expression(s).is_none());
        }
    }
    #[test]
    fn rejects_shell_metacharacters() {
        for s in ["1 + 1; rm -rf /", "$(whoami)"] {
            assert!(evaluate_expression(s).is_none());
        }
    }
    #[test]
    fn finite_bounds_and_precedence() {
        assert!(evaluate_expression(&("+".repeat(4000) + "1")).is_none());
        for s in ["sqrt(-1)", "1/0", "process(1)", &"(".repeat(500)] {
            assert!(evaluate_expression(s).is_none());
        }
        assert_eq!(evaluate_expression("2^3^2").unwrap().formatted, "512");
    }
}
