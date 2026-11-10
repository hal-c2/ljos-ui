<p align="center">
  <img src="docs/brand/ljos.svg" alt="Ljós" width="252">
</p>

# Ljós UI

*Ljós* is Icelandic for "light".

Ljós is a Rust UI toolkit that runs QML, aiming for compatibility with Qt Quick and Qt Quick Controls. It renders through egui, runs JavaScript with rquickjs, and targets desktop and mobile. It is designed to support:

- hot code update;
- spending almost nothing on off-screen content;
- accessibility and localisation from the start;
- Erlang-style fault isolation;
- MCP tooling for coding agents out of the box.

Status: under active development. The engine, the Qt Quick and Controls types, the MCP tooling and the QML test and Gherkin suites run today; `docs/ARCHITECTURE.md` has the design and its milestones (section 20).

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
