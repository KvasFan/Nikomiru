// SPDX-License-Identifier: GPL-3.0-only

//! Speech, through whisper.cpp's `whisper-cli` (SUB-062, SHD-001): the audio
//! of a video or of the microphone made into 16 kHz mono WAV by ffmpeg, then
//! read on this computer into SRT. Nothing leaves the computer.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process::{Job, file_arg};

/// The sample rate whisper.cpp reads.
pub const SAMPLE_RATE: u32 = 16_000;

/// The longest video whose audio is read: ten hours.
pub const MAX_MS: i64 = 10 * 3_600_000;

/// The longest recording of the microphone (SHD-001).
pub const MAX_RECORDING_MS: i64 = 30_000;

/// The microphone ffmpeg records from unless the settings name another one:
/// `<ffmpeg input format>:<device>`.
pub fn default_input() -> &'static str {
    if cfg!(target_os = "linux") {
        "pulse:default"
    } else if cfg!(target_os = "macos") {
        "avfoundation::0"
    } else {
        // DirectShow names devices: the user picks one in the settings
        ""
    }
}

/// ffmpeg's arguments to write the audio stream `stream` of `input` (its
/// ff-index; `None`: the first audio stream) to `wav`.
pub fn audio_job(
    ffmpeg: &Path,
    input: &Path,
    stream: Option<i64>,
    wav: &Path,
    duration_ms: i64,
) -> Job {
    let map = match stream {
        Some(i) => format!("0:{i}"),
        None => "0:a:0".into(),
    };
    let rate = SAMPLE_RATE.to_string();
    let args: Vec<OsString> = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        file_arg(input),
        "-map".into(),
        map.into(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        rate.into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-f".into(),
        "wav".into(),
        file_arg(wav),
    ];
    let mut job = Job::new(ffmpeg, args);
    // decoding is much faster than playing; a slow disk or a network
    // share gets a minute plus a tenth of the length
    job.timeout =
        Duration::from_secs(60) + Duration::from_millis((duration_ms.clamp(0, MAX_MS) / 10) as u64);
    job.outputs = vec![wav.to_path_buf()];
    job
}

/// How whisper-cli reads one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The program.
    pub whisper: PathBuf,
    /// The ggml model.
    pub model: PathBuf,
    /// The voice-activity model, when installed: fewer invented lines over
    /// music and silence.
    pub vad: Option<PathBuf>,
    /// The spoken language (`ja`), or `auto`.
    pub language: String,
    /// Threads; 0: half the processor's cores.
    pub threads: usize,
}

/// The threads used when the settings say 0.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
        .min(16)
}

/// whisper-cli's job: `wav` read into `<base>.srt`.
pub fn whisper_job(reading: &Reading, wav: &Path, base: &Path, duration_ms: i64) -> Job {
    let threads = if reading.threads == 0 {
        default_threads()
    } else {
        reading.threads.min(64)
    };
    let language = if reading.language.trim().is_empty() {
        "auto".to_string()
    } else {
        reading.language.trim().to_string()
    };
    let mut args: Vec<OsString> = vec![
        "-m".into(),
        reading.model.clone().into_os_string(),
        "-f".into(),
        wav.as_os_str().to_os_string(),
        "-l".into(),
        language.into(),
        "-t".into(),
        threads.to_string().into(),
        "-osrt".into(),
        "-of".into(),
        base.as_os_str().to_os_string(),
        "-pp".into(),
        "-np".into(),
    ];
    if let Some(vad) = &reading.vad {
        args.extend(["--vad".into(), "-vm".into(), vad.clone().into_os_string()]);
    }
    let mut job = Job::new(&reading.whisper, args);
    // a slow processor reads an hour of speech in a few hours
    job.timeout =
        Duration::from_secs(600) + Duration::from_millis((duration_ms.clamp(0, MAX_MS) * 4) as u64);
    job.outputs = vec![srt_of(base)];
    job
}

/// The SRT whisper-cli writes for `base`.
pub fn srt_of(base: &Path) -> PathBuf {
    let mut name = base.as_os_str().to_os_string();
    name.push(".srt");
    PathBuf::from(name)
}

/// The percentage in a line of whisper-cli's `-pp` output.
pub fn progress(line: &str) -> Option<u8> {
    let (_, rest) = line.split_once("progress =")?;
    let number = rest.trim().trim_end_matches('%').trim();
    number.parse::<u8>().ok().filter(|p| *p <= 100)
}

/// ffmpeg's job recording `seconds` of the microphone `input`
/// (`<format>:<device>`) to `wav`; `None` when `input` names no format.
pub fn record_job(ffmpeg: &Path, input: &str, ms: i64, wav: &Path) -> Option<Job> {
    let (format, device) = input.trim().split_once(':')?;
    if format.is_empty() || format.starts_with('-') || device.starts_with('-') {
        return None;
    }
    let ms = ms.clamp(500, MAX_RECORDING_MS);
    let seconds = format!("{}.{:03}", ms / 1000, ms % 1000);
    let rate = SAMPLE_RATE.to_string();
    let args: Vec<OsString> = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        format.into(),
        "-i".into(),
        device.into(),
        "-t".into(),
        seconds.into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        rate.into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        "-f".into(),
        "wav".into(),
        file_arg(wav),
    ];
    let mut job = Job::new(ffmpeg, args);
    job.timeout = Duration::from_millis(ms as u64) + Duration::from_secs(15);
    job.outputs = vec![wav.to_path_buf()];
    Some(job)
}

