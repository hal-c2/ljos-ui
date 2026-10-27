---
title: What is Ljós?
description: Ljós is a general-purpose Rust UI toolkit that runs QML natively on desktop and mobile.
section: start
order: 1
---

# What is Ljós?

This page explains what Ljós is, what it gives you when you build an interface in QML and Rust, and where to go next in the documentation.

## Overview

*Ljós* is Icelandic for "light". It is a general-purpose Rust UI toolkit that runs QML and aims for compatibility with Qt Quick and Qt Quick Controls. It renders through `egui`, runs JavaScript with `rquickjs`, and targets desktop and mobile under the MIT licence.

Ljós is for developers and teams who want to write declarative user interfaces in QML while keeping their application logic, data models and domain services in Rust. You can run a standalone `.qml` file directly from the command line with `ljos run`, port an existing Qt Quick application with the help of `ljos check --port`, or embed the `ljos` crate in a Rust binary and register your own native types.

## What you get

### QML compatibility

Ljós compiles and runs QML files using the familiar `QtQuick` and `QtQuick.Controls` modules, along with `QtQuick.Layouts`, `QtQuick.LocalStorage`, `QtQml.Models`, `QtQml.XmlListModel`, `Qt.labs.folderlistmodel` and `QtTest`. Expressions that only read a property path run in tier 0, arithmetic and simple expressions run in the tier-1 binding VM, and full JavaScript runs in `rquickjs`.

```qml
import QtQuick

Rectangle {
    width: 200
    height: 100
    border.width: 2

    Text {
        anchors.centerIn: parent
        text: "Halló"
        font.bold: true
        horizontalAlignment: Text.AlignHCenter
    }
}
```

### Rust native types

You can expose Rust structs to QML by deriving `QmlObject` and marking methods and signals with `#[qml_impl]`. Properties declared with `#[qml(notify)]` participate in the QML binding graph, and `#[qml(invokable)]` methods can be called directly from QML handlers. You register your types into a module on an `Engine` with `ljos::module!`:

```rust
use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "ColorPalette", name = "RestService", extends = QtObject)]
pub struct RestService {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub url: String,
    #[qml(default)]
    pub resources: Vec<ObjectId>,
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!(
        "ColorPalette" 1.0 => [RestService]
    ));
}
```

### Hot reload

When you run an application with `ljos run --watch`, Ljós watches your `.qml`, `.js`, `.mjs`, `qmldir` and `CMakeLists.txt` files, as well as image files under the project. Saving a file recompiles only what changed and patches the running scene in place without closing the window. Script-assigned property values, focused controls, text selections, scroll positions, navigation stacks and open popups survive the reload. If a save has a syntax or compile error, the running application stays interactive and shows the error over the window until you fix the file.

### MCP for coding agents

Ljós ships two Model Context Protocol (MCP) servers out of the box so coding agents can work with your QML code and your running interface:

- `ljos mcp` starts the authoring server over standard input and output. It lets an agent check and lint QML files, format code, look up types and API status, explain binding tiers, and run behaviour scenarios without opening a window.
- `ljos run --mcp` starts the live inspector inside a running application. An agent (or the [`ljos observe`](./observe.md) window) can read the accessibility outline, inspect properties and their binding dependencies, synthesise real pointer and keyboard input, mutate properties, evaluate JavaScript and profile frames and wakes.

### 0 % CPU when idle

An application at rest runs nothing. The UI thread sleeps until input, a running `Timer`, a running animation, a message on a bounded inbox or the windowing system wakes it. Connecting an MCP inspector or opening `ljos observe` uses no polling timers; watches sleep on the server thread until the application actually does work.

### Accessibility and fault isolation

Interactive controls expose their role and accessible name through AccessKit, and the same tree drives screen readers, Gherkin behaviour tests and the MCP inspector. `ljos check` and `qml_lint` audit your QML source for unnamed interactive items, pointer-only focus traps, contrast and motion rules, and untranslated strings. At run time, Erlang-style `ErrorBoundary` items isolate faults so a failing subtree can restart or show a fallback item without taking down the window.

## Where to go next

- [Install Ljós](./install.md) to set up the toolchain and build `ljos`.
- [Your first app](./first-app.md) to write a QML file, run it and change it live with `--watch`.
- [Command-line reference](./cli.md) for every `ljos` subcommand and option.
- [Hot reload](./hot-reload.md) for how state is preserved across saves and how production bundles work.
- [Testing](./testing.md) for `tst_*.qml` suites, deterministic PNG renders and Gherkin scenarios.
- [MCP servers for agents](./mcp.md) for the authoring server and live inspector tools.
- [Observer](./observe.md) for inspecting processes, mailboxes, objects, memory and wakes with `ljos observe`.

## See also

- [Install Ljós](./install.md)
- [Your first app](./first-app.md)
- [Language overview](../language/overview.md) <!-- check: verify relative path to language overview page in the site layout -->
