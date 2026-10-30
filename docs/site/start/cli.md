---
title: Command-Line Reference
description: Reference for every subcommand and option of the ljos command-line tool.
section: start
order: 4
---

# Command-Line Reference

This page describes every subcommand of the `ljos` command-line tool, what it is for, its main options and how to invoke it.

```sh
ljos <COMMAND>
```

## `ljos run`

Opens a window on a QML file. With `--watch`, every save to the project's QML, JavaScript, `qmldir`, `CMakeLists.txt` or image files is applied to the running application in place.

```sh
ljos run [OPTIONS] <FILE> [-- <ARGS>...]
```

Arguments placed after `--` are passed to the QML application in `Application.arguments` (with the program and entry file first, matching `qml6`). Passing `-style NAME` after `--` selects the `QtQuick.Controls` style before `QT_QUICK_CONTROLS_STYLE` and the configuration file.

### Important options

- `--project <DIR>`: Project root for module discovery (defaults to the directory of `<FILE>`).
- `--watch`: Watch the project directory and hot-reload on changes.
- `--size <WxH>`: Initial window size in logical pixels, such as `1280x800`. Without `--size`, a root item opens at its own width and height (`800` by `600` for any side left at `0`).
- `--mcp[=<CAPS>]`: Start the live MCP inspector with a comma-separated list of capabilities (`inspect`, `input`, `mutate`, `eval`). Passing `--mcp` alone enables `inspect`. Writes the listener address and bearer token to `target/ljos/mcp-<pid>.json` (mode `0600`) and removes it on exit.
- `--mcp-redact`: Redact all editable text fields in inspector snapshots, properties and screenshots, and refuse `app_eval`, methods and signal handlers.
- `--mcp-bind <ADDR>`: Bind the MCP HTTP listener to `<ip>:<port>` instead of `127.0.0.1:0`. Non-loopback addresses require `--mcp-insecure-lan`.
- `--mcp-socket <PATH>`: Listen on a Unix domain socket (created mode `0600`, maximum 107 bytes on Linux and 103 bytes on macOS) instead of TCP.
- `--mcp-rate <N>`: Answer a client up to `N` calls per second (1 to 1000) instead of 20. For a program that starts the application and drives it as part of its own interface. Grants no capability; the token file states the rate as `calls_per_second`.
- `--top`: Show the `top` overlay over the window with the time of the last frame by lane and by owner.
- `--example <ID>`: Look `<ID>` up in `corpus/examples.json` and use its imports.

### Example

```sh
ljos run --watch --mcp=inspect,input,mutate Main.qml -- -style Basic
```

## `ljos check`

Compiles QML projects without opening a window and reports compiler errors, warnings, accessibility lints, internationalisation lints and design rule findings. Every diagnostic line ends with its rule code in brackets (such as `[invalid-assignment]` or `[a11y-unnamed-interactive]`).

```sh
ljos check [OPTIONS] <DIRS>...
```

### Important options

- `--tree`: Print the parsed syntax tree of each file.
- `--stats`: Print expression tier statistics (how many bindings run in tier 0, the tier-1 binding VM or JavaScript) and the top fallback patterns.
- `--top <TOP>`: Number of fallback patterns to show with `--stats` (default: `25`).
- `--summary`: Print one summary line per directory instead of per-file diagnostics.
- `--quiet`: Print only totals and errors.
- `--no-lint`: Skip accessibility and internationalisation lints.
- `--port`: Report Qt Quick types and members used by the project that Ljós does not yet support, grouped by module and ordered by frequency.
- `--json`: Emit the `--port` report as JSON.
- `--deny-missing`: Exit with status `1` if `--port` finds any unsupported or dropped APIs.
- `--ledger <FILE>`: Path to `ledger/qtquick.toml` (auto-detected from the working directory by default).
- `--limit <N>`: Stop after `<N>` QML files.

### Example

```sh
ljos check --stats .
```

