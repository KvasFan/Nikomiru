// SPDX-License-Identifier: GPL-3.0-only

//! Screenshots of the frame shown, without subtitles (PLY-011): a PNG in
//! `$XDG_PICTURES_DIR/Nikomiru` named after the file and the position.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_player::PlayerHandle;

/// How long mpv may take to write the file.
const TIMEOUT: Duration = Duration::from_secs(10);

/// File names stay well under the 255 bytes most file systems allow.
const MAX_STEM_BYTES: usize = 200;

/// `<file name> <HH-MM-SS.mmm>.png` for a video at `seconds`.
pub fn file_name(video: &Path, seconds: f64) -> String {
    let stem = video
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut end = stem.len().min(MAX_STEM_BYTES);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60);
    format!("{} {h:02}-{m:02}-{s:02}.{:03}.png", &stem[..end], ms % 1000)
}

/// `path`, or `name (2).png`, `name (3).png`… if it is taken.
fn free(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}).png")))
        .find(|p| !p.exists())
        .unwrap_or(path)
}

/// Saves the frame shown as `dir/name` (or a free name next to it) on a
/// thread of its own, and calls `done` with the file written or the reason it
/// was not.
pub fn start(
    player: PlayerHandle,
    dir: PathBuf,
    name: String,
    done: Box<dyn FnOnce(Result<PathBuf, String>) + Send>,
) {
    let spawned = std::thread::Builder::new()
        .name("screenshot".into())
        .spawn(move || done(save(&player, &dir, &name)));
    if let Err(e) = spawned {
        tracing::error!("cannot start the screenshot thread: {e}");
    }
}

fn save(player: &PlayerHandle, dir: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = free(dir.join(name));
    let reply = player.screenshot(&path).map_err(|e| e.to_string())?;
    match reply.recv_timeout(TIMEOUT) {
        Ok(Ok(())) => Ok(path),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-PLY-011
    #[test]
    fn the_name_holds_the_file_and_the_position() {
        let video = Path::new("/videos/Frieren - 01.mkv");
        assert_eq!(file_name(video, 0.0), "Frieren - 01 00-00-00.000.png");
        assert_eq!(file_name(video, 3723.4567), "Frieren - 01 01-02-03.457.png");
        assert_eq!(file_name(video, -1.0), "Frieren - 01 00-00-00.000.png");
        let long = format!("/v/{}.mkv", "字".repeat(100));
        let name = file_name(Path::new(&long), 1.0);
        assert!(name.len() < 230, "{}", name.len());
        assert!(name.ends_with(" 00-00-01.000.png"));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a 00-00-01.000.png");
        assert_eq!(free(first.clone()), first);
        std::fs::write(&first, b"x").unwrap();
        let second = free(first.clone());
        assert_eq!(second, dir.path().join("a 00-00-01.000 (2).png"));
        std::fs::write(&second, b"x").unwrap();
        assert_eq!(free(first), dir.path().join("a 00-00-01.000 (3).png"));
    }
}
