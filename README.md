<p align="center">
  <img src="docs/brand/ljos.svg" alt="Ljós" width="252">
</p>

# Ljós UI

*Ljós* is Icelandic for "light".

Ljós is a Rust UI toolkit for malleable software: apps that people and their agents can reshape while they run. Interfaces are declarative, scripts are typed TypeScript, and every part of a running app can be observed and driven. Existing QML runs as a way in.

Ljós renders through egui, runs scripts with rquickjs, and targets desktop and mobile. It is designed for:

- **Malleable apps.** An app can let the people who use it change its interface and scripts and see the change live, checked by the same compiler and lints the dev tools use. Hot code update keeps the app's state across a change.
- **TypeScript first.** Scripts are a strict subset of TypeScript, checked in Rust before anything runs, so `tsc`, editors and coding models work on them. Today a QML file imports `.ts` modules, and a property can have a type they declare (`docs/DIALECT.md`).
- **Observability.** Every running app can be inspected and driven over MCP: its accessibility tree, properties, bindings, processes, mailboxes, frames and wakes. `ljos observe` shows the same in a window. Coding agents get authoring tools out of the box.
- **Stability.** Erlang-style fault isolation and fair scheduling. A failing part restarts or shows a fallback, and a runaway handler cannot take over the frame or the process.
- **Low cost.** An idle app uses 0 % CPU, and off-screen content costs almost nothing.
- **Accessibility and localisation** from the start.
- **Existing QML.** QML you already have runs, so an existing app is a place to start. `ljos check --port` lists what it uses that Ljós lacks (`docs/MIGRATION.md`), and `ljos migrate` rewrites it in Ljós's own API (`docs/DIALECT.md`).

Status: under active development. The engine, the `QtQuick` and `QtQuick.Controls` types, TypeScript modules, the MCP tooling, `ljos observe`, and the QML test and Gherkin suites run today; `docs/ARCHITECTURE.md` has the design and its milestones (section 20).

Ljós is a clean-room implementation: see `CLEAN_ROOM.md`.

## Getting started

Install [mise](https://mise.jdx.dev/getting-started.html), then, in the clone:

```sh
mise trust          # mise reads this checkout's mise.toml (its Rust pin and settings) only once trusted
mise run setup      # Rust 1.98.1 and tools from mise.toml, submodules, Qt's types if Qt is here; says what it did, skipped and lacks
mise run check      # format and clippy, with the pinned Rust (it prints cargo --version)
mise run dev        # a QML app reloaded on save (bench/feed.qml; or: mise run dev path/to/Main.qml)
```

`mise run --skip-tools setup --check` installs nothing and says what is missing, each with the one command that gets it. mise.toml puts its Rust first on `PATH` for every task, so a system `cargo` of another version is never the one a task runs.

The workspace builds FilmCraft's crates (QtMultimedia's decoders) from the `third_party/filmcraft` submodule, which setup fetches with git. `mise trust` gives git no access: you need read access to its repository (https://github.com/storytold/filmcraft, in `.gitmodules`) and git credentials for github.com, as `gh auth setup-git` or another credential helper gives. When git cannot fetch it, setup stops there; once git can, run `mise run setup` again. setup asks for a submodule only while a `Cargo.toml` names a path in it, so a checkout whose manifests do not use FilmCraft skips it.

Then:

```sh
mise run test       # Rust tests (-p <crate> for one)
mise run features   # the Gherkin ledger
mise run fmt        # rustfmt every crate (--check only says what would change)
mise run docs       # API docs (warnings printed) and the Markdown links
mise run run path/to/Main.qml   # one QML file, without reload (--release, --check)
mise run build      # target/debug/ljos (--release)
mise run install    # puts ljos on your PATH (cargo install --path crates/ljos_cli --locked; --root <dir>)
mise run tasks:smoke  # every task in a quick mode, as CI runs them
mise tasks          # every task; mise run <task> --help shows its arguments
```

Builds go where cargo would put them: `CARGO_TARGET_DIR`, else `CARGO_BUILD_TARGET_DIR`, else the first `build.target-dir` in cargo's settings (`.cargo/config` in the checkout or a directory above it, or `.cargo/config.toml` where there is no `.cargo/config`, then cargo's home), else `target/`. A linked git worktree whose own settings name no target builds into its main checkout's target directory instead (`target/` or the one the main checkout's settings name, following `target/` when it is a link), and gets the main checkout's settings and those above it key by key, except the keys its own settings or the environment set (`CARGO_BUILD_JOBS`, say), which win as in plain cargo. So every task and the MCP server `.mcp.json` starts reuse the build that is there rather than making one in each new worktree. What a worktree's runs write there (the gallery's report, a sandbox's home, a bench report) goes in `worktrees/<name>-<sum>/` inside that target, so two checkouts' runs never write over each other, and a long run keeps links of its own to the programs it built. A bare repository's worktrees, the worktrees of a checkout whose git directory is kept elsewhere, and submodules keep their own. The paths below say `target/` for that directory.

Qt 6 is optional. Without it Ljós reads QML against its own types and the Qt comparisons (`features:parity`, `gallery:reference`, `fuzz:qt`, `qttest:parity`) can't run; `mise run setup` says which. CI compares with Qt 6.11.2 (the version `.github/workflows/ci.yml` pins), and `setup` (with `--check` too) notes another minor version, with the command that installs CI's, and goes on: it fails only when something it needs is missing.

### QtWebEngine

`import QtWebEngine` is an opt-in cargo feature (`webengine`) that shows pages through the Chromium Embedded Framework. The first build downloads CEF (about 1.5 GB unpacked) into `$CEF_PATH`, by default `cef/` in the target directory:

```sh
mise run webengine:build                               # ljos with QtWebEngine, and its helper
mise run webengine:run examples/webbrowser/Main.qml    # a browser in one window
```

On macOS CEF runs from an application bundle with helper applications, so `webengine:build` also makes `target/debug/ljos.app` and `webengine:run` runs `ljos` from inside it. To ship an application of your own that shows web pages on macOS, bundle its executable the same way: `ljos-webengine-bundle <executable>` (built with `cargo build -p ljos_webengine --bin ljos-webengine-bundle`) makes `<Name>.app` with CEF's framework and the helper applications, copying the framework so the bundle stands on its own.

`docs/ARCHITECTURE.md` section 7.9 has the details.

## Community

Ljós strives to follow the [Community Software](https://community-software.org/) principles: we build it together, as peers, in the open. Its [charter](https://community-software.org/#charter) has seven:

1. Everyone is welcome
2. We are peers
3. Every contribution counts
4. Influence grows from participation
5. Be kind to people, rigorous with ideas
6. “No” and “not yet” are valid answers
7. Look after yourself and each other

Code is one way to help. Testing, writing docs, design, translation and accessibility reviews count just as much, and questions from newcomers are welcome.

## Licence

MIT