```text
Main.qml:3:37: error: Invalid property assignment: unknown enumeration [invalid-assignment]
1 files, 1 errors, 0 warnings
ljos: 1 errors
```

## `ljos fmt`

Formats QML files in place to the canonical layout: one member per line, four spaces per indentation level, preserving comments and JavaScript expressions. A file with a syntax error is never modified. Pass `-` to format from standard input to standard output.

```sh
ljos fmt [OPTIONS] <PATHS>...
```

### Important options

- `--check`: Do not write files; exit with status `1` if any file is not formatted.
- `--diff`: Print a unified diff of the changes without writing files.

### Example

```sh
ljos fmt Main.qml
```

```text
formatted Main.qml
1 files, 1 formatted
```

## `ljos test`

Runs `TestCase` items in `tst_*.qml` files headlessly on a virtual clock and prints the report in the same format as Qt's `qmltestrunner`. The exit status is the number of failed tests (clamped to `254`).

```sh
ljos test [OPTIONS] [PATHS]...
```

Each path may be a `tst_*.qml` file or a directory (searched recursively for `tst_*.qml` in path order; defaults to `.`).

### Important options

- `--import <DIR>` (or `-import <DIR>`): Additional QML module import directory (may be passed more than once).
- `--filter <NAME>`: Only run tests matching `Case::function`, `Case::function:tag`, a case name or a function name.
- `--json <FILE>`: Write a structured JSON report to `<FILE>`.

### Example

```sh
ljos test tst_math.qml
```

```text
PASS   : Math::initTestCase()
PASS   : Math::test_add()
PASS   : Math::cleanupTestCase()
Totals: 3 passed, 0 failed, 0 skipped
```

## `ljos render`

Draws a QML project to a PNG file headlessly without opening a window, after advancing a virtual clock by a settle duration.

```sh
ljos render [OPTIONS] --out <OUT> <DIR>
```

### Important options

- `--out <OUT>`: Output PNG path (required).
- `--entry <ENTRY>`: Entry QML file relative to `<DIR>` (default: `Main.qml`).
- `--size <WxH>`: Render size in logical pixels. Without `--size`, uses the root item's size (`800x600` for any side left at `0`).
- `--settle <MS>`: Virtual milliseconds to advance before capturing (default: `1500`).
- `--now <RFC3339>`: Pin the wall clock that JavaScript `Date` sees (for example, `2026-03-30T14:00:00Z`), which then advances with `--settle`.
- `--system-fonts`: Load host system fonts. Without this flag, only the bundled font is used so renders are deterministic across machines.
- `--network`: Allow `http:` and `https:` fetches. Without this flag, network requests fail immediately as offline.
- `--example <ID>`: Look `<ID>` up in `corpus/examples.json` and use its entry, imports and size.

### Example

```sh
ljos render . --entry Main.qml --settle 500 --now 2026-03-30T14:00:00Z --out shot.png
```

## `ljos shaders`

Compiles GLSL 440 vertex and fragment shaders referenced by `ShaderEffect` items in QML files (or standalone `.vert` and `.frag` files) into pre-compiled `.ljsh` files beside them, so a shipped application does not need to compile shaders at run time.

```sh
ljos shaders [OPTIONS] <PATHS>...
```

### Important options

- `--check`: Verify that an up-to-date `.ljsh` file already exists beside every shader without writing any files; exits with status `1` if any `.ljsh` is missing or stale.

### Example

```sh
ljos shaders --check .
```

## `ljos i18n`

Extracts translatable strings (`qsTr`, `qsTranslate`, `QT_TR_NOOP` and related calls) from a QML directory tree and merges them into a Qt Linguist `.ts` catalogue file. Existing translations are kept, new strings are added as `unfinished`, and strings no longer present in the source are marked `vanished`.

```sh
ljos i18n extract --out <OUT> <DIRS>...
```

### Important options

