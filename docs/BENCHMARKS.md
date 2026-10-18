# Benchmarks: Ljós and Qt side by side

`mise run bench` measures the same QML applications on Ljós (`ljos run`, release or `dist` build)
and on Qt (`qml6`). Each application runs inside a limit on memory and CPU. Every number is read
from outside the application:

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

Examples get the same project directory and `qmldir` overlays as in the gallery. Both runtimes open
them at the root's own size, as qml6 does, not at the gallery's capture size: until 2026-10-18 Ljós
was given the capture size and Qt was not, so clocks, rssnews and samegame drew bigger windows on
Ljós (samegame 800×600, a bigger board, against Qt's 320×480).

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
`BENCH_PROFILE=dist mise run bench …` builds and measures the `dist` profile instead, the binary
that is handed out (one LLVM module; the build takes about 15 minutes). The report's notes name
the profile.
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
550 ms. Pass it with `--app-env` to tell the machine's drivers apart from Ljós. (Since
2026-10-18 Ljós leaves NVIDIA's Vulkan driver and EGL vendor out itself while NVIDIA's kernel
driver is not loaded; see "Vulkan drivers left out as EGL's are" below.)

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


### Hot reload: handlers, functions and moved lines in place (2026-10-18, evening)

A file that keeps its shape now takes more edits in place, as well as values and bindings:

- a handler body (`onClicked: …`);
- a function body;
- a value turned into a binding, or the reverse;
- an edit that moves lines, such as a line break, a comment or a member added on a line of its
  own.

Every handler, function and binding runs through a slot that holds its file and expression. A
reload in place points each slot at the new file. Objects and bindings kept over moved lines are
told their new line, so a later warning gives the line the file has now. These edits still take
the full path:

- an object added, removed, moved or retyped;
- a member removed;
- a signal, an enum, or a function's parameters changed;
- a member added over a property another layer overrides (a StackView page's `width`);
- any edit while a file with `pragma LjosReload: reset` is live.

Measured with `reload_profile`, as in the section before. It now registers `QtQml.XmlListModel`,
so rssnews loads; before, every rssnews reload failed with the module not installed. The
"before" column is the path the rules of the section before give; it was not measured again.
Two saves per row for clocks, maroon, rssnews and todolist, and six for thermostat and samegame
(control page opened, game started). The load average was 15–40 for clocks, maroon, rssnews and
todolist, and 6–8 for thermostat and samegame.

| App, edit | Path before → now | Shown | Kept, new |
|---|---|---:|---:|
| clocks, a colour in `Clock.qml` | in place → in place | 16.6–17.5 ms | 85, 0 |
| clocks, a function body | full → in place | 17.0 ms (outliers at load 39) | 85, 0 |
| clocks, a handler body | full → in place | 16.3–17.2 ms | 85, 0 |
| clocks, a line break | full → in place | 16.3–17.4 ms | 85, 0 |
| clocks, an object added | full → full | 20.5–22.4 ms | 85, 4 |
| maroon, `x` in `Main.qml` | in place → in place | 19.7–19.8 ms | 158, 0 |
| maroon, a binding in a delegate | in place → in place | 19.9–20.2 ms | 158, 0 |
| maroon, a `Timer` handler | full → in place | 19.7–20.7 ms | 158, 0 |
| maroon, a line break | full → in place | 20.7–20.8 ms | 158, 0 |
| maroon, an object added | full → full | 24.5–25.4 ms (apply 3.7 ms) | 158, 1 |
| rssnews, a colour in `Main.qml` | failed → in place | 17.7–18.5 ms | 80, 0 |
| rssnews, a handler body | failed → in place | 18.2–18.3 ms | 80, 0 |
| rssnews, a line break | failed → in place | 20.5 ms | 80, 0 |
| rssnews, an object added | failed → full | 46–124 ms (apply 10–20 ms) | 70, 1 |
| todolist, a text in `HomePageForm.ui.qml` | in place → in place | 23.0–23.1 ms | 481, 0 |
| todolist, a function body in `TasksListsView.qml` | full → in place | 23.2–23.8 ms | 481, 0 |
| todolist, a line break | full → in place | 23.7–23.8 ms | 481, 0 |
| todolist, an object added | full → full | 51–57 ms (apply 23–27 ms) | 455, 6–7 |
| thermostat, a text in `ThermostatViewForm.ui.qml` | in place → in place | 32.5–34.4 ms | 4 914, 0 |
| thermostat, an object added there | full → full | 201–259 ms; the first save 1 028 ms | 4 902, 1 |
| samegame, a text in `Main.qml` | in place → in place | 22.2–23.0 ms | 8 620, 0 |
| samegame, an object added there | full → full | 99–127 ms | 8 620, 1 |

todolist warns at every load that four icons cannot be opened. They are given as
`"images/…"` through an alias to an `Image` in `imports/CustomControls`, and resolved against
that file. qml6 does the same for the same layout. The warnings come at start-up too, so they are
not caused by a reload.

**Why the full path is not under 100 ms yet.** The steps of thermostat's full path, at load 6:

- make: 88–128 ms;
- destroy: 40–53 ms;
- swap: 21–28 ms;
- snapshot: 4 ms.

Keeping the unchanged subtrees of a remade file would save the make and destroy of everything
but the edited object. That needs one of two things the engine cannot do now. It could make part
of a component into an instance that already exists. Or it could move nested instances from the
old instance of the file to the new one. A binding, an id or a handler of a kept subtree can name
any object of its file, so each one would be pointed at the new file. Both changes touch every
part of instantiation, so the work went where the edits are made most: edits that keep a file's
shape, now including moved lines, go in place at 16–35 ms. An edit that adds, removes or moves
an object is still the full path.

samegame's full path is spread differently, at load 6–7:

- objects made by scripts again: 26–37 ms;
- destroy: 21–29 ms;
- swap: 20–25 ms;
- make: 2–5 ms.

Its 8 000 blocks are made by `createObject`, not declared, so keeping declared subtrees would not
help it.

**samegame's apply over 16 reloads**, at load 6.5:

| Reload | Apply |
|---|---:|
| 0 | 74 ms |
| 1–3 | 83–87 ms |
| 4–15 | 92–101 ms, with no rise after the 5th |

The step that grows is the scripted one: 26 ms at first, 35–37 ms from the 5th reload. RSS went
from 95 MB before the first reload to 133 MB after it, then stayed at 138–142 MB. So the rise
reaches a plateau after about five reloads and does not keep growing. Why the first full reload
is the cheapest was not found.


### Hot reload: objects added and removed in place (2026-10-18, night)

An object added to one that stays, an object removed, and an edit of properties together with an
added object now go in place. The new object is made into the parent that is already there, at the
place the file gives it among its siblings. A removed object hears `Component.onDestruction` and
is destroyed. Every other object keeps its state. A page that a `StackView` pushed from a file
grows in place too, as does a `Loader` given a `source`, because it is made once from its file
like a root. These still take the full path:

- an object moved, reordered or given another type;
- an id moved to another object;
- an edit inside a delegate, a `Repeater` row or a `Loader`'s `sourceComponent`;
- an object placed before a sibling in a list that can only append. `Item.data` can insert, so
  children and resources can be placed anywhere.

Measured with `reload_profile`, as in the sections before, at load 59–74 (other work on the
machine). Saves alternate between the edit and the file as it was, so every other row is the
object removed again.

| App, edit | Path before → now | Shown | Apply |
|---|---|---:|---:|
| thermostat, an `Item` added to a `Text` (6 saves) | full → in place | 33.9–37.8 ms | 2.4–2.8 ms |
| thermostat, the `Text`'s text edited and an `Item` added (4 saves) | full → in place | 34.1–36.6 ms | 2.4–2.6 ms |
| samegame, an `Item` added to a `Text` in `Main.qml` (6 saves) | full → in place | 22.9–24.8 ms | 2.8–3.7 ms |

Before, the same edits were shown in 201–259 ms on thermostat and 99–127 ms on samegame. Most of
what is left is the watcher's quiet time (15 ms) and the build (5–19 ms). samegame's resident
memory stayed at 92–94 MB over the six saves.

**thermostat's first save at 1 028 ms.** The steps of the save from the section before were timed
again over 24 first saves. The extra time was not the reload's. `reload_profile` wrote the file on
the thread it applies the reload on. ext4 writes out a file that was truncated and written again
when it is closed. That `close` returned 46 ms to 2.5 s after the file was whole, and the reload
waited for it. The watcher's quiet time, the build and the frame after were as for any other save.
Once in 24 runs, the build took 631 ms at load 40–50. A save from an editor comes from another
process, so it never waited like this. `reload_profile` now writes from a thread of its own and
tells the wait between the build and the apply. Over 12 first saves of thermostat's full path at
load 40, the wait was 0–0.7 ms and the save was shown in 205–555 ms, as the apply (162–468 ms)
followed the load.

A function a script made before an edit that moved lines (a `connect` callback, a `Qt.binding`)
now also throws at the line the file gives it now. The places the reload moved are applied to the
line table of the script loaded for the file before, which those functions still run.

### Hot reload: delegates, moved objects and sorted lists in place (2026-10-18, late night)

More edits that change a file's shape now go in place:

- An edit inside a delegate of a `ListView`, `GridView` or `Repeater`, or inside a `Loader`'s
  inline `sourceComponent`, goes to every live row whose key matches. Rows keep their scroll
  position, a field's text and a running animation. Rows made later use the new delegate.
- An object moved to another parent in the same file, or among its siblings, is made again
  where the file now puts it. It keeps its scripts' values, as on the full path. The longest
  run of siblings still in order is kept, and only the others are made again.
- An object of another type at the same place is made again, with what is in it. Nothing else
  changes.
- An object added before others goes in place in lists that sort what they hold:
  - a `Flickable`'s content;
  - the content of a `Pane`, a `Popup` or an `ApplicationWindow`;
  - a `Container` such as `SwipeView`;
  - a `Menu`;
  - an `ObjectModel`;
  - the `states` and `transitions` lists.

  Each of these lists can now insert as well as append. `Window` already used `Item.data`, which
  could insert.

These still take the full path:

- an id moved to another object;
- an object removed or reordered in a list given to a member (`states: [ … ]`).

Measured with `reload_profile`, as in the section before. Saves alternate between the edit and
the file as it was. The delegate rows were measured once before, at load 16–20, and four times
after, at load 31–36. The other rows were measured four times each, at load 36–46. "Kept, new"
is now.

| App, edit | Path before → now | Shown | Apply | Kept, new |
|---|---|---:|---:|---:|
| clocks, an object added in `Clock.qml`, the list's delegate | full → in place | 19.9 → 16.7–17.7 ms | 2.2 → 0.4–0.6 ms | 142, 7 |
| rssnews, an object added in an inline delegate | full → in place | 21.7 → 17.4–18.5 ms | 3.6 → 0.4–0.5 ms | 80, 5 |
| rssnews, an object added in `CategoryDelegate.qml` | full → in place | 21.7 → 17.3–18.7 ms | 3.2 → 0.4 ms | 80, 5 |
| rssnews, that object removed | full → in place | 20.2 → 17.2–18.6 ms | 2.4 → 0.4–0.8 ms | 70, 0 |
| clocks, two siblings swapped in `Clock.qml` | full → in place | 30.9–38.1 → 18.2–20.1 ms | 8.4–10.9 → 1.0–3.4 ms | 135, 7 |
| clocks, `cityLabel` moved to the delegate's root | full → in place | 24.8–35.3 → 20.6–21.1 ms | 4.5–13.8 → 1.3–1.4 ms | 135, 7 |
| thermostat, an `Item` added first in a `Pane` | full → in place | 94.0–100.1 → 34.0–34.9 ms | 53.5–59.9 → 1.7–1.8 ms | 1 426, 1 |

A moved object is new in every row of the list that shows it, which is the 7 new in clocks.

thermostat's full path for that edit spent 25–28 ms making, 12–13 ms swapping and 15–16 ms
destroying. Its resident memory went from 100–104 MB to 80–83 MB. What is left of its 34 ms is
the watcher's quiet time (15 ms) and the build (15 ms).

**The 631 ms build.** The steps of the build were timed on thermostat over 124 saves. Each was
timed for wall time, time on a processor, time waiting to run, and time spent reading the saved
file:

| Load | Build | On a processor | Waiting to run | Reading the file |
|---|---:|---:|---:|---:|
| 15–22 | 14.5–17.0 ms | 13.8–16.7 ms | under 0.1 ms | under 1 ms |
| 32–106, 40 busy threads and a 3 GB write | 37–172 ms | 22–28 ms | 14–57 ms | up to 7 ms; twice 108–120 ms |
| 44–56, a 3 GB write | 14–338 ms | 14–32 ms | 0–87 ms | up to 2.5 ms; twice 213–323 ms |

The work of the build did not change with the load. It took the same faults, about 1 030 on a
first save and 900 after, and nothing was done only the first time. The time on a processor rose
from 15 to 23–28 ms when every core was busy. That is the cores being shared, not more work. The
long builds were spent off the processor:

- waiting to run, when other work held the cores;
- or blocked reading the saved file while the drive was writing out other data. One build took
  337.9 ms: 14.5 ms on a processor, none waiting to run, and 323 ms in the read.

The 631 ms build was the second case. It was the machine, not the build, so the code was not
changed.

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
  has no NVIDIA GPU. glvnd loads them. (Measured since: Qt on X11 uses GLX and maps none of them,
  and on Wayland it maps them too. Ljós now leaves NVIDIA's EGL vendor out when NVIDIA's driver is
  not loaded; see "Rows measured one by one, a smaller node table, and EGL vendors" below.)
- **The reactive graph** (1.2 MiB, sized at start) and **the type oracle read for `--example`**
  (0.9 MiB) are now the largest Rust items. A shipped application never reads the oracle.

### An animation's step, a clicked samegame and llvmpipe (2026-10-18)

Measured with callgrind over a 10 s run off-screen, all instructions on the main thread divided
by the frames drawn (calls to `QuickScene::frame`); "all threads" adds every thread, Mesa's
included. "Before" is d02fb90. samegame's boards are random, so it is given for each call.

| Instructions | Before | After |
|---|---:|---:|
| animations on a GPU: a frame | 1 803 k | 1 489 k |
| animations on a GPU: the animation tick, a frame | 680 k | 547 k |
| animations on llvmpipe: a frame, main thread | 3.82 M | 2.95 M |
| animations on llvmpipe: a frame, all threads | 14.56 M | 9.82 M |
| animations on llvmpipe: frames drawn under callgrind in 10 s | 309 | 391 |
| clocks on llvmpipe: a frame, main thread | 1.23 M | 1.22 M |
| clocks on llvmpipe: a frame, all threads | 3.12 M | 3.16 M |
| samegame: a picture's quads (`image_segs`) | 786 | 582 |
| samegame: encoding a picture | 1 217 | 1 009 |

- **An animation's step** to a number property goes straight to the property's node: no coercion
  through the generic write, no notify list built, and the graph is already batching the tick, so
  the marks are flushed once. It is taken only for an animation's own write of a number of the
  property's own kind to a property without a filter that has a node; anything else, an `int`
  given a real among them, takes the full write. `is_frozen` answers from a count while nothing is
  frozen, a native field reads a value of its own kind without coercing it, the scene's geometry
  effects look up `Shown` once, and a running animation's timeline is boxed so the run list moves
  less.
- **Pixel snapping and colour bytes** round without calling libm's `round`/`ceil`, which on
  llvmpipe, where every frame's vertices go through them, showed among the main thread's top calls.
- **Damage that covers most of the window** (three quarters or more) draws the window whole
  instead of clearing and scissoring each area. On llvmpipe a scissored clear is a drawn quad with
  its state swapped, about 48 k instructions each.
- **A picture's quads** take one colour, worked out once, and stay white when there is no tint or
  fade.
- **A particle step** asks each painter whether it draws one of the groups instead of copying its
  group list.

**What is left.**

- **samegame clicked**: `QuickScene::frame` is about 87 k a frame: the particle emit from the
  timers 74 M over the run, compose 63 M, the pictures' paint 35 M. Compose is 260–320
  instructions a node. Batching sprites into one draw op would touch about fifteen places across
  the crates and was not done.
- **On llvmpipe through XWayland** the swap's `dri_flush` copies the frame, 150–200 M over a
  samegame run. That is the driver's.
- **Clearing in larger areas.** Clearing whole 256 × 256 areas instead of 64 × 64 ones cut the
  main thread on llvmpipe by a fifth but raised all threads by as much, and fewer frames were
  drawn. The areas stay 64 × 64.
- The load average was 69–81 during the GPU runs and 12–20 during the llvmpipe ones. Callgrind
  counts instructions, so the figures do not follow it.

### The first frame of a single large picture (2026-10-18)

Two files show one 4032 × 3024 picture (smooth gradients with noise, generated with seed 16)
with `anchors.fill` and `PreserveAspectFit` in a 1024 × 768 window: `big-jpg` a 4.3 MB baseline
JPEG (quality 90) and `big-png` a 28 MB PNG. photosurface is as above, without a folder until its
slides end. The profile is `desktop` with `--repeat 5 --settle 1 --idle 1 --load 1`; each cell is
the median of 5 runs. The build is release (lto thin) on dc94ab4, with and without the wait. Qt is
6.11.2. The machine's load average was 9 to 30, from other work, while they ran.

| App | Ljós before | Ljós after | Qt |
|---|---:|---:|---:|
| big-jpg first frame (ms) | 197 | 250 | 293 |
| big-png first frame (ms) | 187 | 289 | 584 |
| photosurface first frame (ms) | 265 | 257 | 311 |
| big-jpg CPU to first frame (ms) | 166 | 265 | 261 |
| big-png CPU to first frame (ms) | 167 | 287 | 432 |
| big-jpg peak while starting (MiB) | 70.2 | 76.5 | 199 |
| big-png peak while starting (MiB) | 74.7 | 84.5 | 200 |
| Settled, each (MiB, cgroup) | 71 | 70 | 202 |

Before, the first frame came without the picture and the picture a frame or so later. After, the
first frame has it, as Qt's has. The window waited 47 ms for the JPEG and 55 ms for the PNG in a
run with a log. photosurface asks for no large picture before its first frame and does not change.

- **The frame waits for the decodes it started.** The shell lays the scene out, asks for the
  large local pictures that wait for that, and waits for their decodes, at most 150 ms for the
  first frame and 50 ms for a later one. It sleeps on the store's condition variable, which each
  result wakes. `features/items/image-decoded-aside.feature` ("The first frame waits a moment
  for a large picture") and a test in `ljos_quick/tests/image.rs`.
- **Asked for once laid out.** A first try asked for the pixels when the source was set, before
  the item had its size or the scene had told the store its view. That decoded the whole picture
  for a moment, and the peak while starting rose to 120 MiB (JPEG) and 143 MiB (PNG). Asked for
  once laid out, the JPEG is decoded smaller by `jpeg-decoder`.

**DECISIONs.**
- 150 ms for the first frame: about three times what one 12 MPx picture takes here. A window
  whose pictures take longer opens without them, as before, and does not wait for them all.
- 50 ms for a later frame, the first after a source is set: the window is up and answers, so
  it waits less. Only decodes started since the last painted pass are waited for, so a folder of
  pictures holds up one frame, not every one.
- Only a large local picture loaded on the spot in Qt (no `asynchronous`, no `sourceSize`) is
  waited for. An `asynchronous` or network picture comes later in Qt too.

themselves (clocks' timer, maroon's scene).

### Startup: the first frame, and the first that answers (2026-10-18)

`ljos bench --profile desktop --warm-up --settle 1 --idle 3 --load 0`, in the off-screen sandbox
(`desktop:cua`: headless sway), with `VK_DRIVER_FILES` naming only the Radeon driver for both
runtimes. "First frame" is the first frame shown. "Answers" is the first frame drawn after the
compositor makes the window 64 px larger, the first sign that it takes input. "Before" is the
`ljos` built from the tree before this work, measured by the same bench (`--ljos`). Before, after
and qml6 took turns app by app, for six rounds on each device, so that the load, which ran from 2
to 90 from other builds, fell on all three alike. Each cell is the median of the six, in ms.

llvmpipe (the sandbox's sway on pixman):

| App | First frame: Ljós before → after | qml6 | Answers: Ljós before → after | qml6 | Idle CPU before → after |
|---|---:|---:|---:|---:|---:|
| animations | 66 → 63 | 117 | 86 → 84 | 134 | 21.55 % → 18.18 % |
| form | 85 → 68 | 168 | 102 → 86 | 185 | 0 % → 0 % |
| listview | 76 → 66 | 144 | 93 → 89 | 161 | 22.85 % → 18.77 % |
| rectangles | 86 → 68 | 122 | 103 → 85 | 155 | 0 % → 0 % |
| text | 86 → 68 | 149 | 102 → 85 | 182 | 0 % → 0 % |
| calqlatr | 94 → 70 | 178 | 110 → 87 | 195 | 0 % → 0 % |
| samegame | 94 → 67 | 153 | 111 → 84 | 169 | 8.49 % → 7.95 % |
| clocks | 94 → 64 | 169 | 111 → 81 | 186 | 11.56 % → 9.67 % |
| thermostat | 153 → 129 | 257 | — | — | 0 % → 0 % |
| todolist | 105 → 88 | 191 | 122 → 105 | 208 | 0 % → 0 % |
| rssnews | 99 → 67 | 170 | 116 → 86 | 186 | 9.79 % → 8.31 % |

GPU (the sandbox's sway on the Radeon render node):

| App | First frame: Ljós before → after | qml6 | Answers: Ljós before → after | qml6 | Idle CPU before → after |
|---|---:|---:|---:|---:|---:|
| animations | 46 → 50 | 73 | 64 → 67 | 90 | 3.06 % → 4.34 % |
| form | 68 → 64 | 114 | 86 → 81 | 132 | 0 % → 0 % |
| listview | 56 → 47 | 95 | 73 → 64 | 112 | 10.31 % → 8.39 % |
| rectangles | 61 → 51 | 77 | 79 → 68 | 105 | 0 % → 0 % |
| text | 62 → 53 | 95 | 83 → 79 | 115 | 0 % → 0 % |
| calqlatr | 73 → 55 | 118 | 91 → 72 | 136 | 0 % → 0 % |
| samegame | 75 → 49 | 107 | 106 → 81 | 124 | 2.15 % → 1.92 % |
| clocks | 65 → 43 | 109 | 82 → 60 | 134 | 2.93 % → 1.64 % |
| thermostat | 123 → 102 | 155 | — | — | 0 % → 0 % |
| todolist | 102 → 71 | 141 | 119 → 88 | 163 | 0 % → 0 % |
| rssnews | 68 → 45 | 123 | 101 → 75 | 140 | 9.07 % → 8.10 % |

Thermostat's window does not take the compositor's new size in either runtime, so it has no
"answers". The apps that idle above 0 % animate themselves. The static ones stay at 0 %. On the
GPU, animations' two slow runs fell in the rounds where qml6 was slow too; its usual runs are
about 3 ms behind before, which is within its spread.

What was cut, on the main thread's path to the first frame:

- **System fonts on a thread.** The font database (fontconfig, through fontique) is read on a
  thread of its own from the start of the process, and the view waits for it only when it first
  shapes text.
- **The first layout while the drivers start.** The shell lays the scene out at the window's size
  while the graphics thread still starts EGL or the wgpu instance. The first frame then only
  paints. The scene is still polished once.
- **Compiling while the drivers start.** `ljos run` compiles the project on a thread of its own and
  joins it only when the engine needs it, so the compile overlaps the display's and the drivers'
  start. A file that does not compile still fails the same way, with the same message and exit 1.

The bench itself was fixed too. It refused a launch that was not yet inside the limit's cgroup,
when the child had only forked and not yet run `systemd-run`, because the child still had the
bench's own executable. The gate is now told apart by its fifo in the arguments as well. A refused
launch is tried again up to three times, but none was refused in the runs above.

What is left, from per-phase timings on the main thread (quiet machine, release build):

| Phase | llvmpipe | GPU |
|---|---:|---:|
| EGL display start (graphics thread) | 37–45 ms | — |
| Controls types registered | 2–4 ms (15 ms under load) | same |
| Root object made (`create_from_url`) | ~12 ms | same |
| First polish | ~15 ms | same |

On llvmpipe the EGL start is now the critical path. The main thread's work overlaps it. On the GPU,
the critical path is the main thread's own work. Next: register the Controls types only when a
module imports them, and make the root object in less time. On the font side, fontconfig's own
scan inside fontique is the rest.

### photosurface's rows from FolderListModel (2026-10-18)

The folder is a copy of 472 screenshots (PNG, 79 MB) on the data drive. Both apps run in the
`desktop:cua` sandbox (sway, X11, llvmpipe). Ljós is a release build, and Qt 6.11.2 runs with
`QT_QUICK_CONTROLS_STYLE=Basic`. Times are in ms from starting the process, three runs after a
first, cold one.

**The listing alone.** A FolderListModel with `nameFilters: ["*.png"]` and no view: the time from
the root's completion to `status` Ready with 472 rows.

| | Ljós | Qt |
|---|---:|---:|
| Ready, on the sandbox's display | 13–31 | 17–22 |
| Ready, Qt's `offscreen` platform | | 1–2 |

Ljós's own work is about 2 ms. Reading the folder on its thread takes 0.75 ms warm (8 ms cold),
and the rows take 1.1 ms on the user interface thread, most of it making a `Date` for each row. The
reply then waits about 10 ms while the user interface thread makes the window. Each file is
stat'ed once, as in Qt; before, `showOnlyReadable` stat'ed it again.

**The whole app.** photosurface, with the welcome slides taken out of a copy and the folder set at
completion.

| | Ljós | Qt |
|---|---:|---:|
| Window mapped | 398–417 | 117–220 |
| Rows (the Repeater has 472 delegates) | 217–235 | 2201–2356 |
| Settled (no CPU for 1 s) | 2909–3239 | 2490–2706 |
| CPU until settled (s) | 26.6–29.2 | 2.9–3.2 |
| of it llvmpipe (s) | 22.5–24.9 | 0.38 |
| Peak RSS (MiB) | 385–400 | 3370–3390 |

- **Rows are ten times sooner.** Qt decodes each picture on the user interface thread while it
  makes the delegate, so its rows take 2.2 s. Ljós reads a large picture's header and decodes it
  aside (the section above).
- **The window maps later.** Ljós compiles from source before its first frame, as in the sections
  above. Qt maps a window before its rows come.
- **The CPU goes into drawing, not decoding.** The decoders spend about 3.8 s, but Ljós draws 100
  frames between its first frame and settling, one as each batch of pictures arrives. On llvmpipe
  each frame costs about 0.2 s of CPU across its threads. The full-window gneiss `ShaderEffect`,
  which does not change, is drawn again each frame: without it, llvmpipe spends 8 s instead of
  22.5 s.

**DECISIONs.**

- The rows' `Date`s are still made when the rows arrive, through the script engine, as every
  `Date` in Ljós is. That takes about 1 ms for 472 rows.

**What is left.**

- **Drawing as pictures arrive.** At most about 10 frames a second while pictures stream in, and
  a static `ShaderEffect` kept as a texture between frames, would bring Ljós's CPU near Qt's on a
  software renderer. Pacing picture arrivals belongs to the image store (`ljos_quick/src/image`).

### Lag from the frame wait while flicking (2026-10-18)

Two files show a folder of 472 generated JPEGs, each 2000 × 1500 (128 MB together), in a
1024 × 600 window. `surface` is photosurface's surface without its welcome slides: a ScrollView
1.5 times the window, with a Repeater of rotated, scaled frames, each with a DragHandler. `grid` is
a GridView of 128 × 96 cells with `cacheBuffer: 0`, so that scrolling makes new delegates. `ljos
bench` now also reports input-to-frame latency: from each input it sends to the first frame shown
after it, both on the compositor's `CLOCK_MONOTONIC`. The profile is `desktop` with `--repeat 3
--settle 1 --idle 2 --load 8`; each cell is the median of 3 runs. The build is release (lto thin)
on 6678afb: with the 50 ms wait ("wait"), with no later wait ("none"), and with the wait gated as
below ("gated"). Qt is 6.11.2 with `QT_QUICK_CONTROLS_STYLE=Basic`. The machine's load average was
35 to 48, from other work, while they ran.

| surface, `--gesture drag` | wait | none | gated | Qt |
|---|---:|---:|---:|---:|
| Frames shown (per second) | 4.99 | 61.0 | 61.0 | 0.50 |
| Median frame interval (ms) | 32.3 | 16.1 | 16.0 | 16.2 |
| 95th percentile frame interval (ms) | 49.2 | 16.1 | 16.1 | 32.4 |
| Median input-to-frame (ms) | 28.3 | 8.89 | 9.48 | 297 |
| 95th percentile input-to-frame (ms) | 64.1 | 15.4 | 15.4 | 608 |

| surface, `--gesture scroll` | wait | none | gated | Qt |
|---|---:|---:|---:|---:|
| Frames shown (per second) | 30.7 | 37.6 | 39.1 | 45.0 |
| Median frame interval (ms) | 33.8 | 32.3 | 32.2 | 16.3 |
| Median input-to-frame (ms) | 14.5 | 10.8 | 10.7 | 11.7 |
| 95th percentile input-to-frame (ms) | 30.6 | 31.0 | 26.6 | 31.5 |

`grid` showed 60 frames a second with a 16.2 ms median interval and 8–9 ms median latency in
every build, and in Qt: drag and wheel on it showed no difference the wait made.

- **No wait while moving.** A later frame does not wait while the pointer is held or wheeled, a
  finger is down, or the scene's next wake is within a frame (a flick, an animation). In the
  "wait" build, a drag held the frame up to 50 ms whenever a picture came into view.
- **An allowance per second.** Later frames wait no more than 100 ms in all in any second
  (`ImageStore::LATER_WAIT_PER_SECOND`), so a view that keeps starting decodes without input
  (a slideshow, a model filling in) does not stall frame after frame. The first frame still
  waits up to 150 ms. Tests in `ljos_quick/src/image/store.rs`.

**DECISIONs.**
- 100 ms a second: two full later waits, a source set by a click and one more. Qt decodes every
  such picture on the UI thread, so it stalls longer than this.
- A click is not "moving": the frame after a click that sets a source still waits, as Qt shows
  the picture in that frame.

**What is left.**
- **Scrolling `surface` shows 30–39 frames a second**, at 32 ms intervals, where Qt shows 45 at
  16 ms. That is the same with or without the wait, so it is the cost of a frame of 472 rotated
  pictures, not the wait. It was not looked into here.

### What PDF documents cost every application (2026-10-18)

`hayro`, which draws PDF pages, brings `brotli`, a third `skrifa` and `read-fonts` (0.46, 0.43)
and `fearless_simd` 1.1. The two `vello_cpu` versions (0.1, 0.3) come from other crates and stay
without it. Release (lto thin) `ljos` on 8c0e5e1 with and without it:

| | with hayro | without |
|---|---:|---:|
| Stripped binary (MB) | 89.5 | 84.0 |
| Max RSS of `ljos render` of a Label and a Button (MiB, median of 6) | 42.5 | 41.2 |
| Settled anonymous PSS of the same, windowed (MiB, `ljos bench`) | 50.4 | 50.0 |

- **PDF is a feature.** `pdf` on `ljos_quick` and on the `ljos` facade is off by default; the
  `ljos` runner and the feature runner turn it on. Without it a PDF source is Error, with
  "Error decoding: <url>: Unsupported image format". Qt 6.11.2 gives that when run (qml6,
  offscreen) with a plugin directory that lacks `libqpdf.so`. A test in
  `ljos_quick/src/image/decode.rs` covers each build; `features/items/image-pdf.feature` covers
  the runner.

**DECISIONs.**
- Off by default: 6.6 % of the binary for a format few applications show. An application that
  shows PDFs turns it on, as it would install Qt's plugin.
- The windowed bench's peak and cgroup figures were not used: the load average went from 22 to
  84 while it ran, and the runs varied from 58 to 147 MiB.

### Per-frame cost against Qt (2026-10-18)

Callgrind, prof build, GPU sandbox. The figures are main-thread instructions per frame: animations
over 10 s after 45 s, clocks over 10 s after 30 s. Qt's animations frame is 0.72 M.

| Run | animations | its compose | its tick | clocks | its compose |
|---|---|---|---|---|---|
| before | 1.487 M | 108 k | 500 k | 440 k | 22.7 k |
| subtree reuse, axis-aligned bounds, fused bounds | 1.484 M | 100 k | — | 439 k | 18.9 k |
| + a place effect that is told, not rerun | 1.379 M | 100 k | 273 k | 441 k | — |

- **Compose** copies a subtree that did not change since the last snapshot whole. The tree marks
  each change up its ancestors, which is about 16 subtrees a frame on clocks. On animations every
  node moves, so its compose falls only through the cheaper bounds: a box that is only scaled
  and moved is mapped from two corners, not four.
- **Tick.** A moving item reran its place effect, which cost about 1 k per animation. Now
  `Graph::effect_told` tells the scene which items moved, and the scene reads their x and y when
  it lays out. What is left for each animation is the generic write path, about 1.1 k.
- **samegame particles.** The step timer runs only while the system has live particles or an
  emitter that may emit. samegame's paint spots live 30 s, so its steps are real work.
  - Group names are now kept once and compared by pointer. That took the string compares in
    grouping from 375 k to none, and grouping from 71 to 42 a particle.
  - A step no longer gathers a list for each particle under an affector.
  - A particle step stays about 120. Painting the particles costs more than stepping them:
    `image_segs` 94 M against `step_system` 118 M over the run.
- **Encode** is about 127 k a frame on animations, about 634 an item, of which `solid()` takes
  243. A node's display list already reuses its mapped segments while the list, the transform
  and the opacity stay the same. Solid rectangles take their own path, which maps them again
  every frame: that is what is left to cache.

### Rows measured one by one, a smaller node table, and EGL vendors (2026-10-18)

The profile and options of "Rows let go, pictures for rows gone, and shared type tables" above,
Ljós and Qt, one app at a time (`--append`), because under a load of 12–75 the bench refused some
runs whose cgroup held another process. Before is the list change and after is the node table
change, both built on 36f3338; the two binaries differ only in `ljos_reactive`.

| App | Peak, before → after (Qt) | Settled | Anon PSS | PSS |
|---|---:|---:|---:|---:|
| calqlatr | 65.2 → 65.8 (99.8) | 63.4 → 64.3 (100) | 56.6 → 56.4 (90.9) | 114 → 182 (133) |
| clocks | 68.5 → 64.6 (107) | 65.4 → 65.1 (108) | 54.6 → 54.2 (98.2) | 114 → 180 (136) |
| samegame | 81.6 → 74.3 (109) | 69.5 → 67.2 (115) | 56.9 → 56.8 (104) | 112 → 123 (150) |
| thermostat | 114 → 117 (159) | 115 → 113 (166) | 97.0 → 94.4 (141) | 159 → 173 (211) |
| todolist | 74.4 → 72.2 (117) | 74.2 → 73.0 (118) | 65.1 → 64.0 (107) | 123 → 133 (167) |

The rise in PSS is file pages of a binary just built under load, as in that run.

- **Rows measured one by one.** A list now lays out each new row before it makes the next, as Qt
  does, instead of taking a row it has not measured to be as long as the first. Polishes spent on
  this do not count toward the scene's loop limit. With a local feed, rssnews makes rows 0–6, as
  Qt does, and none of them twice; before, it made rows 0–14 and remade rows 1–6.
  `features/views/list-rows-measured.feature`; on Qt and on Ljós.
- **The node table.** In all five apps the largest Ljós-owned item in a dhat profile was the
  reactive graph's node table. It only grows, by doubling. Thermostat held 36 863 nodes in room for
  65 536, 304 bytes each: 19.0 MiB of its 42.3 MiB of Rust heap. 58 % of the nodes have no
  sources and 52 % have no observers; 6–8 % have more than two. Edges are now held
  inline up to two, a property's override layers sit behind one pointer, and the tag only an
  application sets is kept in a side map. A node is 224 bytes. Rust heap went down from 42.3 to
  37.8 MiB in thermostat, 23.5 → 21.2 in todolist, 11.8 → 11.2 in calqlatr, 8.3 → 8.0 in clocks
  and 10.7 → 10.5 in samegame. Anon PSS falls by less, because the part of the doubled table that
  is never written is never resident (live nodes × 80 bytes, about 2.8 MiB in thermostat).
  `a_node_holds_little_inline` (ljos_reactive).
- **EGL vendors.** glvnd's libEGL loads every vendor it is told of. This machine has NVIDIA's
  user-space package and no NVIDIA GPU, so each Ljós process mapped 34 MiB of NVIDIA libraries,
  4.4 MiB of it private. Qt on X11 draws through GLX and maps none of them; on Wayland it goes
  through EGL and maps them too. When NVIDIA's kernel driver is not loaded (`/proc/driver/nvidia`)
  and the user has set neither `__EGL_VENDOR_LIBRARY_FILENAMES` nor `__EGL_VENDOR_LIBRARY_DIRS`,
  the shell names the other vendors in `__EGL_VENDOR_LIBRARY_FILENAMES` before anything loads
  libEGL. clocks in the X11 sandbox: 47 NVIDIA mappings → 0, anon PSS 52.5 → 46.9 MiB. (The
  Vulkan loader of the wgpu painter was left as it was; see "Vulkan drivers left out as EGL's are"
  below.)

**What is left.**

- **The Controls styles.** (Also the next step for startup, above.) Every application decodes the whole embedded style pack at start,
  about 1.8 MiB of IR, and registers every type of every style, which loads each file and builds
  its type table. Plain Quick apps such as clocks and samegame pay for it too. It is not cut here.
  A registered type's identity holds its file, so a file cannot be dropped and loaded again later.
  And the `--example` oracle walks every registered module, so registering lazily would still
  load everything on `ljos run`. Cutting it needs registration by name, resolved when a type is
  first named.

### Vulkan drivers left out as EGL's are (2026-10-18)

The Vulkan loader loads every driver it finds when wgpu makes its instance. Like glvnd's libEGL, it
has no call to leave one out, only its environment. When NVIDIA's kernel driver is not loaded and
the user has set none of `VK_DRIVER_FILES`, `VK_ICD_FILENAMES`, `VK_ADD_DRIVER_FILES`,
`VK_LOADER_DRIVERS_SELECT` and `VK_LOADER_DRIVERS_DISABLE`, the shell now names NVIDIA's manifests
in `VK_LOADER_DRIVERS_DISABLE`, at the same point and for the same reasons as the EGL vendors. That
variable leaves the loader looking wherever it looks. The loader's version is read from the file
`libvulkan.so.1` resolves to (opened and closed again, which loads no driver). A loader older than
1.3.234 does not read that variable, and is given the other drivers' files in `VK_ICD_FILENAMES`
instead. `crates/ljos_shell/src/drivers.rs`.

"Before" is the same binary with the step bypassed: `__EGL_VENDOR_LIBRARY_DIRS` set to glvnd's
own directory and `VK_LOADER_DRIVERS_SELECT=*`, both of which the shell leaves alone. The bench
(`--profile desktop --warm-up`, the off-screen sway on the Radeon render node, a load of 17–42)
took turns, before and after, app by app and painter by painter, for three rounds; each cell is
the median, in ms. The driver start is the graphics thread's own time, timed with a local patch
that was not kept (the last round, on the same sway).

| App | Painter | First frame, before → after | Driver start, before → after |
|---|---|---:|---:|
| clocks | wgpu | 711 → 82 | instance 658 → 41 |
| calqlatr | wgpu | 733 → 80 | — |
| clocks | GL | 175 → 58 | EGL display 146 → 28 |
| calqlatr | GL | 187 → 68 | — |

In the X11 sandbox (`desktop:cua`, llvmpipe), clocks's EGL display started in 22–26 ms before and
13–14 ms after, and the wgpu instance in 181–191 ms before and 22–23 ms after (one run of 212 ms
under load). No NVIDIA library is mapped on either painter after (47 mappings before), and anon PSS
falls from 53.9 to 47.4 MiB (GL) and from 56.4 to 49.5 MiB (wgpu).

### llvmpipe's threads, and what wakes an animated application (2026-10-18)

An application drawing at 60 frames a second in the X11 sandbox woke about 8,000 times a second,
and colorpaletteclient 8,200–16,000 while its busy indicator turned. Counted by thread
(`/proc/<pid>/task/*/status`, voluntary switches), about four in five were llvmpipe's raster
threads: Mesa starts one per core (32 here) and wakes each of them for every frame. Qt's are the
same, so qml6 woke as often. Nothing in Ljós polled: at rest both stayed at 0.

Mesa is told how many threads to start only by `LP_NUM_THREADS`, which hardware drivers do not
read. The bench was run with it unset and at 2, 4 and 8 for both (`--profile desktop --repeat 1`,
a load of 27–64; the Qt column at 8 is left out, its run was disturbed). Wakeups per second while
idle, CPU while driven (% of a core) and frames shown while driven:

| App | Threads | Wakeups, Ljós / Qt | CPU driven, Ljós / Qt | Frames driven, Ljós / Qt |
|---|---|---:|---:|---:|
| animations | 32 | 7974 / 6802 | 17.9 / 23.6 | 59.1 / 59.1 |
| animations | 8 | 2418 / — | 12.8 / — | 59.8 / — |
| animations | 4 | 1348 / 1225 | 9.0 / 13.8 | 59.0 / 58.8 |
| animations | 2 | 787 / 871 | 9.5 / 10.8 | 57.3 / 60.6 |
| samegame | 32 | 5886 / 6800 | 111 / 186 | 56.5 / 60.3 |
| samegame | 8 | 1801 / — | 72.8 / — | 54.1 / — |
| samegame | 4 | 979 / 1340 | 82.4 / 184 | 56.1 / 60.7 |
| samegame | 2 | 666 / 1245 | 72.8 / 133 | 40.6 / 41.8 |

Four threads show the frames 32 do; two lose samegame's while it is clicked. When the user has not
set it and the machine has more than four cores, the shell now sets `LP_NUM_THREADS=4` where it
leaves out NVIDIA's drivers, before anything loads Mesa (`crates/ljos_shell/src/drivers.rs`). A
turning square (`ljos run`, 500×400) then woke 1,572 times a second against qml6's 7,914, at 4 %
of a core against 9 %, and colorpaletteclient's connect at most 506 times a second. What is left
is the UI thread, about 15 wakeups a frame (928 a second), as Qt's render thread (841): the frame
callback, the swap and llvmpipe's fences.

### The binary's size and the file pages it keeps in memory (2026-10-18)

The `ljos` command is the development tool: the runner, the inspector and authoring servers, the
QML front end, the feature catalogue, the test runner, the bench, the formatter and every codec and
document format an `Image` or `MediaPlayer` reads. Shipped applications do not link most of that.
Release (thin LTO) makes it 115 MB, 89.7 MB stripped, of which 60.7 MB is code, 12.7 MB read-only
data and 8.7 MB unwinding tables (`panic = "unwind"`, D11). No single thing is large. `nm` puts the
code at 7.2 MB in `core`, 3.5 MB in `alloc`, 2.6 MB in `ljos_quick`, 2.2 MB in `ljos_engine`,
1.9 MB in naga, and under 1.3 MB in each other crate. 4.3 MB of it is drop glue in 22 500 copies.
The biggest data are symphonia's FFT tables (256 kB) and a few codec and TLS tables. The style
pack, the dialogs, the type tables and the two fonts are each well under a megabyte. Each feature
the command turns on is something it does, so none was turned off.

What the pages cost is shown by `smaps` for `ljos run` in the X11 sandbox (clocks, 5 s after
start). The binary has 27.5 MiB of its code and 8.5 MiB of its read-only data in memory, all clean
file pages. Its 2.5 MiB of relocated pointers are private and count as anonymous. Qt's libraries
keep a similar share of themselves in memory (libQt6Quick 2.7 of about 6.5 MiB): this kernel maps
file pages in large folios, and transparent huge pages are `always`. So on this machine a
binary's file PSS follows its size.

`[profile.dist]` builds it as one LLVM module (`lto = "fat"`, `codegen-units = 1`). Stripped, the
binary is 75.9 MB (−15 %): 53.2 MB of code, 11.7 MB of read-only data and 5.5 MB of unwinding
tables. The build takes 884 s, against a few minutes to relink release, so release, which the
bench and the gallery rebuild, stays thin. Build what is handed out with
`cargo build --profile dist -p ljos_cli`.

The bench ran `--profile desktop --warm-up --repeat 3 --settle 1 --idle 2 --load 0` with `--ljos`
set to each binary in turn. It used the off-screen sway on the Radeon render node, under a load of
10–17 from another lane's runs, and each cell is a median. The Qt column is from the same day's
run, with the same flags.

| App | Settled PSS, release → dist (MiB) | Anon PSS, release → dist | First frame, release → dist (ms) | Qt PSS / anon |
|---|---:|---:|---:|---:|
| calqlatr | 119 → 104 | 51.8 → 51.2 | 65 → 65 | 143 / 93.0 |
| clocks | 124 → 107 | 48.9 → 48.2 | 57 → 55 | 147 / 101 |

In the X11 sandbox, run in pairs for two rounds, the binary's own PSS fell from 53–56 to 41–42 MiB
and the process's PSS from 110–117 to 101–108 MiB. The release figures are higher here than in the
table's first runs that day (113 and 114 MiB): how much of a file the kernel maps depends on what
the page cache holds, so only paired runs are compared.

### LocalStorage: a commit that does not wait for the disk (2026-10-18)

todolist's `createTasksLists` held the UI thread for 3.2 s at startup, and adding one task for
52 ms to 5.75 s.

**Method**

- **Script.** A QML file makes the calls todolist's `Database.qml` makes and times them with
  `Date.now()`:
  - opening the database and `CREATE TABLE IF NOT EXISTS`;
  - n transactions of an `INSERT` and a `SELECT … LIMIT 1`;
  - one `SELECT` of every row;
  - n one-row `UPDATE`s;
  - one `DELETE`.
- **Runs.** Both runtimes ran it in the `desktop:cua` sandbox, one after the other: `ljos run`,
  and `qml6` with `QT_QPA_PLATFORM=offscreen`. Their databases sat in the same directory on the
  same drive.
- **Syncs.** An `LD_PRELOAD` shim counted and timed `fsync` and `fdatasync` per file.
- **Builds.** Ljós at c5f25ce3 (before) and the commit after it (after); Qt 6.11.2.

**Where the time went**

- **Not in Ljós.** In memory, one transaction costs Ljós 15 µs, statements prepared anew
  included. The connection was already kept open per database.
- **Waits for the disk.** Both runtimes left SQLite as it comes: `journal_mode=delete`,
  `synchronous=FULL`. Every commit then waits for the disk four times: the journal twice, its
  directory, the database file.
- **A slower sync than Qt's.** Ljós's bundled SQLite uses `fsync`; the system SQLite Qt links
  uses `fdatasync`. For 61 commits:

  | | syncs | time in them |
  |---|---:|---:|
  | Ljós | 244 (`fsync`) | 42–48 ms |
  | qml6 | 244 (`fdatasync`) | 33 ms |

- **The seconds.** They came from a busy drive. While builds wrote to it, both runtimes stalled:
  - Ljós: 4.4 s to open, one insert taking 8.8 s;
  - qml6: one insert taking 4.6 s, 300 inserts 12.7 s.

**Change**

On disk, Ljós now keeps a write-ahead log (`journal_mode=WAL`) with `synchronous=NORMAL`, as
Android keeps its databases:

- **Syncs.** A commit waits for no disk; the log is synced when it is copied into the database,
  at most every 1000 pages and when the database closes.
- **What the app sees is unchanged.**
  - A committed transaction is read by every later one, in this process and in others.
  - It is read by the next run, even after the application crashes.
  - Only a crash of the whole machine can take the last commits back, and it never damages the
    database.
- **Foreign keys.** These are off, as in Qt. rusqlite's build turned them on.

**Results** (idle drive, three runs each; transactions in ms)

| n = 30 | inserts | updates | syncs |
|---|---:|---:|---:|
| Ljós before | 25–28 | 21–22 | 244 |
| Ljós after | 1–8 | 0 | 4 (8 the first time, when the file becomes a log) |
| qml6 | 33–50 | 31–42 | 244 |

| n = 300 | inserts | updates | syncs | time in syncs |
|---|---:|---:|---:|---:|
| Ljós after | 14 | 3 | 4 | 4–5 ms |
| qml6 | 187–209 | 173–316 | 2404 | 319–455 ms |

- **todolist.** The first start after the change took 3.6 s in `createTasksLists`, on a busy
  drive: the file became a log there. Later starts stayed under the 4 ms script budget and logged
  nothing.

**DECISIONs.**
- **WAL with `NORMAL`, not `FULL`.** `FULL` would sync the log on every commit, which is the
  wait this removes. A QML app cannot tell `NORMAL` from `FULL` without pulling the power.
- **`PRAGMA journal_mode` reads `wal`.** This is a `@qt-diverges` scenario in
  `features/platform/local-storage.feature`.
- **No cache of prepared statements.** At 15 µs a transaction, it would not show, and rusqlite
  would need its `cache` feature.

### photosurface's backdrop and pictures arriving (2026-10-18)

photosurface on a folder of 472 pictures, in the desktop:cua sandbox (sway, llvmpipe), release
`ljos`, for 30 s from start. CPU is the process's user and system time; "last busy" is the last
second with more than 0.1 s of CPU. qml6 was given a `gneiss.frag.qsb` built with Qt's `qsb`
from the corpus's shader (the corpus has none, so without it qml6 draws no backdrop at all).

| | Total CPU (s) | Last busy (s) |
|---|---:|---:|
| qml6 6.11.2, with the backdrop | 67.1 | 23.8 |
| qml6 6.11.2, no backdrop (no .qsb) | 4.0 | 23.4 |
| Ljós before | 38.8 | 25.1 |
| Ljós, effect kept as a picture | 14.4 | 21.9 |
| Ljós, kept, and arrivals 50 ms apart | 12.8–13.8 | 22.9–24.1 |
| Ljós, kept, and arrivals 100 ms apart | 12.9–13.0 | about 25 |

- **The backdrop was the cost.** Its program takes about 150 ms of llvmpipe for the window, and
  it ran again for every frame anything over it changed. Kept as a picture after its second
  unchanged frame, each slide costs about 0.2 s of llvmpipe instead of about 4 s. qml6 spends
  11–12 s of llvmpipe at each slide change. The screen is the same pixel for pixel outside the
  photo pile, which is random in each run.
- **Arrivals.** Decoded pictures now wait up to 50 ms for their frame, so the arrival drew 46
  frames instead of 96. Arrivals while the window is hidden or the pictures are off-screen
  already drew nothing: about 220 passes were "unchanged and not presented".
- **What remains is overdraw.** 472 photos pile about 24 deep in one window, and buffer-age
  unions widen the damaged area; llvmpipe holds about 3 cores while the pool decodes on 1 for
  about 2.4 s. Qt decodes each Image on the spot and draws the pile once.

**DECISIONs.**
- 50 ms: 100 ms saved no CPU and showed the last picture later.
- No feature scenario: frames presented are not visible from QML and no step counts them;
  Rust tests in `ljos_shell` (`gl_shader`) and `ljos_egui` (`tests/image.rs`) cover both.

**What is left.** Not drawing what opaque pictures above hide (Qt's opaque pass does this).

### Why the wheel scrolls the surface at 32 ms a frame (2026-10-18)

`ljos bench` surface, wheel gesture, in the desktop:cua sandbox (sway, llvmpipe, 60 Hz), release
`ljos`, with a temporary per-pass trace. The host's load was 40–108, so all times are noisy.

| | Median drawn interval | Frames drawn in 8.5 s |
|---|---:|---:|
| Before | 34 ms | 212 |
| Passes not drawn wait for the scene | about 24 ms | 264 |
| and drawn ones a frame after they began | about 24 ms | about 264 |

- **The scheduler.** A notch turned while a frame is presented runs two passes that draw
  nothing, because the glide starts where the content is. The second was held a whole frame
  even though the scene asked for its next one 7–8 ms later, so that frame missed a refresh.
- **The phase.** Drawn passes were held a frame after a due time chained from earlier ones. That
  time drifted, so passes started late in the refresh. They are now held a frame after they began.
- **The frame.** After a drawn frame winit holds RedrawRequested about 16 ms, until sway's frame
  callback. The frame then costs 13–14 ms: the pass 1, building 0.4, GL calls 4.4, and
  llvmpipe's raster at present 7–8 ms. Everything moves, so each frame repaints the whole
  window. Together with sway's own render, that is just over a refresh, and every other one is
  missed. The bench's composited median stays 32.2 ms. The grid example, which costs little to
  draw, runs at 16.2 ms in the same sandbox.

**DECISION.** No feature scenario: frame times are not visible from QML. The Rust tests in
`ljos_shell` (`desktop`) cover the scheduling.

**What is left.** A cheaper frame: scrolling is not partial damage, so the gain is in the raster.
A 441 ms pass about 8.4 s into one run is not explained yet.

### Solid rectangles, the write path and particle quads (2026-10-18)

Callgrind, prof build, GPU sandbox, main-thread instructions per frame as in the per-frame section
above; "all threads" is the perf counter of every thread over 10 s (I(14 s) − I(4 s)), one run,
with Qt's `qml6` on the same sandbox. "Before" is 12ace64e.

| Main thread, a frame | Before | After |
|---|---:|---:|
| animations | 1.379 M | 1.322 M |
| clocks | 441 k | 438 k |
| samegame: a picture's quads and their encoding | ≥ 625 | ≤ 410 |
| samegame: mapping meshes to the screen, over the run | 40.2 M | 14.8 M |

| All threads, M/s | Ljós before | Ljós after | Qt |
|---|---:|---:|---:|
| animations, GPU | 73 | 71 | 43 |
| clocks, GPU | 21–29 | 22–29 | 14 |
| samegame, idle board, GPU | 17 | 16 | 16 |
| thermostat, todolist, GPU | 0 | 0 | 0 |
| animations, llvmpipe | 474 | 478 | 1 036–1 076 |
| clocks, llvmpipe | 150 | 175 | 939–945 |
| samegame, llvmpipe | 246 | 246 | 1 584–1 600 |

At 60 frames a second the GPU animations frame is about 1.2 M on all threads against Qt's 0.72 M.
On llvmpipe Ljós draws only what changed and Qt redraws the window, so Ljós takes less than half
of Qt. Clocks moves by a third between runs of one binary, with where its one-second tick falls.

- **Solid rectangles.** A solid's encoded output is already one `RectShape`; what each frame paid
  for was asking again whether it is a pixel-exact opaque quad. An upright transform now maps two
  corners instead of four, opacity 1 skips re-multiplying the colour, and `near_whole` rounds by
  adding 1.5 × 2²³. `solid()`'s closure, 30 k a frame, is gone from the top of the profile, and
  the shell's `fill_rects`, which asks the same, fell from 40 k to 34 k a frame.
- **Told moves** set the scene entry's place directly. Before, they went through the shared place
  map only to be drained again.
- **The write path.** An animated `x` write is about 1.1 k, with no one part above 360: the
  graph's write and marks 360, the native store 240 (finding the `Item` in the native chain 74,
  the setter 82), the timeline 157, the write's checks 107. No signal arguments are built and no
  value is boxed on it. `from_value` cloned the `Value` before matching it; plain-data types now
  match it in place.
- **Particle quads.** Pictures of one image were already one mesh. Each picture took the mesh out
  of its segment and put it back, pushed white corners and coloured them after, and grew the mesh
  by doubling. Quads now go into the mesh where it lies, already coloured. A new segment is sized
  for the run of pictures of that image that follows, and an upright node's meshes are mapped to
  the screen per axis.

**What is left.** The animations frame is still 0.6 M main-thread instructions above Qt's. The
largest parts are the tick (about 1.1 k an animation, spread over the generic write), the egui
painter (`GlPainter::paint` about 200 k, the swap about 84 k) and the reads of told places (about
170 each). Cutting the write further needs typed write handles that keep the native field's place
instead of finding it per write.

### Startup off the main thread, library types described on first use, and memory (2026-10-18)

Desktop profile, `--settle 1 --idle 10`, on llvmpipe and on the GPU (radeon, `VK_DRIVER_FILES`), in
the headless sway sandbox. Before is the build of "Startup: the first frame, and the first that
answers" above. Each round runs before, after and qml6 one after the other, over all 11 apps, so
that load from other work on the machine falls on all three. The load average was 5–125 over the
runs, so the tables give each binary's best round: the round least disturbed. Memory is the median.

**First frame and first answer, ms (best of 3 rounds on llvmpipe, of 2 on the GPU).** After is
this section's work. On llvmpipe it is measured without the rebase; on the GPU it is rebased on main.

| App | llvmpipe: before → after | qml6 | GPU: before → after | qml6 |
|---|---:|---:|---:|---:|
| animations | 66 / 83 → 60 / 77 | 111 / 128 | 47 / 64 → 41 / 64 | 68 / 85 |
| form | 78 / 95 → 62 / 78 | 162 / 179 | 67 / 85 → 48 / 65 | 113 / 130 |
| listview | 69 / 86 → 65 / 82 | 134 / 150 | 56 / 73 → 34 / 51 | 88 / 106 |
| rectangles | 73 / 90 → 68 / 85 | 119 / 143 | 63 / 84 → 52 / 70 | 71 / 88 |
| text | 76 / 93 → 65 / 82 | 143 / 176 | 67 / 84 → 60 / 94 | 92 / 111 |
| calqlatr | 87 / 104 → 59 / 76 | 156 / 173 | 82 / 100 → 56 / 74 | 123 / 140 |
| samegame | 85 / 102 → 63 / 80 | 146 / 163 | 87 / 105 → 51 / 84 | 104 / 121 |
| clocks | 79 / 97 → 61 / 78 | 145 / 162 | 67 / 84 → 45 / 63 | 93 / 110 |
| thermostat | 172 → 111 | 221 | 128 → 88 | 206 |
| todolist | 98 / 114 → 76 / 92 | 183 / 199 | 92 / 109 → 67 / 84 | 164 / 181 |
| rssnews | 84 / 101 → 63 / 80 | 170 / 187 | 73 / 90 → 55 / 89 | 114 / 131 |

Thermostat's first answer was not reached within the bench's window on any runtime.

**Idle CPU.** The animated apps idled at 3.06 % → 4.34 % on the GPU in one run after the first of
these changes. Interleaved, the medians are animations 4.78 → 2.54 %, samegame 3.47 → 1.70 %,
clocks 4.33 → 4.46 %, listview 10.98 → 9.73 % and rssnews 2.42 → 2.28 %; on llvmpipe they are the
same or lower. The rise was load. The static apps stay at 0.00 %. Every thread started for startup
ends before the window is up: `ljos-fonts`, `ljos-compile`, and the two new ones, `ljos-fontrules`
(scoped to the font engine's build) and `ljos-scripts` (ends when its scripts are compiled).

**What changed on the main thread** (callgrind, rectangles, instructions):

- **Library QML types are described on first use.** Importing `QtQuick.Controls` registered every
  type of every style, which loaded each file and built its type table. Registration now records
  the file's key; a lookup describes the type the first time it is named. A library file that does
  not load is warned about at that first use, not at import. `type_dump` and the `--example`
  oracle describe everything first, as they did. The style pack's IR is decoded per file when the
  file is first read. Registration went from 26.4 M to 6.7 M instructions. The note about this in
  "What is left" in the section before is done.
- **`create_from_url`: 67.1 M → 45.4 M.** The biggest piece was QuickJS parsing the scripts every
  engine runs first (the global prelude and the module scripts). They are compiled once per
  process, on a thread started with the engine, and each engine reads the bytecode; a script whose
  text differs is parsed as before. The JS host's start went from about 3.2 to 1.5 ms.
- **First polish.** The biggest piece was fontconfig's set-up, paid at the first substitution of
  "Sans Serif". The rules are now loaded on a thread beside the font collection, and the first
  substitution went from 16.6 to about 1.5 ms.

**Memory against qml6 (MiB, median, GPU).** After is rebased on main, which brings the node table
and the EGL vendors of the section before. The filters (below) are measured on their own on four
apps; their row is the last column.

| App | PSS: before → after (qml6) | Anon PSS: before → after (qml6) | After 10 s idle, anon PSS | Anon PSS with the filters |
|---|---:|---:|---:|---:|
| animations | 98.4 → 76.0 (83.7) | 42.2 → 30.3 (37.1) | 30.4 (37.1) | |
| form | 114.5 → 85.1 (107.5) | 51.1 → 37.5 (50.9) | 37.5 (50.9) | 37.3 |
| listview | 101.4 → 75.6 (91.3) | 42.1 → 29.9 (43.4) | 31.1 (43.7) | |
| rectangles | 118.4 → 94.0 (86.9) | 60.9 → 47.2 (42.9) | 47.2 (42.9) | 46.1 |
| text | 102.4 → 78.4 (93.3) | 44.3 → 31.6 (46.3) | 31.6 (46.3) | |
| calqlatr | 110.2 → 83.4 (111.6) | 45.7 → 33.4 (49.7) | 33.4 (49.7) | |
| samegame | 107.0 → 78.5 (102.3) | 46.2 → 33.6 (48.6) | 35.0 (49.3) | |
| clocks | 101.3 → 76.7 (103.0) | 42.5 → 29.9 (45.9) | 29.9 (46.0) | |
| thermostat | 124.4 → 105.1 (112.6) | 70.8 → 56.1 (62.1) | 56.4 (62.1) | 55.5 |
| todolist | 113.0 → 94.6 (120.2) | 55.4 → 42.2 (55.0) | 42.2 (55.0) | 41.8 |
| rssnews | 130.5 → 99.5 (173.4) | 68.6 → 49.6 (120.4) | 38.0 (186.9) | |

On llvmpipe Ljós is below qml6 in every app, in PSS and in anon PSS (rectangles 136 against 162 and
72.0 against 100.1). On the GPU, rectangles is the one app above qml6.

- **Startup threads move memory, they do not add it.** The fontconfig state and the compiled
  scripts are made on their threads, so they live in those threads' malloc arenas: four more
  arenas, 3.9 MiB, against 6.7 MiB less in the main heap (smaps, rectangles). The trim after start
  (`give_back_freed_heap`) already runs; what the arenas hold is live.
- **File-backed PSS** varies by 30 MiB between runs of one binary, with how many other processes
  map the same libraries; RSS of the same binaries agrees within 1 MiB. Read the anon column.
- **The biggest item left on rectangles is still the node table**: 26 nodes per Rectangle. Ten of
  them are for properties the file never sets. Every item watched `anchors.margins` for its first
  value, and every rectangle watched `gradient` to refuse what is no gradient. Each watch is a node
  and an effect. Both are now write filters, which see the same writes (bindings' included) and
  need no node. 1000 rectangles: 26 044 nodes → 22 038.

**What is left.**

- **Rectangles on the GPU**, 3.2 MiB anon over qml6. Its nodes are now 22 per Rectangle. Of those
  left for properties it never sets, the `parent` and `focus` watchers cannot become filters: what
  they run writes the same property back, which a filter refuses. `childrenRect` is a binding made
  for every item so that it costs nothing until read; its node is made all the same.
- **Startup**: the fallback for a glyph no font in the family has ("⅟x" in calqlatr) runs
  fontconfig's sort, 4–7 ms. The QML module files are still parsed when the engine starts, not
  compiled ahead.

### Ljós's `dist` binary against Qt, at equal window sizes (2026-10-18)

`BENCH_PROFILE=dist BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench calqlatr clocks samegame
todolist rssnews --profile desktop --settle 1 --idle 3 --load 5`: the binary that is handed out
(lto fat, one codegen unit) against `qml6` 6.11.2 with the Basic style, both Wayland clients of
the sandbox's headless sway, medians of 3 runs, load average 3.2–6.1. Ljós at 009ed006, todolist's
driven rows again at 77d6a7af.

| Ljós/Qt, geometric mean | Before (42c1ac11) | After |
|---|---:|---:|
| first frame | ×0.29 | ×0.23 |
| CPU to first frame | ×0.59 | ×0.56 |
| first frame that answers input | | ×0.28 |
| peak memory while starting | | ×0.57 |
| settled memory (cgroup) | | ×0.50 |
| settled private PSS | | ×0.46 |
| settled PSS | ×0.59 | ×0.60 |
| idle CPU | | ×0.55 |
| CPU while driven | ×0.60 | ×0.38 |

| Driven | Ljós | Qt |
|---|---:|---:|
| samegame CPU, before (800×600 against 320×480) | 49.9 % | 29.7 % |
| samegame CPU, after (both 320×480) | 105 % | 249 % |
| samegame frames a second, before / after | 38.1 / 57.3 | 59.9 / 62.1 |
| todolist median input-to-frame latency, before / after | 1 507 / 231 ms | 149 / 220 ms |
| todolist 95th percentile latency, before / after | 2 468 / 472 ms | 410 / 467 ms |

- **Equal windows.** Until this run Ljós was given the gallery's capture size and Qt took the
  root's own, so clocks, rssnews and samegame drew bigger windows on Ljós; samegame's board was
  800×600 against 320×480, which made it look 1.7 times Qt's CPU when it is 0.42 times. Both now
  open at the root's size. Stray samegame processes from a diagnosis ran during one of the
  intermediate runs, not this one.
- **todolist** waited 1.5 s per click. The sandbox's seat has a pointer and no keyboard, and winit
  hears of focus only from the keyboard, so the window was never active and a press could not
  give the field the focus. A press now activates a window until a keyboard is heard of. Qt still
  shows more frames (7.2 a second against 2.3) for the same latency; Ljós draws only the blink.

**What is left.** samegame's slowest idle frame interval (286 ms against 16 ms) and the 95th
percentile frame interval while driven in calqlatr and rssnews (48 and 32 ms against 16 ms). rssnews
idles at 1.45 times Qt's CPU.

### Script-side gaps to qml6: samegame, calqlatr and maroon (2026-10-18)

Copies of the three demos with bench functions added, so that one script runs on qml6 (offscreen)
and on Ljós (`frame_profile --start`). Wall time is the fastest of 5 runs, alternating the
builds. Instruction counts (callgrind, release) are the reliable figure; wall time moved by
about 7 % over the session. The base build is 12ace64e.

**samegame**, at 320x480 on both, because the board's block count follows the canvas size.
A run at another size than qml6's is what made the gap look like 21.3 against 7.2–7.8 ms.

| 40 games | qml6 | Ljós before | Ljós after |
|---|---:|---:|---:|
| `startNewGame` | 8.0 ms each | 287 ms (7.2 each) | 279 ms (7.0 each) |
| the frame that destroys the old boards | | 290 ms | 190 ms |
| new game and 20 clicks | 12.8 ms each | 1 050 ms (26 each) | 359 ms (9.0 each) |

A new game, its destroy frame included, comes to about 11.7 ms in Ljós against 11.2 in qml6.
Clicks alone are about 0.085 ms each, against 0.24 in qml6. Destroyed objects now leave their
holders' lists in one pass per frame (51278e60): `forget_in_lists` had been looking up each
holder for each object. A pulse or burst of a particle system no longer asks its other
emitters (2caf3a91).
Evaluations in the destroy frame fell from 122 000 to 390. Re-measured at the end on the slower
machine, base → head: 308 → 300 ms for `startNewGame`, 1 060 → 385 ms for game and clicks.

**calqlatr**, one press = `benchPressOnce`, 14 presses, ×200. Ir is 20 runs of it in callgrind
less a run with none.

| Build | Ir for 280 presses | Wall, ×200 | Per press |
|---|---:|---:|---:|
| base | 779 M | 753 ms | 0.269 ms |
| + a QML function a script calls runs as a script function | 747 M | | |
| + also when it passes a script file | 700 M | | |
| + `Qt.lighter`/`Qt.darker` of a colour in tier 1 | 587 M | 487 ms | 0.174 ms |
| qml6 | | | 0.111 ms |

Before, every `isButtonDisabled` and `getTextColor` call left the script, went through
`Engine::invoke`, and came back in: 8 711 and 3 070 calls per run. A QML function that a script
calls is now a script function bound to its object (`Scope::direct_method`). Its parameters are
read as the call starts, and it throws and returns what the function does, −0 included as in
Qt. It is not used while a reload is tracked, before the object completes, for a body that makes
objects, or for a script file that imports another. `getTextColor`'s `Qt.darker` kept its binding
in the script; it now runs in the binding VM.

What is left of a press: writes and the graph work they set off (≈ 320 M, `Graph::batch` flushes
and recomputes), `try_read` 71 M, starting Behavior animations 42 M, and ListModel
append/clear and `positionViewAtEnd` (≈ 85 M, native model and view work).

**maroon**, three towers and a mob made with `createObject`, then three more and another mob.
Main thread only. The PNG decodes of the first set (≈ 16 M Ir) run on the image pool.

| | qml6 | Ljós before | Ljós after |
|---|---:|---:|---:|
| start (`startGame`), Ir | | 261.5 M | 253.0 M |
| first set, Ir | | 8.33 M | 5.03 M |
| second set, Ir | | 4.00 M | 3.90 M |
| first set, wall | 9–13 ms | 1.7 ms | 1.3 ms |
| second set, wall | 1–3 ms | 0.6 ms | 0.6 ms |

The spike was a type's first object building its prototype: an accessor for every member of the
type, inherited ones included. That was about 72 per type, and 1 645 for maroon's 23 types. A
prototype now inherits its base's, and only what the type adds or overrides is built.
The first object's extra cost fell from 4.3 M to 1.1 M Ir. Compiling the first function of a
file is most of what is left. Tested by
`a_type_s_first_object_builds_only_what_its_type_adds` (ljos_engine). As a side effect,
`for…in` over an object lists a type's own members before its base's. Qt lists them base first.
