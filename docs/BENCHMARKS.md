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
| Idle CPU of threads that ended | the part of idle CPU no thread alive at the end of the window used: the cgroup's usage less the live threads' `schedstat`. Wakeups are read from live threads, so a short worker shows here and not as wakeups |
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
- **rssnews keeps working after it settles.** It used 3–4 % with no frames and no wakeups. That
  was the tail of its picture loads, not work after them: see "Memory against Qt" below.

### Memory against Qt (2026-10-18)

The desktop profile on llvmpipe in the default sandbox, `--warm-up --repeat 3`, medians in MiB.
"Anon PSS" is the private memory the app made; "PSS" adds its share of mapped files, the binary and
libraries included. Qt is the 2026-10-17 run (load 18–47). Ljós before (88f0762) and after were run
back to back under load 66–148 and 16–143. Other workers' builds never let the machine get quiet,
so the CPU columns are noisy; anon PSS hardly moves with load.

| App | Peak, before → after (Qt) | Settled | Anon PSS | PSS |
|---|---:|---:|---:|---:|
| animations | 87.4 → 71.9 (103) | 85.2 → 69.2 (106) | 61.3 → 57.4 (92.7) | 188 → 130 (144) |
| listview | 86.6 → 64.2 (110) | 89.2 → 67.1 (106) | 61.0 → 58.0 (87.3) | 185 → 123 (142) |
| calqlatr | 81.9 → 75.6 (112) | 81.7 → 69.3 (113) | 64.6 → 60.3 (90.9) | 192 → 123 (159) |
| clocks | 86.9 → 76.6 (107) | 82.5 → 69.8 (110) | 61.7 → 58.5 (98.2) | 189 → 122 (151) |
| maroon | 91.6 → 80.8 (115) | 94.2 → 81.5 (120) | 77.1 → 72.2 (108) | 149 → 134 (157) |
| rssnews | 126 → 84.5 (116) | 167 → 116 (261) | 101 → 91.2 (245) | 179 → 158 (307) |
| samegame | 90.4 → 71.1 (111) | 88.0 → 72.9 (116) | 65.6 → 61.3 (104) | 144 → 118 (166) |

Peak, settled and PSS count file pages, and those come and go with the page cache. In the quieter
2026-10-17 run, Ljós's peak was 112 and 140 MiB for listview and calqlatr, above Qt, and its PSS of
files was up to 70 MiB against Qt's 47. Both were page cache: the 97 MiB static binary has 36–43 MiB
resident where Qt's libraries have 25–33 MiB. In the run before → after above, the PSS of the before
binary was high in four apps for the same reason. Anon PSS is the column the changes moved.

**What held it.** A dhat profile of clocks had 15.6 MiB of Rust heap live. The largest items were
the reactive graph's metadata (2.0 MiB), egui's text atlas (2.0 MiB), the graph (1.2 MiB) and the
type oracle read for `--example` (1.0 MiB). In rssnews, it had 39 MiB, of which 17 MiB were pictures
of rows that are made but not painted, kept at the window's size. `smaps` showed a 35 MiB `[heap]`
and 13 MiB of 1–8 MiB anonymous mappings. Most of both is outside Rust: llvmpipe's JIT, Mesa and
fontconfig.

- **egui's text atlas** is as wide as the texture limit egui is told. llvmpipe's limit is 16384, so
  the atlas was 2 MiB on the CPU and again as a texture, for the little text egui itself draws.
  egui is now told 2048. Ljós's own textures (pictures, shapes, its glyph atlas) keep the GPU's
  limit (`ljos_render::b0::set_max_texture_side`). About 3.5 MiB.
- **Freed memory inside glibc's heaps.** glibc gives back only what is free at a heap's top.
  Start-up and a burst of picture decodes leave holes below live memory. The shell calls
  `malloc_trim` a second after the first frame, a second after a burst of background picture loads,
  under memory pressure and when a window hides. It is never periodic. About 1.7 MiB in clocks
  and rssnews.

Both are counted by tests: `egui_is_told_a_small_limit_and_pictures_keep_the_gpus` (b0_image) and
`memory_freed_below_live_memory_goes_back_to_the_system` (ljos_shell).

