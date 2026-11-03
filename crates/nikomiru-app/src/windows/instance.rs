// SPDX-License-Identifier: GPL-3.0-only

//! One instance at a time (APP-001), on Windows: not yet; a named pipe would
//! take the place of the Unix socket (spike S6). Every launch is its own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors of the single-instance mechanism.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    /// Not on this system.
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// Another instance holds the lock but did not answer.
    #[error("another Nikomiru is running but does not answer on {0}")]
    NoAnswer(PathBuf),
}

impl nikomiru_core::Localized for InstanceError {
    fn message_id(&self) -> &'static str {
        match self {
            InstanceError::Io { .. } => "error-instance-io",
            InstanceError::NoAnswer(_) => "error-instance-no-answer",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            InstanceError::Io { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            InstanceError::NoAnswer(_) => Vec::new(),
        }
    }
}

/// What a launch sends to the running instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Files (absolute paths) and URLs to open.
    pub open: Vec<String>,
}

/// The outcome of [`acquire`].
#[derive(Debug)]
pub enum Role {
    /// This is the first instance.
    Primary(Server),
    /// The request went to the running instance: this process can exit.
    Forwarded,
}

/// Never made on Windows yet.
#[derive(Debug)]
pub struct Server(());

/// Holds nothing.
#[derive(Debug)]
pub struct Guard;

/// Always unavailable on Windows yet.
pub fn acquire(socket: &Path, _request: &Request) -> Result<Role, InstanceError> {
    Err(InstanceError::Io {
        path: socket.to_path_buf(),
        source: std::io::ErrorKind::Unsupported.into(),
    })
}

impl Server {
    /// Serves nothing.
    pub fn serve(self, _on_request: impl Fn(Request) + Send + 'static) -> Guard {
        Guard
    }
}
