// SPDX-License-Identifier: GPL-3.0-only

//! The word audio player (AUD-005): a second mpv core, audio only, apart
//! from the video's, so playing a word changes nothing of the player.

use std::path::Path;

use libmpv2::Mpv;

use crate::player::PlayerError;

/// An audio-only mpv core.
pub struct WordPlayer {
    mpv: Mpv,
}

impl std::fmt::Debug for WordPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WordPlayer").finish_non_exhaustive()
    }
}

/// The options of the core: no video, no configuration, no scripts, nothing
/// from the user's mpv.
const OPTIONS: [(&str, &str); 10] = [
    ("vid", "no"),
    ("video", "no"),
    ("audio-display", "no"),
    ("config", "no"),
    ("input-default-bindings", "no"),
    ("terminal", "no"),
    ("idle", "yes"),
    ("ytdl", "no"),
    ("load-scripts", "no"),
    ("osc", "no"),
];

impl WordPlayer {
    /// Starts the core; `ao` is the audio output of the settings, if any
    /// (`null` in the tests).
    pub fn new(ao: Option<&str>) -> Result<Self, PlayerError> {
        let mpv = Mpv::with_initializer(|init| {
            for (k, v) in OPTIONS {
                init.set_property(k, v)?;
            }
            if let Some(ao) = ao {
                init.set_property("ao", ao)?;
            }
            Ok(())
        })?;
        Ok(Self { mpv })
    }

    /// Plays `file` at `volume` percent, instead of the word playing.
    pub fn play(&self, file: &Path, volume: f64) -> Result<(), PlayerError> {
        self.mpv.set_property("volume", volume.clamp(0.0, 100.0))?;
        let path = file.to_string_lossy();
        self.mpv.command("loadfile", &[&path, "replace"])?;
        Ok(())
    }

    /// Stops the word playing.
    pub fn stop(&self) {
        let _ = self.mpv.command("stop", &[]);
    }

    /// Whether a word is playing, for the tests.
    pub fn playing(&self) -> bool {
        self.mpv
            .get_property::<bool>("idle-active")
            .is_ok_and(|idle| !idle)
    }
}
