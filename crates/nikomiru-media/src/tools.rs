// SPDX-License-Identifier: GPL-3.0-only

//! Discovery of ffmpeg and ffprobe and of their encoders (EXT-001). The
//! executables are looked for in the `paths.ffmpeg` setting, then in the
//! `NIKOMIRU_FFMPEG` variable, then in `PATH`. The version and the encoder list
//! decide which features can work; a missing encoder disables its format and
//! the next one takes its place (R-4).

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Oldest supported FFmpeg major version (SPECIFICATION §2).
pub const MIN_MAJOR: u32 = 6;

/// How long `-version` and `-encoders` may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the executables were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The `paths.ffmpeg` setting.
    Setting,
    /// The `NIKOMIRU_FFMPEG` variable.
    Environment,
    /// `PATH`.
    Path,
}

/// Errors of the discovery.
#[derive(Debug, thiserror::Error)]
pub enum ToolsError {
    /// No ffmpeg in the setting, in `NIKOMIRU_FFMPEG` or in `PATH`.
    #[error("ffmpeg was not found (paths.ffmpeg, NIKOMIRU_FFMPEG, PATH)")]
    NotFound,
    /// ffmpeg was found, ffprobe was not.
    #[error("ffprobe was not found next to {ffmpeg}")]
    NoFfprobe {
        /// The ffmpeg that was found.
        ffmpeg: PathBuf,
    },
    /// An executable could not be run.
    #[error("{path}: {source}")]
    Run {
        /// The executable.
        path: PathBuf,
        /// The error.
        source: io::Error,
    },
    /// An executable did not answer in time.
    #[error("{path} did not answer within {} s", PROBE_TIMEOUT.as_secs())]
    Timeout {
        /// The executable.
        path: PathBuf,
    },
    /// The version is older than [`MIN_MAJOR`].
    #[error("FFmpeg {version} is too old: {MIN_MAJOR}.0 or newer is needed")]
    TooOld {
        /// The version reported by ffmpeg.
        version: String,
    },
}

impl nikomiru_core::Localized for ToolsError {
    fn message_id(&self) -> &'static str {
        match self {
            ToolsError::NotFound => "error-ffmpeg-missing",
            ToolsError::NoFfprobe { .. } => "error-ffprobe-missing",
            ToolsError::Run { .. } => "error-ffmpeg-run",
            ToolsError::Timeout { .. } => "error-ffmpeg-timeout",
            ToolsError::TooOld { .. } => "error-ffmpeg-old",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            ToolsError::NotFound => Vec::new(),
            ToolsError::NoFfprobe { ffmpeg } => vec![("path", ffmpeg.display().to_string())],
            ToolsError::Run { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            ToolsError::Timeout { path } => vec![
                ("path", path.display().to_string()),
                ("seconds", PROBE_TIMEOUT.as_secs().to_string()),
            ],
            ToolsError::TooOld { version } => vec![
                ("version", version.clone()),
                ("minimum", format!("{MIN_MAJOR}.0")),
            ],
        }
    }
}

/// Where to look for the executables.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// The `paths.ffmpeg` setting (empty: not set).
    pub setting: String,
    /// `NIKOMIRU_FFMPEG`.
    pub env: Option<OsString>,
    /// `PATH`.
    pub path: Option<OsString>,
}

impl Search {
    /// The setting plus this process's environment.
    pub fn from_env(setting: &str) -> Self {
        Self {
            setting: setting.to_string(),
            env: std::env::var_os("NIKOMIRU_FFMPEG"),
            path: std::env::var_os("PATH"),
        }
    }
}

/// Sentence audio formats (ANK-033), in fallback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// AAC in M4A (D-4, the default).
    AacM4a,
    /// Opus in Ogg.
    OpusOgg,
    /// MP3.
    Mp3,
}

impl AudioFormat {
    /// All formats, in fallback order.
    pub const ALL: [AudioFormat; 3] = [AudioFormat::AacM4a, AudioFormat::OpusOgg, AudioFormat::Mp3];

    /// From the `anki.audio_format` setting.
    pub fn from_config(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.config_name() == name)
    }

    /// Name in the settings.
    pub fn config_name(self) -> &'static str {
        match self {
            AudioFormat::AacM4a => "aac-m4a",
            AudioFormat::OpusOgg => "opus-ogg",
            AudioFormat::Mp3 => "mp3",
        }
    }

    /// The ffmpeg encoder it needs.
    pub fn encoder(self) -> &'static str {
        match self {
            AudioFormat::AacM4a => "aac",
            AudioFormat::OpusOgg => "libopus",
            AudioFormat::Mp3 => "libmp3lame",
        }
    }
}

