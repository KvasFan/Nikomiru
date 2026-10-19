# ADR 0001 — Video and UI composition in the same window

| Field | Value |
|---|---|
| Status | **Under review** (revision of 2026-10-19, see "4K and CPU contention"): the GtkOverlay composition holds at 1080p on an idle machine, but not at 4K under load; spike S1b proposed |
| Date | 2026-10-19 |
| Requirements | NFR-PERF-03, SPECIFICATION §5.2, ARC-001…ARC-004, PLY-005, PLY-009 |
| Risk | R-1 (transparent webview + GLArea composition not smooth) |
| Spike | S1 — code in [`spikes/s1-composition/`](../../spikes/s1-composition/) |

## Context

Dioxus desktop renders the whole UI in a WebKitWebView; mpv renders the video with OpenGL. SPECIFICATION §5.2.1 proposes a `GtkOverlay` with the mpv `GtkGLArea` as the main child and the transparent webview as an overlay. Before M1, spike S1 must show that this composition handles 1080p60 without dropping frames, that input, resizing, fullscreen and HiDPI work, and that opening and closing the player does not accumulate memory. If it fails, a fallback is chosen (B/C/D of §5.2.3).

## What the spike does

- **Composition**, strategy (a): on the first main loop iteration after the initial render, the `WebKitWebView` is removed from tao's `default_vbox`, the `GtkOverlay` is created, the `GtkGLArea` is set as the main child and the webview is put back as an overlay. Transparent webview background (`with_background_color((0,0,0,0))` plus CSS).
- **mpv**: fixed options of PLY-005, `hwdec=auto-safe`, OpenGL render context created on `realize` with `get_proc_address` via direct FFI (`eglGetProcAddress` on Wayland, `glXGetProcAddressARB` on X11) and the native display for hwdec interop. The update callback only does `queue_render` on the GTK thread; the `render` signal reads `GL_DRAW_FRAMEBUFFER_BINDING`, renders at physical size (allocation × scale factor) and calls `report_swap`.
- **Events**: a separate mpv client in an `mpv-events` thread observes `time-pos`, `pause` and the counters; `time-pos` reaches the UI at most at 30 Hz, always with the latest value (ARC-004).
- **Test UI** (Dioxus): a bar with buttons and click and key counters, and a fake subtitle whose graphemes are clickable `span`s. The text changes on every `time-pos` update, i.e. 30 times per second: this is the worst case for the webview, because real subtitles change every few seconds.
- **Measurements** (`--measure`, `--stress`, `--exercise`): dropped frames (`frame-drop-count`, `vo-delayed-frame-count`, `mistimed-frame-count`), GLArea renders per second, CPU from `/proc` for the process and all its descendants (the WebKit processes), RSS, render size.

## Measurement environment

AMD Ryzen 7 7800X3D, Radeon RX 7900 XTX (Mesa 26.2.4, radeonsi, VA-API), KDE Plasma on Wayland, mpv 0.41 (libmpv API 2.5), WebKitGTK 2.54.1, GTK 3.24.52, dioxus 0.7.10 (tao 0.34.8, wry 0.53.5), libmpv2 6.0.0.

