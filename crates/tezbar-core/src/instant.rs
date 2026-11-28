//! Pure, bounded instant answers. No network, shell, clipboard or UI ownership.
pub mod calculator;
pub mod color;
pub mod currency;
pub mod emoji;
mod fuzzy;
pub mod qr;
pub mod transform;

/// ECMAScript-style finite number display (including negative zero and exponent thresholds).
pub fn number(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value.is_sign_negative() {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    if value == 0.0 {
        return "0".into();
    }
    let a = value.abs();
    if a >= 1e21 || (a > 0.0 && a < 1e-6) {
        let s = format!("{value:e}");
        let (mantissa, exponent) = s.split_once('e').unwrap();
        let exponent: i32 = exponent.parse().unwrap();
        format!(
            "{mantissa}e{}{exponent}",
            if exponent >= 0 { "+" } else { "" }
        )
    } else {
        value.to_string()
    }
}
pub const MAX_INPUT_BYTES: usize = 1_048_576;
