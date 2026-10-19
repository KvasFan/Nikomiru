// SPDX-License-Identifier: GPL-3.0-only

//! The Nikomiru binary: `QGuiApplication`, the QML engine, single instance and the
//! wiring between the crates (D-10).
//!
//! The pieces that do not need Qt live in the library half of this crate.

#![deny(unsafe_code)]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use nikomiru_app::inhibit::PortalInhibitor;
use nikomiru_app::{cli, instance, linux, logging};
use nikomiru_core::{Config, ConfigError, Paths, SystemClock};
use nikomiru_library::Library;
use nikomiru_player::{Player, VideoOutput};
use nikomiru_ui::Services;
use nikomiru_ui::i18n::Translator;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The main window, compiled into the `Nikomiru.UI` QML module.
const MAIN_QML: &str = "qrc:/qt/qml/Nikomiru/UI/qml/Main.qml";

fn main() -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_default();
    let options = match cli::parse(std::env::args_os().skip(1), &cwd) {
        Ok(cli::Command::Run(o)) => o,
        Ok(cli::Command::Help) => {
            print!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Version) => {
            println!("nikomiru {VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("nikomiru: {e}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    linux::prepare_environment();

    let debug = options.debug || std::env::var("NIKOMIRU_LOG").is_ok_and(|v| v == "debug");
    let paths = Paths::from_env();
    let logs = paths.as_ref().ok().map(Paths::logs_dir);
    let _log = logging::init(logs.as_deref(), debug);
    let paths = match paths {
        Ok(p) => p,
        Err(e) => return fatal(&e),
    };
    tracing::info!("Nikomiru {VERSION}");

    // APP-001
    let request = instance::Request {
        open: options.targets.clone(),
    };
    let server = match paths.socket_path() {
        None => {
            tracing::warn!("XDG_RUNTIME_DIR is not set: no single-instance check");
            None
        }
        Some(socket) => match instance::acquire(&socket, &request) {
            Ok(instance::Role::Forwarded) => {
                tracing::info!("targets handed to the running instance");
                return ExitCode::SUCCESS;
            }
            Ok(instance::Role::Primary(server)) => Some(server),
            Err(e @ instance::InstanceError::NoAnswer(_)) => return fatal(&e),
            Err(e) => {
                tracing::warn!("single-instance check unavailable: {e}");
                None
            }
        },
    };
    run(&paths, &options, server)
}

/// The first termination signal closes the window like the user would, so the
/// position is saved (LIB-006); a second one exits at once.
fn watch_termination_signals() {
    let mut signals = match linux::catch_termination_signals() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("termination signals: {e}");
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("signals".into())
        .spawn(move || {
            let mut asked = false;
            while let Some(signal) = signals.wait() {
                if asked {
                    tracing::warn!("signal {signal} again: exiting now");
                    std::process::exit(128 + signal);
                }
                tracing::info!("signal {signal}: quitting");
                nikomiru_ui::request_quit();
                asked = true;
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("signal thread: {e}");
    }
}

/// EXT-001: finds ffmpeg and ffprobe and logs what they can do, on a thread so
/// that a slow executable does not delay the window. Nothing in M1 uses them
/// yet; from M2 the result goes to the features that need it.
fn check_ffmpeg(setting: &str) {
    let search = nikomiru_media::tools::Search::from_env(setting);
    let spawned = std::thread::Builder::new()
        .name("ffmpeg-probe".into())
        .spawn(move || match nikomiru_media::tools::probe(&search) {
            Ok(tools) => {
                tracing::info!(
                    "ffmpeg {} at {} ({:?})",
                    tools.version,
                    tools.ffmpeg.display(),
                    tools.origin
                );
                for m in tools.missing_encoders() {
                    tracing::warn!(
                        "ffmpeg has no {} encoder: the {} format is disabled",
                        m.encoder,
                        m.format
                    );
                }
            }
            Err(e) => tracing::warn!("{e}: subtitle extraction and card media are disabled"),
        });
    if let Err(e) = spawned {
        tracing::warn!("ffmpeg probe thread: {e}");
    }
}

/// Logs a startup error, prints it and fails.
fn fatal(e: &dyn std::fmt::Display) -> ExitCode {
    tracing::error!("{e}");
    eprintln!("nikomiru: {e}");
    ExitCode::FAILURE
}

/// The settings, or the defaults and the error to show (ERR-001).
fn load_config(paths: &Paths) -> (Config, Option<ConfigError>) {
    match Config::load(&paths.config_file()) {
        Ok(config) => {
            let unknown = config.unknown_keys();
            if !unknown.is_empty() {
                tracing::info!("settings this version does not know: {unknown:?}");
            }
            (config, None)
        }
        Err(e) => {
            tracing::warn!("{e}");
            (Config::default(), Some(e))
        }
    }
}

fn run(paths: &Paths, options: &cli::Options, server: Option<instance::Server>) -> ExitCode {
    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut()
            .set_application_name(&QString::from("nikomiru"));
        app.as_mut()
            .set_application_display_name(&QString::from("Nikomiru"));
        app.as_mut()
            .set_application_version(&QString::from(VERSION));
    }
    linux::force_c_numeric_locale();
    nikomiru_player::video::init();

    let (config, config_notice) = load_config(paths);
    check_ffmpeg(&config.paths.ffmpeg);
    let translator =
        Translator::from_setting(&config.general.ui_language, |k| std::env::var(k).ok());
    let mut notices = Vec::new();
    if let Some(e) = config_notice {
        notices.push(translator.error(&e));
    }
    let library = match Library::open(&paths.library_db()) {
        Ok(library) => {
            if library.read_only() {
                tracing::warn!("library schema {} is newer: read-only", library.schema());
                notices.push(translator.tr("error-library-read-only", &[]));
            }
            Some(library)
        }
        Err(e) => {
            tracing::error!("{e}");
            notices.push(translator.error(&e));
            None
        }
    };

    let player = match Player::new(VideoOutput::RenderApi, nikomiru_ui::deliver) {
        Ok(player) => player,
        Err(e) => {
            tracing::error!("{e}");
            eprintln!("nikomiru: {}", translator.error(&e));
            return ExitCode::FAILURE;
        }
    };
    nikomiru_player::video::set_core(&player);
    let inhibitor = PortalInhibitor::start(translator.tr("inhibit-reason", &[]));
    let subtitle_source = nikomiru_app::subtitles::Loader::start(
        Translator::from_setting(&config.general.ui_language, |k| std::env::var(k).ok()),
        config.paths.ffmpeg.clone(),
        paths.cache_dir.join("subtitles"),
    );
    nikomiru_ui::install(Services {
        player: player.handle(),
        config,
        library,
        translator,
        clock: Box::new(SystemClock::new()),
        notices,
        inhibitor: Box::new(inhibitor),
        subtitle_source: Box::new(subtitle_source),
    });
    let instance = server.map(|server| {
        server.serve(|request| {
            if let Some(target) = request.open.into_iter().next() {
                nikomiru_ui::request_open(target);
            }
            nikomiru_ui::request_activate();
        })
    });
    if let Some(target) = options.targets.first() {
        nikomiru_ui::request_open(target.clone());
    }
    watch_termination_signals();
    #[cfg(feature = "test-harness")]
    if let Some(socket) = &options.harness {
        nikomiru_app::harness::start(socket);
    }

    let failed = Arc::new(AtomicBool::new(false));
    let mut engine = QQmlApplicationEngine::new();
    let connection = engine.as_mut().map(|mut engine| {
        let failed = failed.clone();
        let connection = engine.as_mut().on_object_creation_failed(move |_, url| {
            tracing::error!("QML: cannot create {url}");
            failed.store(true, Ordering::SeqCst);
        });
        engine.as_mut().load(&QUrl::from(MAIN_QML));
        connection
    });
    let code = match app.as_mut() {
        Some(app) if connection.is_some() && !failed.load(Ordering::SeqCst) => app.exec(),
        _ => 1,
    };

    // Shutdown order (SPECIFICATION §5.2.2): the window, the controller and the
    // video item's render context go first, then the mpv core.
    drop(connection);
    drop(engine);
    player.shutdown();
    drop(instance);
    tracing::info!("exit {code}");
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
