# Working on Ljós

Ljós is a general-purpose Rust UI toolkit for malleable software: apps that people and their agents can reshape while they run. Interfaces are declarative, scripts are typed TypeScript, every part of a running app can be observed and driven, and existing QML runs as a way in. It renders through egui. Read `docs/ARCHITECTURE.md` before changing anything; it wins over the research reports in `docs/research/`.

## Rules

1. **Clean room.** Follow `CLEAN_ROOM.md`. Never read or copy Qt or Slint source.
2. **General purpose.** No crate, type, API, tool or milestone is written for one application. App-specific glue lives in that app's repository (ARCHITECTURE.md principle 9).
3. **Behaviour first.** New behaviour starts as a scenario in `features/` (see `features/README.md`), then code.
4. **egui stays in three crates:** `ljos_egui`, `ljos_shell`, and the B0 backend of `ljos_render`.
5. **The parser is the app's choice.** Runtime crates depend on `ljos_ir`, never on `qml_syntax` or `qml_front`. An app that lets its users change it may link `ljos_compiler` through an opt-in feature; one that doesn't links no parser (malleable software, ARCHITECTURE.md goal 1).
6. **0 % CPU when idle.** Nothing polls. Work is scheduled by dirty state, timers, animations, input or messages.
7. **Bounded messages.** Threads talk to the Engine through `UiHandle` only.

## Commands

| Task | Command |
|---|---|
| First run after a clone, safe to repeat (tools, submodules, Qt types; `--webengine` for CEF, `--no-corpus`) | `mise trust`, then `mise run setup` |
| What is here and what is missing, installing nothing (fails on a Qt that is not CI's 6.11) | `mise run --skip-tools setup --check` |
| Run a QML file with hot reload (default `bench/feed.qml`); `--check` runs it 2 s with no window | `mise run dev [file.qml] [--check] [ljos run args…]` |
| Run a QML file without reload | `mise run run <file.qml> [--release] [--check] [ljos run args…]` |
| Build / install the `ljos` CLI | `mise run build [--release] [--workspace]`, `mise run install [--root <dir>]` |
| Format, clippy (prints `cargo --version`); format only | `mise run check`, `mise run fmt [--check]` |
| The web build of the protocol half (`check` runs it) | `mise run check:wasm` |
| Rust tests | `mise run test [-p <crate>]` |
| API docs (rustdoc warnings printed) and the Markdown links | `mise run docs [-p <crate>] [--open]` |
| Gherkin ledger | `mise run features` |
| Gherkin, only the feature files a branch adds or changes (a change's gate; CI runs the ledger) | `mise run features:changed [--base <rev>] [path…]` |
| Part of the ledger, on Ljós, on Qt (`@qt-parity`) | `mise run features:run [globs…] [-t tags] [-n name]`, `features:ljos`, `features:parity` |
| Lint the ledger | `mise run features:lint` |
| External suite, and the acceptance mechanism's own test | `mise run features:acceptance <suite>`, `mise run features:acceptance-test` |
| QML tests (`tst_*.qml`), and against Qt's qmltestrunner | `mise run qttest [files…]`, `mise run qttest:parity` |
| qtdoc gallery | `mise run gallery` (after `mise run gallery:reference`; it runs `gallery:build`) |
| Run one corpus example; fetch its assets | `mise run gallery:run <id>` (no id lists them), `mise run gallery:assets [id…]` |
| Click through corpus examples off-screen, list what each click did and logged | `mise run gallery:drive [id…]` |
| Edit corpus examples while they run off-screen, list what each save kept | `mise run gallery:hot <id…>` |
| Pictures and corpus runs as the gallery compares them | `mise run compare -- <diff A B \| run …>` (`-- --help` lists them) |
| Parse / compile the qtdoc corpus and print the baseline | `mise run parse [paths…]`, `mise run compile [dir]` |
| Differential fuzzing against Qt | `mise run fuzz:qt [area] [--cases N] [--seed S]` |
| Qt's type table from the local Qt (git-ignored; `setup` makes it) | `mise run types [--qt <dir>]` |
| Benchmarks against Qt (docs/BENCHMARKS.md) | `mise run bench [ids…]`, `bench:3d`, `bench:egui`, `bench:js`, `bench:cross`, `bench:hot [--check]` |
| Play a Lottie file | `mise run lottie [file] [--qt] [--check]` |
| Build / run with QtWebEngine (CEF, fetched once into `$CEF_PATH`, default `<target>/cef`) | `mise run webengine:build`, `mise run webengine:run <file.qml>` |
| MCP authoring server | `mise run mcp [project]` (what `.mcp.json` runs; builds where the tasks do, through `LJOS_BUILD_WRAPPER` when set; `LJOS_BIN` skips the build) |
| Run an app off-screen (headless sway, own bus, cua-driver) | `mise run desktop:cua <file.qml> [args]`, `desktop:cua:exec <command…>`, `desktop:cua:call <tool> [json]`, `desktop:cua:stop` |
| See what drags from other applications carry | `mise run desktop:drag-in [--check]` |
| Diff a vendored fork against its release; try the oxc patch on the newest one | `mise run accesskit:diff`, `quickjs:diff`, `oxc:diff` (`--brief`), `mise run oxc:check` |
| What CI runs for a change; test CI's scripts and the tasks' shared code | `mise run ci:changes [base]`, `mise run ci:test`, `mise run tasks:test` |
| Every task in a quick mode (CI's `tasks-smoke` job) | `mise run tasks:smoke [--list] [--only <task>] [--dry]` |
| Every task and its arguments | `mise tasks`, `mise run <task> --help` |

## Conventions

- Rust 1.98.1, edition 2024, resolver 3. Versions are pinned in `[workspace.dependencies]`.
- `mise-tasks/` are bash scripts with `#MISE` headers and `set -euo pipefail`. A task that runs cargo sources `mise-tasks/_lib/env.sh` and runs `cargo` (never `exec cargo`): `target_dir` there is the one place the target directory is decided. That is cargo's own order: `CARGO_TARGET_DIR`, else `CARGO_BUILD_TARGET_DIR`, else a `build.target-dir` in `.cargo/config.toml`, else `target/`. A linked worktree takes its main checkout's `.cargo/config.toml` and `target/` when it has no config of its own. So a new worktree, and the MCP server its `.mcp.json` starts, reuse the main checkout's build.
- Use `thiserror` in libraries and `anyhow` only in binaries.
- Comments explain why, not what.