**rssnews after it settles.** The bench's idle window starts 3 s after the first frame. rssnews's
feed names pictures up to 7728 × 5152, and they arrive over the network for up to 4 s after
start. Each is fetched and decoded on a short-lived `ljos-image-fetch` thread, scaled down while it
is decoded, and that thread ends. Wakeups are read from the threads alive at the end of the window,
which is why there was CPU and no wakeups. The new "threads that ended" row puts all of rssnews's
idle CPU there: 1.1 % of 1.1 %. Run in the sandbox and sampled each second, rssnews uses no CPU at all
from 4–5 s after start, on llvmpipe and on a GPU alike, and with or without a screen reader. Nothing
was left running to stop. It asks for 9 pictures with no screen reader and 14 with one, against Qt's
7, because rows kept for a screen reader load theirs (ARCHITECTURE.md §14).

**What is left.**

- **Decoded pictures stay on the CPU after upload.** Effects, tinting, Canvas, particles, the R
  backend, re-shrinking and video all read `ImageHandle`'s pixels, so they cannot simply go.
  Dropping them needs a way to read pixels back or decode again when one of those asks. (Done in
  "Pictures kept at the size they are drawn" below.)
- **Pictures of unpainted rows** stay at the window's size (896 px for 800 × 600). Only painted
  items are shrunk to what they draw. Shrinking the others to their item's size would save most of
  rssnews's 17 MiB. Dropping them would cost a network fetch when they scroll in. (Done in
  "Pictures kept at the size they are drawn" below.)
- **Memory outside Rust**, chiefly llvmpipe and Mesa, is about half of anon PSS.
- **The binary's resident text** (36–43 MiB of PSS) is above Qt's shared libraries. On a system that
  runs several Ljós apps, it is shared like theirs.

### The script and binding side (2026-10-18)

`frame_profile` now runs Timers and JS `Date` on its virtual clock, so a profiled clock ticks and
a Timer fires. It also says how many frames asked for the next one at once. The main thread was
profiled with callgrind over the egui passes only (`--toggle-collect=*run_ui_dyn*`). Each figure
is the difference between a 1200-frame run and a 600-frame run, divided by 600, so start-up and
the first script are left out.

| Per frame | Before | After |
|---|---:|---:|
| animations: the pass | 1 224 k | 1 152 k |
| animations: `QuickScene::frame` | 900 k | 829 k |
| animations: 200 animation steps (`animation::tick`) | 675 k | 640 k |
| animations: the steps' 200 writes (`write_impl`) | 298 k | 262 k |
| clocks: the pass | 221 k | 220 k |

- **A write** first checked for an alias, a redirect and a value still to be made, looking the
  object up again for each. It now looks once. It borrows the property's type instead of cloning
  it, and refreshes the field cache from `Graph::settled`, which records no read. A read makes a
  value that is still to be made in its own lookup. An animation step's write costs about 12 % less.
- **Geometry** no longer sorts what moved each frame.

samegame was left out of the instruction table. Its boards are random, so two runs remove
different blocks. Under callgrind its click handler also ran past the script budget and was cut
short; `frame_profile` now lifts the budget.

**CPU, as % of one core.** Desktop profile, medians of 3. "Before" is ec5b08a, run again
straight after "after" so that both met the same load. Qt's figure is in parentheses.

| GPU (`renderD128`) | Idle, before | Idle, after | Clicked, before | Clicked, after |
|---|---:|---:|---:|---:|
| animations | 2.2 (2.1) | 2.6 (2.1) | 3.2 (1.7) | 2.6 (1.9) |
| clocks | 2.3 (1.7) | 2.0 (2.0) | 2.1 (3.3) | 2.4 (1.9) |
| samegame | 1.8 (1.8) | 1.1 (1.1) | 9.0 (3.5) | 7.6 (3.5) |

| llvmpipe (sway on pixman) | Idle, before | Idle, after | Clicked, before | Clicked, after |
|---|---:|---:|---:|---:|
| animations | 14.3 (21.6) | 15.4 (23.1) | 13.3 (23.1) | 15.2 (22.3) |
| clocks | 11.2 (21.1) | 12.3 (22.0) | 11.2 (20.6) | 13.8 (22.5) |
| samegame | 8.1 (26.2) | 8.4 (32.0) | 91.3 (234.4) | 114.2 (269.5) |

The machine was not quiet. The load average during the runs was 3–98 on the GPU and 5–107 on
llvmpipe. Qt's own figures moved by up to 20 % between the two sets, so these differences are
within the noise. The earlier "before" set, at a load of 35–140, read 3.4 / 2.0 / 8.0 on the GPU.

