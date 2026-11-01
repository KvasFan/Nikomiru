// SPDX-License-Identifier: GPL-3.0-only

//! The L6 environment: an off-screen KWin on a private D-Bus bus, isolated XDG
//! folders, and the harness client (TEST §2.1). Nothing touches the user's
//! session: the bus has no service activation, so no real portal answers, and
//! the runtime folder is private, so no audio server is found either.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Whether the machine has room for a measure of time to mean something:
/// other work on it, such as another project compiling, says nothing of
/// Nikomiru's speed.
pub fn machine_is_free() -> bool {
    let load = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|l| l.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0);
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    load <= cores as f64
}

/// A generated fixture (`cargo xtask fixtures`).
pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(name);
    assert!(
        path.exists(),
        "{} is missing: run `cargo xtask fixtures`",
        path.display()
    );
    path.canonicalize().unwrap().to_string_lossy().into_owned()
}

/// The X displays whose sockets exist (`:0`, `:1`…).
fn x_displays() -> Vec<String> {
    std::fs::read_dir("/tmp/.X11-unix")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix('X').map(|n| format!(":{n}"))
        })
        .collect()
}

/// Polls `f` until it returns `Some`, or panics after `timeout`.
pub fn until<T>(timeout: Duration, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits for a child to exit.
pub fn exit_status(child: &mut Child, timeout: Duration) -> ExitStatus {
    until(timeout, "process exit", || child.try_wait().unwrap())
}

/// The internet sockets (TCP, UDP or raw, IPv4 or IPv6) that process `pid`
/// holds, as the kernel's tables list them.
pub fn internet_sockets(pid: u32) -> Vec<String> {
    let inodes: Vec<String> = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .map(|dir| {
            dir.flatten()
                .filter_map(|fd| {
                    let link = std::fs::read_link(fd.path()).ok()?;
                    let link = link.to_string_lossy();
                    Some(
                        link.strip_prefix("socket:[")?
                            .strip_suffix(']')?
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut found = Vec::new();
    for table in ["tcp", "tcp6", "udp", "udp6", "raw", "raw6"] {
        let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/net/{table}")) else {
            continue;
        };
        for line in text.lines().skip(1) {
            // the tenth column is the socket's inode
            if line
                .split_whitespace()
                .nth(9)
                .is_some_and(|inode| inodes.iter().any(|i| i == inode))
            {
                found.push(format!("{table}: {line}"));
            }
        }
    }
    found
}

/// An off-screen desktop for one test.
pub struct Desktop {
    dir: tempfile::TempDir,
    bus: Child,
    /// The off-screen KWin; `None` on the session the tests run in
    /// ([`Desktop::this_session`]).
    kwin: Option<Child>,
    /// The Wayland socket of that session, as an absolute path.
    host_wayland: Option<String>,
    bus_address: String,
    launches: usize,
    /// Helpers started for a test: the accessibility bus.
    helpers: Vec<Child>,
    /// More variables for the apps launched from now on.
    env: Vec<(String, String)>,
    /// The X display, when the apps run on X11 through KWin's Xwayland
    /// (T-PORT-001).
    x11: Option<String>,
    /// A program the apps run under, with its arguments (`strace`).
    wrapper: Vec<String>,
}

const WAYLAND: &str = "wayland-e2e";

impl Desktop {
    pub fn start() -> Self {
        Self::start_with(false)
    }

    /// A desktop whose apps run on X11, through KWin's Xwayland
    /// (T-PORT-001, NFR-PORT-01).
    pub fn start_x11() -> Self {
        Self::start_with(true)
    }

    /// The Wayland session the tests run in, instead of an off-screen
    /// KWin: only for what the virtual KWin cannot measure, the playback of
    /// T-PERF-03 on a real GPU, run by hand on the reference machine. The
    /// app still has its own folders and bus.
    pub fn this_session() -> Self {
        let runtime = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
        let display = std::env::var("WAYLAND_DISPLAY").expect("a Wayland session");
        let socket = if display.starts_with('/') {
            display
        } else {
            format!("{runtime}/{display}")
        };
        let (dir, bus, bus_address) = Self::folders_and_bus();
        Self {
            dir,
            bus,
            kwin: None,
            host_wayland: Some(socket),
            bus_address,
            x11: None,
            launches: 0,
            helpers: Vec::new(),
            env: Vec::new(),
            wrapper: Vec::new(),
        }
    }

    /// The app's folders and its private session bus.
    fn folders_and_bus() -> (tempfile::TempDir, Child, String) {
        let dir = tempfile::tempdir().unwrap();
        // `home` is the app's HOME: T-SEC-003 finds it empty at the end
        for sub in ["config", "state", "data", "cache", "run", "home"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let run = dir.path().join("run");
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o700)).unwrap();
        // No audio server is reachable here: mpv's audio output is null. Probing
        // the others also races in mpv's PipeWire output, which can hang the core
        // when a second file is loaded while it is still failing to connect.
        std::fs::create_dir_all(dir.path().join("config/nikomiru")).unwrap();
        std::fs::write(
            dir.path().join("config/nikomiru/config.toml"),
            "[player]\nmpv_options = { ao = \"null\" }\n",
        )
        .unwrap();

        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                run.join("bus").display()
            ),
        )
        .unwrap();
        let mut bus = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon");
        let mut bus_address = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut bus_address)
            .unwrap();
        let bus_address = bus_address.trim().to_string();
        (dir, bus, bus_address)
    }

    fn start_with(xwayland: bool) -> Self {
        let (dir, bus, bus_address) = Self::folders_and_bus();
        let run = dir.path().join("run");
        let log = std::fs::File::create(dir.path().join("kwin.log")).unwrap();
        let x_before = x_displays();
        let kwin = Command::new("kwin_wayland")
            .args([
                "--virtual",
                "--width",
                "1280",
                "--height",
                "720",
                "--scale",
                "1",
            ])
            .args(["--socket", WAYLAND])
            .args(xwayland.then_some("--xwayland"))
            .args([
                "--no-lockscreen",
                "--no-global-shortcuts",
                "--no-kactivities",
            ])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("XDG_RUNTIME_DIR", &run)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus_address)
            // the screenshots of `capture` come from any client of this desktop
            .env("KWIN_SCREENSHOT_NO_PERMISSION_CHECKS", "1")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("kwin_wayland");
        let socket = run.join(WAYLAND);
        until(Duration::from_secs(20), "KWin's Wayland socket", || {
            socket.exists().then_some(())
        });
        // KWin's X display: the first socket it adds (the second is its own);
        // Xwayland starts when the first client connects
        let x11 = xwayland.then(|| {
            until(Duration::from_secs(20), "KWin's Xwayland", || {
                let mut new: Vec<String> = x_displays()
                    .into_iter()
                    .filter(|d| !x_before.contains(d))
                    .collect();
                new.sort_by_key(|d| d[1..].parse::<u32>().unwrap_or(u32::MAX));
                new.into_iter().next()
            })
        });
        Self {
            dir,
            bus,
            kwin: Some(kwin),
            host_wayland: None,
            bus_address,
            x11,
            launches: 0,
            helpers: Vec::new(),
            env: Vec::new(),
            wrapper: Vec::new(),
        }
    }

    /// An accessibility bus for the apps launched from now on, turned on as a
    /// screen reader would turn it on (AT-SPI, NFR-A11Y-01); its address.
    /// GSettings stays in memory: the user's settings are not touched.
    pub fn start_accessibility(&mut self) -> String {
        use nikomiru_app::dbus::{Connection, Message, Value};
        let launcher = [
            "/usr/lib/at-spi-bus-launcher",
            "/usr/libexec/at-spi-bus-launcher",
            "/usr/lib/at-spi2-core/at-spi-bus-launcher",
        ]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .expect("at-spi-bus-launcher (at-spi2-core) is needed by this test");
        let log = std::fs::File::create(self.path("at-spi.log")).unwrap();
        let child = Command::new(launcher)
            .arg("--launch-immediately")
            .env_remove("DISPLAY")
            .env("WAYLAND_DISPLAY", WAYLAND)
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            .env("GSETTINGS_BACKEND", "memory")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("at-spi-bus-launcher");
        self.helpers.push(child);
        let mut conn = Connection::open(&self.bus_address).unwrap();
        let address = until(Duration::from_secs(10), "the accessibility bus", || {
            let reply = conn
                .call(Message::call(
                    "org.a11y.Bus",
                    "/org/a11y/bus",
                    "org.a11y.Bus",
                    "GetAddress",
                ))
                .ok()?;
            match reply.body.first() {
                Some(Value::Str(a)) if !a.is_empty() => Some(a.clone()),
                _ => None,
            }
        });
        // the registry, which the bus would start through systemd
        let registry = [
            "/usr/lib/at-spi2-registryd",
            "/usr/libexec/at-spi2-registryd",
            "/usr/lib/at-spi2-core/at-spi2-registryd",
        ]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .expect("at-spi2-registryd (at-spi2-core) is needed by this test");
        let log = std::fs::File::create(self.path("at-spi-registry.log")).unwrap();
        let child = Command::new(registry)
            .env_remove("DISPLAY")
            .env("WAYLAND_DISPLAY", WAYLAND)
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            .env("GSETTINGS_BACKEND", "memory")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("at-spi2-registryd");
        self.helpers.push(child);
        let mut a11y = Connection::open(&address).unwrap();
        until(
            Duration::from_secs(10),
            "the accessibility registry",
            || {
                let reply = a11y
                    .call(
                        Message::call(
                            "org.freedesktop.DBus",
                            "/org/freedesktop/DBus",
                            "org.freedesktop.DBus",
                            "NameHasOwner",
                        )
                        .with_body(vec![Value::Str("org.a11y.atspi.Registry".into())]),
                    )
                    .ok()?;
                (reply.body.first() == Some(&Value::Bool(true))).then_some(())
            },
        );
        conn.call(
            Message::call(
                "org.a11y.Bus",
                "/org/a11y/bus",
                "org.freedesktop.DBus.Properties",
                "Set",
            )
            .with_body(vec![
                Value::Str("org.a11y.Status".into()),
                Value::Str("IsEnabled".into()),
                Value::Variant(Box::new(Value::Bool(true))),
            ]),
        )
        .unwrap();
        address
    }

    /// Gives the apps launched from now on the variable `key`.
    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.push((key.to_string(), value.to_string()));
    }

    /// A screenshot of the whole desktop, as a PNG at `out` (spectacle).
    pub fn capture(&self, out: &Path) {
        let status = Command::new("spectacle")
            .args(["--background", "--nonotify", "--fullscreen", "--output"])
            .arg(out)
            .env_clear()
            .env("HOME", self.dir.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("WAYLAND_DISPLAY", WAYLAND)
            .env("QT_QPA_PLATFORM", "wayland")
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spectacle");
        assert!(status.success() && out.exists(), "spectacle: {status}");
    }

    /// The compositor's socket, for the test's own Wayland clients.
    pub fn wayland_socket(&self) -> PathBuf {
        self.path("run").join(WAYLAND)
    }

    /// A path inside this desktop's home folder.
    pub fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// A file of the app's data folder (`$XDG_DATA_HOME/nikomiru`).
    pub fn data_file(&self, name: &str) -> PathBuf {
        self.path("data").join("nikomiru").join(name)
    }

    /// The registered dictionaries of this desktop's profile, read while the
    /// app is not running.
    pub fn registered_dictionaries(&self) -> Vec<String> {
        use std::sync::Arc;
        let data = self.path("data").join("nikomiru");
        let library =
            Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager =
            nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
                .unwrap();
        manager
            .list()
            .unwrap()
            .into_iter()
            .map(|(_, r)| r.title)
            .collect()
    }

    /// A Yomitan dictionary of `entries` made-up terms, in banks of 10 000,
    /// written in this desktop's folder: long enough to import that a test
    /// can act while it runs.
    pub fn large_dictionary(&self, entries: usize) -> String {
        use std::io::Write as _;
        let path = self.path("large.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("index.json", options).unwrap();
        zip.write_all(br#"{"title":"Nikomiru large","format":3,"revision":"1"}"#)
            .unwrap();
        for bank in 0..entries.div_ceil(10_000) {
            let rows: Vec<String> = (bank * 10_000..((bank + 1) * 10_000).min(entries))
                .map(|i| {
                    format!(
                        r#"["語{i}","ご{i}","n","",0,["word {i}", "a longer gloss so that the bank weighs what a real one does"],{i},""]"#
                    )
                })
                .collect();
            zip.start_file(format!("term_bank_{}.json", bank + 1), options)
                .unwrap();
            zip.write_all(format!("[{}]", rows.join(",")).as_bytes())
                .unwrap();
        }
        zip.finish().unwrap();
        path.to_string_lossy().into_owned()
    }

    /// Imports dictionaries (zips of `target/fixtures/dicts`) into this
    /// desktop's profile, before the app starts.
    pub fn import_dictionaries(&self, names: &[&str]) {
        use std::sync::Arc;
        let data = self.path("data").join("nikomiru");
        let library =
            Arc::new(nikomiru_library::Library::open(&data.join("library.redb")).unwrap());
        let clock: Arc<dyn nikomiru_core::Clock> = Arc::new(nikomiru_core::SystemClock::new());
        let manager =
            nikomiru_library::DictionaryManager::open(library, data.join("dicts"), clock, "ja")
                .unwrap();
        for name in names {
            let zip = fixture(&format!("dicts/{name}.zip"));
            manager
                .import(Path::new(&zip), None, &nikomiru_core::Cancel::new(), |_| {})
                .unwrap();
        }
    }

    /// `nikomiru` (the harness build) with `args`, in this desktop and with its
    /// folders.
    pub fn app(&mut self, args: &[&str]) -> Command {
        self.command(Path::new(env!("CARGO_BIN_EXE_nikomiru")), args)
    }

    /// Another build of `nikomiru` (the release one) in this desktop.
    pub fn command(&mut self, binary: &Path, args: &[&str]) -> Command {
        self.launches += 1;
        let log = std::fs::File::create(self.path(&format!("app-{}.log", self.launches))).unwrap();
        let mut cmd = match self.wrapper.split_first() {
            Some((program, rest)) => {
                let mut cmd = Command::new(program);
                cmd.args(rest).arg(binary);
                cmd
            }
            None => Command::new(binary),
        };
        cmd.args(args)
            .env_clear()
            .env("HOME", self.path("home"))
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("LANG", "C.UTF-8")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            .env("NIKOMIRU_TEST_AO", "null")
            .env("NIKOMIRU_TEST_OCR", "明日も食べました")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        // Wayland, or X11 alone: winit then has no choice (T-PORT-001)
        match &self.x11 {
            Some(display) => {
                cmd.env("DISPLAY", display);
            }
            None => {
                cmd.env(
                    "WAYLAND_DISPLAY",
                    self.host_wayland.as_deref().unwrap_or(WAYLAND),
                )
                .env("QT_QPA_PLATFORM", "wayland");
            }
        }
        cmd
    }

    /// Starts `nikomiru --test-harness` and connects to it.
    pub fn launch(&mut self) -> (Child, Harness) {
        self.launch_with(&[])
    }

    /// Runs the apps launched from now on under `program` and its arguments,
    /// the app's path and arguments last.
    pub fn run_under(&mut self, program: &[&str]) {
        self.wrapper = program.iter().map(|a| a.to_string()).collect();
    }

    /// Writes `config.toml` in this desktop's settings folder.
    pub fn write_config(&self, text: &str) {
        let dir = self.path("config/nikomiru");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), text).unwrap();
    }

    /// Like [`Desktop::launch`], with more arguments.
    pub fn launch_with(&mut self, args: &[&str]) -> (Child, Harness) {
        let socket = self.path(&format!("harness-{}.sock", self.launches + 1));
        let listener = UnixListener::bind(&socket).unwrap();
        let socket_arg = socket.to_string_lossy().into_owned();
        let mut all = vec!["--test-harness", socket_arg.as_str()];
        all.extend_from_slice(args);
        let mut child = self.app(&all).spawn().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stream = until(Duration::from_secs(30), "the harness connection", || {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("nikomiru exited at startup: {status}\n{}", self.log());
            }
            listener.accept().ok().map(|(s, _)| s)
        });
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let logs = vec![
            self.path(&format!("app-{}.log", self.launches)),
            self.path("state/nikomiru/logs"),
        ];
        (child, Harness::new(stream, logs))
    }

    /// The application logs of this desktop, for failure messages.
    pub fn log(&self) -> String {
        let mut out = String::new();
        for i in 1..=self.launches {
            if let Ok(text) = std::fs::read_to_string(self.path(&format!("app-{i}.log"))) {
                out.push_str(&format!("--- launch {i}\n{text}"));
            }
        }
        if let Ok(entries) = std::fs::read_dir(self.path("state/nikomiru/logs")) {
            for e in entries.flatten() {
                if let Ok(text) = std::fs::read_to_string(e.path()) {
                    out.push_str(&format!("--- {}\n{text}", e.path().display()));
                }
            }
        }
        out
    }
}

