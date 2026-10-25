// SPDX-License-Identifier: GPL-3.0-only

//! Generated fixtures (TEST §4): media made with ffmpeg `lavfi` and zipped
//! dictionaries, all in `target/fixtures/`. Each file is regenerated only if its
//! recipe has changed.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::synth;
use crate::util::{self, SplitMix64, rel};

/// Changing it forces all media to be regenerated.
const MEDIA_VERSION: &str = "media-v3";

/// `cargo xtask fixtures [--large] [--perf]`.
pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let (mut large, mut perf) = (false, false);
    for a in args {
        match a.as_str() {
            "--large" => large = true,
            "--perf" => perf = true,
            other => bail!("fixtures: unknown option {other}"),
        }
    }
    let out = root.join("target/fixtures");
    std::fs::create_dir_all(out.join("src"))?;
    if !util::have_program("ffmpeg") {
        bail!("ffmpeg is not in PATH: it is needed to generate the test media (TEST §4.1)");
    }
    media(&out, perf)?;
    zip_dict_sources(&root.join("tests/fixtures/dicts"), &out.join("dicts"))?;
    crate::evil::generate(
        &out.join("dicts"),
        &root.join("tests/fixtures/dicts/mini-ja"),
    )?;
    audio_databases(root, &out.join("audio"))?;
    if large {
        synth::generate(&out.join("dicts/synth"))?;
    }
    eprintln!("ok: fixtures in {}", rel(&out));
    Ok(())
}

/// The databases of the Local Audio Server for AUD-003, written by SQLite
/// itself through Python's `sqlite3` (TEST §4): only if the script changed.
fn audio_databases(root: &Path, out: &Path) -> Result<()> {
    let script = root.join("xtask/scripts/android_db.py");
    let recipe = std::fs::read_to_string(&script)?;
    let target = out.join("android.db");
    if fresh(&target, &recipe) {
        return Ok(());
    }
    if !util::have_program("python3") {
        eprintln!("  skipped: python3 is not in PATH (the audio databases of AUD-003)");
        return Ok(());
    }
    util::run(Command::new("python3").arg("-I").arg(&script).arg(out))?;
    stamp(&target, &recipe)
}

fn stamp_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.stamp"))
}

/// True if `target` exists and was produced with the same recipe.
pub fn fresh(target: &Path, recipe: &str) -> bool {
    target.exists() && std::fs::read_to_string(stamp_path(target)).is_ok_and(|s| s == recipe)
}

/// Records the recipe with which `target` was produced.
pub fn stamp(target: &Path, recipe: &str) -> Result<()> {
    std::fs::write(stamp_path(target), recipe)?;
    Ok(())
}

/// Reproducible output without global metadata (TEST §4.1). Only `:g`: with
/// `-map_metadata -1` ffmpeg would also strip the chapter titles.
const BITEXACT: [&str; 8] = [
    "-fflags",
    "+bitexact",
    "-flags:v",
    "+bitexact",
    "-flags:a",
    "+bitexact",
    "-map_metadata:g",
    "-1",
];

/// Encodes `name` with ffmpeg if the recipe (arguments + side files) has changed.
fn encode(out: &Path, name: &str, args: &[String], side: &[&str]) -> Result<()> {
    let target = out.join(name);
    let recipe = format!(
        "{MEDIA_VERSION}\n{}\n{}",
        args.join("\u{1f}"),
        side.join("\u{1f}")
    );
    if fresh(&target, &recipe) {
        eprintln!("  = {name}");
        return Ok(());
    }
    let ext = target
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = out.join(format!(".{name}.tmp.{ext}"));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .args(BITEXACT)
        .arg(&tmp);
    eprintln!("  + {name}");
    util::run(&mut cmd)?;
    std::fs::rename(&tmp, &target)?;
    stamp(&target, &recipe)
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|i| (*i).to_string()).collect()
}