**Where a frame's CPU goes.** Measured with `frame_profile`, without a profiler. At 60 frames a
second:

- **animations:** the pass takes 0.08 ms a frame, about 0.5 % of a core.
- **clocks:** the pass takes 0.017 ms a frame, about 0.1 %.
- **samegame:** about 0.3–0.5 ms a frame while it is clicked, 2–3 %. More than half of that is
  composing and encoding the scene. The script and its animation, particle and binding work is
  about 1.3 % of a core.

Every frame of animations and clocks asks for the next one, so neither is drawing frames it does
not need. The script and binding side cannot account for the gap on a GPU. It is in what each frame
costs past the pass: painting, submitting and presenting.

**What is left.**

- **animations and clocks on a GPU.** The gap to Qt is outside the scene's pass, in the GL
  painter, the submit and the present. That is renderer work.
- **samegame clicks on a GPU.** About 55 % of the pass is render compose and encode: growing the
  clip stack, and hashing texture keys with SipHash. The rest is QuickJS (about 284 k
  instructions a frame), animation steps (280 k) and particles (175 k). A script property write
  with what it sets off costs about 28 k instructions. Its cascade, not the write, is what costs.
- **An effect rerun** costs about 1.4 k instructions besides what it reads: the heap pop, the
  finish and the edge diff. A rotated item's whole geometry effect reruns while only its transform
  list moves (7 a frame in clocks, 48 k). Giving the transform list an effect of its own would add
  an effect to every item.

- **rssnews keeps working after it settles.** It uses 3–4 % with no frames and no wakeups, while
  pictures are still fetched and decoded. Qt has finished by then.

### Hot reload: from the save to the first frame (2026-10-18)

`ljos run --watch` now logs each reload with its own clock:

```text
ljos: reloaded in 83.9 ms (compile 4.2 ms, apply 79.7 ms): 8137 kept, 0 new, 0 made again; shown 111.4 ms after the save
```

"Shown" runs from the file's modification time to the end of the first frame painted after the
reload. Each corpus app was copied to scratch and run in the off-screen sandbox
(`desktop:cua`: headless sway, Xwayland, llvmpipe) as a release build. A script drove it into a
non-initial state and edited its QML as a developer would. Other builds
were running on the machine, so take the spread, not the digits.

At the start there was no in-app clock. "Before" is the sweep's own time from the save to the
change showing in the accessibility tree, polled every 50 ms. It is an upper bound and includes the
watcher's wait for the save to settle.

| App, edit | Before: save → seen | After: reload | After: shown after the save |
|---|---:|---:|---:|
| calqlatr, a colour in `content/Display.qml` | 1 000–2 040 ms | 10–13 ms | 28–43 ms |
| samegame, a text in `Main.qml` (8 137 objects kept) | 1 050–1 510 ms | 84–104 ms | 111–140 ms |
| samegame, a component used by every block | 1 110 ms (did not show) | 129–328 ms | 153–376 ms |
| thermostat, a text in a `.ui.qml` page | 360–376 ms (in-app clock) | 217–234 ms | 266–285 ms |
| todolist, a label on a pushed page | 330–450 ms | 33–36 ms | 52–61 ms |
| maroon, a text in a scene | 50–585 ms | 10–12 ms | 26–35 ms |
| rssnews, a text in a delegate | 50–60 ms | 6–7 ms | 22–38 ms |
| clocks, a colour in the delegate | 50–490 ms | 9–13 ms | 34–37 ms |

What dominated, and what was cut:

- **Waiting for the save.** The watcher waited a fixed debounce after each event. A save that has
  finished now reloads 15 ms after its last write.
- **Formatting at every object.** The reload built a path string and a log line for each object it
  matched, even with logging off. Paths are now pushed segment by segment and hashed with Fx, and
  the steps are logged at debug level only.
- **Bookkeeping for the tree being replaced.** The tree that a reload throws away no longer works
  out the bindings and connections it still owes.
- **Quadratic script output.** A script that made hundreds of objects was counted past all of them
  for each one it made.
- **Measuring the JS heap to find a log line's origin.** A warning logged outside a script looked
  for its script by measuring the heap. It now looks only while a script runs.
- **Palette and font cascades.** Setting a colour on one control told every palette in the scene
  to look again at every reload. Now only the first colour set does. A font set on a control tells
  the others only under a style font.

What was left then: apply was the whole cost. On samegame it was re-binding the 8 137 objects it
keeps. On thermostat it was remaking the `.ui.qml` page that was edited, with its anchors and
images. The next section takes that on.