/// What is compared of a line: no spaces, punctuation or symbols; katakana
/// as hiragana; Latin letters in lower case; full-width forms as ASCII.
pub fn comparable(text: &str) -> Vec<char> {
    text.chars()
        .filter_map(|c| {
            let c = match c as u32 {
                // full-width ASCII
                0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                _ => c,
            };
            let c = match c as u32 {
                // katakana that have a hiragana
                0x30A1..=0x30F6 => char::from_u32(c as u32 - 0x60).unwrap_or(c),
                _ => c,
            };
            (c.is_alphanumeric() || matches!(c as u32, 0x3041..=0x309F | 0x30FC))
                .then(|| c.to_lowercase().next().unwrap_or(c))
        })
        .collect()
}

/// How close what was said is to the line, from 0 to 100: one minus the
/// edit distance over the longer of the two, on [`comparable`] text.
pub fn similarity(line: &str, said: &str) -> u8 {
    let (a, b) = (comparable(line), comparable(said));
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 0;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut row = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            row[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(row[j] + 1);
        }
        std::mem::swap(&mut previous, &mut row);
    }
    let distance = previous[b.len()];
    (100 - distance * 100 / longest) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-062: the audio goes to 16 kHz mono WAV, the chosen stream only.
    #[test]
    fn the_audio_is_made_for_whisper() {
        let job = audio_job(
            Path::new("ffmpeg"),
            Path::new("/v/-ep.mkv"),
            Some(2),
            Path::new("/t/a.wav"),
            1_440_000,
        );
        let args: Vec<String> = job
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(at("-i"), "file:/v/-ep.mkv");
        assert_eq!(at("-map"), "0:2");
        assert_eq!(at("-ar"), "16000");
        assert_eq!(at("-ac"), "1");
        assert_eq!(args.last().unwrap(), "file:/t/a.wav");
        assert_eq!(job.outputs, [PathBuf::from("/t/a.wav")]);
        let first = audio_job(
            Path::new("ffmpeg"),
            Path::new("/v/a.mkv"),
            None,
            Path::new("/t/a.wav"),
            0,
        );
        assert!(first.args.iter().any(|a| a == "0:a:0"));
    }

    // T-SUB-062: whisper-cli is asked for SRT, progress and the VAD model.
    #[test]
    fn whisper_is_asked_for_srt() {
        let mut reading = Reading {
            whisper: "whisper-cli".into(),
            model: "/m/ggml.bin".into(),
            vad: Some("/m/vad.bin".into()),
            language: "ja".into(),
            threads: 3,
        };
        let job = whisper_job(&reading, Path::new("/t/a.wav"), Path::new("/t/out"), 60_000);
        let args: Vec<String> = job
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        for expected in [
            "-osrt",
            "-pp",
            "--vad",
            "/m/vad.bin",
            "/m/ggml.bin",
            "ja",
            "3",
            "/t/out",
        ] {
            assert!(args.iter().any(|a| a == expected), "{expected}: {args:?}");
        }
        assert_eq!(job.outputs, [PathBuf::from("/t/out.srt")]);
        reading.vad = None;
        reading.language = String::new();
        reading.threads = 0;
        let job = whisper_job(&reading, Path::new("/t/a.wav"), Path::new("/t/out"), 0);
        let args: Vec<String> = job
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(!args.iter().any(|a| a == "--vad"));
        assert!(args.iter().any(|a| a == "auto"));
        assert_eq!(
            progress("whisper_print_progress_callback: progress =  45%"),
            Some(45)
        );
        assert_eq!(progress("progress = 100%"), Some(100));
        assert_eq!(progress("progress = 140%"), None);
        assert_eq!(progress("read_audio_data: reading"), None);
    }

    // T-SHD-001: the microphone is `<format>:<device>`, never an option.
    #[test]
    fn the_microphone_is_a_format_and_a_device() {
        let job = record_job(
            Path::new("ffmpeg"),
            "pulse:default",
            2_500,
            Path::new("/t/r.wav"),
        )
        .unwrap();
        let args: Vec<String> = job
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(at("-f"), "pulse");
        assert_eq!(at("-i"), "default");
        assert_eq!(at("-t"), "2.500");
        let job = record_job(
            Path::new("ffmpeg"),
            "avfoundation::0",
            99_000,
            Path::new("/t/r.wav"),
        )
        .unwrap();
        assert!(job.args.iter().any(|a| a == ":0"));
        assert!(job.args.iter().any(|a| a == "30.000"));
        assert!(record_job(Path::new("ffmpeg"), "default", 1000, Path::new("/t/r.wav")).is_none());
        assert!(record_job(Path::new("ffmpeg"), "-f:x", 1000, Path::new("/t/r.wav")).is_none());
        assert!(record_job(Path::new("ffmpeg"), "pulse:-x", 1000, Path::new("/t/r.wav")).is_none());
    }

    // T-SHD-001: kana, width and punctuation do not count; words do.
    #[test]
    fn the_score_compares_what_was_said() {
        assert_eq!(similarity("行ってきます。", "行ってきます"), 100);
        assert_eq!(similarity("カタカナ！", "かたかな"), 100);
        assert_eq!(similarity("ＡＢＣ", "abc"), 100);
        assert_eq!(similarity("あいうえお", "あいうえか"), 80);
        assert_eq!(similarity("あいうえお", ""), 0);
        assert_eq!(similarity("", ""), 0);
        assert!(similarity("おはようございます", "こんばんは") < 30);
    }
}
