// SPDX-License-Identifier: GPL-3.0-only

//! How loud an audio track is over time (SUB-018): ffmpeg decodes it to 8 kHz
//! mono PCM on its standard output, which is read here bin by bin; nothing is
//! written to disk.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::process::{Cancel, JobError, file_arg, spawn};

/// The sample rate the audio is decoded at: speech is below 4 kHz.
pub const RATE: i64 = 8_000;

/// How long decoding may take.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// The ffmpeg arguments that write `limit_ms` of the stream `stream` (an
/// ff-index; `None`: the first audio stream) of `input` as 8 kHz mono signed
/// 16-bit PCM on the standard output.
pub fn args(input: &Path, stream: Option<i64>, limit_ms: i64) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-t"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(format!("{}.{:03}", limit_ms / 1000, limit_ms % 1000).into());
    args.push("-i".into());
    args.push(file_arg(input));
    args.extend(["-vn", "-sn", "-map"].iter().map(OsString::from));
    args.push(match stream {
        Some(i) => format!("0:{i}").into(),
        None => "0:a:0".into(),
    });
    args.extend(
        ["-ac", "1", "-ar", "8000", "-f", "s16le", "-"]
            .iter()
            .map(OsString::from),
    );
    args
}

/// The RMS level in dB (0 = full scale) of every `bin_ms` of the PCM `samples`;
/// a silent bin is `-inf`.
pub fn bin_levels(samples: &[i16], bin_ms: i64) -> Vec<f32> {
    let per_bin = usize::try_from(RATE * bin_ms / 1000).unwrap_or(1).max(1);
    samples
        .chunks(per_bin)
        .map(|chunk| {
            let sum: f64 = chunk.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
            let mean = sum / chunk.len() as f64 / (32_768.0 * 32_768.0);
            if mean > 0.0 {
                (10.0 * mean.log10()) as f32
            } else {
                f32::NEG_INFINITY
            }
        })
        .collect()
}

/// The levels of `limit_ms` of an audio stream, every `bin_ms`: see [`args`]
/// and [`bin_levels`]. Killed after [`TIMEOUT`] or when `cancel` fires.
pub fn levels(
    ffmpeg: &Path,
    input: &Path,
    stream: Option<i64>,
    limit_ms: i64,
    bin_ms: i64,
    cancel: &Cancel,
) -> Result<Vec<f32>, JobError> {
    let program = ffmpeg.to_path_buf();
    let mut child = spawn(
        Command::new(ffmpeg)
            .args(args(input, stream, limit_ms))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .map_err(|source| JobError::Spawn {
        program: program.clone(),
        source,
    })?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(JobError::Failed {
            program,
            status: "no pipes".into(),
            stderr: String::new(),
        });
    };
    let errors = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stderr.read_to_end(&mut text);
        let start = text.len().saturating_sub(2048);
        String::from_utf8_lossy(&text[start..]).trim().to_string()
    });
    // the reading below blocks: a watcher kills ffmpeg on a timeout or a cancel
    let watcher = {
        let cancel = cancel.clone();
        let program = program.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + TIMEOUT;
            loop {
                if let Ok(Some(status)) = child.try_wait() {
                    return Ok(status);
                }
                let stop = if cancel.is_cancelled() {
                    Some(JobError::Cancelled {
                        program: program.clone(),
                    })
                } else if Instant::now() >= deadline {
                    Some(JobError::Timeout {
                        program: program.clone(),
                        timeout: TIMEOUT,
                    })
                } else {
                    None
                };
                if let Some(error) = stop {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        })
    };
    let mut bytes = Vec::new();
    let read = stdout.read_to_end(&mut bytes);
    let status = watcher.join().unwrap_or_else(|_| {
        Err(JobError::Failed {
            program: program.clone(),
            status: "watcher".into(),
            stderr: String::new(),
        })
    })?;
    let stderr = errors.join().unwrap_or_default();
    if !status.success() || read.is_err() {
        return Err(JobError::Failed {
            program,
            status: status.to_string(),
            stderr,
        });
    }
    let samples: Vec<i16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| i16::from_le_bytes(b))
        .collect();
    Ok(bin_levels(&samples, bin_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-018
    #[test]
    fn levels_of_pcm_bins() {
        let mut samples = vec![0i16; 800];
        samples.extend(std::iter::repeat_n(16_384, 800));
        samples.extend(std::iter::repeat_n(-32_768, 400));
        let levels = bin_levels(&samples, 100);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], f32::NEG_INFINITY);
        assert!((levels[1] - -6.02).abs() < 0.01, "{}", levels[1]);
        assert!(levels[2].abs() < 0.01, "{}", levels[2]);
        let a = args(Path::new("-v.mkv"), Some(2), 61_500);
        let a: Vec<String> = a.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(
            a.join(" "),
            "-nostdin -hide_banner -loglevel error -t 61.500 -i file:-v.mkv -vn -sn -map 0:2 -ac 1 -ar 8000 -f s16le -"
        );
    }
}
