// SPDX-License-Identifier: GPL-3.0-only

//! Explicit Linux distribution-package installation for Poppler (DOC-004).
//! Ordinary PDF reads never enter this module. Package transactions belong to the OS.
use nikomiru_core::Cancel;
use nikomiru_media::process::{Job, run};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    manager: &'static str,
    args: Vec<&'static str>,
}
#[cfg(any(target_os = "linux", test))]
fn plan(os: &str) -> Option<Plan> {
    let field = |key: &str| {
        os.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|v| v.trim().trim_matches(['\"', '\'']))
            .unwrap_or("")
    };
    let ids = format!("{} {}", field("ID="), field("ID_LIKE="));
    for id in ids.split_ascii_whitespace() {
        let p = match id {
            "arch" | "manjaro" | "endeavouros" => Plan {
                manager: "pacman",
                args: vec!["-S", "--needed", "--noconfirm", "poppler", "poppler-data"],
            },
            "debian" | "ubuntu" | "linuxmint" => Plan {
                manager: "apt-get",
                args: vec![
                    "install",
                    "--yes",
                    "--no-install-recommends",
                    "poppler-utils",
                    "poppler-data",
                ],
            },
            "fedora" | "rhel" | "centos" => Plan {
                manager: "dnf",
                args: vec!["install", "--assumeyes", "poppler-utils", "poppler-data"],
            },
            "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "sles" => Plan {
                manager: "zypper",
                args: vec![
                    "--non-interactive",
                    "install",
                    "poppler-tools",
                    "poppler-data",
                ],
            },
            _ => continue,
        };
        return Some(p);
    }
    None
}
#[cfg(target_os = "linux")]
fn trusted(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let p = std::fs::canonicalize(Path::new("/usr/bin").join(name)).ok()?;
    let m = std::fs::metadata(&p).ok()?;
    (m.is_file()
        && m.uid() == 0
        && m.permissions().mode() & 0o022 == 0
        && m.permissions().mode() & 0o111 != 0)
        .then_some(p)
}
fn native_plan() -> Option<(PathBuf, PathBuf, Vec<&'static str>)> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            return None;
        }
        let p = plan(&std::fs::read_to_string("/etc/os-release").ok()?)?;
        Some((trusted("pkexec")?, trusted(p.manager)?, p.args))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
/// Whether this native host offers an explicitly authenticated supported package action.
pub fn available() -> bool {
    native_plan().is_some()
}
fn execute(auth: &Path, manager: &Path, args: &[&str], cancel: &Cancel) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Err("setup-tool-cancelled".into());
    }
    let mut command = vec![manager.as_os_str().to_os_string()];
    command.extend(args.iter().map(std::ffi::OsString::from));
    let mut job = Job::new(auth, command);
    job.timeout = Duration::from_secs(3600);
    // Once authenticated, let the OS finish its transaction. Stop prevents subsequent setup jobs.
    // Killing a privileged package manager cannot safely roll back its database or dependencies.
    run(&job, &Cancel::new()).map_err(|e| e.to_string())
}
/// Install the explicitly selected native PDF package, then validate its real executable.
pub fn install(cancel: &Cancel) -> Result<PathBuf, String> {
    let (auth, manager, args) = native_plan().ok_or("setup-tool-unsupported")?;
    execute(&auth, &manager, &args, cancel)?;
    let path = crate::document_support::find_pdf("").ok_or("setup-tool-missing")?;
    // A completed transaction remains a real result even if Stop arrived while it ran.
    crate::document_support::validate_pdf(&path, &Cancel::new()).map_err(|e| e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    // T-DOC-004: fixed package-manager plans resolve Poppler/CMap data without shell interpolation or system upgrades.
    #[test]
    fn native_package_plans_are_fixed_and_unknown_distributions_fail_closed() {
        assert_eq!(
            plan("ID=arch\n").unwrap().args,
            ["-S", "--needed", "--noconfirm", "poppler", "poppler-data"]
        );
        assert_eq!(
            plan("ID='ubuntu'\nID_LIKE=debian\n").unwrap().manager,
            "apt-get"
        );
        assert_eq!(
            plan("ID=custom\nID_LIKE=\"fedora rhel\"\n")
                .unwrap()
                .manager,
            "dnf"
        );
        assert!(plan("ID=unknown\n").is_none());
        assert!(plan("ID=$(touch surprise)\n").is_none());
        for os in ["arch", "ubuntu", "fedora", "opensuse-tumbleweed"] {
            let p = plan(&format!("ID={os}")).unwrap();
            assert!(p.args.contains(&"poppler-data"));
            assert!(
                !p.args
                    .iter()
                    .any(|s| s.contains("upgrade") || s.contains("refresh"))
            );
        }
    }
    // T-DOC-004: execute the real argument-vector path against a harmless fake authenticator; no real OS package action runs in tests.
    #[cfg(unix)]
    #[test]
    fn explicit_package_hook_preserves_arguments_and_honours_preflight_cancellation() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join("auth helper");
        std::fs::write(&auth,"#!/bin/sh\n[ \"$1\" = '/manager with spaces' ] && [ \"$2\" = 'install' ] && [ \"$3\" = 'poppler-utils' ]\n").unwrap();
        std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o700)).unwrap();
        execute(
            &auth,
            Path::new("/manager with spaces"),
            &["install", "poppler-utils"],
            &Cancel::new(),
        )
        .unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        assert!(
            execute(
                Path::new("/never-spawn"),
                Path::new("/never-manager"),
                &[],
                &cancel
            )
            .unwrap_err()
            .contains("cancelled")
        );
    }
}
