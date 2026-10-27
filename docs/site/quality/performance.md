---
title: Performance and benchmarks
description: Understand how Ljós is benchmarked against Qt with ljos bench, what the startup, memory, and CPU numbers mean, and how to keep your application at 0 % CPU when idle.
section: quality
order: 7
---

# Performance and benchmarks

This page explains how Ljós is measured side by side against Qt (`qml6`) using `ljos bench`, what the reported memory, startup, and frame numbers mean, and how to write QML that stays within the 16.7 ms frame budget and uses 0 % CPU when idle.

## Running `ljos bench`

The benchmark suite runs the same QML applications—both synthetic workloads in `bench/*.qml` and examples from the Qt documentation corpus—on Ljós (`ljos run`) and on Qt (`qml6`). Both runtimes run inside a transient `systemd-run --user --scope` cgroup on a headless Wayland compositor (`sway` in `scripts/cua-sandbox.sh`) with `QT_QUICK_CONTROLS_STYLE=Basic`.

```sh
# Run every benchmark app on both runtimes (ljos and qt) and both profiles (desktop and pi-zero)
mise run bench

# Run specific apps only
mise run bench rectangles samegame

# Pass options after the app IDs directly to ljos bench
mise run bench calqlatr --runtime ljos

# Build and measure the single-module distribution profile (takes ~15 minutes to link)
BENCH_PROFILE=dist mise run bench rectangles

# Record hot functions under callgrind to order crates/ljos_cli/link/hot.txt
mise run bench:hot

# Verify that ljos cross-compiles for aarch64-unknown-linux-gnu
mise run bench:cross
```

By default, `mise run bench` passes `--warm-up --repeat 3 --bisect pi-zero` (configurable via `BENCH_DEFAULTS`) and writes its output to `target/bench/` (or `$BENCH_DIR`):

- `report.json`: raw measurements and thread CPU breakdowns (`thread_cpu_ms`) for every repeat.
- `report.md`: median summary tables.
- `work/`: per-application logs and outputs.

For long runs, pass `--append` to add results incrementally to an existing `report.json`:

```sh
for id in rectangles form listview; do
  for rt in ljos qt; do mise run bench "$id" --runtime "$rt" --append; done
done
```

## Benchmark profiles and metrics

Every measurement is read from outside the application using cgroup accounting, `/proc`, and the Wayland compositor's screencopy interface. If a required cgroup controller (`memory` or `cpu`) is not delegated, `ljos bench` exits with status `2` rather than reporting unthrottled numbers.

### Hardware profiles

| Profile | Memory limit | Swap | CPU limit |
|---|---|---|---|
| `desktop` | Unlimited | Host default | Unlimited |
| `pi-zero` | `256M` (`--memory-max`) | None | `4` CPUs on distinct physical cores (`--cpus`), `CPUQuota = cpus × 100 / 20` (`--cpu-factor 20`), counted every `10 ms` |

The `pi-zero` profile models a Raspberry Pi Zero 2 W (four Cortex-A53 cores at 1 GHz and 512 MB of RAM, with roughly half available to the application after the OS and compositor). Counting the quota over a 10 ms period stalls a throttled app for a few milliseconds at a time, like a slow core, rather than for most of a 100 ms window.

### What each metric means

| Metric | How it is measured |
|---|---|
| **Time to first frame** | Wall time from process start until `sway` maps the window. |
| **CPU time to first frame** | The cgroup's `cpu.stat` usage when the first frame is mapped. |
| **Peak memory while starting** | `memory.peak` up to the first frame. |
| **Settled memory / PSS** | `memory.current`, anonymous private PSS, and total PSS from `smaps_rollup` after `--settle` seconds (default `3 s`). |
| **Idle CPU and wakeups** | CPU usage and context switches over `--idle` seconds (default `10 s`) with no input. |
| **Frames while idle** | Compositor screencopy frames during the idle window whose pixels differ from the previous frame (non-zero only for apps that animate). Duplicate frames sent without pixel changes are reported separately as frames sent. |
| **CPU and frames while driven** | Measured over `--load` seconds (default `10 s`) of virtual pointer input (`--gesture`: `click`, `drag`, or `scroll`), after waiting up to `5 s` for the window's pixels to stay still for `300 ms`. |
| **Mapped install** | Total on-disk size of files mapped by the process (runtime binary, shared libraries, plugins, fonts). |
| **Least memory** | Smallest `MemoryMax` (found by bisection under `pi-zero`) at which the app shows its first frame and stays alive 1 second later without an `(OOM)` kill. |

