# Spike 03: B0 (epaint) cost for 300 text nodes on Android

Date: 2026-10-10. Milestone: M0 (input to the B1 decision; ARCHITECTURE says
B1 waits for M4 unless this spike shows B0 is too slow). Code: a scratch bench
(`ljos_spike_bench`) and the spike 01 eframe app, at
`<scratch>/ljos-spike-android/` (outside the repo). `crates/ljos_render` was
used as a path dependency and **not edited**.

## Question

Can B0 (ljos_render compose, then the epaint backend, then egui tessellation)
draw 300 visible text nodes in **under 4 ms CPU per frame** on a mid-range
phone (ARCHITECTURE §8.5: CPU at most half the frame budget, about 4 ms at
120 Hz)?

## Result

**Steady state: yes, by a wide margin. First frame: borderline, needs a real
device.**

- Warm (nothing changed) and one-changed frames cost **0.15–0.23 ms p50**
  on the x86_64 emulator and 0.33–0.49 ms even under ARM binary translation,
  so 10–25x under the 4 ms target.
- The cold first frame (empty font atlas and galley cache) is **1.7–7.8 ms** on
  the x86_64 emulator, dominated by egui text layout. On a phone it may go
  over 4 ms once; that is a startup cost, not a per-frame cost.
- A full eframe frame with 300 egui labels held 60 Hz on the emulator at
  about 3.3 ms of CPU (p50), including egui-wgpu painting on a software GPU.

Nothing here suggests B1 needs to move ahead of M4.

## Method

The bench builds a `RenderTree` with 300 text-only children
(`DisplayListBuilder::text_stopgap`), all inside the viewport, and times each
stage of one egui pass:

- **compose**: `RenderTree::compose`;
- **encode**: `FrameView::new` + `EpaintBackend::sync` + `encode` into an
  `EguiSink` (text goes through egui `layout_no_wrap`, which hits egui's
  galley cache when warm);
- **tess**: `Context::tessellate` of the produced shapes.

"sum" is compose + encode + tess. Cases: **cold** (first frame after an empty
pass that applies `pixels_per_point`; that pass is reported as "font init"),
**warm** (unchanged tree, after 10 settle frames), **one** (node 0 gets a new
string every frame). Warm and one are p50/p90 over 300 frames (100 for the
translated run).

| Scene | Viewport (pt) | ppp | Font | Text | Columns | Vertices |
|---|---|---|---|---|---|---|
| A desktop | 1280x800 | 1 | 13 pt | "label number {i}" | 10 | 16,360 |
| B phone | 411x914 | 2.625 | 11 pt | "n{i:03} item" | 5 | 9,600 |
| C phone-long | 411x914 | 2.625 | 11 pt | "label number {i}" | 3 | 16,360 |

B and C match the emulator screen (1080x2400 px at 420 dpi).

## Numbers (ms)

### Host: AMD Ryzen AI MAX+ 395, 32 threads, Linux

| Scene | Font init | Cold sum (encode / tess) | Warm sum p50/p90 | One sum p50/p90 |
|---|---|---|---|---|
| A | 0.686 | 1.956 (1.811 / 0.120) | 0.070 / 0.073 | 0.083 / 0.092 |
| B | 0.127 | 1.460 (1.389 / 0.050) | 0.060 / 0.060 | 0.073 / 0.076 |
| C | 0.117 | 1.592 (1.470 / 0.105) | 0.070 / 0.074 | 0.087 / 0.091 |

### Emulator, native x86_64 (4 vCPU)

| Scene | Font init | Cold sum (encode / tess) | Warm sum p50/p90 | One sum p50/p90 |
|---|---|---|---|---|
| A | 2.142 | 7.810 (6.274 / 1.462) | 0.218 / 0.343 | 0.224 / 0.304 |
| B | 0.226 | 1.731 (1.523 / 0.187) | 0.148 / 0.209 | 0.166 / 0.223 |
| C | 0.370 | 3.190 (2.359 / 0.786) | 0.212 / 0.286 | 0.226 / 0.304 |

Warm breakdown (A): compose 0.001, encode 0.057, tess 0.157. One-changed
compose is 0.012 ms.

### Emulator, arm64 binary under the image's ARM translation (not representative)

| Scene | Font init | Cold sum | Warm sum p50/p90 | One sum p50/p90 |
|---|---|---|---|---|
| A | 50.0 | 166.4 | 0.414 / 0.510 | 0.487 / 0.650 |
| B | 37.0 | 20.1 | 0.333 / 0.357 | 0.420 / 0.469 |
| C | 1.5 | 12.2 | 0.407 / 0.426 | 0.475 / 0.668 |