/// Audio: silence with a `freq` Hz tone in the given windows.
fn tones(freq: u32, windows: &[(f64, f64)], duration: u32) -> String {
    let cond: Vec<String> = windows
        .iter()
        .map(|(a, b)| format!("between(t,{a},{b})"))
        .collect();
    format!(
        "aevalsrc=exprs='if({},0.5*sin(2*PI*{freq}*t),0)':s=48000:d={duration}",
        cond.join("+")
    )
}

const BASIC_SRT: &str = "1\n00:00:02,000 --> 00:00:02,500\n食べました\n\n2\n00:00:05,000 --> 00:00:05,500\n日本語を勉強しています。\n\n3\n00:00:08,000 --> 00:00:08,500\nラーメンが好き\n";

const BASIC_ASS: &str = "\
[Script Info]
ScriptType: v4.00+
PlayResX: 1280
PlayResY: 720
WrapStyle: 0

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Noto Sans CJK JP,48,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,1,2,20,20,30,1
Style: Top,Noto Sans CJK JP,40,&H0000FFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,8,20,20,30,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:02.00,0:00:02.50,Default,,0,0,0,,食べました
Dialogue: 0,0:00:05.00,0:00:05.50,Top,,0,0,0,,{\\an8}日本語を\\N勉強しています。
Dialogue: 1,0:00:08.00,0:00:08.50,Default,,0,0,0,,{\\pos(640,360)\\c&H00FF00&}ラーメンが{\\b1}好き{\\b0}
";

const BASIC_CHAPTERS: &str = "\
;FFMETADATA1
[CHAPTER]
TIMEBASE=1/1000
START=0
END=4000
title=Chapter 1
[CHAPTER]
TIMEBASE=1/1000
START=4000
END=7000
title=Chapter 2
[CHAPTER]
TIMEBASE=1/1000
START=7000
END=12000
title=Chapter 3
";

