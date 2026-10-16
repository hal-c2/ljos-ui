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

- Ljós uses wgpu's GL backend (llvmpipe on a headless compositor).
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
| CPU and frames while clicked | over `--load` seconds (10) of clicks on a grid over the window (wlr virtual pointer) |
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
window waits only for what is left of it. One instance per backend, each on its own thread, would
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

None of that can be changed from Ljós. A GL painter that draws straight into the window, or a
Vulkan software driver, would avoid the copy.