Translation inflates cold paths (code is translated the first time it runs),
so these cold numbers say nothing about a phone. They only show the arm64
binary runs.

### Full eframe frame on the emulator

The spike 01 app with "300 labels": 300 egui labels in `horizontal_wrapped`,
11 pt, all visible, one label changing every frame, `request_repaint` each
frame. eframe's `frame.info().cpu_usage` covers the whole frame on the CPU
including egui-wgpu paint and submit (here to SwiftShader, a software Vulkan).

| | `ui()` p50 | eframe `cpu_usage` p50 / p90 | Frame period | Process CPU (top) |
|---|---|---|---|---|
| TalkBack off | 0.67–0.74 ms | 3.3 / 3.6 ms | 16.6 ms (60 Hz held) | 26.9 % |
| TalkBack on | about 1.17 ms | 3.5 / 3.9 ms | 16.6 ms | |

This path uses egui widgets rather than ljos_render, so it is an upper bound
on what egui itself costs for the same amount of text, plus the GPU backend.

## Interpretation

- **Warm cost is small because of caching.** B0 keeps per-node mesh/`Arc`
  caches and egui caches galleys by text and style, so an unchanged frame
  only walks the tree and copies shapes. One changed string adds one galley
  layout (about 0.01–0.02 ms).
- **Cold cost is text layout.** Encode dominates the cold frame: each of the
  300 strings is shaped and laid out once and glyphs are added to the font
  atlas. Scene A at ppp 1 rasterises at a different size than B/C, hence the
  higher font init and cold encode.
- **Tessellation scales with vertices** (16,360 vs 9,600) and is the larger
  warm stage on the emulator.

### How the numbers translate, and what needs a real device

The emulator runs x86_64 code on a fast desktop core (Ryzen AI MAX+ 395) at
roughly 2–3x the host times, because of virtualisation and only 4 vCPUs.
A mid-range arm64 phone (Cortex-A78/A76-class big cores, slower little cores)
is commonly several times slower than this desktop core single-threaded, so a
rough guess is 3–6x the host numbers: warm about 0.2–0.5 ms, cold about
5–12 ms. That keeps steady state well inside 4 ms and puts the first frame
over it. These are estimates; the emulator cannot measure them because:

- the CPU is an x86 desktop core, not a phone core with big.LITTLE scheduling;
- rendering goes to SwiftShader, so wgpu/GL submit and driver costs are not a
  phone's;
- the arm64 run is binary-translated;
- there is no thermal throttling and the emulator display is 60 Hz.

To measure on a real mid-range arm64 device in M3:

1. cold first frame and font init (system fonts, CJK fallback adds atlas work);
2. warm and one-changed B0 frames, p50/p90/p99 over thousands of frames;
3. wgpu (Vulkan and GL) encode and submit time, and whether egui's paint is
   inside or outside the 4 ms budget;
4. frame pacing with Choreographer at 90/120 Hz;
5. sustained runs (10 minutes) for thermal behaviour;
6. a `simpleperf` profile of the cold frame.

## Reproduce

```
cd <scratch>/ljos-spike-android/src
cargo run -p ljos_spike_bench --release -- 300                 # host
cargo ndk -t x86_64 -t arm64-v8a -P 26 build -p ljos_spike_bench --release
adb push <target>/x86_64-linux-android/release/ljos_spike_bench /data/local/tmp/ljos_spike_bench_x86_64
adb shell /data/local/tmp/ljos_spike_bench_x86_64 300
```

(`<target>` is `<scratch>/ljos-spike-android/target`, set in
`src/.cargo/config.toml`.) Raw output is in `bench-host.txt`,
`bench-emu-x86_64.txt` and `bench-emu-arm64-translated.txt`.

Full app frame: build and install the spike 01 APK, tick "300 labels
(spike 3)" and read the `SPIKE-FRAME` lines in `adb logcat -s SPIKE-EGUI`
(logged every 120 frames).

## Recommendations for M3

1. Keep B0 for Android; B1 stays in M4.
2. Hide the cold cost: warm the font atlas and galley cache for the first
   screen before the first visible frame (or during the splash screen), and
   keep the galley cache across frames.
3. Add a real-device benchmark (this bench, ported to an instrumented APK or
   `adb shell` binary) to the M3 checklist with the six measurements above,
   and record a budget for the first frame separately from steady state.
4. Measure egui-wgpu paint and submit on device; that, not B0, is the likely
   largest CPU item in a frame.