The automatic measurements run in an **off-screen virtual KWin** (`kwin_wayland --virtual`, 1920×1080 output at 60 Hz, separate D-Bus; script `run-virtual.sh`), with native Wayland or with XWayland (`GDK_BACKEND=x11`, real X11 from GTK's point of view). In the real session the window opened behind the active ones (KWin focus stealing prevention) and, being covered, did not receive frame callbacks: GTK did not lay out the overlay inserted after the first frame (1×1 allocation) and the GLArea never rendered. This is not a defect of the composition: as soon as the window is visible, layout resumes. It does, however, set a rule for the L6 tests, which must run in an off-screen compositor (TEST §10).

## Results

Values in the measurement window, after 3 s of settling. "Process CPU" is the Anikoto process (mpv included); "total" adds the WebKit processes (mainly `WebKitWebProcess`, which here redraws the fake subtitle 30 times per second). The NFR-PERF-03 threshold applies to the process CPU: ≤ 25% of one core.

### 1080p60 fullscreen playback (T-PERF-03)

| GDK backend | Video | Effective hwdec | Duration | Dropped frames | Delayed | Renders/s | Process CPU | Total CPU |
|---|---|---|---|---|---|---|---|---|
| Wayland | H.264 | vaapi | 290 s | **0** | 0 | 60.0 | **9.8%** | 19.5% |
| Wayland | HEVC 10-bit | vaapi | 290 s | **0** | 0 | 60.0 | **9.6%** | 19.6% |
| X11 (XWayland) | H.264 | vulkan-copy (`auto-safe`) | 290 s | **0** | 0 | 60.0 | **12.5%** | 22.6% |
| X11 (XWayland) | HEVC 10-bit | vulkan-copy (`auto-safe`) | 290 s | **5122** ✗ | 0 | 42.3 | 11.1% | 20.0% |
| X11 (XWayland) | HEVC 10-bit | vaapi-copy | 60 s | 0 | 0 | 60.0 | 19.3% | 29.8% |
| X11 (XWayland) | HEVC 10-bit | vulkan-copy (`auto-copy-safe`) | 60 s | 1 | 0 | 60.0 | 15.0% | 25.5% |
| X11 (XWayland) | HEVC 10-bit | no (software) | 60 s | 0 | 0 | 60.0 | 40.1% ✗ | 49.1% |
| X11 (XWayland) | HEVC 10-bit | vaapi-copy | 290 s | **847** ✗ | 0 | 57.1 | 21.1% | 32.5% |
| X11 (XWayland) | H.264 | vaapi-copy | 290 s | **0** | 0 | 60.0 | 16.9% | 27.9% |
| X11 (XWayland) | HEVC 10-bit | vaapi-copy + `opengl-pbo=yes` | 290 s | **565** ✗ | 0 | 58.1 | 19.1% | 28.9% |
| X11 (XWayland) | HEVC 10-bit | vulkan-copy + `opengl-pbo=yes` | 290 s | **0** | 0 | 60.0 | 11.6% | 20.9% |
| X11 (XWayland) | HEVC 10-bit | vulkan-copy + `opengl-pbo=yes` (repeat) | 290 s | **2021** ✗ | 0 | 53.0 | 12.6% | 22.6% |
| X11 (XWayland) | H.264 | vulkan-copy + `opengl-pbo=yes` | 290 s | **4118** ✗ | 0 | 45.8 | 8.9% | 17.6% |

**Reading the results.**

- **Wayland is stable.** Every run has 0 dropped frames and 60 renders per second: 290 s with H.264 and HEVC 10-bit, 60 s without DMA-BUF, 30 s at 4K, the resize exercise and the initial 20 s test. Process CPU stays between 9.6% and 12% at 1080p, well below 25%.
- **X11 (nested XWayland) cannot be measured reliably.** Identical runs give opposite results: H.264 with `vulkan-copy` drops 0 frames without PBO and 4118 with PBO; HEVC 10-bit with `vulkan-copy` + PBO drops 0 in the first run and 2021 in the second. CPU always stays between 9% and 21%, no delay is reported (`vo-delayed-frame-count` = 0) and the dropped frames come in bursts. The problem seems to be the XWayland presentation cadence inside the virtual KWin, not decoding or upload. No combination of `hwdec` and `opengl-pbo` fixes it repeatably. A measurement on a real X11 session (Xorg) is needed.

### Variants

| Test | Outcome |
|---|---|
| Wayland with `WEBKIT_DISABLE_DMABUF_RENDERER=1` (the environment that Dioxus sets on its own), H.264, 60 s | 0 dropped, but WebKit CPU goes from 9% to **15%** (process 11.9%, total 28.2%): the DMA-BUF renderer must stay on |
| 4K output (3840×2160) on Wayland, H.264, 30 s | 0 dropped, 60 renders/s, process CPU **23.3%** (total 32.8%): within the threshold, but with little margin |
| HiDPI 2× on X11 (`GDK_SCALE=2`), H.264, 30 s | scale factor 2, 960×540 logical window rendered at 1920×1080 physical; 1 frame dropped at startup, then 60 renders/s |
| Integrated GPU: Radeon 610M (Ryzen 7 7800X3D, RDNA2 with 2 CUs, 2022) through `DRI_PRIME`, Wayland, H.264 1080p60, 30 s | VA-API active, but **20 dropped frames** with the default profile and **9 with `profile=fast`**; **22** with the subtitle overlay frozen (`S1_STATIC_SUB=1`, WebKit at 0.4% CPU). The iGPU reaches 100% load in bursts. The cost is in the GLArea + mpv + compositing chain, not in the webview. In this setup the virtual KWin may also composite on the iGPU |
| Heavy CPU load (load average ~26 on 16 threads from unrelated `cargo test --release` builds), Wayland, H.264 1080p60, 8 s | **9 dropped frames** (0 at idle). mpv renders on the GTK main thread (`render()` p50 3.3 ms, max 17 ms of CPU time), so main-thread stalls turn into dropped frames |
| Developer's real session (KDE, 7900 XTX, 4K at 120 Hz, fractional scale 2.7 → GTK 3 renders at 3×), interactive, 70 s | about **2000 dropped frames**, but measured during the same heavy load. To repeat at idle with `--measure 60` (the report now includes the GPU actually used, the frame clock rate, render times and a per-second drop timeline) |
| HiDPI 2× on Wayland | **not verifiable in the virtual KWin**: GTK 3 always receives scale factor 1 (even with `--scale 2` or `GDK_SCALE=2`). Left to manual verification; the developer's real session reports scale factor 2 |

### Resize and fullscreen (`--exercise`)

3 rounds of: 960×540 → 1600×900 → fullscreen → windowed → 640×360 → 1280×720, one step every 1.5 s, during playback of H.264 1080p60. On both backends the render size always follows the new allocation (for example 1920×1080 in fullscreen, 640×360 in the small window) and **no frame is dropped**. The average process CPU during the test is 8.8% (Wayland) and 15.8% (X11).

### 50 player openings and closings (`--stress 50`)

Each cycle destroys the GLArea (unrealize: render context, `quit`, join of the event thread, `Mpv`) and creates a new one with a new mpv, which must render at least 20 frames of `av_basic.mkv`.

| Backend | RSS after cycle 5 | RSS after cycle 50 | Growth |
|---|---|---|---|
| Wayland | 310.8 MiB | 310.2 MiB | **−0.2%** |
| X11 (XWayland) | 310.9 MiB | 313.6 MiB | **+0.9%** |

Limit: +10%.

## Revision of 2026-10-19: 4K and CPU contention

The developer's real session (KDE Wayland, RX 7900 XTX, 4K monitor at 120 Hz with fractional scale 2.7, so GTK 3 renders at 3×) ran `--measure 60` at 10:27: **17.3 renders/s, 2456 dropped frames in 60 s** (about 43 per second, steady, not in bursts), VA-API active, every process on the 7900 XTX (`drm-pdev 0000:03:00.0`), process CPU 60%, `render()` p50 1.1 ms. The machine was under heavy load at the time: five unrelated `cargo test --release` runs at nice −4, load average 26–35 on 16 threads.

Control runs in the virtual KWin at 3840×2160, during the same load, 20 s each:

| Configuration | Renders/s | Dropped | Process CPU |
|---|---|---|---|
| **mpv alone** (`vo=gpu-next`, `hwdec=auto-safe` → vulkan) | 60 | **0** | — |
| Spike, GLArea only (`S1_NO_WEBVIEW=1`) | 53.5 | 131 | 27% |
| Spike, GLArea + transparent webview (normal) | 14.4 | **912** | 31% |
| Same, `WEBKIT_DISABLE_DMABUF_RENDERER=1` | 12.3 | 960 | 30% |
| Same, `WEBKIT_DISABLE_COMPOSITING_MODE=1` | 13.6 | 933 | 30% |
| Same, X11 with `GDK_SCALE=1` / `GDK_SCALE=3` | 17.6 / 17.6 | 850 / 852 | 25% / 25% |

**Reading.**

- **The cost is the transparent webview over a changing GLArea.** With the webview hidden, the GLArea alone renders at 53.5 per second. With it, each video frame forces GTK 3 to recompose the whole window, including the full-window webview, on the main thread; at 4K that takes about four times longer.
- **WebKit's rendering mode and the GTK scale are not the cause.** Neither WebKit switch helps, and scale 1 and scale 3 give identical results.
- **mpv alone does not drop a frame under the same load.** It renders on its own thread and lets the compositor present its surface. Our design puts video presentation on the GTK main thread, which also runs Dioxus, the webview and the window composition; any contention or extra work there costs frames.
- At idle and 1080p the design works (results above). But SPECIFICATION §1.2 asks for "the quality of mpv", 4K HiDPI monitors are common, and a laptop under load is normal. **The GtkOverlay composition is therefore not adequate as the final design.**

**Proposed next step: spike S1b (Wayland subsurface).** The video goes to a `wl_subsurface` placed *below* the window's main surface, with its own EGL surface. mpv renders on a dedicated thread (render API with its own EGL context, presentation driven by the compositor's frame callbacks), and its size follows the content area through `wp_viewporter`, at the exact physical resolution even with fractional scaling. The window background is transparent, and the webview stays on top in the main surface, receiving all input (the subsurface has an empty input region). KWin or Mutter then composite video and UI, as for any other surface. The main thread does no per-frame video work: it only repaints the webview when the UI changes.

