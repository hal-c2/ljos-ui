---
name: embedding-qml-in-rust
description: "Writes a Rust program that shows a QML interface with Ljós: the App, native types, list models from Rust, threads that update the interface through UiHandle, and a headless test. Use when the user asks to embed or drive QML from Rust, or to build a Rust application with a QML interface."
---

# Embedding QML in Rust

Write the Rust program the user describes, which shows its interface in QML with Ljós. It needs Rust and Cargo besides what every skill here needs.

## Requires

- The `ljos` command on `PATH` (Install Ljós in the Ljós documentation).
- Its authoring server connected to the agent as the MCP server `ljos`: `ljos mcp --project .`. The tools are named here as `ljos:<tool>`.

## Steps

1. Depend on the `ljos` crate. While the program is written, its `dev-qml` feature and `App::qml_dir` compile the QML from disk at start-up, with no build script and no type dump. A program that ships compiles its QML in `build.rs` and embeds it with `ljos::include_qml!`; that needs `ljostypes.json`, which `LJOS_TYPES=write cargo test` writes, once and after every change to a native type. If you may not run that, ask the user to.
2. A native type is a struct with `#[derive(QmlObject)]` and `#[qml_impl]`, registered with `App::register`: references/native-types.md.
3. A list from Rust implements `QmlListModel` and is attached with `Engine::set_list_model`; the `ModelSignals` it returns tell the views what changed: references/models.md.
4. Only the UI thread touches the `Engine`. Another thread takes the `UiHandle` of `setup.ui_handle()` in `App::run_with` and sends it messages or closures, which run on the UI thread: references/threads-and-uihandle.md.
5. Test headless with `App::build`, which builds the Engine and the root without a window: references/testing-rust.md.
6. `ljos:qml_format`, `ljos:qml_check` and `ljos:qml_lint` on the QML files, then `cargo build` and `cargo test`. Fix every Error and repeat; the work is done when all of them pass.

A tool error with the code `unavailable`, or one that says a capability is not granted, means that check did not happen: say so in your answer, do not call the work checked.