Over 20 reloads, RSS stayed within 1.3 MB on samegame, thermostat and todolist. calqlatr's grew
by 11.8 MB in two steps in one run and stayed flat in the next. Idle CPU after the reloads was 0 % on every app except the ones that animate
themselves (clocks' timer, maroon's scene). The next section explains calqlatr's steps.

### Hot reload: an edit applied where the objects stand (2026-10-18, later)

An edit that changes only values and bindings is now applied to the live objects. Nothing is
made again and nothing is destroyed: the edited bindings are put in place and the edited values
written. Any other edit (an object added, removed, moved or retyped, a new member, a changed
handler) takes the full path of the section above, which makes the edited files' objects again.

These numbers come from `reload_profile` (`crates/ljos_shell/examples/reload_profile.rs`). It
shows the app in a `ShellApp` on an `egui::Context` with no window, saves an edit again and again,
and reports the time from the save to the end of the first pass that shows it. Each app was a
copy in scratch, release build, six saves per row. thermostat had its control page opened and
samegame a game started before the first save. To make a value edit take the full path, the same
edit was made with a line break in the new value. The load average on the machine was 13–38.

| App, edit | Before: build + apply → shown | After: build + apply → shown | Kept, new before → after |
|---|---:|---:|---:|
| calqlatr, a colour in `content/Display.qml` | 4 + 5 → 26 ms | 2.5 + 0.2 → 18–20 ms | 220, 0 → 233, 0 |
| calqlatr, the same edit on the full path | 2.6 + 7 → 26–27 ms | 2.4 + 7 → 27–31 ms | 233, 1 → 233, 0 |
| samegame, a text in `Main.qml` | 5 + 74–96 → 100–121 ms | 5 + 2 → 22–24 ms | 8 620, 0 → 8 620, 0 |
| samegame, the same edit on the full path | 5 + 74–96 → 100–121 ms | 5 + 70–105 → 96–132 ms | 8 620, 0 → 8 620, 0 |
| samegame, a duration in `content/Block.qml` (every block) | full path | 4.5 + 2 → 22–23 ms | 8 620, 0 |
| thermostat, a text in `ThermostatViewForm.ui.qml` | 16 + 188–245 → 262–320 ms | 16 + 2 → 33–40 ms | 4 582, 876 → 4 914, 0 |
| thermostat, the same edit on the full path | 16 + 188–245 → 262–320 ms | 15 + 185–209 → 234–259 ms | 4 582, 876 → 4 902, 0 |

Every row kept the app's state: samegame's board and score, thermostat's open page, calqlatr's
display. The shown time of a few in-place saves was 0.5–1.1 s while build and apply stayed at
2–7 ms. Those saves came when the load was highest. They are left out of the ranges above.

What changed on the full path:

- **A Repeater in a Container made its items twice.** The Container moved the Repeater into its
  content item as it completed, and the move made the items again. A Repeater declared in a
  Container now makes its items once. One moved later still makes them again, as on Qt
  (`features/controls/container-repeater.feature`). On thermostat this halved the objects made
  new (876 → 431) and took about 30 % off the full path's apply.
- **The tree being replaced was given back what it was about to lose.** When the root was
  replaced, the scene put back every hidden and disabled item's own `visible` and `enabled` first.
  That ran their bindings again just before they were destroyed. Now only items that outlive the
  old root get them back (`QuickScene::replace_root`).
- **Releasing the old tree** (`destroy_now`) is now about 14 % of a full reload, and taking
  objects into the scene is about 8 %. The rest is making the new objects. Dropping the
  destroyed objects' headers is about 2 %. It is not put off until after the frame, because a
  native destructor may touch the engine.
- **Templates counted as new.** A style gives each part of a control as a `Component`, which is
  destroyed once the part is made. A `ListElement` goes with the rows it declared. With nothing
  at their place in the old tree, every reload counted them as new (431 on thermostat, 1 on
  calqlatr). They hold no state and are not counted now.

**calqlatr's memory over reloads.** The two steps of the section above were not a leak.
Measured by mapping in the windowed sandbox over 90 reloads, they were:

- glibc's heap moving its high-water mark in both directions: 40.7 → 66 → 51.8 → 52.6 MB;
- pages of shared libraries (LLVM, the GPU driver) paged in and out.

With logging off, the heap stayed at 40.7 MB.

