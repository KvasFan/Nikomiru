// SPDX-License-Identifier: GPL-3.0-only

//! Architecture checks (SPECIFICATION §3.1, §5.1, NFR-MAINT-01):
//! SPDX headers, use of `unsafe`, documentation and dependency rules.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::util::{self, rel, split_comment, walk};

/// Mandatory first line of every source file (SPECIFICATION §3.1).
pub const SPDX: &str = "// SPDX-License-Identifier: GPL-3.0-only";

/// Directories that contain project sources.
const SOURCE_DIRS: [&str; 4] = ["crates", "xtask", "spikes", "tests"];

/// Source extensions that need the SPDX header: Rust, and the C++ and QML of the
/// spikes.
const SOURCE_EXTENSIONS: [&str; 5] = ["rs", "cpp", "h", "hpp", "qml"];

/// C++ extensions: no C++ in the crates since D-13 (ARC-011).
const CPP_EXTENSIONS: [&str; 4] = ["cpp", "h", "hpp", "cc"];

/// Crates in which `unsafe` is allowed, with the path prefixes (relative to the
/// crate) that may contain it (ARC-011): the player for the mpv FFI and its
/// render context; the app for system calls (`src/linux`) and the OpenGL context
/// of the window (`src/window.rs`).
const UNSAFE_ALLOWED: [(&str, &[&str]); 2] = [
    ("nikomiru-player", &["src/"]),
    ("nikomiru-app", &["src/linux", "src/window.rs"]),
];

/// Pure-logic crates: they do not depend, not even indirectly, on the UI toolkit
/// or mpv.
const LOGIC_CRATES: [&str; 5] = [
    "nikomiru-text",
    "nikomiru-subs",
    "nikomiru-dict",
    "nikomiru-anki",
    "nikomiru-library",
];

/// Runs all checks and fails with the list of problems.
pub fn check_all(root: &Path) -> Result<()> {
    let mut problems = check_headers(root)?;
    problems.extend(check_crate_attributes(root)?);
    problems.extend(check_unsafe(root)?);
    problems.extend(check_cpp_location(root)?);
    problems.extend(check_sync_reads(root)?);
    problems.extend(check_panic_lints(root)?);
    let metadata = cargo_metadata()?;
    problems.extend(check_deps(&Graph::from_metadata(&metadata)?));
    report("architecture checks", &problems)
}

/// Prints the problems and fails if there are any.
pub fn report(what: &str, problems: &[String]) -> Result<()> {
    if problems.is_empty() {
        eprintln!("ok: {what}");
        return Ok(());
    }
    for p in problems {
        eprintln!("  ✗ {p}");
    }
    bail!("{what}: {} problems", problems.len())
}

fn source_files(root: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in SOURCE_DIRS {
        let dir = root.join(dir);
        if dir.is_dir() {
            for ext in extensions {
                walk(&dir, ext, &mut files)?;
            }
        }
    }
    files.sort();
    Ok(files)
}

// T-ARC-020
/// Every `.rs`, `.cpp`, `.h` and `.qml` file starts with the SPDX header.
pub fn check_headers(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for file in source_files(root, &SOURCE_EXTENSIONS)? {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let first = text
            .trim_start_matches('\u{feff}')
            .lines()
            .next()
            .unwrap_or("");
        if first.trim_end() != SPDX {
            problems.push(format!("{}: the first line must be `{SPDX}`", rel(&file)));
        }
    }
    Ok(problems)
}