Because Linux cgroups charge shared pages to whichever cgroup faulted them in first, `--warm-up` loads shared libraries into the page cache before measurement starts. Treat **least memory** as what the application needs on top of a system that already has its runtime libraries loaded; on a single-app embedded board, add the **mapped install** size.

## Where startup and frame time go

### First-frame breakdown

Measured on `bench/rectangles.qml` (release build on the sandbox's headless `sway` with pixman), wall-clock time from process start breaks down as follows:

| Phase | Wall time |
|---|---:|
| QML parsed and compiled | ~5 ms |
| System fonts (`fontconfig`) | 20–70 ms |
| Application built (engine, root object) | ~20 ms |
| Graphics instance (Vulkan + GL in `wgpu` / EGL alone) | 300–700 ms / 100–265 ms |
| First paint (`llvmpipe`) | 30–100 ms |

Graphics driver initialisation dominates startup. With the default EGL painter (`egui_glow`), Ljós starts only the EGL display on a background thread as soon as the Wayland display opens, without creating a `wgpu` instance. On a machine where an unused NVIDIA Vulkan driver was installed alongside a Radeon GPU, restricting `VK_DRIVER_FILES` to the Radeon driver brought Ljós's time to first frame from about **550 ms** down to **85–115 ms** (compared with **110–205 ms** on Qt). Since 2026-10-18, Ljós automatically skips NVIDIA's Vulkan ICD and EGL vendor when the NVIDIA kernel module is not loaded.

### Comparison run against Qt 6.11.2 (2026-10-15)

Measured on 2026-10-15 on an AMD Ryzen AI Max+ 395 (32 CPUs, 63 GiB RAM, Linux 7.2, machine load average 3.5 to 143) across 25 applications (the 5 `bench/*.qml` apps and 20 runnable corpus examples), comparing Ljós (`419a04c`, `--release` with thin LTO; `animations` at `ecda6d6`) against Qt 6.11.2. Values below `×1.00` mean Ljós used less time or memory than Qt:

| Geometric mean (`Ljós / Qt`) | `desktop` | `pi-zero` |
|---|---:|---:|
| Time to first frame | `×2.05` | `×1.24` |
| CPU time to first frame | `×1.27` | `×1.16` |
| Peak memory while starting | `×0.67` | `×0.61` |
| Settled memory | `×0.65` | `×0.61` |
| Settled PSS | `×0.87` | `×0.86` |

- **Least memory (`pi-zero`):** Ljós needed **54–58 MiB** for small examples and **68–101 MiB** for larger ones, compared with **92–123 MiB** for Qt. The one exception was `rssnews`, which decoded feed images at full resolution and required **271 MiB** on Ljós versus **135 MiB** on Qt (both were OOM-killed under the 256 MiB limit).
- **Idle CPU:** **0 %** on both Ljós and Qt for all applications that do not animate.

### Drawing only what changed (2026-10-16)

On 2026-10-16, profiling `bench/animations.qml` on software rasterisation (`llvmpipe` over `wgpu` GL) showed that building the frame took `0.1–0.5 ms`, tessellating `0.02 ms`, and submitting `1.1 ms`, while `llvmpipe` raster threads used ~80 % of a core painting three full-window quads (clear, backdrop, and root `Rectangle`). Skipping presentation when nothing changed, omitting the redundant backdrop clear, and promoting an opaque root rectangle to the clear colour produced the following before → after results (measured on 2026-10-16, load average 37–68):

| App | Idle CPU, `desktop` (%) | Frames/s, `desktop` | CPU clicked, `desktop` (%) | Frames/s, `pi-zero` |
|---|---|---|---|---|
| `animations` | `86.7 → 43.9` (Qt `21.8`) | `61.9 → 59.9` (Qt `62.1`) | `87.5 → 44.9` (Qt `23.1`) | `20.7 → 45.5` (Qt `61.8`) |
| `listview` | `72.3 → 44.0` (Qt `23.2`) | `62.2 → 59.8` (Qt `62.0`) | `72.4 → 49.2` (Qt `24.1`) | `16.8 → 31.2` (Qt `62.1`) |
| `clocks` | `92.0 → 50.7` (Qt `21.2`) | `56.0 → 60.6` (Qt `60.2`) | `96.5 → 53.2` (Qt `20.7`) | `18.0 → 27.4` (Qt `60.2`) |
| `maroon` | `61.5 → 62.5` (Qt `58.3`) | `62.2 → 62.0` (Qt `62.2`) | `68.7 → 73.9` (Qt `69.0`) | `30.8 → 35.4` (Qt `29.1`) |
| `samegame` | `107 → 58.4` (Qt `28.2`) | `53.6 → 45.2` (Qt `61.7`) | `161 → 144` (Qt `145`) | `12.4 → 18.5` (Qt `59.2`) |
| `rectangles` | `0 → 0` (Qt `0`) | `0 → 0` (Qt `0`) | `37.1 → 0.98` (Qt `0.13`) | — |
| `calqlatr` | `0 → 0` (Qt `0`) | `0 → 0` (Qt `0`) | `9.83 → 1.21` (Qt `1.00`) | — |

To eliminate the remaining ~27 % fixed per-frame overhead of `wgpu`'s offscreen GL copy, Ljós introduced a direct EGL painter (`egui_glow`, 2026-10-16) that draws directly into the window buffer, turns opaque axis-aligned rectangles into scissored clears, and uses EGL buffer age to redraw only damaged regions (in `bench/animations.qml`, 200 moving bars touch only 11,500 of the window's 1.92 million pixels per frame). <!-- check: benchmark table numbers for the egui_glow partial-damage painter after the truncated line in docs/BENCHMARKS.md -->