There was also a small real leak, found without a window: about 11 kB more in use per full
reload. A script that holds an object a reload replaced reaches the object that went on for it,
through a table of forwards. Every full reload added a forward for every replaced object a script
had a wrapper for, and dropped none. Now a forward goes when no script holds the old object any
more. Over 100 full reloads, the memory in use went from 22.6 → 24.45 MB before to 22.6 →
23.47 MB after, and stayed flat from about the 40th reload. RSS was flat from the 60th.

What is left:

- **The full path is still over 100 ms on large pages.** It takes 234–259 ms on thermostat and
  96–132 ms on samegame. Getting it lower means making less: keep the subtrees of an edited file
  whose source did not change, rather than remaking the whole file. Only value and binding edits
  are under 100 ms now.
- **samegame's full-path apply rises from about 76 to 100–115 ms over 16 reloads, with RSS
  flat.** The load rose over the same runs, so it is not yet clear that the reloads cause it.
- **thermostat logged two warnings each time its control page was made**, at start-up as on a
  reload, for `palette.highlight: "#2CDE85"` on a `Row`. Every item has a `palette` since, as in
  qml6, and the warnings are gone.


### A write's cascade, a turning transform and the script crossing (2026-10-18)

Measured as in the section before: callgrind over the egui passes, the 1200-frame run less the
600-frame run, divided by 600. samegame's boards are random, so it is given for each write rather
than for each frame.

| Instructions | Before | After |
|---|---:|---:|
| clocks: a frame | 221 k | 189 k |
| clocks: the geometry effect's reruns, a frame | 48.7 k | 0 |
| clocks: the transform list's reruns, a frame | — | 19.1 k |
| clocks: finding an object's script wrapper | 335 | 195 |
| animations: a frame | 1 158 k | 1 159 k |
| samegame: a property set from a script | 28.1 k | 24.0 k |
| samegame: a write a Behavior takes | 23.6 k | 18.3 k |
| samegame: starting the Behavior's animation | 16.8 k | 12.6 k |

- **A Behavior's write** stopped the run it replaced by sweeping every running animation. A
  stopped run is now marked, and the next tick, wake or count sweeps once. The timeline reads a
  `PropertyAnimation`'s targets, ends and duration, and a spring's or smoothed animation's
  parameters, by index instead of by name. The plan is built twice for each write.
- **A turning transform.** A `Rotation` or `Scale` in `transform` that moved reran the item's
  whole geometry effect. An item whose transform list is not empty now has an effect of its own for
  the list, made when the list fills and dropped when it empties. Every other item has no new
  effect. The item's own `rotation` and `scale` stay in the geometry effect. The effect run count
  is the same (7.3 a frame in clocks), but each run reads the list alone: about 2.6 k instead of
  6.7 k.
- **The script crossing.** Every value a script reads, and every call into it, finds its object's
  wrapper by id, and the maps hashed that id with SipHash. They use Fx now. Past that, the glue of
  a crossing is small: about 500 instructions for a property read and 2 k for a set before the
  write. What a handler costs is the cascade it sets off.

**CPU, as % of one core.** Desktop profile, medians of 3. "Before" is 389f819, run straight after
"after" so that both met the same load. Qt's figure is in parentheses.

| GPU (`renderD128`) | Idle, before | Idle, after | Clicked, before | Clicked, after |
|---|---:|---:|---:|---:|
| animations | 2.6 (1.8) | 2.7 (1.5) | 4.0 (1.9) | 2.9 (1.6) |
| clocks | 3.9 (2.1) | 1.9 (1.9) | 2.2 (2.0) | 2.0 (1.8) |
| samegame | 1.9 (2.9) | 1.6 (2.0) | 8.0 (5.8) | 9.8 (3.7) |

| llvmpipe (sway on pixman) | Idle, before | Idle, after | Clicked, before | Clicked, after |
|---|---:|---:|---:|---:|
| animations | 13.7 (22.5) | 16.3 (22.2) | 12.8 (22.1) | 16.7 (22.9) |
| clocks | 10.7 (—) | 8.2 (20.6) | 11.2 (—) | 8.0 (21.5) |
| samegame | 7.9 (27.2) | 7.8 (25.9) | 89.2 (222.8) | 89.5 (241.1) |

The machine was not quiet. The load average was 2–91 on the GPU and 14–112 on llvmpipe, and it
moved by tens within a set. The bench refused Qt's clocks run before the change: its cgroup held
other processes. These differences are within the noise. The instruction counts above are the
measure.

