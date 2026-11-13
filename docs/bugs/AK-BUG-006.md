# AK-BUG-006 — Gap speed can restore after the next line starts

- **Requirement**: PLY-022.
- **Found**: 2026-11-12, full GUI regression suite, before releasing M23.
- **Environment**: Linux, libmpv with `ao=null`, gap speed 4×, normal speed 1×,
  pause at the end of each subtitle line.
- **Reproduction**: subtitles at 1–2 s and 7–8 s. Resume after the first line's
  stop and watch the gap. The observed position/speed can go from 6.2 s at 4×
  directly to 7.533 s at 1×, leaving no normal-speed lead before speech.
- **Expected**: the user's speed is restored before the next line, with a lead
  for its audio.
- **Fix**: reserve additional lead for audio queued at the old rate, proportional
  to the difference between normal and accelerated speed, besides the dialogue
  padding and position-event margin.
- **Regression**: `gaps::gap_speed_restores_dialogue_and_keeps_the_pause_at_each_line`
  in `tests/e2e/gaps.rs` records the trajectory and requires the restored speed
  before the second line. The pure gap tests cover the rate-dependent lead.
