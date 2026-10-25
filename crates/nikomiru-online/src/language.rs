// SPDX-License-Identifier: GPL-3.0-only

//! The language and the episode of a subtitle file, from its name (ADR 0008
//! §8). Translated from Fushi `jimaku_client.dart` (`detectSubtitleLanguage`,
//! `parseSubtitleEpisode`, commit `ac8034e`).

use crate::name::parse_name;

/// Subtitle extensions.
const SUBTITLE_EXTENSIONS: [&str; 4] = ["srt", "ass", "ssa", "vtt"];

/// A language token (`ja`, `jpn`, `chs`, `zh-cn`, `ja[cc]`) as an ISO 639-1 code.
pub fn language_of_token(token: &str) -> Option<&'static str> {
    let token = token.trim().to_lowercase();
    // `ja[cc]`, `en[sdh]`: the brackets are not part of the language
    let token = match token.find('[') {
        Some(i) if token.ends_with(']') => token[..i].trim().to_string(),
        _ => token,
    };
    let code = |t: &str| match t {
        "ja" | "jpn" | "jp" | "japanese" => Some("ja"),
        "zh" | "zho" | "chi" | "chs" | "cht" | "sc" | "tc" | "chinese" => Some("zh"),
        "en" | "eng" | "english" => Some("en"),
        "ko" | "kor" | "korean" => Some("ko"),
        "it" | "ita" | "italian" => Some("it"),
        "es" | "spa" | "spanish" => Some("es"),
        "fr" | "fre" | "fra" | "french" => Some("fr"),
        "de" | "ger" | "deu" | "german" => Some("de"),
        "pt" | "por" | "portuguese" => Some("pt"),
        "ru" | "rus" | "russian" => Some("ru"),
        _ => None,
    };
    code(&token).or_else(|| code(token.split(['-', '_']).next().unwrap_or("")))
}

/// The language a subtitle file name states: a suffix before the extension
/// (`ep01.ja.srt`), a marker (`日本語`, `简体`), a bracketed tag (`[JP]`,
/// `(ENG)`); `None` when it states none.
pub fn subtitle_language(file_name: &str) -> Option<&'static str> {
    let parts: Vec<&str> = file_name.split('.').collect();
    if parts.len() >= 3
        && let Some(code) = language_of_token(parts[parts.len() - 2])
    {
        return Some(code);
    }
    const MARKERS: [(&str, &str); 10] = [
        ("日本語", "ja"),
        ("日语", "ja"),
        ("简体", "zh"),
        ("簡体", "zh"),
        ("繁體", "zh"),
        ("繁体", "zh"),
        ("中文", "zh"),
        ("英語", "en"),
        ("英语", "en"),
        ("한국어", "ko"),
    ];
    if let Some((_, code)) = MARKERS.iter().find(|(m, _)| file_name.contains(m)) {
        return Some(code);
    }
    let lower = file_name.to_lowercase();
    let mut rest = lower.as_str();
    while let Some(open) = rest.find(['[', '(']) {
        let after = &rest[open + 1..];
        let Some(close) = after.find([']', ')']) else {
            break;
        };
        let inner = &after[..close];
        if (2..=5).contains(&inner.len())
            && inner.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            && let Some(code) = language_of_token(inner)
        {
            return Some(code);
        }
        rest = &after[close + 1..];
    }
    None
}

/// Whether a file name is a subtitle Nikomiru draws (`.srt .ass .ssa .vtt`).
pub fn is_subtitle(file_name: &str) -> bool {
    extension(file_name).is_some_and(|e| SUBTITLE_EXTENSIONS.contains(&e.as_str()))
}

/// The extension, in lower case.
pub fn extension(file_name: &str) -> Option<String> {
    file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 5)
}

/// The episode a subtitle file name states: the extension and a language
/// suffix removed, then the file-name parser of MET-010.
pub fn subtitle_episode(file_name: &str) -> Option<u32> {
    let mut stem = file_name;
    if let Some((s, ext)) = stem.rsplit_once('.')
        && SUBTITLE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
    {
        stem = s;
    }
    if let Some((s, tag)) = stem.rsplit_once('.')
        && language_of_token(tag).is_some()
    {
        stem = s;
    }
    parse_name(stem).episode
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-003: Fushi `detectSubtitleLanguage`, `parseSubtitleEpisode`
    #[test]
    fn languages_and_episodes_of_subtitle_names() {
        for (name, code) in [
            ("ep01.ja.srt", Some("ja")),
            ("Show - 01.jpn.ass", Some("ja")),
            ("Show - 01.ja[cc].srt", Some("ja")),
            ("Show - 01.zh-cn.srt", Some("zh")),
            ("[Group] Show 01 [CHS].ass", Some("zh")),
            ("Show 01 日本語.srt", Some("ja")),
            ("Show 01 (ENG).srt", Some("en")),
            ("Show 01.srt", None),
            ("[Kitsunekko] Show 01.ass", None),
        ] {
            assert_eq!(subtitle_language(name), code, "{name}");
        }
        assert_eq!(subtitle_episode("Show - 12.ja.srt"), Some(12));
        assert_eq!(subtitle_episode("[Group] Show - 03 [1080p].ass"), Some(3));
        assert_eq!(subtitle_episode("Show S01E07.srt"), Some(7));
        assert_eq!(subtitle_episode("Show.zip"), None);
        assert!(is_subtitle("a.ASS"));
        assert!(!is_subtitle("a.zip"));
    }
}