**What is left.**

- **An effect rerun** still costs about 1.4 k instructions in the graph's bookkeeping: the heap
  pop, the guard and the edge diff. That path already skips the diff when the sources are the
  same.
- **A spring's plan** (`Track::plan`, about 3.4 k a write) simulates the spring to find when it
  settles, which a write to a property with a `SpringAnimation` Behavior needs.
- **samegame on a GPU** is still most of all render compose and encode, with texture keys hashed
  by SipHash. That is renderer work.

### Pictures kept at the size they are drawn, and let go once drawn (2026-10-18)

The same apps, profile and options as "Memory against Qt" above, Ljós only. Qt is the same 2026-10-17 run.
Before (8ea10ca) and after were run back to back under a load of 2–33 and 13–58. The after run was
made in parts: the bench twice refused an app whose scope it found in a cgroup holding other
processes, and the apps it refused were run again with `--append`.

| App | Peak, before → after (Qt) | Settled | Anon PSS | PSS |
|---|---:|---:|---:|---:|
| animations | 79.2 → 74.1 (103) | 69.4 → 68.9 (106) | 57.4 → 57.4 (92.7) | 143 → 115 (144) |
| listview | 64.0 → 64.7 (110) | 67.3 → 67.7 (106) | 58.1 → 58.0 (87.3) | 122 → 111 (142) |
| calqlatr | 69.8 → 70.1 (112) | 69.3 → 69.9 (113) | 61.0 → 60.9 (90.9) | 121 → 117 (159) |
| clocks | 74.4 → 72.9 (107) | 71.1 → 69.8 (110) | 58.8 → 58.3 (98.2) | 130 → 110 (151) |
| maroon | 81.2 → 73.6 (115) | 81.9 → 75.2 (120) | 72.4 → 66.1 (108) | 138 → 116 (157) |
| rssnews | 80.2 → 77.8 (116) | 98.1 → 92.2 (261) | 88.8 → 82.1 (245) | 152 → 140 (307) |
| samegame | 77.4 → 77.9 (111) | 72.6 → 71.5 (116) | 61.6 → 60.9 (104) | 123 → 113 (166) |

Anon PSS moved in the two apps that show pictures larger than a few icons: maroon by 6.3 MiB and
rssnews by 6.7 MiB. The others' pictures are small, and the rest of their memory is the same code.
As before, PSS and peak follow the page cache.

- **Pictures not painted** are kept at the size their item draws them, as painted ones were. That
  is done from the pixels already kept, as soon as the picture comes and whenever the item's size
  or fill mode changes, so nothing is fetched or decoded again. In rssnews a row's width follows
  its picture's size, so the row has no size when the picture comes.
  `features/items/image-kept-size.feature` and four tests in `ljos_quick/tests/image.rs` count the
  bytes kept.
- **Pixels once drawn.** A picture that can be made again from fewer bytes than its pixels lets
  them go once it is a texture. A file is read again; registered, `data:` and fetched bytes are kept
  for it when they are fewer than the pixels. A reader (Canvas `drawImage`, effects, tinting,
  particles, the R backend, a shrink) has them made again once, and they stay. Nothing is done per
  frame: letting go happens at upload, and making again only when a reader asks.
- **Pictures for rows gone.** In rssnews six of the nine pictures it asks for arrive after the row
  that asked has been let go. They were kept as about 2 MiB of pixels each. One that can be made
  again from fewer bytes is now kept as those bytes, so a row made later decodes them and does not
  fetch them again.
- **The table of types** that compiling from source reads (`ljos run`, `--example`) gives back
  the room its lists grew: 1.02 → 0.87 MiB. A shipped application never reads it.

**What is left.**

- **Pictures whose bytes outweigh their pixels.** rssnews's feed names JPEGs up to 7728 × 5152,
  of several MiB each, kept at 896 px (2 MiB). They stay as pixels: keeping the bytes instead
  would cost more, and dropping them would cost a fetch. Three of the pictures for rows gone are
  such pictures, about 6 MiB.
  Since then, such a picture that comes for rows gone is no longer kept ("Rows let go, …" below).
- **On llvmpipe a texture is in the process's own memory.** There, letting pixels go after upload
  frees one of two copies; on a GPU it frees the only copy in system memory.
- **Why rssnews lets rows go before their pictures come** was not looked into. Qt asks for 7
  pictures where Ljós asks for 9. Since then, it was looked into and the extra rows are fixed ("Rows let go, …" below).
