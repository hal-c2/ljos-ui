# Benchmarks: Ljós and Qt side by side

`mise run bench` measures the same QML applications on Ljós (`ljos run`, release build) and on Qt
(`qml6`). Each application runs inside a limit on memory and CPU. Every number is read from outside
the application:

- **Memory and CPU:** the cgroup's own accounting and `/proc`.
- **First frame:** when the compositor maps the window.
- **Frames:** counted with the compositor's screencopy.

There are no floors. The bench reports; it does not pass or fail an application.

## Running it

```sh
mise run bench                          # every app, both runtimes, both profiles
mise run bench rectangles samegame      # some apps
mise run bench calqlatr --runtime ljos  # options after the ids go to `ljos bench`
mise run bench:cross                    # can ljos be built for a small aarch64 board?
```

`ljos bench --help` lists every option.

**Which apps.** These are run:

- the synthetic apps in `bench/*.qml`;
- the corpus examples that rendered in the last `mise run gallery` (all runnable ones if there was
  no gallery run).

Examples get the same project directory, `qmldir` overlays and window size as in the gallery.

**Defaults.** The defaults are `--warm-up --repeat 3 --bisect pi-zero`:

- **`--warm-up`** starts each app once, unmeasured, so neither runtime is charged for bringing
  shared libraries into the page cache.
- **`--repeat 3`** measures each cell three times. The tables show the median. Every run's own
  numbers are kept in `report.json`.
- **`--bisect pi-zero`** searches for the least memory each app starts in (see below).

To replace the defaults, set `BENCH_DEFAULTS`, or set it empty for none.

**Output.** Reports go to `target/bench/`, or `$BENCH_DIR` if set:

- `report.json`: every run;
- `report.md`: the tables;
- `work/`: each app's output.

**Long runs.** A full run takes hours. Make it in parts with `--append`, which adds to the
`report.json` already there, for example one app and one runtime at a time:

```sh
for id in rectangles form listview; do
  for rt in ljos qt; do mise run bench "$id" --runtime "$rt" --append; done
done
```

**Where it runs.** The task builds the release `ljos` first. Nothing compiles while it measures.
Apps draw on the headless sway of `scripts/cua-sandbox.sh` as Wayland clients, never on the user's
display:

- Ljós draws with egui's GL painter on EGL (llvmpipe on a headless compositor); `LJOS_PAINTER=wgpu` measures wgpu's GL backend instead.
- Qt uses its Wayland platform with `QT_QUICK_CONTROLS_STYLE=Basic`, the style Ljós draws.

## Profiles

| Profile | Memory | Swap | CPU |
|---|---|---|---|
| `desktop` | unlimited | as the machine | unlimited |
| `pi-zero` | `--memory-max` (256M) | none | `--cpus` (4) CPUs on distinct physical cores, `CPUQuota` = cpus × 100 / `--cpu-factor` (20), counted every 10 ms |

`pi-zero` models a Raspberry Pi Zero 2 W: four Cortex-A53 cores at 1 GHz and 512 MB of RAM, about
half of which is left for an application once the OS and compositor run.

**The CPU factor.** 20 says that one A53 core at 1 GHz does about a twentieth of the work of one
core of a current desktop, single-threaded. It is a parameter, not a measurement. Give the measured
ratio for your machine with `--cpu-factor`.

**The quota period.** The quota is counted over 10 ms rather than systemd's default 100 ms. A
throttled app then stalls for a few milliseconds at a time, as a slow core would, not for most of a
tenth of a second.

**The gate.** Each app starts in a transient systemd scope (`systemd-run --user --scope`) behind a
gate. The bench reads the scope's cgroup and lets the app run only if every limit is in place. If a
limit is missing (for example, when the user manager may not delegate the `memory` or `cpu`
controller), the bench stops with status 2 and says which limit. It never reports numbers measured
outside a limit.

**Out of memory.** The kernel's OOM kill is read from the scope's `memory.events` and shown as
`(OOM)` in the tables.

## What is measured

