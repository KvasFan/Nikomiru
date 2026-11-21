// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask android`: Nikomiru for Android as an APK (D-16, ADR 0009),
//! built from Linux with the NDK. The app is `libnikomiru_android.so`, loaded
//! by a NativeActivity; beside it, libmpv and FFmpeg from libmpv-android's
//! AAR. The APK carries the licenses and `SOURCES.txt`, which names the AAR,
//! its SHA-256 and where its source code is, as the GPL asks of whoever
//! passes it on. It is signed with the maintainer's key, which must stay the
//! same for every release so that Android installs one over the other.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::util;

/// The oldest Android: 9 (iconv in bionic, AAudio).
const MIN_SDK: u32 = 28;
/// The Android the app is made for: 14 (15 would draw it under the system
/// bars).
const TARGET_SDK: u32 = 34;

/// The ABIs built by default, with their Rust targets: phones and tablets,
/// then emulators and Chromebooks.
const ABIS: [(&str, &str); 2] = [
    ("arm64-v8a", "aarch64-linux-android"),
    ("x86_64", "x86_64-linux-android"),
];

/// The libraries of libmpv-android's AAR the app needs (not its Java
/// player's `libplayer.so`).
const MPV_LIBS: [&str; 9] = [
    "libmpv.so",
    "libavcodec.so",
    "libavdevice.so",
    "libavfilter.so",
    "libavformat.so",
    "libavutil.so",
    "libswresample.so",
    "libswscale.so",
    "libc++_shared.so",
];

const USAGE: &str = "cargo xtask android --libmpv <libmpv-….aar> [--abi arm64-v8a,x86_64] \
    [--keystore <release.jks>] [--pass-file <file with its password>] [--debug]";

/// The SDK and NDK folders and the tools taken from them.
struct Sdk {
    android_jar: PathBuf,
    build_tools: PathBuf,
    llvm: PathBuf,
}

