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

## Licence

MIT
