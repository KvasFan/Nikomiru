# Releasing

How a release is built and published from the maintainer's Linux machine, without a CI service. Each step names the command and what to check.

## 1. Version and checks

1. Raise `[workspace.package] version` in `Cargo.toml`, run `cargo update -w`, move the CHANGELOG's Unreleased entries under the new version and write `docs/qa/<version>.md`.
2. Regenerate the Flatpak sources and notices: `cargo xtask flatpak-sources`.
3. Run `cargo xtask ci --full` once. It must be all green.
4. Merge into `main` and push the annotated tag `v<version>` on the merge commit.

## 2. Files

All of them are built from the tagged tree.

| File | Command | Check |
|---|---|---|
| `nikomiru-<v>-linux-x86_64-glibc<g>-openssl<o>.tar.gz` and `.tar.xz` | `cargo xtask linux` (into `target/dist/`) | The task refuses a program that does not report `nikomiru <v>`; the name carries the glibc and OpenSSL it needs. |
| `nikomiru-<v>-x86_64.flatpak` | `cargo xtask flatpak`, then copy `target/nikomiru.flatpak` | `flatpak install --user` it, then `flatpak run --command=nikomiru io.github.kvasfan.Nikomiru --version` |
| `nikomiru-<v>-windows-x86_64.zip` and `.msi` | `cargo xtask windows --libmpv <mpv-dev-x86_64-….7z> --ffmpeg <ffmpeg-…-win64-gpl-….zip>` (the MSI needs `wixl` from msitools) | Under Wine, `nikomiru.exe --version`, and `msiexec /i` then `/x`. |
| `nikomiru-<v>-android.apk` | `cargo xtask android --libmpv <libmpv-….aar>` with `ANDROID_HOME` set to an SDK with the NDK; the AAR is `dev.jdtech.mpv:libmpv` from Maven Central. It is signed with `~/Android/nikomiru-release.jks` (password in `~/Android/nikomiru-release.pass`): keep both safe and outside the repository, since every later APK must be signed with the same key for Android to install it over the earlier one | `apksigner verify --print-certs` shows the same certificate as before; on an emulator, `adb install`, then the library and a video. |
| `nikomiru-<v>-source.tar.gz` | Download `https://github.com/KvasFan/Nikomiru/archive/refs/tags/v<v>.tar.gz` | Its SHA-256 goes into `packaging/aur/PKGBUILD`. |
| `PKGBUILD` | `packaging/aur/PKGBUILD` with `pkgver` and `sha256sums` updated | `makepkg --verifysource` |
| `SHA256SUMS` | `sha256sum nikomiru-<v>-* PKGBUILD > SHA256SUMS` | — |

Release programs are built with the build machine's paths remapped (`util::release_rustflags`), so no home folder ends up in them.

## 3. Publishing

1. Create the release with all the files: `gh release create v<v> --verify-tag --latest --title … --notes-file … <files>`.
2. Compare the uploaded digests with `SHA256SUMS`: `gh release view v<v> --json assets`.
3. Open a packaging PR against `main`. It pins `packaging/aur/PKGBUILD` and the Flatpak manifest (`tag`, `commit`) to the new tag and records the publication in `docs/qa/<v>.md`.

## 4. Signed updates (UPD-001…UPD-004)

Only once the maintainer's key exists and its public line is set in `nikomiru_online::updates::PRODUCTION_KEY`:

1. Make the manifest: `cargo xtask update-manifest --channel stable target/dist/nikomiru-<v>-linux-x86_64-….tar.xz nikomiru-<v>-windows-x86_64.zip`. This writes `target/dist/stable.json`; only `.tar.xz` and `.zip` archives can be installed by the updater.
2. Sign it: `minisign -S -s ~/.minisign/nikomiru.key -m target/dist/stable.json -t "nikomiru stable <v>"`.
3. Upload `stable.json` and `stable.json.minisig` to the GitHub release tagged `updates` with `--clobber`. That is the feed address in `nikomiru_online::updates::FEED`.

Never sign a manifest for an older release than the one already published. Installs that accepted the newer one refuse it, and so do the others once they see the newer one (UPD-002).