/// The newest folder in `dir` by name (versions sort as numbers there).
fn newest(dir: &Path) -> Result<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| dir.display().to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort_by_key(|p| {
        p.file_name()
            .map(|n| {
                n.to_string_lossy()
                    .split(['.', '-'])
                    .map(|part| part.parse::<u64>().unwrap_or(0))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    entries
        .pop()
        .with_context(|| format!("nothing in {}", dir.display()))
}

/// The NDK's compilers and archiver for `target`, set on `cmd` as cargo and
/// the `cc` crate look for them (OpenSSL is built with them).
pub fn toolchain(cmd: &mut std::process::Command, llvm: &Path, target: &str) {
    let upper = target.to_uppercase().replace('-', "_");
    let under = target.replace('-', "_");
    let clang = llvm.join(format!("{target}{MIN_SDK}-clang"));
    cmd.env(format!("CARGO_TARGET_{upper}_LINKER"), &clang)
        .env(format!("CC_{under}"), &clang)
        .env(
            format!("CXX_{under}"),
            llvm.join(format!("{target}{MIN_SDK}-clang++")),
        )
        .env(format!("AR_{under}"), llvm.join("llvm-ar"));
}

/// The NDK's LLVM tools, if an SDK with an NDK is installed.
pub fn llvm() -> Option<PathBuf> {
    sdk().ok().map(|s| s.llvm).filter(|l| l.is_dir())
}

fn sdk() -> Result<Sdk> {
    let home = std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .context("set ANDROID_HOME to the Android SDK")?;
    let ndk = match std::env::var_os("ANDROID_NDK_HOME") {
        Some(ndk) => PathBuf::from(ndk),
        None => newest(&home.join("ndk"))?,
    };
    let platform = newest(&home.join("platforms"))?;
    Ok(Sdk {
        android_jar: platform.join("android.jar"),
        build_tools: newest(&home.join("build-tools"))?,
        llvm: ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin"),
    })
}

fn version(root: &Path) -> Result<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let manifest: toml::Table = toml::from_str(&text)?;
    manifest["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_string)
        .context("no workspace version")
}

/// The version code Android compares: 2.174.1 → 2174001.
fn version_code(version: &str) -> Result<u32> {
    let parts: Vec<u32> = version
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("version {version}"))?;
    match parts[..] {
        [major, minor, patch] if minor < 1000 && patch < 1000 => {
            Ok(major * 1_000_000 + minor * 1000 + patch)
        }
        _ => bail!("version {version} is not major.minor.patch"),
    }
}

fn sources(version: &str, aar: &Path, sha256: &str) -> String {
    let name = aar
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "Nikomiru {version} for Android\n\
         \n\
         Nikomiru is free software under the GNU GPL version 3 (LICENSE).\n\
         Its source code: https://github.com/KvasFan/nikomiru/tree/v{version}\n\
         \n\
         libmpv.so is mpv's library and libav*.so and libsw*.so are FFmpeg,\n\
         built by libmpv-android under the GNU GPL version 3 or later; \n\
         libc++_shared.so is the NDK's C++ library (Apache-2.0 with LLVM\n\
         exceptions). They come from {name} (SHA-256 {sha256}),\n\
         published on Maven Central as dev.jdtech.mpv:libmpv.\n\
         The sources: https://github.com/jarnedemeulemeester/libmpv-android,\n\
         the release of the same version, which names the versions of mpv,\n\
         FFmpeg and every library it builds.\n"
    )
}

/// `cargo xtask android --libmpv <aar> [--abi …] [--keystore …] [--pass-file …]`.
pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(aar) = value("--libmpv").map(PathBuf::from) else {
        bail!("usage: {USAGE}");
    };
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let keystore = value("--keystore")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("Android/nikomiru-release.jks"));
    let pass_file = value("--pass-file")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("Android/nikomiru-release.pass"));
    if !keystore.is_file() || !pass_file.is_file() {
        bail!(
            "no signing key: make one once and keep it (every release needs the same):\n  \
             keytool -genkeypair -keystore {} -alias nikomiru -keyalg RSA -keysize 4096 \
             -validity 20000 -dname CN=Nikomiru\n  and put its password in {}",
            keystore.display(),
            pass_file.display()
        );
    }
    let debug = args.iter().any(|a| a == "--debug");
    let wanted: Vec<String> = value("--abi")
        .map(|v| v.split(',').map(str::to_string).collect())
        .unwrap_or_else(|| ABIS.iter().map(|(abi, _)| abi.to_string()).collect());
    let abis: Vec<(&str, &str)> = ABIS
        .iter()
        .copied()
        .filter(|(abi, _)| wanted.iter().any(|w| w == abi))
        .collect();
    if abis.is_empty() {
        bail!("no known ABI in {wanted:?}");
    }
    let sdk = sdk()?;
    let work = root.join("target/apk");
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    std::fs::create_dir_all(&work)?;
    // the AAR's libraries
    let bytes = std::fs::read(&aar).with_context(|| aar.display().to_string())?;
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mpv_dir = work.join("libmpv");
    for (abi, _) in &abis {
        for lib in MPV_LIBS {
            let mut entry = archive
                .by_name(&format!("jni/{abi}/{lib}"))
                .with_context(|| format!("{lib} for {abi} is not in the AAR"))?;
            let out = mpv_dir.join(abi).join(lib);
            std::fs::create_dir_all(out.parent().context("no folder")?)?;
            std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
        }
    }
    // the app, for each ABI
    for (abi, target) in &abis {
        let upper = target.to_uppercase().replace('-', "_");
        let rustflags = format!(
            "{} -L native={}",
            util::release_rustflags(root),
            mpv_dir.join(abi).display()
        );
        let mut cargo = util::cargo();
        toolchain(&mut cargo, &sdk.llvm, target);
        util::run(
            cargo
                .env(format!("CARGO_TARGET_{upper}_RUSTFLAGS"), rustflags.trim())
                .args([
                    "build",
                    "--release",
                    "--locked",
                    "-p",
                    "nikomiru-android",
                    "--target",
                    target,
                    "--target-dir",
                    "target/android",
                ]),
        )?;
        let built = root
            .join("target/android")
            .join(target)
            .join("release/libnikomiru_android.so");
        let out = mpv_dir.join(abi).join("libnikomiru_android.so");
        util::run(
            std::process::Command::new(sdk.llvm.join("llvm-strip"))
                .arg("--strip-unneeded")
                .arg("-o")
                .arg(&out)
                .arg(&built),
        )?;
    }
    // the resources and the manifest
    let version = version(root)?;
    let manifest = std::fs::read_to_string(root.join("packaging/android/AndroidManifest.xml"))?
        .replace("@VERSION@", &version)
        .replace("@CODE@", &version_code(&version)?.to_string());
    let manifest_path = work.join("AndroidManifest.xml");
    std::fs::write(&manifest_path, manifest)?;
    let res = work.join("res.zip");
    util::run(
        std::process::Command::new(sdk.build_tools.join("aapt2"))
            .arg("compile")
            .arg("--dir")
            .arg(root.join("packaging/android/res"))
            .arg("-o")
            .arg(&res),
    )?;
    let linked_apk = work.join("linked.apk");
    let unaligned = work.join("unaligned.apk");
    util::run(
        std::process::Command::new(sdk.build_tools.join("aapt2"))
            .arg("link")
            .arg("-o")
            .arg(&linked_apk)
            .arg("-I")
            .arg(&sdk.android_jar)
            .arg("--manifest")
            .arg(&manifest_path)
            .args(["--min-sdk-version", &MIN_SDK.to_string()])
            .args(["--target-sdk-version", &TARGET_SDK.to_string()])
            .arg("-R")
            .arg(&res)
            .arg("--auto-add-overlay")
            // `--debug`: debuggable, for `adb shell run-as` (never a release)
            .args(debug.then_some("--debug-mode")),
    )?;
    // the libraries and the licenses into the APK
    let licenses = crate::licenses::about_for(
        root,
        "nikomiru-android",
        "aarch64-linux-android",
        "THIRD_PARTY_LICENSES-android.html",
    )?;
    let notes = sources(&version, &aar, &sha256);
    {
        // a new archive with aapt2's entries copied as they are, then ours
        let mut linked = zip::ZipArchive::new(std::fs::File::open(&linked_apk)?)?;
        let mut apk = zip::ZipWriter::new(std::fs::File::create(&unaligned)?);
        for i in 0..linked.len() {
            apk.raw_copy_file(linked.by_index_raw(i)?)?;
        }
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (abi, _) in &abis {
            let mut libs: Vec<PathBuf> = std::fs::read_dir(mpv_dir.join(abi))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            libs.sort();
            for lib in libs {
                let name = lib.file_name().context("no name")?.to_string_lossy();
                apk.start_file(format!("lib/{abi}/{name}"), deflated)?;
                std::io::copy(&mut std::fs::File::open(&lib)?, &mut apk)?;
            }
        }
        for (source, name) in [
            (licenses, "THIRD_PARTY_LICENSES.html"),
            (root.join("LICENSE"), "LICENSE.txt"),
        ] {
            apk.start_file(format!("assets/{name}"), deflated)?;
            std::io::copy(&mut std::fs::File::open(&source)?, &mut apk)?;
        }
        apk.start_file("assets/SOURCES.txt", deflated)?;
        apk.write_all(notes.as_bytes())?;
        apk.finish()?;
    }
    let aligned = work.join("aligned.apk");
    util::run(
        std::process::Command::new(sdk.build_tools.join("zipalign"))
            .args(["-P", "16", "-f", "4"])
            .arg(&unaligned)
            .arg(&aligned),
    )?;
    let out = work.join(format!("nikomiru-{version}-android.apk"));
    util::run(
        std::process::Command::new(sdk.build_tools.join("apksigner"))
            .arg("sign")
            .arg("--ks")
            .arg(&keystore)
            .arg("--ks-pass")
            .arg(format!("file:{}", pass_file.display()))
            .arg("--out")
            .arg(&out)
            .arg(&aligned),
    )?;
    util::run(
        std::process::Command::new(sdk.build_tools.join("apksigner"))
            .args(["verify", "--print-certs"])
            .arg(&out),
    )?;
    eprintln!("wrote {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_codes_grow_with_the_version() {
        assert_eq!(version_code("2.174.1").unwrap(), 2_174_001);
        assert!(version_code("2.180.0").unwrap() > version_code("2.174.12").unwrap());
        assert!(version_code("3.0.0").unwrap() > version_code("2.999.999").unwrap());
        assert!(version_code("2.1000.0").is_err());
        assert!(version_code("2.1").is_err());
    }
}
