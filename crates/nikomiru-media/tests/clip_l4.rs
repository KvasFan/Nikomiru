// SPDX-License-Identifier: GPL-3.0-only

//! L3/L4: the sentence audio and the card image (T-ANK-033, T-ANK-034,
//! T-ANK-035, T-ANK-042): the ffmpeg arguments, then the real ffmpeg on
//! `av_basic.mkv`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use nikomiru_media::clip::{
    AudioClip, animated_args, animated_job, image_args, image_job, thumbnail_args, thumbnail_job,
};
use nikomiru_media::process::{Cancel, JobError, run};
use nikomiru_media::tools::{AudioFormat, ImageFormat, Search, probe};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name)
}

fn shown(args: &[std::ffi::OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn clip(stream: Option<i64>, format: AudioFormat, quality: u8) -> AudioClip {
    AudioClip {
        input: "/videos/-weird: name.mkv".into(),
        stream,
        start_ms: 10_250,
        end_ms: 12_400,
        format,
        quality,
    }
}

// T-ANK-033
#[test]
fn sentence_audio_arguments() {
    let out = Path::new("/cache/mining/a.m4a");
    assert_eq!(
        shown(&clip(Some(2), AudioFormat::AacM4a, 0).args(out)),
        "-nostdin -hide_banner -loglevel error -y -ss 10.250 -t 2.150 -i file:/videos/-weird: name.mkv \
         -vn -sn -map_chapters -1 -map 0:2 -c:a aac -ac 1 -b:a 64k -f ipod file:/cache/mining/a.m4a"
    );
    assert!(
        shown(&clip(None, AudioFormat::OpusOgg, 1).args(out))
            .contains("-map 0:a:0 -c:a libopus -ac 2 -b:a 128k -f ogg")
    );
    assert!(
        shown(&clip(Some(1), AudioFormat::Mp3, 2).args(out))
            .contains("-c:a libmp3lame -ac 2 -b:a 192k -f mp3")
    );
    // at most 30 s, never before 0
    let mut long = clip(Some(1), AudioFormat::AacM4a, 0);
    (long.start_ms, long.end_ms) = (-500, 90_000);
    assert!(shown(&long.args(out)).contains("-ss 0.000 -t 30.000 -i"));
}

// T-ANK-034
#[test]
fn image_arguments() {
    let (png, jpg) = (
        Path::new("/cache/mining/f.png"),
        Path::new("/cache/mining/f.jpg"),
    );
    assert_eq!(
        shown(&image_args(png, jpg, ImageFormat::Jpeg, 1)),
        "-nostdin -hide_banner -loglevel error -y -i file:/cache/mining/f.png -frames:v 1 \
         -vf scale=w='if(gte(iw,ih),min(1000,iw),-2)':h='if(gte(iw,ih),-2,min(1000,ih))' \
         -c:v mjpeg -q:v 3 -update 1 -f image2 file:/cache/mining/f.jpg"
    );
    let original = shown(&image_args(png, jpg, ImageFormat::Webp, 3));
    assert!(!original.contains("scale"), "{original}");
    assert!(
        original.contains("-c:v libwebp -quality 95 -f webp"),
        "{original}"
    );
}

fn ffmpeg() -> PathBuf {
    probe(&Search {
        setting: String::new(),
        env: None,
        path: std::env::var_os("PATH"),
    })
    .unwrap()
    .ffmpeg
}

/// The mono 48 kHz samples of an audio file.
fn samples(file: &Path) -> Vec<i16> {
    let out = Command::new(ffmpeg())
        .args(["-nostdin", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-f", "s16le", "-ac", "1", "-ar", "48000", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect()
}

/// The tone's frequency, from the rising zero crossings between its first
/// and last loud sample.
fn frequency(samples: &[i16]) -> f64 {
    let loud = |s: &i16| s.unsigned_abs() > 1000;
    let (Some(first), Some(last)) = (
        samples.iter().position(loud),
        samples.iter().rposition(loud),
    ) else {
        return 0.0;
    };
    let tone = &samples[first..=last];
    let crossings = tone.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
    crossings as f64 / (tone.len() as f64 / 48_000.0)
}

// T-ANK-033: the right track by its ff-index, the right length
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_sentence_audio_is_the_chosen_track() {
    let dir = tempfile::tempdir().unwrap();
    for (stream, tone) in [(1, 1000.0), (2, 660.0)] {
        let out = dir.path().join(format!("cue-{stream}.m4a"));
        let clip = AudioClip {
            input: fixture("av_basic.mkv"),
            stream: Some(stream),
            start_ms: 5_000,
            end_ms: 5_500,
            format: AudioFormat::AacM4a,
            quality: 0,
        };
        run(&clip.job(&ffmpeg(), &out), &Cancel::new()).unwrap();
        let pcm = samples(&out);
        let seconds = pcm.len() as f64 / 48_000.0;
        assert!((seconds - 0.5).abs() <= 0.05, "{seconds} s");
        let f = frequency(&pcm);
        assert!(
            (f - tone).abs() < tone * 0.05,
            "track {stream}: {f} Hz, not {tone}"
        );
    }
}

// T-ANK-035: a video without audio gives no sentence audio
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_video_without_audio_fails() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("none.m4a");
    let clip = AudioClip {
        input: fixture("av_noaudio.mp4"),
        stream: None,
        start_ms: 1_000,
        end_ms: 2_000,
        format: AudioFormat::AacM4a,
        quality: 0,
    };
    let err = run(&clip.job(&ffmpeg(), &out), &Cancel::new()).unwrap_err();
    assert!(matches!(err, JobError::Failed { .. }), "{err}");
    assert!(!out.exists(), "nothing left behind");
}

fn size(file: &Path) -> (u32, u32) {
    let out = Command::new(ffmpeg().with_file_name("ffprobe"))
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let (w, h) = text.trim().split_once(',').unwrap();
    (w.parse().unwrap(), h.parse().unwrap())
}

// T-ANK-034
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_image_is_scaled_to_its_level() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("frame.png");
    let status = Command::new(ffmpeg())
        .args(["-nostdin", "-loglevel", "error", "-ss", "2", "-i"])
        .arg(fixture("av_basic.mkv"))
        .args(["-frames:v", "1"])
        .arg(&png)
        .status()
        .unwrap();
    assert!(status.success());
    for (format, level, expected) in [
        (ImageFormat::Jpeg, 1, (1000, 562)),
        (ImageFormat::Jpeg, 0, (720, 406)),
        (ImageFormat::Jpeg, 2, (1280, 720)),
        (ImageFormat::Jpeg, 3, (1280, 720)),
        (ImageFormat::Webp, 1, (1000, 562)),
    ] {
        let ext = if format == ImageFormat::Jpeg {
            "jpg"
        } else {
            "webp"
        };
        let out = dir.path().join(format!("card-{level}.{ext}"));
        run(
            &image_job(&ffmpeg(), &png, &out, format, level),
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(size(&out), expected, "{format:?} level {level}");
        let bytes = std::fs::read(&out).unwrap();
        match format {
            ImageFormat::Jpeg => assert_eq!(&bytes[..3], [0xFF, 0xD8, 0xFF]),
            _ => assert_eq!(&bytes[8..12], b"WEBP"),
        }
    }
}

// T-ANK-042
#[test]
fn animated_picture_arguments() {
    let (video, out) = (
        Path::new("/videos/-weird: name.mkv"),
        Path::new("/cache/mining/a.webp"),
    );
    assert_eq!(
        shown(&animated_args(
            video,
            (10_250, 12_400),
            out,
            ImageFormat::WebpAnimated,
            1
        )),
        "-nostdin -hide_banner -loglevel error -y -ss 10.250 -t 2.150 -i file:/videos/-weird: name.mkv \
         -map 0:v:0 -an -sn -dn \
         -vf fps=12,scale=w='if(gte(iw,ih),min(640,iw),-2)':h='if(gte(iw,ih),-2,min(640,ih))' \
         -c:v libwebp_anim -quality 70 -loop 0 -f webp file:/cache/mining/a.webp"
    );
    // at most 10 s from the sentence's start, never before 0
    let avif = shown(&animated_args(
        video,
        (-500, 90_000),
        out,
        ImageFormat::AvifAnimated,
        3,
    ));
    assert!(avif.contains("-ss 0.000 -t 10.000 -i"), "{avif}");
    assert!(
        avif.contains("fps=15,scale=w='if(gte(iw,ih),min(1280,iw)"),
        "{avif}"
    );
    assert!(
        avif.contains("-c:v libaom-av1 -cpu-used 8 -row-mt 1 -crf 30 -pix_fmt yuv420p -f avif"),
        "{avif}"
    );
    // the still formats of a frame
    let still = shown(&image_args(out, out, ImageFormat::AvifAnimated, 1));
    assert!(still.contains("-c:v mjpeg"), "{still}");
}

// T-ANK-042
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn the_sentence_becomes_an_animated_picture() {
    let dir = tempfile::tempdir().unwrap();
    for (format, ext) in [
        (ImageFormat::WebpAnimated, "webp"),
        (ImageFormat::AvifAnimated, "avif"),
    ] {
        let out = dir.path().join(format!("sentence.{ext}"));
        // the first cue of av_basic, padded as a card's audio is
        let job = animated_job(
            &ffmpeg(),
            &fixture("av_basic.mkv"),
            (1_880, 2_700),
            &out,
            format,
            1,
        );
        run(&job, &Cancel::new()).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        match format {
            // an animation: its ANIM chunk, after VP8X
            ImageFormat::WebpAnimated => {
                assert_eq!(&bytes[8..12], b"WEBP");
                assert!(bytes.windows(4).any(|w| w == b"ANIM"), "not animated");
            }
            // an image sequence
            _ => assert_eq!(&bytes[4..12], b"ftypavis"),
        }
        let probe = Command::new(ffmpeg().with_file_name("ffprobe"))
            .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=width,height", "-of", "csv=p=0"])
            .arg(&out)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&probe.stdout);
        assert_eq!(text.lines().next(), Some("640,360"), "{format:?}");
        // about 10 frames (0.82 s at 12 fps)
        let frames = Command::new(ffmpeg().with_file_name("ffprobe"))
            .args(["-v", "error", "-count_frames", "-show_entries"])
            .args(["stream=nb_read_frames", "-of", "csv=p=0"])
            .arg(&out)
            .output()
            .unwrap();
        let counts = String::from_utf8_lossy(&frames.stdout);
        let most = counts
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        assert!((8..=12).contains(&most), "{format:?}: {counts}");
    }
}

// T-PLY-014
#[test]
fn thumbnail_arguments() {
    assert_eq!(
        shown(&thumbnail_args(
            Path::new("/videos/-weird: name.mkv"),
            83_250,
            144,
            Path::new("/cache/thumbnails/t.png")
        )),
        "-nostdin -hide_banner -loglevel error -y -ss 83.250 -i file:/videos/-weird: name.mkv \
         -map 0:v:0 -frames:v 1 -vf scale=-2:144 -an -sn -c:v png -f image2 file:/cache/thumbnails/t.png"
    );
}

// T-PLY-014
#[test]
#[ignore = "L4: needs `cargo xtask fixtures`"]
fn a_thumbnail_is_a_small_frame() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("t.png");
    let job = thumbnail_job(&ffmpeg(), &fixture("av_basic.mkv"), 2_000, 144, &out);
    run(&job, &Cancel::new()).unwrap();
    assert_eq!(size(&out), (256, 144));
}