- `-o, --out <OUT>`: Output `.ts` file path (created if it does not exist, updated in place if it does).
- `-l, --language <LANGUAGE>`: Target language code (such as `is_IS` or `de_DE`) written to `<TS language="...">`.

### Example

```sh
ljos i18n extract --language is_IS --out i18n/app_is.ts src/
```

## `ljos mcp`

Serves the authoring Model Context Protocol (MCP) server over standard input and output. Used by coding agents to check, lint, format and query QML projects without running a GUI window.

```sh
ljos mcp [OPTIONS]
```

### Important options

- `--project <PROJECT>`: Project root directory (default: `.`).
- `--caps <CAPS>`: Comma-separated capabilities to grant (`inspect`, `input`, `mutate`, `eval`, `fs`, `run`; default is `""`, which is read-only).
- `--features-runner <PROGRAM>`: Executable used by `features_run` to run Gherkin scenarios.

### Example

```sh
ljos mcp --project . --caps fs,run
```

## `ljos observe`

Opens a graphical observer window attached to a running Ljós application that was started with `ljos run --mcp`. Displays processes, mailboxes, objects, logs, traces, the supervision tree, frames, memory, models, wakes, components, bindings, growth and design audits.

```sh
ljos observe [OPTIONS] [TARGET]
```

`TARGET` is either a `target/ljos/mcp-<pid>.json` token file or a project directory to search for the newest token file (default: `.`).

### Important options

- `--remote`: Allow connecting to a non-loopback address in the token file.
- `--size <WxH>`: Initial observer window size in logical pixels (default: `1200x760`).
- `--mcp[=<CAPS>]`: Serve an MCP inspector on the observer's own window (`target/ljos/mcp-observe-<pid>.json`).
- `--mcp-rate <N>`: With `--mcp`, answer up to `N` calls per second (1 to 1000) instead of 20.

### Example

```sh
ljos observe .
```

## `ljos attach`

Bridges a running application's live MCP inspector (`target/ljos/mcp-<pid>.json`) to standard input and output (one JSON-RPC message per line) for MCP clients that launch servers as subprocesses. Re-reads the target file on every message so that if the application restarts, `ljos attach` reconnects to the new process automatically.

```sh
ljos attach [OPTIONS] [TARGET]
```

### Important options

- `--remote`: Allow connecting to a non-loopback address in the token file.

### Example

```sh
ljos attach .
```

## `ljos bench`

Measures QML applications on Ljós and on Qt (`qml6`) inside a Linux cgroup v2 memory and CPU limit (`systemd-run --user --scope`), writing `report.json` and `report.md`. Exits with status `2` when a run exceeds its budget.

```sh
ljos bench [OPTIONS] [FILES]...
```

### Important options

- `--manifest <MANIFEST>`: JSON manifest of benchmark cases.
- `--runtime <RUNTIMES>`: Comma-separated runtimes to measure (`ljos`, `qt`; default: `ljos,qt`).
- `--profile <PROFILES>`: Hardware profiles to enforce (`desktop`, `pi-zero`).
- `--memory-max <BYTES>`: Memory limit when no `--profile` is set (default: `256M`, with `MemorySwapMax=0`).
- `--cpus <N>` / `--cpu-factor <N>` / `--cpu-list <LIST>`: CPU core count, throttle percentage and pinning.
- `--settle <S>` / `--idle <S>` / `--load <S>`: Durations in seconds for startup settle, idle measurement and synthetic gesture load (`--gesture click|drag|scroll`).
- `--bisect`: Binary-search the lowest `memory.max` (within `--bisect-range`, default `8,512` MiB) that completes without an OOM kill.
- `--out <DIR>`: Output directory for `report.json` and `report.md` (default: `target/bench`).

### Example

```sh
ljos bench Main.qml --runtime ljos --profile pi-zero --out target/bench
```

## See also

- [Your first app](./first-app.md)
- [Hot reload](./hot-reload.md)
- [Testing](./testing.md)
- [MCP servers for agents](./mcp.md)
- [Observer](./observe.md)