/// Crate directories in `crates/`, in order.
fn crate_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn crate_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// T-ARC-011
// T-MAINT-001
/// `#![forbid(unsafe_code)]` or `#![deny(unsafe_code)]` as required by ARC-011, and
/// `#![deny(missing_docs)]` in every library crate (NFR-MAINT-01).
pub fn check_crate_attributes(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let mut roots: Vec<(String, PathBuf, bool)> = Vec::new();
    for dir in crate_dirs(root)? {
        let lib = dir.join("src/lib.rs");
        let main = dir.join("src/main.rs");
        if !lib.is_file() && !main.is_file() {
            problems.push(format!("{}: neither src/lib.rs nor src/main.rs", rel(&dir)));
        }
        // a crate with both targets (nikomiru-app) has both roots checked
        if lib.is_file() {
            roots.push((crate_name(&dir), lib, true));
        }
        if main.is_file() {
            roots.push((crate_name(&dir), main, false));
        }
    }
    roots.push(("xtask".into(), root.join("xtask/src/main.rs"), false));
    for (name, file, is_lib) in roots {
        let text = std::fs::read_to_string(&file)?;
        // nikomiru-app denies `unsafe` at crate level and allows it again only in
        // the modules of UNSAFE_ALLOWED; nikomiru-player uses it for the mpv FFI and
        // the render context.
        let required = match name.as_str() {
            "nikomiru-player" => None,
            "nikomiru-app" => Some("#![deny(unsafe_code)]"),
            _ => Some("#![forbid(unsafe_code)]"),
        };
        if let Some(attr) = required
            && !text.contains(attr)
        {
            problems.push(format!("{}: missing `{attr}` (ARC-011)", rel(&file)));
        }
        if is_lib && !text.contains("#![deny(missing_docs)]") {
            problems.push(format!(
                "{}: missing `#![deny(missing_docs)]` (NFR-MAINT-01)",
                rel(&file)
            ));
        }
    }
    Ok(problems)
}

/// Positions (line, 1-based) of the `unsafe` keywords in the code, excluding comments
/// and strings, each with a flag telling whether the preceding comment contains
/// `SAFETY:`.
pub fn unsafe_sites(text: &str) -> Vec<(usize, bool)> {
    let mut sites = Vec::new();
    let mut in_block = false;
    let mut pending_comments: Vec<String> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let (code, comment) = split_comment(line, &mut in_block);
        let has_unsafe = code
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| w == "unsafe");
        if has_unsafe {
            let documented = comment.contains("SAFETY:")
                || pending_comments
                    .iter()
                    .any(|c| c.contains("SAFETY:") || c.contains("# Safety"));
            sites.push((i + 1, documented));
        }
        let code_trim = code.trim();
        if code_trim.is_empty() && !comment.is_empty() {
            pending_comments.push(comment);
        } else if code_trim.starts_with("#[") || code_trim.is_empty() {
            // attributes and blank lines do not break the comment block
        } else {
            pending_comments.clear();
        }
    }
    sites
}

// T-ARC-011
/// `unsafe` only in the allowed crates and modules, always with a `// SAFETY:` comment.
pub fn check_unsafe(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for dir in crate_dirs(root)? {
        let name = crate_name(&dir);
        let allowed: Option<&[&str]> = UNSAFE_ALLOWED
            .iter()
            .find(|(c, _)| *c == name)
            .map(|(_, p)| *p);
        let mut files = Vec::new();
        walk(&dir, "rs", &mut files)?;
        for file in files {
            let text = std::fs::read_to_string(&file)?;
            let in_crate = file
                .strip_prefix(&dir)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            for (line, documented) in unsafe_sites(&text) {
                match allowed {
                    Some(prefixes) if prefixes.iter().any(|p| in_crate.starts_with(p)) => {}
                    Some(prefixes) => problems.push(format!(
                        "{}:{line}: `unsafe` allowed in {name} only under {} (ARC-011)",
                        rel(&file),
                        prefixes.join(", ")
                    )),
                    None => problems.push(format!(
                        "{}:{line}: `unsafe` not allowed in {name} (ARC-011)",
                        rel(&file)
                    )),
                }
                if !documented {
                    problems.push(format!(
                        "{}:{line}: `unsafe` without a `// SAFETY:` comment (ARC-011)",
                        rel(&file)
                    ));
                }
            }
        }
    }
    Ok(problems)
}

// T-ARC-011
/// No C++ in the crates: since D-13 the video is drawn from Rust.
pub fn check_cpp_location(root: &Path) -> Result<Vec<String>> {
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for ext in CPP_EXTENSIONS {
        walk(&crates, ext, &mut files)?;
    }
    Ok(files
        .into_iter()
        .map(|f| format!("{}: no C++ in the crates (ARC-011, D-13)", rel(&f)))
        .collect())
}