| Measure | How |
|---|---|
| Time to first frame | from starting the app to sway mapping its window |
| CPU time to first frame | the cgroup's `cpu.stat` usage at that point |
| Peak memory while starting | `memory.peak` by the first frame |
| Settled memory | `memory.current` after `--settle` seconds (3) |
| Settled private memory | PSS of anonymous pages (`smaps_rollup`) |
| Settled PSS | PSS of all pages |
| Idle CPU, idle wakeups | CPU use and context switches over `--idle` seconds (10) with no input |
| Frames while idle | screencopy frames over the idle window: non-zero only for apps that animate |
| CPU and frames while driven | over `--load` seconds (10) of `--gesture` input (wlr virtual pointer): `click` (default) clicks a grid over the window, `drag` holds the button in its middle and moves up and down a third of its height once a second, `scroll` turns the wheel ten notches down and ten up |
| Frame interval while driven | the median and 95th percentile time between the frames the compositor showed then |
| Mapped install | the files the app maps: runtime, libraries, plugins, fonts |
| Least memory | the smallest `MemoryMax`, found by bisection, at which the first frame is shown and the app is still running a second later |

**Shared pages.** cgroup memory counts the pages a cgroup brought in first. Shared libraries that
another process already loaded are not charged to the app. That is the reason for the warm-up, and
it is why "least memory" can be lower than "peak memory" for the same app. Treat least memory as
"what this app needs on top of a system that already runs its runtime's libraries". On a small
board with one app, add the libraries (the mapped-install row gives their size on disk).

## Noise

Timings depend on the machine's load. Every run records the one-minute load average, and
`report.md` gives the range. Compare runs made under similar load. Memory is much less sensitive,
but `memory.current` also counts page cache, so a cell can vary between repeats when other work
evicts or brings in files. Look at the repeats in `report.json` before trusting a single outlier.

## Profiling Ljós's heap

A `dhat-heap` build records every Rust heap allocation with its call stack. It is slow and never
shipped.

```sh
cargo build --release -p ljos_cli --features dhat-heap
LJOS_DHAT_FILE=heap.json LJOS_DHAT_SECONDS=8 target/release/ljos run app.qml
scripts/dhat-subsystems.py heap.json 25   # live bytes by crate and by call site
```

The profile is written `LJOS_DHAT_SECONDS` (default 5) seconds after the start. At that point it
holds what was live then and the peak until then. Run the app under the bench's limits with
`systemd-run --user --scope -p MemoryMax=…` if the limit matters. dhat adds overhead, so give it
more memory than the app needs on its own. `scripts/dhat-subsystems.py` sums live bytes by the
first frame outside the allocator and the standard library. It gives the share of each subsystem
(reactive graph, type registration, image store, …).

dhat does not see memory outside Rust's allocator: the GPU stack (Mesa, llvmpipe), fonts that
fontconfig loads, and glibc's arenas. Compare its total with the cgroup's numbers to see that part.

## Cross-compiling for a small board

`mise run bench:cross` checks whether `ljos` builds for `aarch64-unknown-linux-gnu`
(`BENCH_TRIPLE` overrides the triple). It installs nothing. It names each missing piece and fails
if any is missing:

- Rust's standard library for the triple;
- a linker (`CARGO_TARGET_<TRIPLE>_LINKER`; `zig cc -target aarch64-linux-gnu` works with a
  sysroot);
- a sysroot of the board with the development files of the C libraries the binary links (alsa-lib
  and fontconfig), given with `PKG_CONFIG_SYSROOT_DIR` and `PKG_CONFIG_PATH`.

When all are present, it builds and prints the stripped size. It always prints the host's release
size for comparison.

The bench measures `ljos run` on a QML file, which parses and compiles the QML at startup.
Shipped apps load precompiled IR with no parser in the binary (ARCHITECTURE.md). There is no player
binary for that yet, only the `App` API, so the numbers include the front end's cost.

## Where the first frame's time goes

