// SPDX-License-Identifier: GPL-3.0-only

//! A cancellation token shared by long jobs: ffmpeg processes (EXT-003) and
//! dictionary imports (DIC-010).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Lets another thread stop a running job. Clones share the same flag.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops the jobs that watch this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether [`Cancel::cancel`] was called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