// T-ARC-003
/// ARC-003: the UI never reads mpv properties synchronously. `get_property` is
/// called only inside nikomiru-player; `property_string` exists only with the
/// player's `test-support` feature and is called only by the player's tests and
/// by the GUI harness (`harness.rs`).
pub fn check_sync_reads(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let player = root.join("crates/nikomiru-player");
    for file in source_files(root, &["rs"])? {
        // the spikes are standalone experiments, outside the product
        if file.starts_with(root.join("spikes")) {
            continue;
        }
        let in_player = file.starts_with(&player);
        let is_harness = file.file_name().is_some_and(|n| n == "harness.rs");
        let text = std::fs::read_to_string(&file)?;
        let mut in_block = false;
        for (i, line) in text.lines().enumerate() {
            let (code, _) = split_comment(line, &mut in_block);
            if !in_player && code.contains("get_property") {
                problems.push(format!(
                    "{}:{}: `get_property` outside nikomiru-player (ARC-003)",
                    rel(&file),
                    i + 1
                ));
            }
            if !in_player && !is_harness && code.contains("property_string(") {
                problems.push(format!(
                    "{}:{}: synchronous mpv read outside the tests and the harness (ARC-003)",
                    rel(&file),
                    i + 1
                ));
            }
        }
    }
    let api = std::fs::read_to_string(player.join("src/player.rs"))?;
    let lines: Vec<&str> = api.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("pub fn property_string")
            && !lines[..i]
                .iter()
                .rev()
                .take_while(|l| {
                    l.trim_start().starts_with("#[") || l.trim_start().starts_with("///")
                })
                .any(|l| l.contains(r#"#[cfg(feature = "test-support")]"#))
        {
            problems.push(format!(
                "crates/nikomiru-player/src/player.rs:{}: `property_string` must need the \
                 `test-support` feature (ARC-003)",
                i + 1
            ));
        }
    }
    Ok(problems)
}

// T-ERR-002
/// ERR-002: `unwrap_used` and `expect_used` are errors for the whole workspace,
/// every crate inherits the workspace lints, and no source outside the tests
/// turns them off.
pub fn check_panic_lints(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let clippy = &manifest["workspace"]["lints"]["clippy"];
    for lint in ["unwrap_used", "expect_used"] {
        if clippy.get(lint).and_then(|v| v.as_str()) != Some("deny") {
            problems.push(format!(
                "Cargo.toml: clippy `{lint}` must be \"deny\" (ERR-002)"
            ));
        }
    }
    let mut manifests: Vec<PathBuf> = crate_dirs(root)?
        .into_iter()
        .map(|d| d.join("Cargo.toml"))
        .collect();
    manifests.push(root.join("xtask/Cargo.toml"));
    for m in manifests {
        let table: toml::Table = std::fs::read_to_string(&m)?.parse()?;
        let inherits = table
            .get("lints")
            .and_then(|l| l.get("workspace"))
            .and_then(toml::Value::as_bool);
        if inherits != Some(true) {
            problems.push(format!(
                "{}: missing `[lints] workspace = true` (ERR-002)",
                rel(&m)
            ));
        }
    }
    for file in source_files(root, &["rs"])? {
        let r = rel(&file);
        // integration tests (`tests/` folders) may unwrap, like unit tests
        if r.contains("/tests/") || r.starts_with("tests/") || r.starts_with("spikes/") {
            continue;
        }
        let text = std::fs::read_to_string(&file)?;
        let mut in_block = false;
        for (i, line) in text.lines().enumerate() {
            let (code, _) = split_comment(line, &mut in_block);
            if code.contains("allow(clippy::unwrap_used")
                || code.contains("allow(clippy::expect_used")
            {
                problems.push(format!(
                    "{r}:{}: unwrap/expect allowed outside the tests (ERR-002)",
                    i + 1
                ));
            }
        }
    }
    Ok(problems)
}

/// `cargo metadata` of the workspace for the Linux platform.
pub fn cargo_metadata() -> Result<Value> {
    let out = util::output(util::cargo().args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
    ]))?;
    serde_json::from_str(&out).context("invalid cargo metadata output")
}

/// Graph of the resolved dependencies, reduced to what the checks need.
#[derive(Debug, Default)]
pub struct Graph {
    /// Name of every package, by id.
    pub names: BTreeMap<String, String>,
    /// Ids of the workspace members.
    pub members: BTreeSet<String>,
    /// Edges `package → (dependency, dev only)`.
    pub edges: BTreeMap<String, Vec<(String, bool)>>,
}

