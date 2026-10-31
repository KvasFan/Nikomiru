// SPDX-License-Identifier: GPL-3.0-only

//! External processes (EXT-002, EXT-003): a program and an argument vector,
//! never a shell; a timeout and a cancellation token; the files a failed or
//! cancelled job was writing are removed.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Default timeout of a job (EXT-003).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

pub use nikomiru_core::Cancel;

/// What starts the address of the relay of a media server (SRV-009). An
/// actual file's path is absolute, so it cannot start like this.
pub const RELAY_PREFIX: &str = "http://127.0.0.1:";

/// `file:` + path: an input that starts with `-` or contains `:` cannot be
/// taken for an option or a protocol (EXT-002). The address of a media
/// server's relay stays as it is.
pub fn file_arg(path: &Path) -> OsString {
    if path.as_os_str().to_string_lossy().starts_with(RELAY_PREFIX) {
        return path.as_os_str().to_os_string();
    }
    let mut arg = OsString::from("file:");
    arg.push(path.as_os_str());
    arg
}

/// A command to run.
#[derive(Debug, Clone)]
pub struct Job {
    /// The executable.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<OsString>,
    /// How long it may run.
    pub timeout: Duration,
    /// Files it writes, removed if it does not succeed.
    pub outputs: Vec<PathBuf>,
}

impl Job {
    /// A job with the default timeout and no outputs.
    pub fn new(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args
                .into_iter()
                .map(|a| a.as_ref().to_os_string())
                .collect(),
            timeout: DEFAULT_TIMEOUT,
            outputs: Vec::new(),
        }
    }
}

/// Why a job did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// The program could not be started.
    #[error("{program}: {source}")]
    Spawn {
        /// The program.
        program: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// It ran longer than its timeout and was killed.
    #[error("{program} was stopped after {} s", timeout.as_secs())]
    Timeout {
        /// The program.
        program: PathBuf,
        /// The timeout.
        timeout: Duration,
    },
    /// It was cancelled and killed.
    #[error("{program} was cancelled")]
    Cancelled {
        /// The program.
        program: PathBuf,
    },
    /// It exited with an error.
    #[error("{program} failed ({status}): {stderr}")]
    Failed {
        /// The program.
        program: PathBuf,
        /// The exit status.
        status: String,
        /// The end of its standard error.
        stderr: String,
    },
}

impl nikomiru_core::Localized for JobError {
    fn message_id(&self) -> &'static str {
        match self {
            JobError::Spawn { .. } => "error-job-spawn",
            JobError::Timeout { .. } => "error-job-timeout",
            JobError::Cancelled { .. } => "error-job-cancelled",
            JobError::Failed { .. } => "error-job-failed",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        let name = |p: &Path| p.display().to_string();
        match self {
            JobError::Spawn { program, source } => {
                vec![("program", name(program)), ("reason", source.to_string())]
            }
            JobError::Timeout { program, timeout } => vec![
                ("program", name(program)),
                ("seconds", timeout.as_secs().to_string()),
            ],
            JobError::Cancelled { program } => vec![("program", name(program))],
            JobError::Failed {
                program, stderr, ..
            } => vec![("program", name(program)), ("reason", stderr.clone())],
        }
    }
}

/// Starts `cmd`. An executable that another process still has open for
/// writing fails with `ETXTBSY` (a package update, or a fork that briefly
/// inherited the descriptor): that is retried for a short while.
pub(crate) fn spawn(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 20 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            result => return result,
        }
    }
}

/// Keeps the last `limit` bytes of standard error.
const STDERR_TAIL: usize = 2048;

/// The standard output kept by [`output`] at most.
const STDOUT_LIMIT: u64 = 64 * 1024;

/// Runs `job` until it exits, times out or `cancel` fires.
pub fn run(job: &Job, cancel: &Cancel) -> Result<(), JobError> {
    execute(job, cancel, false).map(drop)
}

/// Like [`run`], and returns the first 64 KiB of its standard output.
pub fn output(job: &Job, cancel: &Cancel) -> Result<String, JobError> {
    execute(job, cancel, true)
}

fn execute(job: &Job, cancel: &Cancel, capture: bool) -> Result<String, JobError> {
    let remove_outputs = || {
        for o in &job.outputs {
            let _ = std::fs::remove_file(o);
        }
    };
    let mut child = spawn(
        Command::new(&job.program)
            .args(&job.args)
            .stdin(Stdio::null())
            .stdout(if capture {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(Stdio::piped()),
    )
    .map_err(|source| JobError::Spawn {
        program: job.program.clone(),
        source,
    })?;
    let stdout = child.stdout.take();
    let out_reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(mut s) = stdout {
            let _ = (&mut s).take(STDOUT_LIMIT).read_to_end(&mut text);
            // the rest is read and dropped: a full pipe would stop the program
            let _ = std::io::copy(&mut s, &mut std::io::sink());
        }
        String::from_utf8_lossy(&text).into_owned()
    });
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut text);
        }
        let start = text.len().saturating_sub(STDERR_TAIL);
        String::from_utf8_lossy(&text[start..]).trim().to_string()
    });
    let deadline = Instant::now() + job.timeout;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        let stop = if cancel.is_cancelled() {
            Some(JobError::Cancelled {
                program: job.program.clone(),
            })
        } else if Instant::now() >= deadline {
            Some(JobError::Timeout {
                program: job.program.clone(),
                timeout: job.timeout,
            })
        } else {
            None
        };
        if let Some(error) = stop {
            let _ = child.kill();
            let _ = child.wait();
            remove_outputs();
            return Err(error);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stderr = reader.join().unwrap_or_default();
    let stdout = out_reader.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        remove_outputs();
        Err(JobError::Failed {
            program: job.program.clone(),
            status: status.to_string(),
            stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        use nikomiru_core::Localized;
        let program = || PathBuf::from("/usr/bin/ffmpeg");
        let errors = [
            JobError::Spawn {
                program: program(),
                source: std::io::Error::other("x"),
            },
            JobError::Timeout {
                program: program(),
                timeout: DEFAULT_TIMEOUT,
            },
            JobError::Cancelled { program: program() },
            JobError::Failed {
                program: program(),
                status: "exit status: 1".into(),
                stderr: "x".into(),
            },
        ];
        let all: Vec<&dyn Localized> = errors.iter().map(|e| e as _).collect();
        let dir = Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    // T-SRV-008
    fn the_relay_is_not_a_file() {
        let relay = Path::new("http://127.0.0.1:4000/abc/video");
        assert_eq!(
            file_arg(relay),
            OsString::from("http://127.0.0.1:4000/abc/video")
        );
        let local = Path::new("http:/127.0.0.1/x");
        assert_eq!(file_arg(local), OsString::from("file:http:/127.0.0.1/x"));
    }

    #[test]
    fn inputs_get_the_file_prefix() {
        assert_eq!(
            file_arg(Path::new("-i x:y.mkv")),
            OsString::from("file:-i x:y.mkv")
        );
    }
}
