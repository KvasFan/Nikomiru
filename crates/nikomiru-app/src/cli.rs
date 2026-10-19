// SPDX-License-Identifier: GPL-3.0-only

//! Command line: `nikomiru [--debug] [FILE | URL]` (LIB-001, APP-001).

use std::ffi::OsString;
use std::path::Path;

/// Help text for `--help` and for invalid arguments.
pub const USAGE: &str = "\
Usage: nikomiru [OPTIONS] [FILE | URL]

Opens FILE (a video) or URL (file://, nikomiru://) in Nikomiru. If Nikomiru is
already running, the running window opens it.

Options:
      --import-dictionary ZIP
                  Imports a Yomitan dictionary (a .zip; repeatable) and exits.
                  An older revision of the same dictionary is updated.
      --debug     Verbose log, libmpv's messages included
  -h, --help      Show this help
  -V, --version   Show the version
";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start (or hand the targets to the running instance).
    Run(Options),
    /// Print the help.
    Help,
    /// Print the version.
    Version,
    /// Import these dictionaries and exit.
    ImportDictionaries(Vec<std::path::PathBuf>),
}

/// Options of a normal start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Files (absolute paths) and URLs to open, in order.
    pub targets: Vec<String>,
    /// Verbose log.
    pub debug: bool,
    /// Socket of the GUI test harness (TEST §2.1).
    #[cfg(feature = "test-harness")]
    pub harness: Option<std::path::PathBuf>,
}

/// Parses the arguments (without the program name). Relative paths are made
/// absolute against `cwd`, so another instance can open them.
// `--test-harness` takes the next argument from inside the loop
#[cfg_attr(not(feature = "test-harness"), allow(clippy::while_let_on_iterator))]
pub fn parse(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Result<Command, String> {
    let mut options = Options::default();
    let mut only_targets = false;
    let mut imports = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let Some(arg) = arg.to_str().map(str::to_string) else {
            return Err(format!(
                "{}: the path is not valid UTF-8",
                arg.to_string_lossy()
            ));
        };
        if !only_targets && arg.starts_with('-') && arg != "-" {
            match arg.as_str() {
                "--" => only_targets = true,
                "-h" | "--help" => return Ok(Command::Help),
                "-V" | "--version" => return Ok(Command::Version),
                "--debug" => options.debug = true,
                "--import-dictionary" => {
                    let zip = args.next().ok_or("--import-dictionary needs a .zip file")?;
                    let zip = std::path::PathBuf::from(zip);
                    imports.push(if zip.is_absolute() {
                        zip
                    } else {
                        cwd.join(zip)
                    });
                }
                #[cfg(feature = "test-harness")]
                "--test-harness" => {
                    let socket = args.next().ok_or("--test-harness needs a socket path")?;
                    options.harness = Some(socket.into());
                }
                _ => return Err(format!("unknown option {arg}")),
            }
            continue;
        }
        options.targets.push(target(&arg, cwd));
    }
    if !imports.is_empty() {
        return Ok(Command::ImportDictionaries(imports));
    }
    Ok(Command::Run(options))
}

/// A URL as it is, a path made absolute.
fn target(arg: &str, cwd: &Path) -> String {
    let is_url = arg.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    if is_url || Path::new(arg).is_absolute() {
        return arg.to_string();
    }
    cwd.join(arg).to_string_lossy().into_owned()
}

#[cfg(test)]
// `..Options::default()` fills the harness field, which exists only with the feature
#[cfg_attr(not(feature = "test-harness"), allow(clippy::needless_update))]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from), Path::new("/home/u/videos"))
    }

    #[test]
    fn targets_become_absolute_and_urls_stay() {
        assert_eq!(
            run(&[
                "ep1.mkv",
                "/a/b.mp4",
                "file:///c/d.mkv",
                "nikomiru://open?uid=x",
                "--debug"
            ]),
            Ok(Command::Run(Options {
                targets: vec![
                    "/home/u/videos/ep1.mkv".into(),
                    "/a/b.mp4".into(),
                    "file:///c/d.mkv".into(),
                    "nikomiru://open?uid=x".into(),
                ],
                debug: true,
                ..Options::default()
            }))
        );
    }

    #[test]
    fn options_help_version_and_errors() {
        assert_eq!(run(&[]), Ok(Command::Run(Options::default())));
        assert_eq!(run(&["--version", "x.mkv"]), Ok(Command::Version));
        assert_eq!(run(&["-h"]), Ok(Command::Help));
        assert_eq!(
            run(&[
                "--import-dictionary",
                "jmdict.zip",
                "--import-dictionary",
                "/d/k.zip"
            ]),
            Ok(Command::ImportDictionaries(vec![
                "/home/u/videos/jmdict.zip".into(),
                "/d/k.zip".into()
            ]))
        );
        assert!(run(&["--import-dictionary"]).is_err());
        assert!(run(&["--fullscreen"]).is_err());
        // after `--` a name starting with a dash is a file
        assert_eq!(
            run(&["--", "-strange.mkv"]),
            Ok(Command::Run(Options {
                targets: vec!["/home/u/videos/-strange.mkv".into()],
                ..Options::default()
            }))
        );
    }

    #[test]
    fn non_utf8_paths_are_refused() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![b'a', 0xff, b'.', b'm']);
        assert!(parse([bad], Path::new("/")).is_err());
    }
}