- **Memory outside Rust** (llvmpipe, Mesa) and **the binary's resident text** are as above.

### photosurface on a folder of 472 large pictures (2026-10-18)

photosurface is given a folder, after `--`, of 472 generated PNGs (seed 15): 64 to 3440 px a side,
mostly 0.3 to 3.5 MPx, flat colour with noisy strips. The files take 134 MB, and their pixels
1.8 GiB as RGBA. The folder is generated so that no one's own pictures are measured. Ljós runs
`--project <dir> --example photosurface` and Qt runs `qml6 -- <folder>`, in a 1024 × 600 window
with the gallery's `qmldir` overlay. The profile is `desktop`; the build is release (lto thin) on
e68954b with what is below; Qt is 6.11.2 with `QT_QUICK_CONTROLS_STYLE=Basic`. The welcome
slides set the folder when they end, so the pictures load a while after the first frame. Ljós
starts loading about 5 s after the first frame, and Qt about 20 s after. Settled memory is read
40 s after the first frame (15 s for "before", which Ljós had loaded by then).

| | Ljós before | Ljós after | Qt |
|---|---:|---:|---:|
| First frame (ms) | 366 | 282 | 387 |
| CPU to first frame (ms) | 833 | 834 | 267 |
| Peak while starting (MiB) | | 73.7 | 127 |
| Settled (MiB, cgroup) | 2060 | 250 | 3920 |
| Settled anon PSS (MiB) | 2042 | 241 | 3897 |
| Settled PSS (MiB) | | 314 | 3981 |
| Idle CPU (% of one core) | | 0 | 0.01 |

**While the pictures load.** The bench drove each app for 8 s from the moment it starts loading:
Ljós with `--settle 4 --idle 1 --load 8`, Qt with `--settle 19 --idle 1 --load 8`.

| While loading | Ljós | Qt |
|---|---:|---:|
| Frames shown (per second) | 10.8 | 2.55 |
| Median frame interval (ms) | 36.0 | 360 |
| 95th percentile frame interval (ms) | 363 | 366 |
| CPU (% of one core) | 438 | 156 |

Sampling `/proc/<pid>/task/*/stat` each second shows how the work was split. Qt's main thread
spent 1.75 s of CPU over 3 s, 1.08 s of it in one second, while its other threads were idle. Its
RSS went from 189 MiB to 3994 MiB in those 3 s. In Ljós the main thread spent 0.2–0.5 s per second
for 5 s, while the pool threads decoded at 5–9 cores; RSS peaked at 404 MB.

- **Decoded away from the user interface.** `asynchronous` is false by default, so Qt decodes a
  local file on the UI thread when its source is set. Ljós does the same for a small picture.
  For a still picture over 512 × 512 with no `sourceSize`, Ljós reads only the header; the
  Image is Ready with its size at once, as in Qt. The pixels are decoded on the pool and painted
  when they come. A broken body turns the Image to Error when its decode comes back.
  `features/items/image-decoded-aside.feature`, and a test in `ljos_quick/tests/image.rs`.
- **Decoded at the drawn size.** The decode asks for the size the item is drawn at, the limit
  "Pictures kept at the size they are drawn" uses, so the whole picture is not kept, not even for
  a moment. photosurface draws each picture inside a frame scaled down to about 200 px, so the drawn
  size now counts the `scale` of the item's parents as well as its own. Before, a picture loaded
  this way was kept at the view's size, up to 1024 px.

**DECISIONs.**

- Only local still pictures with a plain request take this path. Readers other than the screen,
  such as Canvas `drawImage` and effects, still decode on the spot, because they need the pixels
  in the same frame.
- The first frame of a large picture may lack its pixels; in Qt it never does. Two scenarios now
  wait 100 ms before reading pixels (`image-kept-size.feature`, `image-large.feature`).

**What is left.**

- **Ljós's welcome ends about 15 s before Qt's.** Each slide lasts 5 s, so Ljós seems to show one
  slide of `welcome.pdf` where Qt shows four. The reason was not looked into.
- **CPU while loading** is higher in Ljós (438 % against 156 %) because it decodes on many
  threads at once and keeps drawing frames. Qt blocks instead.
- **CPU to first frame** is unchanged by this work; it is the compile from source, as in the
  sections above.

### Rows let go, pictures for rows gone, and shared type tables (2026-10-18)

