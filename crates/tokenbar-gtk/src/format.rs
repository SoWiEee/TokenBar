//! Small number-formatting helpers shared across the lenses.

/// Format an integer with thousands separators (e.g. 1234567 -> "1,234,567").
pub fn group_thousands(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// Compact token counts for dense rows: 1_234 -> "1.2K", 4_300_000_000 -> "4.3B".
pub fn compact(n: i64) -> String {
    let a = n.unsigned_abs();
    let sign = if n < 0 { "-" } else { "" };
    let (value, suffix) = match a {
        0..=999 => return format!("{sign}{a}"),
        1_000..=999_999 => (a as f64 / 1e3, "K"),
        1_000_000..=999_999_999 => (a as f64 / 1e6, "M"),
        _ => (a as f64 / 1e9, "B"),
    };
    format!("{sign}{value:.1}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(42), "42");
        assert_eq!(group_thousands(1234), "1,234");
        assert_eq!(group_thousands(1234567), "1,234,567");
        assert_eq!(group_thousands(-9876543), "-9,876,543");
    }

    #[test]
    fn compacts_magnitudes() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_200), "1.2K");
        assert_eq!(compact(4_300_000), "4.3M");
        assert_eq!(compact(4_300_000_000), "4.3B");
    }
}