Measured on `bench/rectangles.qml` (release build, the sandbox's headless sway on pixman), wall
time from the start of the process:

| Phase | Wall |
|---|---:|
| QML parsed and compiled | ~5 ms |
| System fonts (fontconfig) | 20–70 ms |
| Application built (engine, root object) | ~20 ms |
| Graphics instance: Vulkan, then GL, in one wgpu instance | 300–700 ms |
| First paint (llvmpipe) | 30–100 ms |

The instance dominates. On pixman, Vulkan cannot present (its surface is lost), so wgpu draws with
GL on llvmpipe; the Vulkan driver was loaded for nothing. EGL alone takes 100–265 ms.

The shell starts the instance on a thread of its own as soon as the display is open, and the first
window waits only for what is left of it. With the GL painter (the default where there is EGL) that
thread starts only EGL's display, and no wgpu instance is made. One instance per backend, each on its own thread, would
wait for the slowest rather than for both, but in the sandbox the drivers started side by side
crashed (SIGSEGV) or hung the process. What is left is the drivers' own start. Qt Quick asks for
OpenGL only on Linux by default.

On the machine measured, most of the instance's time was the Vulkan loader loading the NVIDIA
driver, which was installed with no NVIDIA GPU in the machine. With `VK_DRIVER_FILES` naming only
the Radeon driver, Ljós showed its first frame in 85–115 ms (Qt: 110–205 ms) instead of about
550 ms. Pass it with `--app-env` to tell the machine's drivers apart from Ljós.

## An example run (2026-10-15)

These numbers are from one run on one machine. They are an example of what the report shows, not a
target.

**Setup**

- **Machine:** AMD Ryzen AI Max+ 395 (32 CPUs, 63 GiB), Linux 7.2.
- **Builds:** Ljós at 419a04c (`--release`, thin LTO); the `animations` app was measured again at
  ecda6d6, after its fix. Qt 6.11.2.
- **Apps:** 25: the five `bench/*.qml` apps and the 20 corpus examples that rendered.
- **Load:** the one-minute load average was 3.5 to 143. Other work shared the machine, so the
  timings are noisy.

**Results**

Each figure is the geometric mean of Ljós/Qt over the apps (below 1 is Ljós smaller or faster).

| | desktop | pi-zero |
|---|---:|---:|
| Time to first frame | ×2.05 | ×1.24 |
| CPU time to first frame | ×1.27 | ×1.16 |
| Peak memory while starting | ×0.67 | ×0.61 |
| Settled memory | ×0.65 | ×0.61 |
| Settled PSS | ×0.87 | ×0.86 |

- **Least memory under pi-zero.**
  - Ljós: 54–58 MiB for the small examples, 68–101 MiB for the larger ones.
  - Qt: 92–123 MiB.
  - One exception: rssnews needed 271 MiB on Ljós and 135 MiB on Qt. With 256M, both were
    OOM-killed. Ljós decodes the feed's pictures at full size.
- **Idle CPU.** 0 on both runtimes for apps that do not animate.
- **Animating.** Moving 200 rectangles (`bench/animations.qml`):
  - desktop: Ljós used 91 % of a core, Qt 23 %;
  - pi-zero: Ljós was held at its 20 % quota and showed 22 frames a second. Qt showed 62 on 9 %.
- **CPU while clicked.**
  - For apps where clicks change nothing on screen, Ljós used 5–37 % of one core. Qt stayed near
    0.1 %.
  - The reason was that Ljós drew and presented a frame for every pointer event. It no longer
    does (see below).

### After drawing only what changed (2026-10-16)

`bench/animations.qml` was profiled in the sandbox (llvmpipe through wgpu's GL backend, as the
bench runs it). Building the frame took 0.1–0.5 ms, tessellating 0.02 ms and submitting 1.1 ms. The
rest was llvmpipe's raster threads, at about 80 % of a core. Their cost hardly depended on what
moved: a white window with one moving 12×3 rectangle cost as much as 200. Each full-window quad cost
about 22 % of a core at 60 frames a second, and the window had three: the clear, the shell's
backdrop and the root `Rectangle`.

Three changes followed:

- A pass that draws what is already shown is not presented.
- The backdrop is not painted over a clear of the same colour.
- An opaque root rectangle covering the window becomes the clear colour.

A single run of each, Ljós before → after, with Qt from the same session (load average 37–68):

| App | Idle CPU, desktop | Frames, desktop | CPU clicked, desktop | Frames, pi-zero |
|---|---|---|---|---|
| animations | 86.7 → 43.9 (Qt 21.8) | 61.9 → 59.9 (Qt 62.1) | 87.5 → 44.9 (Qt 23.1) | 20.7 → 45.5 (Qt 61.8) |
| listview | 72.3 → 44.0 (Qt 23.2) | 62.2 → 59.8 (Qt 62.0) | 72.4 → 49.2 (Qt 24.1) | 16.8 → 31.2 (Qt 62.1) |
| clocks | 92.0 → 50.7 (Qt 21.2) | 56.0 → 60.6 (Qt 60.2) | 96.5 → 53.2 (Qt 20.7) | 18.0 → 27.4 (Qt 60.2) |
| maroon | 61.5 → 62.5 (Qt 58.3) | 62.2 → 62.0 (Qt 62.2) | 68.7 → 73.9 (Qt 69.0) | 30.8 → 35.4 (Qt 29.1) |
| samegame | 107 → 58.4 (Qt 28.2) | 53.6 → 45.2 (Qt 61.7) | 161 → 144 (Qt 145) | 12.4 → 18.5 (Qt 59.2) |
| rectangles | 0 → 0 (Qt 0) | 0 → 0 (Qt 0) | 37.1 → 0.98 (Qt 0.13) | — |
| calqlatr | 0 → 0 (Qt 0) | 0 → 0 (Qt 0) | 9.83 → 1.21 (Qt 1.00) | — |

"Frames" counts the frames the compositor was given. Before the change that included frames
showing the same picture again; after it, only new pictures are counted.

What is left in an animating frame is mostly fixed cost in wgpu's GL backend, about 27 % of a core
at 60 frames a second against Qt's 7 %. Each frame it:

- clears the frame;
- copies it, flipped, from an offscreen buffer to the window;
- swaps the buffers;
- makes the context current twice and waits on fences.

None of that can be changed from Ljós, so on EGL the shell now draws with a GL painter of its own
(next section).

### A GL painter that draws only what changed (2026-10-16)

Where there is EGL the shell now paints with egui_glow on one EGL context, straight into each
window's surface, with swap interval 1. Three things make a frame cheaper on a software rasteriser:

- **No offscreen copy.** The frame is drawn into the window's own buffer.
- **Solid rectangles are clears.** An opaque square-cornered rectangle on whole pixels is filled
  with a scissored clear rather than a blended mesh.
- **Only what changed is drawn.** Each changed shape's place, where it lay and where it lies (with
  the pixel its smoothed edge reaches), is an area of its own. The painter adds the areas changed
  since the buffer it draws into was last shown (EGL's buffer age), clears and draws only those,
  and tells the compositor the same. In `bench/animations.qml`, 200 bars move 11 500 of the
  window's 1.92 million pixels a frame. Small areas' backdrop is drawn as one mesh, since a
  software rasteriser draws each scissored clear on its own; areas of 64×64 pixels or more are
  cleared.

Two more things changed what the bench counts:

- **Frames are not lost to an early pass.** On the sandbox's 62 Hz compositor the frame callback
  came a little before the animation's next step was due; the pass found nothing to do and the frame
  was lost (593 frames in 10 s, gaps of 32 ms, against Qt's 619). A pass less than half a frame
  early now runs the scene: 619 frames, the slowest 16.8 ms.
- **Animations step at the display's rate,** which the shell now tells the scene.

Each cell is the median of three runs, Ljós before → after, with Qt from the after session. "Before"
is the first run of this page (load average 37–66); "after" ran with nothing else on the machine but
an unrelated test suite and the bench itself (load average 2–9, a few runs up to 22). CPU is % of
one core.

| App | Idle CPU, desktop | CPU clicked, desktop | Frames, desktop | Frames, pi-zero |
|---|---|---|---|---|
| animations | 86.7 → 19.5 (Qt 25.3) | 87.5 → 20.5 (Qt 25.1) | 61.9 → 61.8 (Qt 62.0) | 20.7 → 59.7 (Qt 61.8) |
| listview | 72.3 → 22.9 (Qt 25.3) | 72.4 → 23.8 (Qt 25.2) | 62.2 → 62.2 (Qt 62.0) | 16.8 → 56.2 (Qt 62.1) |
| clocks | 92.0 → 11.4 (Qt 22.5) | 96.5 → 11.4 (Qt 22.1) | 56.0 → 62.1 (Qt 61.9) | 18.0 → 61.7 (Qt 60.9) |
| maroon | 61.5 → 47.2 (Qt 64.0) | 68.7 → 57.7 (Qt 70.3) | 62.2 → 62.2 (Qt 62.2) | 30.8 → 40.5 (Qt 28.0) |
| samegame | 107 → 13.1 (Qt 28.1) | 161 → 105 (Qt 272) | 53.6 → 47.0 (Qt 62.2) | 12.4 → 42.1 (Qt 57.8) |
| rectangles | 0 → 0 (Qt 0) | 37.1 → 0.41 (Qt 0.11) | 0 → 0 (Qt 0) | — |
| calqlatr | 0 → 0 (Qt 0) | 9.83 → 0.67 (Qt 0.94) | 0 → 0 (Qt 0) | — |

Ljós also showed its first frame sooner than Qt in every cell (desktop: 143–243 ms against
254–314 ms).

samegame's frames are not lost: its menu waits in a `PauseAnimation`, during which nothing changes
and Ljós draws nothing (its slowest gap is that pause, 494 ms), where Qt keeps drawing.

**On a real GPU.** `LJOS_CUA_GPU=/dev/dri/renderD128` puts the sandbox's compositor, and so the
apps, on the machine's Radeon 8060S (radeonsi). Desktop profile, load average 2–8:

| App | Idle CPU: GL / wgpu / Qt | CPU clicked: GL / wgpu / Qt | First frame, ms: GL / wgpu / Qt |
|---|---|---|---|
| animations | 6.8 / 4.3 / 3.8 | 8.1 / 5.4 / 4.0 | 45 / 607 / 72 |
| listview | 14.1 / 10.4 / 4.7 | 12.7 / 11.4 / 4.4 | 50 / 633 / 87 |
| clocks | 2.9 / 3.4 / 1.8 | 2.9 / 3.0 / 2.0 | 61 / 636 / 95 |
| maroon | 4.3 / 4.7 / 2.2 | 4.9 / 4.5 / 2.7 | 70 / 659 / 113 |
| samegame | 6.6 / 7.0 / 2.3 | 50.4 / 53.0 / 6.0 | 65 / 617 / 114 |

The bench put wgpu ahead on animations and listview. The two painters were then run in turn, three
times each, for 10 s: animations 7.3–7.8 % with GL and 7.2–8.0 % with wgpu, listview 16.0–18.7 %
and 16.1–17.3 %. On a GPU they cost the same. GL stays the default: it costs the same on a GPU and a
third on a software rasteriser, and it shows the first frame in a tenth of the time, since wgpu
starts Vulkan first (on this machine that includes loading the NVIDIA driver; see "Where the first
frame's time goes").

**What is left.**

- **On a GPU Qt uses half to a third of Ljós's CPU.** With the pixels free, what is left is the
  scene's own work each frame. In listview that is making and dropping delegates as they scroll by
  (the reactive graph, the incubator, text), spread thin with no single hot spot; the painter is
  under 4 % of the main thread.
- **Clicks in samegame** cost 105 % of a core on llvmpipe and 50 % on the GPU (Qt: 272 % and 6 %).
- **pi-zero frames.** At a fifth of a core Ljós shows fewer frames than Qt in listview (56 against
  62), animations (60 against 62) and samegame's game. It shows more in maroon (40 against 28).
- **A click that changes nothing** costs Ljós 0.4 % in rectangles, Qt 0.1 %.

### rssnews: pictures after the first frame (2026-10-16)

rssnews was measured against a local mirror of its feed and pictures, so the network was not a
variable. The mirror was a Python HTTP server that logged when each request came in. The manifest
pointed the app's feed at the mirror, and `WAYLAND_DEBUG` timed when the window was made and its
first buffer committed. Before this change, the times in ms from the start of the process were:

| | Ljós | Qt |
|---|---:|---:|
| Feed fetched | 71 | 64 |
| Window (toplevel) made | 71 | 239 |
| First picture asked for | 247 | 325 |
| First buffer committed | 278 | 312 |
| Last picture received | 363 | 343 |

Neither runtime's first frame waited on the network. Ljós made the delegates in its first pass, and
their fetches and decodes started then. That cost about 80 ms of CPU before the first frame and
about 28 MiB of peak memory. Qt asks for pictures after it has drawn.

Three changes followed:

- **Pictures start after the frame is drawn.** The desktop shell holds the image store's
  background loads until the pass that asked for them has been drawn. Pictures then fill in after
  the window is up, as in Qt.
- **Fetched bodies are bounded.** The decode gate counts a fetched body from the moment it arrives
  until it is decoded. A new fetch starts only while the gate has room.
- **PNGs shrink as they decode.** A PNG kept smaller than its size is made smaller row by row as it
  is decoded, and is never whole in memory.

Medians of 3 runs, before → after. The load average was 6–39 during the runs.

| | desktop | pi-zero |
|---|---|---|
| rssnews mirrored, first frame (ms) | 225 → 207 (Qt 278–288) | 1207 → 1075 (Qt 1169–1309, OOM) |
| rssnews mirrored, CPU to first frame (ms) | 256 → 184 (Qt 228) | 242 → 215 (Qt 225–253) |
| rssnews mirrored, peak memory (MiB) | 107 → 82 (Qt 113) | 110 → 70 (Qt 100–108, OOM) |
| rssnews mirrored, settled memory (MiB) | 120 → 118 (Qt 262) | 120 → 113 |
| rssnews live, first frame (ms) | 211 → 234 (Qt 286–299) | 921 → 1093 (Qt 1257–1572, OOM) |
| rssnews live, peak memory (MiB) | 78.5 → 78.2 (Qt 115) | 68.8 → 68.9 (Qt 111, OOM) |

The live feed changes from run to run, and the live first frames moved with the load (Qt's moved
with it). The maroon and samegame controls changed within noise.

**What is left.** In 6 s Ljós asked for 21–24 of the feed's pictures and Qt for 7. Ljós makes
delegates for more of the list than Qt does, and loads a picture for each of them.

### The scene's own work each frame (2026-10-17)

With the pixels free on a GPU, what was left was the engine's work each frame: the reactive graph,
delegates, text and the scene. These changes cut it:

- **A rerun that reads what the last run read keeps its edges.** The graph compares the new reads
  with the old ones in place, instead of dropping every edge and making it again.
- **An item that only moves reads only where it is.** Its position (x, y) is an effect of its
  own. An item animated along x used to read all thirteen of its geometry properties every frame.
  Now a frame that only moves it sets only its transform.
- **Per-frame maps hash with FxHash, not SipHash.** This covers the render backend, the scene's
  sets of drawn items, a view's per-row maps, the accessibility pass, the engine's maps, and the
  text layout and shape caches.
- **A read that finds its object builds no error.** The error, with its formatted name, is built
  only when the object is gone.
- **A particle system steps only the emitters that give birth,** and an `ImageParticle` paints all
  its particles into its own display list, as one mesh per image. It no longer makes an item for
  each particle with its own bindings.
- **An item's anchor lines (`left`, `top`, …) are made only when read.** Every item used to
  write its seven lines as it was made, each a newly allocated list, whether or not anything
  anchored to it.

`crates/ljos_shell/examples/frame_profile.rs` drives a project at 60 Hz on a virtual clock without a
window, so a frame can be split by function under callgrind. A samegame click went from 156 G to
2.6 G instructions.

The tables below are medians of 3 runs. The GPU runs had a load average of 6–58. The llvmpipe
runs had 3.6–199: a few runs fell in other workers' builds, so samegame and rssnews are the
noisiest cells. CPU is % of one core. "Before" is the GPU table above.

| App | Idle CPU, GPU | CPU clicked, GPU | Idle CPU, llvmpipe | CPU clicked, llvmpipe |
|---|---|---|---|---|
| animations | 6.8 → 6.2 (Qt 2.7) | 8.1 → 5.4 (Qt 3.5) | 20.4 (Qt 25.9) | 19.0 (Qt 26.2) |
| listview | 14.1 → 14.8 (Qt 5.3) | 12.7 → 15.9 (Qt 5.5) | 23.7 (Qt 24.0) | 24.1 (Qt 25.9) |
| clocks | 2.9 → 4.1 (Qt 1.9) | 2.9 → 4.0 (Qt 2.5) | 10.2 (Qt 23.4) | 9.9 (Qt 23.7) |
| maroon | 4.3 → 3.2 (Qt 3.5) | 4.9 → 4.8 (Qt 3.9) | 50.3 (Qt 69.9) | 58.8 (Qt 74.1) |
| samegame | 6.6 → 2.3 (Qt 2.4) | 50.4 → 14.1 (Qt 4.4) | 10.1 (Qt 26.0) | 121 (Qt 224) |
| rssnews | 3.7 (Qt 0) | 2.7 (Qt 1.0) | 2.6 (Qt 0) | 107 → 4.2 (Qt 9.1) |

The first frame came sooner than Qt's in every desktop cell: 45–85 ms against 77–140 ms on the
GPU, and 210–267 ms against 264–326 ms on llvmpipe. In the pi-zero profile on llvmpipe, Ljós
shows 61.7 frames a second in animations (it was 60; Qt 61.9) and 60.1 in listview (it was 56;
Qt 62.0). samegame's lower count is its menu's `PauseAnimation`, as above.

**How many delegates a ListView keeps.** qml6 was run as a black box (`QT_QPA_PLATFORM=offscreen`)
next to Ljós on the same list: 100 000 rows of 20 px in a 300 px view with `cacheBuffer: 320`.

- **At start and while scrolling,** both keep the same delegates: 32 at the top, 33 while
  scrolling a little at a time.
- **After a jump to the middle,** Qt makes none of the buffer before the first visible row. It
  fills the buffer before the visible rows only as it scrolls back over them, and fills the
  buffer after them at once. Ljós fills both sides: 49 live against Qt's 33, and 16 more made per
  jump.

The Qt documentation does not promise either behaviour, so it was not changed. A list that only
starts and scrolls makes no more delegates than Qt's. The pictures rssnews asks for beyond Qt's 7
are not explained by the buffer.

### Per-frame work on a GPU, and rssnews's delegates (2026-10-17)

Instructions are callgrind's, over the whole process.

| | Before | After |
|---|---:|---:|
| listview.qml, 330 frames: `QuickScene::frame` | 1.69 G | 1.20 G |
| listview.qml, 330 frames: `pass::run` | 573 M | 224 M |
| listview.qml, 300 frames: `ensure_node` | 153 M | 56 M |
| animations on a GPU, per frame | 4.99 M | 2.03 M |
| Effect runs for 10 ListView rows of Text | 180 | 170 |
| 1000 rows of 23 px in a 300 px view: delegates made | 66 (Qt 27) | 27 |

- **listview.** A row the incubator makes is laid out once when the frame's creation jobs stop,
  not twice. A property that is read and never changed has no graph node until something writes
  it.
- **animations.** The frame's redraw areas are merged in one sweep (4.99 M → 3.90 M). Small solid
  rectangles join the mesh, where before each was a scissored clear and a draw of its own
  (→ 2.07 M). The frame's lists start at the size of the last frame's (→ 2.03 M).
- **Delegates.** An effect that a check found dirty ran a second time on its first write. The
  anchors of every Text in a delegate ran twice.
- **A new list of text rows.** On its first pass, nothing had been measured, so every row was
  taken as 1 px long. It now waits for the first row's length. In rssnews, the first pass found all
  40 rows unmeasured, so it laid all of them; it now makes 9 (Qt 8).

rssnews in the sandbox still asked for 24 pictures, where it asked for 26 before. The sandbox's
cua-driver listens over AT-SPI, and while a screen reader listens Ljós keeps 20 rows either side
of the view (ARCHITECTURE.md §14). Qt does not. The count without a listener was not measured in
the sandbox.

**What is left.**

- **listview on a GPU** costs about three times Qt's CPU. Nearly all of it is making and dropping
  the 8 delegates a frame that scroll past. Each costs about 680 k instructions: making it 185 k,
  adding it to the scene 125 k, dropping it 61 k, text layout 57 k and anchors 51 k. About 8 % is
  graph nodes made by a delegate's first tracked reads, about 46 per item, and another 9 % is
  freeing them. Sharing one node for all of an object's unread properties loses which property a
  binding depends on: bindings rerun when nothing they read changed, and the counting tests fail.
- **animations and clocks on a GPU** still cost Ljós 2× Qt's CPU.
- **samegame clicks on a GPU** cost 14 % against Qt's 4 %.
- **rssnews keeps working after it settles.** It uses 3–4 % with no frames and no wakeups, while
  pictures are still fetched and decoded. Qt has finished by then.
