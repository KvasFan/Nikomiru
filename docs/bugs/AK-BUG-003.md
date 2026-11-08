# AK-BUG-003: no sound after the pause at the end of a line

**Symptom.** With the pause at the end of each line on (SUB-026), playback stops at the end of a line as it should, but Space plays the next line without its sound. Reported by the maintainer on 2.15.0.

**Steps to reproduce.** Turn on Shift+P, let a line end, press Space. mpv's `audio-pts` stays empty while `time-pos` moves on.

**Cause.** Nikomiru stops playback with mpv's `end` property, as the cue replay of SUB-013 does. Reaching `end` takes the audio to its end-of-file state; moving `end` further and unpausing starts the picture again, not the audio. The replay from the popup had the same fault once it had stopped at its end.

**Fix.** When playback leaves a stop of `end`, the session seeks to where it is (`absolute+exact`), which starts the audio again: in the pause at the end of each line once it has stopped at a line, and when a replay that reached its end is left. Test `playback_pauses_at_the_end_of_each_line` waits for `audio-pts` past the stop.