/// Card image formats (ANK-034), in fallback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG (the default).
    Jpeg,
    /// WebP.
    Webp,
}

impl ImageFormat {
    /// All formats, in fallback order.
    pub const ALL: [ImageFormat; 2] = [ImageFormat::Jpeg, ImageFormat::Webp];

    /// From the `anki.image_format` setting.
    pub fn from_config(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.config_name() == name)
    }

    /// Name in the settings.
    pub fn config_name(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Webp => "webp",
        }
    }

    /// The ffmpeg encoder it needs.
    pub fn encoder(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "mjpeg",
            ImageFormat::Webp => "libwebp",
        }
    }
}

/// A format that cannot be produced because ffmpeg lacks its encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEncoder {
    /// The format, as named in the settings.
    pub format: &'static str,
    /// The encoder ffmpeg does not have.
    pub encoder: &'static str,
}

/// The executables that were found, their version and encoders.
#[derive(Debug, Clone)]
pub struct Tools {
    /// ffmpeg.
    pub ffmpeg: PathBuf,
    /// ffprobe.
    pub ffprobe: PathBuf,
    /// Where they were found.
    pub origin: Origin,
    /// The version reported by `ffmpeg -version`.
    pub version: String,
    /// The encoders listed by `ffmpeg -encoders`.
    pub encoders: BTreeSet<String>,
}

impl Tools {
    /// Whether ffmpeg has `encoder`.
    pub fn has_encoder(&self, encoder: &str) -> bool {
        self.encoders.contains(encoder)
    }

    /// `preferred` if ffmpeg can encode it, otherwise the first available format
    /// in fallback order; `None` if none is available.
    pub fn audio_format(&self, preferred: AudioFormat) -> Option<AudioFormat> {
        std::iter::once(preferred)
            .chain(AudioFormat::ALL)
            .find(|f| self.has_encoder(f.encoder()))
    }

    /// `preferred` if ffmpeg can encode it, otherwise the other format.
    pub fn image_format(&self, preferred: ImageFormat) -> Option<ImageFormat> {
        std::iter::once(preferred)
            .chain(ImageFormat::ALL)
            .find(|f| self.has_encoder(f.encoder()))
    }

    /// The formats this ffmpeg cannot produce (shown as warnings at startup).
    pub fn missing_encoders(&self) -> Vec<MissingEncoder> {
        let audio = AudioFormat::ALL.map(|f| (f.config_name(), f.encoder()));
        let image = ImageFormat::ALL.map(|f| (f.config_name(), f.encoder()));
        audio
            .into_iter()
            .chain(image)
            .filter(|(_, encoder)| !self.has_encoder(encoder))
            .map(|(format, encoder)| MissingEncoder { format, encoder })
            .collect()
    }
}

