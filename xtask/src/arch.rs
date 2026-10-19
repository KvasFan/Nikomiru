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

/// Crates in which `unsafe` is allowed, with the path prefix (relative to the crate)
/// that may contain it (ARC-011).
const UNSAFE_ALLOWED: [(&str, &str); 2] =
    [("anikoto-player", "src/"), ("anikoto-app", "src/linux")];

/// Pure-logic crates: they do not depend, not even indirectly, on UI, GTK or mpv.
const LOGIC_CRATES: [&str; 5] = [
    "anikoto-text",
    "anikoto-subs",
    "anikoto-dict",
    "anikoto-anki",
    "anikoto-library",
];

/// Runs all checks and fails with the list of problems.
pub fn check_all(root: &Path) -> Result<()> {
    let mut problems = check_headers(root)?;
    problems.extend(check_crate_attributes(root)?);
    problems.extend(check_unsafe(root)?);
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

fn source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in SOURCE_DIRS {
        let dir = root.join(dir);
        if dir.is_dir() {
            walk(&dir, "rs", &mut files)?;
        }
    }
    Ok(files)
}

// T-ARC-020
/// Every `.rs` file starts with the SPDX header.
pub fn check_headers(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for file in source_files(root)? {
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
/// `#![forbid(unsafe_code)]` where required by ARC-011 and `#![deny(missing_docs)]` in
/// every library crate (NFR-MAINT-01).
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
        // anikoto-app denies `unsafe` at crate level and allows it again only in the
        // `linux` module; anikoto-player uses it for the GL callbacks.
        let required = match name.as_str() {
            "anikoto-player" => None,
            "anikoto-app" => Some("#![deny(unsafe_code)]"),
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
        let allowed = UNSAFE_ALLOWED
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
                    Some(prefix) if in_crate.starts_with(prefix) => {}
                    Some(prefix) => problems.push(format!(
                        "{}:{line}: `unsafe` allowed only in {name}/{prefix}… (ARC-011)",
                        rel(&file)
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

fn is_dioxus(name: &str) -> bool {
    name == "dioxus" || name.starts_with("dioxus-")
}

fn is_gtk(name: &str) -> bool {
    matches!(
        name,
        "gtk" | "gtk-sys" | "gdk" | "gdk-sys" | "webkit2gtk" | "webkit2gtk-sys" | "tao" | "wry"
    )
}

fn is_mpv(name: &str) -> bool {
    name == "libmpv2" || name == "libmpv2-sys"
}

// T-ARC-010
/// Dependency rules of SPECIFICATION §5.1.
pub fn check_deps(g: &Graph) -> Vec<String> {
    let mut problems = Vec::new();
    for member in &g.members {
        let name = g.name(member);
        let direct = g.direct(member);
        if name == "anikoto-core" {
            for d in &direct {
                if g.members.contains(*d) {
                    problems.push(format!(
                        "anikoto-core depends on {}: it must not depend on workspace crates",
                        g.name(d)
                    ));
                }
            }
        }
        if LOGIC_CRATES.contains(&name) {
            for d in g.closure(member) {
                let dn = g.name(&d);
                if is_dioxus(dn) || is_gtk(dn) || is_mpv(dn) {
                    problems.push(format!(
                        "{name} depends (possibly indirectly) on {dn}: it is a pure-logic crate"
                    ));
                }
            }
        }
        for d in &direct {
            let dn = g.name(d);
            let rule = if is_mpv(dn) && name != "anikoto-player" {
                Some("only anikoto-player")
            } else if is_gtk(dn) && name != "anikoto-app" {
                Some("only anikoto-app")
            } else if is_dioxus(dn) && name != "anikoto-ui" && name != "anikoto-app" {
                Some("only anikoto-ui and anikoto-app")
            } else {
                None
            };
            if let Some(rule) = rule {
                problems.push(format!(
                    "{name} depends directly on {dn}: {rule} may depend on it"
                ));
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
                "anikoto-core",
                "anikoto-dict",
                "anikoto-player",
                "anikoto-ui",
                "anikoto-app",
                "redb",
                "libmpv2",
                "dioxus",
                "gtk",
            ],
            &[
                "anikoto-core",
                "anikoto-dict",
                "anikoto-player",
                "anikoto-ui",
                "anikoto-app",
            ],
            &[
                ("anikoto-dict", "anikoto-core", false),
                ("anikoto-dict", "redb", false),
                ("anikoto-player", "libmpv2", false),
                ("anikoto-ui", "dioxus", false),
                ("anikoto-app", "anikoto-player", false),
                ("anikoto-app", "anikoto-ui", false),
                ("anikoto-app", "gtk", false),
                ("anikoto-app", "dioxus", false),
            ],
        );
        assert_eq!(check_deps(&g), Vec::<String>::new());
    }

    #[test]
    fn deps_rules_reject_ui_in_logic_crates_even_indirectly() {
        let g = graph(
            &[
                "anikoto-subs",
                "helper",
                "dioxus-core",
                "anikoto-core",
                "anikoto-text",
            ],
            &["anikoto-subs", "anikoto-core", "anikoto-text"],
            &[
                ("anikoto-subs", "helper", false),
                ("helper", "dioxus-core", false),
                ("anikoto-core", "anikoto-text", false),
            ],
        );
        let p = check_deps(&g);
        assert!(
            p.iter()
                .any(|m| m.contains("anikoto-subs") && m.contains("dioxus-core")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("anikoto-core depends on anikoto-text")),
            "{p:?}"
        );
    }

    #[test]
    fn deps_rules_ignore_dev_dependencies_and_reject_direct_misuse() {
        let g = graph(
            &[
                "anikoto-text",
                "proptest",
                "dioxus",
                "anikoto-ui",
                "libmpv2",
                "gtk",
            ],
            &["anikoto-text", "anikoto-ui"],
            &[
                ("anikoto-text", "proptest", true),
                ("proptest", "dioxus", false),
                ("anikoto-ui", "libmpv2", false),
                ("anikoto-ui", "gtk", false),
            ],
        );
        let p = check_deps(&g);
        assert!(!p.iter().any(|m| m.contains("anikoto-text")), "{p:?}");
        assert!(
            p.iter()
                .any(|m| m.contains("anikoto-ui depends directly on libmpv2")),
            "{p:?}"
        );
        assert!(
            p.iter()
                .any(|m| m.contains("anikoto-ui depends directly on gtk")),
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
