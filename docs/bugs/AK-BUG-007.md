# AK-BUG-007 — Auto-pause can advance before the pause event arrives

- **Requirement**: SUB-026.
- **Found**: 2026-11-12, full GUI regression suite for 2.29.0.
- **Environment**: Linux, libmpv with `ao=null`, pause at each subtitle line.
- **Observed**: `playback_pauses_at_the_end_of_each_line` expected its first
  stop at 2.5 s but observed a pause at 5.433 s, with the next stop already
  8.5 s. The same test passed alone, so the player failure is intermittent.
- **Deterministic reproduction**: with ends at 2,500, 5,500 and 8,500 ms,
  current stop 2,500, position 2,500 and `paused=false`, the scheduler
  returned 5,500. Thus an end-position event arriving before the pause event
  could move mpv's end while its stop was pending. The new regression failed
  against the previous scheduler with `Some(5500)` instead of `Some(2500)`.
- **Fix**: retain the current stop while playing within the existing 100 ms
  endpoint margin. Once the pause is observed, arm the next stop normally.
  A seek beyond the margin still advances; no delay is added to the native
  pause itself.
- **Regression**: `the_stop_waits_for_the_pause_event_at_the_endpoint` in
  `crates/nikomiru-ui/src/subtitles.rs`, and the real-player auto-pause and
  gap-speed tests in `tests/e2e/{main,gaps}.rs`.