/// A setting or variable value: an executable, or the folder that holds it.
fn executable_in(value: &Path, name: &str) -> PathBuf {
    if value.is_dir() {
        value.join(name)
    } else {
        value.to_path_buf()
    }
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn in_path(path_var: Option<&OsString>, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var?)
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

/// Finds ffmpeg and ffprobe, in the order of EXT-001. A configured location
/// without an executable is skipped with a warning.
pub fn find(search: &Search) -> Result<(Origin, PathBuf, PathBuf), ToolsError> {
    let configured = [
        (
            Origin::Setting,
            (!search.setting.is_empty()).then(|| OsString::from(&search.setting)),
        ),
        (
            Origin::Environment,
            search.env.clone().filter(|v| !v.is_empty()),
        ),
    ];
    for (origin, value) in configured {
        let Some(value) = value else { continue };
        let ffmpeg = executable_in(Path::new(&value), "ffmpeg");
        if !is_executable(&ffmpeg) {
            tracing::warn!(
                "{origin:?}: {} is not an executable, skipped",
                ffmpeg.display()
            );
            continue;
        }
        let ffprobe = ffmpeg.with_file_name("ffprobe");
        if !is_executable(&ffprobe) {
            return Err(ToolsError::NoFfprobe { ffmpeg });
        }
        return Ok((origin, ffmpeg, ffprobe));
    }
    let ffmpeg = in_path(search.path.as_ref(), "ffmpeg").ok_or(ToolsError::NotFound)?;
    let ffprobe =
        in_path(search.path.as_ref(), "ffprobe").ok_or_else(|| ToolsError::NoFfprobe {
            ffmpeg: ffmpeg.clone(),
        })?;
    Ok((Origin::Path, ffmpeg, ffprobe))
}

/// Runs `program args`, returning its standard output; killed after
/// [`PROBE_TIMEOUT`].
fn output(program: &Path, args: &[&str]) -> Result<String, ToolsError> {
    let run_err = |source| ToolsError::Run {
        path: program.to_path_buf(),
        source,
    };
    let mut child = crate::process::spawn(
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .map_err(run_err)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| run_err(io::Error::other("no standard output")))?;
    // read on a thread, so a chatty child never blocks on a full pipe
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        text
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        if child.try_wait().map_err(run_err)?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolsError::Timeout {
                path: program.to_path_buf(),
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = reader.join().unwrap_or_default();
    Ok(String::from_utf8_lossy(&text).into_owned())
}

/// The version from the first line of `ffmpeg -version` (`ffmpeg version
/// 9.0.2 Copyright…`), and its major number when it has one (git builds such
/// as `N-112233-g…` do not).
pub fn parse_version(text: &str) -> (String, Option<u32>) {
    let version = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("ffmpeg version "))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    let major = version
        .trim_start_matches('n')
        .split(['.', '-'])
        .next()
        .and_then(|m| m.parse().ok());
    (version, major)
}

/// Encoder names from `ffmpeg -encoders`: the lines after ` ------`, each one
/// `<flags> <name> <description>`.
pub fn parse_encoders(text: &str) -> BTreeSet<String> {
    text.lines()
        .skip_while(|l| !l.trim_start().starts_with("------"))
        .skip(1)
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(str::to_string)
        .collect()
}

/// Finds the executables and checks their version and encoders.
pub fn probe(search: &Search) -> Result<Tools, ToolsError> {
    let (origin, ffmpeg, ffprobe) = find(search)?;
    let (version, major) = parse_version(&output(&ffmpeg, &["-hide_banner", "-version"])?);
    match major {
        Some(m) if m < MIN_MAJOR => return Err(ToolsError::TooOld { version }),
        Some(_) => {}
        None => tracing::warn!("ffmpeg version {version:?} not recognized: assuming it is recent"),
    }
    let encoders = parse_encoders(&output(&ffmpeg, &["-hide_banner", "-encoders"])?);
    Ok(Tools {
        ffmpeg,
        ffprobe,
        origin,
        version,
        encoders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_parsed() {
        assert_eq!(
            parse_version("ffmpeg version 9.0.2 Copyright (c) 2000-2026\nbuilt with gcc"),
            ("9.0.2".into(), Some(9))
        );
        assert_eq!(
            parse_version("ffmpeg version n6.1.1-3ubuntu5 Copyright").1,
            Some(6)
        );
        assert_eq!(parse_version("ffmpeg version 5.1.6-0+deb12u1").1, Some(5));
        assert_eq!(parse_version("ffmpeg version N-118000-g1234abcd").1, None);
        assert_eq!(parse_version("garbage").0, "");
    }

    #[test]
    fn encoders_are_parsed() {
        let text = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n \
                    V....D mjpeg                MJPEG (Motion JPEG)\n \
                    A....D aac                  AAC (Advanced Audio Coding)\n \
                    A....D libopus              libopus Opus\n";
        let names = parse_encoders(text);
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            ["aac", "libopus", "mjpeg"]
        );
        assert!(parse_encoders("no table here").is_empty());
    }

    #[test]
    fn formats_fall_back_in_order() {
        let tools = |encoders: &[&str]| Tools {
            ffmpeg: PathBuf::new(),
            ffprobe: PathBuf::new(),
            origin: Origin::Path,
            version: "9.0".into(),
            encoders: encoders.iter().map(|e| e.to_string()).collect(),
        };
        let t = tools(&["aac", "libopus", "libmp3lame", "mjpeg", "libwebp"]);
        assert_eq!(t.audio_format(AudioFormat::Mp3), Some(AudioFormat::Mp3));
        assert!(t.missing_encoders().is_empty());
        let t = tools(&["libmp3lame"]);
        assert_eq!(t.audio_format(AudioFormat::AacM4a), Some(AudioFormat::Mp3));
        assert_eq!(t.image_format(ImageFormat::Jpeg), None);
        assert_eq!(t.missing_encoders().len(), 4);
        assert_eq!(
            AudioFormat::from_config("opus-ogg"),
            Some(AudioFormat::OpusOgg)
        );
        assert_eq!(ImageFormat::from_config("png"), None);
    }

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let path = || PathBuf::from("/usr/bin/ffmpeg");
        let errors = [
            ToolsError::NotFound,
            ToolsError::NoFfprobe { ffmpeg: path() },
            ToolsError::Run {
                path: path(),
                source: io::Error::other("x"),
            },
            ToolsError::Timeout { path: path() },
            ToolsError::TooOld {
                version: "5.1".into(),
            },
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let dir = Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
