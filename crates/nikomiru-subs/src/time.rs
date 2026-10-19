// SPDX-License-Identifier: GPL-3.0-only

//! Timestamps of the three formats.

/// `[H:]MM:SS[,.]fff`: SRT and WebVTT timestamps. The fraction is decimal
/// (`,5` = 500 ms); hours are optional and may have any number of digits.
pub(crate) fn clock_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    let (clock, fraction) = match text.rfind([',', '.']) {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    };
    if fraction.len() > 3 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ms = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().ok()? * 10i64.pow(3 - fraction.len() as u32)
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let number = |s: &str| -> Option<i64> {
        (!s.is_empty() && s.len() <= 9 && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (h, m, s) = match parts[..] {
        [h, m, s] => (number(h)?, number(m)?, number(s)?),
        [m, s] => (0, number(m)?, number(s)?),
        _ => return None,
    };
    if m >= 60 || s >= 60 {
        return None;
    }
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}

/// `H:MM:SS.cc`: ASS timestamps (centiseconds).
pub(crate) fn ass_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    let (clock, fraction) = text.split_once('.').unwrap_or((text, "0"));
    if fraction.is_empty() || fraction.len() > 3 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ms = fraction.parse::<i64>().ok()? * 10i64.pow(3 - fraction.len() as u32);
    let parts: Vec<i64> = clock
        .split(':')
        .map(|p| {
            (!p.is_empty() && p.len() <= 9 && p.bytes().all(|c| c.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        })
        .collect::<Option<_>>()?;
    let [h, m, s] = parts[..] else { return None };
    if m >= 60 || s >= 60 {
        return None;
    }
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks() {
        assert_eq!(clock_ms("00:00:04,230"), Some(4230));
        assert_eq!(clock_ms("01:02:03.004"), Some(3_723_004));
        assert_eq!(clock_ms("02:03.5"), Some(123_500));
        assert_eq!(clock_ms("00:61:00,000"), None);
        assert_eq!(clock_ms("00:00:01,0000"), None);
        assert_eq!(clock_ms("x:00:01,000"), None);
        assert_eq!(ass_ms("0:00:01.50"), Some(1500));
        assert_eq!(ass_ms("1:00:00.05"), Some(3_600_050));
        assert_eq!(ass_ms("0:00:01"), Some(1000));
        assert_eq!(ass_ms("0:00"), None);
    }
}
