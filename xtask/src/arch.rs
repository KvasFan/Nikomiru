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

/// Source extensions that need the SPDX header: Rust, the C++ video item, QML.
const SOURCE_EXTENSIONS: [&str; 5] = ["rs", "cpp", "h", "hpp", "qml"];

/// C++ extensions: C++ is allowed only in `nikomiru-player/cpp/` (ARC-011).
const CPP_EXTENSIONS: [&str; 4] = ["cpp", "h", "hpp", "cc"];

/// Crates in which `unsafe` is allowed, with the path prefixes (relative to the
/// crate) that may contain it (ARC-011). `src/bridge` covers the cxx-qt bridge
/// modules (`src/bridge.rs` or `src/bridge/`), which hold generated FFI declarations;
/// the player's `build.rs` uses the `unsafe` `cc_builder` hook of cxx-qt-build.
const UNSAFE_ALLOWED: [(&str, &[&str]); 3] = [
    ("nikomiru-player", &["src/", "build.rs"]),
    ("nikomiru-app", &["src/linux", "src/bridge"]),
    ("nikomiru-ui", &["src/bridge"]),
];

/// Pure-logic crates: they do not depend, not even indirectly, on Qt or mpv.
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
    let metadata = cargo_metadata()?;
    problems.extend(check_deps(&Graph::from_metadata(&metadata)?));
    problems.extend(check_cxx_versions(&metadata));
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
        if lib.is_file() {
            roots.push((crate_name(&dir), lib, true));
        } else if main.is_file() {
            roots.push((crate_name(&dir), main, false));
        } else {
            problems.push(format!("{}: neither src/lib.rs nor src/main.rs", rel(&dir)));
        }
    }
    roots.push(("xtask".into(), root.join("xtask/src/main.rs"), false));
    for (name, file, is_lib) in roots {
        let text = std::fs::read_to_string(&file)?;
        // Crates with cxx-qt bridges deny `unsafe` at crate level and allow it again
        // only in the modules of UNSAFE_ALLOWED; nikomiru-player uses it for the mpv
        // FFI and the video item.
        let required = match name.as_str() {
            "nikomiru-player" => None,
            "nikomiru-app" | "nikomiru-ui" => Some("#![deny(unsafe_code)]"),
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
/// C++ only for the Qt Quick video item, in `crates/nikomiru-player/cpp/`.
pub fn check_cpp_location(root: &Path) -> Result<Vec<String>> {
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Ok(Vec::new());
    }
    let allowed = crates.join("nikomiru-player/cpp");
    let mut files = Vec::new();
    for ext in CPP_EXTENSIONS {
        walk(&crates, ext, &mut files)?;
    }
    Ok(files
        .into_iter()
        .filter(|f| !f.starts_with(&allowed))
        .map(|f| {
            format!(
                "{}: C++ is allowed only in crates/nikomiru-player/cpp/ (ARC-011)",
                rel(&f)
            )
        })
        .collect())
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

/// Qt through cxx-qt (D-10): `cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`, `cxx`, …
fn is_qt(name: &str) -> bool {
    name == "cxx" || name.starts_with("cxx-qt") || name == "qt-build-utils"
}

fn is_mpv(name: &str) -> bool {
    name == "libmpv2" || name == "libmpv2-sys"
}

/// The toolkit replaced by D-10: no workspace crate may depend on it again.
fn is_retired_ui(name: &str) -> bool {
    name == "dioxus"
        || name.starts_with("dioxus-")
        || matches!(name, "gtk" | "gdk" | "webkit2gtk" | "tao" | "wry")
}

/// Crates allowed to depend directly on Qt (cxx-qt).
const QT_CRATES: [&str; 3] = ["nikomiru-ui", "nikomiru-player", "nikomiru-app"];

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
                if is_qt(dn) || is_mpv(dn) || is_retired_ui(dn) {
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
            } else if is_qt(dn) && !QT_CRATES.contains(&name) {
                Some("only nikomiru-ui, nikomiru-player and nikomiru-app may depend on it")
            } else if is_retired_ui(dn) {
                Some("the UI toolkit is Qt 6 (D-10)")
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

/// `cxx-gen` 0.7.N writes the C++ side of the bridges with symbols named after
/// N, the `cxx` 1.0.N macro the Rust side: a lock file that resolves them to
/// different patch levels links with undefined `cxxbridge1$N$…` symbols.
pub fn check_cxx_versions(metadata: &Value) -> Vec<String> {
    let patch = |name: &str| -> BTreeSet<String> {
        metadata["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["name"] == name)
            .filter_map(|p| p["version"].as_str()?.split('.').nth(2).map(str::to_string))
            .collect()
    };
    let (cxx, generator) = (patch("cxx"), patch("cxx-gen"));
    if generator.is_empty() || cxx == generator {
        return Vec::new();
    }
    vec![format!(
        "cxx 1.0.{cxx:?} and cxx-gen 0.7.{generator:?} differ: run \
         `cargo update -p cxx-gen --precise 0.7.<cxx patch>`"
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cxx_and_cxx_gen_must_share_the_patch_level() {
        let meta = |gen_version: &str| {
            serde_json::json!({"packages": [
                {"name": "cxx", "version": "1.0.205"},
                {"name": "cxx-gen", "version": gen_version},
            ]})
        };
        assert!(check_cxx_versions(&meta("0.7.205")).is_empty());
        assert_eq!(check_cxx_versions(&meta("0.7.198")).len(), 1);
        assert!(check_cxx_versions(&serde_json::json!({"packages": []})).is_empty());
    }

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
                "cxx",
                "cxx-qt",
                "cxx-qt-lib",
                "cxx-qt-build",
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
                ("nikomiru-player", "cxx", false),
                ("nikomiru-player", "cxx-qt-build", false),
                ("nikomiru-ui", "cxx-qt", false),
                ("nikomiru-ui", "cxx-qt-lib", false),
                ("nikomiru-app", "nikomiru-player", false),
                ("nikomiru-app", "nikomiru-ui", false),
                ("nikomiru-app", "cxx-qt-lib", false),
            ],
        );
        assert_eq!(check_deps(&g), Vec::<String>::new());
    }

    #[test]
    fn deps_rules_reject_qt_in_logic_crates_even_indirectly() {
        let g = graph(
            &[
                "nikomiru-subs",
                "helper",
                "cxx-qt-lib",
                "nikomiru-core",
                "nikomiru-text",
            ],
            &["nikomiru-subs", "nikomiru-core", "nikomiru-text"],
            &[
                ("nikomiru-subs", "helper", false),
                ("helper", "cxx-qt-lib", false),
                ("nikomiru-core", "nikomiru-text", false),
            ],
        );
        let p = check_deps(&g);
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-subs") && m.contains("cxx-qt-lib")),
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
            ],
            &["nikomiru-text", "nikomiru-ui", "nikomiru-media"],
            &[
                ("nikomiru-text", "proptest", true),
                ("proptest", "cxx", false),
                ("nikomiru-ui", "libmpv2", false),
                ("nikomiru-ui", "dioxus", false),
                ("nikomiru-media", "cxx-qt", false),
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
                .any(|m| m.contains("nikomiru-ui depends directly on dioxus") && m.contains("D-10")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("nikomiru-media depends directly on cxx-qt")),
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
