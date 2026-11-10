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

## Quick start

```sh
mise install
mise run check
mise run test
mise run features
```

### QtWebEngine

`import QtWebEngine` is an opt-in cargo feature (`webengine`) that shows pages through the Chromium Embedded Framework. The first build downloads CEF (about 1.5 GB unpacked) into `$CEF_PATH`, by default `target/cef`:

```sh
mise run webengine:build                               # ljos with QtWebEngine, and its helper
mise run webengine:run examples/webbrowser/Main.qml    # a browser in one window
```

On macOS CEF runs from an application bundle with helper applications, so `webengine:build` also makes `target/debug/ljos.app` and `webengine:run` runs `ljos` from inside it. To ship an application of your own that shows web pages on macOS, bundle its executable the same way: `ljos-webengine-bundle <executable>` (built with `cargo build -p ljos_webengine --bin ljos-webengine-bundle`) makes `<Name>.app` with CEF's framework and the helper applications, copying the framework so the bundle stands on its own.

`docs/ARCHITECTURE.md` section 7.9 has the details.

## Licence

MIT
