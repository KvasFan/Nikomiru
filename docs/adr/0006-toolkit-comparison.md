# ADR 0006 — UI toolkit: egui instead of Qt Quick

| Field | Value |
|---|---|
| Status | **Accepted** (decision D-13, 2026-10-20): the maintainer chose egui after the comparison below |
| Date | 2026-10-20 |
| Requirements | NFR-PERF-03, NFR-PERF-06, NFR-A11Y-01, ARC-010, ARC-011, SPECIFICATION §3 (licenses) and §5.2 |
| Supersedes | [ADR 0003](0003-qt-quick-ui.md) (Qt 6 Quick, D-10) and the QML part of [ADR 0005](0005-popup-qml.md) |
| Spike | S5 — code in [`spikes/s5-toolkits/`](https://github.com/KvasFan/nikomiru/tree/v1.0.0/spikes/s5-toolkits) (removed after 1.0.0; in the tag `v1.0.0`) |

## Context

[ADR 0003](0003-qt-quick-ui.md) chose Qt 6 Quick (D-10) after Dioxus/GTK failed at 4K under CPU load, and M1–M3 were built on it. The maintainer asked to try the UI in **iced** and in **egui**, and to compare their performance with Qt's on the two things that weigh most in Nikomiru: the video with its overlay, and the dictionary popup with big entries ([ADR 0005](0005-popup-qml.md)).

## Method

- **The scenario of S1/S1q.** The Qt numbers come from S1q's binary, measured in the same session. mpv renders through the libmpv render API (OpenGL) with the options of PLY-005, under S1q's overlay:
  - a bar with two buttons and click and key counters;
  - a fake subtitle of 27 graphemes, each clickable, white with a black outline, changing about 30 times per second.
- **The same measurement in the three binaries.** It starts 3 s after playback passes 0.5 s and lasts 20 s of playback. It records:
  - mpv's `frame-drop-count` and `vo-delayed-frame-count`;
  - renders per second, and dropped frames per second of the window;
  - process CPU and RSS from `/proc`;
  - subtitle changes per second.
- **Environment.** The runs happen in an off-screen virtual KWin, at 3840×2160 (1920×1080 for the first case). For each case the three toolkits run one after the other.
- **Sources.** 1080p60 H.264 and HEVC 10-bit test clips, shown at 1080p or scaled to 2160p, and a real 2160p60 HEVC 10-bit clip (`testsrc2`; see "How to repeat").
- **Load.** Busy loops at normal priority: one per hardware thread (×1, 16 loops) or three (×3, 48 loops). They press less on the UI threads than the nice −4 builds of ADR 0003.
- **Controls.**
  - S1q without its overlay: how much of Qt's CPU is the overlay.
  - egui with iced's copy-back decoding: how many of iced's dropped frames the copy explains.
- **The dictionary popup**, from Nikomiru's real view model. The content is what the Qt app produced for five lookups with Jitendex and JMdict: 今日は学校に行きたくない, 学校に行きたくない, 行きたくない, 食べてしまった猫が好きです, 猫が好きです.
  - Each popup is shown three times in a 1280×720 window.
  - Timing runs from the state change to the frame that draws the popup.
  - The end mark is a paint callback inside the popup (egui) or a custom primitive drawn after the frame's text is prepared (iced). Both are before the buffer swap.
- **Review.** A first round of measurements was reviewed adversarially against the raw data and the code. 39 problems were confirmed, among them:
  - "4K" cases that decoded 1080p sources;
  - a CPU probe that read every process in `/proc` on the UI thread at the start of the window, which caused the dropped frames seen in the first second;
  - unequal subtitle rates;
  - egui's popup measured at 8% opacity, during its fade-in;
  - iced's probe placed before text preparation;
  - claims that no measurement supported.

  The results below come from the corrected spike.

## Integrating mpv

- **egui 0.37** (eframe on OpenGL through glow), about 60 lines:
  - The GL context is current in paint callbacks. mpv draws into the window's framebuffer from a callback painted under everything else, and egui draws the overlay on top.
  - VA-API without copies works at once on Wayland.
  - On X11, eframe 0.37 picks GLX before EGL (`ApiPreference::FallbackEgl`), and mpv cannot import VA-API surfaces in a GLX context. An app needs its own integration (winit, glutin with EGL first, egui_glow), which egui supports.
- **iced 0.14** (wgpu):
  - libmpv renders only through OpenGL, so wgpu has to run on its OpenGL backend. mpv renders into a GL texture created in wgpu's own context and wrapped as a wgpu texture, and a `shader` widget samples it.
  - mpv's rendering changes GL state that wgpu-hal sets only once: its vertex array (which gave a white window) and its sampler objects (which gave black frames). Saving and restoring that state fixes both.
  - **VA-API without copies gives garbled frames** in wgpu's context: the dmabuf import succeeds, but the planes read as noise. Within the time box, iced decodes with a copy back to memory (`vulkan-copy`).
- **Qt** (S1q): the C++ `QQuickFramebufferObject` item of ADR 0003. It renders on Qt Quick's render thread into an FBO, which the scene graph then composites.

## Results

Test machine: Ryzen 7 7800X3D (8 cores, 16 threads), RX 7900 XTX; virtual KWin on Wayland.

### Video

Two clean runs per case, except the first two cases (one each). Each cell gives dropped frames in 20 s, then process CPU in % of one core.

| Case | Qt Quick | egui | iced |
|---|---|---|---|
| 1080p60 H.264 at 1080p, idle | 0 · 5.8% | 0 · 3.2% | 0 · 10.6% |
| 1080p60 H.264 scaled to 2160p, idle | 0 · 5.9% | 0 · 3.3% | 0 · 10.6% |
| 1080p60 HEVC 10-bit scaled to 2160p, idle | 0 · 6.5–6.9% | 0 · 3.3–3.4% | 0 · 11.0–11.2% |
| 2160p60 HEVC 10-bit, idle | 0 · 6.4–6.6% | 0 · 3.6% | 0–1 · 18.1–18.2% |
| 2160p60 HEVC 10-bit, load ×1 | 0–1 · 7.2–7.3% | 0–1 · 4.0% | 14–15 · 20.7% |
| 1080p60 HEVC 10-bit scaled to 2160p, load ×3 | 0 · 6.5–6.6% | 1 · 3.7% | 61–73 · 12.5–13.4% |
| **2160p60 HEVC 10-bit, load ×3** | **0** · 6.7% | **2** · 3.7–3.9% | **146–186** · 18.4–18.8% |

Common to all rows:
- **Renders per second:** 60.0, except iced under load (59.3, then 56.4–56.9 and 50.7–52.7).
- **Delayed frames:** 0 everywhere.
- **RSS:** Qt 226–293 MiB, egui 226–291 MiB, iced 228–414 MiB (the copy-back buffers at 4K).
- **Decoding:** `vaapi` for Qt and egui, `vulkan-copy` for iced.

Controls:
- **S1q without its overlay** (2160p60 HEVC 10-bit, idle): 0 dropped, 5.5–5.8% CPU, against 6.4–6.6% with it.
- **egui with copy-back decoding** (2160p60 HEVC 10-bit, load ×3): 36–45 dropped (57.8–58.2 renders/s), 13.7–13.9% CPU, 416–441 MiB.

What the data shows:
- **Idle.** All three keep 60 renders per second with no dropped frames at every size, 4K HEVC 10-bit included; iced dropped one frame in one run.
- **CPU.** egui uses about half of Qt's process CPU: 3.2–4.0% against 5.8–7.3%.
  - Qt's overlay accounts for about 1 point of that (the no-overlay control).
  - The other ~2 points are in Qt's video path and frame loop: the FBO and its composition by the scene graph, and two threads. This spike does not split them further.
- **Heavy load.** Qt and egui hold the video: 0–2 dropped frames in 20 s.
  - These drops no longer fall in the first second: those came from the probe reading `/proc` on the UI thread.
  - iced drops 61–186.
- **What explains iced's drops.** The copy-back decoding accounts for part of them: egui with the same decoding drops 36–45 at 4K under load ×3, while iced drops 146–186. The rest comes from iced's own frame path (wgpu on GL and the texture hand-off), which this spike does not split further.
- **Subtitle rate.**
  - Qt and egui: 24–30 changes per second. The 33 ms throttle is checked on each video frame, so a late frame skips one change.
  - iced: 30 per second, from a 33 ms poll.
- **Excluded runs.** A second repetition ran while other work used the machine. The 1-minute load averages give an estimated 15–33 extra runnable threads during its load phases. Its numbers keep the same order but with more drops for all three: Qt up to 38, egui up to 13, iced up to 375. They are not in the table, and neither are two cases of a third run disturbed in the same way. The raw reports keep them all.

### Dictionary popup

Each cell gives milliseconds; for egui and iced, the first showing, then the two repeats.

| Lookup | egui | iced | Qt app: content → first frame, then all created | Qt app: click → first frame (ADR 0005) |
|---|---|---|---|---|
| 今日は学校に行きたくない | 5.6–7.0, then 2.7–3.1 | 40–50, then 34–36 | 14–15, then 113–117 | 51–66 |
| 学校に行きたくない | 1.3–1.6, then 0.7–1.0 | 3.9–4.7, then 1.7–2.8 | 4, then 17–25 | 20 |
| 行きたくない (行く) | 6.2–6.7, then 3.9–4.2 | 45–47, then 34–35 | 25–37, then 126–131 | 53–64 |
| 食べてしまった猫が好きです | 2.3–3.1, then 1.7–1.9 | 20.5–20.7, then 16.9–17.4 | 6–7, then 67–81 | 28–40 |
| 猫が好きです | 1.8–1.9, then 1.1–1.4 | 6.7–7.0, then 5.0–5.6 | 8–12, then 20–23 | 22–27 |

- **egui**
  - It lays out and draws the whole entry in its first frame.
  - A new area is first sized in an invisible pass. A second pass in the same frame (`Context::request_discard`) draws it; without that pass, the popup would appear one frame later (about 17 ms at 60 Hz).
  - While the popup stays open, the whole UI pass (popup included) takes 0.34–0.37 ms at p50 and 0.90–0.97 ms at p95 per frame. Tessellation and painting come on top.
- **iced**
  - It builds the whole entry each time the popup is shown. Text layout and glyph preparation (cosmic-text) take most of the time.
  - While the popup stays open, rebuilding the view costs 0.10–0.12 ms at p50. Layout and drawing come on top.
- **Qt**
  - These numbers come from the real app, instrumented on 2026-10-20: release build, Jitendex and JMdict, a 1280×720 window in a 1600×900 virtual KWin, load average 5–6, three runs.
  - Timing runs from the arrival of the content (a JSON string that QML parses) to the first `frameSwapped`, so it ends after the swap.
  - The app creates the first part of the entry (about the first sense) at once, and the rest in the next frames.
- **How they compare.** The end points differ: the spikes start from parsed content and stop before the swap. So only the order of magnitude compares:
  - egui draws a whole big entry in under 7 ms, in one frame;
  - Qt shows its first part in 4–37 ms and completes the big entries in 70–130 ms;
  - iced shows the whole entry in 2–50 ms.

### Other criteria

| | Qt Quick (D-10) | egui 0.37 | iced 0.14 |
|---|---|---|---|
| Video integration | C++ item on the render thread; zero-copy VA-API | Rust: about 60 lines in the spike; zero-copy VA-API on Wayland; on X11, an own integration instead of eframe (EGL) | wgpu on GL, GL state saved and restored; no zero-copy VA-API found |
| Dependencies | cxx-qt and its build crates; Qt itself is a system library (10 libraries) | 243 crates in the spike with production features (AccessKit brings the AT-SPI stack and zbus) | 187 crates in the spike |
| Licenses outside class A (SPECIFICATION §3.2) | none | 1: `libloading` (ISC), through glutin and winit's system libraries | 6: `libloading` (ISC); `slotmap` (Zlib); `arrayref` (BSD-2); `tiny-skia` and `tiny-skia-path` (BSD-3, through winit's Wayland decorations); `hexf-parse` (**CC0**, forbidden without exception, through wgpu's shader compiler, so unavoidable) |
| Stripped binary (spike) | 1.75 MiB, plus Qt | 14.4 MiB | 13.4 MiB |
| Accessibility (NFR-A11Y-01) | Qt Accessible (AT-SPI) | AccessKit (AT-SPI) | none in 0.14 |
| Japanese input (IME) | mature | through winit (text-input on Wayland, XIM on X11) | through winit |
| Text | HarfBuzz, system font fallback | OpenType shaping (harfrust); system fonts on demand through fontique (fontconfig) | cosmic-text, system font fallback |
| UI model | QML plus Rust objects (cxx-qt): a second language, with the pitfalls of ADR 0005 | immediate mode, all Rust | Elm architecture, all Rust |

## Decision

**The maintainer chose egui (D-13). It replaces Qt 6 Quick (D-10).**

- **Video.** egui equals Qt on the video: both keep 4K HEVC 10-bit at 60 frames per second under heavy load without copies, and egui uses about half of Qt's CPU.
- **Popup.** egui draws a whole big entry in a few milliseconds and in one frame, with no incremental creation.
- **One language.** Everything is Rust: no QML, no C++ and no cxx-qt, so one language and one build. The QML pitfalls of ADR 0005 go away.
- **Licenses.** egui needs one class B exception, `libloading` (ISC). The maintainer approved it with the decision.
- **iced is not chosen**, for these reasons:
  - no zero-copy video was found;
  - it drops many frames under load, more than its copy-back decoding explains;
  - its popup is the slowest on big entries;
  - it has no accessibility;
  - it depends on a CC0 crate, which the license policy forbids.

## Consequences

- **An own integration instead of eframe.** The stack is winit, glutin (EGL first, GLX as fallback), egui-winit and egui_glow, so that X11 keeps zero-copy VA-API. For each frame:
  1. mpv draws the video into the window's framebuffer;
  2. egui paints the interface over it;
  3. the app swaps the buffers and reports the swap to mpv.
- **The UI and the video share one thread.** mpv renders on the UI thread, and that thread must never wait for the mpv core (`<mpv/render.h>`, "Threading"). The player's commands therefore move to a command thread.
  - Work that took milliseconds on Qt's GUI thread now delays video frames: library reads, directory listings, dictionary registry writes.
  - Such work moves off the UI thread where it matters.
- **What is rewritten and what stays.**
  - Rewritten: the Qt-specific parts, meaning the QML, the cxx-qt bridge and the C++ video item.
  - Kept: the Rust logic (session, subtitles, popup state, dictionaries, shortcuts, i18n), the harness protocol and the L6 tests.
- **A typed popup view model.** No StyledText and no JSON hop; images are decoded on the lookup thread.
- **Platform gaps to close.**
  - Drag and drop on Wayland, which winit 0.30 lacks.
  - File dialogs, through the XDG portal and the own D-Bus client.
  - Window activation on Wayland.
- **Client-side decorations.** winit's Adwaita decorations need `tiny-skia` (BSD-3), which would be a further class B exception. Without them, compositors that do not draw server-side decorations (GNOME) show the window without a title bar.
- **Checks still to do.**
  - On the developer's real session: fractional scale, Japanese IME (fcitx5, ibus), AccessKit with Orca, X11 with EGL, 50 opens and closes.
  - T-PERF-03 and T-PERF-06 on the egui build.
  - On REF hardware (4 cores, 8 threads, integrated GPU), which this spike did not measure.
- **Documents.** D-13 goes in SPECIFICATION §10.1, and the work track "UI on egui" in ROADMAP M3.

## After the port

The finished egui interface (2026-10-20), measured like the spike with `measure_app.py` (the release build with the test harness, fullscreen at 3840×2160 in the off-screen KWin, 20 s after the warm-up, a fresh profile each run). Another project's tests were running on the machine at the same time, so these numbers are indicative; the spike's above remain the reference.

| 2160p60 HEVC 10-bit | Dropped frames in 20 s | CPU | Memory |
|---|---|---|---|
| idle (2 runs) | 0 | 3.2–3.6% | 258 MiB |
| load ×3 (4 runs) | 0, 1, 2, 5 | 3.2–4.3% | 243–261 MiB |

Decoding was VA-API zero-copy (`hwdec-current` = `vaapi`) in every run. The app draws the same as the spike, plus the control bar and the subtitle layer: the video holds as it did there.

## How to repeat

```sh
cd spikes/s1q-qt && QMAKE=qmake6 cargo build --release && cd ../..
cd spikes/s5-toolkits && cargo build --release && cd ../..
ffmpeg -f lavfi -i testsrc2=size=3840x2160:rate=60 -f lavfi -i sine=frequency=440:sample_rate=48000 \
  -t 30 -c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -c:a aac -ac 1 target/fixtures/av_2160p60_hevc10.mkv
spikes/s5-toolkits/measure-all.sh target/spike-s5 [POPUPS_FILE] [REPETITIONS]
```

`measure-all.sh` writes one JSON report per run. Each report records:
- the window's 1-minute load average at the start and the end;
- dropped frames per second of the window;
- subtitle changes per second.

Keep the machine otherwise idle: other work changes the results, as in the excluded repetition.
