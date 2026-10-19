// SPDX-License-Identifier: GPL-3.0-only

//! Frequencies for display and for Anki (LKP-040, ANK-025): the rank of a
//! value, the harmonic rank over several dictionaries and the frequency list.
//! Ports of Fushi's `frequency_rank.dart` and `frequency_field.dart` (ac8034e).

use crate::yomitan::Frequency;

/// The harmonic rank when no dictionary has a rank.
pub const NO_RANK: u64 = 9_999_999;

/// The rank of a value: the leading integer of its display string when > 0
/// (`1234㋕`, DIC-003), otherwise the value when > 0.
pub fn rank(f: &Frequency) -> Option<u64> {
    let shown = f.display.as_deref().and_then(|d| {
        let digits: String = d
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse::<u64>().ok().filter(|&n| n > 0)
    });
    shown.or_else(|| u64::try_from(f.value).ok().filter(|&n| n > 0))
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `floor(n / Σ 1/rᵢ)` over the first ranked value of each dictionary
/// (`groups` with the same name are one dictionary), computed exactly:
/// [10, 15] gives 12. [`NO_RANK`] without ranks.
pub fn harmonic_rank<'a>(groups: impl IntoIterator<Item = (&'a str, &'a [Frequency])>) -> u64 {
    let mut seen: Vec<&str> = Vec::new();
    let mut ranks: Vec<u64> = Vec::new();
    for (name, values) in groups {
        if seen.contains(&name) {
            continue;
        }
        if let Some(r) = values.iter().find_map(rank) {
            seen.push(name);
            ranks.push(r);
        }
    }
    if ranks.is_empty() {
        return NO_RANK;
    }
    // Σ 1/rᵢ = num / den
    let (mut num, mut den): (u128, u128) = (0, 1);
    for &r in &ranks {
        let r = u128::from(r);
        let (Some(a), Some(b)) = (
            num.checked_mul(r).and_then(|x| x.checked_add(den)),
            den.checked_mul(r),
        ) else {
            // denominators beyond 128 bits: close enough in floating point
            let sum: f64 = ranks.iter().map(|&r| 1.0 / r as f64).sum();
            return (ranks.len() as f64 / sum).floor() as u64;
        };
        let g = gcd(a, b);
        (num, den) = (a / g, b / g);
    }
    let n = ranks.len() as u128;
    u64::try_from(n * den / num).unwrap_or(u64::MAX)
}

/// The value as shown: its display string, or the number.
pub fn shown(f: &Frequency) -> String {
    match f.display.as_deref() {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => f.value.to_string(),
    }
}

/// The `{frequencies}` field (ANK-025): `<ul style="text-align: left;">`
/// with one `<li>D: v</li>` per value, or empty without values.
pub fn html<'a>(groups: impl IntoIterator<Item = (&'a str, &'a [Frequency])>) -> String {
    let items: String = groups
        .into_iter()
        .flat_map(|(name, values)| {
            values
                .iter()
                .map(move |v| format!("<li>{name}: {}</li>", shown(v)))
        })
        .collect();
    if items.is_empty() {
        String::new()
    } else {
        format!(r#"<ul style="text-align: left;">{items}</ul>"#)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(value: i64, display: Option<&str>) -> Frequency {
        Frequency {
            reading: None,
            value,
            display: display.map(Into::into),
        }
    }

    // T-LKP-040
    #[test]
    fn harmonic_rank_is_exact() {
        let a = [f(10, None)];
        let b = [f(15, None)];
        assert_eq!(harmonic_rank([("A", &a[..]), ("B", &b[..])]), 12);
        let c = [f(3, Some("1234㋕"))];
        assert_eq!(harmonic_rank([("C", &c[..])]), 1234);
        let none = [f(0, None), f(-1, None)];
        assert_eq!(harmonic_rank([("N", &none[..])]), NO_RANK);
        assert_eq!(
            html([("D", &a[..])]),
            r#"<ul style="text-align: left;"><li>D: 10</li></ul>"#
        );
        assert_eq!(html([("D", &none[..0])]), "");
    }
}
