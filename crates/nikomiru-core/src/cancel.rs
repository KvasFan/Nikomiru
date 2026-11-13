// SPDX-License-Identifier: GPL-3.0-only

//! A cancellation token shared by long jobs: ffmpeg processes (EXT-003) and
//! dictionary imports (DIC-010).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Lets another thread stop a running job. Clones share the same flag.
#[derive(Debug, Clone, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
    parents: Vec<Cancel>,
}

impl Cancel {
    /// A token that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops the jobs that watch this token.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// A separate token stopped by either parent; cancelling it leaves parents alone.
    pub fn linked(&self, other: &Self) -> Self {
        Self {
            flag: Arc::default(),
            parents: vec![self.clone(), other.clone()],
        }
    }

    /// Whether [`Cancel::cancel`] was called.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst) || self.parents.iter().any(Self::is_cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-019: independent requests share shutdown, never each other's flags.
    #[test]
    fn linked_requests_stop_independently_and_on_shutdown() {
        let stop = Cancel::new();
        let a = Cancel::new();
        let b = Cancel::new();
        let job_a = stop.linked(&a);
        let job_b = stop.linked(&b);
        a.cancel();
        assert!(job_a.is_cancelled());
        assert!(!job_b.is_cancelled());
        assert!(!stop.is_cancelled());
        stop.cancel();
        assert!(job_b.is_cancelled());
    }
}
