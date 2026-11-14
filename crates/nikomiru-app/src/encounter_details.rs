// SPDX-License-Identifier: GPL-3.0-only

//! Historical encounter details, independent of current word statuses (KNW-023).

/// Latest 1,000 first encounters, deterministic on equal timestamps.
pub fn rows(words: &nikomiru_library::encounters::Encounters) -> Vec<(String, String, i64)> {
    let mut rows: Vec<_> = words
        .iter()
        .filter_map(|(key, at)| {
            let (expression, reading) = key.split_once('\n')?;
            Some((expression.to_string(), reading.to_string(), *at))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    rows.truncate(1_000);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-023: bound details, newest first and keep expression/reading/time.
    #[test]
    fn details_are_bounded_and_ordered() {
        let mut words = nikomiru_library::encounters::Encounters::new();
        for i in 0..1005 {
            words.insert(format!("word{i}\nreading{i}"), i);
        }
        words.insert("invalid-key".into(), 9_999);
        let rows = rows(&words);
        assert_eq!(rows.len(), 1_000);
        assert_eq!(rows[0], ("word1004".into(), "reading1004".into(), 1004));
        assert_eq!(rows.last().unwrap().2, 5);
    }
}
