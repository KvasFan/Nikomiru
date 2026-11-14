// SPDX-License-Identifier: GPL-3.0-only

//! Pinned optional executable downloads (INS-007); loading this does no I/O.

use serde::Deserialize;

/// One platform-specific, verified upstream release artifact.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artifact {
    /// OS and architecture, e.g. linux-x86_64.
    pub platform: String,
    /// Immutable upstream release URL.
    pub url: String,
    /// Mandatory SHA-256 before extraction or execution.
    pub sha256: String,
    /// Exact compressed/downloaded bytes.
    pub bytes: u64,
    /// raw, zip or tar.xz.
    pub format: String,
    /// Exact allowed executable basenames, never archive-provided destinations.
    pub programs: Vec<String>,
}

/// An optional support program, with its explicit dependencies and provenance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tool {
    /// Stable assistant id.
    pub id: String,
    /// Upstream name.
    pub name: String,
    /// Pinned release identifier.
    pub version: String,
    /// License of the distributed executable.
    pub license: String,
    /// Upstream source repository.
    pub source: String,
    /// Other tool ids required by this tool.
    pub dependencies: Vec<String>,
    /// Supported OS/architecture artifacts.
    pub artifacts: Vec<Artifact>,
}

/// Shipped catalog; never resolved from a mutable latest URL at install time.
pub fn catalog() -> Vec<Tool> {
    serde_json::from_str(include_str!("../../../../assets/setup/tools.json")).unwrap_or_default()
}

/// This binary's target, with no guesses about a different architecture.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Dependencies before dependants, deduplicated; unknown/cyclic ids fail closed.
pub fn resolve(catalog: &[Tool], requested: &[String]) -> Option<Vec<String>> {
    fn visit(
        id: &str,
        catalog: &[Tool],
        visiting: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Option<()> {
        if out.iter().any(|s| s == id) {
            return Some(());
        }
        if visiting.iter().any(|s| s == id) {
            return None;
        }
        let tool = catalog.iter().find(|t| t.id == id)?;
        visiting.push(id.into());
        for dep in &tool.dependencies {
            visit(dep, catalog, visiting, out)?;
        }
        visiting.pop();
        out.push(id.into());
        Some(())
    }
    let mut out = Vec::new();
    for id in requested {
        visit(id, catalog, &mut Vec::new(), &mut out)?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-INS-007: all supported architectures have pinned assets; dependency closure is explicit.
    #[test]
    fn pinned_platforms_and_dependency_order() {
        let c = catalog();
        assert_eq!(c.len(), 3);
        assert_eq!(
            resolve(&c, &["yt-dlp".into()]).unwrap(),
            ["ffmpeg", "deno", "yt-dlp"]
        );
        for tool in &c {
            for platform in [
                "linux-x86_64",
                "linux-aarch64",
                "windows-x86_64",
                "windows-aarch64",
            ] {
                let a = tool
                    .artifacts
                    .iter()
                    .find(|a| a.platform == platform)
                    .unwrap();
                assert!(a.url.starts_with("https://github.com/"));
                assert!(!a.url.contains("/latest/"));
                assert_eq!(a.sha256.len(), 64);
                assert!(a.bytes > 0 && a.bytes < 512 << 20);
            }
        }
        assert!(resolve(&c, &["unknown".into()]).is_none());
        let mut cyclic = c;
        cyclic[0].dependencies = vec!["yt-dlp".into()];
        assert!(resolve(&cyclic, &["yt-dlp".into()]).is_none());
    }
}