/// The media of TEST §4.1.
fn media(out: &Path, perf: bool) -> Result<()> {
    let src = out.join("src");
    let srt = src.join("av_basic.ja.srt");
    let ass = src.join("av_basic.ja.ass");
    let chapters = src.join("av_basic.chapters.txt");
    std::fs::write(&srt, BASIC_SRT)?;
    std::fs::write(&ass, BASIC_ASS)?;
    std::fs::write(&chapters, BASIC_CHAPTERS)?;
    let file = |p: &Path| format!("file:{}", p.display());
    let windows = [(2.0, 2.5), (5.0, 5.5), (8.0, 8.5)];

    let mut basic = s(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1280x720:rate=30:duration=12",
    ]);
    basic.extend(s(&["-f", "lavfi", "-i"]));
    basic.push(tones(1000, &windows, 12));
    basic.extend(s(&["-f", "lavfi", "-i"]));
    basic.push(tones(660, &windows, 12));
    basic.extend(["-i".into(), file(&srt), "-i".into(), file(&ass)]);
    basic.extend([
        "-f".into(),
        "ffmetadata".into(),
        "-i".into(),
        file(&chapters),
    ]);
    basic.extend(s(&[
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
        "-map",
        "3:s",
        "-map",
        "4:s",
        "-map_chapters",
        "5",
        "-c:v",
        "mpeg4",
        "-q:v",
        "4",
        "-g",
        "30",
        "-c:a",
        "aac",
        "-b:a",
        "96k",
        "-c:s:0",
        "srt",
        "-c:s:1",
        "ass",
        "-metadata:s:a:0",
        "language=jpn",
        "-metadata:s:a:0",
        "title=Japanese",
        "-metadata:s:a:1",
        "language=eng",
        "-metadata:s:a:1",
        "title=English",
        "-metadata:s:s:0",
        "language=jpn",
        "-metadata:s:s:0",
        "title=SRT",
        "-metadata:s:s:1",
        "language=jpn",
        "-metadata:s:s:1",
        "title=ASS",
    ]));
    encode(
        out,
        "av_basic.mkv",
        &basic,
        &[BASIC_SRT, BASIC_ASS, BASIC_CHAPTERS],
    )?;

    encode(
        out,
        "av_noaudio.mp4",
        &s(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30:duration=5",
            "-c:v",
            "mpeg4",
            "-q:v",
            "4",
        ]),
        &[],
    )?;

    // 90 frames at 30 fps, then 90 at 15 fps: 9 s at variable frame rate.
    encode(
        out,
        "av_vfr.mkv",
        &s(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30:duration=6",
            "-vf",
            "setpts='if(lt(N,90),N/30,3+(N-90)/15)/TB'",
            "-fps_mode",
            "vfr",
            "-c:v",
            "mpeg4",
            "-q:v",
            "4",
        ]),
        &[],
    )?;

    let mut long = s(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=24:duration=600",
        "-f",
        "lavfi",
        "-i",
    ]);
    long.push("aevalsrc=exprs='if(lt(mod(t,30),0.5),0.5*sin(2*PI*1000*t),0)':s=48000:d=600".into());
    long.extend(s(&[
        "-c:v",
        "mpeg4",
        "-q:v",
        "6",
        "-c:a",
        "aac",
        "-b:a",
        "64k",
        "-metadata:s:a:0",
        "language=jpn",
    ]));
    encode(out, "av_long.mkv", &long, &[])?;

    if perf {
        let input = s(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1920x1080:rate=60:duration=300",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=300",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
        ]);
        let mut h264 = input.clone();
        h264.extend(s(&[
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-g",
            "120",
        ]));
        encode(out, "av_hd60_h264.mkv", &h264, &[])?;
        let mut hevc = input;
        hevc.extend(s(&[
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p10le",
            "-x265-params",
            "log-level=error:keyint=120",
        ]));
        encode(out, "av_hd60_hevc10.mkv", &hevc, &[])?;
    }

    // Copies with hostile names (EXT-002, T-SEC-005) and corrupt files (PLY-012, NFR-REL-02).
    let basic_bytes = std::fs::read(out.join("av_basic.mkv"))?;
    for name in ["-weird: name & spaces ü 日本.mkv", "http:fake.mkv"] {
        write_if_changed(&out.join(name), &basic_bytes)?;
    }
    write_if_changed(
        &out.join("corrupt_truncated.mkv"),
        &basic_bytes[..basic_bytes.len() * 2 / 5],
    )?;
    let mut rng = SplitMix64::new(7);
    let mut garbage = basic_bytes.clone();
    let mut pos = 16 * 1024;
    while pos + 4096 <= garbage.len() {
        for b in &mut garbage[pos..pos + 4096] {
            *b = rng.next_u64() as u8;
        }
        pos += 64 * 1024;
    }
    write_if_changed(&out.join("corrupt_garbage.mkv"), &garbage)?;
    let mut header = basic_bytes;
    for b in header.iter_mut().take(256) {
        *b = rng.next_u64() as u8;
    }
    write_if_changed(&out.join("corrupt_header.mkv"), &header)?;
    Ok(())
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// Deterministic ZIP options: deflate and a fixed date (1980-01-01).
pub fn zip_options(level: i64) -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(level))
        .last_modified_time(zip::DateTime::default())
}

/// Every `tests/fixtures/dicts/<name>/` directory with an `index.json` becomes
/// `target/fixtures/dicts/<name>.zip` (TEST §4.3).
fn zip_dict_sources(src: &Path, out: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(out)?;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(src)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("index.json").is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut files = Vec::new();
        collect_files(&dir, &mut files)?;
        files.sort();
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = out.join(format!("{name}.zip"));
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for f in &files {
                let inner = f.strip_prefix(&dir)?.to_string_lossy().replace('\\', "/");
                zip.start_file(inner, zip_options(6))?;
                zip.write_all(&std::fs::read(f)?)?;
            }
            zip.finish()?;
        }
        write_if_changed(&target, buf.get_ref())?;
    }
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            collect_files(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}
