# Benchmarks: Ljós and Qt side by side

`mise run bench` measures the same QML applications on Ljós (`ljos run`, release or `dist` build)
and on Qt (`qml6`). Each application runs inside a limit on memory and CPU. Every number is read
from outside the application:

- **Memory and CPU:** the cgroup's own accounting and `/proc`.
- **First frame:** when the compositor maps the window.
- **Frames:** counted with the compositor's screencopy, by whether their pixels changed.

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
the profile. The `dist` link lays out first the functions named in `crates/ljos_cli/link/hot.txt`,
the ones the bench apps run; `mise run bench:hot [ids…]` records them again under callgrind (about
15 minutes), and should after a toolchain or dependency update, since the names carry the crates'
hashes. A stale list only orders less.
Apps draw on the headless sway of `scripts/cua-sandbox.sh` as Wayland clients, never on the user's
display:

- Ljós draws with egui's GL painter on EGL (llvmpipe on a headless compositor); `LJOS_PAINTER=wgpu` measures wgpu's GL backend instead.
- Qt uses its Wayland platform.
- Both run with `QT_QUICK_CONTROLS_STYLE=Basic`, so both draw the same controls.

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
| CPU by thread (`thread_cpu_ms` in report.json) | each window's CPU split by the name of the threads alive at its end, threads of one name added up: whether a runtime's work is on its main thread, a Wayland or clipboard thread, or the rasteriser |
| Frames while idle | screencopy frames over the idle window whose pixels differ from the one before: non-zero only for apps that animate. A runtime that sends the compositor a frame that shows what the last one did (Qt does, in an animation that moves nothing on screen) is not counted for it; those are reported apart, as frames sent. Counts before 2026-10-18 include them |
| Frame interval while idle | the 95th percentile time between those frames, and the slowest. A pause in an animation is a long interval in either runtime |
| Wait before driving | the pointer is put where the gesture starts, then the bench waits until the window's pixels have not changed for 300 ms (at most 5 s), so that the input starts on a settled window in every runtime: what the pointer's entry starts, and a startup animation, are over. Not counted in the load. An animation that never pauses that long is driven at 5 s |
| CPU and frames while driven | over `--load` seconds (10) of `--gesture` input (wlr virtual pointer): `click` (default) clicks, evenly over the span, the controls `mise run gallery:drive` clicked in that example, at the same places in every runtime (its `clicks.json`, fractions of the window, used when it found three or more), or else a 5 × 5 grid over the window; `drag` holds the button in its middle and moves up and down a third of its height once a second, `scroll` turns the wheel ten notches down and ten up |
| Frame interval while driven | the median, 95th percentile and slowest time between the frames that changed then |
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

### Number slots, whole frames on a device and shared image pages (2026-10-18)

Callgrind, prof build, GPU sandbox, main-thread instructions per frame. "All threads" is now
counted per thread in one process: a 10 s window after 4 s of warm-up (`ljos-scratch` `thr.py`).
Qt's `qml6` runs in the same sandbox. "Before" is c914a854.

| Main thread, a frame | Before | Number slots | Whole on a device | Shared pages |
|---|---:|---:|---:|---:|
| animations | 1.312 M | 1.288 M | 1.176 M | 1.172 M |
| clocks | 445 k | | 441 k | 396 k |

| All threads, M/s | Ljós before | Ljós after | Qt |
|---|---:|---:|---:|
| animations, GPU | 69.8 | 63.2 | 43.6 |
| clocks, GPU | 23.1 | 20.3 | 14.8 |
| samegame, GPU | 14.9 | 14.7 | 15.8 |
| thermostat, todolist, GPU | 0 | 0 | 0 |
| animations, llvmpipe | 472 | 454 | 1 041 |
| clocks, llvmpipe | 163 | 153 | 924 |
| samegame, llvmpipe | 239 | 241 | 1 592 |

At 60 fps the GPU animations frame is now 1.05 M on all threads, against Qt's 0.73 M.

- **Where the other threads are.** In Ljós, painting, uploading, tessellating and handing frames to
  the compositor all run on the main thread. The only other threads are egui-winit's clipboard
  (0.5 M/s) and Mesa's shader compiler (0.15 M/s). Qt spends 25 M/s on its GUI thread, 15 M/s on
  its render thread and 3 M/s on driver and Wayland threads.
- **Number slots.** An animation resolves its target's number field once and then writes straight
  through it, with no property lookup, `Value` or generic setter. Notify and change semantics are
  unchanged. A write is now about 590 instructions, 384 of them the graph's write, which is mostly
  marking the observers.
- **Whole frames on a device.** Drawing damage in parts cuts each shape crossing a part, clears
  each part and hands the compositor a list of rectangles. On a GPU that costs the CPU more than
  the pixels it spares: a damaged swap cost 166 k against 99 k for a whole one, plus 103 k of
  clipping. A frame whose damage covers half the window or more is now drawn whole. Software
  rasterisers keep the ¾ threshold.
- **Shared image pages.** Each texture a frame samples was a separate draw, and the driver
  re-checks its state for every draw at the swap. Clocks drew each dial, hand and centre
  separately, and the swap was 38 % of its frame. Clamped pictures up to 256 px a side now share
  512² pages, so the swap fell from 172 k to 132 k per frame.
- **Clocks' swing.** Measured in one process, clocks holds 22–25 M/s in every 2 s window, and five
  runs spread over 22.7–23.7 M/s. The third came from the old method, which subtracts the
  start-ups of two processes; those moved by 8 M with machine load (load average 17–39).
- **Callgrind and the hardware counter.** Callgrind counts each `rep stos` step, so radeonsi's
  memset in the swap shows as about 65 k per frame. The hardware counter does not see it, so it
  reads about 55 k per frame lower than callgrind × fps.

**What is left.** In clocks, the timers are 59 k per frame: 710 JS `timeChanged` calls at 47 k
each. Typed write handles for bindings that write a native property are not done yet.

### Startup and footprint against qml6: the binary, the glyph fallback and the module files (2026-10-18)

**The binary.** Sizes are of the stripped `ljos` command, in bytes as `stat` gives them. "Before" is
the tree at the start of this work, built the same way into a target directory of its own.

| Build | Before | After |
|---|---:|---:|
| release (thin LTO), stripped | 89 356 648 | 83 390 832 (−6.7 %) |
| release, unstripped | 115 760 032 | 110 529 328 |
| dist (fat LTO, one codegen unit), stripped | 75 994 760 | 70 674 896 (−7.0 %) |

The release "after" figures were measured with packed relocations. Only `dist` packs them now (see
below), so a release build is about 2.8 MB larger than the table says.

What is in it (release, rust-lld's link map summed by the crate each input section came from).
Code is 60.8 MB. The largest crates are `ljos_quick` 4.2 MB, the `ljos` binary 3.8, `ljos_bdd` 2.8,
`ljos_engine` 2.3, `ljos_controls` 2.2, naga 1.9, SQLite 1.8 and wgpu-core 1.4; drop glue is 2.1 MB
in 9 600 copies. Read-only data is 12.3 MB: ICU's datetime and collation data (3.4 MB together),
egui's own fonts (1.4 MB), ljos_text's two Liberation fonts (0.8 MB) and the style pack (0.5 MB).
The rest is relocations (3.3 MB), unwinding tables (`.eh_frame` 6.3 MB and `.gcc_except_table`
2.4 MB) and relocated pointers (`.data.rel.ro` 2.5 MB).

What was cut:

- **Packed relocations** (`-z pack-relative-relocs`, from `ljos_cli`'s build script, for Linux and
  glibc): 140 000 relative relocations of 24 bytes each became a 44 kB bitmap, −2.8 MB. The binary
  then needs glibc 2.36, and the loader says so by version, so only the `dist` profile packs them;
  debug and release builds still run on older systems. This keeps ASLR, which a non-PIE link would
  have given up.
- **egui without its own fonts** (`default-features = false`): −1.4 MB. egui draws only the dev
  overlay and the inspector's text, and the shell now gives it ljos_text's Liberation Sans for both
  of its families. The overlay's monospace text is Liberation Sans as well.
- **The developer tools at `opt-level = "s"`** (`[profile.release.package]`): the feature catalogue
  and its runner (`ljos_bdd`, cucumber, gherkin), the authoring server (`ljos_mcp_author`, rmcp,
  schemars), the bench, the formatter and the QtTest runner, −1.7 MB together. `[profile.dist]`
  inherits these overrides: its code fell by 1.6 MB.
- **Generic code moved out of each copy**: `Graph::untrack` was made 583 times (244 kB) and
  `Engine::extension` 95 times (120 kB). The frame push and pop, and the lookup by `TypeId`, are
  each one function now: about −150 kB, across `ljos_quick`, `ljos_controls` and `ljos_engine`.

Kept, each as a DECISION:

- **The duplicate dependency versions.** vello_cpu 0.1/0.3, skrifa, read-fonts, tiny-skia 0.11/0.12,
  smithay-client-toolkit 0.19/0.20 and fearless_simd ×3 come from egui, hayro (PDF) and winit's
  client-side decorations against one another. PDF is a feature, and the decorations are what GNOME
  on Wayland draws with. None of these is in our hands.
- **Uncompressed fonts and tables.** Compressing them would turn clean file pages, which the kernel
  shares and drops, into anonymous heap, and would add decompression to startup.
- **ICU's baked data** (3.5 MB). Qt loads libicu, 39 MB of files.
- **The unwinding tables.** `panic = "unwind"` is D11: the supervisor recovers from panics.

**Against qml6's footprint** (calqlatr, GPU sandbox, `smaps` 6 s after start). qml6 maps 47 Qt files
(the libQt6* libraries and the qt6 plugins, 57.8 MB on disk), with 17.5–17.8 MiB of PSS. It also
maps libicu (3 files, 39 MB) and 116 other system files (430 MB on disk, 62 MiB of PSS, with the
NVIDIA EGL libraries and LLVM). Ljós maps its own binary, libicu (2 files, 35.3 MB, pulled in by the
system's libxml2) and 47 other files (247 MB on disk, 20–24 MiB of PSS). In runs alternated two by
two, the binary's own PSS went from 39.6–40.2 MiB before to 36.2–36.7 MiB after. Its private dirty
pages stayed at 2.7–2.8 MiB. So Ljós keeps about twice what Qt's own libraries keep, as one file in
place of 47, and less of the system's.

**The glyph fallback.** The first character the default family lacks ("⅟" in calqlatr) had fontique
ask fontconfig for the fallback (`FcFontMatch`) and then sort every installed font for that family
(`FcFontSort` over 2 385 fonts). That took 5–11 ms in the frame that first drew it. fontique is not
ours, so the sort stays. An engine built with the system fonts now lays out one such character as
it is built, on the `ljos-fonts` thread that already reads the fonts, and the collection keeps what
it found. The first symbol drawn then takes 16–23 µs, against 5.3–10.7 ms. The engine's build stayed
at 28–44 ms, as before. Symbols, arrows and punctuation (the Common script) share the one lookup.
Another script pays its own on first use: Han 13–15 ms, Arabic 0.5–2 ms, emoji 2–11 ms. They are
not looked up ahead, because most programs never draw them.

**The QML module files.** DECISION: the files a library ships are not parsed at startup, and there
is nothing left to precompile. The styles (`ljos_controls`) and the dialogs (`ljos_platform`)
compile their QML to IR in their build scripts, and the IR is decoded per file when the file is
first read. Decoding all 99 files of the style pack (523 kB) takes 1.0 ms, about 10 µs a file
(ComboBox, the largest, 37 µs). An application decodes only the few it uses.

What does parse at startup is `ljos run`'s project, which a shipped application compiles at build
time. The entry is compiled on the `ljos-compile` thread while the window system and the drivers
start. Every other file was compiled on the main thread when the engine first made an object of it:
24 files and 23.4 ms in thermostat, 31 and 29.5 ms in todolist, 9 and 7.4 ms in samegame, 5 and
2.1 ms in calqlatr. The compile thread now goes on to the files the entry makes objects of, and
theirs, until the shell's setup asks for the source. What it compiled is held back and told when
the engine asks for it, so warnings come when they did. The main thread now compiles 4 files
(1.0 ms) in thermostat, 2 (0.7 ms) in samegame and none in calqlatr. In todolist the compile thread
gets only to `App` and `HomePage` before setup: opening the project, which scans its tree, takes
10–29 ms there. The first frame did not move with this (the table below), because those compiles
already overlapped the drivers' start.

**Re-measured** (interleaved as in the section before: before, after and qml6 in turn over the 11
apps, three rounds on llvmpipe and three on the GPU, desktop profile, `--settle 1 --idle 10`, load
1.5–51; each cell is the median of three).

| App | First frame, llvmpipe: before → after | qml6 | First frame, GPU: before → after | qml6 | Idle CPU after / qml6, llvmpipe | GPU |
|---|---:|---:|---:|---:|---:|---:|
| animations | 55 → 57 | 117 | 35 → 40 | 72 | 16.25 / 25.07 % | 1.85 / 2.43 % |
| form | 63 → 66 | 154 | 49 → 51 | 114 | 0 / 0 | 0 / 0 |
| listview | 61 → 55 | 139 | 42 → 41 | 90 | 21.53 / 23.07 % | **7.27 / 4.31 %** |
| rectangles | 58 → 66 | 121 | 49 → 50 | 81 | 0 / 0 | 0 / 0 |
| text | 61 → 63 | 142 | 46 → 49 | 94 | 0 / 0 | 0 / 0 |
| calqlatr | 64 → 57 | 165 | 51 → 47 | 118 | 0 / 0 | 0 / 0 |
| samegame | 64 → 58 | 150 | 48 → 50 | 121 | 8.71 / 29.21 % | 1.67 / 3.03 % |
| clocks | 65 → 53 | 158 | 42 → 45 | 100 | 11.70 / 19.71 % | 1.78 / 1.87 % |
| thermostat | 103 → 102 | 209 | 88 → 91 | 176 | 0 / 0 | 0 / 0 |
| todolist | 81 → 79 | 190 | 71 → 70 | 152 | 0 / 0 | 0 / 0 |
| rssnews | 61 → 57 | 165 | 46 → 43 | 120 | **1.49 / 1.35 %** | **1.29 / 1.08 %** |

The first frames moved by no more than the spread of one binary's rounds: thermostat was 83–105 ms
before on the GPU and 84–100 after. Startup was not made slower or faster by this work; it was
already about half of qml6's on every app.

Memory after 10 s idle, median, MiB (anon PSS before / after / qml6; the file-backed PSS of one
binary moves by up to 30 MiB with what the page cache holds, see the section before):

| App | GPU | llvmpipe |
|---|---:|---:|
| animations | 30.3 / 30.6 / 37.2 | 45.8 / 45.9 / 88.1 |
| form | 37.4 / 37.5 / 50.9 | 59.0 / 59.0 / 105.5 |
| listview | 30.7 / 30.8 / 43.7 | 46.6 / 46.6 / 91.6 |
| rectangles | **46.2 / 46.4 / 42.9** | 70.0 / 70.1 / 100.1 |
| text | 31.6 / 31.7 / 46.3 | 50.0 / 50.0 / 100.2 |
| calqlatr | 33.2 / 33.3 / 49.8 | 48.6 / 48.5 / 91.9 |
| samegame | 34.5 / 35.4 / 49.2 | 48.5 / 48.7 / 102.3 |
| clocks | 29.8 / 30.7 / 46.0 | 46.0 / 46.4 / 95.6 |
| thermostat | 56.3 / 55.0 / 62.6 | 85.6 / 86.2 / 139.7 |
| todolist | 42.2 / 42.3 / 55.1 | 56.7 / 56.8 / 104.8 |
| rssnews | 39.7 / 41.5 / 187.7 | 57.2 / 57.7 / 341.1 |

Anonymous memory did not change: what was cut is file pages, which the binary's own PSS above
shows.

**Where Ljós still loses to qml6.**

- **Idle CPU of listview on the GPU**: 7.27 % against 4.31 %. On llvmpipe it is below qml6 (21.5
  against 23.1 %). The cost on the GPU is per frame, not in the scene.
- **Idle CPU of rssnews**: 1.29 against 1.08 % on the GPU, and 1.49 against 1.35 % on llvmpipe.
  This is within a run's spread, but it was above qml6 in every round.
- **Rectangles' memory on the GPU**: 46.4 MiB of anon PSS against 42.9 (49.3 against 46.7 MiB
  `memory.current` after idle). The node table, as in the section before.
- **The binary's file pages**: 36 MiB of PSS against Qt's 17.5 MiB for its libraries. The command
  carries the developer tools, the codecs and the PDF reader, and a shipped application links a
  smaller set. Anonymous memory is what both pay per process, and there Ljós is lower on every app
  but rectangles.
- **Startup of todolist and thermostat** is still 70–100 ms. Neither is bound by compiling any more:
  opening todolist's project (scanning its tree) takes 10–29 ms on the compile thread, and
  thermostat's first frame waits on something that is not its compile.

### Binding slots, notify, unchanged frame work and clocks' timers (2026-10-18)

Same method as the section above: callgrind main-thread instructions per frame with the prof
build in the GPU sandbox, and all-thread M/s from `thr.py` in one process. "Before" is d9b176c9,
where bindings already write numbers through slots.

| Main thread, a frame | Before | Whole run | Notify | No empty shapes | Snap |
|---|---:|---:|---:|---:|---:|
| animations | 1.171 M | 1.029 M | 1.011 M | 970 k | 962 k |
| clocks | 394 k | 393 k | | 390 k | 403 k |

Clocks moves by about ±10 k between runs, because the number of timer calls a frame varies.

| All threads, M/s | Ljós before | Ljós after | Qt |
|---|---:|---:|---:|
| animations, GPU | 63.2 | 50.9 | 43.6 |
| clocks, GPU | 20.3 | 20.6 | 14.8 |
| samegame, GPU | 14.7 | 14.7 | 15.8 |
| animations, llvmpipe | 454 | 462 | 1 040 |
| clocks, llvmpipe | 153 | 156 | 925 |
| samegame, llvmpipe | 241 | 242 | 1 592 |

- **Whole run.** On a device, a frame whose damage is half the window or more is drawn whole. Its
  damage was still worked out first: comparing shapes, `Damage::of` and the grid came to 151 k a
  frame for 200 bars, then went unused. For 30 frames after such a frame, the GL painter now asks
  only whether the picture changed, and that check stops at the first shape that differs. A frame
  with no change ends the run. egui-wgpu draws every frame whole, so it never asks for areas.
- **Notify.** An animated `x` has one observer: the place effect, which is told rather than run.
  Nothing is marked twice and nothing is allocated. What could go:
  - the scene's hash-set insert for each item told it moved (76 of the 100 instructions);
  - taking the mark stack out and putting it back twice per write.

  The told list is now a `Vec`, read once per frame through a stamp on the entry. The flush fell
  from 28 k to 14 k a frame.
- **No empty shapes.** `fill_rects` joins the 200 bars into one mesh and left 199 `Shape::Noop`s
  behind. egui's tessellator spends about 300 instructions on each of them, so tessellating fell
  from 75 k to 30 k once they are removed.
- **Snap.** A hard-edged rectangle snapped to pixel centres is no longer checked for lying on whole
  pixels.
- **Draw clocks whole on a device.** Tried and dropped. All-thread M/s moved by 4 %, within run
  noise (19.9 against 20.8), so the ½ rule stays.
- **Clocks' `timeChanged`.** One call is about 47 k. About 14 k is native:
  - four property sets, 8.9 k, about 1.9 k each in `write_impl` including the flush they set off;
  - six reads, 2.8 k;
  - `new Date`, 2.2 k;
  - the nested call from the Timer's handler through `Engine::invoke` back into JS, 2.3 k.

  The other ~33 k is QuickJS: interpreting the body, the accessor trampolines for each property
  read and write (`JS_GetPropertyInternal` → `JS_CallFree`, `call_setter` → `JS_CallInternal`),
  the proxy lookup of `clock`, and rquickjs's wrapping of the eleven native calls. That is a matter
  for ilang, not the native side.

**What is left.** At 60 fps the GPU animations frame is 0.85 M on all threads, against Qt's 0.73 M.
The largest remaining pieces of the main thread are:

- the animation tick, 214 k: about 600 per write, 371 of it the graph;
- `compose`, 100 k;
- encode, 105 k;
- the swap, about 80 k in hardware.

In clocks, the transform-list effect of each hand that springs reruns every frame. Each rerun is
2.9 k, mostly tracked reads of `Rotation`'s field-backed `origin`, `axis` and `distanceToPlane`,
about 20 k a frame.

### Script gaps to qml6, second pass: calqlatr, clocks, for…in and samegame (2026-10-18)

The same bench copies, runs and builds as the section above. The base is 1a65e365. Ir is from
callgrind on a release build. Wall time is the fastest of 5 runs, alternating builds, on a
machine with a load average of 20–37 from other work, so only Ir is a reliable comparison
between builds. qml6 was run under the same load.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| calqlatr, Ir per press (280 presses) | | 2.127 M | 2.025 M |
| calqlatr, wall per press (700 presses) | 0.102 ms | 0.176 ms | 0.164 ms |
| clocks, Ir per `timeChanged` (700 calls) | | 39.8 k | 36.2 k |
| clocks, wall per `timeChanged` (7 000 calls) | 0.0007 ms | 0.0028 ms | 0.0025 ms |
| samegame, Ir per board built (2 boards) | | 88.4 M | 81.3 M |
| samegame, of that the script call | | 71.1 M | 66.3 M |
| samegame, of that the destroy frame | | 16.9 M | 14.7 M |
| samegame, wall per new game (40) | 7.6–7.8 ms | 6.9 ms | 6.9 ms |
| samegame, wall per game and 20 clicks (40) | 12.1–12.3 ms | 9.4 ms | 8.7 ms |

**calqlatr.**
- A script's write of a number, bool or text to a plain property goes straight to its node
  (`plain_script_write`), and a script's list face is found by pointer first.
- A ListModel change, followed by `positionViewAtEnd`, no longer lays the view out twice in a
  batch. Each append is now one pass each way, down from three
  (`a_row_appended_and_shown_at_the_end_is_laid_out_once_each_way`).
- What is left per press: about 200 host property reads (about 680 Ir each, plus conversion);
  33 writes, each 16–24 k with the bindings they flush; about 11 Behavior starts of about 14 k
  each; and `grid.children` copied about 96 times (14 %).
- DECISION: the `children` copies stay. A shared frozen array would change what `push` and
  `sort` do on it. A live list object is a larger change.

**clocks.**
- `Date` is a plain wrapper again, not a Proxy, and asks the host only while the clock is
  pinned.
- What is left per call: rquickjs's accessor trampolines (about 600 Ir per access),
  `get_date_fields` (about 840), and the interpreter (about 6.5 k).
- DECISION: C accessors that skip rquickjs are left out. They would depend on the layout of
  rquickjs's class opaque, which is not part of its API.

**for…in.**
- Properties are listed base first, then signals, as qml6 does
  (`features/language/object-enumeration.feature`, @qt-parity).
- A type's prototype is given copies of its base's accessors in that order, instead of
  chaining to the base prototype. maroon's `benchSpike` costs 357.7 M → 360.2 M Ir (+0.7 %):
  2 858 entries copied over 29 types, about 880 Ir each. About 70 % of the prototype win of the
  section above is kept.
- DECISION: methods stay non-enumerable and `Object.keys` stays empty. Qt lists both.

**samegame `benchNew`.** A block is about 20 objects: an Item, three Behaviors with their
animations, an Image, two emitters with their shapes and directions, and two States. The cuts:
- A destroyed object is taken out of its owner's and visual parent's child lists by itself.
  Before, each of the hundreds of blocks in one pass of deferred deletes had the whole list
  scanned, with a store lookup per sibling.
- The list properties of a holder are cleaned against a sorted list of the dead, not compared
  with each of them.
- An empty list property shares one empty list until it is written. An Item has six. The
  in-place edit of a list no longer makes an empty one to swap in.
- Lookups that throw away the reason (`find_property`) build no error text. The place a
  Behavior is written is formatted only for a warning. A dotted member name is one string, not
  a join plus a format. Required properties are listed once per type, not found by scanning
  every property of every object made.
- Maps keyed by ids hash with Fx: contexts' ids, singleton scopes (looked up for every
  destroyed object), the registry, and Behavior followers.

What is left of a block is the per-object work of creation: binding setup, property writes,
completion, `Item` init with its two watches, and about 400 allocations. The Image's completion
(about 39 k Ir) is the image store's.

### Frames that change, frame pacing and the lag left against Qt (2026-10-18)

`BENCH_PROFILE=dist BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench calqlatr clocks samegame
todolist rssnews --profile desktop --settle 3 --idle 4 --load 5`: Ljós's `dist` binary at 91d5d9ef
against `qml6` 6.11.2 with the Basic style, at equal window sizes, medians of 3 runs. Other work
kept the machine's load average at 12.5–50.4, so the timings are noisier than the last section's.

| | Before (11ef7c78): Ljós | Qt | After: Ljós | Qt |
|---|---:|---:|---:|---:|
| samegame idle, 95th percentile frame interval | 32 ms | 16 ms | 16.1 ms | 16.1 ms |
| samegame idle, slowest frame interval | 488 ms | 481 ms | 485 ms | 483 ms |
| samegame idle, frames that changed a second | 37.5 | 44.0 | 46.5 | 43.7 |
| calqlatr driven, 95th percentile frame interval | 16.1 ms | 32.3 ms | 16.1 ms | 32.1 ms |
| rssnews driven, 95th percentile frame interval | 32.2 ms | 32.3 ms | 16.2 ms | 32.4 ms |
| rssnews idle CPU | ×1.45 of Qt | | 0.02 % | 0.00 % |

The "before" lag rows are a release run that already counts frames by their pixels.

| Ljós/Qt, geometric mean | Last section | This run |
|---|---:|---:|
| first frame | ×0.23 | ×0.22 |
| CPU to first frame | ×0.56 | ×0.54 |
| first frame that answers input | ×0.28 | ×0.26 |
| peak memory while starting | ×0.57 | ×0.55 |
| settled memory (cgroup) | ×0.50 | ×0.40 |
| settled PSS | ×0.60 | ×0.49 |
| idle CPU (clocks, samegame) | ×0.55 | ×0.32 |
| CPU while driven | ×0.38 | ×0.42 |
| frames shown while driven | | ×1.04 |

- **Frames are counted by their pixels.** The bench counted every frame the compositor was sent.
  Qt sends frames that show what the last one did (samegame: 62 a second sent, 43.7 that changed),
  so the counts and intervals favoured it. The "286 ms against 16 ms" slowest interval was such
  a count: counted by pixels, both pause for the logo's 500 ms `PauseAnimation`.
- **Frames kept the display's pace.** A drawn pass held the next a frame after it began. Two
  things added a refresh one time in thirty:
  - Late wake-ups added up, and Ljós's passes came 16.9 ms apart.
  - The sandbox's headless output says it has no refresh rate. It draws at about 62 Hz, and Ljós
    held each pass a 60 Hz frame.

  Ljós now keeps the last pass's beat when a pass begins less than half a frame late. A display
  that does not say its rate holds a pass three quarters of a 60 Hz frame, and the platform's
  frame callback paces the rest.
- **calqlatr and rssnews driven.** calqlatr's 48 ms was the old count; counted by pixels it was
  already 16 ms. rssnews's 32 ms was the pacing above, and it is now at 16 ms. Qt's are 32 ms:
  more of its changed frames come two refreshes apart. The waits for each click are the same in
  both runtimes (rssnews's median is 42.5 ms against 42.6).
- **rssnews idle CPU** was no wake-up. The per-thread split showed the 1.45 was picture fetching
  that had not finished in a one-second settle: about 146 ms of Ljós's fetch threads against
  Qt's 147 ms pixmap reader and 66 ms network thread. Settled for 3 s, both idle at about zero.
- **samegame's 95th percentile input-to-frame latency** (193 ms against 15.9) is the click
  grid's phase, not lag. The first clicks hit no control, so their wait is for the logo's next
  frame. Ljós starts 240 ms sooner than Qt, so 7 s later its logo is 1.07 s into the 3 s cycle,
  inside the pause, with 433 ms of it left: its first two waits are 433 and 193 ms in every run
  that lands there. Qt's land there in other runs (158–268 ms at its fourth click).
- **The text cursor** between pixels is now drawn solid over both columns its 1 px rectangle
  touches, as qml6 draws it. It was a grey blend. See `features/text-input/cursor-pixels.feature`.

**What is left.** Driven, samegame changes 57.9 frames a second against Qt's 61.8, with the same
95th percentile and slowest intervals. Activation on a seat without a keyboard has no runner to
check it yet (`features/window/activation-without-keyboard.feature` stays `@blocked`).

### Hot reload: edits fuzzed against fresh loads (2026-10-18)

`crates/ljos_shell/examples/edit_fuzz.rs` loads a copy of each corpus app in a `ShellApp`
without a window and saves 25 random small edits to its files, with seed 7. The edits add,
remove or move an object, change a binding, a handler or a function, or rename a property.
After each reload, the running app is compared with the same files loaded afresh: the object
tree, every property's value, and whether each one is bound. Four lanes ran the 45 apps;
clocks and photosurface ran again after the last fix. 13 apps don't load (types Ljós doesn't
have: `MapLoader`, `AssetDownloader`, `RestService`, `PhysicsWorld`, `GraphsTheme`, `Map`,
`PlaybackSeekControl`, `OSMManager`, `BackEnd`, `View3D`, `MyFocusScopeWidget`, `Surface3D`,
`WindowContainer`).

- **No reload:** the edit hit a file the app doesn't load, which happens when snippets share
  a folder.
- **Kept state:** values the run built up that a reload rightly keeps.
- **Window sized by the host:** the fresh load gets another window size.

| App | in place | full path | did not compile | no reload | nothing to reload | diverged | kept state | window sized by the host |
|---|---|---|---|---|---|---|---|---|
| alarms | 17 | 8 | – | – | – | 5 | – | – |
| android-splashscreen | 21 | 4 | – | – | – | – | – | – |
| calqlatr | 16 | 7 | 2 | – | – | 1 | 1 | 1 |
| clocks | 15 | 9 | 1 | – | – | 1 | 18 | – |
| coffee | 14 | 5 | 6 | – | – | – | – | – |
| fx-material-showroom | 1 | – | 24 | – | – | – | – | – |
| hangman | 1 | 1 | 23 | – | – | – | – | – |
| maroon | 20 | 3 | 2 | – | – | – | – | – |
| photosurface | 19 | 5 | 1 | – | – | 12 | 2 | – |
| rssnews | 19 | 5 | 1 | – | – | – | – | – |
| samegame | 16 | 2 | 2 | 5 | – | – | – | – |
| snippet-alias-application | 17 | 5 | 3 | – | – | – | 17 | 8 |
| snippet-borderimage-defaults | 25 | – | – | – | – | – | 19 | 6 |
| snippet-components-application | 15 | 7 | 3 | – | – | – | – | – |
| snippet-flow-diagram | 3 | – | – | 22 | – | – | – | – |
| snippet-focus-column | 8 | 1 | 16 | – | – | – | 7 | 4 |
| snippet-focus-scopes | 2 | 1 | – | 22 | – | – | – | – |
| snippet-methods-app | 25 | – | – | – | – | – | 18 | 7 |
| snippet-properties-application | 12 | – | 2 | 11 | – | – | – | – |
| snippet-property-signals | 12 | – | 2 | 11 | – | – | – | – |
| snippet-rectangle-smooth | 16 | 6 | 3 | – | – | – | 1 | 3 |
| snippet-signals-basic | 25 | – | – | – | – | – | 12 | 1 |
| snippet-signals-connectdynamic | 25 | – | – | – | – | – | – | – |
| snippet-signals-connectslots | 25 | – | – | – | – | – | – | 1 |
| snippet-signals-no-parameters | 23 | 2 | – | – | – | – | 22 | 3 |
| snippet-signals-parameters | 23 | 2 | – | – | – | – | 21 | 2 |
| snippet-text-lines | 3 | – | – | 22 | – | – | – | – |
| snippet-texteditor | 3 | – | – | 22 | – | – | – | – |
| snippet-vertical-positioner-transition | 24 | 1 | – | – | – | 9 | – | – |
| thermostat | 9 | – | 16 | – | – | – | – | – |
| todolist | 11 | 3 | 4 | 7 | – | – | – | – |
| toycustomizer | 2 | – | 23 | – | – | – | – | – |

Of the 800 edits, 467 went in place and 77 took the full path. Another 134 didn't compile,
and 122 hit a file the app doesn't load. Earlier in this batch, 96 took the full path, 35 of
them for an edit to a member of a group or an attached object (`font.pixelSize`,
`anchors.margins`, `border.color`, `Layout.preferredWidth`). Those now go in place.

**Why the 77 still take the full path:**

| Edits | Reason | Why it stays |
|---|---|---|
| 25 | a property renamed (signature changed) | The type's members change, and with them every object made from it. |
| 10 | objects came or went where they can't be put (a `ListElement` removed or moved out) | A model's rows are data, not objects in the tree. |
| 8 | a member is no longer given | It goes back to its default, which only a fresh object knows. |
| 7 + 1 | grouped or attached members changed | A same-type sibling moved into another's place sets other members; same as above. |
| 6 | a user of the component sets the edited member | What the user writes outranks the file; this stays until edits can skip such instances. |
| 5 | a component removed | Its objects are made from a component that is gone. |
| 4 | a model's elements changed | Same as rows above. |
| 3 | an edited binding reads an id that is gone | It would throw and keep its old value; a fresh load starts it from its default. |
| 2 + 2 | members given objects, or declared properties, changed | The object is a different shape. |
| 1 each | parent can't hold the object, a state holds the edited object, an alias's member not found, a file added | |

**Bugs found and fixed, each with a scenario:**

1. The visual-children order before an anchor, and focus with a Repeater.
2. An item moved before a sibling that is not an item.
3. An object added last in a Control.
4. A template or delegate inside an edited instance.
5. The slots of an instance whose inner file changed (rssnews).
6. A descendant shown in another item stayed in that item's lists after it was destroyed
   (a Dialog left in the overlay, alarms).
7. A value a script gave an object remade in place was lost at the next full path (samegame).
8. An animation `start()` set going was stopped by a full path (photosurface).
9. A binding that now reads a removed id kept its old value (photosurface, and
   `anchors.centerIn` in clocks).

**State kept on purpose, so these remain as differences (28 edits):**

- **Values scripts wrote, and the bindings that follow from them.** calqlatr's `dimmed` and its
  button text colours.
- **Animations and Behaviors part-way through.** clocks' hands, photosurface's intro frame,
  the positioner's move transition.
- **Time.** clocks' minute rolls over between the two loads.
- **Things that only happen once.** photosurface's intro destroys itself and then picks the
  folder; the fresh load hasn't got there yet.
- **The overlay.** Once made, it stays even when empty. A fresh load without popups never makes
  one, while Qt always has an overlay.
- **Tumbler.** Its PathView `currentIndex` is bound in the live app but written with `=` in the
  fresh one, with the same value: the view's own write breaks the binding. How views write is
  not changed here.
- **The positioner.** In vertical-positioner-transition, the Column whose child has
  `anchors.fill` stops placing, as Qt's does.
- **Model rows.** The row objects `get()` made (todolist) are a cache, compared through the
  model.

### Hot reload: fewer full-path reloads (2026-10-18)

The same fuzzer, three lanes, seeds 7 and 13, run again after each step. A step's column is
the build with that step and the ones before it. "step 4" also has the fixes made after it.

1. A user of the component sets the edited member: the other instances take the edit in
   place, and those whose user sets it keep the user's value.
2. A member no longer given, or a same-type sibling moved into another's place: the member
   goes back to what a new object of its type has (the reset, else the native default and its
   default binding). The object stays.
3. A `ListElement` added, removed or moved in a model written in QML: the model's own insert,
   remove and move. Views keep the delegates of the rows that stay.
4. A property renamed: in place when no other file of the app reads or sets either name.
   Otherwise it takes the full path, and the debug log says which file.

**Seed 7, edits that took the full path, by reason:**

| Reason | before | step 1 | step 2 | step 3 | step 4 |
|---|---:|---:|---:|---:|---:|
| a component's signature changed | 34 | 34 | 34 | 34 | 9 |
| objects came or went where they cannot be put | 10 | 10 | 10 | 1 | 1 |
| a member is no longer given | 8 | 8 | – | – | – |
| an object's grouped members changed | 7 | 7 | 1 | 1 | 1 |
| ComponentRemoved | 6 | 6 | 6 | 6 | 6 |
| an object's declared properties changed | 6 | 6 | 6 | 6 | 4 |
| a user of the component sets the edited member | 6 | – | – | – | – |
| an edited binding reads an id that is gone | 5 | 5 | 5 | 5 | 5 |
| a model's elements changed | 4 | 4 | 4 | – | – |
| an object's members given objects changed | 3 | 3 | 3 | 3 | 3 |
| a singleton's file edited | 1 | 1 | 1 | 1 | 5 |
| a state or a Binding holds the edited object | 1 | 1 | 1 | 1 | 2 |
| an edited member is not found | 1 | 1 | 1 | 1 | 1 |
| the parent can't hold the object | 1 | 1 | 1 | 1 | 1 |
| FileAdded | 1 | 1 | 1 | 1 | 1 |
| an object's attached members changed | 1 | 1 | – | – | – |
| an element is not in a model that stays | – | – | – | 3 | 3 |
| a renamed property's object has members its user declares | – | – | – | – | 3 |
| a renamed property is read or set in another file | – | – | – | – | 1 |
| **full path** | **95** | **89** | **74** | **64** | **46** |
| **in place** | **535** | **541** | **556** | **566** | **561** |
| **diverged** | **38** | **37** | **48** | **55** | **48** |

**Seed 13:**

| Reason | before | step 1 | step 2 | step 3 | step 4 |
|---|---:|---:|---:|---:|---:|
| a component's signature changed | 54 | 54 | 54 | 54 | 13 |
| ComponentRemoved | 8 | 8 | 8 | 8 | 9 |
| an edited member is not found | 7 | 7 | 7 | 7 | 7 |
| objects came or went where they cannot be put | 7 | 7 | 7 | – | – |
| a state or a Binding holds the edited object | 6 | 6 | 6 | 6 | 6 |
| a member is no longer given | 5 | 5 | – | – | – |
| a user of the component sets the edited member | 5 | – | – | – | – |
| an object's members given objects changed | 4 | 4 | 4 | 4 | 4 |
| an object's declared properties changed | 4 | 4 | 4 | 4 | 3 |
| a model's elements changed | 3 | 3 | 3 | – | – |
| an object's grouped members changed | 3 | 3 | – | – | – |
| a singleton's file edited | 2 | 2 | 2 | 2 | 5 |
| an edited binding reads an id that is gone | 2 | 2 | 2 | 2 | 2 |
| FileAdded | 1 | 1 | 1 | 1 | 1 |
| a component's name, id, kind or signature changed | 1 | 1 | 1 | 1 | – |
| an object's members changed | 1 | 1 | 1 | 1 | – |
| a renamed property is read or set in another file | – | – | – | – | 5 |
| a renamed property's object has members its user declares | – | – | – | – | 3 |
| an element is not in a model that stays | – | – | – | 1 | 1 |
| a component's name, id or kind changed | – | – | – | – | 1 |
| **full path** | **113** | **108** | **100** | **91** | **60** |
| **in place** | **548** | **553** | **561** | **570** | **578** |
| **diverged** | **48** | **50** | **56** | **55** | **54** |

`ComponentRemoved` and `FileAdded` are the reload's own names for a component or a file that
came or went. Some reasons grow in the last column because the edits before them now go in
place: the next edit then meets other code. For example, a rename that now goes in place
leaves the next edit to a singleton's file. The number of edits that compile also changes
from build to build, so the in-place and full-path counts don't add up to the same total.

What stays on the full path:

- **A renamed property that another file reads or sets.** The users' bindings name it.
- **A renamed property on an object whose user declares members.** The user's members are laid
  out after the type's.
- **A partial grouped member or an alias given a value.** What the member was before can't be
  worked out per field.
- **A singleton's file, a component removed, a signal or a declared property's type changed,
  members given objects changed, an id that a binding reads removed.** Same as before.

**Bugs found on the way and fixed, each with a scenario or a test:**

1. A rename on an object whose user declares members took the user's members' places.
2. A native member no longer given lost its default binding, for example `Accessible.name`.
3. An object destroyed by a reload's model edit waited for the next input to be deleted. It
   now wakes the host, as any deferred delete does.
4. An id removed from a component's file kept naming the removed object. The bindings that
   read it got `null` where a fresh load finds the user's object of that name (todolist).

**More state kept on purpose:**

- **Delegate order after a row moved or was removed.** The delegates of the rows that stay are
  kept, so they are made in another order than a fresh load makes them (clocks, rssnews). Qt
  keeps them too when a script changes the model.
- **A view's highlight before its new delegates.** When the model a view reads becomes another
  model, the highlight stays and the new delegates come after it, as when a script sets
  `model`.
- **A `LayoutItemProxy` given another target.** It keeps the items it holds, as when a script
  sets `target` (calqlatr).
- **Nested models.** A row whose nested list changed gets a new nested model, made after the
  others. Every delegate reads the same data, as after a script's `set()` (alarms).
- **Follow bindings after a reset.** A side margin or a corner radius no longer given follows
  `margins` or `radius` again with a binding. A fresh object binds it only when needed, so the
  value is the same and only "bound" differs.

photosurface is not fixed. After an in-place rename, its full path makes 11 to 14 objects new
that it used to keep. This is a matching count only: the values agree.

### Hot reload: from saving a file to the frame that shows it (2026-10-18)

The time from a file's save to the first frame that shows the edit, median of 20 saves 2.5 s
apart.

- **Ljós:** `reload_profile` with `--features dev` and no GPU paint, on a loaded machine. The
  edits were a binding's value (in place), and a change to the signature of `Clock` or of
  calqlatr's `Display` (full path).
- **qml6:** qml6 has no reload of its own, so it was timed through Qt's `qmlpreview`, which
  reloads the whole file each time. A copy of the app had a `frameSwapped` probe added.

| App | Ljós in place | Ljós full path | qml6 through qmlpreview |
|---|---:|---:|---:|
| clocks | 16.8 ms | 21.1 ms | 119.5 ms |
| calqlatr | 19.0 ms | 28.4 ms | 131 ms |

- **The quiet wait.** Ljós's times include the 15.1 ms quiet wait that gathers an editor's
  writes into one reload. The reload itself, to the frame, takes about 1.7 ms (clocks) and
  3.9 ms (calqlatr) in place, and 6 ms and 13 ms on the full path.
- **qmlpreview's watcher.** Most of qml6's time is the watcher's own delay: no new tree was
  made in less than 104 ms.
- **The slowest saves.** On both sides, a few saves out of 20 took hundreds of ms because the
  machine was loaded. The medians leave them out.

### Hot reload: the full path cut by half again (2026-10-23)

The same fuzzer as above: 25 edits to every corpus example, at seeds 7, 13 and 29. "before" is
the build at the start of this work and "after" is the build with every step below. An edit
that a step below handles now goes in place, and its result is checked against a fresh load.

1. An alias renamed.
2. A `Component` written inside an object comes or goes with it. Only templates may come or go,
   and the objects made from a template that is gone are destroyed.
3. A singleton's file edited: the singleton is edited in place, like any other object.
4. A binding edited to read an id that is gone: a warning and `undefined`, as a fresh load gives.
   An axis placed only by anchors to that id goes back to where a fresh load puts it.
5. An object given to a member that is removed: the member goes back to what a new object has.
   The full path stays when the inner component gives that member.
6. An object made again that an alias names: the alias names the new object, and the users that
   read it, or give values through it, are given them again.
7. A value a state's `PropertyChanges` gives, edited: the state in force is applied again
   without its transition or scripts. Every object made from the file takes the new value.
8. A member edited through a name its object does not have: a warning, in place.

| Reason | 7 before | 7 after | 13 before | 13 after | 29 before | 29 after |
|---|---:|---:|---:|---:|---:|---:|
| a component's signature changed | 9 | 4 | 13 | 8 | 10 | 4 |
| ComponentRemoved | 6 | – | 9 | – | 9 | – |
| a singleton's file edited | 5 | – | 5 | – | 6 | – |
| an edited binding reads an id that is gone | 5 | – | 2 | – | 4 | – |
| an object's declared properties changed | 4 | – | 3 | – | 6 | – |
| a renamed property's object has members its user declares | 3 | 5 | 3 | 6 | 3 | 6 |
| an object's members given objects changed | 3 | 2 | 4 | – | 8 | 3 |
| an element is not in a model that stays | 3 | 3 | 1 | 1 | – | – |
| a state or a Binding holds the edited object | 2 | 1 | 6 | 3 | 2 | 2 |
| a renamed property is read or set in another file | 1 | 7 | 5 | 8 | 5 | 8 |
| an edited member is not found | 1 | – | 7 | – | 1 | – |
| FileAdded | 1 | 1 | 1 | 1 | 1 | 1 |
| objects came or went, grouped members, the parent can't hold it | 3 | 3 | – | – | – | – |
| the component an object is made from gives a part of it | – | – | – | 2 | – | 2 |
| an object's children changed | – | – | – | 1 | – | – |
| a component's name, id or kind changed | – | – | 1 | – | – | – |
| a state or a Binding holds an object made again (alias) | – | – | – | – | – | 1 |
| an object is given through an alias of an object made again | – | – | – | – | – | 1 |
| **full path** | **46** | **26** | **60** | **30** | **55** | **28** |
| **in place** | **561** | **581** | **578** | **584** | **571** | **598** |
| **diverged** | **41** | **41** | **48** | **49** | **30** | **29** |

The renamed-property rows grow because edits that used to stop earlier now reach them.
The two alias rows are a guard added last. Without it, seed 29 took two coffee edits in place
and diverged on them: a state wrote through an alias whose object was made again, and a user
gave a state through one. The guard matches the alias by name, so it can send more edits to the
full path than it needs to.

**Save to frame**, median of 20 saves 2.5 s apart, at a load average of 20–40 from other work.
For thermostat, qml6 ran through `qmlpreview` as in the section above. The clocks and calqlatr
qml6 times are from that section.

| Edit | Ljós in place | Ljós full path | qml6 through qmlpreview |
|---|---:|---:|---:|
| clocks, a binding's value | 17.4 ms | 20.7 ms | 119.5 ms |
| calqlatr, a binding's value | 24.7 ms | 59.5 ms | 131 ms |
| thermostat, a singleton's colour | 34.2 ms | 129.1 ms | 270 ms |
| thermostat, a state's `PropertyChanges` value | 35.0 ms | 122.4 ms | 301 ms |

On the full path, thermostat also holds about 97 MB where in place holds 76 MB.

**State kept on purpose**, beyond the list above:

- **The order of objects made again.** Siblings, or a view's contents, made again come after
  those that stay, as when a script makes them (samegame, alarms).
- **Layouts after a move.** A layout that held an object moved into it keeps the widths it gave
  (stocqt).
- **Values the run built up**, such as calqlatr's `dimmed`.

**Not fixed:**

- **A Text on a SwipeView page that isn't shown.** Its contentHeight is not measured again when
  its font changes (thermostat). This is not a reload bug.
- **The palette after a reload.** Removing a Popup or a Button leaves the root's palette at the
  default colours where a fresh load gives the style's (alarms, android-splashscreen). The
  baseline build does the same.

### Hot reload: renames read elsewhere, and the palette after a reload (2026-10-24)

The same fuzzer: 25 edits to every corpus example, at seeds 7, 13 and 29. "before" is the
"after" build of the section above, and "after" is this one. The steps:

1. **The modules in use are counted again after a reload.** The style's palette follows, so
   removing the last Popup or Button gives the colours a fresh load gives, in every colour
   group. The files counted are those of the objects that stand, and of the instances: the
   root of a component file carries its user's location.
2. **The alias guard matches the object that has the alias, not its name.** At these seeds it
   changes no count: the two edits it holds back are the two that diverge without it.
3. **A renamed property that another file reads goes in place.** That file's bindings that read
   either name are bound again, as on a fresh load. So does a rename whose user declares members.
4. **A delegate written inline that is removed takes its `Component`.**
5. **A file that compiles again, which no loaded file names, goes in place.** This is FileAdded.
6. **A crash is fixed.** The rename check panicked with "RefCell already borrowed" when finding
   a type loaded a file (maroon, edit 3 at seed 7). Every build in these tables stopped maroon
   there, at 3 of its 25 edits.

| Reason | 7 before | 7 after | 13 before | 13 after | 29 before | 29 after |
|---|---:|---:|---:|---:|---:|---:|
| a renamed property is read or set in another file | 7 | 3 | 8 | 5 | 8 | 7 |
| a renamed property's object has members its user declares | 5 | – | 6 | – | 6 | – |
| a component's signature changed | 4 | 4 | 8 | 8 | 4 | 4 |
| an element is not in a model that stays | 3 | 3 | 1 | 1 | – | – |
| an object's members given objects changed | 2 | – | – | – | 3 | 1 |
| a state or a Binding holds the edited object, or one made again | 1 | 1 | 3 | 3 | 3 | 3 |
| FileAdded | 1 | – | 1 | – | 1 | – |
| the component an object is made from gives a part of it | – | – | 2 | 2 | 2 | 3 |
| an object is given through an alias of an object made again | – | – | – | – | 1 | 1 |
| a member no longer given is an alias | – | 1 | – | – | – | 1 |
| other | 3 | 3 | 1 | 1 | – | – |
| **full path** | **26** | **15** | **30** | **20** | **28** | **20** |
| **in place** | **581** | **592** | **584** | **594** | **598** | **606** |
| **diverged** | **41** | **39** | **49** | **48** | **29** | **29** |

The renames that stay on the full path are set by another file, or connected to through their
signal, or read by a script file. A fresh load refuses those, or connects them differently.

**Diverged.** Before steps 1 and 4 were added, this build took the palette edits (alarms, seed 7)
and a removed delegate (alarms, seed 29) in place and diverged on them. A window made from another
file also lost Fusion's palette (robotarm, seed 29). None of these diverge now. The rest of the
count moves with photosurface's and clocks' timing from run to run.

**Save to frame**, median of 20 saves 2.5 s apart, at a load average of 15–20. qml6 ran through
`qmlpreview` as above.

| Edit | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| clocks, a binding's value | 17.4 ms | 17.6 ms | 119.5 ms |
| calqlatr, a binding's value | 24.7 ms | 18.7 ms | 131 ms |
| thermostat, a singleton's colour | 34.2 ms | 33.1 ms | 270 ms |
| thermostat, a state's `PropertyChanges` value | 35.0 ms | 33.6 ms | 301 ms |
| thermostat, a property another file reads, renamed | 110.7 ms (full path) | 37.9 ms | 223.5 ms |

After the rename, thermostat holds 81 MB in place against 99 MB on the full path.

**Not fixed:**

- **The overlay after the last popup is removed.** The window's hidden overlay item stays, where a
  fresh load of a window without popups makes none (alarms, seed 7). Nothing in it is drawn.
- **A moved object made again binds an anchor margin** that a fresh load leaves unbound. The value
  is the same (maroon's BuildButton).
- **A Text on a SwipeView page that isn't shown** is not measured again after a font change
  (thermostat). This is a scene bug.

### Clicks on a settled window, and a press that arrives (2026-10-18)

`BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench samegame --profile desktop --settle 3 --idle 4
--load 5`: the release binary at 1c73e398 with the bench's changes, against `qml6` 6.11.2, at a
load average of 33–51.

| samegame, driven by the click grid | Before: Ljós | Qt | After: Ljós | Qt |
|---|---:|---:|---:|---:|
| 95th percentile input-to-frame latency (ms; each run) | 193 | 15.9 | 15.1, 15.7, 32.1 | 15.8, 15.6, 14.6 |
| median input-to-frame latency (ms; median of 3) | | | 7.6 | 7.3 |
| wait before driving (ms; median of 3) | | | 321 | 2 778 |
| CPU while driven (% of one core; median of 3) | | | 93.4 | 218 |

- **The grid now starts on a settled window.** The pointer is put where the first click goes and
  the bench waits until the window has not changed for 300 ms. Both runtimes then begin inside the
  logo's pause, where Ljós (which starts sooner) already is; Qt waits 2.8 s to get there.
- **A press arrives.** On sway, a button sent with the motion that brings the pointer onto a window
  goes to the window the pointer left, so the first click on a window could press nothing in either
  runtime. The bench's pointer now moves first, waits for the compositor, then presses. The same
  pointer drives the `@desktop` scenarios, and
  `features/window/activation-without-keyboard.feature` now runs and passes on the desktop sandbox.

### The whole gallery against qml6 (2026-10-18)

Every corpus example that runs on both runtimes (28), benched in turn, four to a `mise run bench`
call: `BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench <ids> --profile desktop --settle 3
--idle 5 --load 10`. Ljós is the release build at 77414def; Qt is `qml6` 6.11.2 with
`QT_QUICK_CONTROLS_STYLE=Basic`. Both run as Wayland clients of the sandbox's headless sway
(pixman), on an AMD Ryzen AI MAX+ 395 (32 threads), at a load average of 4.8–119.6: other work
shared the machine throughout. The load drives each app with the controls `mise run gallery:drive`
clicked there (the `clicks` column: how many points; 25 is the 5×5 grid, used where it found fewer
than three). Each cell is the median of three runs, with Qt's in brackets. Lag is input-to-frame
latency; "—" means no click changed a frame, so there was nothing to time.

A cell is in bold when Ljós loses by more than the run-to-run spread seen at this load: first
frame and memory when it is higher at all; idle CPU by more than 0.1 % of a core; driven CPU by
more than 10 % + 0.5 % of a core; lag p50 by more than 25 % + 4 ms, lag p95 by more than 25 % + 8 ms.

| example | first frame ms (Qt) | settled MiB (Qt) | idle CPU % (Qt) | driven CPU % (Qt) | lag p50 ms (Qt) | lag p95 ms (Qt) | clicks |
|---|---:|---:|---:|---:|---:|---:|---|
| calqlatr | 212 (330) | 55.0 (103) | 0.00 (0.00) | 2.1 (6.7) | 1.9 (18.1) | 583 (583) | 19 |
| clocks | 58.4 (264) | 49.8 (112) | 4.6 (19.9) | 4.6 (19.9) | 8.9 (8.6) | 15.5 (16.1) | 25 |
| hangman | 57.5 (308) | 44.8 (97.6) | 0.00 (0.00) | 0.22 (0.08) | — (—) | — (—) | 25 |
| maroon | 75.5 (450) | 75.0 (225) | 27.3 (67.1) | 39.1 (71.3) | 8.4 (8.5) | 14.2 (14.8) | 25 |
| rssnews | 68.1 (545) | 74.7 (861) | 0.01 (0.00) | 2.3 (10.9) | 512 (515) | 1923 (1924) | 25 |
| samegame | 65.5 (310) | 53.2 (116) | 5.7 (26.0) | 60.1 (180) | 8.7 (10.8) | 378 (300) | 25 |
| thermostat | 108 (397) | 98.2 (164) | 0.00 (0.00) | 2.3 (5.9) | — (—) | — (—) | 25 |
| todolist | 92.1 (354) | 65.2 (122) | 0.00 (0.00) | 2.1 (1.6) | 2572 (2572) | 2722 (2573) | 4 |
| photosurface | **359 (314)** | 63.6 (119) | **13.2 (5.8)** | 2.3 (6.6) | **859 (550)** | 3667 (3758) | 25 |
| alarms | 77.5 (319) | 54.4 (107) | 0.00 (0.00) | 0.35 (7.0) | 1.0 (2.1) | 47.1 (34.2) | 8 |
| snippet-borderimage-defaults | 59.6 (280) | 41.6 (98.9) | 0.00 (0.00) | 0.30 (0.07) | — (—) | — (—) | 25 |
| snippet-vertical-positioner-transition | 173 (269) | 40.8 (92.0) | 0.00 (0.00) | 0.26 (0.11) | 40.7 (40.8) | 40.7 (40.8) | 25 |
| snippet-flow-diagram | 71.3 (293) | 45.5 (101) | 0.00 (0.00) | 0.32 (0.08) | — (—) | — (—) | 25 |
| snippet-focus-column | 62.3 (272) | 40.7 (94.3) | 0.00 (0.00) | 0.36 (0.09) | — (—) | — (—) | 25 |
| snippet-focus-scopes | 59.4 (294) | 42.7 (103) | 0.00 (0.33) | 0.52 (0.43) | 192 (213) | 458 (456) | 25 |
| snippet-rectangle-smooth | 59.8 (478) | 41.7 (98.0) | 0.00 (0.00) | 0.46 (0.09) | — (—) | — (—) | 25 |
| snippet-texteditor | 165 (363) | 38.7 (173) | 0.00 (0.20) | 0.78 (0.70) | 206 (256) | 488 (461) | 25 |
| snippet-text-lines | 203 (437) | 41.1 (122) | 0.00 (0.00) | 0.38 (0.10) | — (—) | — (—) | 25 |
| snippet-components-application | 293 (439) | 41.2 (162) | 0.00 (0.00) | 0.46 (0.08) | — (—) | — (—) | 25 |
| snippet-properties-application | 85.8 (377) | 50.9 (119) | 0.00 (0.00) | 0.25 (0.08) | — (—) | — (—) | 25 |
| snippet-alias-application | 57.1 (278) | 42.2 (105) | 0.00 (0.00) | 0.40 (0.08) | — (—) | — (—) | 25 |
| snippet-property-signals | 53.0 (559) | 44.1 (129) | 0.00 (0.00) | 0.27 (0.09) | — (—) | — (—) | 25 |
| snippet-signals-basic | 147 (263) | 37.3 (96.0) | 0.00 (0.00) | 0.41 (0.10) | — (—) | — (—) | 25 |
| snippet-signals-parameters | 55.0 (412) | 38.1 (95.0) | 0.00 (0.00) | 0.36 (0.13) | — (—) | — (—) | 25 |
| snippet-signals-no-parameters | 47.9 (248) | 37.4 (96.1) | 0.00 (0.00) | 0.42 (0.14) | — (—) | — (—) | 25 |
| snippet-signals-connectslots | 50.3 (270) | 38.4 (96.5) | 0.00 (0.00) | 0.35 (0.11) | — (—) | — (—) | 25 |
| snippet-signals-connectdynamic | 290 (314) | 57.2 (95.0) | 0.00 (0.00) | 0.55 (0.14) | — (—) | — (—) | 25 |
| snippet-methods-app | 165 (455) | 40.7 (105) | 0.00 (0.00) | 0.51 (0.13) | — (—) | — (—) | 25 |

Ljós starts sooner and settles in less memory than qml6 in every example but one first frame.
Where it loses:

| example | loss | cause | owner |
|---|---|---|---|
| photosurface | first frame 359 against 314 ms; idle CPU 13.2 against 5.8 %; lag p50 859 against 550 ms | It is never idle: both runtimes keep animating the welcome PDF's slide show over a shader backdrop. Ljós draws that at about 4.4 frames a second on llvmpipe's raster threads, Qt at 6. The PDF page and the backdrop are rasterised on the CPU in Ljós's render path; the first frame, 45 ms later, is likely the same cost. | render |
| the static snippets (16 of them) | driven CPU 0.22–0.55 against 0.07–0.14 % of a core: inside the bold margin, but lost in every one | Per click the main thread runs about eight UI passes where qml6 wakes about three times (likely move, press and release, each followed by the extra pass egui asks for after input): 204 passes for 25 clicks under callgrind, about 74 000 instructions each. That is about 0.6 M instructions a click, yet 1–2 ms of main-thread user time at this load, most likely because each pass after a sleep runs with cold caches on a busy machine. Fewer passes per input that changes nothing, not cheaper ones, would close it. | shell pacing |
| samegame | lag p95 378 against 300 ms (inside the margin) | Its slowest clicks land in the logo's pause before the next animated frame, as in the section above: the wait is the animation's, in both runtimes. It varies with where in the pause a click lands, which is why it is inside the spread. | engine (animation timing) |
| alarms | lag p95 47.1 against 34.2 ms (inside the margin) | Eight clicks on the alarm list and its dialog; not investigated further, as it is inside the spread. The dialog's controls are the likeliest place. | controls |
| snippet-texteditor | lag p95 488 against 461 ms (inside the margin) | The medians favour Ljós (206 against 256 ms) and the 95th percentile is 6 % worse; not investigated further, as it is inside the spread. | text |
| todolist | driven CPU 2.1 against 1.6 %; lag p95 2 722 against 2 573 ms (inside the margins) | Four clicks; each waits for the next unrelated frame in both runtimes, so the lag is the frame's pace, not the click's. The CPU is the same per-click passes as the snippets. | shell pacing |

Many lag cells are equal in both runtimes (rssnews, todolist, the focus snippets): a click that
changes nothing on screen is timed to the next frame something else draws, which is the same in
both.

- **DECISION: only the desktop profile, three runs each.** Twenty-eight examples at three profiles
  would not finish inside the bench's time on a shared machine, and the desktop profile is the one
  the comparison with Qt is about.
- **DECISION: the per-click passes are left for now.** Skipping egui's pass after an input that
  changed nothing needs egui to say what it would have drawn, which is a change to how the shell
  paces egui and not something to fold into a benchmark batch. The bench now reports each window's
  CPU by thread name, so the next change can be measured on the main thread alone.

### The first frame of a scroll, scrolling against qml6, and an Image's completion (2026-10-18)

**The first frame of a scroll.** A list of 200 delegates on llvmpipe, scrolled once with the
wheel. The copies a scroll makes (the clip kept, the framebuffer it is moved through) are now
made once while the window is idle, not in the first frame of the scroll.

| First scroll, llvmpipe | Before | After |
|---|---:|---:|
| the clip kept, first copy | 13.1 ms | 0.15 ms |
| the clip moved back, first time | 15.8 ms | 0.8 ms |
| its two passes | 14.7 / 16.3 ms | 1.4 / 1.3 ms |

The copies cost about 44 ms once, at idle after the first frames. The scroll copy still moves
clips: on llvmpipe in 95 of 108 moving passes of photosurface and 139 of 161 of the list, and on
the GPU in 85 of 97 of photosurface. On the GPU the list moves none, because the whole run of
frames starts first, but each whole frame there costs 0.61 ms (median) and 1.25 ms (95th
percentile).

**Scrolling against qml6.** `mise run bench rssnews listview photosurface --profile desktop
--settle 3 --idle 3 --load 5 --gesture scroll`, `--warm-up --repeat 3`, medians, Ljós / Qt.

| | llvmpipe | GPU |
|---|---:|---:|
| rssnews, frames that changed a second | 62.0 / 61.0 | 61.6 / 60.6 |
| rssnews, slowest frame interval | 16.2 / 48.3 ms | 16.3 / 32.5 ms |
| rssnews, CPU while scrolled | 18.8 / 64.6 % | 6.47 / 5.91 % |
| listview, frames that changed a second | 62 / 62 | 61.9 / 61.8 |
| listview, CPU while scrolled | 31.7 / 23.1 % | 10.2 / 6.03 % |
| photosurface, frames that changed a second | | 55.2 / 57.5 |

- **photosurface on the GPU** had 300–880 ms frames in a first run and none in Qt's. The
  welcome slides turn a PDF page every 5 s, but a page draws in 1–3 ms. Run on its own,
  scrolled as the bench scrolls it, no frame went over budget (longest 14.6 ms). A second run at
  a load average of 72–124 on 32 cores stalled in one of three runs of each runtime (Ljós 350 ms,
  Qt 278 ms). The stalls are the machine's load.

**An Image's completion** (samegame's blocks): 41.7 k instructions before, 29.3 k after (57.9 k
and 45.9 k with its effect). A plain file name beside a plain document is joined as it is
(resolving the URL: 7.4 k to 2.7 k). A small picture of an item not yet painted is kept whole,
without first working out a size limit for it.

**What is left.** On the GPU, Ljós spends more CPU a frame than Qt (listview ×1.69). The cost is
per frame, not in the scene, as in the sections before.

### Where Ljós lost to qml6: listview's idle, the first frame, the binary's pages and rectangles (2026-10-18)

What "Startup and footprint against qml6: the binary, the glyph fallback and the module files"
left. `LJOS_CUA_GPU=/dev/dri/renderD128 BENCH_PROFILE=dist
BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench listview rectangles todolist thermostat
--profile desktop --settle 1 --idle 10`, before (a69b7ff8) and after (9708bc89), each in a target
directory and sandbox of its own. The machine was shared with other benches and builds: load
average 10–124, so first frames are rough, and idle CPU moves by a few points between runs of one
binary. Medians of two rounds of three runs each, qml6 of four. File PSS counts every mapped
file and moves with the page cache; the binary's own pages are measured on their own below.

| App | First frame, ms: before / after / qml6 | Idle CPU %: before / after / qml6 | Anon PSS, MiB: before / after / qml6 | File PSS, MiB: before / after |
|---|---:|---:|---:|---:|
| listview | 44 / 48 / 106 | 10.3 / 10.6 / 6.7 | 30.1 / 30.3 / 43.7 | 46.7 / 37.6 |
| rectangles | 60 / 50 / 87 | 0 / 0 / 0 | 45.7 / 43.3 / 42.9 | 43.1 / 41.8 |
| todolist | 72 / 86 / 187 | 0 / 0 / 0 | 41.3 / 40.5 / 55.0 | 63.9 / 54.7 |
| thermostat | 106 / 93 / 216 | 0 / 0 / 0 | 55.4 / 53.8 / 62.2 | 45.4 / 44.2 |

**listview's idle CPU is not idle.** `bench/listview.qml` scrolls its list with a looping
animation, so "idle" is 10 s of drawing at 60 frames a second (597 frames in the window, against
qml6's 615), and the figure is the cost of an animated scroll. Under callgrind the scroll's binding
churn (a QtTest stand-in that scrolls the same list for 10 s) went from 2 637 M to 2 595 M
instructions (−1.6 %) with the corners below. What is left is spread over the frame: the reads
of each delegate's bindings, layout and painting, none of it above a few percent. It stays above
qml6 on the GPU (10.6 against 6.7 %) (on llvmpipe it was below qml6, 21.5 against 23.1 %). DECISION: not cut
further here; the frame is the work, not a waste in one place.

**The first frame.** Timed from exec, in a quiet stretch, on the GPU: thermostat registers its
types by about 18 ms, then waited until 28 for the compile thread, made its objects in 30 ms, laid
out ahead (9 ms) and had its device at about 79. With `--example`, the compile thread first built a
full engine of the standard modules and described all its types (13 + 8 ms) to compile against,
for every example. An example that registers no types of its own (todolist, thermostat, and all
but six) now compiles against the type table, as any program does: the entry is compiled at about
9 ms in place of 30, setup no longer waits, and objects are made by 51 ms in thermostat (59.6
before, best of three) and 42 in todolist. The warnings of the 28 corpus apps with arguments are
the same before and after. What the first frame still costs: registration (13–18 ms), making the
objects (25–30 ms: the JS host's first start through `Qt.locale()` 13 M instructions, parsing each
file's JS module 4 M, fontconfig's substitution 13 M, synchronous picture decodes 20 M in
thermostat, as Qt's Image is when not `asynchronous`), and the layout ahead (6–10 ms). Through this
load the bench sees thermostat's median fall from 106 to 93 ms and not todolist's (72 → 86, runs
of 60–167 ms).

**The binary's file pages.** In the order the linker gets them, crate by crate, the 12.9 MiB of
code the apps run (12 415 functions) lies over nearly all of the binary's 48.3 MiB, and every 4 kB
page with one function an app enters stays mapped, with the kernel's read-around. `mise run
bench:hot` runs the 33 bench apps under callgrind for 15 s each and writes the functions of the dist
binary they entered to `crates/ljos_cli/link/hot.txt`; the dist link (`--symbol-ordering-file`,
from `ljos_cli`'s build script) puts them first, together in 13.0 MiB. The binary's own PSS, `dist`,
GPU sandbox, 6 s after start, two runs each, unordered then ordered:

| App | PSS, MiB: unordered → ordered | Code (r-xp), MiB |
|---|---:|---:|
| todolist | 36.7–37.0 → 21.5–22.3 | 29.6–30.0 → 14.4–15.1 |
| thermostat | 34.5–34.7 → 20.9–21.2 | 27.8–28.1 → 14.0–14.1 |
| calqlatr | 33.4–33.5 → 20.9 | 27.0 → 13.9–14.0 |

Against Qt's 17.5–17.8 MiB for its 47 library files, Ljós's one file is now 21–22 MiB. Only `dist`
is ordered: the names are recorded from its binary, and it is the one handed out. An app that runs code no bench app runs pays for those pages as before.

**Rectangles' memory.** A Rectangle bound its four corner radii and its antialiasing to `radius`,
and every Item its `childrenRect`, as it was made: graph nodes and bindings for values most programs
never read. They are bound now on the first read or change handler (`bind_on_read` in the engine),
and until then a read gives what the binding would; `features/items/rectangle-corners.feature`
holds the behaviour. Rectangles' anon PSS on the GPU went from 45.7 to 43.3 MiB, against qml6's
42.9.

**Where Ljós still loses to qml6.**

- **An animated scroll** (listview): 10.6 % CPU on the GPU against 6.7 %.
- **Rectangles' memory**: 43.3 MiB of anon PSS against 42.9, within a run's spread now.
- **The binary's file pages**: 21–22 MiB against Qt's 17.5.
- **Startup** is not lost: it is about half of qml6's on these four apps, and todolist and
  thermostat no longer wait on the compile thread.

### B14: animations below qml6 on the GPU, clocks' native work (2026-10-18)

Instructions retired a second, all threads of the process, sampled by the hardware counter
(period 20 k) over 8 s after a 40 s warm-up (50 s for clocks). The apps ran off-screen in the
cua sandbox on the GPU (`/dev/dri/renderD128`) and on llvmpipe (`LIBGL_ALWAYS_SOFTWARE=1`).
Qt is `qml6` 6.11.2 with the Basic style. Three runs each; 1 M/s is about 16.7 k instructions
a frame at 60 fps.

| | Batch start | After (ddf8d843) | qml6 |
|---|---:|---:|---:|
| animations, GPU | 50.9 M/s | 42.18 / 42.27 / 42.29 | 43.56 / 43.61 / 43.58 |
| animations, llvmpipe | 1589 (see below) | 469.8 / 470.9 | 1040 / 1041 |
| clocks, GPU | 20.6 M/s | 20.84 / 20.69 | 14.79 / 14.96 |

Callgrind on the main thread, animations on the GPU, instructions a frame: 790 k at 3ca8044b
and 773 k at 4a21b3f7. At the start the tick was 214 k, compose 100 k, encode 105 k and the
swap about 80 k.

- **Animations are 3 % below qml6.** The tick lost the most:
  - an animated number is stored as it is written, and its node is looked up once a write;
  - a number is read without making a `Value`, and an item's x and y are read together;
  - an item's transform is set as its place is read, and an identity transform is known by
    nine comparisons;
  - once a frame is asked for, the other runs are not looked at again.

  Compose and encode then lost:
  - a leaf is composed without sorting its children, or without a span when it has none;
  - a leaf with no content clip takes its parent's;
  - a moved node marks its ancestors in one walk;
  - a solid rectangle waits as its clip, rect and colour until it is drawn;
  - a rectangle's four edges are checked for whole pixels at once;
  - `ceil` is done without libm.

  On the shell side:
  - small rectangles are joined into a mesh in one pass;
  - the scene's meshes go to the painter as they are;
  - the last picture keeps the shapes.
- **Callgrind is not the goal.** It overcounts the driver's `rep movs` copy in
  `eglSwapBuffers` (64 k a frame for clocks). The hardware total is what is compared. After the
  batch, the animations process splits as follows:
  - the binary: 38.6 M/s;
  - libgallium: 2.3;
  - libc: 1.7;
  - wayland: 0.6;
  - the smithay-clipboard thread: 0.6.
- **llvmpipe.** The batch started from a base that drew every frame whole on llvmpipe
  (1589 M/s); 944455e8 fixed it. The rest of the batch kept it at 470–486 against the 472 → 454
  of earlier sections, under half of Qt's 1040.
- **Clocks stays above qml6.** At 370 k a frame (580 frames):
  - `QuickScene::frame` takes 112 k:
    - the timers take 48 k, of which 45 k is the script `timeChanged` handlers;
    - the animation tick takes 35 k, of which `list_effect` (the hands' `Rotation`s) is 20 k;
    - compose takes 18 k and encode 15 k.
  - The paint takes 141 k, nearly all of it the swap's copy.
  - `clip_to` takes 14 k.

  By the hardware counter the script share (QuickJS and `ljos_js`) is about 3.3 M/s. That
  belongs to the script work this batch (ilang), and nothing here touches `ljos_js`. Without
  it, Ljós would be at about 17.5 M/s against 14.9. The native cuts left are each under
  1 M/s:
  - `of_transform`'s field reads: 13 k a frame;
  - `clip_to`'s bounding rectangles: 6 k;
  - `refresh_bounds`: 7 k.

  The remaining gap is the driver and the swap (gallium 3.2, libc 2.2 M/s), not the scene.

DECISIONs:
- The smithay-clipboard thread stays. Data-control would break the clipboard on GNOME.
- Encode does not merge solid rectangles, because damage would get coarser.
- `refresh_bounds` keeps its NaN-safe union.
- `write_number` keeps its two store lookups. Holding the store across `settled` can panic, for
  about 6 k a frame.
- Reusing an unchanged mesh across frames does not apply to animations, since every bar moves
  every frame.
- The AT-SPI tree update seen in the sandbox is the cua driver asking for the tree. It is not
  the app's cost.

### A list's rows on llvmpipe: clears that were callbacks (2026-10-18)

On Mesa's llvmpipe, listview cost 31.7 % CPU against qml6's 23.1 %. Per thread over 8 s, the
llvmpipe threads (the fragments) were close (17.9 against 16.4 %), and the gap was Ljós's main
thread (13.4 %) against qml6's render and GUI threads (5.1 + 4.5 %). llvmpipe does a draw's vertex
work, binning and state checks on the thread that draws: about 45–66 k instructions a draw.

**Rows were a draw each.** `fill_rects` draws an opaque rectangle as a scissored clear through a
paint callback, and egui_glow sets its whole GL state up before and after every callback, then
starts a new draw for what follows. Each row of the list (a full-width Rectangle and its Text) was
a callback, its state, and a draw for its text: under callgrind (`--instr-atstart=no`, 10 s of
scrolling), `GlPainter::paint` took 2.90 M instructions a frame, with 20 `paint_primitives` a
frame. The clears are now a `Clear` the meshes' painter draws itself (a scissor and `glClear`),
and each is moved back, at most 256 shapes, past the shapes whose bounds it does not touch;
another callback stays a barrier. The rows clear first and their texts are one draw:
`GlPainter::paint` 1.87 M a frame (−35 %), no `paint_primitives` left.

Old and new binaries interleaved in one sandbox (`ljos run --mcp bench/listview.qml`, 12 s,
`app_metrics`), medians of three: frame time p50 2.48 → 1.64 ms, mean 1.38 → 1.01 ms.
`mise run bench listview photosurface --profile desktop --settle 3 --idle 3 --load 5 --gesture
scroll`, `--repeat 3`, medians, load average falling 100 → 25 through the after run (72–104 before):

| App | CPU scrolled %: before / after / qml6 after | fps: before / after / qml6 after |
|---|---:|---:|
| listview | 42.2 / 31.7 / 23.0 | 58.6 / 60.5 / 61.8 |
| photosurface | 9.6 / 13.0 / 46.1 | 54.8 / 56.4 / 57.6 |

The load moved qml6 too (26.2 → 23.0 % on listview), so the bench's CPU is not the measure of
this change; the frame times are. photosurface scrolls at qml6's rate here for a fifth of its CPU.

**For the GPU path (H2), not done here.** Per frame on the main thread, after this change:

- Each draw still costs the driver's validation; listview has about 20 a frame (the clip and the
  glyph atlas against pictures split them). Fewer, larger draws help any driver.
- Every paint callback other than a clear still makes egui_glow set up its whole state twice.
- `swap_buffers_with_damage` costs about 260 k instructions a frame, and the vertex and index
  uploads about 130 k a call.
- The scene: a delegate made costs about 115 k instructions in polish, reconcile and adopt, and its
  Text's polish about 101 k; the bench makes and drops about eight a frame at 60 frames a second.

### Script gaps to qml6, third pass: clocks' dispatch and calqlatr's children (2026-10-18)

The bench copies and runs of the two sections above. The base is the "after" of the second pass
(d970be89). Ir is from callgrind on a release build: clocks per `timeChanged` (700 calls),
calqlatr per press (140 presses). Wall time is the fastest of 8 runs, alternating Ljós and
`qml6` 6.11.2, with Date on the machine's clock (`--wall`) for Ljós. Other work kept the load
average at 39–64, so the wall rows are only good to a few per cent.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| clocks, Ir per `timeChanged` | | 37.1 k | 12.8 k |
| clocks, wall per `timeChanged` (140 000 calls) | 0.66 µs | 2.50 µs | 0.68 µs |
| calqlatr, Ir per press | | 2.02 M | 1.56 M |
| calqlatr, wall per press (2 800 presses) | 0.102 ms | 0.169 ms | 0.136 ms |

**clocks.** Neither app is below qml6 yet. Clocks is within the noise: 95.5 ms against 92 ms
for the run. The cuts, in order:
- A QML method with plain parameters is the exported function itself, with no closure and no
  `apply` per call: 37.1 k → 32.5 k Ir.
- Property accessors and host globals are QuickJS C functions (`JS_NewCFunctionData`) and not
  rquickjs closures: 32.5 k → 27.0 k.
- The wrappers have a QuickJS class of their own, so one `JS_GetOpaque` is the type check, and
  primitives cross by their tag: 22.9 k → 20.3 k.
- A write of the value a property already holds is no write. `Date` has nothing in front of it
  unless the clock is pinned.
- A plain read is one look at the object, and a plain write asks no map whether to forward or
  intercept.
- A direct method is a C function holding its bound arguments: 17.96 k → 16.38 k. Its
  arguments are on the stack.
- A number or bool is read and written with no `Scope`, no `JsValue` and no conversion:
  15.37 k → 13.96 k.
- The wrapper answers its type's properties itself (`host/answer.rs`), about 14.0 k → 12.8 k.
  Its class is exotic, so QuickJS asks it for a name it does not hold. It finds the name among
  its type's slots by atom and reads or writes with no accessor call. A name with no slot walks
  the prototypes as before, and a method or signal that took a property's name drops its slot.
  `JsHost::accessor_calls` counts what still calls an accessor
  (`the_wrapper_answers_its_properties_with_no_accessor_call`).

What is left of a call: the interpreter (about 3 k), `Date`'s field split (about 1.2 k), the
slot lookups (about 100 Ir each), and the engine's plain read and write (about 500 each).

**calqlatr.**
- `children`, `resources` and `data` are a live list (`live_list.rs`): a QuickJS object that
  asks the engine for its length and entries, not a copy of the list for each read
  (`features/items/children-list.feature`, @qt-parity).
- A script's read of an object or a field kept on the object takes the quick way.
- A Behavior's easing curve is read by index from its group.

What is left per press is mostly the 33 writes and the bindings they flush: about 35 % of the
press, of which the graph's flush is 0.64 M. The 11 Behavior starts are about 40 k together,
with no allocation of note left. `tint::tinted` is about 15 k, in the image store.

DECISIONs:
- A live list over a copy kept until the children change. It is what Qt has: a held list stays
  right, and a script that mutates the list is told what Qt tells it.
- An exotic wrapper class over an extra prototype level of plain data, which would have to be
  kept in step with every write.
- A write the wrapper refuses throws a TypeError whenever QuickJS says the write may throw.
  Ljós runs scripts strict.
- One object of the live list's prototype is kept, so QuickJS reuses its shape.

### calqlatr's press, fourth pass: lookups by name and by type (2026-10-18)

The bench copy and run of the section above. The base is its "after" (5614814f). Ir is from
callgrind on a release build, per press (140 presses). Wall time is per press over 2 800
presses, 5 runs of Ljós next to 3 of `qml6` 6.11.2. Other work kept the machine loaded, and
`qml6` itself ranged from 0.098 to 0.125 ms, so the wall rows are only good to about 10 %.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| calqlatr, Ir per press | | 1.558 M | 1.481 M |
| calqlatr, wall per press (fastest run) | 0.098–0.103 ms | 0.130 ms | 0.120–0.124 ms |

Ljós is still above qml6, by about 20 %. The cuts, in order:
- A tier1 program keeps, per name it reads, what the name was on the last types it met. It looks
  a member or a script method up by name only when the type changes: 1.558 M → 1.524 M.
- The field reads of an object find a reader's entries by an index of property and reader, not
  by looking through all the object's entries: 1.524 M → 1.507 M. A counting test keeps it
  (`many_readers_of_one_property_each_keep_their_two_entries`).
- The derive finds a native part in one call (`QmlObject::part`), where the walk asked each
  level its type and then its base: 1.507 M → 1.491 M.
- An easing reads `bezierCurve`, a list made anew at each read, only for a `BezierSpline`:
  1.491 M → 1.486 M.
- `is_a` asks the object's type without cloning it (`Engine::inherits`): 1.486 M → 1.481 M.

None of these changes what a script sees, so there is no new scenario. The animation, bindings
and language features pass unchanged.

What is left of a press, about 1.48 M Ir:
- The ListView's model and view work, about 0.45 M (30 %): `append` about 137 k, the views
  pass about 178 k, `positionViewAtEnd` about 79 k, `clear` about 58 k.
- About 11 buttons change `dimmed` a press. Each one re-runs three colour bindings in the tier1
  VM (5–6 k each) and starts a Behavior's ColorAnimation (about 13 k).
- `updateDimmed` reads `grid.children` about 95 times. Each read is a new wrapper, as in qml6,
  where `children === children` is false.

DECISIONs:
- The ListView's share is left to the lane that owns the view.
- `children` keeps returning a new wrapper at each read, as qml6 does.
- An animation looks up its target's property again when it starts. Its `properties` may
  change between the build and the start.

### Two passes a click, and the gallery against qml6 again (2026-10-18)

A click that changes nothing on a static snippet ran six to eight UI passes in Ljós (204 passes for
25 clicks under callgrind); qml6 wakes about three times. egui asked for two more passes after each
pass with input, and one more after each pass that asked for another at once. Those passes serve
egui's own widgets, which answer a frame late. The scene answers in its own pass, so the extra
passes drew nothing. Now the shell schedules only what the scene asks for, and a click is two
passes: the press, which arms press-and-hold, and the release. A press-and-hold timer stopped
before its 800 ms no longer leaves a wake behind. `features/window/passes-per-click.feature` pins
this on the desktop sandbox.

The gallery was run again as in "The whole gallery against qml6" above, with the same 28 examples,
command and bold rule, and one more column. Before each chunk of four, the runner waited up to
1 200 s for a 1-minute load average under 16. Ljós is the release build of the pass change (bdf5998c), built on an older base for chunks 1–6 and
on the current one for chunk 7 (the last four snippets). The machine never stayed
under 16: chunk 2 started at 64.2 after the full wait, and the runs reached load averages of
7.0–117.4. The new column, "driven busiest thread", is the CPU of the one thread that worked
hardest while driven. Qt splits a frame between its GUI thread and `QSGRenderThread`, while Ljós
does script, layout and the frame's recording on one thread.

| example | first frame ms (Qt) | settled MiB (Qt) | idle CPU % (Qt) | driven CPU % (Qt) | driven busiest thread % (Qt) | lag p50 ms (Qt) | lag p95 ms (Qt) | clicks |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| calqlatr | 62.6 (332) | 51.7 (102) | 0.00 (0.00) | 2.3 (7.4) | 1.8 (2.2) | 2.0 (18.2) | 583 (583) | 19 |
| clocks | 81.6 (313) | 54.7 (105) | 6.0 (21.1) | 6.2 (21.8) | **4.1 (3.2)** | 8.4 (8.6) | 15.7 (15.9) | 25 |
| hangman | 167 (337) | 68.0 (124) | 0.00 (0.00) | 0.20 (0.09) | 0.17 (0.06) | — (—) | — (—) | 25 |
| maroon | 111 (471) | 65.5 (134) | 32.4 (61.2) | 36.9 (68.9) | **8.1 (4.8)** | 7.7 (8.1) | 15.5 (15.3) | 25 |
| rssnews | 105 (499) | 104 (833) | 0.00 (0.00) | 1.8 (7.0) | 0.95 (0.88) | 514 (515) | 1 924 (1 923) | 25 |
| samegame | 73.9 (345) | 56.2 (127) | 4.8 (27.4) | 65.1 (191) | 14.2 (21.3) | 8.5 (9.3) | **34.1 (15.8)** | 25 |
| thermostat | 115 (948) | 85.5 (190) | 0.00 (0.00) | 2.4 (5.9) | 2.2 (2.0) | 1 707 (—) | 2 689 (—) | 25 |
| todolist | 90.8 (610) | 60.9 (160) | 0.00 (0.00) | **2.6 (1.5)** | **0.96 (0.29)** | 2 572 (2 572) | 2 654 (2 572) | 4 |
| photosurface | 190 (313) | 61.6 (129) | **12.2 (6.0)** | 2.9 (4.3) | 0.82 (0.39) | 456 (927) | 3 834 (3 749) | 25 |
| alarms | 73.5 (335) | 53.8 (109) | 0.00 (0.00) | 0.36 (7.0) | 0.29 (1.6) | 1.2 (2.1) | 47.4 (34.4) | 8 |
| snippet-borderimage-defaults | 56.0 (269) | 41.9 (96.6) | 0.00 (0.00) | 0.16 (0.08) | 0.14 (0.05) | — (—) | — (—) | 25 |
| snippet-vertical-positioner-transition | 65.5 (264) | 39.1 (92.0) | 0.00 (0.00) | 0.21 (0.10) | 0.19 (0.07) | 40.9 (40.9) | 40.9 (40.9) | 25 |
| snippet-flow-diagram | 53.7 (287) | 43.0 (103) | 0.00 (0.00) | 0.18 (0.08) | 0.16 (0.05) | — (—) | — (—) | 25 |
| snippet-focus-column | 55.3 (253) | 42.1 (100) | 0.00 (0.00) | 0.15 (0.07) | 0.13 (0.05) | — (—) | — (—) | 25 |
| snippet-focus-scopes | 61.2 (280) | 52.4 (101) | 0.00 (0.42) | 0.32 (0.48) | 0.28 (0.10) | 184 (173) | 456 (466) | 25 |
| snippet-rectangle-smooth | 65.6 (270) | 50.9 (97.3) | 0.00 (0.00) | 0.18 (0.08) | 0.16 (0.06) | — (—) | — (—) | 25 |
| snippet-texteditor | 54.3 (287) | 43.1 (99.3) | 0.00 (0.22) | 0.39 (0.70) | 0.34 (0.20) | 183 (173) | 457 (464) | 25 |
| snippet-text-lines | 53.7 (494) | 43.1 (145) | 0.00 (0.00) | 0.16 (0.08) | 0.14 (0.06) | — (—) | — (—) | 25 |
| snippet-components-application | 89.7 (594) | 81.9 (128) | 0.00 (0.00) | 0.21 (0.14) | 0.19 (0.09) | — (—) | — (—) | 25 |
| snippet-properties-application | 99.8 (289) | 49.1 (105) | 0.00 (0.00) | 0.17 (0.07) | 0.14 (0.05) | — (—) | — (—) | 25 |
| snippet-alias-application | 53.2 (454) | 44.2 (105) | 0.00 (0.00) | 0.17 (0.10) | 0.14 (0.06) | — (—) | — (—) | 25 |
| snippet-property-signals | 59.8 (252) | 49.0 (92.4) | 0.00 (0.00) | 0.15 (0.08) | 0.13 (0.05) | — (—) | — (—) | 25 |
| snippet-signals-basic | 50.1 (269) | 41.1 (95.7) | 0.00 (0.00) | 0.21 (0.10) | 0.19 (0.07) | — (—) | — (—) | 25 |
| snippet-signals-parameters | 55.9 (433) | 41.9 (95.1) | 0.00 (0.00) | 0.27 (0.11) | 0.24 (0.08) | — (—) | — (—) | 25 |
| snippet-signals-no-parameters | 45.6 (214) | 41.4 (95.9) | 0.00 (0.00) | 0.30 (0.12) | 0.27 (0.09) | — (—) | — (—) | 25 |
| snippet-signals-connectslots | 48.1 (256) | 40.1 (96.9) | 0.00 (0.00) | 0.33 (0.10) | 0.30 (0.07) | — (—) | — (—) | 25 |
| snippet-signals-connectdynamic | 54.1 (269) | 42.4 (96.8) | 0.00 (0.00) | 0.31 (0.11) | 0.27 (0.08) | — (—) | — (—) | 25 |
| snippet-methods-app | 188 (255) | 41.9 (96.0) | 0.00 (0.00) | 0.42 (0.10) | 0.39 (0.07) | — (—) | — (—) | 25 |

**Static snippets.** Driven CPU went from 0.22–0.55 % of a core to 0.15–0.42 %, against qml6's
0.07–0.14 %. All four snippets above 0.27 % are in chunk 7, which ran at another time and load.
Ljós still loses in every one, inside the bold margin. What is left is the cost of each of the two
passes after a sleep, not the number of passes.

Where Ljós loses:

| example | loss | cause | owner |
|---|---|---|---|
| photosurface | idle CPU 12.2 against 6.0 % | As in the section above: both runtimes keep animating the slide show over a shader backdrop, and Ljós rasterises the PDF page and the backdrop on llvmpipe. | render |
| todolist | driven CPU 2.6 against 1.5 %; busiest thread 0.96 against 0.29 % | Two of the four clicks start a page transition. Over each, Ljós drew 32–44 frames at 32 ms intervals and qml6 drew 30 at 16 ms. Ljós's llvmpipe threads used about 160 ms rasterising them, against 30–40 ms for Qt's. Ljós's own thread used 72–115 ms, about what qml6's GUI and render threads used together (42–64 ms). This is raster cost per animated frame, not click passes. | render |
| samegame | lag p95 34.1 against 15.8 ms | Ljós's four llvmpipe threads ran 1.3–1.7 s each in a 12 s drive, and qml6's ran 0.8–1.1 s. Its slowest frames are those the raster threads finish late, the same per-frame cost as todolist's. One Ljós run reached 72.7 ms. | render |
| maroon | busiest thread 8.1 against 4.8 % (driven CPU 36.9 against 68.9 %) | The busiest threads are llvmpipe's. Ljós's work goes to four raster threads of about 1 s each, while qml6's is spread over more threads of about 0.34 s. In total Ljós uses half the CPU. | render |
| clocks | busiest thread 4.1 against 3.2 % (driven CPU 6.2 against 21.8 %) | Ljós's one thread does what qml6 splits between its GUI thread (about 0.1 s) and `QSGRenderThread` (0.38–0.51 s), and it does less in total. | shell (threading) |
| the static snippets | driven CPU 0.15–0.42 against 0.07–0.14 % (inside the margin) | Two passes a click now. Each costs 1–2 ms of main-thread time after a sleep on a busy machine. | shell pacing |
| alarms | lag p95 47.4 against 34.4 ms (inside the margin) | Unchanged from the section above. Not investigated, as it is inside the spread. | controls |

snippet-texteditor no longer loses (p95 457 against 464 ms). Its lag is set by the cursor's blink:
a click that changes nothing on screen is timed to the next blink frame, 500 ms apart in both
runtimes. Where in that phase a click lands sets its lag, which is why its p95 swings either way
within the spread. thermostat now times clicks in Ljós (p50 1 707 ms) that changed no frame in
qml6. The lag is the next frame an animation draws, not the click's.

- **DECISION: chunk 7 was run on the rebased change.** The first try of that chunk hit the
  30-minute limit on background commands. The change is the same on both bases, so the chunk was
  re-run on the rebased tree instead of rebuilding the older one.
- **DECISION: the table was published at loads above 16.** The machine is shared, and waiting
  longer did not bring the load down. The bold rule's margins were set at loads like these
  (4.8–119.6 last time).
- **DECISION: the render losses are reported, not fixed here.** todolist, samegame, maroon and
  photosurface all lose on raster cost per animated frame on llvmpipe, a render-path change
  outside this batch.

### Software GL: listview's draws, todolist's pages and the scroll copies (2026-10-19)

All runs use the off-screen sway sandbox. GL is Mesa's llvmpipe, and Qt is `qml6` 6.11.2 with
`QT_QUICK_CONTROLS_STYLE=Basic`. "llvmpipe" is the CPU of the process's `llvmpipe-*` threads.
The base is 5614814f, and the builds are release. The load average was 6 to 60 from other work.
Each cell is the range of three runs.

| bench/listview.qml, scrolling, per second | base | draws only | all | Qt |
|---|---:|---:|---:|---:|
| CPU (ms) | 225–287 | 215–278 | 118–163 | 216–228 |
| llvmpipe (ms) | 131–165 | 140–160 | 15–23 | 163–165 |

| todolist, gear then back (2 s, `ljos bench` clicks) | base | all | Qt |
|---|---:|---:|---:|
| CPU (ms) | 234–266 | 177–202 | 114–170 |
| llvmpipe (ms) | 142–162 | 45–51 | 71–124 |
| Frames shown, median interval (ms) | 54, 16.1 | 54, 16.2 | 19–30, 16.2 |
| 95th percentile interval (ms) | 16.2 | 16.3 | 44–333 |

| photosurface `surface` and `grid` (2026-10-18 section), `--gesture scroll` | before | after |
|---|---:|---:|
| surface: frames shown per second | 25.4 | 51.1 |
| surface: median interval, median input-to-frame (ms) | 32.4, 21.4 | 16.2, 10.5 |
| surface: CPU while driven (% of a core) | 187 | 96 |
| grid: CPU while driven (% of a core) | 53 | 14 |

- **Draws.** A row cut to a few changed pixels counted as a whole row and became a clear. Now
  it is a quad of the pixels that show. Flat colour joins any draw, and a ShaderEffect skips
  egui_glow's state setup. listview went from 17 draws a frame to 2. That alone does not change
  llvmpipe's time: the pixels are the cost, not the calls.
- **Pictures handed over again.** Glyphs filled into the atlas during a page slide made every
  frame whole. Now the atlas sends only the boxes it filled. The painter redraws only the shapes
  whose triangles sample those texels (`present.rs` tests,
  `b0_hands_over_only_the_glyphs_it_fills`).
- **The scroll copies.** The kept clip was an RGBA renderbuffer, the other way up from the
  window's BGRA buffer, so llvmpipe ran a shader for each copy. That cost more than drawing the
  clip again. The clip is now a BGRA texture kept top row first, and both copies are row copies.

**DECISIONs.**
- The clip is copied cheaply rather than skipped on a software rasteriser. Never scrolling on
  llvmpipe gave about the same todolist numbers (44–55 ms) but would give up the copies that
  photosurface's scroll was measured to need when they came in (d44ff1a1).
- A driver that refuses a BGRA texture gets the RGBA renderbuffer, which says so with a GL error.
  No extension is looked up, because Mesa's desktop context takes BGRA without advertising it.

**What is left.** todolist's pop waits about 59 ms for its first frame, on the engine side.
Ljós's CPU outside llvmpipe (about 130 ms for the push and pop) is still above Qt's total. Qt
gets there by showing about half the frames.

### calqlatr's press, fifth pass, and clocks again (2026-10-19)

The bench copies and runs of the third and fourth passes above. The base is the "after" of the
fourth pass (716c34e6). Ir is from callgrind on a release build, per calqlatr press (140 presses) and per
clocks `timeChanged` (700 calls). Wall time alternates Ljós and `qml6` 6.11.2, five runs each:
calqlatr per press over 2 800 presses, clocks for 140 000 calls. The load average was 12–35.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| calqlatr, Ir per press | | 1.481 M | 1.410 M |
| calqlatr, wall per press | 0.101–0.105 ms | 0.120–0.124 ms | 0.114–0.118 ms |
| clocks, Ir per `timeChanged` | | 12.8 k | 12.9 k |
| clocks, wall for 140 000 calls | 94–103 ms | | 97–103 ms |

calqlatr is about 12 % above qml6. Clocks is within the noise, about 3 % above at the fastest
runs. The cuts, in order:
- `positionViewAtEnd` on a view already at its end lays the rows out once, not twice. The view
  keeps the items it read the lengths of; a pass that leaves the same ones, at the same place,
  needs no second run (`going_where_the_view_already_is_lays_the_rows_out_once`):
  1.481 M → 1.439 M.
- A Behavior's animation is built anew at each value it hears. The build asks the provenance of
  `target`, `targets`, `property`, `properties`, `from`, `to`, the easing leaves and `loops`,
  and takes the type's default for one never given a value: 1.439 M → 1.410 M.
  `features/animation/behavior-no-change.feature` (@qt-parity) pins what a Behavior does with
  a value that changes nothing.

What is left of a press, about 1.41 M Ir:
- 50 bindings run a press: 34 are the buttons' colours, about 6 k each with the flush, and 16
  are a new row's delegate. Of the colours, 23 give back the colour they had. The colour has to
  be worked out to know that, and qml6 works it out too.
- 11 Behavior starts, about 13 k each with the intercept. Most are a restart towards the colour
  the text still shows, since no frame runs between presses. It runs its whole duration, as on
  qml6, so the run, its curve and its slots are built all the same.
- A delegate made for `append` is about 167 k, and `clear` about 810 k for a round's rows.
- The script itself, about 0.13 M in the interpreter.

DECISIONs:
- The first step of a Behavior's run still writes the value it starts from. Skipping that write
  would leave the property unassigned until the first frame.
- `empty_row` still runs a removed row's bindings that are out of date. Qt has them evaluated
  already.

### B15: scrolling on the GPU against qml6 (2026-10-19)

Two measures, both off-screen in the cua sandbox on the GPU (`/dev/dri/renderD128`), with Qt as
`qml6` 6.11.2 and the Basic style:

- **Instructions retired a second.** All threads of the process, counted by the hardware
  counter over 8 s while an injected wheel turns one notch every 50 ms. Load on the machine
  does not move it. listview is `bench/listview.qml`, a ListView of 10 000 rows that scrolls
  itself.
- **CPU while driven.** `ljos bench --gesture scroll --warm-up --repeat 3 --profile desktop
  --settle 3 --idle 3 --load 5` over rssnews, listview and thermostat, as in "The first frame
  of a scroll, scrolling against qml6…" above. This one moves with load: the same binary gave
  listview 10.2 % against qml6's 6.03 % in that section and 13.0 against 10.8 here.

| Scroll, M instructions/s | Batch start | After (35eb0b56) | qml6 |
|---|---:|---:|---:|
| thermostat | 1131 | 130 | 125 |
| rssnews | 106.9 | 46 | 51.9 |
| listview | 309 | 270 | 99.9 |

| Scroll, CPU % of one core | Batch start | After 599c9c21 (qml6 same run) | After 35eb0b56 | qml6 (same run) |
|---|---:|---:|---:|---:|
| thermostat | 26.2 | 18.9 (6.67) | 2.72 | 4.78 |
| rssnews | 4.95 | 4.71 (4.54) | 5.33 | 4.97 |
| listview | 17.7 | 13.0 (10.8) | 10.6 | 7.09 |

rssnews' instruction count is bimodal on one binary (about 96 or 120 M/s before the last cut),
and its CPU % sits within noise of qml6's.

What each commit took, by the hardware counter, thermostat / rssnews / listview:

| Commit | thermostat | rssnews | listview |
|---|---:|---:|---:|
| start | 1131 | 106.9 | 309 |
| 71bd8ad9, 7cd5dfad, 599c9c21: a run's field reads in one entry; a text's inputs in one read; GPU solid rectangles drawn with the mesh, never cleared | 814 | ~100 | 275 |
| b1bbd2f0: scrolled content is an a11y space, a scroll moves the space | 244 | 102.7 | 276 |
| 9035d67c: a shape of another kind is not compared as moved | 222 | 102 | 276 |
| 43da9d6a: a scroll looks only at the items that came or went | 199 | ~97 | 276 |
| c9e8f3f1: text that only moved keeps its quads, moved | 187 | 83 | 271 |
| 35eb0b56: the scripts' heap is measured once a rest is long | 130 | 46 | 270 |

Found along the way:

- **The a11y tree was rebuilt on every scroll.** Every row's bounds changed, so the whole tree
  went to AT-SPI. Now the content item is a space of its own and a scroll moves only the space.
- **`Engine::collect_if_idle` ran at every rest between wheel notches.** It calls
  `JS_ComputeMemoryUsage`, which walks the whole heap. That was 23 % of thermostat's main
  thread and 32 % of rssnews'. It now runs once, after 100 ms with nothing to do.
- **Text that only moved was shaped into a new mesh.** Its quads are now translated instead.
  `glyph_meshes_built` stays at zero for a scroll by whole device pixels.
- **Every content effect was looked at on every frame of a scroll.** Now only the symmetric
  difference of the visible set is.

**listview stays at 2.7× qml6's instructions.** Callgrind of 348 frames on the main thread
(35eb0b56) shows that 4982 delegates were made and dropped, about 8 a frame at 60 fps. Each
one costs about 480 k instructions, against about 200 k for qml6 by the same rate. The split
per delegate:

- polish: 170 k, of which text layout is 62 k (parley shaping 37 k, its builder 28 k) and the
  batch flush 30 k;
- adoption: 113 k, the first runs of its effects:
  - text polish: 30 k;
  - anchors: 26 k and 8 k;
  - geometry: 19 k;
  - inherited properties: 5–10 k;
- the animation tick's share: 71 k;
- the frame's reconcile: 65 k;
- the incubator's creation job: 44 k;
- deferred deletes: 33 k;
- content effect installs: 21 k;
- the views' refresh: 17 k;
- a11y sync: 7 k;
- encode: 10 k;
- about 42 graph nodes are made and disposed.

No function is a hotspot. The top self costs per delegate are `try_read` 20 k, `read_access`
14 k, `FieldReads::add` 14 k (already one entry per run with a last-run fast path; the cost is
the number of reads), the `ComputeGuard` drop 13 k, `free` 11 k and `malloc` 8 k. As B9 found,
the cost is spread over creating a delegate. The way to close it is to keep delegates instead of
remaking them (pooling, `reuseItems`), or to make a delegate's graph cheaper to build. Both are
larger than this batch.

DECISIONs:
- On a device, solid rectangles are drawn as part of the mesh and never cleared on their own.
  Damage stays per rectangle.
- In a11y, a Flickable's content item is a space. Container ids are hashed into the range
  above `1 << 61`, so they never collide with object ids.
- The wheel is injected through the sandbox's virtual pointer at one notch every 50 ms, so the
  two runtimes get the same input.
- `moved_by` answers "not moved" when the two shapes are of different kinds. A mesh is never
  compared, so it is redrawn.
- Glyph quads are carried only for upright text with no decorations, a whole-pixel device dx
  and nothing culled at either place. Anything else is laid out again.
- The scripts' heap is measured after 100 ms of rest: one more wake after activity, and none
  while idle, so idle stays at 0 %.
- listview is left with its spread per-delegate cost (above).
- Clocks' remaining native part was not taken. It is under 1 M/s a cut (see B14).

### calqlatr's press, sixth pass: Behavior starts, script reads, provenance (2026-10-19)

Measured as in the fifth pass above. The base is origin at b411fe63, which already has the
one-entry field reads (f76868aa). Ir is from callgrind on a release build, per calqlatr press
(140 presses) and per clocks `timeChanged` (700 calls). Wall time alternates `qml6` 6.11.2, the
base and the after, five runs each, per press over 2 800 presses. The load average was 42–67.
The table keeps the three runs taken at the lower end of that.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| calqlatr, Ir per press | | 1.418 M | 1.376 M |
| calqlatr, wall per press | 0.103–0.106 ms | 0.117–0.124 ms | 0.111–0.121 ms |
| clocks, Ir per `timeChanged` | | 16.26 k | 16.26 k |

calqlatr is now 7–10 % above qml6, down from 12 %. Clocks does not change. The cuts, in order,
with Ir per press as measured on the base each one was made on:
- A Behavior first checks that its animation moves the property, then starts it. The start
  looked up the same `target`, `targets`, `property` and `properties` again. The check now
  hands the slots it found to the start: 1.423 M → 1.415 M.
- When a script reads a text or an object that already has a wrapper, the read hands QuickJS a
  raw value. Before, it made a value tied to the context, which is a dup and a free of the
  context at each read: 1.415 M → 1.394 M.
- The first moment of a property animation without `from` shows the value the property
  already holds. A property that already has a value of its own is no longer written then.
  One that was never given a value is still written, since on Qt that start gives the property
  the value as its own: 1.394 M → 1.385 M. `features/animation/behavior-start-gives-value.feature`
  (@qt-parity) pins the start: an item's width no longer follows its implicit width once a
  Behavior has started on it.
- `provenance` looks at the object once. It used to look once to follow aliases and again to
  answer. An animation's start asks it about nine times.

What is left of a press, about 1.38 M Ir:
- The script, about 0.13 M in the interpreter, plus `try_read` from the host: some 600 reads a
  press, about 120 Ir each on their own.
- 35 tier 1 runs, about 1.25 k each in the machine itself, with about 400 Ir per read.
- 11 Behavior starts, about 7 k each with the flush. The easing and duration reads are about
  1.2 k of that, the `running` write and its batch most of the rest.
- `updateDimmed` reads `children` of two grids per entry. Each read is the whole list,
  about 250 Ir.

DECISIONs:
- No counting tests for the slots handed over, the raw values or the skipped first writes. The
  engine has no counter for named lookups, context dups or writes that change nothing.
  The animation and language features pin the behaviour.
- The slots handed from the check to the start could be stale if the animation's targets
  changed in between. Both happen in the same flush, so this is accepted.
- QuickJS's own accessor entry still dups and frees the context per call: rquickjs has no
  borrowed `Ctx`.
- A Behavior on an attached property (`Layout.leftMargin`) is not animated by Ljós; it writes
  the value straight through. qml6 animates it. This is left for its own scenario.
- ListView's delegate creation and teardown are left to the views lane.

### Scroll copies against a redraw, todolist's pop, and the CPU outside llvmpipe (2026-10-19)

The setup is as in the "Software GL" section above: the off-screen sway sandbox, llvmpipe, and
`qml6` 6.11.2 with `QT_QUICK_CONTROLS_STYLE=Basic`. Builds are release. "Base" is b6a6f165 and
"after" is this batch on top of it. Both ran back to back with Qt at a load average of 7 to 30,
with one spike to 92. Each cell lists three runs, in ms of CPU over the bench window.

| | Ljós base | Ljós after | Qt |
|---|---:|---:|---:|
| listview scroll: outside llvmpipe | 613 / 1034 / 549 | 346 / 646 / 752 | 321 / 420 / 349 |
| photosurface scroll: outside llvmpipe | 252 / 247 / 287 | 278 / 271 / 333 | 291 / 268 / 251 |
| todolist push and pop: outside llvmpipe | 58 / 84 / 84 | 79 / 88 / 81 | 37 / 36 / 31 |
| todolist pop: first frame (ms) | 74.2 / 74.8 / 74.8 | 1.4 / 1.2 / 1.1 | 1.5 / 1.2 / 1.1 |

Under this load the wall-clock CPU moves by up to 2× between runs of one build, so it cannot
tell the builds apart. The CPU work is compared below by instructions counted under callgrind.

- **Scroll copies, pixel for pixel.** `ljos_bdd/tests/scroll_copies.rs` scrolls a clipped
  ListView and a Flickable fourteen times, once with the copies on and once with
  `LJOS_SCROLL_COPIES=off`, and compares the windows. The scenario is
  `features/window/scroll-pixels.feature`.
  - The test found that egui's dithering, which is noise placed by screen position, moved with
    the copied pixels. Frames differed by one or two levels on 200 to 15 000 pixels. Neither
    painter dithers now, and Qt Quick does not dither either.
  - Moved pixels now match the previous frame, moved, exactly. Of 14 scrolls, 8 moved pixels.
- **todolist's pop.** The page under the pushed one owned it, because the pushed page was
  declared inside it. Freezing the covered page froze the shown page with it: its timers
  stopped, and the scene stopped following it until the release popped the stack. A page that
  owns a page the view shows now stays awake (`features/controls/stack-view-owned-page.feature`).
- **Outside llvmpipe** (callgrind on listview's main thread, instructions inside
  `QuickScene::frame`, per frame):
  - The build before b6a6f165 (4a497a06) used 8.53 M per frame and the base uses 8.00 M. Most
    of that drop comes from two upstream rewrites that landed before this batch:
    - 30eb8b5b gives a run one field-read entry with a bit per property;
    - c0053d1d reads a text's inputs, its font among them, in one go.

    Each replaced a change this batch had made to the same code (a field-read index made only
    past 64 entries, and a font read once). Those changes were dropped.
  - *The scroll diff.* The search for what went now looks only as far as what came. It rejects
    shapes of another kind, or bounds more than half a pixel off the shift, before moving a
    copy to compare. `LastPicture::change` went from 6.6 % of a frame to 5.4 %. The frame as a
    whole stayed at 8.0 M because the rows created vary between runs.
  - Where a frame goes now: delegate creation 32 %, GL paint 15 %, `TextInputs::read` 11 %,
    keeping field reads 8 %, and the scroll diff 5 %.

**DECISIONs.**
- Only a page that owns a page shown by the same StackView stays awake. Owning a page shown by
  another view does not count.
- Shapes are told apart by bounds within 0.5 px, which is below any shift a scroll copies.
- Edge pixels may differ by one level between copies on and off. They do not have to be equal,
  because llvmpipe covers a smoothed edge differently at another place on screen: 51 pixels in
  15 frames, one level apart.

**What is left.**
- listview: Ljós's CPU outside llvmpipe is still above Qt's.
  - Creating the rows that scroll into sight is a third of the frame.
  - `TextInputs::read` runs four times per text per frame: complete, the polish effect, polish,
    and the content effect.
- todolist's push and pop are not yet profiled on their own. Outside llvmpipe they cost about
  2.3× Qt's.
- About 9 % of the main thread is memcpy in the EGL/gallium present. Those are row copies, which
  are inherent to software GL.

### B16: listview's delegates, made and reused (2026-10-19)

The same two measures as B15, on the same machine, off-screen in the cua sandbox on the GPU,
with `qml6` 6.11.2 and the Basic style. The start is B15's end (7da8bdb4). The Ljós binary is
bd697c5d as it was before it was rebased onto the calqlatr commits above. The a11y commit
after it does no work with the screen reader off. qml6 was run again alongside it and came out
within 3 % of B15's figures.

| Scroll, M instructions/s | Start (B15) | After | qml6 (same run) |
|---|---:|---:|---:|
| thermostat | 130 | 100.7 | 125 |
| rssnews | 46 | 34.6 | 50.7 |
| listview | 270 | 246 | 102.9 |
| listview with `reuseItems: true` | — | 134 | 60.5 |

| Scroll, CPU % of one core | Ljós | qml6 (same run) |
|---|---:|---:|
| thermostat | 3.02 | 4.33 |
| rssnews | 3.85 | 2.39 |
| listview | 6.89 | 5.17 |

The CPU run had a one-minute load of 12 to 68, so it reads as "within noise of qml6's" and
nothing finer. Frames are 60 a second on both, with the same median and 95th percentile
intervals and input latency.

**Per delegate, by callgrind** (main thread, 40 s warm-up, 20 s window; delegates counted as
half the `Item` completions):

| Binary | Delegates | Instructions | Per delegate |
|---|---:|---:|---:|
| Before the four cuts below | 9581 | 4.35 G | 454 k |
| bd697c5d | 8612 | 3.80 G | 442 k |

B15's 480 k was over 348 frames of a different window, so it does not compare closely.

The commits:

- 2bcf6cd7, 67f447a5: `reuseItems`. The ListView scenarios are pinned against qml6 as a
  black box. A reuse re-runs only the bindings that read the row and completes nothing again.
  The delegate keeps its own state, and the new row's roles have been read by the time
  `reused` is told. A pooled delegate is still visible when it is told, and a view that
  shrinks keeps its pool. TableView now pools the rows that scrolled out before it loads the
  row that scrolls in, as qml6 does. Before, stepping one row made a new delegate where qml6
  reuses one.
- 74546381: an item anchored by nothing reads its nine anchor lines in one go and stops there.
- 9a5e7fec: an item's geometry is read in two batched reads, not eleven separate ones.
- 820d5c4d: a shape already broken at a width is not broken again at the same width.
- bd697c5d: a text that its completion measured is not polished to measure the same inputs
  again.
- 8d806112: the links of a Text are placed in the nearest scrolling space, like items. A
  scroll of a view no longer re-runs the effect that lists them. A press maps its point
  through where the item is at that moment.

**listview is still 2.4× qml6's instructions**, and 2.2× with `reuseItems`. As in B15, the
remaining cost has no hotspot. Per delegate, the top self costs are: `try_read` 14 k, the
`ComputeGuard` drop 11.5 k, `free` 11 k, `read_access` 10 k, `malloc` 8 k and
`FieldReads::add` 8 k. Parley's shaping, font resolution and analysis together come to about
9 k.

DECISIONs:
- A per-Component template of bindings was not built. The cost is spread over reads, graph
  nodes and allocation rather than over working out the shape of the graph. A template would
  save the planning, not the nodes, and it touches the creator that ilang is working in.
- When TableView trims its pool is not pinned. qml6 gives no black-box signal for it that is
  stable from run to run.
- Take a row whose top lies exactly on the view's bottom edge. Ljós keeps it, and qml6 pools
  it. This is left as it is and is not pinned by a scenario.
- A binding that reads and writes the same root property loops in Ljós. It was found while
  writing the reuse scenarios, and is left to the bindings work.
- In qml6, a required property is already set when the delegate's `Component.onCompleted`
  runs. The scenarios rely only on that, not on the order in which the signals are emitted.

### calqlatr's press, seventh pass: attached Behaviors, list entries, native parts (2026-10-19)

Measured as in the sixth pass above. The base is origin at f1c98955. Ir is from callgrind on a
release build with line tables, per calqlatr press (140 presses) and per clocks `timeChanged`
(700 calls). Wall time alternates `qml6` 6.11.2 and Ljós, seven runs each, per press over
2 800 presses. The load average was 25–55.

| | qml6 | Ljós base | Ljós after |
|---|---:|---:|---:|
| calqlatr, Ir per press | | 1.387 M | 1.378 M |
| calqlatr, wall per press | 0.099–0.110 ms | 0.116–0.143 ms | 0.115–0.126 ms |
| clocks, Ir per `timeChanged` | | 16.34 k | 16.35 k |

On this machine, at this load, calqlatr is 14–17 % above qml6. That gap is wider than the
sixth pass's 7–10 %. The qml6 runs were faster than in the sixth pass (276–308 ms per 2 800,
against 290–297 ms then). Ljós's own runs are about as fast as they were. Clocks does not
change.

Correctness first:
- A Behavior on an attached property (`Behavior on Layout.leftMargin`) or on a member of a
  group (`Behavior on border.width`) now stands in front of that property and animates it, as
  on qml6. Before, the value was written straight through.
  `features/animation/behavior-on-member.feature` (@qt-parity) covers it.
- A side margin of `Layout` that was never given a value (`Layout.leftMargin`) reads
  `Layout.margins`, also when `margins` changes later. `features/layouts/attached-set-later.feature`
  (@qt-parity) covers it.
- The sixth pass made the first step of an animation without `from` skip its write, but only
  for a property that already has a value of its own. The write still made for one that was
  never given a value is now pinned: `features/animation/behavior-start-gives-margin.feature`
  (@qt-parity). A side margin that follows `Layout.margins` stops following it once a
  Behavior has started on it, because the start gave it the margin as its own. If the skip
  also covers unset properties, the scenario fails ("5 5 8 8" where qml6 gives "5 5 5 8").

The cuts, with Ir per press:
- `children[i]` and `children.length` from a script look at the list where it is held, either
  a node's value or a native field. Before, each read made the whole list a value. A tracked
  read is kept as `try_read` keeps it. 1.387 M → 1.386 M: the copy was an `Arc` clone, so
  little was saved. What an entry costs is mostly elsewhere, in the next cut.
- `find_native_mut` looks for the part of an object's native chain that declares a field. It
  made three dynamic calls per part: `as_any`, `type_id` and `base_mut`. The derive now
  answers `part_mut` as it already answered `part`, in one call, with the base's own called
  by its type. Every native field write and every list entry goes through it.
  1.386 M → 1.378 M.

What is left of a press, about 1.38 M Ir:
- Writes and what they set off: about 0.41 M from scripts' writes through the graph's batch.
  That is mostly bindings and layouts recomputing, which is the work itself.
- QuickJS itself, about 0.33 M.
- Host reads: about 94 k from `get`, about 45 k of that making live list objects
  (95 a press at about 280 Ir in `JS_NewObjectProtoClass`). Each entry is still about 360 Ir
  between the host's entry, the list read and `to_js`.
- tier 1: 35 runs at about 4.8 k each, almost all of it reads (about 50 Ir each through
  `try_read`). The machine's own dispatch is about 10 Ir per instruction. The frame setup is
  no longer measurable.

DECISIONs:
- List objects are not cached per property. On qml6 each read of `a.children` gives a new
  object: `===` is false, and an expando does not stick. A cache would be visible.
- The side margins read `Layout.margins` by having `margins`'s write give each unset side the
  value as a default (`reset_to`). A side keeps it until it is given a value of its own. Qt
  documents the side margins as following `margins` when unset; this matches it on qml6.
- A Behavior on a member of a value (`font.pixelSize`) is not animated yet; the write goes
  through. qml6 animates it. A Behavior on a member of an attached value
  (`Drag.hotSpot.x`) crashes qml6 6.11.2, so it has no scenario.
- No counting test for the in-place list read or the derive's `part_mut`: the engine has no
  counter for list copies or dynamic calls. The language, views and layouts features pin the
  behaviour.
- ListView's delegate creation and teardown are left to the views lane.

### The cost of a click's passes, and alarms' slow clicks (2026-10-19)

The section above left the static snippets at 0.15–0.42 % of a core while driven, against qml6's
0.07–0.14 %, with two passes a click. This pass profiled single clicks under callgrind (release
build): clicks that change nothing (focus-column, flow-diagram, borderimage-defaults) and clicks
whose handler runs a script (the signals snippets, methods-app).

**A script's click walked the whole JS heap.** When a window rests after a script ran, the engine
asks whether the scripts' heap grew enough to collect. QuickJS gives its byte count only through
`JS_ComputeMemoryUsage`, which walks every object, shape and atom: 78 % of what such a click cost
under callgrind. The runtime now allocates through a counting allocator, and the check reads the
count. B15 above made the check run once a rest is 100 ms long; a click every half second
still has one such rest, so each click still paid for the walk. On signals-basic, the main thread's time for 25 clicks went from 29–40 ms to 20–24 ms
(qml6: 6–8 ms), and driven CPU from 0.27–0.36 % to 0.19–0.23 % (qml6: 0.08–0.09 %).

**A click that changes nothing draws nothing, in both runtimes.** Frames sent while driven are 0
in Ljós and in qml6 for every snippet whose clicks change nothing on screen (all but the two with a
blinking cursor and vertical-positioner-transition). The pass after such a press or release finds nothing
dirty and records nothing. What is left of a no-change click (about 230 k instructions under
callgrind on focus-column) is the cost of waking: about four main-thread wakes a click, one of
them the extra loop turn a redraw request costs, and the clipboard's thread, which shares the
Wayland connection and wakes about five times a click.

**alarms.** Making a Material attached object bumped the style's epoch, which made every other
Material object look up its values again. Opening the alarm dialog makes many of them, one at a
time, so its cost grew with the square of what it made. A new object now changes nothing for
the others; what an author gives it still reaches the items below by the write that gives it.
`features/controls/material-made-later.feature` pins this, and
`material_items_made_one_by_one_cost_what_each_costs` in `crates/ljos_controls/tests/controls.rs`
counts the effects for 40 and 80 rows. The dialog's open went from 40 M to 22 M instructions
(129 M to 110 M for the whole run).

The p95 did not move: 47.0–50.2 ms against qml6's 34.3–34.4 ms (one qml6 run 104 ms), at loads
15–40. Its slow clicks are presses on the dialog's round "+" and "−" buttons. alarms imports
`QtQuick.Controls.Material`, and Ljós's Material controls have no ripple or raised look when
pressed, so the press changes nothing on screen and the bench waits for the release's frame,
40 ms later. That frame took 9 ms. qml6 draws the ripple on the press, about 2 ms after it.

| | driven CPU % | busiest thread % | main thread ms | lag p95 ms |
|---|---:|---:|---:|---:|
| alarms, Ljós | 0.32 | 0.26 | 27.0 | 47.3 |
| alarms, qml6 | 6.44 | 1.41 | 24.0 | 34.4 |

**The static snippets again.** The 18 snippets were run against qml6 back to back on the two
changes above, with `--warm-up --repeat 3`, in chunks of three or four, at load averages of
6.7–135. Medians:

| example | driven CPU % (Qt) | busiest thread % (Qt) | main thread ms (Qt) | lag p95 ms (Qt) |
|---|---:|---:|---:|---:|
| snippet-borderimage-defaults | 0.12 (0.07) | 0.10 (0.05) | 11.8 (6.1) | — (—) |
| snippet-vertical-positioner-transition | 0.18 (0.10) | 0.16 (0.07) | 18.7 (7.9) | 41.8 (40.7) |
| snippet-flow-diagram | 0.17 (0.09) | 0.14 (0.06) | 16.8 (6.9) | — (—) |
| snippet-focus-column | 0.16 (0.07) | 0.14 (0.05) | 15.9 (5.6) | — (—) |
| snippet-focus-scopes | 0.32 (0.41) | 0.28 (0.09) | 32.5 (10.5) | 456.3 (457.0) |
| snippet-rectangle-smooth | 0.16 (0.07) | 0.13 (0.05) | 15.8 (5.7) | — (—) |
| snippet-texteditor | 0.38 (0.77) | 0.33 (0.19) | 39.0 (15.3) | 457.5 (456.3) |
| snippet-text-lines | 0.17 (0.08) | 0.14 (0.05) | 16.4 (6.2) | — (—) |
| snippet-components-application | 0.20 (0.09) | 0.17 (0.07) | 19.8 (7.8) | — (—) |
| snippet-properties-application | 0.19 (0.09) | 0.16 (0.06) | 18.7 (6.9) | — (—) |
| snippet-alias-application | 0.14 (0.08) | 0.12 (0.05) | 14.0 (6.3) | — (—) |
| snippet-property-signals | 0.17 (0.09) | 0.14 (0.05) | 17.0 (6.1) | — (—) |
| snippet-signals-basic | 0.24 (0.09) | 0.21 (0.06) | 25.1 (7.5) | — (—) |
| snippet-signals-parameters | 0.27 (0.15) | 0.23 (0.10) | 27.3 (11.6) | — (—) |
| snippet-signals-no-parameters | 0.21 (0.11) | 0.18 (0.08) | 21.7 (9.2) | — (—) |
| snippet-signals-connectslots | 0.21 (0.10) | 0.19 (0.07) | 22.0 (8.5) | — (—) |
| snippet-signals-connectdynamic | 0.24 (0.12) | 0.21 (0.09) | 24.4 (10.4) | — (—) |
| snippet-methods-app | 0.24 (0.09) | 0.21 (0.07) | 24.3 (8.1) | — (—) |

Ljós now uses 0.12–0.38 % of a core while driven, against 0.07–0.77 %. It wins snippet-focus-scopes
and snippet-texteditor, whose cursor blinks, and still uses about twice qml6's CPU on the rest,
where a click costs a few wakes and no frame. Its main thread spends 12–39 ms on 25 clicks,
against 6–15 ms for qml6's.

What is left:

| loss | cause | owner |
|---|---|---|
| static snippets, about twice qml6's CPU while driven | The cost of each wake, not of the passes' work: a redraw request costs a loop turn of its own, and the clipboard thread wakes on every Wayland event. | shell |
| alarms lag p95 47 against 34 ms | Material controls draw nothing on press: no ripple, no raised look. | controls |
| a Material value given in every delegate | An author's write of a Material value still bumps one epoch for the whole style, so delegates that each give one are quadratic to make. | controls |

- **DECISION: the heap count is pinned by a Rust test, not a feature.** It changes no behaviour
  a QML author can see, only how often the heap is walked.
- **DECISION: the redraw request's extra loop turn is left.** Taking it out changes when frames
  are paced against the compositor's callbacks, which the passes-per-click scenario and the
  animation benches depend on; it needs its own measurement.
- **DECISION: Material press feedback is left for later.** A ripple and a raised look on every
  Material control is a style feature of its own, with its own scenarios against qml6.
- **DECISION: an author's write still bumps the global epoch.** Scoping it to the items below the
  writer needs the attached objects to know their descendants, a larger change than this batch's.

### B17: graph nodes, property reads and a Text's inputs per delegate (2026-10-23)

These are B16's two measures, taken on the same machine. Everything ran off-screen in the cua
sandbox on the GPU, against `qml6` 6.11.2 with the Basic style. The start is 0f513821, the commit
that wrote B16's section, with nothing from this batch. The end is 99a70314. Both are
profiling builds (release, line tables). qml6 was run alongside them. The one-minute load was
33 to 41 throughout, so read small differences as noise.

| Scroll, M instructions/s | Start | After | qml6 (same run) |
|---|---:|---:|---:|
| thermostat | 100.8 | 96.4 | 125.0 |
| rssnews | 34.4 | 34.1 | see below |
| listview | 248.6 | 227.0 | 102.8 |
| listview with `reuseItems: true` | 133.1 | 124.5 | 60.4 |

rssnews reads a live feed. In this run the feed carried images, and qml6 decoded them while
it scrolled: 206 M/s on its `QQuickPixmapReader` thread, against 43 M/s on its other threads.
Ljós does not show the feed's images in this example yet. Its first run, while it loaded the
feed, read 76 M/s on both binaries, so neither total compares with B16's 50.7.

| Scroll, CPU % of one core | Ljós | qml6 (same run) |
|---|---:|---:|
| thermostat | 6.04 | 10.3 |
| rssnews | 7.74 | 10.1 |
| listview | 15.1 | 10.9 |

The load was 32, so these percentages are about twice B16's on both sides. Only the ratio
means anything: listview is ×1.39 of qml6, and the geometric mean of the three is ×0.85.

**Per delegate, by callgrind.** This is listview's main thread, with a 40 s warm-up and a
20 s window. Delegates are counted as half the `Item` completions.

| Binary | Delegates | Instructions | Per delegate | `malloc` per delegate |
|---|---:|---:|---:|---:|
| Start | 9100 | 4.00 G | 440 k | 233 |
| d4c935a6 (the text and read commits) | 9128 | 3.60 G | 394.5 k | 193 |
| 99a70314 | 8039 | 3.12 G | 388 k | 194 |

The last commit removes 6 of the 60 `Vec` growths per delegate, not mallocs.

The commits:

- 475333a9: a text's inputs are read once per change, by its polish effect. Paint and
  polish use that one read. `TextInputs::read` used to run four times per text per frame.
  Now it runs twice per text when the text is made: once when its completion measures it,
  and once in the polish effect.
- 4f1c1315: asking whether an object is of a type builds no error value. The chooser and the
  style's `is_a` asked this for every delegate.
- 5c234832: most objects are read by only a few runs, and those keep their reads in a short
  list scanned in order. A map is built only from 16 runs.
- 9211575d: when a scope is disposed, its node list is kept for the next scope. The pool
  holds up to 256 lists, each with a capacity of 256 or less. `Graph::bind` went from 13
  mallocs per delegate to 7.
- 0233dfbc: `read_values_unless_bound_on_read` reads a group of properties in one pass,
  including properties whose bindings are still waiting to be made on a read. A write that
  drops such a waiting binding tells everyone who read past it.
- d4c935a6: a Text's four side paddings, its `horizontalAlignment` and its
  `effectiveHorizontalAlignment` are bound only when something reads them. Until then they
  cost a stored value and nothing else, and the layout works out the same values itself.
  `features/text/padding.feature` pins this against qml6.
- 99a70314: an object's waiting bindings, and the first runs that read it, start in one
  allocation instead of growing.

**listview is still 2.2× qml6's instructions per second, and 2.1× with `reuseItems`.** The
top self costs per delegate are now:

- `try_read` 12 k;
- `free` 9 k;
- the `ComputeGuard` drop 8.7 k;
- `read_access` 8.4 k;
- the 19-property Text read 7.7 k;
- `malloc` 6.9 k;
- `Graph::dispose` 5.8 k;
- `FieldReads::add` 5 k.

Parley's shaping costs about 28 k per delegate inclusive, but it shapes once per text.

These are left:

- `views::pass::tell` makes four computed writes per call, about 11.6 per delegate and
  4.5 k instructions. That code belongs to the views work.
- `FieldReads::new` makes 8 mallocs per delegate.
- `read_access` finds a native part through `QmlObject::part` and its base chain. That is
  665 k calls of `Text::part` in the window, 15 M instructions.
- `__qml_get_font` clones the font, and `text_style` makes an `Arc` of the family. Each
  costs about 2 mallocs per delegate.

DECISIONs:

- A Text is still read twice when it is made, once by the completion measure and once by
  the polish effect, and once per change after that. Merging the two reads at creation
  would mean the completion has to wait for the polish.
- When a write drops a binding that was waiting for a read, readers are told before the
  write lands. A reader may therefore run once more than it needs to, but only when the
  written value differs from the stored one, which is rare.
- The pool keeps at most 256 node lists, each with a capacity of 256 or less, so one huge
  scope does not hold its memory for good.
- `pass::tell` is left to the views work.

### B24: todolist's push and pop against qml6, and numbers that hold under load (2026-10-23)

**How to measure on a loaded machine.** The machine these runs share had a one-minute load of
7 to 98. At that load the CPU time of one run of a bench differs from the next by 15 to 50 %,
so a cut of 10 % cannot be seen in it. Count instructions instead:

- `ljos bench` counts the instructions each thread retires in user space, by thread name
  (`--no-instructions` turns it off). It reads the processor's own counter, so it needs one,
  and `perf_event_paranoid` at most 2. Five runs of todolist's push and pop at load 30 to 98
  counted within 0.5 % of each other; their CPU time did not.
- Divide by frames, or fit against them. A run that drops frames does less work, and a
  loaded machine drops more of them. For todolist's push and pop, the main thread's count is
  about 3.4 M instructions per transition frame plus about 35 M that do not depend on the
  frame count. Compare those two numbers, not the totals.
- Count the renderer's threads apart from the runtime's. Under software GL, llvmpipe's
  threads (`llvmpipe-N`) do more work than the main thread. A change that cuts the main thread
  can cost llvmpipe more, as the one below did.
- For where it goes, run callgrind under the same bench through a `--ljos` wrapper, and dump
  it every 2 s with `callgrind_control -d`. Each dump then holds about 9 to 10 frames. Divide
  by the calls to a function that runs once a frame (`Desktop::pass`, `GlPainter::paint`,
  `QuickScene::frame`). Do not use the inclusive cost of recursive functions, which
  callgrind counts more than once. Valgrind is slow enough that the app drops frames, so
  compare its counts per frame, never per run.
- Wall-clock numbers (frame times, latency) still need the minimum of several runs at a known
  load, and they are only good for comparing runs made side by side.

**todolist: pushing the edit page and popping it.** Five runs each, off-screen in the cua
sandbox on llvmpipe with the Basic style, at load 31 to 39. The table shows instructions in
G (billions) for the whole run, and in M (millions) per frame.

| | Main thread, G | Per frame, M | llvmpipe, G | Per frame, M | Frames | CPU ms |
|---|---:|---:|---:|---:|---:|---:|
| Start | 0.180–0.198 | 4.17 | 0.426–0.474 | 10.0 | 43–48 | 149–180 |
| After | 0.157–0.166 | 3.46 | 0.471–0.510 | 10.6 | 45–48 | 143–178 |
| qml6 6.11.2 | 0.105 | 3.50 | 0.827 | 27.6 | 30 | 153–184 |

The 2.3× gap to Qt that we had measured was mostly frames. qml6 drew 30 frames for the two
transitions, while Ljós drew 45 to 48. Per frame, Ljós's main thread now does what Qt's does.
Counting llvmpipe as well, Ljós costs about 14 M instructions per frame and Qt about 31 M.

The main thread's cost per frame came from scattered partial damage. During a transition the
frame was drawn as 10 to 17 scissored areas, and each area drew its items again: 25 to 53
draws a frame, and Mesa pays for each draw. The commits:

- de3059ed: when more than 4 areas are damaged and they cover at least a fifth of the window,
  the frame is drawn whole. This cut the main thread by 17 % per frame and cost llvmpipe 6 %,
  because it fills more pixels.
- 6cd0e9a4: a page pushed onto a StackView is shown before anything that follows it runs.
  The first frame of the transition no longer lays out the page twice.

Left:

- The frame count: 45 to 48 against Qt's 30. Ljós draws every frame of the transition, and
  Qt drops frames under llvmpipe. That shows the animation better, but it costs more in total.
- When a page is pushed, the inherited properties (palette, font) cascade through it. This is
  the largest one-off cost in the 35 M.
- llvmpipe's extra 6 % from drawing whole frames.

### calqlatr's press, eighth pass: the tint ring, a Behavior's new value, read-only reads (2026-10-24)

#### Method

The base is origin at 6bbeb113. `fp.list` is the build from before the tint ring (f497f315,
"the few tints made last are held"); `b27a` is the base; `b27b` and `b27c` are this pass's
two cuts. Ir is from callgrind on a release build with line tables, per calqlatr press (one
click, 140 presses) and per clocks `timeChanged` (700 calls). Wall time alternates `qml6`
6.11.2 and each Ljós build, eleven rounds, per press over 2 800 presses (calqlatr) and per
call over 7 000 calls (clocks). The figures are medians, with the fastest round in brackets.
The load average was 14–20.

#### Numbers

| | qml6 | Ljós `list` | Ljós `b27a` (base) | Ljós `b27b` | Ljós `b27c` (after) |
|---|---:|---:|---:|---:|---:|
| calqlatr, Ir per press | | 1.320 M | 1.268 M | 1.243 M | 1.240 M |
| calqlatr, wall per press | 111.1 µs (106.4) | 118.6 µs (110.1) | 111.3 µs (107.2) | 105.5 µs (102.7) | 107.6 µs (103.3) |
| clocks, Ir per `timeChanged` | | 16.49 k | 16.35 k | 16.35 k | 16.33 k |
| clocks, wall per call | 0.71 µs | 1.11 µs | 1.08 µs | 1.09 µs | 1.10 µs |

At this load calqlatr's press is now level with qml6, a little under it in the medians; the
seventh pass had it 14–17 % above. `b27b` and `b27c` differ by 0.2 % in Ir, so their wall
difference is noise. qml6 prints clocks' time in whole milliseconds (5 ms for 7 000 calls),
so its 0.71 µs is coarse; Ljós is still about 50 % above it there, and nothing in this pass
touches clocks' path.

#### The cuts

- The tint ring (before this pass, measured here): a colour that flips back finds its tint
  among the few made last. 1.320 M → 1.268 M, −51.7 k (−3.9 %) a press; clocks −0.14 k.
- A Behavior's new value on the way restarts the timeline it has. Each press starts about
  twelve Behaviors on the buttons' text colours, and with no frame between clicks each start
  found a run still going. A start was about 6.5 k Ir: `build_in` 2.5 k (`curve_of` 0.7 k,
  `duration_of` 0.35 k), the first step 1.8 k, the `running` write 1 k, taking the old run
  out 1.1 k, allocation 0.5 k. Now a running `PropertyAnimation` whose target, kind, `from`,
  duration, loops and easing read as they did gets the new `to` and starts again from the
  value shown. Anything else is built anew. `start_driven` 71 k → 51 k a press;
  1.268 M → 1.243 M. `features/animation/behavior-new-value-reads-animation.feature`
  (@qt-parity) pins that a duration or easing changed on the way is taken by the next value.
- A tracked read of a read-only QML property with no node is not kept in `FieldReads`. Only
  its making writes it, before anything reads it, and reload leaves read-only properties
  alone. calqlatr's `readonly property color` literals were about 22 of the 143 field reads a
  press. `keep_field_read` 24.7 k → 21.6 k; 1.243 M → 1.240 M.
  `features/language/readonly-value-reads.feature` (@qt-parity) pins a binding on one, also
  when `createObject` gives it another value.

#### QuickJS's own work by function

QuickJS's interpreter loop (`JS_CallInternal`, exclusive) is 126 k of a press, and QuickJS
as a whole about 0.33 M. To split it by calqlatr function, each was called on its own from a
closure, 60 rounds against 10:

| call | Ir per call | interpreter |
|---|---:|---:|
| `numberPad.updateDimmed()` (once a press) | 567 k | 119 k |
| its 31 `applicationState.isButtonDisabled(text)` | 31 × 16.0 k | 31 × 4.2 k |
| its 31 `children[i].dimmed = …` writes, unchanged | 31 × 1.9 k | 31 × 0.5 k |
| its 31 `children[i].text` reads | 31 × 1.35 k | 31 × 0.35 k |

So `updateDimmed` is 46 % of a press and nearly all of the interpreter's own 126 k. Of each
`isButtonDisabled`, about 10.5 k is finding two free names, `CalcEngine` (the script import)
and `display` (a property of the state object). Each goes through the global proxy
(`js_proxy_has` 4.6 k, `js_proxy_get` 5.9 k inclusive) into the host's `raw_global` and
`resolve_id`, on every call. `isOperationDisabled` itself is a few string compares.

#### Next targets

- A function's free names: resolving `CalcEngine` and `display` once per function and scope,
  not through the proxy on each call, is up to about 325 k a press (26 %).
- `restart_towards` is 20 k a press, 7.9 k of it `curve_of` re-reading the easing to compare;
  a change stamp on the animation's settings would make the guard a compare.
- `keep_field_read` keeps 21.6 k: `dimmable` (a dynamic property that can change) and the
  native `pressed` and `hovered`, which native code writes. Left to the field-reads work.
- The graph's flush after the `dimmed` writes is about 0.49 M a press: bindings and
  layouts recomputing, the work itself.

#### DECISIONs

- A reused timeline goes into a new run, so runs still advance in the order they started.
- The reuse guard reads exactly what a build reads, so it cannot accept what a build would
  read differently.
- `running` is not written again on a reuse: it is already true.
- The read-only skip covers only properties declared in QML (a dynamic slot). A native
  read-only field such as `pressed` is written by native code, so its reads stay kept.

### B20: a click's wakes, the redraw's loop turn, clocks and Material's press (2026-10-24)

"The cost of a click's passes, and alarms' slow clicks" above left the static snippets at
about twice qml6's CPU while driven, with a click costing a few wakes and no frame, and
alarms' p95 at 47 against 34 ms. This pass cut the wakes and the clock reads, and gave
Material buttons a press of their own. It does what that section left for later: the redraw's
loop turn, Material's press and the epoch an author's write bumped.

**The clipboard's thread.** egui-winit kept a clipboard thread on the window's own Wayland
connection, so it woke on every event the window got: about five times a click. egui's copies
and pastes now go to the platform's clipboard, which reads and writes the system's only when
asked. `features/window/passes-per-click.feature` counts the wakes of the application's threads
over a click. The accessibility adapter's thread woke on every pass too, for the same root
update; a pass that does not change egui's tree now sends nothing.

**The redraw's loop turn.** Asking the platform to redraw costs a turn of the event loop to
hear yes. A window the platform has let draw since its last frame now runs its pass at once.
A drawn frame makes the next one ask again, so frames that follow one another still wait for
the compositor. Pacing was measured against the compositor with the binary before and after,
three runs each, medians (`--settle 3 --idle 4 --load 5`, drag and scroll `--idle 2`):

| | before | after |
|---|---:|---:|
| samegame frame interval p50 / p95 ms | 16.12 / 16.15 | 16.11 / 16.15 |
| samegame input-to-frame p50 / p95 ms | 8.15 / 16.08 | 8.17 / 15.68 |
| alarms input-to-frame p50 / p95 ms | 1.87 / 49.34 | 1.37 / 48.77 |
| listview drag interval p50 / p95 ms, fps | 16.17 / 16.20, 61.85 | 16.16 / 16.20, 61.92 |
| listview drag input-to-frame p50 / p95 ms | 8.08 / 15.19 | 8.08 / 15.22 |
| listview scroll interval p50 / p95 ms, fps | 16.17 / 16.20, 61.94 | 16.16 / 16.20, 61.60 |
| listview scroll input-to-frame p50 / p95 ms | 8.18 / 15.42 | 8.48 / 15.23 |
| focus-column, 25 clicks: CPU ms, main thread M instructions | 20.82, 5.81 | 18.86, 5.42 |

Frame intervals are the same in every case, and input-to-frame latency is the same or lower.

**Clock reads.** An LD_PRELOAD counter of `clock_gettime` and `getpid` on the main thread,
symbolized by backtrace, on focus-column with 25 clicks: `clock_gettime` went from 87–92 a click
to 44. An inbox drain now starts its budget clock at the first message; the image store and the
platform skip draining an empty inbox; the image store waits for no frame when nothing waits;
and settle hands its last read to the script collector. What is left belongs to winit, calloop
and polling, egui's input, one or two reads a pass for the pass's own clock, and the painter's
three step timings a frame. `getpid` is 0 a click: all of it comes from libEGL loading GL
function pointers at startup.

**Material's press.** qml6's Material Button and RoundButton were watched black-box on a
compositor. A pressed button is covered by a layer of the theme's text colour, about
`#10000000` in the light theme (214 → 201 on the button's grey). A RoundButton takes it at
once and drops it at once; a Button's comes in over about 100 ms and fades over about 400 ms
after release. A raised button's shadow, about 4 px deep at rest, is about 10 px deep while
held, at once. Ljós now draws the same layer, timings and a deeper shadow while held, so a
press draws a frame of its own. `features/controls/material-press.feature` pins the colours and
timings; it is `@ljos-only`, because qml6's vnc platform, the one that takes input, draws no
Material button.

**Delegates that each give a Material value.** An author's write bumped one epoch for the
whole style, so every attached object looked its values up again. A walk for an inherited value
now notes the items it passed that had no attached object, and only a value given there later
bumps. 80 rows that each give a background ran 5900 effects against 2160 for 40, and now run
within 2.2 times of 40's.

**Against qml6 again.** alarms and the 18 static snippets were run against qml6 back to back on
ca9ba3ec (rebased on the branch), with `--warm-up --repeat 3`, two apps a chunk, at load
averages of 3.3–51.3. Medians:

| example | driven CPU % (Qt) | busiest thread % (Qt) | main thread ms (Qt) | lag p95 ms (Qt) |
|---|---:|---:|---:|---:|
| alarms | 0.29 (5.87) | 0.25 (1.10) | 25.9 (20.1) | 3.2 (34.2) |

alarms' presses on the round "+" and "−" now draw a frame of their own. Its input-to-frame p95
went from 47.3 to 3.2 ms, against qml6's 34.2. The rest of its numbers barely moved.

| example | driven CPU % (Qt) | busiest thread % (Qt) | main thread ms (Qt) | lag p95 ms (Qt) |
|---|---:|---:|---:|---:|
| snippet-focus-column | 0.12 (0.07) | 0.12 (0.04) | 14.1 (5.1) | — (—) |
| snippet-borderimage-defaults | 0.12 (0.08) | 0.12 (0.05) | 14.1 (5.4) | — (—) |
| snippet-vertical-positioner-transition | 0.19 (0.11) | 0.19 (0.07) | 22.2 (7.9) | 42.0 (40.9) |
| snippet-flow-diagram | 0.15 (0.08) | 0.16 (0.05) | 18.3 (6.3) | — (—) |
| snippet-focus-scopes | 0.29 (0.44) | 0.27 (0.09) | 31.6 (9.6) | 456.3 (461.0) |
| snippet-rectangle-smooth | 0.14 (0.09) | 0.14 (0.05) | 16.1 (6.4) | — (—) |
| snippet-texteditor | 0.39 (0.77) | 0.38 (0.19) | 44.2 (14.9) | 456.4 (465.0) |
| snippet-text-lines | 0.10 (0.06) | 0.10 (0.03) | 11.8 (4.0) | — (—) |
| snippet-components-application | 0.17 (0.08) | 0.16 (0.06) | 19.3 (6.8) | — (—) |
| snippet-properties-application | 0.13 (0.09) | 0.13 (0.05) | 15.7 (6.3) | — (—) |
| snippet-alias-application | 0.09 (0.06) | 0.09 (0.03) | 11.1 (4.1) | — (—) |
| snippet-property-signals | 0.10 (0.07) | 0.10 (0.04) | 11.9 (4.4) | — (—) |
| snippet-signals-basic | 0.15 (0.09) | 0.15 (0.06) | 17.5 (6.8) | — (—) |
| snippet-signals-parameters | 0.19 (0.10) | 0.18 (0.07) | 21.7 (8.2) | — (—) |
| snippet-signals-no-parameters | 0.20 (0.10) | 0.20 (0.07) | 23.3 (8.2) | — (—) |
| snippet-signals-connectslots | 0.16 (0.09) | 0.15 (0.07) | 17.9 (7.7) | — (—) |
| snippet-signals-connectdynamic | 0.18 (0.11) | 0.17 (0.08) | 19.9 (9.2) | — (—) |
| snippet-methods-app | 0.20 (0.11) | 0.19 (0.07) | 22.8 (8.6) | — (—) |

The static snippets now use 0.09–0.39 % of a core while driven, against qml6's 0.06–0.77 %
(B19: 0.12–0.38 against 0.07–0.77). Ljós still wins focus-scopes and texteditor. On the
others it went from about twice qml6's CPU to 1.4–2.1 times. Its main thread spends 11–44 ms on
25 clicks against qml6's 4–15 ms, still 2.3–3.0 times.

What is left:

| loss | cause | owner |
|---|---|---|
| static snippets, 1.4–2.1 times qml6's CPU while driven | About 44 clock reads a click remain, most of them in winit, calloop and egui's input. Each wake costs more on the main thread than qml6's does. | shell |
| Material Button's shape | qml6's Material Button is a pill (radius 20) and about 8 px wider; Ljós's has radius 4. | controls |
| the press layer grows as a circle in qml6 | Ljós covers the whole background at once, because it clips only to rectangles. | render |

- **DECISION: the platform clipboard is the only one.** egui's own is not kept as a fallback; a
  compositor without data-control falls back to XWayland's X11 clipboard.
- **DECISION: the window becoming active on its first click still wakes the accessibility
  thread.** A screen reader is told of that.
- **DECISION: the press layer covers the whole background** rather than growing as a circle from
  the press point. Ljós clips to rectangles, and after 100 ms the two look the same.
- **DECISION: the shadow is layered faint rectangles, not a blur.** A blur costs a pass over the
  button every frame.
- **DECISION: the painter's step timings are kept.** They are three reads a frame, and a frame
  that draws nothing does not run them.

### B18: what a view tells its delegates, and who hears of an attached object (2026-10-24)

B17's measures again, on the same machine and in the same way: off-screen in the cua sandbox
on the GPU, against `qml6` 6.11.2 with the Basic style. The start is b70b6f13, the tip this
batch was rebased onto. The end is this batch's five code commits on it (since rebased onto
a951ef0a as the commits listed below). Both are profiling builds (release, line tables), and
each pair ran back to back.

**Per delegate, by callgrind.** listview's main thread, 40 s warm-up, 20 s window;
delegates are half the `Item` completions.

| Binary | Delegates | Frames | Instructions | Per delegate | Per frame (`QuickScene::frame`) | `malloc` per delegate |
|---|---:|---:|---:|---:|---:|---:|
| Start | 9950 | 1909 | 4.27 G | 430 k | 4.08 M | 202 |
| End | 9970 | 2183 | 4.20 G | 421 k | 3.46 M | 203 |

The start measures 430 k, not B17's 388 k. The difference is how many frames fall to a
delegate: under callgrind a cheaper frame means more frames in the window, and each frame
costs the same paint whatever it made. The end drew 14 % more frames than the start for the
same delegates, so its per-delegate total hides part of the cut. The frame and the functions
below are the fairer comparison:

| Inclusive, per delegate | Start | End |
|---|---:|---:|
| `views::pass::pass` (layout, telling the rows) | 130.6 k | 109.9 k |
| `ViewDelegates::request` | 78.3 k | 63.6 k |
| `QuickScene::adopt` | 89.8 k | 82.2 k |
| `QuickScene::polish` | 64.4 k | 52.9 k |
| `Graph::recompute` | 226.5 k | 200.0 k |
| `views::pass::tell` | 12.6 k (2371 per call) | 2.6 k (434 per call) |
| `Engine::attached_object` | 7.8 k | 4.6 k |
| `text_style` | 0.50 k | 0.17 k |
| `try_read` | 51.7 k (134 calls) | 45.1 k (116 calls) |

| Scroll, M instructions/s | Start | End | qml6 (same run) |
|---|---:|---:|---:|
| thermostat | 99.4 | 99.2 | 125.1 |
| rssnews | 44.3 | 43.3 | 102.3 |
| listview | 227.5 | 215.4 | 102.8 |
| listview with `reuseItems: true` | 126.5 | 119.5 | 60.6 |

rssnews's qml6 figure includes its network and image threads (62 M/s on
`QNetworkAccessManager`'s); Ljós does not show the feed's images in this example.

| Scroll, CPU % of one core (`ljos bench --gesture scroll`) | End | qml6 (same run) |
|---|---:|---:|
| thermostat | 3.38 | 4.97 |
| rssnews | 6.48 | 5.32 |
| listview | 8.38 | 3.74 |

The load was 5 to 14 here, against B17's 32, so these are not B17's percentages and the
ratios are not B17's either: listview is ×2.24 of qml6. The CPU % of one binary moves by
half its value from run to run at this load. Twelve listview runs of each binary, taken
alternately, gave medians of 8.77 % (start) and 9.27 % (end) with the two spreads (6.9 to
10.6, 6.3 to 15.8) on top of each other, while their instructions per second were 227.3 and
214.8 M every time. The instruction count is the measure to trust, and it is down 5.5 %.

The commits:

- d883cc3d: what the view last told a delegate (`isCurrentItem` and the three section
  names) is kept on its attached object. A pass that would tell the same writes nothing.
  `tell` read the view's `currentIndex` itself on each call; the pass now reads it once.
- 171b6dfb: `text_style` shares the family name's `Arc<str>` with the last style made
  when the family is the same, instead of making one per text.
- ebaa1c94: an object's makers on read carry a 64-bit mask of their properties. Every
  read asked whether its property had a maker by looking through the list; the mask says
  no at once.
- a81086af: what the view told starts as the properties do, so a new row that is not
  current and has no sections is not written to on its first pass either.
- 125b29c2: a listener on attached objects can name the type it is for
  (`Engine::on_attached_created_of::<T>`), and is then called only for that type or one
  derived from it. Every attached object a delegate got called all twenty-two listeners,
  each asking whether the object was its type, after cloning the list. ScrollBar's,
  which is for two types, stays general; Material's and Universal's is now one each.

**listview is still 2.1× qml6's instructions per second, and 2.0× with `reuseItems`.**

These are left:

- `try_read` is 11 k self per delegate over 116 calls. Most of those are the bindings of
  a new delegate reading its first values, so they go with the number of bindings.
- `free` is 9 k self per delegate and `malloc` 6.6 k, over about 200 each.
- anchors' `read_values<9>` costs 6 k per anchored item: 5 k is the binding it evaluates
  and 2.2 k is `push_override` making the node and its override.
- `read_access` finds a native part through `part()` and the base chain. That costs about
  1.7 k per delegate, so it was not worth a cache keyed by type.
- `FieldReads::new` (8 mallocs per delegate) and `__qml_get_font`'s clone of the font cost
  0.2 to 0.6 k per delegate together.
- `reuseItems: true` was not looked at in this batch.

DECISIONs:

- The family is shared through a one-entry memo in `text_style`, not by making
  `TextStyle.family` an `ArcStr`. That keeps arcstr out of `ljos_text`'s API.
- A typed listener is called for the type it names and for types derived from it, which
  is what `is_a` answered before.
- `read_access`'s part lookup, `FieldReads::new` and the font clone were left: by
  callgrind, each is under 2 k per delegate.

### B19: a reused delegate, a text's advances and a list row's places (2026-10-24)

B18's measures again, on the same machine and in the same way: off-screen in the cua sandbox
on the GPU, against `qml6` 6.11.2 with the Basic style. The start is a04e45de (B18). The end
is this batch's four code commits rebased onto 22dcbbc0, which also carries the commits that
landed on main in between. Both are profiling builds (release, line tables). The `reuseItems`
bench is bench/listview.qml with `reuseItems: true` on the view.

**Per delegate, by callgrind.** The main thread, with a 40 s warm-up and a 20 s window. For
listview a delegate is one `TextEngine::build_layout` (one text each). With `reuseItems` it is
one `views::pass::revive`.

| Bench | Binary | Delegates | Frames | Instructions | Per delegate | `malloc` per delegate |
|---|---|---:|---:|---:|---:|---:|
| listview | Start | 9819 | 1075 | 4.18 G | 425.5 k | 205 |
| listview | End | 9833 | 1005 | 4.05 G | 411.5 k | 199 |
| `reuseItems` | Start | 9987 | 620 | 2.11 G | 211.0 k | 65 |
| `reuseItems` | End | 9987 | 761 | 1.59 G | 159.4 k | 58 |

The same measure after each commit: `reuseItems` went from 211.0 k to 162.5 k with the pool,
then 161.1 k with the advances, 159.8 k with the row places and 159.4 k at the end. listview,
which the pool does not touch, went from 425.5 k to 420.0 k with the advances, 418.0 k with the
row places and 411.5 k at the end; the last step also takes in the rebase.

| `reuseItems`, inclusive per reused delegate | Start | End |
|---|---:|---:|
| `views::pass::pass` | 90.8 k | 36.6 k |
| `views::pass::revive` | 27.7 k | 0.8 k |
| `views::pass::release` (into the pool) | 27.3 k | 1.2 k |
| `QuickScene::polish` (nearly all text layout) | 63.2 k | 61.0 k |
| `ViewDelegates::request` (the row's new index and what it moves) | 24.8 k | 23.8 k |

| Scroll, M instructions/s (`scr.sh`, back to back) | Start | End | qml6 (same run) |
|---|---:|---:|---:|
| listview | 214.1 | 210.2 | 102.9 |
| listview with `reuseItems: true` | 118.7 | 87.7 | 60.6 |
| thermostat | 98.1 | 98.0 | 125.3 |
| rssnews | 43.6 | 43.4 | 112 to 618 |

rssnews was run three more times with each binary (43.5, 44.0 and 43.6 against 43.4, 43.3 and
43.4); the first pair read 34.9 and 95.1. qml6's figure for it is mostly its network and image
threads (`QNetworkAccessManager` 68 to 71 M/s, `QQuickPixmapReader` up to 73), so it moves with
the feed.

| Scroll, CPU % of one core (`ljos bench --gesture scroll`) | End | qml6 (same run) |
|---|---:|---:|
| thermostat | 3.91 | 5.33 |
| rssnews | 12.7 | 5.81 |
| listview | 7.86 | 4.51 |

Other work on the machine held the load between 18 and 68 for these runs, against 5 to 14 in
B18. As B18 found, a CPU % at that load moves by half its value from run to run. Read the
instruction counts instead.

The commits:

- 5f76df35: a delegate in a view's pool is left out of the scene instead of having its
  `visible` overridden. Taking it out and giving it back changed `visible` for it and
  everything in it: every effect and binding that read it ran twice per reuse. qml6 does the
  same: a pooled delegate reads `visible`, is never told it changed, and is neither drawn nor
  given the pointer (features/views/list-pooled-rows.feature and table-pooled-cells.feature,
  which also pass on qml6).
- b89a935f: a text's advances, rounded as Qt counts them, are worked out once, when the text
  is shaped and measured. Laying it out takes them from the shaped entry instead of walking
  the glyphs again. This found a bug: an editor that wrapped a shape the cache shares did not
  record the break, so a label of the same text in the same font was laid out wrapped
  (features/text/editor-and-label.feature).
- 771d73cd: a view without a move, add, remove or displaced transition keeps no table of
  where each row was. One set later makes the pass run again, so it has that table before the
  next change (features/views/list-late-displaced.feature).
- 55e06379: a row of a list is put along the view only. Where it sits across is read only
  when a grid or a running transition needs it.

**listview is still 2.0× qml6's instructions per second, and 1.45× with `reuseItems`.**
thermostat stays below qml6.

These are left:

- Making a delegate is spread over the reactive graph, about 35 nodes, 6 effects and 200
  allocations per delegate. Adoption (the first runs of its effects) is 83 k; creation
  (`ViewDelegates::request`) is 76 k; text layout is 60 k; destruction is 28 k; and the
  content effects that paint it are 10 k. No single part is large enough to halve the total.
  Getting near qml6 means fewer nodes and effects per item, not cheaper ones.
- Taking a released delegate out of the view costs 13 k (`set_parent_item(None)` and the
  effects that read its parent). qml6 does that too: it sets the parent to null and
  `visible` to false before the delegate goes (checked with `onParentChanged`), so the step
  stays.
- Text shaping of "Row N" is 38 k per text, in parley's builder (28 k), harfrust and
  fontique; fontique's family-name lookup alone is 1.6 k. A reused delegate pays all of it,
  because its text changes.
- A text effect that runs again reads all 19 of its inputs (`TextInputs::read`, 9.2 k), and
  each read records its dependency again. A new delegate reads them twice: once to measure
  at completion and once in the polish effect's first run.
- `Graph::dispose_scope` disposes a scope's nodes one at a time: each is unlinked from
  observers that die with it, and each flushes. That costs about 11 k per delegate.
- The sandbox's driver reads the accessibility tree, so the a11y pass runs in these
  measurements (about 8 k per delegate). A user without assistive technology does not pay it.
- B18's items under 2 k (`part()` in `read_access`, `FieldReads::new`, the font clone,
  `try_read`, `free`, `ComputeGuard`) are still flat in this profile and were not done.

DECISIONs:

- A pooled delegate is left out of the scene, as qml6 does, instead of being made
  invisible.
- Fontique's family-name lookup is left as it is: parley 0.12 has no family given by id.
- The a11y cost in the sandbox is noted, not removed: the driver needs the tree.
- The unparenting of a released delegate is kept, because qml6 does it.

### B20 (H2): fewer runs and nodes per delegate (2026-10-25)

B19's measures again, on the same machine and in the same way: off-screen in the cua sandbox
on the GPU, against `qml6` 6.11.2 with the Basic style. The start is d567e542; the end is this
batch's four commits, rebased onto 6b072a0c. Both are profiling builds (release, line tables),
and the start was measured again next to the end.

**Per delegate, by callgrind.** The main thread, with a 40 s warm-up and a 20 s window.
Delegates are counted as in B19.

| Bench | Binary | Delegates | Instructions | Per delegate | `malloc` per delegate |
|---|---|---:|---:|---:|---:|
| listview | Start | 9833 | 4.17 G | 424.0 k | 203 |
| listview | After the first two commits | 9771 | 3.76 G | 385.1 k | 196 |
| listview | End | 9851 | 3.78 G | 384.2 k | 192 |
| `reuseItems` | Start | 9968 | 1.58 G | 158.3 k | 58 |
| `reuseItems` | End | 9985 | 1.58 G | 157.9 k | 57 |

qml6 makes a delegate of this bench for about 200 k.

| Scroll, M instructions/s (`scr.sh`, back to back) | Start | End | qml6 (same run) |
|---|---:|---:|---:|
| listview | 210.5 | 207.1 to 207.3 | 103.1 |
| listview with `reuseItems: true` | 88.2 | 87.5 | 60.5 |

The scroll rate falls by much less than the per-delegate count: the wheel run makes far fewer
delegates per second than the 500 a second of the bench's own animation, so per-frame work
dominates its count.

| `ljos bench --gesture scroll`, end binary | ljos | qml6 (same run) |
|---|---:|---:|
| listview, CPU while driven (% of one core) | 7.85 | 3.03 |
| rssnews | 3.25 | 2.04 |
| thermostat | 3.22 | 2.64 |
| listview, first frame (ms) | 39.0 | 91.1 |

The load was 2.5 to 14.8 while these ran.

The commits:

- c402861f: `Graph::effect_now` makes an effect whose first run happens before it returns,
  also inside a batch (a frozen scope still waits for the thaw).
- 1bf96414: a Text's 19 inputs are read once as it is made. The effect that follows them is
  made at completion and measures from that read; the scene's polish takes what it read
  instead of reading them all again. A change that does not change the layout (colour,
  style colour, alignment within the width) no longer measures the text again, and a shown
  text has no content effect of its own (features/text/measured-once.feature: two new
  scenarios). The text's binding is evaluated at completion as before, so nothing a binding
  logs moves.
- e0705aee: a scope's nodes are disposed together. An edge between two of its nodes is not
  unlinked, only observers that stay are marked, and marks spread once.
- 787651fa: `Graph::dispose_all` does the same for a few nodes; the scene's `unadopt` and the
  a11y pass's `forget` use it. The nodes taken out go to a list kept for the next disposal.

The text commit and the scope disposal together took 39 k off a new delegate (424.0 k to
385.1 k): the second read of the text's inputs and its polish run, and the text's content
effect, a node and its first run. `dispose_all` took off another 1 k. Disposal is still
about 17 k a delegate, but nearly all of that is dropping the nodes themselves (about 200
instructions each for their closures and the `Rc`s they hold), not unlinking them. A reused
delegate creates and disposes nothing, so it barely moved.

**listview is still 2.0× qml6's instructions per second, and 1.45× with `reuseItems`.**

These are left:

- Text shaping, 49 k of a new delegate's 384 k and most of a reused one's 158 k. For "Row N"
  it is parley's builder (`shape_text` 20 k, `analyze_text` 5 k, resolving the style set
  4.6 k), then 6.8 k to take advances and widths out and 5.6 k to extract the runs. A reused
  delegate pays all of it, because its text changes.
- Adoption is 73 k: anchors 23 k (an item anchored by nothing still reads its 9 lines,
  5.6 k), geometry 21 k (mostly the first evaluation of `width` and `height` bindings),
  inherit 8 k, a11y 5.5 k, place 4.6 k, structure 4 k. Folding anchors into geometry would
  make an effect that writes what it reads; it was not done.
- Creating the delegate (`create_delegate`) is 58 k; the text's follow effect 19.5 k, most of
  it the first evaluation of `text`.
- The a11y pass runs in the sandbox because its driver reads the tree (B19).

DECISIONs:

- A shown Text gets no content effect: its content is made once, untracked, as it is shown,
  and later changes reach it through the text's own effect.
- What a Text read as it was completed is handed to the polish (`text::follow_into`), not
  read again. The Text's effect is disposed when the scene forgets it, so an item off the
  scene follows nothing.
- A change that only paints differently (colour, style colour, alignment within the width)
  does not measure again.
- No cache of shaped runs beyond the one there is. ljos_text already shares shaped runs and
  layouts between delegates, keyed by text, style and constraints, and keeps them 30 frames.
  In this bench every text is new within a sweep, so a cache misses unless it holds a whole
  sweep: one "Row N" shaped and laid out holds 3.2 KB (measured with `TextEngine::memory`
  over 1000 texts), so 10 000 rows are 32 MB to save 49 k a delegate on the way back. Not
  worth it in memory, and not what a real list repeats.
- Batch disposal tells a scope's nodes apart by their scope (a node never changes scope), so
  `Node` grows no flag; it is kept at 224 bytes.

### JS1: JavaScript cost against qml6 (2026-10-25)

Eight small JavaScript-heavy programs (`bench/js1/`), each a `TestCase` that times its own loop
with `Date.now()` and logs it:

- **bindings**: make and destroy an item with seven bindings, through `createObject`;
- **handlers**: a QML signal emitted from a loop, whose handler calls a QML function;
- **arrays**: `map`, `filter`, `sort` with a comparator and `reduce` over 10 000 numbers;
- **strings**: building strings with `+=`, template literals and `join`;
- **json**: `JSON.parse` and `JSON.stringify` of 102 kB;
- **math**: five `Math` calls a step;
- **props**: reads and writes of properties of two other objects;
- **model**: `ListModel.get(i).value` over 1000 rows, and a `setProperty` each round.

Ljós runs them with `ljos test`, a release build. qml6 runs them with Qt 6's
`qmltestrunner` (`/usr/lib/qt6/bin/qmltestrunner`; the one on `PATH` may be Qt 5's), offscreen.
Qt 6.11.2, load 7 to 12.

- **Wall** is the best of 5 runs, in ms of the loop alone.
- **Ir per round** is callgrind's count for a run of 2n rounds minus a run of n, divided by
  n. That takes out start-up and everything before the clock.
- Under valgrind, qml6 runs without its JIT. **Its Ir counts the interpreter**: compare against
  qml6 by wall time. Ir is for Ljós before and after.

To reproduce:

```sh
cargo build --release -p ljos_cli
mise run bench:js                 # all eight, Ljós and qml6
NO_QT=1 mise run bench:js model   # Ljós only, one bench
LJOS=path/to/ljos mise run bench:js
```

| bench | rounds | Ljós before, ms | Ljós after, ms | qml6, ms | Ljós Ir before | Ljós Ir after |
|---|---:|---:|---:|---:|---:|---:|
| bindings | 20 000 | 134 | 132 | 197 | 93.6 k | 92.8 k |
| handlers | 1 000 000 | 1036 | 435 | 297 | 16.6 k | 6.78 k |
| arrays | 50 | 147 | 149 | 114 | 55.6 M | 55.6 M |
| strings | 200 | 52 | 54 | 99 | 6.69 M | 6.83 M |
| json | 40 | 88 | 90–106 | 124 | 46.4 M | 46.3–53.6 M |
| math | 1 000 000 | 199 | 199 | 103 | 3.20 k | 3.21 k |
| props | 1 000 000 | 359 | 367 | 252 | 6.73 k | 6.74 k |
| model | 100 | 53 | 26 | 15 | 7.33 M | 3.69 M |

**Ljós is faster than qml6 on bindings, strings and json.** It is slower on handlers (1.5×),
math (1.9×), model (1.7×), props (1.5×) and arrays (1.3×).

The commits:

- 5727bbb2: a signal read from a script is one small callable object. It used to be three
  named functions (the emitter, `connect` and `disconnect`), half of an emit's cost. As on
  Qt, `connect` and `disconnect` are on `Function.prototype`, and each read is a new function
  with no properties of its own (features/language/signals-read-from-script.feature, which
  also passes on qml6). handlers: 1036 → 539 ms.
- 4eaf2551: `ListModel.get()` of a row whose object exists brings the object up to date only
  when the model has gained roles since. Every change to a row already updates its object.
  `get()` used to write every role again, and rebuild the object's shape, on each call.
  model: 53 → 29 ms.
- 42ce35dd: a QML method read from a script is a plain C function with the object and the
  method in its data, instead of an rquickjs closure. That saves about 425 Ir a call.
  model: 29 → 26 ms.
- 1a97c3f4: tier 1 keeps the property a write goes to per type, as it keeps a member read,
  instead of looking it up by name on every run. Its write of a number, bool or text to a
  plain property goes the way a script's does (`plain_script_write`), without the looks for
  an alias, a reset, a group, a setter or a Behavior. A signal's number and bool arguments
  are read off their tag. handlers: 7.52 k → 6.78 k Ir; 502 → 435 ms at the same load
  (load 8).

These are left:

- **arrays and math** spend almost all of their instructions in QuickJS's interpreter
  (`JS_CallInternal`, `js_call_c_function`, `find_own_property`, its sort). qml6 JIT-compiles
  the same loops. Nothing of Ljós's is in the way.
- **json** depends on where the heap's boundaries fall. quickjs-ng frees a 4 KiB arena of
  small blocks when its last block goes, and builds a new one for the next allocation. A
  loop that allocates and frees right at that boundary rebuilds about 800 arenas a round
  (`arena_new`, 4.3 M Ir). The final binary hits it with `bench/js1/json.qml` and the base
  does not; with the rounds as a constant instead of a property, neither does. The fix is in
  QuickJS (keep one empty arena per size), which rquickjs-sys 0.14 builds without a switch
  for it.
- **handlers**: what remains of an emit is the handler. The tier-1 VM runs
  `total += step(i)` and the call to `step` (about 2.8 k Ir: the register loop itself, its
  frame and the values it drops), the write to `total`, and the emit's dispatch.
- **props**: about 1.1 k Ir per read or write of another object's property, split between
  the host accessors (`answer::get` and `answer::set`, `raw_plain`) and the engine's
  `quick_script_read` and `plain_script_write`. The engine side overlaps H2's
  `read_access` work and was left to it. A write to `b.x`, which has a node, is about
  1.1 k on its own: the graph write, the flush its batch ends with (an item's `x` has an
  effect to run), and the field it updates.
- **model**: about 3.7 k Ir per `get(i).value`. That is the method call (1 k in
  `EngineHost::call` and `Engine::invoke`), the read of `.value` on the row object
  (0.85 k), and the read of `m.count` in the loop condition.

**samegame's new game** (`Logic.startNewGame(gameCanvas)`, Main.qml:114 of the qtdoc demo)
was reported at 270 ms, over its 32 ms soft budget. It is timed by a `TestCase` that makes the
demo's `GameArea` at 800×600 (450 blocks) and calls `startNewGame` six times. To run it under
qml6 the `content` directory needs a `qmldir` that makes `Settings` a singleton. The best of
two runs of six, load 7 to 13:

| build | ms per new game | per round, Ir |
|---|---:|---:|
| Ljós, release | 21–27 (best 21) | 266 M |
| qml6 | 20–26 (best 20) | — |
| Ljós, debug | about 92 for 150 blocks | — |

The 270 ms is a debug build's: `gallery:drive` builds debug unless `GALLERY_PROFILE` says
otherwise, and a debug build warns at eight times the soft budget (256 ms). Released, the
handler is within its budget and level with qml6. Its cost is spread over making each block's
fifteen or so objects: `createObject` is 172 M Ir a round (380 k a block), `Image`'s
completion 39 k an image (its pictures come from the cache: 13 loads for 1360 images), and
the destruction of the last game's blocks 54 M. Nothing takes more than 6 % on its own; the
largest share is malloc and free. That is the per-object cost of creation, H2's.

DECISIONs:

- Qt is compared by wall time only. Under valgrind its JIT is off, so its Ir is not what it
  runs.
- `connect` and `disconnect` go on `Function.prototype` with the first host type, not when
  the host is made. A host without objects is left as QuickJS makes it, and the 8 MiB
  out-of-memory test runs right at its limit.
- QuickJS's arena allocator is left unpatched.
- props is left as it is: what is left of it is the engine's read and write, H2's.


### Slow handlers of a debug build: shadows of sources with items in them (2026-10-25)

In debug-build drives of the corpus, coffee's theme toggle took 512–862 ms
(`CustomToolBar.qml:11`). Two of mediaplayer's handlers took 426–533 ms: the playlist button
(`Main.qml:292`) and the theme radios (`ThemeInfo.qml:27`, `:34`). These figures are the "JS
dispatch exceeded its soft budget" lines of the gallery:drive logs of 10-18 and 10-24. Other
work has made them smaller since. They were measured again off-screen in the desktop:cua
sandbox, in release and debug builds, with the hot-bindings mode of `app_frames`:
`{"bindings": true}` times each binding and handler that ran. The handler was run by
`app_eval` (`clicked()` on its button) and timed there with `performance.now()`. Coffee was
measured on the page `getStartedButton` pushes, which has the cards. Mediaplayer's radios were
clicked by reference from a snapshot, because its control bar moves with the pointer.

**The cause.** Both apps put a `MultiEffect` shadow on a `Rectangle` that has items in it:
coffee's cards and circles, and mediaplayer's settings panel. Two things were wasted:
- **A picture thrown away.** Each colour written to such a source made the effect draw the
  source alone, as a picture, and compose its shadow, inside the handler. In the same frame,
  the scene drew the source again from its whole subtree and threw that picture away.
- **A hidden effect.** An effect that was hidden was still drawn. Mediaplayer hides its
  settings panel when the playlist opens or closes.

**The fixes** (`crates/ljos_quick`):
- **The scene draws the source.** An effect whose source has items in it leaves the drawing
  to the scene (`a_source_with_items_in_it_is_left_to_the_scene_to_draw`).
- **Hidden effects wait.** The scene draws an effect only while it and the items it is in are
  shown, or while a `ShaderEffectSource` or a layer samples it. Such an effect is drawn again
  once it is shown (`a_closed_panel_s_shadow_is_drawn_again_only_when_it_is_shown`,
  `an_effect_in_a_hidden_item_a_shader_effect_source_samples_is_drawn`).

Each handler, run four times (the last three for the radios):

| handler | build | before | after |
|---|---|---|---|
| coffee, theme toggle | release | 15.3 ms first, then 3.5–5.2 | 1.1 ms first, then 0.52–0.59 |
| coffee, theme toggle | debug | 48–53 ms | 5.9 ms first, then 2.1–3.1 |
| mediaplayer, playlist open and close | release | 0.42–1.13 ms | 0.23–0.49 ms |
| mediaplayer, playlist close | debug | 26–32 ms | 1.0–1.3 ms |
| mediaplayer, theme radio | release | 5.1–5.9 ms | 3.9–4.5 ms |
| mediaplayer, theme radio | debug | 64–128 ms | 31–40 ms |

Coffee's page push went from 7.2 to 3.8 ms in release. In debug it stayed at 44 ms.

**Left as the app's.** A radio still costs about 4 ms in release, about the 4 ms soft budget.
That is the app's own theme change: the handler runs about 100 bindings that set colours
across the window, and the panel's shadow is shown, so it is drawn. This was not broken down
further. A debug build is 8–10 times slower than release at this. `vello_cpu` is built at
opt-level 3 in debug, but `vello_common` and `fearless_simd` are not. Building them at
opt-level 3 too is an idea not tried.

DECISIONs:
- The 512–862 ms figures were taken from the older drives' logs. They were not measured
  again on those trees.
- There is no features scenario. What changed is when a picture is made, and QML cannot see
  that: the shadow shown is the same before and after. The behaviour is pinned by the tests
  named above.

- samegame's new game is left as it is: a release build is within the budget, and what it
  costs is the creation of objects, H2's.

### mediaplayer's theme radio: colours bound on a menu bar (2026-10-25)

The radio's handler is one write, `Config.activeTheme = Config.Theme.Light`, to a file singleton.
In release it took 3.9–4.5 ms in the desktop:cua sandbox (llvmpipe).

#### Where the time went

Measured with the observer's hot bindings (`app_frames {"bindings": true}`) on the click, and by
editing copies of the app:
- **The bindings, 0.3 ms.** About 100 of them run, at 0.04 ms or less each. The handler's own
  time was 4.08 ms of its 4.39 ms.
- **Writes and dirtying, about 3 ms.** These are the menu bar's colours. `PlayerMenuBar.qml` binds
  `palette.base`, `.text`, `.highlightedText` and `.highlight` to the singleton, on the `MenuBar` and
  on each `Menu`. With those lines made constant, the handler took 0.9–1.5 ms. The images and the
  colour bindings elsewhere cost 0.12–0.2 ms a switch in a repro of 24 SVG `Image`s and 70
  `Rectangle`s.
- **Drawing.** The frames after the click are the ones playback draws (p50 4.0–4.6 ms, from the
  media slider), not the switch's.

The cause was in `ljos_controls`' palette inheritance:
- **Every control looked again.** A bound colour changed its value, and it moved the epoch that
  tells every control in the scene to resolve its palette. That happened once for each colour, and
  each resolve walks the control's ancestors for every role and group.
- **Every plain item recomputed every slot.** Each colour then handed down all 84 slots of every
  plain item below the bar, not only the role that changed.

#### The fix

- **A colour that was already the author's does not move the epoch.** A control that took it read
  it as it resolved, so it runs again on its own. Only a colour that was not the author's before is
  news to the rest of the scene.
- **The plain items below are handed only the role that changed.**

`a_bound_colour_that_changes_costs_what_is_below_it` (`crates/ljos_controls/tests/controls.rs`)
pins this. A switch beside 80 buttons ran 181 effects before, against 41 beside 10. The
scenario in `features/controls/control-look-later.feature` checks against qml6 that the colours
still reach the controls and plain items below.

#### Numbers

Release. The repro is a Fusion window with the app's `MenuBar` (5 bound colours), 3 `Menu`s of 4
items (3 bound colours each) and 40 `Button`s and 40 `Label`s (`frame_profile --start`, one switch,
then three more):

| | switch | effect runs |
|---|---|---|
| before | 12.4 ms | 1929 |
| the epoch kept | 3.5–3.9 ms | 184–210 |
| and one role handed down | 1.2–1.7 ms | 184–210 |
| without the menu bar | 0.08–0.49 ms | 1 |

The same scene as a `tst_*.qml`, 400 switches with nothing drawn: **qml6 0.50 ms a switch, Ljós
1.12 ms.**

mediaplayer's radio, from the hot bindings in the sandbox: **3.9–4.5 ms before; 2.7 ms on the
first switch after, then 1.41–1.42 ms.**

#### Left

The rest is each `Menu`'s colours. A popup copies the colours set on it to its item, and the
item's colours back to the popup (`popup/item.rs` `mirror_look`). Each copy runs the item's
resolve again. This accounts for most of the 210 effects. `Engine::provenance` is 12 % of what
is left in callgrind.

### A theme or font switch against qml6 (2026-10-25)

This follows the section above. The theme scene is the same Fusion window: a `MenuBar` with 5
bound colours, 3 `Menu`s of 4 items with 3 bound colours each, 40 `Button`s and 40 `Label`s. Each
`tst_*.qml` runs 400 switches with nothing drawn, through `ljos test` and through qml6's
`qmltestrunner` (release builds of both).

#### A theme switch

- **A menu's colours reach its item once.** A popup gave each colour set on it to its item with a
  watch of its own, and read all 84 of the item's colours back each time one changed. Now one
  effect hands the popup's colours down in a batch. The item's colours go back one at a time,
  and only when they change. A colour that the item makes its own without a change no longer
  moves the palette epoch.
- **The plain items in a menu look once.** The item's watch and its groups' watch each handed a
  changed colour down on its own. Each object's roles now go down together.
- **Frozen values are made once.** A colour or font that comes back across the JS boundary
  reuses one of the last four values made for that kind.

`colours_bound_on_menus_cost_what_each_menu_holds` (`crates/ljos_controls/tests/controls.rs`)
pins the count. With 8 menus, the first switch went from 1022 effects to 368, and every later
switch runs 368.

| | Ljós | qml6 |
|---|---|---|
| before | 1.12 ms | 0.50 ms |
| a menu's colours reach its item once | 0.845 ms | 0.51 ms |
| and the plain items look once | **0.405 ms** | 0.51–0.53 ms |

With `frame_profile`, Ljós takes 0.98 ms for the first switch, then about 0.50 ms with 189
effect runs. The first switch was 1.2–1.7 ms before, with 184–210 effect runs.

#### A `font.*` change on a root that controls inherit from

The root is a `Pane` whose font is written by the test. One write is `font.bold`, the other a
`font.pixelSize` that a property binds.

| scene | Ljós bold / size | qml6 bold / size |
|---|---|---|
| empty root, before | 0.005 / 0.013–0.015 ms | 0.005 / 0.003 ms |
| 80 labels, 40 buttons, 20 checks, 20 fields, before | 0.320–0.325 / 0.352–0.365 ms | 0.258–0.275 / 0.292–0.315 ms |
| the same, after | 0.297–0.315 / 0.33–0.357 ms | as above |
| 80 labels | 0.125 / 0.133 ms | 0.125 / 0.130 ms |
| 40 buttons | 0.152 / 0.165 ms | 0.135 / 0.150 ms |
| 20 checks | 0.040 / 0.050 ms | 0.063 / 0.087 ms |
| 20 fields | 0.092–0.095 / 0.168–0.177 ms | 0.058 / 0.065 ms |

The changes:
- **Reading and writing `font`.** Reading the root's font went from about 109K to 12K
  instructions. A member write through the value went from 196K to 34K. `__ljos_with` keeps
  the value it made last for each source.
- **A field resized by its font is placed once.** A `TextField` whose height follows the font
  placed its text twice. The first placing was painted, and then the size moved.
  `a_field_whose_height_follows_a_new_font_is_painted_once` pins this at 10 effect runs a
  field, down from 11.

#### Left for a font switch

- **Fields cost more than on qml6.** A size change costs about 117K instructions per field: 90K
  in the editor's effect, of which the relayout is 75K.
- **Buttons are 10 % slower.**
- **Labels and checks** are even with qml6, or faster.

#### mediaplayer's theme radio

The theme tab is shown in the desktop:cua sandbox (llvmpipe, release). The switch is
`Config.activeTheme` written by an eval with the observer's recording off.

| | switch |
|---|---|
| before this section, from the hot bindings | 1.41–1.42 ms |
| from the hot bindings | 4.5 ms on the first switch, then 1.06–1.38 ms |
| eval, recording off | 1.01–1.33 ms |
| eval, playback paused | 1.02–1.44 ms (the first 1.82 ms) |
| the same app off-screen (`frame_profile`) | 0.30–0.39 ms |

callgrind counts 4.46M instructions a switch in the sandbox and 3.85M off-screen. The off-screen
profile breaks down as:
- `Engine::provenance`: 329K. Of that, 213K is `resolve_palette`, which asks it 2496 times.
- Six palette resolves: about 161K each.
- `try_read`: 136K.

The wall time in the sandbox is about three times the off-screen time, for only 16 % more
instructions. Pausing playback does not close that gap. **The 1 ms target holds off-screen and
not in the sandbox.**

### `vello_common` and `fearless_simd` at opt-level 3 in a debug build (2026-10-25)

The question was whether to build `vello_common` and `fearless_simd` at opt-level 3 in the dev
profile. `vello_cpu` is built that way already. The bar: change the profile only if a clean debug
build gets no more than 10 % slower. The build has two `vello_common`s (0.1, through
egui's `epaint`; 0.3, through `vello_cpu` and `glifo`) and three `fearless_simd`s. A profile entry
by name covers each of them.

**Build.**
- **A clean debug build of `ljos`.** Measured with `SCCACHE_RECACHE=1`, so every crate compiles, in
  a target dir of its own. It took about 71 minutes on a machine other builds shared: 50 minutes
  until the timeout, and 21 to finish. Load was up to 60 on 32 threads.
- **The five crates on their own,** cleaned and built again with everything else built:

  | opt-level | time |
  |---|---|
  | 0 | 82 s (a first run of 590 s timed out under load) |
  | 3 | 136 s, 136 s |

  So 54 s more at most, about 1.3 % of the clean build even if all of it is on the critical path.
- **Switching the profile on a built tree** compiled everything above them again in 547 s.

**Handlers and frames.** These are debug builds of the same tree, run in the desktop:cua sandbox.
Each was run twice, and the two builds took turns. The figures are mediaplayer's: the settings
button, the theme tab, and four theme radios.

| | settings button | theme radio | frames while playing, p50 |
|---|---|---|---|
| as now | 4.4–4.5 ms | 6.0–9.3 ms | 5.3–6.0 ms |
| at opt-level 3 | 4.8–4.9 ms | 6.1–11.7 ms | 5.7–6.2 ms |

No gain: the differences are within the noise of the runs. What a frame runs of these crates is
mostly generic. It is made, and optimised, in `vello_cpu`, which is built at opt-level 3 already.

DECISION: the dev profile is left as it is. The build would cost little (about 1 %), but nothing
measured got faster.

The theme radio in debug went from 31–40 ms to 6–9 ms with the palette fix above.

### B21 (H2): per-frame cost while scrolling (2026-10-25)

B20's measures again, on the same machine and in the same way: off-screen in the cua sandbox
on the GPU, against `qml6` 6.11.2 with the Basic style. The start is B20's end; the end is
this batch's two commits on 219fedfa. Both are profiling builds (release, line tables), run
back to back.

**First, which frames cost.** B20 guessed that per-frame work, not making delegates, set the
wheel run's count. For listview that is wrong. The bench's own animation moves `contentY`
about 400 px a frame under callgrind, so each frame makes and drops about 10 delegates. Its
4.0 M instructions a frame (1008 frames, 9862 delegates) are about 396 k per delegate, and
an in-buffer frame is 0.5 M. A scroll that makes no delegate was measured on its own with
`lvstill.qml`: a ListView of 30 rows with `cacheBuffer: 4000`, whose `contentY` runs 0 to 600
and back every 2 s, so all its delegates are made once and kept.

| An in-buffer scroll frame (main thread, callgrind) | Start | View pass kept | End |
|---|---:|---:|---:|
| Whole frame | 498 k | 408 k | 395 k |
| The view's layout effect | 146 k | 26 k | 26 k |
| Finding what changed (`LastPicture::change`), per frame | — | 42 k | 21.6 k |

Where an end frame goes:
- Paint, 156 k. Of that, 113 k is the driver's `eglSwapBuffers` (mesa); on a device a scroll
  is drawn whole for 30 frames (`WHOLE_RUN`).
- egui's pass, 149 k:
  - the scene's frame, 76 k: the animation tick and its effects 26 k, `compose` 24 k,
    `apply_content` 9 k;
  - b0 `encode`, 44 k: a translated list is a new segment key, so its world segments and
    glyph meshes are built again;
  - egui itself, about 17 k.
- `prepare` (fill rectangles), 22 k.
- `LastPicture::change`, 21.6 k.
- Tessellation, 6.8 k.

| Scroll, M instructions/s (`scr.sh`, back to back) | Start | End | qml6 (same run) |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 25.65 | 17.51 | 13.32 |
| listview | 205.49 | 206.13 | 99.87 |
| listview with `reuseItems: true` | 85.06 | 84.79 | 59.08 |

The in-buffer scroll fell by a third. listview and `reuseItems` did not move: every frame
of theirs brings a row in, and that frame runs the full pass.

| `ljos bench --gesture scroll`, CPU while driven (% of one core) | Start | End | qml6 (with the end) |
|---|---:|---:|---:|
| listview | 7.28, 10.6, 9.42 | 9.51, 5.49, 6.34 | 5.40 |
| rssnews | 3.22, 7.81, 12.3 | 3.06, 6.54, 11.3 | 4.91 |
| thermostat | 3.86, 2.68, 2.45 | 2.51, 2.25, 3.53 | 3.49 |

Three runs each, with the load at 3 to 57 while they ran: other workers were building on
the same machine. These numbers do not separate the two binaries; the instruction counts
above do.

The commits:

- b7b94025: a ListView scrolled with the same rows on both sides of its window lays none
  out. The last pass records what it laid out with: geometry, count, the delegate trigger,
  reuse, the current item's span. A new effect follows only the view's position. When the
  position moves and the rows the pass kept are still exactly those the window needs (the
  first begins at or before it, the last ends at or after it, and neither neighbour does),
  the effect only culls, keeps the anchor row, moves the current item's span and updates
  `currentSection`. Anything else runs the full pass as before. GridView still always runs
  it. `a_row_appended_and_shown_at_the_end_is_laid_out_once_each_way` now counts three
  passes, not four: going to the end no longer lays the rows out a second time.
- 2e53a1ed: the present layer tries up to three clips and distances for a scroll. Each one
  after a fit now gets the best one's areas as its budget, so it stops as soon as it cannot
  win. The scroll picked is the same.

**listview is still 2.1× qml6's instructions per second, and 1.4× with `reuseItems`;
the in-buffer scroll is 1.3×.**

These are left:

- A new delegate is still about 396 k. The parts are:
  - Text measured at completion, 64 k: parley's builder 28 k, `ranged_builder` 4.8 k, line
    widths 6.8 k, extracting runs 5.7 k.
  - Adoption, 71 k: geometry 13 k on the Rectangle, anchors 5.5 k on the unanchored
    Rectangle, inherit 4.2 k, a11y 2.3 k, place 2.2 k, structure 2 k, and the same again
    for the Text.
  - Completion and binding evaluation, the rest.
- Anchors on an item anchored by nothing: 5.5 k. The nine tracked reads (3.7 k) are what
  lets a later `anchors.*` write, a `PropertyChanges` or an `AnchorChanges` reach the item.
  Skipping them needs a hook on every path that can set a line.
- Geometry's first run (8.2 k an item): 6 k of it is the first evaluation of the item's
  own `width`/`height` bindings (`ListView.view.width`), JS work that happens once either way.
- Looking up each field's native part once per multi-property read (`read_values`,
  `read_numbers`) was tried. It took 54 M off `read_access` but nothing off a delegate
  (396 k before, 400 k after), so it was not kept.
- A text culled for more than 30 frames loses its layout (`KEEP_FRAMES`) and its glyph runs
  (ARCHITECTURE §7.5). Coming back into view lays it out again: 151 times in 950 frames of
  `lvstill`, about 8 k a frame on average.
- b0 `encode` rebuilds a translated list's world segments, 44 k a frame. Moving segments by
  the translation instead is the next per-frame cut.

DECISIONs:

- `view_layout_passes` counts full passes only; a scroll that keeps the rows is not one.
- No features/ scenario: the fast path changes no order or timing QML can see. Position,
  `indexAt`, culling, `currentSection` and the current item come out as the full pass gives
  them. The Rust test `a_scroll_inside_the_cache_buffer_lays_no_rows_out` pins the pass
  count and positions.
- The fast path is ListView only. GridView keeps the full pass.
- Off-screen text keeps dropping its glyph runs (§7.5).
- The GL painter's whole-frame run during a scroll is kept. A swap with damage costs the
  driver more per frame (291 k against 119 k) than a whole one.

### B22 (H2): a new delegate's text, a moved node's meshes, GridView's in-buffer scroll (2026-10-25)

B21's measures again, on the same machine and in the same way. The start is a profiling
build from before this batch's commits; the end has all three. Both are release builds with
line tables, run back to back, with the load between 7 and 25 from other workers' builds.

| listview under callgrind (main thread, 20 s after 40 s) | Start | End |
|---|---:|---:|
| Per delegate (total ÷ text layouts) | 385.6 k | 386.6 k |
| Text layout (`TextEngine::build_layout`) | 57.1 k | 53.1 k |
| — Parley's `ranged_builder` | 4.8 k | — |
| — resolving the font family | 6.4 k | 1.9 k |
| — Parley's build (shaping, line breaking) | 28.5 k | 28.2 k |
| Adoption (`QuickScene::adopt`) | 74.1 k | 75.2 k |
| b0 `encode`, per frame | 144 k | 161 k |

| `lvstill.qml` under callgrind (in-buffer, every delegate kept) | Start | End |
|---|---:|---:|
| b0 `encode`, per frame | 43.4 k | 38.7 k |
| World segments built again, in about 750 frames | 11 613 | 400 |

A delegate did not get cheaper; its text did, by 4 k, and the rest moved by as much from run
to run (a third end run gave 383.9 k). listview's `encode` per frame rose because that run
made 14.3 delegates a frame against 13.6, and a new node builds its meshes either way; only
nodes that were kept are moved.

| Scroll, M instructions/s (`scr.sh`, back to back) | Start | End | qml6 (same run) |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.48 | 19.43 | 15.22 |
| listview | 209.56 | 212.18 | 102.93 |
| listview with `reuseItems: true` | 89.08 | 88.85 | 60.60 |

The scroll rates did not move. CPU % was not measured: the machine was not quiet.

The commits:

- e6bf4c01: a GridView scrolled inside its cache buffer lays no cells out. It kept its
  laid-out cells only when every line held all its cells; it now takes ListView's in-buffer
  path, which only culls when the same lines are on both sides of the window, and leaves
  its sections alone. The Rust test
  `a_grid_scrolled_inside_the_cache_buffer_lays_no_cells_out` pins the pass count and the
  cells' positions.
- ed70f6af: b0 keeps a node's meshes when only its translation changed, and moves them. A
  node whose list and opacity are the same and whose transform differs only in its offset
  translates its meshes in place, shifts its text rects and carries its glyph meshes when
  they still land on the same pixel phase. A shape or shader segment, or a node moved 32
  times in a row, is built again. The new `world_moves` counter counts moves. The test
  moves a rounded rect 40 times by 2.37 px and checks each frame against a fresh backend to
  1e-3, with one tessellation.
- d6bf1e3d: a text with no spans and no objects goes to Parley through its style-run
  builder with one full style. Parley's ranged builder resolves a default style, with its
  `sans-serif` family, and then each property. The Qt and measured advances read the run's
  glyphs from their iterator instead of collecting them. The test shapes nine strings
  (Latin, Greek, Cyrillic, Hebrew, Arabic, combining marks, CJK, empty, two lines with
  spaces) in three styles, wrapped and not, with bundled and system fonts. It checks that
  each comes out as the ranged builder gives it: line measures, ranges and every glyph.

**A new delegate is still about 386 k, against qml6's ~200 k.** Where it goes, per delegate:

- Text measured at completion, 61 k. Parley's build is 28 k of it: shaping a unique string.
  The bench's strings are all different, so the shaped-text cache (keyed by text and style)
  cannot help.
- Adoption, 75 k:
  - anchors, 20.7 k for the two items: nine tracked reads and the `verticalCenter`
    binding (`read_values<9>` 12.8 k; the binding's first run 5.3 k, 17 field reads
    3.4 k at 195 each, `FieldReads::add` 1.4 k);
  - geometry, 18 k;
  - inherit, 5.8 k;
  - a11y, 5.7 k.
- Destruction, about 62 k: `sync_parent` 13 k, graph disposal of about 30 nodes, the header
  drop 7.9 k.

DECISIONs:

- No features/ scenario for any of the three: none changes what QML can see. GridView's
  positions, culling and current item come out as the full pass gives them; b0's frames
  match a fresh build to 1e-3 px; a plain text shapes glyph for glyph as before.
- No shape cache by font, size and string beyond the existing one. It is already keyed so,
  and the bench's strings do not repeat. A cache of shaped words would break Parley's
  shaping across word boundaries (kerning, Arabic joining), and §7.5 limits what a culled
  text may keep.
- Looking up each native part once per multi-property read (`NativeReads`) was tried again
  for the anchors' reads. It took the `part` calls from nine per item to one, but a read
  still cost 196, not 195: the cost is the group's getter, its own lookup, and the value's
  clone and drop. Not kept. A cheaper anchors read needs the anchors group to hand back its
  nine lines in one call; every path that sets a line (`anchors.*`, `PropertyChanges`,
  `AnchorChanges`) already writes those fields, so they stay the tracked reads.
- A move stops after 32 in a row (`MAX_MOVES`), so float error from repeated translation
  does not build up.

### Lottie: a mid-sized animation against qml6 (2026-10-25)

`bench/lottie.qml` is a 512×512 black `Rectangle` with one `LottieAnimation` that loops
`bench/lottie/mid.json` for ever. `bench/lottie/gen.py` writes that file: 57 kB, 60 fps, 180
frames, 40 shape layers, and a precomp used four times. Every layer moves, so each frame is
new and nothing can be cached. Both sides draw the same file: Ljós with `ljos_lottie`, qml6
with `Qt.labs.lottieqt` 1.0.

To reproduce:

```sh
python3 bench/lottie/gen.py
BENCH_DEFAULTS="--warm-up --repeat 3" mise run bench lottie
cargo run --release -p ljos_lottie --example frame_time -- bench/lottie/mid.json
```

The figures are medians of 3 runs. Ljós was 166b4d96, a release build with thin LTO, and Qt was
6.11.2. Both ran in headless sway with pixman (llvmpipe), off-screen, on an AMD Ryzen AI Max+ 395.
Load average was 3.9 to 39.6.

Desktop profile:

| Measure | Ljós | Qt | Ljós/Qt |
|---|---:|---:|---:|
| Time to first frame | 182 ms | 651 ms | ×0.28 |
| CPU to first frame | 131 ms | 285 ms | ×0.46 |
| Peak memory while starting | 106 MiB | 478 MiB | ×0.22 |
| Settled memory | 73.7 MiB | 130 MiB | ×0.57 |
| Settled private PSS | 48.9 MiB | 98.2 MiB | ×0.50 |
| Settled PSS | 121 MiB | 220 MiB | ×0.55 |
| Memory after idling | 61.8 MiB | 129 MiB | ×0.48 |
| CPU while animating | 40.5 % | 54.1 % of a core | ×0.75 |
| Wakeups | 1308/s | 39226/s | ×0.03 |
| Frames | 62.1/s | 61.9/s | ×1.00 |
| 95th percentile frame interval | 16.1 ms | 16.2 ms | ×1.00 |
| CPU while driven | 41.5 % | 53.7 % | ×0.77 |
| Input-to-frame latency, median | 8.26 ms | 7.18 ms | ×1.15 |
| Mapped install | 387 MiB | 487 MiB | ×0.80 |

pi-zero profile (256 MiB, 20 % of one core):

| Measure | Ljós | Qt | Ljós/Qt |
|---|---:|---:|---:|
| Time to first frame | 526 ms | 1140 ms | ×0.46 |
| CPU to first frame | 106 ms | 213 ms | ×0.50 |
| Peak memory while starting | 70.4 MiB | 151 MiB | ×0.47 |
| Settled memory | 53.6 MiB | 108 MiB | ×0.50 |
| Settled PSS | 153 MiB | 157 MiB | ×0.98 |
| CPU, animating untouched | 18.8 % | 19.1 % | both at the quota |
| Frames, animating untouched | 30.0/s | 33.0/s | ×0.91 |
| 95th percentile frame interval, untouched | 45.9 ms | 40.0 ms | ×1.15 |
| Slowest frame interval, untouched | 56.1 ms | 78.8 ms | ×0.71 |
| Frames while driven | 31.1/s | 25.8/s | ×1.21 |
| Slowest frame interval while driven | 60.2 ms | 110 ms | ×0.55 |
| Wakeups | 756/s | 17353/s | ×0.04 |

`frame_time` times only Ljós's own work, without drawing: parsing the file, and turning one
frame into paths.

| Step | Time |
|---|---:|
| Parse `mid.json` | 0.96–1.11 ms |
| One frame to paths, mean | 29.7–29.8 µs |
| One frame to paths, worst | 57–79 µs |
| Paths per frame | 83.7 |

- Interpreting the Lottie costs about 30 µs of a 16.7 ms frame. Most of Ljós's 40 % of a core
  goes to filling those paths in software under llvmpipe, which is the same fill that Shapes and
  SVG use.
- On the desktop both keep 62 frames a second. Ljós does it on three quarters of qml6's CPU,
  with half its memory and 3 % of its wakeups.
- On the pi-zero profile both hit the 20 % quota. Left alone, qml6 keeps three more frames a
  second. Under input, Ljós keeps five more, and its slowest interval is half of qml6's.
- Input-to-frame latency is 1 ms slower on Ljós. The click lands on a `Rectangle` with no
  handler, so this measures how long a full redraw takes to reach the screen.
- qml6 never stops its frame clock while the animation runs. Ljós sets one deadline a frame,
  and none once a finite animation has stopped or while it is hidden. The "nothing scheduled"
  scenarios in `features/lottie/` pin that.

### B23 (H2): the delegate track's baseline (2026-10-25)

B18–B22 made each piece of a new delegate cheaper and the total stayed near 386 k, against
qml6's ~200 k. The delegate track now removes work instead: the scene's bookkeeping moves
out of the reactive graph ([ADR 0001](adr/0001-scene-bookkeeping-outside-the-graph.md)) and
what every instance of a component shares is linked once
([ADR 0002](adr/0002-component-plans.md)). This batch records where it starts. Its code
only counts: `Graph::census`, the `lookup-counts` feature of `ljos_engine` and
`crates/ljos_quick/tests/budget.rs`. The profiling build is 483042e5 with them, in which the
counting compiles to nothing.

**Budgets, pinned exactly in `budget.rs`** (ten instances, after the component has been
instantiated once; per instance in brackets):

| | ListView delegate (`bench/listview.qml`'s) | Item with a Text, by `createObject` |
|---|---:|---:|
| Engine objects | 40 (4) | 20 (2) |
| Graph values | 80 (8) | 60 (6) |
| Graph bindings | 70 (7) | 20 (2) |
| Graph effects | 160 (16) | 150 (15) |
| Graph scopes | 20 (2) | 20 (2) |
| Member lookups by name | 191 (19.1) | 61 (6.1) |
| Type lookups by name | 20 (2) | 10 (1) |
| Context name lookups | 0 | 0 |
| Atoms interned | 0 | 0 |
| Heap allocations | 1866 (186.6) | 1057 (105.7) |

The delegate's 186.6 allocations were 185.6 when the pin was set; a graph node that went
from 216 to 200 bytes keeps an override's id beside its layers (d574f1d5), one more allocation
per delegate for 16 bytes less on every node.

Most effects are the scene's, not the delegate's: the plain Item and Text have two
bindings and fifteen effects. ADR 0001 takes those down; ADR 0002 takes the lookups to zero.

| listview under callgrind (main thread, 20 s after 40 s) | Run 1 | Run 2 |
|---|---:|---:|
| Per delegate (total ÷ text layouts) | 401.1 k (3.374 G / 8413) | 409.6 k (4.007 G / 9782) |
| Delegates per frame | 11.3 | 10.3 |
| Adoption (`QuickScene::adopt`) | 74.9 k | 74.5 k |
| Text layout (`TextEngine::build_layout`) | 53.0 k | 53.0 k |
| Teardown: `Engine::destroy_now` | 30.2 k | 29.9 k |
| — `ViewShared::destroy` | 18.5 k | 18.5 k |
| — `QuickScene::unadopt` | 16.0 k | 15.8 k |

The parts are B22's end to within 1 k. The total is higher than B22's 386.6 k because these
runs made fewer delegates a frame (B22's made 14.3), so each one carries more of the
per-frame work (encode alone is about 150 k a frame); per delegate, the target stays the
parts above. qml6 makes one for about 200 k (B20).

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós | qml6 (same run) |
|---|---:|---:|
| `lvstill.qml` (in-buffer) | 19.40 | 15.32 |
| listview | 211.93 | 102.92 |
| listview with `reuseItems: true` | 88.64 | 60.55 |

The target of the track: under 200 k a new delegate, and scroll at or under qml6's rates.

DECISIONs:

- The lookup counters are a cargo feature, not `cfg(test)`: the test that reads them is in
  `ljos_quick`, which links `ljos_engine` as a normal crate. Without the feature
  `ljos_engine::lookups()` does not exist, so a test cannot read zeros that were never
  counted. `ljos_quick` turns it on for its tests only; in a workspace test build it is on
  for every crate, which costs a thread-local add per lookup.
- Atoms interned are counted beside lookups: `Atom::intern` hashes its text, and the plan
  should leave none per instance.
- Allocations are counted per thread by the test binary's own global allocator, and each case
  instantiates its component once before it counts, so the process-wide caches (fonts, the
  text engine) are warm whichever test runs first. The counts are the same over repeated
  runs, in parallel and with one test thread, and with 2, 4 or 40 frames after the step.

### MEM1: resident memory against qml6, and what driving adds (2026-10-25)

Fifteen corpus examples, off-screen in the cua sandbox (headless sway, pixman), Ljós release
builds against `qml6` 6.11.2 with the Basic style. Each app is measured after its first frame
has settled ("start"), then driven for 60 s with `--gesture click` (a grid of clicks over the
window) and measured again ("end"). The machine was short of memory and swapped while these
ran, so RSS at the end says more about the kernel than the app. "Private" is
`Pss_Anon + SwapPss` from `smaps_rollup`: what the process holds of its own, swapped or not.
Sizes are MiB. "Before" is aef1a811; "after" is this batch's commits. qml6's columns come from
the same run as "before"; the commits change only Ljós.

| App | RSS start, Ljós | qml6 | Private start, Ljós | qml6 | Private end, Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|---:|---:|---:|---:|
| alarms | 156.2 | 244.2 | 49.4 | 95.9 | 51.0 | 57.3 | 103.1 |
| animations | 166.2 | 230.4 | 44.3 | 98.5 | 52.2 | 47.8 | 147.5 |
| calqlatr | 158.6 | 248.0 | 45.6 | 101.0 | 45.4 | 46.1 | 101.1 |
| clocks | 155.1 | 234.4 | 42.9 | 93.0 | 43.0 | 42.9 | 95.4 |
| form | 173.5 | 268.8 | 55.4 | 117.1 | 51.8 | 55.5 | 113.2 |
| hangman | 153.4 | 229.1 | 40.5 | 88.4 | 40.5 | 38.5 | 88.4 |
| listview | 162.8 | 229.6 | 43.9 | 90.4 | 46.9 | 46.8 | 90.4 |
| maroon | 168.4 | 262.9 | 53.5 | 118.1 | 62.6 | 59.7 | 124.2 |
| photosurface | 168.5 | 256.8 | 52.4 | 104.6 | 61.2 | 65.3 | 122.0 |
| rssnews | 170.5 | 803.7 | 52.4 | 649.1 | 53.3 | 52.6 | 148.9 |
| samegame | 156.4 | 245.0 | 45.4 | 103.1 | 59.9 | 59.7 | 150.8 |
| snippet-texteditor | 122.5 | 226.2 | 34.4 | 92.1 | 39.0 | 42.9 | 92.1 |
| text | 160.7 | 249.7 | 51.8 | 109.2 | 53.5 | 49.6 | 109.2 |
| thermostat | 193.0 | 297.3 | 83.4 | 141.0 | 184.6 | 175.4 | 178.0 |
| todolist | 175.3 | 261.5 | 60.3 | 110.4 | 61.2 | 60.5 | 110.4 |
| **Sum** | **2441** | **4288** | **756** | **2112** | **906** | **901** | **1774** |

Ljós holds less than half of qml6's private memory at the start in every app, and at the end
in every app but thermostat. The before and after columns at the end differ by a few MiB
either way from run to run; the clicks do not land on the same things twice.

**Growth.** The four apps that grew most were driven three times back to back
(`--rounds 3`), with memory read after each 60 s:

| App | Start | Round 1 | Round 2 | Round 3 |
|---|---:|---:|---:|---:|
| thermostat, before | 84.6 | 186.1 | 258.9 | 252.7 |
| thermostat, after | 77.4 | 172.1 | 237.3 | 237.3 |
| thermostat, qml6 | 141.1 | 178.3 | 183.1 | 169.9 |
| samegame, before → after | 43.5 → 45.5 | 61.5 → 59.6 | 61.6 → 62.3 | 62.6 → 62.3 |
| maroon, before → after | 53.6 → 53.4 | 59.7 → 59.2 | 65.2 → 63.4 | 66.2 → 64.4 |
| maroon, qml6 | 118.1 | 122.8 | 122.7 | 122.9 |
| photosurface, before → after | 54.7 → 52.2 | 69.8 → 62.5 | 71.0 → 62.9 | 71.0 → 62.9 |

None of it is a leak. Each stops growing once the clicks have been everywhere they reach:

- thermostat's grows with the page shown. Its Schedule page is a grid of about 12 000
  objects; at the end the engine holds 222 000 graph nodes and 64 MB of reactive state, with
  1136 accessible objects. A StackView keeps its declared `initialItem` while pages are
  pushed over it, as Qt's does, so the start page is still there too. A StackView whose
  pages are files replaced again and again keeps a constant count of objects and nodes
  (`pages_replaced_by_file_leave_nothing_behind`).
- samegame and photosurface settle after the first round: the tiles and images made once.
- maroon creeps by about 1 MiB a round. Its object and node counts do not grow, so this is
  the allocator and caches, not the engine's objects; it was not chased further.

**What held the memory, and the cuts.** dhat on thermostat put most of the engine's share in
graph nodes. Its "58 MB in `Graph::with_options`" is the node table's capacity, of which only
the touched part is resident. Counted with a Repeater of 100 of each, settled, in graph nodes
per instance:

| Type | Before | After |
|---|---:|---:|
| Item, Rectangle, Text | 12.3, 13.3, 13.3 | same |
| Label | 33.4 | 28.4 |
| Slider | 125.4 | 120.4 |
| Switch | 141.4 | 136.4 |
| RadioButton | 142.4 | 137.4 |
| Button | 145.4 | 140.4 |
| Pane holding an item | 131.4 | 84.4 |

The Button row is before the last cut; a Button with no mnemonic now also has no `Shortcut`
object and the nodes of its sequence, enablement and connection.

- A container watched each of its 21 palette colours with a graph node, so that a colour set
  later reached the controls inside. The colour's write filter now tells them, and a colour
  bound on a container starts to listen the first time the binding gives it a new value
  (features/controls/item-palette-inherited.feature, two new scenarios, also on qml6).
- A graph node is 200 bytes, not 216: an override's id is kept beside the list of layers, not
  in each layer, and a node's eleven flags are bits of one `u16`.
- An accessible object followed its role and its labels with graph effects; write filters do
  the same with no node, so one costs one node (`an_accessible_object_costs_one_graph_node`).
- A button makes its mnemonic's `Shortcut` with the first text that has one
  (features/controls/mnemonics.feature, a new scenario for a mnemonic given later, also on
  qml6).

Together they took thermostat's start from 84.6 to 77.4 MiB and its plateau from 253 to
237 MiB. The other apps hold few controls, and their private memory moved by less than the
noise.

**The bench.** `ljos bench --rounds N` drives the load span N times and reads memory after
each; the report gives every process's `loaded` memory and `rounds`, and counts `SwapPss` as
part of what a process holds.

**What is left.** Thermostat after driving is still about 35 % above qml6, and it is all page
size: about 18 graph nodes an object. Next:

- Control's own effects: a bare Control is still about 60 nodes, most of them effects that
  place its parts and follow its font and palette.
- Accessible objects are made for every item the a11y pass reaches; one costs an object as
  well as its node.
- `Shortcut` objects for the actions and menus that have no sequence.
- maroon's creep of about 1 MiB a round.

DECISIONs:

- Swapped pages count as the process's own memory; RSS alone made Ljós and qml6 look smaller
  the more the machine swapped.
- The load is grid clicks for 60 s, the same for both runtimes. Round 1 of the first measure
  was run again, and the second run is the one shown.
- Thermostat's growth is page size and stays, not a leak: Qt keeps the declared initial item
  the same way.
- A container's colour watches are made by the colour's setter when one is first set, not
  when the container is made.
- The graph's u64 stamps were not narrowed: a 32-bit tick could wrap while frames are live.
- The a11y object's role and label watches became write filters.

### B24 (H2): the component plan (2026-10-25)

[ADR 0002](adr/0002-component-plans.md)'s plan, in three commits. A loaded file links, on
first use and beside itself, what its objects and expressions resolve to against the native
types; every later instance allocates its objects, writes literals and installs bindings by
index. Where each object and binding was written is derived from the instance when it is
asked for, not stored per object. Expression arguments are resolved slots. Ids resolve by
slot through the instance, so a new instance interns and writes down none. The IR is at
version 14: the compiler writes down per object whether it runs completion handlers, is a
`ListModel` or a `ListElement`, and per component its ids and the objects they name.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets). The
budget QML now gives each component an id and uses it in a binding (`row.index`,
`item.width`), so the zero name lookups also cover ids:

| | ListView delegate, B23 | B24 | Item by `createObject`, B23 | B24 |
|---|---:|---:|---:|---:|
| Member lookups by name | 191 (19.1) | 40 (4) | 61 (6.1) | 10 (1) |
| Type lookups by name | 20 (2) | 0 | 10 (1) | 0 |
| Context name lookups | 0 | 0 | 0 | 0 |
| Atoms interned | 0 | 0 | 0 | 0 |
| Heap allocations | 1866 (186.6) | 1796 (179.6) | 1057 (105.7) | 1027 (102.7) |

Objects, graph values, bindings, effects and scopes are unchanged (ADR 0001's work).

| listview under callgrind (main thread, 20 s after 40 s) | Run 1 | Run 2 | B23 |
|---|---:|---:|---:|
| Per delegate (total ÷ text layouts) | 390.4 k (3.844 G / 9848) | 404.8 k (3.995 G / 9871) | 401.1 k, 409.6 k |
| Delegates per frame | 12.2 | 10.1 | 11.3, 10.3 |
| Adoption (`QuickScene::adopt`) | 73.1 k | 72.8 k | 74.9 k |
| Text layout (`TextEngine::build_layout`) | 53.1 k | 52.9 k | 53.0 k |
| Teardown: `Engine::destroy_now` | 29.2 k | 29.0 k | 30.2 k |
| — `ViewShared::destroy` | 18.3 k | 18.3 k | 18.5 k |
| — `QuickScene::unadopt` | 15.8 k | 15.6 k | 16.0 k |
| `Engine::apply_members` | 10.9 k | 11.0 k | |
| `Engine::apply_unit` | 17.5 k | 17.5 k | |

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós | qml6 (same run) | Ljós, B23 |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.46 | 15.34 | 19.40 |
| listview | 210.25 | 102.81 | 211.93 |
| listview with `reuseItems: true` | 88.35 | 60.54 | 88.64 |

The plan takes the lookups away, but by instructions it is a small part of a delegate: a few
thousand per delegate, within the run-to-run spread of the totals, and scrolling is where
B23 left it. A delegate still costs about twice qml6's ~200 k; the rest is adoption, text
layout and teardown, which ADR 0001 and B25–B30 take on.

DECISIONs:

- The 4 member lookups left per delegate are the view's row, found by name for each
  delegate: B25's row plan. The 1 left per `createObject` item is the property it is given,
  named by the caller at run time, which no plan can know.
- The IR flags are what the runtime asks per object as it makes one: completion handlers,
  `ListModel`, `ListElement`. Whether an object has bindings or handlers is already O(1)
  from its members, and a "type is a file" flag would be wrong because a native type can be
  registered from QML. Parameter shapes and id references already were in the IR
  (`Arg::Object`, `Resolved::Id`), so only the per-component id list is new.
- A context made for a new instance points at the instance and finds ids through
  `ComponentDef::ids`. One that is given ids one by one (a reload in place, an id taken
  away) writes the instance's ids into its map first, and the map wins over the slots.
- Bindings installed through the plan are recorded as bits in the object's `Made`, with
  their place derived on query; an explicit place (a reload, a binding set from script)
  still wins. A reload in place writes the derived places down before it moves anything.

### B25 (H2): the row plan, and where a delegate's time goes (2026-10-26)

A view keeps one row plan per delegate component and set of model roles, made the first time
a delegate needs it and dropped when the view's source changes or the model resets or
changes its roles. The plan lists the delegate's required properties, each with the row
value it is fed from (`index`, `modelData` or a role, by position). When every required
property has one, no role is read unqualified and the model gives no row object of its own,
the view writes those properties straight onto the delegate when it makes or reuses it and
whenever its row changes. There is no row object, no `model` and no binding to them. Other
delegates still get a row object, built from the plan's role list, and their required
properties are bound to it by index instead of by name.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets):

| ListView delegate | B24 | B25 |
|---|---:|---:|
| Objects | 40 (4) | 30 (3) |
| Graph values | 80 (8) | 90 (9) |
| Bindings | 70 (7) | 60 (6) |
| Effects | 160 (16) | 160 (16) |
| Member lookups by name | 40 (4) | 0 |
| Heap allocations | 1796 (179.6) | 1667 (166.7) |

The `createObject` item is unchanged (1027, 1 member lookup). An object and a binding fewer
are the row object and `index`'s binding to it. The value more is the node `index` needs on
the delegate once something other than a binding writes it. The row object had no effects,
so the effect count stays the same.

| listview under callgrind (main thread, 20 s after 40 s) | Run 1 | Run 2 | B24 |
|---|---:|---:|---:|
| Per delegate (total ÷ text layouts) | 393.3 k (3.877 G / 9858) | 396.2 k (3.890 G / 9820) | 390.4 k, 404.8 k |
| Delegates per frame | 10.0 | 9.75 | 12.2, 10.1 |
| Row setup (`ViewShared::setup`) | 11.8 k | | |

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós | qml6 (same run) | Ljós, B24 |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.47 | 15.32 | 19.46 |
| listview | 203.81 | 102.79 | 210.25 |
| listview with `reuseItems: true` | 87.22 | 60.65 | 88.35 |

As with B24's plan, the lookups and allocations go, but per delegate the instructions stay
within the spread between runs. Scrolling listview is 3 % lower, at a load of 9 to 11.

**Where a delegate's ~395 k goes.** These are the self costs of run 1, each function put in
the innermost group that holds it on its stack. A function under several groups is split by
how often each one calls it, and allocator time is folded into whoever allocated. The heap
is 57 k (14.6 %) of the total. Run 2 is within 1 k on every line.

| Group | Per delegate | Parts |
|---|---:|---|
| Text | 88 k | `ljos_text` layout and shaping 57.4 k; `ljos_quick::text` completion, measuring and following inputs 30.5 k |
| Creation | 87 k | parent and tree (`set_parent_item`, `sync_parent`) 25.4 k; evaluating the delegate's own bindings 20.3 k; `init_item` and item registration 12.7 k; `apply_members` and `apply_unit` 9.2 k; the rest of the creation job 8.8 k; `allocate_step` 4.6 k; completion (not Text's) 3.3 k; row setup 3.0 k |
| Scene effects | 56 k | anchors 17.2 k, geometry 10.3 k, place 2.6 k (B28: 30.1 k); structure and reconcile 8.2 k (B30); content and list 8.4 k; inherit 6.7 k (B29); the rest of adopt 3.0 k |
| Teardown | 40 k | graph disposal 17.8 k (B26); the rest of `destroy_now` 14.3 k; `ViewShared::destroy` 4.5 k; `unadopt` 3.8 k |
| The frame, divided by delegates | 121 k | paint, egui and the event loop 49.3 k; the view's pass (refresh, scrolled, release) 37.5 k; a11y 11.3 k; the rest of the scene's frame 9.1 k; Fill layout and its requests 7.8 k; animation ticks 6.5 k |

The first four groups total ~270 k, the delegate's own work; qml6's ~200 k is its main
thread only, because it renders on a thread of its own. Mapped to the plan:

- B27 (the item record) takes on parent and tree, `init_item` and registration: ~38 k.
- B28 (geometry, place, anchors) is ~30 k, plus its share of disposal.
- B26 (scopes) aims at graph disposal, ~18 k, and part of the rest of teardown.
- B30 (structure and text polish) is ~8 k, and up to ~18 k more if text's input following
  moves into it.
- B29 (inherited state) is the smallest, at 6.7 k.
- Not on the plan: text layout and shaping (57 k), the view's pass (37.5 k) and paint
  (49 k). Each of these is larger than any of B26–B30.

DECISIONs:

- The view writes a fed required property as the host. If the delegate has assigned it or
  bound it itself, the view leaves it alone, which is Qt's rule: an assigned required
  property stops following its row (`list-delegate-context.feature`, two new scenarios
  checked on qml6).
- A row object is still made for object rows and JavaScript-object rows, for a delegate
  that requires `model`, and when roles are read unqualified. Only those delegates can tell
  the difference.
- When a row is removed, a fed delegate keeps its role values and its index becomes -1,
  as a row object's would. A change of roles alone does nothing to it, because the plan is
  made again and the next refresh writes from it.
- The breakdown counts self cost, split proportionally among the groups on the stack.
  Callgrind's inclusive cost counts recursive and shared functions more than once, so it
  cannot be added up this way.

### A theme or font switch against qml6, continued (2026-10-26)

These runs follow "A theme or font switch against qml6" above. All builds are release.

#### Palette resolve

A palette now asks the engine once per object which of its open colour slots an author gave
(`Engine::authored`, `Graph::provenance_each`). Before, it asked colour by colour. An ancestor
that gave none of the slots is skipped.

| | before | after | qml6 |
|---|---|---|---|
| synthetic theme switch, `frame_profile` | 8.77M instructions | 7.67M | |
| theme tst | 0.40 ms | 0.383–0.400 ms | 0.517–0.522 ms |
| mediaplayer switch, provenance lookups | 329K | 187K | |

#### A font change on fields and buttons

A field lays out its text and writes `contentHeight`. It also reads its own height, which
follows `implicitHeight`, which reads `contentHeight`. Its layout effect therefore ran again
after every change, even when the height came out the same.

Now an effect that hears of a maybe-change while it runs is checked when the run ends. It runs
again only if a source really changed.

`a_field_whose_height_stays_with_a_new_font_lays_its_text_out_once` pins 5 effect runs per
field, down from 7.

| | before | after | qml6 |
|---|---|---|---|
| field, size change (callgrind, per field) | 118.8K | 100.9K | |
| 20 fields, bold / size | 0.092 / 0.165 ms | 0.092–0.098 / 0.152–0.160 ms | 0.058 / 0.065–0.068 ms |
| 40 buttons, bold / size | 0.152 / 0.165 ms | 0.140 / 0.155 ms | 0.142–0.150 / 0.160 ms |
| 80 labels, bold / size | 0.125 / 0.133 ms | 0.125 / 0.135 ms | 0.125 / 0.130 ms |

**Buttons are now even with qml6, but fields still take about twice as long.** Most of a field's
remaining cost is in the graph:
- 42 tracked reads.
- 4 effects.
- 5 computed writes.
- The editor's layout batch: 35K.
- The placeholder's eager measure: 12K. TEXT4 measures it when it is read instead.

Taking the text polish out of the graph would close this gap. That work belongs to the delegate
track's dirty bits.

#### mediaplayer's radio: sandbox against off-screen

The sandbox timing is taken inside the script (`performance.now` around the write). The
`frame_profile` timing also covers parsing the eval. Both runs do the same graph work: 66–71
evaluations and 110–115 effect runs a switch.

| | switch |
|---|---|
| sandbox, one switch per eval | 0.97–1.25 ms |
| sandbox, 50 in one eval | 0.26–0.30 ms each |
| sandbox, three in one eval | 1.0, 0.45, 0.33 ms |
| sandbox, 20 ms of busy JS first | 1.0–1.1 ms |
| off-screen, `--start`s back to back | 0.21–0.22 ms |
| off-screen, `--apart` (a frame between) | 0.39–0.53 ms |
| off-screen, 50 in one eval | 0.20–0.25 ms each |

The gap is cold caches, not more work:
- A switch after a frame runs from a cold cache. Under callgrind it executes only 1.6 % more
  instructions than one run back to back.
- A 48 MB sweep between two switches in one eval, with no frame between them, slows the next
  switch just as a frame does. It goes from 0.29–0.46 to 0.79–0.85 ms off-screen, and from
  0.45–0.72 to 1.09–1.23 ms in the sandbox.
- The sandbox draws through llvmpipe, which rasterises every frame on the CPU's own cores and
  caches. That frame evicts much more than the off-screen pass, which only tessellates.

The fix on Ljós's side is a smaller switch. It now executes 2.43M instructions.

The off-screen numbers before this section were also inflated. Every devtools eval compiled
its result runner again, at 857K instructions. An empty eval took 257 µs and now takes 16–47
µs. `frame_profile --apart` now times each `--start` after a frame of its own, as a tool
reaching a shown window does.

### Text per delegate (2026-10-26)

The ListView delegate of `bench/listview.qml` lays out one `Text` ("Row {i}"). At the start
it cost 57.5 k instructions in `ljos_text` and 30.9 k in `ljos_quick::text`, out of 392.9 k
per delegate. qml6 does a whole delegate in ~200 k. This pass cuts the `ljos_text` part
and keeps layouts byte-for-byte the same.

How it was measured:

- **Per commit:** a scratch microbench lays out `"Row {i}"` twice per row, once to measure
  and once to paint, as a delegate does. It uses the system fonts and the default style and
  params, and calls `end_frame` every 8 rows. After 2000 rows of warm-up, callgrind counts
  only rows 0..10000 (`--toggle-collect`). Repeat runs agree within about ±10 per row.
- **Behaviour:** a scratch dump, Debug-printed and deterministic, compared with `cmp`
  against the baseline after every commit. It covers about 135 k layouts (35 texts × 25
  styles × 77 params, elided text, advance widths, ranged layouts and font metrics) on two
  engines.
- **In the app:** the release build with line tables, under callgrind in the GPU sandbox
  (20 s after 40 s of warm-up, main thread). Per delegate is the total ÷ `build_layout`
  calls, and phases are attributed with B25's markers.

| Commit | What | Microbench, per row | Change |
|---|---|---:|---:|
| d781a603 | baseline | 55,523 | |
| 0111321a | a run's glyphs read from Parley once; known advances in pages, not a hash map | 51,826 | −3,697 |
| 2b4ff999 | a line's items read once, for both widths and painting | 50,551 | −1,275 |
| d256a163 | a font's tables (for vertical metrics) read once per face | 49,824 | −727 |
| f8921535 | which characters a pinned font covers, kept as a bit set | 48,165 | −1,659 |
| 9f6b4eb5 | frame caches trim from a log of stamps, not a sort of every entry | 47,754 | −411 |
| 6de8b7d7 | a run's glyph vectors reserved once, not grown cluster by cluster | 47,258 | −496 |

That is −8,265 per row (−14.9 %). The messages of 0111321a and 2b4ff999 give numbers from an
earlier, noisier method (the difference of two whole runs, ±500); the table has the
toggle-collect numbers.

| listview under callgrind (main thread, per delegate) | Before (d781a603) | After (6de8b7d7) |
|---|---:|---:|
| Delegates (`build_layout` calls) | 9779 | 9861 |
| Frames | 793 | 1051 |
| text: layout and shaping (`ljos_text`) | 57.5 k | 48.8 k |
| text: completion, measure, follow (`ljos_quick::text`) | 30.9 k | 31.0 k |
| paint, egui and the event loop | 43.5 k | 53.1 k |
| view: pass refresh/scrolled/release | 33.0 k | 39.1 k |
| Total | 392.9 k | 405.4 k |

The text in `ljos_text` fell by 8.7 k per delegate (−15 %). The total rose because the second
capture drew a third more frames for about the same number of delegates. Painting and the
view's pass are paid per frame, so they rose with them, and none of that is text.

| Scroll, M instructions/s (`scr.sh listview`, back to back, twice) | Run 1 | Run 2 |
|---|---:|---:|
| Ljós before | 211.11 | 211.45 |
| Ljós after | 207.30 | 207.15 |
| qml6 | 102.76 | 103.07 |

Ten ListView delegates now make 1786 allocations, down from 1796 (`budget.rs`, 4ab9b69a).

What is left in `ljos_text`, per row of the microbench:

- **Parley:** about 28 k to build the layout, 2.2 k to push the style and 3.3 k to break
  lines. Shaping is already cached by text and style, so a row pays for layout building,
  not shaping. (The next section removes these for one-line plain text.)
- **Our own reading of the layout:** about 5 k (glyph iteration and advance snapping) and
  1.7 k to extract it.
- **Not ours:** `TextInputs::read` in `ljos_quick::text` (16 k per delegate) is mostly
  `Graph::try_get`, which belongs to the graph track (B26–B30).

DECISIONs:

- Parley's shaping, line building and font queries were left as they were in this pass.
  "One-line plain text without Parley's builder" below does that work for short single-line
  text, and a byte-for-byte dump shows the results are the same.
- Each commit is measured with the microbench, not with a thin-LTO app build per commit.
  The app is measured once on the whole stack.
- A run reserves one glyph per byte of its text, which is exact for ASCII. For text of
  wider characters it over-reserves for a run kept in the shape cache.

### Paint while scrolling (2026-10-26)

How much of a scrolling frame goes to painting, to egui and to the event loop, and what
came off it. The view's own pass and the scene graph are B26–B30's, so this covers
`ljos_render` (compose, B0 encode), `ljos_egui` and `ljos_shell` (presenter, GL painter,
frame loop).

**Method.** Callgrind on the release-with-symbols build in the GPU sandbox
(`desktop:cua`, `renderD128`, `LJOS_SCREEN_READER=off`): 8 s warm-up, counters zeroed,
20 s counted. Each function's inclusive cost is credited to the first phase on its stack,
divided by `eglSwapBuffers` calls, so "per frame" means per frame shown. `lvstill.qml` is
listview's delegate scrolled inside a cache buffer that holds every row; listview is
`bench/listview.qml`. Real time is `scr.sh`: all threads' instructions/s from perf
counters over 220 wheel notches, run back to back with qml6.

**Where a frame goes, before** (per frame shown, thousands of instructions):

| Phase | lvstill | listview |
|---|---:|---:|
| `eglSwapBuffers` and the driver below it | 133.8 | 120.1 |
| Scene frame (`ljos_quick`, not this work's) | 39.6 | 1460.9 |
| Engine (not this work's) | 25.7 | 1860.7 |
| GL painter (`gl.rs`, `gl_mesh.rs`) | 41.0 | 68.3 |
| B0 encode | 38.3 | 96.3 |
| egui pass (`Context::run`, end of pass) | 30.8 | 30.8 |
| Compose, display list, render tree | 28.6 | 135.8 |
| Presenter (change, clip, tessellate glue) | 28.1 | 20.3 |
| winit, Wayland, calloop | 21.3 | 19.0 |
| Event loop, the rest | 18.9 | 15.5 |
| Total | 413.3 | 3833.8 |

On lvstill, paint, egui and the loop are about half the frame. On listview they are under a
tenth: the delegates made and torn down each frame are the rest.

**What came off it**, one commit each (callgrind, per frame):

| Commit | What | lvstill | listview |
|---|---|---:|---:|
| b0: a run's glyph mesh is built as its glyphs go into the atlas | B0 encode | 38.3 → 38.0 k | 96.3 → 66.6 k |
| b0: a moved run is told near its clip from the span of its glyphs | `places_all` | 6.7 → 1.5 k | 3.3 → 0.6 k |
| | B0 encode | 38.0 → 30.7 k | 66.6 → 64.8 k |
| ljos_shell: a mesh's bounds are found in plain comparisons | Bounds (`visual_bounding_rect` and the hoist closure) | 10.4 → 4.8 k | 11.5 → 4.7 k |

The first commit stops a new run from walking its glyphs three times: once into the atlas,
once for quads, and once to learn whether a glyph was left out. The second keeps a moved
mesh's glyph span, so "is it near the clip" takes four comparisons instead of placing every
glyph again. The third finds a mesh's rect without `f32::min`/`max`, which handle a NaN at
every vertex. A unit test pins it to epaint's answer. The rects and meshes are the same, so
no image test or reference changed.

**Real time, back to back with qml6** (M instructions/s, all threads):

| | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| `lvstill.qml` | 20.09, 21.82 | 19.08 | 15.22 |
| listview | 211.11, 211.31 | 208.83 | 102.94 |
| listview with `reuseItems: true` | 88.72, 88.61 | 86.60 | 60.57 |

"Before" was run on either side of "after". Perf counters count `rep movs` and memcpy as
fewer instructions than callgrind does, so the real-time gain is smaller than the callgrind
gain.

**Left, and why** (DECISIONs):

- **The swap**, ~130 k a frame, is mostly one memcpy in the driver below `eglSwapBuffers`:
  the sandbox's software path. It is not Ljós's to cut.
- **Three event-loop turns per frame.** Only one turn carries `RedrawRequested`. The other
  two wake on Wayland traffic: `dispatch_pending` runs six times a frame, which fits the
  compositor's frame callback and buffer release. A turn with nothing due costs ~1.5 k in
  `settle`, and each frame makes one redraw request, from the scroll. No repaint request
  was redundant. No loop or wake code changed, so idle is as before.
- **The egui pass**, ~31 k a frame, is the same whatever is drawn. It is the begin and end
  of egui's pass, and Ljós lays out nothing in it. egui stays the host, so the pass stays.
- **The accesskit compare** (~5.5 k) runs only while an AT client is connected; the
  sandbox's driver is one.
- **Compose** (`compose_node`) and **B0 encode** are now flat: no source line costs more
  than ~0.8 k. What is left is per node and per moved vertex. Reusing a composed subtree
  across a pure translation would cut it, but it changes what compose hands B0. It is left
  for after B26–B30, which change the same nodes.
- **Glyph lookups on listview** (~25 k: key, hash, probe) come from the rows made each
  frame. A run is looked up once when it first draws; a carried run is not.
- **Hoist's walk** over the earlier fills, to see what a fill may be moved past, costs
  ~3 k. An index would add more code than it saves.
- **A fused copy-and-move for a moved mesh** measured within noise.
- These numbers predate "Text per delegate" above. That work changes how text is shaped,
  not how B0 draws it.

### Memory against qml6 (2026-10-26)

Five apps, off-screen in the cua sandbox, Ljós release builds against `qml6` 6.11.2 with the
Basic style, both as Wayland clients of the sandbox's sway:

- `bench/minimal.qml`: a Window with one Text.
- `bench/list1000.qml`: a ListView of 1000 ItemDelegates with a ScrollBar.
- Three corpus demos of different kinds: thermostat (controls-heavy), todolist (a list
  model and LocalStorage), and photosurface (images).

The sandbox has no GPU device, so both runtimes draw through Mesa's llvmpipe. MEM1 above
covers fifteen apps with the private column alone. This section adds a view by mapping, the
heap, and what Ljós's share is spent on.

**Settled**, 8 s after start, in MiB. Each runtime ran alone. Sizes come from
`/proc/PID/smaps`. The malloc figures come from an `LD_PRELOAD` counter of the bytes asked
for, live and at the peak, so they include pages a driver reserved and never touched.

| App | RSS, Ljós | qml6 | PSS, Ljós | qml6 | anonymous PSS, Ljós | qml6 | malloc live, Ljós | qml6 | malloc peak, Ljós | qml6 | threads, Ljós | qml6 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| minimal | 153.0 | 219.7 | 112.4 | 163.0 | 30.4 | 72.4 | 45.4 | 128.9 | 46.7 | 130.3 | 15 | 71 |
| list1000 | 160.6 | 228.7 | 125.7 | 172.0 | 34.8 | 76.2 | 49.7 | 132.1 | 50.9 | 133.3 | 15 | 71 |
| thermostat | 209.2 | 312.5 | 174.4 | 245.2 | 72.0 | 131.7 | 91.3 | 184.6 | 110.4 | 196.0 | 15 | 71 |
| todolist | 172.4 | 259.6 | 157.5 | 190.2 | 49.9 | 94.4 | 67.8 | 144.3 | 68.1 | 145.9 | 16 | 71 |
| photosurface | 188.3 | 257.1 | 153.3 | 200.7 | 49.6 | 86.6 | 61.1 | 142.5 | 64.2 | 143.0 | 16 | 80 |

Ljós holds 42–60 % of qml6's anonymous memory and 35–56 % of its malloc'd bytes. PSS moves
by up to 15 MiB from run to run with whatever else maps the same libraries, so it is the
weakest column.

**What is mapped**, settled minimal app, RSS (PSS) in MiB:

| Mapping | Ljós | qml6 |
|---|---:|---:|
| LLVM (llvmpipe's JIT) | 46.0 (25.9) | 46.3 (28.3) |
| the program / Qt's libraries | 44.7 (44.7) | 26.2 (26.2) |
| other libraries | 8.1 (1.6) | 53.6 (22.6) |
| GPU driver (Mesa, libgallium) | 13.7 (7.0) | 15.1 (8.5) |
| fonts and fontconfig caches | 9.8 (2.7) | 5.2 (4.8) |
| `[heap]` | 20.7 | 18.0 |
| other anonymous | 9.6 | 53.7 |

File-backed mappings are 82 of Ljós's 112 MiB of PSS and 91 of qml6's 163. The private
dirty part of Ljós's file mappings is 8.3 MiB. 2.9 MiB of that is the program's own relocated
`.data.rel.ro`; the rest is the libraries' relocations. Ljós maps the fontconfig cache twice,
once for fontique and once for `ljos_text`'s substitution rules, which load beside it so the
first unknown family does not wait. The pages are shared, so the second mapping costs RSS
but no PSS.

**Where Ljós's heap goes.** On the minimal app, massif's peak is 46 MB:

- 18.7 MB is made by `glutin` creating the GL context, inside Mesa.
- 15.9 MB is allocated by llvmpipe's own threads.
- 1.7 MB is in `GlPainter::paint`.
- 1.25 MB is the GL copy of the glyph atlas.
- 1.0 MB is in `set_window`.

Ljós's own Rust heap is about 4.8 MB. An llvmpipe context costs about 31.6 MB of malloc
whatever API is asked for: GL compat, GL core, GLES 2 and GLES 3 were each made alone, and
`LP_NUM_THREADS` moves it by 0.3 MB. qml6 pays the same.

Driven thermostat after its Schedule page opened, from the engine's memory report (`ljos
observe`, `memory`), in MB:

| Owner | MB |
|---|---:|
| reactive graph (112 863 nodes) | 30.3 |
| objects (6715) | 14.3 |
| scene | 2.9 |
| script (JS heap) | 1.8 |
| glyph atlas, then 512² | 1.3 |
| textures | 1.0 |
| meshes | 0.4 |
| compiled IR | 0.4 |
| images | 0.2 |
| text caches | 0.06 |
| free in the allocator | 3.9 |
| not attributed: driver and the rest | 75.0 |

dhat on the same run counts 75.2 MiB of Rust heap. By crate:

- `ljos_reactive` 35.1
- `ljos_engine` 22.8
- `ljos_quick` 4.25
- `ljos_render` 3.0

The largest site is the graph's node table, at 27 MB. The graph and per-object costs belong
to the delegate track (B23–B27), so this work left them alone. Text caches, the JS heap,
egui's font atlas (0.25 MiB at 2048 wide) and meshes are each too small to be worth a cut.

**After driving.** These come from `ljos bench --gesture click --load 20`, before the cut
below. Private is `Pss_Anon + SwapPss` in MiB; first frame is in ms.

| App | first frame, Ljós | qml6 | private settled, Ljós | qml6 | private after driving, Ljós | qml6 | start peak (cgroup), Ljós | qml6 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| minimal | 62 | 447 | 38.4 | 85.2 | 38.4 | 85.2 | 49.6 | 207 |
| list1000 | 54.4 | 332 | 42.7 | 90.1 | 44.0 | 90.2 | 53.5 | 124 |
| thermostat | 131 | 420 | 59.6 | 141 | 227 | 178 | 140 | 191 |
| todolist | 82 | 527 | 60.9 | 110 | 61.0 | 110 | 71.4 | 141 |
| photosurface | 235 | 336 | 54.8 | 113 | 63.5 | 131 | 120 | 143 |

Scrolling, after the cut, with `--gesture scroll --load 20`, median of 3, MiB:

| App | private settled, Ljós | qml6 | private after scrolling, with swap, Ljós | qml6 |
|---|---:|---:|---:|---:|
| list1000 | 40.2 | 90.1 | 43.0 | 90.6 |
| minimal | 37.9 | 85.2 | 37.9 | 85.2 |

Scrolling 1000 rows adds 1–3 MiB on Ljós and none on qml6. The cgroup peaks of this run
(up to 255 MiB for Ljós's list1000) are page cache, not the app's own memory: the cgroup's
`file` was up to 200 MiB while its `anon` held at 40–42. The newly built 133 MB binary was
being read on a machine whose load average was 40–54, and which swapped.

Thermostat is the one loss, as in MEM1. Driven three rounds, Ljós's anonymous memory goes
242, 200 (+38 swapped), 200 while qml6's holds at 187, so it is a one-time step and not a
leak. A click-by-click drive puts the step at the page "Adjust your thermostat settings":

- objects 4.9 → 13.6 MB
- graph 13.7 → 28.8 MB
- Rust heap 36 → 75 MB

That is page size, on the delegate track.

**The cut.** The glyph atlas started at 512². That held 512² of coverage and the same again
as RGBA texels, 1 310 720 bytes, plus the driver's copy of the texture. The apps fill few
glyphs:

| App | glyphs |
|---|---:|
| minimal | 5 |
| list1000 | 24–34 |
| todolist | 35–223 |
| thermostat | 157–319 |
| photosurface | 10–188 |

It now starts at 256², 327 680 bytes, and doubles when a glyph does not fit. Both builds
were made from the same tree and differ only in the start side. Malloc live, settled, is
three runs each for minimal and one each for todolist:

| | 512² | 256² |
|---|---:|---:|
| minimal | 48.542 MB | 47.555 MB |
| todolist | 72.094 MB | 71.099 MB |

That is −0.99 MB, steady to 0.4 KB across runs. `b0_starts_with_a_small_atlas` pins the
bytes exactly. `b0_grows_the_atlas_and_keeps_the_ink` draws 62 glyphs at 48 px, grows the
atlas to 512² and compares the ink with the reference rasteriser.

**Left**, none of it this track's to cut:

- Software rendering pays llvmpipe's context: about 30 MB of malloc and 46 MiB of LLVM
  mappings. A CPU path that draws into shared memory instead of through EGL would not.
- Graph nodes and objects per page, on thermostat (delegate track).
- Photosurface takes 661 ms of CPU to its first frame against qml6's 271, and answers input
  at 515 ms against 354.
- Once, todolist's `Component.onCompleted` took 8.3 s (a JS soft-budget warning) on the
  first run of a binary with a fresh data directory. It did not happen again in three more
  runs, one of them also with a fresh directory.

DECISIONs:

- Thermostat stands in for a controls gallery, because stocqt does not run.
- `bench/list1000.qml` is the 1000-row list, since no corpus example has one.
- Only the desktop profile was run.
- Swapped pages count as the process's own memory.
- llvmpipe's context is the driver's cost and was left alone.
- The fontconfig cache's second mapping costs no PSS and keeps the first unknown family
  quick, so it stays.
- The cut is pinned in a Rust test, because the headless feature runner has no renderer
  whose atlas a scenario could read.

### One-line plain text without Parley's builder (2026-10-26)

This follows "Text per delegate". The microbench, the app capture and `scr.sh` are measured
the same way. The dump now covers 35 texts × 97 styles × 134 params (about 1.85 M lines),
plus the elided, advance, ranged and metrics lines, on the bundled and the system fonts.
The styles include serif, monospace, named, missing and fallback families, and one with
underline, strikeout and spacing.

**Where a row went, before.** Of the 47.3 k per row, Parley's
`StyleRunBuilder::build` took 28.0 k:

- `shape_text`: 20.7 k, of which harfrust is 5.5 k, `push_run` 3.7 k and font queries 4.2 k.
- `analyze_text`: 5.1 k.
- Style metrics resolved through a font query: 1.6 k.

Pushing the style took 2.2 k and breaking lines 3.3 k. Our own reading of the lines took
another ~7 k.

**What was reusable through the public API.** The `LayoutContext` and `FontContext` were
already kept per engine. A builder borrows the context and resolves its styles every time
it is made; Parley has no public way to keep a resolved style set, a font query or a built
layout from one builder to the next. So reusing builders, style sets or queries saves
nothing that can be reached.

**What was done** (one commit, `ljos_text: a one-line plain text is shaped without Parley's
builder`). A text qualifies when all of these hold: one style, no spans or objects,
printable ASCII only, no trailing space, no letter or word spacing, and a pinned family
whose first font covers it. Such a text is analysed and shaped with `parley_engine`'s
`Analyzer` and `Shaper`, kept warm on the engine, with the pinned font as the only font.
The shaped clusters become one run on one line. Parley's layout is built later only when a
layout breaks lines, justifies or places carets. A ranged or justified layout skips the
direct attempt, so it never shapes twice. A wrapped text that turns out too wide does
shape twice.

The dump first differed only in blob ids. Parley's builder prunes the font source cache
each time one is made (`prune(128, false)`), so a font file that no recent builder loaded is
read again as a new blob. Which id a run carries therefore depends on how many builders
came before it. The direct path prunes and loads through the source cache at the same
point a builder would, and only once it has succeeded, so every run carries the blob
Parley's run would. With that, the dump is the same byte for byte.

| Microbench, per row | Before | After |
|---|---:|---:|
| Total | 47,257 | 29,178 (−38 %) |
| `shape_text` (one font, no query) | 20.7 k | 13.4 k |
| `analyze_text` | 5.1 k | 5.1 k |
| Builder, style push, line breaking, Parley's layout | ~13 k | 0 |

| listview under callgrind (main thread, per delegate) | Before | After |
|---|---:|---:|
| Delegates (`build_layout` calls) | 9861 | 9851 |
| Frames | 1051 | 1047 |
| text: layout and shaping (`ljos_text`) | 48.8 k | 30.0 k |
| text: completion, measure, follow (`ljos_quick::text`) | 31.0 k | 31.1 k |
| paint, egui and the event loop | 53.1 k | 48.1 k |
| Total | 405.4 k | 373.6 k |

The "after" capture is on a main that also has "Paint while scrolling", so only the
`ljos_text` row belongs to this work.

| Scroll, M instructions/s (`scr.sh listview`) | |
|---|---:|
| Ljós | 190.59, 190.64, 190.52 |
| qml6 | 102.82 |

Before this work, each of the two earlier passes was measured without the other: 207.2 with
"Text per delegate" and 208.8 with "Paint while scrolling". This run has both.
Ten ListView delegates now make 1507 allocations, down from 1657. Ten plain items make
1013, down from 1028.

**What is left in a row.** About 13.4 k is shaping: harfrust's shaping with the plan, GPOS
kerning, and `push_run` with its font metrics from skrifa. Another 5.1 k is ICU's
segmentation in `analyze_text`, and 1.4 k is our advance snapping. All of these are inside
`parley_engine` and harfrust.

DECISIONs:

- Only printable ASCII qualifies. That rules out bidi, more than one script, emoji and
  clusters that need fallback, so a single run from the pinned font is exactly what
  Parley's selector would pick. Wider text keeps Parley's path.
- `parley_engine` is used directly, at the version `parley` itself uses. Its `Analyzer`,
  `Shaper` and `FontSelector` are public. Nothing reaches into Parley's private state.
- A wrapped text that turns out wider than its width is shaped twice: directly, then by
  Parley. Its width is only known once it is shaped. Delegates' texts fit, so the common
  case shapes once.

### B26 (H2): per-instance graph scopes, teardown and one box per binding (2026-10-26)

B26 targeted the ~18 k of graph disposal and the binding setup measured in B25. It made
four changes:

- A disposed node moves out only its kind and its value: those are the parts that can run
  user code when dropped. Before, the whole 208-byte node was kept, and moved three times,
  until the batch dropped.
- A binding is one allocation. The creator's closure goes by value into the graph's box,
  next to a wrapper state that is not generic (`Wrapped`). Before, the closure was boxed,
  and that box was boxed again by the wrapper.
- An object's scope is made the size its declaration's last scope reached once complete.
- The teardown walks keep a delegate's handful of objects in inline arrays: `destroy_now`,
  `tell_destroyed`, `forget_in_lists` and `tell_destruction_of`.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets). The
"before" figures are main's, after the text commits:

| | Before | B26 |
|---|---:|---:|
| ListView delegate, heap allocations | 1657 (165.7) | 1587 (158.7) |
| `createObject` item, heap allocations | 1028 (102.8) | 998 (99.8) |
| Ten `createObject` items destroyed (new pin), heap allocations | 137 (13.7) | 57 (5.7) |

The census (objects, values, bindings, effects, scopes) and the lookups are unchanged.

| listview under callgrind (main thread, 20 s after 40 s), per delegate | Before B26 | B26 |
|---|---:|---:|
| Total ÷ text layouts | 406.0 k (pre-rebase base) | 388.0 k, 380.6 k (on accaace0) |
| `destroy_now`, inclusive | 25.9 k | 23.8 k |
| `dispose_together`, inclusive | 16.6 k | 15.6 k |
| `install_binding`, inclusive | 8.1 k | 7.6 k |

B26's own share is about 3.6 k per delegate. These were the same-base runs before the
rebase: 406.0 k before and 404.6 k after the first round, without the inline walks. Most of
the drop in the total comes from main's text work, which landed in between. qml6's figure
is still ~200 k on its main thread.

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós | qml6 (same run) | Ljós, B25 |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.07 | 15.24 | 19.47 |
| listview | 197.96 | 103.00 | 203.81 |
| listview with `reuseItems: true` | 80.27 | 60.50 | 87.22 |

DECISIONs:

- No single scope shared by all of an instance's objects. Scope bookkeeping is under 1 k
  per delegate: `dispose_scope_inner`'s own overhead is ~0.7 k and the edge unlinking
  ~0.3 k. Destroying, reparenting or freezing one object on its own needs per-object
  membership, and disposal was already batched per scope. The cost was in freeing node
  closures and moving nodes, which is what B26 cuts.
- A scope's node list is sized from the plan, using the most nodes an object of that
  declaration held once complete. Nodes the view adds afterwards (parent, position, scene
  effects) still grow a list once on the first rows. A scrolling view takes recycled lists
  from the graph's pool, so steady scrolling allocates none.
- No teardown order a QML program can observe changed: within a batch, nodes still drop
  newest first, kind before value. So no scenario was added. The teardown, destruction,
  loader-lifetime, incubation, reuse and binding-creation feature files pass unchanged.
- The budget cases now run one at a time behind a lock. Allocations were already counted
  per thread. Under load, though, one case filling a process-wide cache while another was
  counting now and then gave the second case a binding or a value more. Run alone, the
  case passed 40 of 40 under the same load.

### FRAME2: a frame's fixed cost, and a reused delegate against qml6 (2026-10-26)

What a frame costs besides making delegates: the event loop's turns, the egui pass, the
accesskit compare, and lvreuse split against qml6. The method is the one in "Paint while
scrolling" above.

**The turns of a frame.** `app_frames` with `wakes` on lvstill, per frame, before:

| Wake | Caused by | Led to |
|---|---|---|
| 1 | deadline (the frame's time, `frame_hold`) | redraw asked |
| 2 | the eventfd ping of `request_redraw`, then `RedrawRequested` | drawn |
| 3 | nothing Ljós knows: the compositor's frame callback | nothing |

The first wake asked only at its end (`settle`), so winit answered in a wake of its own.
Now the wake at the deadline asks as it starts (`NewEvents`), and winit's redraw loop, later
in the same turn, lets the window draw:

| Wake | Caused by | Led to |
|---|---|---|
| 1 | deadline, `RedrawRequested` | drawn |
| 2 | the ping of that request, already answered | nothing |
| 3 | the compositor's frame callback | nothing |

The frame is drawn ~0.6 ms sooner after its time (two wakes ~0.6 ms apart became one), and
the loop's share fell (callgrind, per frame shown, thousands of instructions):

| Phase | lvstill before | after | listview before | after |
|---|---:|---:|---:|---:|
| winit, Wayland, calloop | 23.9 | 21.6 | 20.0 | 19.5 |
| Event loop, the rest | 19.9 | 17.7 | 15.5 | 15.1 |

listview's frames are asked for by input, not by a deadline, so they are as before. Under
callgrind, lvstill drew 14 % more frames in the same 20 s (3741 against 3281): each frame
comes sooner after the last one's hold ends. A window
the platform answers at once, or one out of sight, is still run at the end of the turn,
after its input. `features/window/frame-on-time.feature` pins it.

**Real time, back to back with qml6** (M instructions/s, all threads, 220 notches):

| | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| `lvstill.qml` | 19.06, 19.07 | 18.99 | 15.21 |
| listview | 199.72, 199.85 | 200.25 | 103.06 |
| listview with `reuseItems: true` | 81.05, 81.37 | 81.22 | 60.58 |

"Before" is the commit before this change, run on either side of "after". In real time the
change is within noise: a turn that only asked costs ~1.5 k, under 0.1 M/s at 60 Hz. The
gap from "Paint while scrolling" (208.83 and 86.60) is "Text per delegate", which landed
in between.

**lvreuse against qml6.** Under callgrind the two ran at different rates (Ljós 13.5 rows a
frame, qml6 8.3, the rows a 60 Hz frame scrolls past), so this is per row. A logged run
showed each row's text laid out once, and no layout repeated:

| Per row (thousands) | Ljós | qml6 |
|---|---:|---:|
| Text layout (`TextEngine::layout`; qml6 `QQuickTextPrivate::updateSize`) | 45.6 | 55.6 |
| Scene and engine, text left out | 69 | 45.5 (main thread, text left out) |

Ljós's render side (compose, B0, GL, present, egui, loop) runs ~20 M/s in real time, below
qml6's render thread (26.6 M/s, plus ~4 M/s on its GL threads). At 500 rows/s, the
~23.5 k/row more in making and binding a delegate (~+12 M/s), less ~10 k/row in text
(~−5 M/s), is most of the gap. That is B26–B30's create and destroy area.

Of the 45.6 k a text costs, Ljós's own code is ~8 k: the line widths with Qt's advances
(5.3 k, already read once a layout, its advances cached per glyph), `extract` (1.7 k),
lines and metrics (1.4 k). The rest is Parley, harfrust and ICU: shaping 21 k (harfrust
5.5 k, a font query per cluster 3.3 k, runs 3.7 k), `analyze_text` 5.2 k, line breaking
3.5 k, styles 2.2 k.

**Left, and why** (DECISIONs):

- **The ping wake** is winit's: every `request_redraw` writes its eventfd, and it is read in
  the next turn even when the request was answered in this one. Drawing without
  `request_redraw` would bypass winit's frame-callback gate, which keeps a hidden window
  from drawing.
- **The frame callback wake** is the compositor's. Drawing in it instead (ask after each
  frame, let the callback answer) would make two wakes a frame on a compositor whose
  callbacks follow the display. But the sandbox's callbacks come ~50 µs after the commit,
  so that cannot be proved there, and on X11 it costs a turn. Left.
- **The egui pass**, ~31 k a frame, is egui's own begin and end. Each frame Ljós adds one
  widget: the view's `interact`, which keyboard focus needs. It sets no style, viewport or
  options, and the output it reads is the platform output. egui stays the host.
- **The accesskit compare** (~5.5 k a frame, only while an AT client is connected) is
  `accesskit_consumer` comparing node properties. accesskit 0.25 takes whole trees from
  egui, with no dirty marks to compare by. The scene's own graft into it is already
  incremental.
- **Text layout** costs less per row than qml6's. What is left is Parley's and harfrust's.
- **Idle**: `idle.qml` (a still rectangle and text) recorded for 2 s: one wake, the
  inspector's own request, which led to nothing, before and after. Pixels: no paint code
  changed, and a frame draws what it drew before, only sooner.

### Start-up against qml6 (2026-10-26)

The five apps of "Memory against qml6" above, started with `ljos bench --profile desktop
--warm-up --repeat 5` in the cua sandbox. Each cell is the median of five runs. "Answers
input" is when the window first fills a strip it was resized into. CPU is the cgroup's, from
launch to the first frame.

**A fair photosurface.** The corpus's photosurface has `gneiss.frag` but no
`gneiss.frag.qsb`, so a bare `qml6` draws no background behind the photos. Ljós compiles the
`.frag` beside the `.qsb` path itself. qml6 was measured on a scratch copy of the app with the
`.qsb` baked by Qt's `qsb` (`--glsl "100es,120,150" --hlsl 50 --msl 12`); Ljós ran on the same
copy. The background is a full-window program that is costly per pixel on llvmpipe. With it,
qml6 used 2.8–8.8 s of CPU in its first 2 s, 2.2–6.1 s of it on llvmpipe's threads, whatever
`LP_NUM_THREADS` was. Before the fix below, Ljós used 1.1 s.

**Where Ljós's photosurface start went.** Per-thread CPU after 1.5 s, before the cut: 1.10 s
for the process, of which llvmpipe's four threads used 1.04 s, the main thread 60 ms, and
threads that had ended about 180 ms. The background ran twice, at about 130 ms of wall time
each. It ran once onto the window for the first frame. It ran again into a picture of its own
when the second frame drew it unchanged, and later frames copied that picture.

**The cut.** An effect whose program is linked for this draw is now drawn into its picture at
once and copied. A still background therefore runs once. The first draw of an effect is
nearly always on the window's first frame, where effects are mostly still. An effect that
changes each frame pays for one picture, then runs straight onto the window as before. Three
runs each, process CPU after 1.5 s:

| | before | after |
|---|---:|---:|
| photosurface | 1100, 1080, 1170 ms | 610, 610, 620 ms |

Screenshots of the settled window are byte-identical before and after.
`a_still_effect_goes_into_its_picture_from_its_first_draw` pins the rule.

**The table.** In ms, median of five, with Ljós after the cut. The one-minute load average was
14–65 while the bench ran.

| App | first frame, Ljós | qml6 | answers input, Ljós | qml6 | CPU to first frame, Ljós | qml6 |
|---|---:|---:|---:|---:|---:|---:|
| minimal | 70.6 | 246 | 99.5 | 278 | 111 | 189 |
| list1000 | 54.5 | 292 | 71.4 | 339 | 106 | 212 |
| thermostat | 143 | 476 | 327¹ | 2070¹ | 179 | 501 |
| todolist | 115 | 385 | 132 | 469 | 173 | 307 |
| photosurface | 205 | 326 | 348 | 599 | 649 | 878 |
| photosurface, before the cut | 191 | | 464 | | 635 | |

¹ Thermostat answered the resize in one run of five on Ljós and two on qml6. In the others no
frame filled the strip in time.

- **The cut's effect.** It moves photosurface's answer to input by about 116 ms. The second
  run came after the first frame, so the first frame and the CPU spent to reach it do not move.
- **Where photosurface's CPU still goes.** Most of the 649 ms to the first frame is the
  background's one run: 4 × 120 ms on llvmpipe's threads. The rest is small: the main thread
  about 30 ms, the font, GPU (EGL) and compile threads 10–20 ms each. The first frame needs
  that run, and qml6 pays for it too.
- **The earlier figure.** The 661 against 271 ms in "Memory against qml6" compared Ljós with
  a qml6 that drew no background.
- **The other apps.** The cut does not touch minimal, list1000, thermostat or todolist, so
  their before-and-after differences are noise. Ljós is ahead of qml6 on every app and every
  column.

#### todolist's 8.3 s `Component.onCompleted`

The 8.3 s once seen at todolist's `Component.onCompleted` is the disk, not Ljós. The method
was a fresh `XDG_DATA_HOME` for each run, so `LocalStorage` created the database anew. An
`LD_PRELOAD` shim timed every `fsync`, `fdatasync` and `syncfs` by file. Both runtimes were
run.

| Run | Ljós syncs, total ms (longest) | qml6 syncs, total ms (longest) | a 4 KB `dd conv=fsync` beside them |
|---|---|---|---|
| 1 | 6, 3161 (1844) | 6, 2883 (1666) | 739 ms |
| 2 | 6, 1478 (1404) | 5, 3296 (1218) | 13 ms |
| 3 | 6, 19 (7) | 6, 1646 (942) | 1419 ms |

- **Ljós's waits.** Creating the database syncs:
  - the rollback journal twice (758 and 343 ms in one run, whose other times follow);
  - its directory (408 ms);
  - the database (2996 ms);
  - the write-ahead log once WAL is on (1202 ms).
- **qml6's waits.** qml6 makes as many syncs. Its longest were 2548 ms on its settings file's
  `.ini.lock` and 1800 ms on a temporary file it deletes.
- **A raw sync stalls too.** A 4 KB `dd conv=fsync`, with neither runtime involved, took up to
  1.4 s.
- **The drive was busy.** `/proc/pressure/io` read `some avg10=81.8`, `full avg10=70.4`, with a
  load average of 79. The drive (NVMe, 91 % full) was being written by other builds.

The syncs are what creating a database durably costs, and qml6 makes the same number. Ljós
already commits in WAL mode with `synchronous=NORMAL` (see "LocalStorage: a commit that does
not wait for the disk" above), so later commits do not wait for the disk. The time depends on
the drive; on a quiet one the six syncs took 19 ms. No fix was made. Moving creation off the
UI thread would change when `Component.onCompleted` sees the tables, which todolist reads at
once.

### Software drawing to shared memory: a lead (2026-10-26)

A design note; built and measured in SHM1 below. Without a GPU, Ljós draws through EGL on Mesa's llvmpipe, as qml6
does. The context alone costs:

- about 31.6 MB of malloc, whatever GL API is asked for;
- 46 MiB of LLVM mapped (26 MiB PSS);
- 14 MiB of Mesa's driver;
- four rasteriser threads;
- 100–265 ms of EGL start (see "Where the first frame's time goes").

On the minimal app that is most of Ljós's 112 MiB of PSS.

**The path.** The window would be a Wayland `wl_shm` buffer, or an X11 `MIT-SHM` image, filled
by the CPU, with no EGL display made:

1. The frame's display list is drawn with R, the `vello_cpu` renderer `ljos_render` already
   has. R is deterministic: one thread and a baseline SIMD level. For the screen it would use
   `vello_cpu`'s threads and the machine's SIMD level.
2. Only the damaged rectangles are drawn and attached (`wl_surface.damage_buffer`), with
   two or three buffers in turn, so a blinking cursor costs its own rectangle.
3. The B0 path (epaint meshes) is left out. Egui-drawn parts, such as the shell's inspector,
   would need a CPU mesh rasteriser of their own, or would not be offered on this path.

**What it would save.** About 30 MB of malloc and 60 MiB of mappings per process. No EGL
start, no JIT, and no wakes of llvmpipe's threads at each frame. A frame that changes a little
would draw and send only its damage.

**What it would cost.**

- **ShaderEffect draws nothing.** This is as in R today, and as in Qt's software scene graph.
  A naga-to-CPU interpreter would be far slower than llvmpipe's JIT. Photosurface's
  background would be lost.
- **Full-window repaints cost more.** Large repaints (a scrolled list, a resize, a
  full-window animation) are drawn by Ljós on the CPU without llvmpipe's JIT-compiled
  rasteriser. That needs measuring against B0 on llvmpipe; `vello_cpu` with its threads may
  well be faster for paths and text.
- **Buffer memory.** 4 bytes a pixel per buffer: 2.4 MB at 1024×600, 7.4 MB for three.
- **Two renderers to keep the same.** R and B0 already must agree, and the reference
  scenarios compare them.

**When.** It is chosen where EGL offers only llvmpipe or softpipe, or when asked for. A
machine with a GPU keeps GL.

### SHM1: a window drawn on the processor, against B0 on llvmpipe and qml6 (2026-10-26)

The lead above, built. `LJOS_PAINTER=cpu` (or `ShellConfig::drawn_on_processor`) draws each
window with R into a `wl_shm` buffer (an `MIT-SHM` image on X11) through softbuffer. It makes
no EGL display:

- R draws with `vello_cpu`'s workers (the processor's cores less one, at most three) and the
  machine's SIMD level;
- only the parts that differ from the last frame are drawn (`frame_damage`: an item that
  moved, changed what it draws, its opacity or its clips, came, went or fell behind another);
- only the changed rectangles are given to `wl_surface.damage_buffer`;
- Wayland's two buffers take turns, and a buffer's age says which earlier changes it lacks.

What R cannot draw is left out and logged once. That is a `ShaderEffect`'s program, what egui
draws (the inspector), and a window that can be seen through (drawn over black). The B0
scenarios' comparison checks the path: `features/window/cpu-drawing.feature` compares the
window's screencopy with R's picture of the same frame (`app_screenshot`), within 2 levels,
before and after moves, colour changes and scrolls in a clip.

**Method.** The sandbox is headless sway with `WLR_RENDERER=pixman`, so B0's EGL is llvmpipe
(`LP_NUM_THREADS` capped at 4, as the shell does) and qml6 draws with llvmpipe too.

- **Benches.** `ljos bench --profile desktop --warm-up --repeat 5 --settle 1 --idle 3 --load 3
  --gesture scroll`, the manifest of "Start-up against qml6". The apps are list1000, minimal,
  thermostat, todolist, listview and animations (a full-window animation). Medians of 5.
- **Malloc.** The `LD_PRELOAD` counter of "Memory against qml6", read 8 s after start and
  again after 10 s of scrolling.
- **Load.** The machine was shared, at a load of 12–104. Start-up times moved by up to 2× from
  run to run, in both directions, so read them as the same order. A first run had the
  processor faster everywhere (minimal 38 against 94 ms).

**Memory** (MiB; private and swapped is the process's own, after driving):

| app | PSS cpu | PSS B0 | PSS qt | own cpu | own B0 | own qt |
|---|---:|---:|---:|---:|---:|---:|
| list1000 | 63.1 | 103 | 222 | 17.7 | 44.9 | 90.2 |
| minimal | 56.0 | 127 | 161 | 11.7 | 37.9 | 83.2 |
| thermostat | 97.4 | 50.4* | 181 | 48.3 | 83.1 | 143 |
| todolist | 85.8 | 131 | 214 | 30.9 | 58.8 | 111 |
| listview | 60.6 | 126 | 195 | 17.9 | 46.2 | 90.4 |
| animations | 62.4 | 151 | 224 | 16.6 | 44.2 | 98.4 |

\* B0's thermostat had been swapped out when its PSS was read. Its own memory, swap counted, is
the comparable figure.

Settled with nothing swapped (RSS / PSS / threads):

| app | cpu | B0 | qt |
|---|---|---|---|
| minimal | 69.1 / 60.8 / 15 | 155 / 132.8 / 16 | 219.7 / 181.6 / 71 |
| list1000 | 74.9 / 66.9 / 15 | 162 / 109 / 16 | 226 / 188 / 71 |
| thermostat | 119 / 111 / 15 | 209 / 155 / 16 | 302 / 264 / 71 |
| todolist | 95 / 87 / 16 | 178 / 160 / 17 | 260 / 222 / 71 |

On minimal, B0's extra is LLVM (37 MiB PSS), heap (21 MiB against 3.3) and Mesa's driver
(12 MiB). The processor's path maps none of them: 145 MiB mapped, against 406 for B0 and
about 490 for qml6.

**Malloc** (live / peak):

| app | after start: cpu | B0 | qt | after scrolling: cpu | B0 | qt |
|---|---:|---:|---:|---:|---:|---:|
| minimal | 7.05M / 7.11M | 47.6M / 49.0M | 135.2M / 136.6M | 7.55M / 7.61M | 47.6M / 49.0M | 135.2M / 136.6M |
| list1000 | 11.1M / 11.1M | 52.1M / 53.3M | 138.5M / 139.8M | 13.7M / 14.1M | 54.3M / 54.8M | 139.5M / 140.9M |
| thermostat | 48.6M / 48.9M | 95.7M / 115.8M | 192.0M / 203.9M | 49.3M / 49.4M | 104.1M / 115.8M | 192.3M / 204.0M |
| todolist | 27.3M / 27.4M | 71.1M / 71.4M | 151.3M / 152.9M | 28.7M / 28.7M | 70.9M / 73.8M | 151.6M / 154.5M |

**Start to first frame** (ms, median):

| app | cpu | B0 | qt |
|---|---:|---:|---:|
| list1000 | 69.6 | 65.4 | 361 |
| minimal | 97.4 | 81.7 | 669 |
| thermostat | 145 | 381 | 639 |
| todolist | 193 | 98.3 | 443 |
| listview | 62.7 | 70.5 | 405 |
| animations | 128 | 144 | 457 |

**Frames while scrolling and animating.** The median and p95 frame intervals were 16.1–16.3
ms for all three on list1000, listview and animations: every frame was shown. Input to frame
had a median of about 8.7 ms on all three. In the first run, B0's p95 input-to-frame was 30.8
ms (listview) and 65.6 ms (animations), against 15.3 and 15.6 on the processor.

Processor time while driven (% of one core):

| app | cpu | B0 | qt |
|---|---:|---:|---:|
| list1000 | 10.8 | 9.97 | 14.0 |
| listview | 15.6 | 14.1 | 25.6 |
| animations | 14.8 | 7.08 | 19.5 |

A frame that repaints the whole window costs the processor's path about twice B0's time on
llvmpipe. It is still below qml6's, and no frame is late.

**Idle.**

- The static apps use 0.00 % of the processor at rest on both paths.
- Both show 0.33 wakes a second in the 3 s window. That is one wake of the main thread after
  the last frame, not polling: with a 1 s window it reads 1.00.
- Animating apps wake per frame: listview 1496 (cpu), 1338 (B0) and 6757 (qt); animations
  6425, 1174 and 6512 over the window.

**Pi Zero profile** (`--profile pi-zero`, repeat 3; cpu / B0 / qt):

| | list1000 | listview | animations |
|---|---|---|---|
| first frame (ms) | 353 / 1264 / 1155 | 311 / 654 / 1293 | 345 / 561 / 892 |
| processor driven (%) | 3.0 / 13.4 / 7.1 | 6.0 / 9.5 / 12.5 | 5.2 / 4.4 / 7.9 |

- PSS was about 61, 144–157 and 225 MiB.
- All three showed 59–60 frames a second while driven.
- At rest, listview's animation showed 59 frames a second on the processor and 39 on B0.

**R's cost per frame** (in-process; 800×600 unless said, wall ms, workers 0 / 3 / 8):

| | wall | processor |
|---|---|---|
| 200 rounded rectangles | 0.56 / 0.42 / 0.39 | 0.55 / 0.80 / 1.00 |
| 2000 rounded rectangles | 3.99 / 2.00 / 2.07 | 3.9 / 4.9 / 6.0 |
| 2000, 1920×1080 | 4.98 / 3.23 / 2.96 | |

Three workers halve a busy frame. Eight draw it no sooner and cost a fifth more of the
processor, so the shell gives three. Copying R's RGBA into the window's `0RGB` words as
little-endian words with the outer bytes swapped took the copy from 0.15 to 0.07 ms at 800×600,
and from 0.65 to 0.34 ms at 1920×1080.

**DECISION: the shell chooses the processor where EGL would draw in software.** It wins on
memory everywhere (PSS a half to a third of B0's, malloc 7–49 MB against 48–104 MB). It shows
every frame while scrolling and in a full-window animation, at no worse latency. Start-up is
the same order on this loaded machine and 2–4× sooner on the Pi Zero profile. The rule
(`crates/ljos_shell/src/software.rs`) is told without making an EGL display. The processor is
chosen when:

- Mesa is told to draw in software (`LIBGL_ALWAYS_SOFTWARE`, `MESA_LOADER_DRIVER_OVERRIDE` of
  `swrast`, `kms_swrast`, `llvmpipe` or `softpipe`);
- or no `/dev/dri/renderD*` opens;
- or the Wayland compositor offers neither `zwp_linux_dmabuf_v1` nor `wl_drm`;
- or the X server has neither DRI3 nor DRI2.

It applies only with an opaque background, and never when the user chose: `LJOS_PAINTER=gl`,
`wgpu` or `cpu`, or `drawn_on_processor`. The log says so at start. A window that shows what R
leaves out, a `ShaderEffect`'s program, what egui draws or a window that can be seen through,
takes every window to GL before it is drawn. That happens once, so nothing is lost; EGL then
starts as it would have at start-up (`features/window/software-choice.feature`). A machine with
a GPU keeps GL.

The cost is a full-window repaint at about twice B0's processor time, which matters on a small
machine at 60 frames a second. The desktop scenarios keep checking B0 (`LJOS_PAINTER=gl`)
unless one lets the shell choose.

### Analysis by class and kept runs for one-line plain text (2026-10-26)

This follows "One-line plain text without Parley's builder", measured the same way. The dump
also has ten more texts with runs of spaces, punctuation and quotes. It is the same byte
for byte as before this work.

**Analysis by class.** Parley's analyser records a script and a set of boundary flags for
each character, and nothing else. In printable ASCII every letter gets the same entry
(Latin; ALetter for words; AL for lines), and so does every digit (Common, Numeric, NU). A
text therefore analyses exactly as its *fold* does: the text with letters made `a` and
digits `0`, the other characters kept, plus the line-breaking setting. The direct path keeps
analyses by fold, and a list's rows share a few of them ("Row 7" and "Row 9" are both
`aaa 0`). A unit test holds the fold to Parley's analyser. It analyses both the text and
its fold under both line-breaking settings: every printable pair, every class triple, and
2000 longer texts. The `char_info`, bidi levels and paragraph level must be equal for each.
At most 64 folds are kept across frames, and a frame can leave at most 128.

**Kept runs.** A run shaped and snapped by the direct path is kept, keyed on the text, the
font blob and index, the size and the synthesis. The size covers the optical-size
variations, and no features are asked for. When the same text in the same font comes back,
it skips analysis, shaping and snapping. That happens for a delegate made anew, or a row
scrolled back after its layout was dropped. Up to 2048 runs of up to 64 bytes are kept
across frames: about 0.5 MB for short rows, at most 4 MB. A frame can leave 3072. Over
2048, `end_frame` drops the oldest one by one, and a run unused for 600 frames goes.
Nothing is added unless something is laid out, so an idle application does not add to it.
Registering a font or changing the default family drops the runs. A hit still prunes and
loads through Parley's source cache, so the run carries the blob Parley's own path would
give it. `TextMemory` counts the runs and their bytes, and `CacheStats::runs_reused` counts
the hits.

| Microbench, per row | Before | After |
|---|---:|---:|
| New row (every text distinct, as before) | 29,186 | 25,221 (−13.6 %) |
| `shape_text` | 13.4 k | 13.4 k |
| `analyze_text` | 5.1 k | 0 |
| A row again, after its layout was dropped | ≈ 29 k | 8,878 |

On the microbench every text is new, so kept runs only cost there. The cost is about 1.1 k
per row: keeping the run, and dropping it once over capacity. Without them a new row would
be about 24 k.

| listview under callgrind (main thread, per delegate) | Before | After |
|---|---:|---:|
| Delegates (`build_layout` calls) | 9843 | 9872 |
| Frames | 880 | 1161 |
| `shape_text` calls | 9838 | 7955 (19.4 % of rows reused a kept run) |
| `analyze_text` calls | 9838 | 8 |
| text: layout and shaping (`ljos_text`) | 30.2 k | 23.4 k (−22.5 %) |
| text: completion, measure, follow (`ljos_quick::text`) | 31.1 k | 31.3 k |
| Total | 359.6 k | 377.6 k |

The total is per delegate, but the "after" run drew 32 % more frames in the same 20 s window
with the same number of delegates. The per-frame phases (paint, view refresh) are divided
over the same delegates, so the total cannot be compared between the two runs. The
`ljos_text` row can. `bench/listview.qml` has 10,000 distinct rows, and about 500 are made
per second, so a row comes back long after its layout was dropped. The rows made in the
last ~4 s before the turn are still kept, and the rest are shaped again.

| Scroll, M instructions/s (`scr.sh listview`) | Before | After |
|---|---:|---:|
| Ljós | 191.55, 191.00 | 189.36, 188.92 |
| qml6 | 102.59 | 102.88 |

A wheel scroll of ten notches revisits rows whose layouts are still kept, so text barely
shows in it.

Ten ListView delegates make 1451 allocations, up from 1437 after B26. Each new row's kept
run adds its text and its glyphs (+21), and the analysis no longer made saves 7. Ten plain
items still make 983.

DECISIONs:

- The analysis is shared through a fold, not computed by hand. `Analysis` can only be
  filled by `Analyzer::analyze`, so the ICU result is reused rather than reimplemented. The
  unit test checks that reuse against Parley's analyser.
- Kept runs are bounded by count (2048 across frames, 3072 in a frame) and by text length
  (64 bytes). Their age limit is 600 frames, longer than a layout's 30, because a row
  scrolled away comes back seconds later. `FrameCache` gains a mode that drops the oldest
  entries of a frame one by one. Otherwise a frame that kept 3072 would hold them all
  until they aged out.
- No Gherkin scenario: nothing a QML application can observe changes. The Rust tests in
  `crates/ljos_text/tests/cache.rs` cover reuse, identical layouts, bounds, ageing and
  font changes.

### B27 (H2): the scene item record, and anchors only where a line is given (2026-10-26)

B27 is step 3 of the delegate track. It made two changes:

- What the scene asks of an item's type is an `ItemClass`, cached on the type's meta
  (`MetaObject::class`, asked through `Engine::class_of`) and computed once per type: its
  kind, whether it paints or polishes, and whether it is a Loader, a MouseArea or a
  Flickable. The dozen `ObjectId`-keyed maps the scene kept per item are one dense record
  (`scene::Records`: a slot index and a `Vec`), with the item's effects held inline. What
  the effects mark is a `Marks` record beside it, and each kind of mark is queued when it
  goes from clear to set, so a pass drains a list instead of hashing a set.
- A property can carry a `given` hook (`#[qml(given = path)]`). It is told after an author
  gives the property a value, a binding or an override. It is not told when a binding
  re-runs, or when an animation, the host or a layout writes the property. The anchor
  lines carry the hook, so an item gets an anchors effect only once one of its lines is
  authored, at adoption or later. Before, every item the scene showed had one.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets):

H2 measured on top of B26 (1437, 1412, 1393). The delegate row here is on top of TEXT3's kept
runs, which add 14; B27 saves the same 25 and 19.

| | Before | One record | Anchors when given |
|---|---:|---:|---:|
| ListView delegate, heap allocations | 1451 (145.1) | 1426 (142.6) | 1407 (140.7) |
| ListView delegate, effects | 160 (16) | 160 (16) | 150 (15) |
| `createObject` item, heap allocations | 983 (98.3) | 955 (95.5) | 906 (90.6) |
| `createObject` item, effects | 150 (15) | 150 (15) | 130 (13) |
| Ten `createObject` items destroyed, graph nodes | 230 | 230 | 210 |

| listview under callgrind (main thread, 20 s after 40 s), per delegate | Before B27 | B27 |
|---|---:|---:|
| Total ÷ text layouts | 379.3 k | 336.1 k, 352.2 k, 357.7 k |
| `anchors::run`, inclusive | 20.6 k (2.03 runs) | 16.5 k (1.02 runs) |
| `QuickScene::adopt`, inclusive | 73.6 k | 70.2 k |
| `QuickScene::unadopt`, inclusive | 15.6 k | 16.4 k |

The three B27 runs spread by 21 k; their median is 7 % under the baseline. Only the Text
is anchored in this delegate, so the Rectangle's anchors effect, and its run, are gone.
`unadopt` now also lets go of the anchors' entry for the item. qml6's figure is still
~200 k on its main thread.

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós | qml6 (same run) | Ljós, B26 |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 18.99 | 15.21 | 19.07 |
| listview | 186.82 | 102.75 | 197.96 |
| listview with `reuseItems: true` | 71.35 | 60.63 | 80.27 |

The scroll runs were made with a load average of about 85 from other builds on the
machine. qml6 in the same runs moved by under 0.3 %.

DECISIONs:

- The anchors are told through a general engine seam: a `given` hook on the property's
  meta, filled by a macro attribute. The other way was a write filter on the lines. It
  would run before the value is stored, so the effect would start and then run again,
  and it would put a filter call on every binding evaluation.
- The anchors module owns its own lifecycle (`anchors::follow` / `unfollow`), in the same
  way as the text's. Its state is a per-engine extension, because the hook is told about
  the object, not the scene. Showing is counted across scenes. At adoption the module
  checks the lines with one `Engine::authored` mask, so an item authored with anchors
  starts in the same order as before.
- The five new scenarios in `layouts/anchors.feature` pin behaviour that already held: a
  line or fill a script writes, a line a script binds, a line whose expression first
  gives `undefined`, and a state's AnchorChanges on an item with no anchors. They pass on
  Qt and on Ljós before and after the change.

### B11 (ilang): a view lays out its model's changes once, at polish (2026-10-26)

A ListView laid its rows out at every change of its model, so a loop of appends laid them
out once per row. Now a change marks the view and asks for a frame, and the polish before
that frame lays the rows out once for every change until then, as qml6 does. `count` reads
the model's count at once and is told with the layout. `forceLayout()` and
`positionViewAt*()` lay the rows out first. `features/views/list-layout-at-polish.feature` and
`list-layout-hidden.feature` pin what a script sees and the order of the signals, against
qml6 6.11.2.

**Method.** Instructions are user-space instructions of the whole process, all threads,
from start to exit (`perf_event_open` with `inherit`). calqlatr's presses are 3000
`digitPressed`/`operatorPressed` calls in 30 timer ticks, and that run minus the same app
with no presses. Milliseconds are the same scripts' own `Date.now()` around the loop, on a
loaded machine (load ~80), so the instruction counts are the ones to compare. The scroll
benches are `scr.sh` from "Paint while scrolling" (GPU sandbox, 220 notches). "Before" is
origin/main (00b50b7e), built the same way.

| | Before | After | qml6 |
|---|---:|---:|---:|
| calqlatr, 3000 presses (M instructions) | 1167 | 955 | ~530 |
| calqlatr, 3000 presses (ms) | 84–94 | 74–111 | 41–57 |
| 2,000 appends in one handler (ms) | 28–31 | 3–4 | 1–2 |
| 2,000 appends, until laid out (ms) | 28–31 | 4–5 | 27–54 |
| 10,000 appends in one handler (ms) | stopped at its 100 ms slice | 22–26 | 5 |
| 10,000 appends, until laid out (ms) | — | 26–29 | 28–42 |
| listview, scrolled (M instructions/s) | 187.3, 187.3 | 187.1, 187.2 | 102.7, 103.1 |
| listview with `reuseItems: true` (M/s) | 71.1, 71.0 | 70.8, 70.3 | 60.5, 60.6 |

"Until laid out" is when `contentHeight` first changes with every row there. qml6 waits for
its next frame for that, and Ljós's polish comes sooner after the handler.

**calqlatr.** Deferral alone took 1167 M to 1161 M. Each press appends at most one row and
then calls `positionViewAtEnd()`, which needs the layout at once, so a press still laid the
rows out. Most presses only edit the last row, and then going to the end ran a pass where the
view already was. That pass laid out the same rows the layout effect had just laid out. With
nothing waiting and the view at the place asked for, it is now skipped (955 M).

**Left** (DECISIONs):

- **calqlatr is still ~1.8× qml6's instructions.** What is left is a press's own work: the
  model's change, its roles reaching the delegate's bindings, and the pass for a press that
  appends. The next step is a profile of that per press, not more deferral.
- **The 10,000-append loop** costs ~2.2 µs an append against qml6's ~0.5 µs. That cost is
  `ListModel.append` and its change notice, not the view: the view now does nothing until
  the polish.
- **`currentIndex`** still moves at once when rows are inserted before the current one. qml6
  moves it at layout too. The scenarios pin what qml6 shows for appends and removals at the
  end; inserts before the current row are not pinned yet.
- **Start-up** read ~9.5 M higher for every build from the working tree than for the
  origin/main build from an exported copy. It was the same before and after these commits,
  so it comes from the build directory, and presses are compared as differences.

### TEXT4: wrapped plain text, and a field's placeholder measured when read (2026-10-26)

#### Wrapped plain text without Parley's builder

TEXT2 and TEXT3 shaped one-line plain text directly. A plain text that wraps still built
Parley's layout to break it, and an elided last line built two more: the rest of its
paragraph and each cut. Chat, mail and list rows with descriptions are made of such texts.

- `wrap_direct` breaks a directly shaped text where Parley's breaker would. `break_atoms`
  takes the soft wrap opportunities from the shared analysis, lets spaces hang, and breaks
  inside a word only where the wrap mode allows. The ink re-break is the same as before.
  It covers ASCII text without inline places or justification; any other text takes the
  builder as before.
- `elide_direct` cuts a plain left-to-right last line the same way `elide` does. It shapes
  the rest of the line and each cut directly. Right-to-left text, text with places, spans
  or inline objects takes the builder.
- Unit tests in `crates/ljos_text/src/engine.rs` hold the direct path to an engine that
  builds every layout. They cover breaks over scripts, bidi, hyphen and space runs, tabs, a
  zero width, ligatures and ink past the width, in three wrap modes, with and without an
  elided last line, and cuts in every elide mode. The `@qt-parity` wrap and elide
  scenarios in `features/text/` pass unchanged.

`bench/wrapped.qml` is a ListView of 10,000 rows: a bold sender and a message wrapped on
two to four lines, the fourth elided into the third.

| `bench/wrapped.qml` under callgrind (main thread) | Before | After |
|---|---:|---:|
| Total | 6,268M | 5,443M |
| Delegates in the window | ≈ 4094 | ≈ 5298 |
| `build_layout`, per delegate | 904 k | 396 k |
| Main thread, per delegate | 1.53M | 1.03M |
| Parley `StyleRunBuilder` calls | 10,214 | 0 |
| `elided_line`, per call | 614 k | 292 k |

| Scroll, M instructions/s (`scr.sh wrapped`) | Before | After |
|---|---:|---:|
| Ljós | 778 | 518–519 |
| qml6 | 442–443 | 442–443 |

Ten wrapped ListView delegates make 1452 allocations, down from 1772
(`ten_wrapped_listview_delegates`).

Left for the delegate track: the wrapped delegate looks up `WrapMode` and `ElideMode` by
type name 20 times, and an elided line still costs 292 k a call, most of it shaping the
cuts.

#### A field's placeholder, measured when read

A `TextField` or `TextArea` measured its placeholder on every polish, even when it was not
shown. That was 12K a field for each font switch ("A theme or font switch against qml6,
continued"). Its measured writes then started a second round of the field's bindings.

Now a `Text` that paints nothing is not polished. That is a plain text that is empty (a
field's placeholder while the field has text, or one without placeholder text), or one
whose own `visible` is false. Its seven measured properties are made on read instead:
`implicitWidth`, `implicitHeight`, `contentWidth`, `contentHeight`, `lineCount`,
`truncated` and `baselineOffset`. Whatever follows them is told they changed. The first
read measures once, for all seven. The makers are function pointers
(`Engine::make_on_read_by`), so nothing is allocated per property.

A text that is only not shown yet, such as a view's new row, is still polished: its first
frame needs the layout anyway, and the delegate budgets stay at 1451 and 983.

An empty text no longer reads its width and height, because it is one empty line in any
room. `empty_text_measures_the_same_in_any_room` checks every measured value over widths,
heights, wrap and elide modes, line counts and line heights.

qml6 gives a text its new size as soon as what it follows changes. Ljós now does the same
for a hidden text, an empty text and a field's placeholder
(`features/text/measured-on-read.feature`, `@qt-parity`, checked on Qt 6.11.2). A shown
text that is not empty still gets its new size at the next polish. That scenario is
`@backlog` until the delegate track takes the text polish out of the graph.
`features/text/measured-once.feature` pins that a hidden text is measured only when read.

20 Fusion `TextField`s on a `Pane`, `font.pixelSize` switched 13/14 a frame apart
(`frame_profile --apart`, 20 switches, release):

| Per field per switch (callgrind) | Before | After |
|---|---:|---:|
| Total | 98.3K | 92.3K |
| Placeholder polish (`write_measured` and what follows it) | 26.4K | 0 |
| `measure_on_read` (mostly the field's bindings re-run on notify) | | 21.8K |
| `made_on_read` (the measure itself) | | 0.9K |
| `TextInputs::read` | 10.0K | 9.3K |

| 20 fields, per switch (wall clock) | Before | After |
|---|---:|---:|
| The write and its effects | 108–121 µs | 140–177 µs |
| The frame's pass | 52 µs | 14 µs |

The work moved from the frame into the write, where qml6 does it, and the sum barely
changed. A `tst_*.qml` loop of 400 switches with no frames between them shows the move
alone. Before, it skipped the placeholder until the end of the loop. Now it measures the
placeholder on every switch, as qml6 does:

| 400 switches, no frames | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| Basic | 0.098–0.113 ms | 0.140–0.178 ms | 0.028 ms |
| Fusion | 0.093–0.128 ms | 0.148–0.168 ms | 0.030–0.033 ms |

What is left of a field's switch is graph traffic. The field's `implicitHeight` binding is
evaluated before the placeholder's inputs effect runs, and runs again when it hears the
placeholder changed. Every input is a tracked read. Both belong to the delegate track's
text polish dirty bits.

DECISIONs:

- Only texts that paint nothing are measured on read. Keying on "not shown" also caught a
  view's new rows and added allocations to every delegate.
- Measuring on read pulls a value: the styles always follow the placeholder's
  `implicitHeight`, so it is measured on each switch, not skipped. It is measured once
  for all seven properties, outside the frame.
- `write_measured` writes over values still due without reading them, so a stale maker
  never runs.
- The wrapped path compares against the builder in unit tests and against qml6 through the
  existing wrap and elide scenarios. Nothing a QML program observes changed, so there is no
  new scenario for it.

### B28 (H2): an item's place and geometry as marks, not effects (2026-10-26)

B28 is phase a of the delegate track's dirty-bit seam (step 4). It made two changes:

- A native property can carry a `change` hook (`#[qml(change = path)]`). The hook is told
  after every change to the property: a value written by code, the host, an animation or a
  layout, and a change that reaches the property's binding. The graph notes the property's
  node (`Graph::set_noted`) and tells the engine once the change has spread. A dirty
  binding is told before it runs, and is not told again until it is read. A store to a
  property with no node calls the hook directly. Nothing is told on a store that leaves
  the value as it was.
- Item's x and y carry `scene::moved`, and z, width, height, visible, opacity, clip,
  rotation, scale, transformOrigin and transform carry `scene::reshaped`. The hooks mark the
  item in each scene that shows it. The scene reads the marked items when it places and
  lays out, where before each item had a place effect and a geometry effect. Flipable's
  sides and Flickable's `moving` mark the items they affect in the same way. Only user
  QML bindings, and the effects of later phases, stay in the graph.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets):

| | B27 (main) | B28 |
|---|---:|---:|
| ListView delegate, heap allocations | 1407 (140.7) | 1305 (130.5) |
| ListView delegate, effects | 150 (15) | 110 (11) |
| `createObject` item, heap allocations | 906 (90.6) | 833 (83.3) |
| `createObject` item, effects | 130 (13) | 90 (9) |
| Ten `createObject` items destroyed, graph nodes | 210 | 170 |

| listview under callgrind (main thread, 20 s after 40 s), per delegate | main (B27 + TEXT3) | B28 |
|---|---:|---:|
| Total ÷ text layouts, three runs | 338.3 k, 351.8 k, 331.3 k | 333.3 k, 340.3 k, 331.8 k |
| Median | 338.3 k | 333.3 k |
| `QuickScene::adopt`, inclusive | 70.3 k | 43.4 k |
| `QuickScene::unadopt`, inclusive | 16.5 k | 13.6 k |
| `Geom::read`, inclusive | 17.3 k (in the effect) | 15.2 k (at the frame) |
| `Engine::tell_change`, inclusive | | 1.5 k (12.2 calls) |
| `anchors::run`, inclusive | 16.5 k | 16.1 k |

Making and dropping an item no longer creates two effects and their first runs, so
`adopt` is 27 k cheaper. Most of that work moves to the frame. The geometry is still
read there, through eleven engine reads per item, so the median falls by only 5 k. qml6
does a whole delegate in ~200 k. Reading the fields directly from the item's record, and
not through the engine, is the next lever.

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós B28 | qml6 (same run) | Ljós B27 |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.00 | 15.19 | 18.99 |
| listview | 180.32 | 103.00 | 186.82 |
| listview with `reuseItems: true` | 70.33 | 60.57 | 71.35 |

Other lanes' builds were running on the machine during these runs. qml6 moved by under
0.3 % from B27's runs.

DECISIONs:

- The seam is general: an engine hook on the property's meta, filled by a macro attribute,
  and fed by a noted flag in the graph. It is not a write filter, which would miss changes
  that come from a binding, and it is not a scene-specific path in the engine. Values made
  on first read, alias objects and `store_value` (a value set before anything can observe
  it) are not told.
- Anchors keep B27's lazy effect and do not become a frame pass. Qt applies anchors as the
  line moves, so a script that reads an anchored item's geometry between a change and the
  next frame would see a stale value. The `given` hook still installs the effect.
- Implicit size has no mark. Only positioners, layouts and text polish read it, and those
  belong to phase c.
- Left for phase b: inherited visible and enabled (`inherit_effect`). Its covered case
  now only marks.
- Left for phase c: the polish and text effects, and the content effects.
- `may_scroll` is still fixed when the item is adopted, as before.
- The gate run on the merged tree has one failure that is not from B28:
  `positioning/position-source.feature` expects `supportedPositioningMethods` to be -1 and
  reads 4294967295. That comes from main's `d4335a0f` (an enum's number reads unsigned),
  which landed after the QtPositioning feature.

### TEXT5: wrapped delegates below qml6 (2026-10-26)

TEXT4 left `bench/wrapped.qml` scrolling at 518 M instructions/s against qml6's 442. It
also left two costs named: an enum looked up by type name in each delegate, and an elided
line at 292 k a call. Shaping turned out to be most of what was left. Parley's analysis
and shaper cost far more than the shaping itself for a plain text in a font it had
already set up.

- **Enum literals.** The compiler leaves `Text.WordWrap` and `Text.ElideRight` to the
  runtime. The runtime now keeps the number once per file. It is used only when the
  literal's type and key match (f865e23f). `literal_value` drops from 1,843 to 548
  instructions a delegate. Type lookups in `ten_wrapped_listview_delegates` drop from 20
  to 0.
- **Shaping by harfrust.** A printable ASCII text in a pinned font is one left-to-right
  Latin segment with a character a grapheme. The first text in a font at a size is shaped
  by Parley, which sets up a voice: harfrust's shaper data, instance and plan, with the
  size, metrics and variation coordinates Parley gave the run. Every later text in that
  font is shaped by harfrust directly, with Parley's buffer settings, and its glyphs are
  read as Parley's shaped clusters read. Soft wrap opportunities come from ICU4X's line
  segmenter with Parley's strict rules.
- **Elided lines.** The last shown line of a truncated wrapped text used to shape the rest
  of the text only to estimate the cut. harfrust's unsafe-to-break flags say where a text
  can be cut and shaped in two without changing. Where the rest starts after a space and
  the flags allow, its clusters are read from the whole text's glyphs, and only the cut
  is shaped. The first text in a font, a cut the shaper marks, or a single unbroken line
  shapes the rest as before. Right-to-left text, and text with places, spans or objects,
  still takes Parley's builder.
- **Tests.** Unit tests in `crates/ljos_text/src/engine.rs` compare against Parley:
  - soft breaks against Parley's analysis, over every printable class three deep, 2,000
    random texts and real sentences, for normal, break-all and keep-all;
  - harfrust's glyphs and clusters against Parley's shaper, across fonts, sizes, bold,
    italic and ellipses (over 100,000 texts);
  - each rest that the flags allow, shaped alone, against the whole text's glyphs;
  - elided lines against an engine that builds every layout, over seven widths, three
    elide modes and one to three lines, in Latin, Greek, Cyrillic, Hebrew and tabs.

  The `@qt-parity` wrap, elide, max-lines and size scenarios in `features/text/` pass
  unchanged.

| `bench/wrapped.qml` under callgrind (600 frames, 5025 delegates) | main (f865e23f) | TEXT5 |
|---|---:|---:|
| Total | 4,987M | 4,152M |
| Per delegate | 992 k | 826 k |
| `text::polish`, per delegate | 402 k | 241 k |
| `build_layout`, per delegate | 398 k | 231 k |
| `shape_direct`, per call | 154 k | 74 k |
| `elided_line`, per call | 305 k | 67 k |

| Scroll, M instructions/s (`scr.sh wrapped`, three runs) | main (f865e23f) | TEXT5 |
|---|---:|---:|
| Ljós | 497–504 | 417–419 |
| qml6 | 442–443 | 442–443 |

Two smaller steps fall inside the TEXT5 column. Before the elide change, the shaping change
alone gave 4,278M total on TEXT4's base (5,064M) and scrolled at 428–433. Before harfrust,
`elided_line` was 152 k a call.

Ten wrapped ListView delegates now make 1354 allocations, and ten one-line delegates make
1322 (+56 and +17). A new voice's harfrust shaper data sets up the font's lookups
lazily, as texts first use them, and the budget counts the first rows. Over a warmed
scroll, harfrust shaping adds about 0.2 allocations a text.

Left:
- ICU4X's line iterator is 129M of the 4,152M (25 k a wrapped text). Each message folds
  differently, so the per-fold cache seldom hits.
- Driving harfrust is 607M, 56 k a text, and is now the largest single cost.
- Attached members are applied from a copy (`value_fields_of` clones every assignment).
  So an attached enum literal's address is not the IR's, and the per-file number is
  checked against type and key rather than trusted by address.

### B29 (H2): inherited visible and enabled at the hook, and an item's marks and record split hot from cold (2026-10-26)

B29 is phase b of the delegate track's dirty-bit seam. It made two changes:

- **Inherited `visible` and `enabled`.** These used to be one effect per item, which read
  the parent's value. Now the change hooks of `visible` and `enabled`, and the visual
  parent listener, pass the value down when the change is told. A child whose value
  changes tells its own children through its own hook. A `MouseArea` keeps its own
  `enabled` and passes its parent's on to its children. A script that hides a parent
  and then reads a child's `visible` reads false straight away, as on Qt. A pass at the
  frame could not give that order (`features/items/inherited-at-once.feature`, checked
  on qml6).
- **Hot and cold fields.** Every item has marks and a record, each kept in a dense list,
  so every item pays for whatever they hold inline. The large fields that few items
  set are now held apart:
  - A Text's inputs sit in one box that its marks and its paint share.
  - The content and list map that a mark carries until the frame live in side maps.
  - The map of a `transform` list and a text field's paint are boxed.

  Marks shrink from 688 bytes to 56, the record from 576 to 328, and a paint from 288
  to 104. `sizes::an_items_marks_and_record_stay_small` pins these.

The budget's allocator now counts bytes as well as calls. `bytes_a_thousand_instances_keep`
pins what a thousand instances keep. The pins of ten land on the step where a list or map
grew, so they jump.

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets):

| | B28 | B29 inherit | B29 |
|---|---:|---:|---:|
| ListView delegate, heap allocations | 1305 (130.5) | 1285 (128.5) | 1299 (129.9); 1316 (131.6) with TEXT5 |
| ListView delegate, effects | 110 (11) | 90 (9) | 90 (9) |
| ListView delegate, values | 90 (9) | 80 (8) | 80 (8) |
| Wrapped delegate, heap allocations | 1298 (129.8) | 1268 (126.8) | 1282 (128.2); 1338 (133.8) with TEXT5 |
| Wrapped delegate, effects | 100 (10) | 80 (8) | 80 (8) |
| `createObject` item, heap allocations | 833 (83.3) | 786 (78.6) | 799 (79.9) |
| `createObject` item, effects | 90 (9) | 70 (7) | 70 (7) |
| Ten `createObject` items destroyed, graph nodes | 170 | 130 | 130 |

| Heap bytes kept per instance, over a thousand | B28 | B29 inherit | B29 |
|---|---:|---:|---:|
| ListView delegate, one-line text | 27 485 | 27 252 | 25 801; 25 946 with TEXT5 |
| ListView delegate, wrapped text | 29 527 | 29 293 | 27 842; 28 865 with TEXT5 |
| `createObject` item | 20 845 | 17 142 | 15 893 |

The columns were measured before TEXT5 landed. "With TEXT5" is the series rebased onto it,
counted on a clock only the frames move. On the machine's clock, a frame's incubation
budget was spent in wall time: under load a frame made fewer rows and the counts drifted
from run to run (bindings 50 to 54). The budget test now gives the same counts at a load
of 79 on 32 cores as on an idle machine.

The split adds one allocation per Text, for the shared box, and takes 1.45 kB off every
delegate. A DHAT run of the thousand one-line delegates shows where the rest goes:
- the reactive graph's node slot map: 6.8 kB per delegate;
- the per-object node lists (`ObjectHeader::set_node`): 2.9 kB;
- the marks and record lists: 1.4 kB and 1.2 kB, before the split;
- the object slot map and the render tree's slot map: about 1.0 kB each.

The graph's nodes are the next lever for memory.

| listview under callgrind (main thread, 20 s after 40 s), per delegate | B28 | B29 inherit | B29 |
|---|---:|---:|---:|
| Total ÷ text layouts, three runs | 346.8 k, 358.7 k, 352.8 k | 291.7 k, 348.3 k, 345.2 k | 324.9 k, 351.0 k, 329.4 k |
| Median | 352.8 k | 345.2 k | 329.4 k |
| `QuickScene::adopt`, inclusive | 43.2 k | 38.2 k | 38.1 k |
| `QuickScene::unadopt`, inclusive | 13.4 k | 12.5 k | 11.0 k |
| `inherit_effect`, inclusive | 5.7 k | | |
| `inherit` (adoption and hooks), inclusive | | | 4.0 k |
| `text_polish`, inclusive | 0.68 k | 0.68 k | 0.77 k |
| `Geom::read`, inclusive | 15.2 k | | 15.3 k |

Totals are noisy between runs: other lanes' builds were running, and how many idle
frames fall in the window varies. The inclusive costs are steady to within 1 %. qml6
does a whole delegate in ~200 k.

| Scroll, M instructions/s (`scr.sh`, back to back) | Ljós B28 | Ljós B29 | qml6 (same run) |
|---|---:|---:|---:|
| `lvstill.qml` (in-buffer) | 19.05 | 18.73 | 15.22 |
| listview | 180.86 | 176.10 | 102.87 |
| listview with `reuseItems: true` | 70.85 | 70.17 | 60.51 |

DECISIONs:

- Inheritance is passed at the hook and the reparent, not at the frame, so that a script
  sees it in Qt's order. An item takes what it inherits once, when it is adopted. Items
  that no scene shows inherit nothing, as before.
- The frame's geometry read stays as it is. `Geom::read` already batches seven numbers
  and three values into two engine calls. Of its ~7.4 k per call, about 5.2 k is the
  delegate's own `width: ListView.view.width` binding running on its first read, which
  any read would run. The read itself (access checks, the values, `faces_away`) is about
  1.5–2 k per item. A one-record read would save under 0.5 % of a delegate, so B28's
  `adopt` savings went to that binding and not to the read.
- The split is kept: bytes fall (1.45 kB per delegate, 1.25 kB per plain item) and
  callgrind does not rise (`unadopt` −1.5 k, `text_polish` +0.09 k).
- A Text's inputs are shared through one box between the marks and the paint. A pool
  for them could save the one allocation per Text, but it would need the paint to read
  the marks at the frame. That is for phase c.
- The content and list a mark carries are held in side maps keyed by item. A box per
  mark would allocate on every change, and a moving `Rotation` changes on every frame.
- A rectangle's paint (104 bytes) stays inline in the record. It is the common paint,
  and boxing it would allocate on every colour change.
- Phase c (B30, structure and text polish) is next. TEXT4's measure-on-read branch in
  `text_polish` is kept; B29 only changed how it stores the inputs.

### B30 (H2): an item's structure, `parent` and `focus` by change hooks, and a positioner and a ListView `remove` once (2026-10-26)

B30 is phase c of the delegate track's dirty-bit seam. It made four changes:

- **Structure by change hooks.** What an item shows of its children used to be one
  effect per item. Now the change hooks of the properties it read mark the item dirty,
  as do the engine's listeners for the end of an incubation and for a destruction. The
  scene reads the structure of a marked item at the frame.
- **`parent` and `focus` by change hooks.** Each item used to watch both, and a watch is
  an effect and a value node. Now their change hooks act on a write. They do nothing
  until `init_item` has run, because it settles both on completion.
- **A positioner lays out once on completion.** A Row or Column compares its inputs with
  the ones it last laid out. Its first polish after completion no longer lays out a
  second time, so the warning that came twice comes once
  (`features/layouts/positioner-anchors.feature`, untagged).
- **A ListView's `remove` runs once in the layout pass.** Rows that leave are gathered in
  the pass. Each run of the transition is told every row that left in the change, inside
  the layout pass and not after it (`features/views/list-remove-batch.feature`, untagged).

**Budgets, pinned exactly in `budget.rs`** (ten instances; per instance in brackets):

| | B29 | B30 |
|---|---:|---:|
| ListView delegate, heap allocations | 1260 (126.0) | 1209 (120.9) |
| ListView delegate, effects | 70 (7) | 30 (3) |
| ListView delegate, values | 80 (8) | 40 (4) |
| Wrapped delegate, heap allocations | 1281 (128.1) | 1232 (123.2) |
| Wrapped delegate, effects | 60 (6) | 20 (2) |
| Wrapped delegate, values | 60 (6) | 20 (2) |
| `createObject` item, heap allocations | 734 (73.4) | 674 (67.4) |
| `createObject` item, effects | 50 (5) | 10 (1) |
| `createObject` item, values | 40 (4) | 0 |
| Ten `createObject` items destroyed, graph nodes | 110 | 30 |

| Heap bytes kept per instance, over a thousand | B29 | B30 |
|---|---:|---:|
| ListView delegate, one-line text | 25 874 | 22 000 |
| ListView delegate, wrapped text | 25 123 | 24 972 |
| `createObject` item | 15 832 | 11 721 |

The B29 column is main at 30cec887. Sixteen bytes of each drop come from a1d09698, which
took the told polish effect out of every item's record.

What a one-line delegate still has as effects: the Rectangle's content effect, and the
Text's anchors and the Text's follow effect.

callgrind and the scroll run against qml6 were not run for B30. The data drive was at
98 % with other lanes building, and a profiling build plus a baseline build would have
needed about 20 GB. The budget and byte pins above are exact.

**positionViewAtEnd in calqlatr.** Each press, frame included, in a debug build over 200
digit presses and 50 operator presses:

| | Digit, allocations | Digit, time | Operator, allocations | Operator, time |
|---|---:|---:|---:|---:|
| `positionViewAtEnd()` | 31.6 | ~255 µs | 222.4 | ~1.35 ms |
| scroll by hand with `contentY` | 43 | ~360 µs | 214.8 | ~1.15 ms |

A digit press changes the last row in place, and `positionViewAtEnd` takes the path that
skips layout. An append lays out twice. The first pass places the pending change where the
view is, and the second places it with the view at the end.

DECISIONs:

- The Text's follow effect stays a tracked effect. It reads state of other objects that no
  hook on its own properties sees: the SelectionScope switch and the parent chain, the
  rich document's epoch and images, the system font scale, the anchors,
  `size_follows_implicit`, and `activeFocus` when the text has links. TEXT4's
  measure-on-read branch in `text_polish` is untouched.
- A Row or Column keeps the told polish effect from 43fa21fa. Its skip on completion
  compares inputs rather than counting polishes.
- The `remove` transition still runs one target at a time for each Transition. A
  `running` guard does this, as it does for `add`. Each run is told all of the change's
  targets. Removals whose op is lost to a reset or a move are finished one by one.
- The `parent` and `focus` hooks act only once `init_item` has run, which sets both on
  completion. The initialised test is that the anchor lines are set.
- `positionViewAtEnd` is left as it is. The first of an append's two passes takes the
  pending op and the targets of its add transition. A pass with the view already at the
  end would not take them. It is cheaper than scrolling by hand on a digit press, and
  0.2 ms dearer on an operator press.

### B12 (ilang): ListModel.append, and a calqlatr press profiled (2026-10-26)

**Method.** As in B11: instructions are user-space instructions of the whole process,
from start to exit. Milliseconds are the scripts' own `Date.now()` around the append loop,
taking the lowest of eight runs on a machine with a load of 40 to 100, so the instruction
counts are the ones to compare. "No view" is a Timer appending N `{ n: i }` rows to a
ListModel nothing shows. "Shown" is the same loop under a ListView of one-line Text rows.
"Before" is origin/main (aa4f4e1c), built the same way.

| | Before | After | qml6 |
|---|---:|---:|---:|
| append, no view (instructions a row, 0 → 10k) | 11.5k | 10.4k | ~3.5k |
| 10k appends, shown (M instructions, whole run) | 509.8 | 264.5 | 474 |
| 20k appends, shown (M instructions, whole run) | stopped at its slice | 389.8 | 553 |
| 10k appends, shown (ms, lowest) | 23 | 12 | 5 |
| 20k appends, shown (ms, lowest) | 61 | 22 | 10 |
| 100 appends, no view (lookups, allocations) | 200, 914 | 0, 714 | |
| calqlatr, 3000 presses (M instructions) | 955.9 | 953.2 | ~530 |

qml6's whole-run counts include frames it renders while it waits for the layout, so its
cost per row is the slope: 7.9k from 10k to 20k rows, against Ljós's 12.5k.

**Appends to a shown model were quadratic.** Every row added before a layout shifted the
indexes of all rows added since the last layout, in case it had landed before them. An
append lands after all of them. The pending change now keeps its highest index, and a row
added past that index visits none of them. The transitions are told the same indexes. qml6
tells the indexes of added rows in row order, whatever order the rows came in, and Ljós now
does too (`features/views/list-add-targets.feature`). A run of appends now costs the same
for each row, and 20,000 appends no longer reach the script's time slice.

**The append itself.** Of ~10k instructions an append, about 8k is the call from script:
- turning the object literal into the engine's value (~1.5k);
- reading its entries out of JS (~1.6k), a string for each key;
- the row, its count and the change notice (~2.4k);
- the host call around it.

Two costs are gone:
- A model nobody connected to no longer finds its row signals by name, two member lookups
  an append.
- A value of a kind the embedder defined, or a colour, is told from other objects by its
  prototype's address. Before, each object was asked for a hidden mark property.

One row is also held without a list of its own on its way in. The budget test
`hundred_list_model_appends` pins the result: no lookups and 7.14 allocations a row.

**calqlatr, one press.** Measured as 3000 presses minus none, with callgrind collecting from
the timers on: ~280k instructions a press, against qml6's ~177k. ~200k of it is the
ListView pass that `positionViewAtEnd()` runs after each press. Each pass lays all ~23
shown rows out again:
- `Fill::lay` reads every row's geometry back: `read_geo`, `read_sizes` and `read_inputs`
  are ~19k a pass.
- It writes every row's place: `item::write_placed`, ~11k a pass.
- `flickable::bounds` is read three times a pass, ~14k.
- One press in three makes a new row and lets an old one go: ~173k to make, of which the
  row's Text completing and measuring is ~55k, and ~40k to destroy, mostly reparenting.

Outside the pass, ~61k a press is the calculator's own script.

**Left** (DECISIONs; geometry, placing and text are H2's B28–B30 area):

- **A pass re-reads and re-places every shown row** when only the last row changed. A
  dirty bit per row's geometry would let it skip the rest. This is the geometry seam.
- **A new row's Text** costs ~55k to complete and measure. This is text polish.
- **Destroying a row** costs ~40k: parent changes and the scene forgetting it. This is
  structure.
- **`current::sync`** costs ~11k a pass: it reads the highlight's settings and the current
  item's geometry each time. Its geometry reads belong with the geometry seam.
- **An append still costs ~3× qml6's.** Most of it is crossing from script: the literal's
  keys are read as strings, and the object is kept for the call. Taking the entries during
  the call, without keeping the object, is the next step. It needs a change to the host
  interface.
- **The `targetIndexes` of several removals** keep the order the rows left in. qml6 moves
  the earlier ones' indexes and gives them in another order: "2,1,0" for `remove(4)`,
  `remove(0)`, `remove(1)` out of six rows. This is not pinned yet.

### B12b (ilang): what crossing from script to the host costs (2026-10-26)

**Method.** As in B12: user-space instructions of the whole process, perf counters, the
lowest of several runs. An append is (10,000 appends − none) / 10,000, `{ n: i }` rows to a
ListModel nothing shows. A calqlatr press is (3000 presses − none) / 3000 at 320×480. The
crossings were counted with callgrind over 300 presses and over 10k appends. "Before" is
the tree after B12.

| | Before | After | qml6 |
|---|---:|---:|---:|
| append, no view (instructions a row) | 10.32k | 6.98k | ~3.5k |
| calqlatr press (instructions) | 310.4k | 307.8k | ~177k |
| 100 appends, no view (allocations) | 714 | 702 | |

**Crossings in an append** (callgrind, instructions an append, before this work's last step):

| Crossing | Calls | Instructions |
|---|---:|---:|
| script calls a host method (`raw_method`, inclusive) | 1 | 5.19k |
| ↳ the object literal copied out of JS (`copy_at`) | 1 | 1.27k |
| ↳↳ its own property names, and each key as a string | 1, 1 | ~385, ~139 |
| ↳ the engine's side of the call (`EngineHost::call`) | 1 | 3.34k |
| ↳↳ the method itself (`invoke`, the row and the count) | 1 | 1.94k |
| ↳↳ the copy turned into an engine value | 1 | ~780 |
| the script's own loop, literal and frees (QuickJS) | | ~1.87k |

**Crossings in a calqlatr press** (instructions a press, inclusive):

| Crossing | Calls a press | Instructions |
|---|---:|---:|
| QML-declared functions run from script (`raw_direct`) | 0.6 | 153k |
| host methods (`raw_method`) | 6.9 | 136k |
| ↳ `positionViewAtEnd` | 0.5 | 128k |
| ↳ `ListModel.append`, `ListModel.get` | 0.2, 0.8 | 1.3k, 1.0k |
| property writes from script (`prop_set`) | 0.9 | 8.3k |
| property reads from script (`prop_get`, both paths) | 2.7 | 2.5k |

A press's crossings themselves are cheap. Its cost is in what `positionViewAtEnd` does:
a ListView pass, which is H2's area (B12's list).

**What changed.**
- A host method takes its arguments by value. It does not clone them.
- A call keeps which arguments are data, and which are copies, in two bitmasks. It used to
  keep them in a list. It converts arguments only when one holds a script value.
- An object given to `append`, `insert` or `set` is copied as it crosses. A date in it is
  copied, and a function makes no role. A row's object is a new copy each time it is read,
  as in qml6 (`features/models/listmodel-copies.feature`).
- An ordinary object is told by its class and its prototype. Before, the host asked each
  object for its prototype's kind. Keys that are valid UTF-8 are read without a lossy
  pass.
- Rows are built from an iterator of their entries. They no longer go through a list made
  for them. Rows added at the end extend the list instead of splicing it.
- The model's `count` is written internally: the owner check is skipped. A number written
  internally to a plain property skips the general write path.
- Model listeners with no one listening return at once.

**Why not 6k** (DECISION). The task's target was 6k instructions an append or less. About
1.9k of the 6.98k is QuickJS's own work: the loop, the literal and the call. Of what is
left, most goes on representing the value three times:
- the JS object;
- the host's map with a string for each key;
- the engine's map with an atom for each key.

That is two allocations a key and one a map. Reading the JS object straight into engine
values would remove them. It needs a visitor in `ljos_js` that the engine implements, and
it changes the host interface for every call. Building the engine's map one entry at a
time, instead of all at once, was tried. It was slower (7.12k), so it was reverted.

### MEM2: what MEM1 left, against qml6 (2026-10-26)

MEM1's "What is left", except the controls' own effects, which wait for the delegate track's
B27–B30. Measured as MEM1 measured: `ljos bench --rounds 3 --load 60` (grid clicks), desktop
profile, off-screen in the cua sandbox, "private" being `Pss_Anon + SwapPss` in MiB. "Base"
is origin/main before this batch, "after" is the batch; qml6's columns come from the run
beside each, so they differ between the two runs only by noise. The machine swapped through
both runs.

| App | Start | Round 1 | Round 2 | Round 3 |
|---|---:|---:|---:|---:|
| thermostat, base | 76.9 | 226.1 | 333.7 | 334.2 |
| thermostat, qml6 (base run) | 139.0 | 176.4 | 176.4 | 176.4 |
| thermostat, after | 73.1 | 224.3 | 222.2 | 222.2 |
| thermostat, qml6 (after run) | 152.1 | 169.4 | 169.4 | 169.4 |
| maroon, base | 53.8 | 58.9 | 63.8 | 64.8 |
| maroon, after | 53.7 | 58.3 | 63.5 | 64.6 |
| maroon, qml6 (after run) | 117.9 | 123.4 | 123.3 | 123.4 |

Thermostat's plateau is now 222 MiB against qml6's 169, 31 % above it (MEM1: about 35 %).
The base run's 334 is high against MEM1's 237 for nearly the same code: most of it was in
swap in that run (329 MiB of SwapPss), and one run of three rounds says little about a
plateau that close to the next. The start fell 3.8 MiB. Maroon stays at about half of qml6.

**maroon's creep was a ReferenceError.** MEM1 said maroon's object and node counts stayed
flat. They do not when the game is played: driven for 6 rounds with its inspector, the
objects went from 179 to 8014 and the graph from 2134 to 55 099 nodes. A base type's
`function die() { destroy(1000) }` called `destroy` by its bare name; no type lists
`destroy`, so the compiler left the name to the running program, which has no global of
that name and threw on every turn of the game's timer. The new piece was already made, the
old one was never taken away. `qml_front`'s `Names::has` now finds `destroy` on every object
(features/components/destroy-by-name.feature, on qml6 too).

| maroon, 6 rounds | Counted by owners, start → end | Objects | Graph nodes | Private |
|---|---:|---:|---:|---:|
| before the fix | 20.0 → 50.7 | 179 → 8014 | 2134 → 55 099 | 60 → 104 |
| after | 20.0 → 29.2 | 179 → 286 (213 to 722 between rounds) | 2134 → 2921 | 54 → 75 |

After the fix what the owners count stays between 29 and 30.6 MiB from round 2 on, and the
objects rise and fall with the pieces in play. What is left of the creep, about 1 MiB a round
(69 → 75 private over rounds 2–6), is outside anything Ljós keeps: Rust allocations no owner
counts stay under 0.6 MiB, while what the allocator keeps freed moves between 4.6 and
11.8 MiB and the C libraries' (llvmpipe, fonts) between 1.1 and 7.7 MiB, round by round.
The bench's grid clicks reach the game less than the creep's drive did, which is why its
base and after columns for maroon hardly differ.

**`Shortcut` objects only with a sequence.** An `Action` made its `Shortcut` as soon as
`shortcut` was anything but undefined, so `shortcut: ""` or `null` cost a Shortcut and the
nodes of its sequence, enablement and connection. It now makes one with the first sequence:
a text that is not blank, or a standard key. A `MenuItem` in a menu (declared, a delegate's,
an action's or a sub-menu's) makes no mnemonic Shortcut, since the menu takes the letters;
one taken out with `takeItem` makes it then. features/controls/action-shortcut.feature has
scenarios for a shortcut given later and a blank one given a sequence, and
`an_action_without_a_sequence_has_no_shortcut`, `a_row_of_a_menu_has_no_mnemonic_shortcut`
count the objects.

Checking `activatedAmbiguously` against qml6 found an older difference. qml6 hands each
press of a sequence several shortcuts share to the next of them in the order they
registered: a declared Shortcut registers as it completes (the last declared first), one
whose sequence changes registers again after all the others, and an action registers as it
is given its sequence. Ljós stepped from the last made backwards, which agreed only for
Shortcuts declared together, and an action told did nothing. Both now follow qml6
(action-shortcut.feature, features/input/shortcut-ambiguous.feature: three scenarios).

**Accessible objects when something asks.** Every control made its `Accessible` object as it
completed, to give it the role and name of its type. The defaults are now registered by type
(`register_accessible_defaults`) and given when the object is made: by QML that reads
`item.Accessible`, by the author's `Accessible.` lines, or by the a11y pass as it first
describes the item. The pass runs only while an assistive technology or a tool listens, and
walks every item it reaches when it starts, so a screen reader that connects late finds the
whole tree (`a_control_has_its_accessible_object_once_something_asks`,
`qml_reads_the_default_role_of_a_control`). This needed nothing of the scene's per-item
maps. Popups and a range slider's handles still make theirs as they are made; they are few.

**mediaplayer.** qml6 cannot run it (the corpus marks it: it needs `io.qt.filenameprovider`,
a C++ module), so there is no qml6 column. Ljós with its inspector over 3 rounds of grid
clicks: objects 543 → 545 (base) and 541 → 543 (after), graph nodes about 14 530 in both,
and what the owners count under 20 MiB. Its own memory is 540–640 MiB and moves with what
plays; 385–470 MiB of it is Rust allocation that no owner tells of, the decoded media.
Counting that is the media owner's next step, not this batch's.

**A frame-on-time flake.** features/window/frame-on-time.feature failed about one run in
three on a loaded machine: a message that found the window's frame due queued its Repaint
and asked the platform only at the turn's end, so the frame came in a turn of its own. The
Repaint now asks while the message is handled, as a deadline's wake does.

DECISIONs:

- A message wake that finds the frame due draws in that wake.
- `destroy` is a member of every object to the name lookup; no type table is changed.
- An action's Shortcut takes its place in the order of registration when the action gets
  its sequence, not when the Shortcut object is made.
- A11y defaults are given lazily for controls and text fields, not for popups and range
  slider handles.
- maroon's remaining creep is written down as the allocator's and the C libraries', not
  chased into glibc.
- mediaplayer is measured on Ljós alone.

### FRAMES: a workspace window, and a positioner shown row by row (2026-10-26)

A desktop app reported frames of 35–60 ms while typing, scrolling, hovering and sliding a
clipped side panel open (p50 0.47 ms, p95 56 ms, longest 104 ms). `bench/workspace.qml`
has the same shape:
- a sectioned sidebar ListView with reused button delegates and a Timer each;
- a transcript of wrapped StyledText;
- a TextArea composer;
- a side panel that animates 0→360 wide, with clip. It holds a tabs ListView whose model
  is empty while closed, and a Repeater of Loaders that load on first show. One page is a
  Column of 400 rows.

It also has a "Stream" button: the last message grows a word at a time, the sidebar rows
change, and a message is added now and then. `root.cycling = true` opens and closes the
panel every 0.5 s without a pointer:

```
frame_profile bench workspace.qml --size 1200x800 --warm 30 --frames 300 \
  --start 'root.cycling = true' --slow 2
```

Every interaction was driven by pointer and keys in the GPU sandbox (headless sway), with
the same input sent to Ljós and to qml6. Instructions are for the whole process. qml6's
frame times come from `qt.scenegraph.time.renderloop` and have whole-millisecond
resolution.

**Cause.** Opening the panel shows a hidden parent, so each of the Column's 400 rows
changes its inherited `visible` in turn. The Column's polish effect read every child's
`visible`, `width` and `height` again on each of those changes. That is 400 × 400 reads.
Under callgrind, `positioners::track` ran 6,360 times for 1.5 G over ten toggles, while
the layouts it led to cost 109 M.

**Fix.** A Row's or Column's effect is now *told* of a change and only marks the
positioner for polish. After the polish lays it out, the effect reads its inputs once
more, which also picks up children added or removed. Layouts keep their effect, because
their own dirty flag is set in `read_inputs`. In a Rust test, showing and hiding a Column
records these reads:

| Rows | Before | After |
|---|---:|---:|
| 100 | 22,504 | 1,444 |
| 400 | 330,004 | 5,644 |

The test is `showing_a_column_reads_its_rows_once_not_once_per_row` in
`ljos_quick/tests/scene.rs`.

| Panel toggle, 8 clicks in 8 s, GPU sandbox | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| Instructions | 3,153–3,190 M | 1,775–1,846 M | 1,703–2,224 M |
| Frame p50 | 1.9–3.5 ms | 2.1–2.4 ms | 0 ms gui, 0 ms render |
| Frame p95 | 17.4–19.3 ms | 6.8–7.4 ms | 1 ms gui, 2 ms render |
| Longest | 23–38 ms | 20–42 ms (first open) | 698 ms gui, 1,067 ms render (first open) |

The longest frame on both is the first open, which makes the 400-row page. In
frame_profile, the extra cost of ten toggles fell from 2,648 M to 1,095 M, and a toggle's
frames fell from about 16.5 ms to about 9 ms. The machine's load was 58–92, so
wall-clock times are noisy and the instruction counts are the measure.

| Interaction, after (8 s) | Ljós | qml6 | Ljós frames p50 / p95 / longest |
|---|---:|---:|---|
| Scroll the transcript | 793 M | 738 M | 0.44 / 1.47 / 1.87 ms |
| Scroll the sidebar | 852 M | 659 M | 0.46 / 1.45 / 4.26 ms |
| Hover sidebar rows | 139 M | 125 M | 0.54 / 2.81 / 9.07 ms |
| Type 50 characters | 34–67 M | 102 M | 1.24 / 9.18 / 9.18 ms |
| Stream updates | 526 M | 348 M | 0.10 / 1.24 / 6.10 ms (one 19.6 ms) |

Left:
- The bench has 1,600 objects and 24k graph nodes, against the app's 30k objects and 394k
  nodes. It has no host message source, so the app's critical lane (the inbox drain) is not
  reproduced here.
- While the panel's width animates, each frame spends its time in these places:
  - `views::pass::run`, 295 M over the run (the transcript and tabs views lay out again as
    the width changes);
  - the wrapped StyledText laid out again, 269 M;
  - encode, 226 M;
  - the Behavior's `start_driven` → `views::pass::bump`, 186 M;
  - `flickable::put` and `views::current::normalise`, 174 M.
- Inherited `visible` (B29's `inherit_effect`) pushes or removes one override per
  descendant, each in its own propagation. This is what told the Column 400 times, and
  any other reader of the rows' `visible` still sees each change. Making that one
  propagation is in the inherited-state seam.

### FRAMES2: a view laid out once a frame while a sibling's width animates (2026-10-26)

**Cause.** While the panel slid open, the transcript ListView, which takes what the panel
leaves, was laid out three or four times a frame:
- once as the animation stepped the panel's width;
- again as each text in its rows wrapped at the new width;
- again as each row's Column took its new height.

The tabs view and the sidebar were not laid out at all, because nothing about them
changed. The 186 M under `start_driven` → `views::pass::bump` is the tabs model being
swapped once per toggle, not once per frame; qml6 also makes those delegates again.

**Fix.** While the scene steps its animations or polishes, a view that a change reaches
now waits until the polish ends, then lays its rows out once. Three cases are still laid
out at once:
- a pass that something asked for, such as rows that came into sight or a new model;
- a view's first pass;
- a view that `positionViewAtEnd` or `positionViewAtBeginning` keeps at an edge while its
  rows take their length. Bindings over `indexAt` read the rows at the view's new place,
  so those rows must already be there (`list-settling-rows.feature`).

In frame_profile, the transcript's passes over 80 animated frames fell from 147 to 81. The
pin is `a_view_a_moving_width_reaches_lays_its_rows_out_once_a_frame` in
`ljos_quick/tests/views.rs`, which records one pass per frame and none for the tabs. Before
the fix, one frame took 28 passes.

| Panel toggle, 8 clicks in 8 s, GPU sandbox | Ljós before | Ljós after | qml6 |
|---|---:|---:|---:|
| Instructions | 1,775–1,846 M | 1,700–1,809 M | 1,156–2,218 M |
| Frame p50 | 2.1–2.4 ms | 1.9–2.7 ms | 0 ms gui |
| Frame p95 | 6.8–7.4 ms | 5.25–5.26 ms | 1 ms render |
| Longest | 20–42 ms (first open) | 18–22 ms (first open) | first open |

The text still wraps again at each new width, as it does in qml6, so the instruction count
moved less than the pass count did. The frames' tail is shorter.

| Interaction (8 s) | Ljós FRAMES | Ljós FRAMES2 | qml6 (this run) |
|---|---:|---:|---:|
| Scroll the transcript | 793 M | 771 M | 545 M |
| Scroll the sidebar | 852 M | 863 M | 643 M |
| Hover sidebar rows | 139 M | 132 M | 94 M |
| Type 50 characters | 34–67 M | 34 M | 19 M |
| Stream updates | 526 M | 515 M | 348 M |

qml6's counts move by up to 2× between runs in this sandbox (toggle 1,156 M and 2,218 M,
typing 19 M and 102 M). Ljós stayed within its own run-to-run spread on every interaction.

Left:
- `flickable::put` and `views::current::normalise` still cost per row, not per changed
  row.
- Wrapped StyledText laid out again at each width, and encode.

### MEDIAMEM: what mediaplayer held that no owner counted (2026-10-26)

MEM2 left 385–470 MiB of mediaplayer's memory as Rust allocation no owner told of. It was
FilmCraft's frame cache. A FilmCraft source keeps a `GopCache` built for an editor that
scrubs back and forth: 384 MiB a source, at least 64 frames, and the pool trims idle caches
only past 1 GiB. Every frame a player was given stayed there. Pictures that left the cache
went to the pool's shelf for reuse. Asked while mediaplayer's 1080p60 clip played, the cache
held 382 MiB and the shelf 20–41 MiB.

FilmCraft is used unmodified (`third_party/`), so the bound is in Ljós. An MP4 or MOV's video
is now read in order by a reader of the player's own (`ljos_media::mp4_video`), built on
`filmcraft-isobmff` and the source's decoder. It picks samples the way the source does: the
edit list, the frame grid, open-GOP leading pictures and sync samples. It keeps:
- the pictures decoded past the last one asked for;
- that last one, so a repeated time costs nothing.

That is four pictures at most for this clip, about 12 MiB at 1080p. A dropped picture is
freed, not kept on a shelf.

`a_reader_gives_the_frames_the_source_gives` checks that its pictures are the source's,
pixel for pixel, at the same times:
- a clip with B-frames and no edit list;
- a rotated one with an edit list;
- seeks forward, back and to the same frame.

`a_reader_playing_holds_a_few_pictures` checks the bound.

The frame an output shows stays on the CPU, with its turned copy for 90 and 270. A video
frame cannot be made again from fewer bytes, and a turn or a lost surface paints it again.
It is now counted too.

**The decoder.** FilmCraft's H.264 decoder threads by frame, with a thread per core (up
to 16). Each thread holds pictures in flight and hands them out in bursts. With it the reader
held 15 pictures and the process 320 MB. The reader now starts on FilmCraft's two-thread
background decoder: 72 MB, 3.8 ms a frame on average, 11.8 ms at p99 and 15.6 ms at most,
against 16.7 ms a frame at 60 fps. It watches the average decode time per sample. When that
passes half a frame's time, it moves to the full decoder at the next IDR picture, so a clip
too heavy for two threads still plays
(`a_reader_too_slow_for_its_clip_takes_a_full_decoder_at_a_refresh`).

**Counted.** `Engine::add_memory_owner` lets a crate built on the engine tell
`app_metrics`' `memory` what it holds. `ljos_media` tells of `media` ("decoded media",
frames), which adds up:
- the reader's pictures;
- the shown frames;
- what FilmCraft's caches still hold for the containers the reader does not read;
- the pool's shelf.

It is added to `counted_bytes` and to what is attributed, so it leaves "allocated by Rust
code, no owner tells of it". The scenario is in features/tooling/mcp/memory.feature.

**Measured.** Release build, off-screen in the cua sandbox (llvmpipe), 32 cores, with the
machine's load average beside each run. "Private" is `Pss_Anon + SwapPss`. "Before" is the
same build with the reader turned off, so the player goes through the FilmCraft source as on
origin/main.

`play.qml` is a 960×540 window with a `MediaPlayer`, looping, on a `VideoOutput`, playing
Qt_LogoMergeEffect.mp4 (1080p60 H.264, 19 s, the clip mediaplayer plays). It counts
`videoSink.videoFrameChanged`:
- CPU is taken over seconds 5–25, playing;
- memory is read at 25 s;
- the player pauses at 25 s, and CPU is taken again over seconds 30–40.

Two runs each, alternating:

| play.qml | Private MiB | RSS MiB | CPU playing | CPU paused | Frames s5–s25 (1200 due) | Load |
|---|---:|---:|---:|---:|---:|---|
| qml6 | 168, 168 | 108, 91 | 51 %, 43 % | 0.0 %, 0.0 % | 1260, 1260 | 71→77, 78→85 |
| Ljós before | 600, 597 | 599, 634 | 270 %, 273 % | 0.0 %, 0.0 % | 1181, 1190 | 76→78, 86→90 |
| Ljós after | 122, 124 | 121, 188 | 265 %, 263 % | 0.1 %, 0.0 % | 1182, 1190 | 77→76, 85→86 |

qml6's sink reports some frames twice: it reports 94 in the second the clip loops. Its
count is not a count of distinct frames. Ljós's sink reports each frame shown once. Ljós
showed the same frames before and after, 10–19 short of 1200 at a load of 76–90. The bound
cost no frames.

mediaplayer itself, with `--example mediaplayer` for its file-name provider, measured over
seconds 10–30 of its clip and read at 30 s. qml6 cannot run it:

| mediaplayer | Private MiB | RSS MiB | CPU playing | Load |
|---|---:|---:|---:|---|
| before | 560, 562 | 273, 541 | 160 %, 152 % | 74→65, 60→50 |
| after | 99, 99 | 80, 86 | 157 %, 157 % | 79→74, 64→60 |

Its private memory fell from 560 MiB to 99 MiB. qml6 holds 168 MiB for the bare player.
(RSS reads low in the before runs because most of the cache was in swap.)

**What is left.** Playing costs far more CPU than qml6's: 263–273 % against 43–51 % for
play.qml. The memory work did not change that. Software decoding takes about 23 % of a core
(3.8 ms × 60). The rest has not been measured: converting each 1080p picture to RGBA, and
uploading and scaling it on llvmpipe. qml6 converts YUV in its shader and decodes with
FFmpeg. A YUV upload, converted on the GPU, is the next step for that.

Changes FilmCraft would need, for its own repository:
- a playback mode for its sources, with a small `GopCache` budget and no 64-frame floor, so
  MKV, WebM, Ogg and MPEG-TS bound the same way;
- a trim of the pool's shelf.

DECISIONs:

- The bound is a reader of Ljós's own for MP4 and MOV. FilmCraft's sources are not changed.
  Other containers go through the source, counted under `media`.
- The reader starts on the two-thread decoder and moves to the full one at an IDR picture
  when decoding takes more than half a frame's time. A move never happens mid-GOP.
- A track with a clean aperture (`clap`) goes through the source, which crops it.
- The frame an output shows is kept on the CPU and counted. It is not let go after upload.
- One owner, `media`, for the multimedia types. It is not split per player: asking is a sum
  of running counts, and nothing is kept per object for it.
- Before and after come from one build with the reader switched off for "before". The switch
  was removed before commit.
