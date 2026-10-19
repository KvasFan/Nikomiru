// SPDX-License-Identifier: GPL-3.0-only

//! Log files in `$XDG_STATE_HOME/nikomiru/logs/`: text, daily rotation, 7 files
//! (SPECIFICATION §5.5). libmpv's messages arrive with the `mpv` target
//! (§5.2.2 #11). PRV-002: no subtitle text and no API keys are logged; paths are.

use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Log files kept by the daily rotation.
const KEPT_FILES: usize = 7;

/// Which messages are written.
pub fn filter(debug: bool) -> Targets {
    let (ours, mpv) = if debug {
        (Level::DEBUG, Level::DEBUG)
    } else {
        (Level::INFO, Level::WARN)
    };
    Targets::new().with_default(ours).with_target("mpv", mpv)
}

/// Starts logging to `dir`, or to stderr if the folder cannot be used. The guard
/// flushes the file when dropped, so it must live until the end of `main`.
pub fn init(dir: Option<&Path>, debug: bool) -> Option<WorkerGuard> {
    let appender = dir.and_then(|dir| {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .ok()?;
        RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("nikomiru")
            .filename_suffix("log")
            .max_log_files(KEPT_FILES)
            .build(dir)
            .ok()
    });
    let registry = tracing_subscriber::registry().with(filter(debug));
    let guard = match appender {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            let _ = registry.with(layer).try_init();
            Some(guard)
        }
        None => {
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr);
            let _ = registry.with(layer).try_init();
            tracing::warn!("the log folder is not writable: logging to stderr");
            None
        }
    };
    install_panic_hook();
    guard
}

/// Panics go to the log too, before the usual message on stderr.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::level_filters::LevelFilter;

    #[test]
    fn mpv_is_quieter_unless_debugging() {
        let normal = filter(false);
        assert!(normal.would_enable("nikomiru_ui::session", &Level::INFO));
        assert!(!normal.would_enable("nikomiru_ui::session", &Level::DEBUG));
        assert!(!normal.would_enable("mpv", &Level::INFO));
        assert!(normal.would_enable("mpv", &Level::WARN));
        let debug = filter(true);
        assert!(debug.would_enable("mpv", &Level::DEBUG));
        assert_eq!(debug.default_level(), Some(LevelFilter::DEBUG));
    }
}