impl Desktop {
    /// The apps a failed test left running on this desktop: the `nikomiru`
    /// processes whose environment holds this desktop's runtime folder.
    fn leftover_apps(&self) -> Vec<u32> {
        self.leftover(&["nikomiru"])
    }

    /// The processes of this desktop (they have its runtime folder) whose
    /// program is one of `names`.
    fn leftover(&self, names: &[&str]) -> Vec<u32> {
        let needle = format!("XDG_RUNTIME_DIR={}", self.path("run").display());
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| {
                let proc = PathBuf::from(format!("/proc/{pid}"));
                let is_app = std::fs::read_link(proc.join("exe")).is_ok_and(|exe| {
                    exe.file_name()
                        .is_some_and(|n| names.iter().any(|name| n == *name))
                });
                is_app
                    && std::fs::read(proc.join("environ"))
                        .is_ok_and(|env| env.split(|b| *b == 0).any(|var| var == needle.as_bytes()))
            })
            .collect()
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // T-SEC-003 (NFR-SEC-02): nothing written outside the XDG folders
        // and those the test chose, checked once the app is gone
        if !std::thread::panicking() && self.leftover_apps().is_empty() {
            let mut written = Vec::new();
            let mut todo = vec![self.path("home")];
            while let Some(dir) = todo.pop() {
                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        todo.push(entry.path());
                    } else {
                        written.push(entry.path());
                    }
                }
            }
            assert!(
                written.is_empty(),
                "T-SEC-003: written in HOME: {written:?}"
            );
        }
        // a failed test may leave its app running: it must not outlive the desktop
        for pid in self.leftover_apps() {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
        // the accessibility bus and its registry, then its launcher
        for pid in self.leftover(&["dbus-daemon", "dbus-broker", "at-spi2-registryd"]) {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
        for child in &mut self.helpers {
            let _ = child.kill();
            let _ = child.wait();
        }
        // KWin with Xwayland removes its X sockets only when asked to quit
        if let Some(kwin) = &mut self.kwin {
            let _ = Command::new("kill")
                .args(["-TERM", &kwin.id().to_string()])
                .status();
            let deadline = Instant::now() + Duration::from_secs(5);
            while kwin.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        for child in self.kwin.iter_mut().chain([&mut self.bus]) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The client side of the harness protocol.
pub struct Harness {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    /// The app's output and its log folder, shown when a command fails.
    logs: Vec<PathBuf>,
}

impl Harness {
    fn new(stream: UnixStream, logs: Vec<PathBuf>) -> Self {
        Self {
            writer: stream.try_clone().unwrap(),
            reader: BufReader::new(stream),
            logs,
        }
    }

    /// The end of the app's logs, for failure messages.
    fn log_tail(&self) -> String {
        let mut text = String::new();
        for path in &self.logs {
            let files: Vec<PathBuf> = if path.is_dir() {
                std::fs::read_dir(path)
                    .map(|r| r.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default()
            } else {
                vec![path.clone()]
            };
            for f in files {
                if let Ok(t) = std::fs::read_to_string(&f) {
                    let lines: Vec<&str> = t.lines().collect();
                    let tail = &lines[lines.len().saturating_sub(60)..];
                    text.push_str(&format!("--- {}\n{}\n", f.display(), tail.join("\n")));
                }
            }
        }
        text
    }

    /// Sends a command and returns its result, or the harness's error.
    pub fn try_cmd(&mut self, command: Value) -> Result<Value, String> {
        writeln!(self.writer, "{command}").unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("bad answer {line:?}: {e}\n{}", self.log_tail()));
        if answer["ok"] == true {
            Ok(answer["result"].clone())
        } else {
            Err(answer["error"].to_string())
        }
    }

    pub fn cmd(&mut self, command: Value) -> Value {
        let shown = command.to_string();
        self.try_cmd(command)
            .unwrap_or_else(|e| panic!("{shown}: {e}\n{}", self.log_tail()))
    }

    pub fn open(&mut self, path: &str) {
        self.cmd(json!({ "cmd": "open", "path": path }));
    }

    pub fn wait(&mut self, event: &str) -> Value {
        self.cmd(json!({ "cmd": "wait", "event": event, "timeout_ms": 20_000 }))
    }

    pub fn drop_files(&mut self, paths: &[&str]) {
        self.cmd(json!({ "cmd": "drop", "paths": paths }));
    }

    pub fn key(&mut self, code: &str) {
        self.cmd(json!({ "cmd": "key", "code": code, "mods": [] }));
    }

    pub fn state(&mut self) -> Value {
        self.cmd(json!({ "cmd": "state" }))
    }

    /// Polls the state until `test` accepts it.
    pub fn state_until(&mut self, what: &str, test: impl Fn(&Value) -> bool) -> Value {
        until(Duration::from_secs(10), what, || {
            let s = self.state();
            test(&s).then_some(s)
        })
    }

    pub fn quit(&mut self) {
        self.cmd(json!({ "cmd": "quit" }));
    }
}