impl Graph {
    /// Builds the graph from the output of `cargo metadata --format-version 1`.
    pub fn from_metadata(v: &Value) -> Result<Self> {
        let mut g = Graph::default();
        for p in v["packages"].as_array().context("metadata: packages")? {
            let id = p["id"].as_str().context("metadata: id")?;
            let name = p["name"].as_str().context("metadata: name")?;
            g.names.insert(id.into(), name.into());
        }
        for m in v["workspace_members"]
            .as_array()
            .context("metadata: workspace_members")?
        {
            g.members
                .insert(m.as_str().context("metadata: member")?.into());
        }
        for node in v["resolve"]["nodes"]
            .as_array()
            .context("metadata: resolve")?
        {
            let id = node["id"].as_str().context("metadata: node id")?;
            let mut deps = Vec::new();
            for d in node["deps"].as_array().into_iter().flatten() {
                let pkg = d["pkg"].as_str().context("metadata: dep pkg")?;
                let kinds = d["dep_kinds"].as_array().into_iter().flatten();
                let dev_only =
                    kinds.clone().count() > 0 && kinds.clone().all(|k| k["kind"] == "dev");
                deps.push((pkg.to_string(), dev_only));
            }
            g.edges.insert(id.into(), deps);
        }
        Ok(g)
    }

    fn name<'a>(&'a self, id: &'a str) -> &'a str {
        self.names.get(id).map_or(id, String::as_str)
    }

    /// Direct (normal and build) dependencies of a package.
    pub fn direct(&self, id: &str) -> Vec<&str> {
        self.edges
            .get(id)
            .into_iter()
            .flatten()
            .filter(|(_, dev)| !dev)
            .map(|(d, _)| d.as_str())
            .collect()
    }

    /// Transitive closure of the normal and build dependencies.
    pub fn closure(&self, id: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<&str> = self.direct(id).into_iter().collect();
        while let Some(d) = queue.pop_front() {
            if seen.insert(d.to_string()) {
                queue.extend(self.direct(d));
            }
        }
        seen
    }
}

/// The UI toolkit (D-13): egui and the crates that put it in a window.
fn is_ui_toolkit(name: &str) -> bool {
    matches!(
        name,
        "egui"
            | "epaint"
            | "emath"
            | "ecolor"
            | "egui-winit"
            | "egui_glow"
            | "winit"
            | "glutin"
            | "glutin-winit"
            | "glow"
    ) || name.starts_with("accesskit")
}

fn is_mpv(name: &str) -> bool {
    name == "libmpv2" || name == "libmpv2-sys"
}

/// The toolkits replaced by D-10 (Dioxus) and D-13 (Qt through cxx-qt): no
/// workspace crate may depend on them again.
fn is_retired_ui(name: &str) -> bool {
    name == "dioxus"
        || name.starts_with("dioxus-")
        || matches!(name, "gtk" | "gdk" | "webkit2gtk" | "tao" | "wry")
        || name == "cxx"
        || name.starts_with("cxx-qt")
        || name == "qt-build-utils"
}

/// Crates allowed to depend directly on the UI toolkit.
const UI_CRATES: [&str; 2] = ["nikomiru-ui", "nikomiru-app"];