The same apps, profile and options as "Pictures kept at the size they are drawn" above, Ljós only. Qt is the same 2026-10-17 run. Before
(8d9db17) and after (c6a8508) were run back to back under a load of 10–44 and 15–83.

| App | Peak, before → after (Qt) | Settled | Anon PSS | PSS |
|---|---:|---:|---:|---:|
| animations | 73.3 → 72.5 (103) | 69.2 → 65.0 (106) | 58.0 → 53.7 (92.7) | 116 → 145 (144) |
| listview | 64.7 → 59.8 (110) | 67.5 → 63.6 (106) | 58.4 → 53.9 (87.3) | 116 → 141 (142) |
| calqlatr | 70.1 → 65.1 (112) | 69.3 → 63.9 (113) | 61.2 → 56.7 (90.9) | 121 → 144 (159) |
| clocks | 86.8 → 72.3 (107) | 75.2 → 66.0 (110) | 58.5 → 54.4 (98.2) | 118 → 142 (151) |
| maroon | 75.8 → 70.0 (115) | 75.2 → 71.7 (120) | 66.5 → 62.0 (108) | 121 → 147 (157) |
| rssnews | 79.1 → 73.4 (116) | 91.6 → 75.4 (261) | 80.7 → 67.3 (245) | 137 → 157 (307) |
| samegame | 78.2 → 76.3 (111) | 71.8 → 68.8 (116) | 61.4 → 56.9 (104) | 117 → 146 (166) |

Anon PSS went down by 4.1–4.5 MiB in every app, and by 13.4 MiB in rssnews. PSS went up by 20–29 MiB,
and all of that is file pages. The after run's file PSS was 83–88 MiB against 52–58 MiB before,
because its binary had just been built while the load was higher. Anon PSS and the cgroup's
settled memory both went down. Time to first frame moved within the noise of these loads.

- **Rows a new list lets go.** A row was measured after one polish since it was made. When the row
  is a text over a block whose height follows the text, that is before the text has been shaped.
  The list then made rows for a size that was too small and let them go a polish later. The row
  the list waits for is now measured after two polishes, as Qt measures a row before it makes the
  next. At 800 × 600, rssnews makes rows 0–10 where Qt makes 0–8. It keeps rows 0–4, as Qt does,
  and asks for the same 8 pictures as Qt, none of them twice. The 9 and 7 noted above were counted
  at another window height. `features/views/list-rows-let-go.feature`; on Qt and on Ljós.
- **A picture kept in shape** (`PreserveAspectFit`) with only a width given is now as tall as it is
  drawn, not as tall as the picture, as on Qt (`features/items/image-fill-mode.feature`). A row
  whose height follows its picture is then measured right the first time.
- **Pictures for rows gone whose bytes outweigh their pixels** are no longer kept. Qt fetches such
  a picture again, and so does Ljós now. A smaller one is still kept as its bytes.
  `features/items/image-network.feature`; on Qt and on Ljós. The large JPEGs were already decoded
  straight to the kept size (a scaled IDCT, 1/2 to 1/8), so neither the full pixels nor the bytes
  of a picture on screen were kept. What was left was these pictures for rows gone.
- **Type tables.** Each type's property and signal tables began as a full copy of its base's, so
  every description was held once per type that inherits it. A dhat profile of clocks had 6.4 of its
  13.7 MiB of Rust heap there. Property and signal entries are now behind an `Arc` shared down the
  hierarchy, and a built table keeps no room to grow. Rust heap in clocks went from 13.7 to 8.3 MiB.
  `a_derived_type_shares_what_it_inherits_and_copies_what_it_changes` (ljos_engine).

**What is left.**

- **llvmpipe.** massif puts 34 MB of clocks's heap in libgallium. Its call stacks have no symbols. By
  their sizes, they are mostly the rasteriser's scene bins and textures. The textures are the window's buffers and Ljós's glyph atlas, its
  pictures and its shapes. egui's atlas was cut in the run before. What is left there is set by
  the window size and the driver, not by Ljós's choices.
- **The NVIDIA user-space libraries** hold about 4.4 MiB anon in each app on this machine, which
  has no NVIDIA GPU. glvnd loads them. This is the machine, not Ljós: Qt's GL goes through glvnd
  too (not measured here).
- **The reactive graph** (1.2 MiB, sized at start) and **the type oracle read for `--example`**
  (0.9 MiB) are now the largest Rust items. A shipped application never reads the oracle.