## Keeping your application fast and at 0 % CPU when idle

Ljós schedules frames only on demand—when state becomes dirty, or when an animation, timer, or input event fires—and pulls render-only bindings lazily. Follow these rules in your QML code to keep idle CPU at 0 % and active animations under the **16.7 ms** frame budget (`limits::motion::FRAME_BUDGET_MS`):

1. **Use declarative animations instead of `Timer` (`motion-timer-driven`):** Never step `x`, `y`, or `opacity` from a 16 ms repeating `Timer`. A repeating timer wakes the CPU on every tick, bypasses the animation clock, and ignores reduced motion.
2. **Bind `running` on infinite animations (`motion-endless`):** Any `SequentialAnimation` or `NumberAnimation` with `loops: Animation.Infinite` must bind `running` to the state it represents (such as `running: syncing && visible`) so the engine sleeps at 0 % CPU when the state ends.
3. **Hide off-screen panes with `visible: false` (`a11y-hidden-exposed`):** Setting `opacity: 0` leaves an item in the accessibility tree. Binding `visible: opacity > 0` removes hidden items from both the accessibility tree and rendering work.
4. **Animate compositor-cheap properties (`motion-layout-property` and `motion-dropped-frames`):** Animate `x`, `y`, `scale`, `rotation`, and `opacity`. Never animate `width`, `height`, `implicitHeight`, or anchor margins, which relayout the scene on every frame.
5. **Virtualise lists with `ListView`:** Use `ListView` rather than `Repeater` inside a `Column` so delegates, text layouts, glyph runs, and textures exist only near the viewport.

```qml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 360
    height: 220
    visible: true
    title: qsTr("Zero idle CPU")

    property bool syncing: false
    property bool drawerOpen: false

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        RowLayout {
            spacing: 8

            Rectangle {
                width: 10
                height: 10
                radius: 5
                color: palette.highlight

                SequentialAnimation on opacity {
                    loops: Animation.Infinite
                    running: syncing
                    NumberAnimation {
                        to: 0.3
                        duration: 600
                        easing.type: Easing.InOutSine
                    }
                    NumberAnimation {
                        to: 1.0
                        duration: 600
                        easing.type: Easing.InOutSine
                    }
                }
            }

            Label {
                text: syncing ? qsTr("Syncing changes…") : qsTr("Up to date")
                Accessible.role: Accessible.StatusBar
                Accessible.name: text
            }
        }

        Pane {
            Layout.fillWidth: true
            opacity: drawerOpen ? 1.0 : 0.0
            visible: opacity > 0

            Behavior on opacity {
                NumberAnimation {
                    duration: 150
                    easing.type: Easing.OutCubic
                }
            }

            Label {
                width: parent.width
                text: qsTr("Does no paint or binding work while hidden.")
                wrapMode: Text.Wrap
            }
        }
    }
}
```

### Profiling Rust heap allocations

To inspect where Rust heap memory goes inside Ljós, build `ljos_cli` with the `dhat-heap` feature and summarise the output by subsystem:

```sh
cargo build --release -p ljos_cli --features dhat-heap
LJOS_DHAT_FILE=heap.json LJOS_DHAT_SECONDS=8 target/release/ljos run app.qml
scripts/dhat-subsystems.py heap.json 25
```

## See also

- [Goals and principles](principles.md)
- [Design guardrails](design-guardrails.md)
- [Motion and reduced motion](motion.md)
- [Layout, content, and typography](layout-and-typography.md)
- [First app](../start/first-app.md)