// T-ARC-010
/// Dependency rules of SPECIFICATION §5.1.
pub fn check_deps(g: &Graph) -> Vec<String> {
    let mut problems = Vec::new();
    for member in &g.members {
        let name = g.name(member);
        let direct = g.direct(member);
        if name == "nikomiru-core" {
            for d in &direct {
                if g.members.contains(*d) {
                    problems.push(format!(
                        "nikomiru-core depends on {}: it must not depend on workspace crates",
                        g.name(d)
                    ));
                }
            }
        }
        if LOGIC_CRATES.contains(&name) {
            for d in g.closure(member) {
                let dn = g.name(&d);
                if is_ui_toolkit(dn) || is_mpv(dn) || is_retired_ui(dn) {
                    problems.push(format!(
                        "{name} depends (possibly indirectly) on {dn}: it is a pure-logic crate"
                    ));
                }
            }
        }
        for d in &direct {
            let dn = g.name(d);
            let rule = if is_mpv(dn) && name != "nikomiru-player" {
                Some("only nikomiru-player may depend on it")
            } else if is_ui_toolkit(dn) && !UI_CRATES.contains(&name) {
                Some("only nikomiru-ui and nikomiru-app may depend on it")
            } else if is_retired_ui(dn) {
                Some("the UI toolkit is egui (D-13)")
            } else {
                None
            };
            if let Some(rule) = rule {
                problems.push(format!("{name} depends directly on {dn}: {rule}"));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(pkgs: &[&str], members: &[&str], edges: &[(&str, &str, bool)]) -> Graph {
        let mut g = Graph::default();
        for p in pkgs {
            g.names.insert((*p).into(), (*p).into());
        }
        for m in members {
            g.members.insert((*m).into());
        }
        for (a, b, dev) in edges {
            g.edges
                .entry((*a).into())
                .or_default()
                .push(((*b).into(), *dev));
        }
        g
    }

    #[test]
    fn deps_rules_accept_the_intended_layout() {
        let g = graph(
            &[
                "nikomiru-core",
                "nikomiru-dict",
                "nikomiru-player",
                "nikomiru-ui",
                "nikomiru-app",
                "redb",
                "libmpv2",
                "egui",
                "egui_glow",
                "winit",
            ],
            &[
                "nikomiru-core",
                "nikomiru-dict",
                "nikomiru-player",
                "nikomiru-ui",
                "nikomiru-app",
            ],
            &[
                ("nikomiru-dict", "nikomiru-core", false),
                ("nikomiru-dict", "redb", false),
                ("nikomiru-player", "libmpv2", false),
                ("nikomiru-ui", "egui", false),
                ("nikomiru-app", "nikomiru-player", false),
                ("nikomiru-app", "nikomiru-ui", false),
                ("nikomiru-app", "egui_glow", false),
                ("nikomiru-app", "winit", false),
            ],
        );
        assert_eq!(check_deps(&g), Vec::<String>::new());
    }

    #[test]
    fn deps_rules_reject_the_toolkit_in_logic_crates_even_indirectly() {
        let g = graph(
            &[
                "nikomiru-subs",
                "helper",
                "egui",
                "nikomiru-core",
                "nikomiru-text",
            ],
            &["nikomiru-subs", "nikomiru-core", "nikomiru-text"],
            &[
                ("nikomiru-subs", "helper", false),
                ("helper", "egui", false),
                ("nikomiru-core", "nikomiru-text", false),
            ],
        );
        let p = check_deps(&g);
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-subs") && m.contains("egui")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-core depends on nikomiru-text")),
            "{p:?}"
        );
    }

    #[test]
    fn deps_rules_ignore_dev_dependencies_and_reject_direct_misuse() {
        let g = graph(
            &[
                "nikomiru-text",
                "proptest",
                "cxx",
                "nikomiru-ui",
                "nikomiru-media",
                "libmpv2",
                "cxx-qt",
                "dioxus",
                "winit",
            ],
            &["nikomiru-text", "nikomiru-ui", "nikomiru-media"],
            &[
                ("nikomiru-text", "proptest", true),
                ("proptest", "cxx", false),
                ("nikomiru-ui", "libmpv2", false),
                ("nikomiru-ui", "dioxus", false),
                ("nikomiru-media", "cxx-qt", false),
                ("nikomiru-media", "winit", false),
            ],
        );
        let p = check_deps(&g);
        assert!(!p.iter().any(|m| m.contains("nikomiru-text")), "{p:?}");
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-ui depends directly on libmpv2")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-ui depends directly on dioxus") && m.contains("D-13")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-media depends directly on cxx-qt")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-media depends directly on winit")),
            "{p:?}"
        );
    }

    #[test]
    fn unsafe_sites_require_safety_comments() {
        let src = "\
fn a() {
    // SAFETY: the pointer is valid for the whole call.
    unsafe { f() };
    unsafe { g() };
    let s = \"unsafe in a string\";
    // unsafe in a comment
}

/// # Safety
/// The caller guarantees that `p` is valid.
#[inline]
pub unsafe fn h(p: *const u8) {}
";
        assert_eq!(unsafe_sites(src), vec![(3, true), (4, false), (12, true)]);
    }

    #[test]
    fn forbid_attribute_is_not_an_unsafe_site() {
        assert!(
            unsafe_sites("#![forbid(unsafe_code)]\n#![deny(unsafe_op_in_unsafe_fn)]\n").is_empty()
        );
    }
}