- X11 has no subsurfaces. There it keeps the GtkOverlay composition, which holds at 1080p, or fallback B (SPECIFICATION §5.2.3). It is measured in M1 together with the X11 T-PERF-03 criterion.
- S1b uses `libwayland-client` and `libwayland-egl` (MIT) through FFI or the `wayland-client`/`wayland-sys` crates (MIT), and `gdk_wayland_window_get_wl_surface`.
- Exit criteria: the same as S1, at 4K and under the load of this revision, compared with mpv alone.

## Findings that change the M1 implementation

1. **Dioxus forces XWayland.** With the default value of `with_disable_dma_buf_on_wayland(true)`, `dioxus-desktop` 0.7.10 always sets `GDK_BACKEND=x11` (in addition to `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Wayland sessions). Native Wayland requires `with_disable_dma_buf_on_wayland(false)`. SPECIFICATION §5.2.1 updated.
2. **`LC_NUMERIC` must be "C".** `gtk_init` sets the locale from the environment (`it_IT.UTF-8`) and `mpv_create` returns NULL with the message "Non-C locale detected". `anikoto-app::linux` must call `setlocale(LC_NUMERIC, "C")` after GTK init and before creating mpv. The UI is not affected, because Rust formats the numbers. SPECIFICATION §5.2.2 updated.
3. **On X11 there is no zero-copy hwdec.** GTK 3 on X11 creates the GL context with GLX, and mpv supports VA-API interop only with EGL. `hwdec=auto-safe` therefore falls back to `vulkan-copy`: hardware decoding, but with a copy of the frame in memory. On Wayland you get zero-copy `vaapi`, as long as the native display is passed to the render context. SPECIFICATION §5.2.2 updated.
4. **libmpv2 6.0 has two fragile spots** to avoid or encapsulate in `anikoto-player`:
   - `create_client(Some(name))` passes mpv the pointer of a temporary `CString` that has already been freed, and `NonNull::new_unchecked` does not check for NULL. We use `create_client(None)`.
   - The `Drop` of `Mpv` calls `mpv_destroy`, which does not stop the core while another client exists (the event thread stays blocked in `wait_event`). The shutdown order must be: render context (with the GL context current) → `quit` command → join of the event thread → main handle. The `'a` lifetime of `RenderContext<'a>` is only a `PhantomData`, so keeping `Mpv` and `RenderContext` in the same struct requires a small documented `unsafe` (allowed in `anikoto-player`, ARC-011).
5. **wry's `WebViewExtUnix::reparent` does not help**: on a `GtkOverlay` it would call `add`, i.e. it would make the webview the main child. Reparenting is done with the GTK APIs.
6. **The Dioxus default menu** ends up in the same vbox: `with_menu(None)` is needed.

## Decision

1. **We adopt the `GtkOverlay` composition, strategy (a)** of SPECIFICATION §5.2.1. On Wayland it meets all the automatic S1 criteria with margin: dropped frames, CPU, resizing, fullscreen and memory stability. No fallback (B/C/D) is needed for Wayland. R-1 drops from "medium" to "low" probability for Wayland.
2. Configuration for M1, already reflected in SPECIFICATION §5.2:
   - `with_disable_dma_buf_on_wayland(false)`, with the WebKit DMA-BUF renderer on: with it off, WebKit CPU rises from ~9% to ~15%;
   - `with_menu(None)`;
   - `setlocale(LC_NUMERIC, "C")` after GTK init;
   - native display in the render context;
   - `hwdec=auto-safe` (zero-copy VA-API on Wayland);
   - player shutdown in the order render context → `quit` → thread join → `Mpv`.
3. **X11 remains open.** Composition, input, resizing, fullscreen, HiDPI 2× and memory stability also work on X11. The "0 dropped frames in 5 minutes" criterion, however, cannot be verified reliably with nested XWayland. The maintainer moved the measurement on a real X11 session to the M1 exit criteria (2026-10-19). If frames are dropped there too, the choice is up to the maintainer:
   - accept a documented tolerance on X11, because hwdec there always involves a copy;
   - use fallback B only on X11 (child window with `wid`, mpv with `vo=gpu` and EGL);
   - postpone X11 support until after 1.0.

## Checks still to do (real session)

The spike cannot automate them. They are done by hand with the interactive window: Space pause, ←/→ seek, F fullscreen. The top bar counts clicks and keys and shows hwdec and dropped frames.

- [ ] Wayland (KDE or GNOME): clicks on the button and on the subtitle characters, and key presses, reach the webview above the video (the counters increase).
- [ ] Wayland: manual resizing, fullscreen and back without artifacts (misaligned black borders, flicker, unscaled video).
- [ ] Wayland on a HiDPI 2× screen: sharp video and text. The developer's session reports scale factor 2.
- [ ] Real X11 session (Xorg): the same points, plus the `--measure 290` run on H.264 and HEVC 10-bit.
- [ ] On REF (integrated GPU): the `--measure 290` run on Wayland. The weakest iGPU that fits the REF definition (Radeon 610M, 2 CUs) drops frames at 1080p60 even with the overlay frozen (table above). Next steps: measure on a mid-range iGPU (Intel Iris Xe, Radeon 680M/780M) on a real session; if it also drops frames, profile the GTK 3 GLArea path in M1 (GDK compositing of the GL texture, `profile=fast`, `video-sync`). The maintainer may also want to make the REF GPU class explicit (SPECIFICATION §7).

## How to repeat the measurements

```sh
cargo xtask fixtures --perf
cargo build --release --manifest-path spikes/s1-composition/Cargo.toml
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 290
GDK_BACKEND=x11     spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_hd60_hevc10.mkv" --fullscreen --measure 290
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_basic.mkv" --stress 50
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_hd60_h264.mkv" --exercise
```
