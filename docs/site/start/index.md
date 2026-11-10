---
title: What is Ljós?
description: Ljós is a Rust UI toolkit for malleable software, apps that people and their agents can reshape while they run.
section: start
order: 1
---

# What is Ljós?

This page explains what Ljós is, what it gives you when you build an interface in QML and Rust, and where to go next in the documentation.

## Overview

*Ljós* is Icelandic for "light". It is a Rust UI toolkit for malleable software: apps that people and their agents can reshape while they run. Interfaces are declarative, scripts are typed TypeScript, and every part of a running app can be observed and driven. Existing QML runs as a way in.

Ljós renders through `egui`, runs scripts with `rquickjs`, and targets desktop and mobile under the MIT licence. You write the interface declaratively and keep application logic, data models and domain services in Rust. You can run a standalone `.qml` file directly from the command line with `ljos run`, bring an existing QML application across with the help of `ljos check --port`, or embed the `ljos` crate in a Rust binary and register your own native types.

## What you get

### Change an app while it runs

An app can take changes to its interface and scripts while it runs and show them live, checked by the same compiler and lints the dev tools use. That is what makes it malleable: the people who use it, alone or with a coding agent, can reshape it instead of accepting what they were given. Today you do that in development with `ljos run --watch`; letting the people who use a shipped app change it is being built.

When you run an application with `ljos run --watch`, Ljós watches your `.qml`, `.js`, `.mjs`, `.ts`, `qmldir` and `CMakeLists.txt` files, as well as image files under the project. Saving a file recompiles only what changed and patches the running scene in place without closing the window. Script-assigned property values, focused controls, text selections, scroll positions, navigation stacks and open popups survive the reload. If a save has a syntax or compile error, the running application stays interactive and shows the error over the window until you fix the file.

### Typed TypeScript scripts

Scripts can be TypeScript, checked before anything runs. A QML file imports a `.ts` module as it imports a script and reaches what the module exports through its name. The types are for the checker only: they are erased, and what is left runs as a script does.

```ts
// filter.ts
export type Tag = "a" | "b"
export function only(tag: Tag): number {
    return tag === "a" ? 2 : 3
}
```

```qml
import QtQuick
import "filter.ts" as Logic

Item {
    property int shown: Logic.only("b")
}
```

### Observe and drive a running app

Every part of a running app can be inspected and driven, by a person or by a coding agent. Ljós ships two Model Context Protocol (MCP) servers out of the box:

- `ljos mcp` starts the authoring server over standard input and output. It lets an agent check and lint QML files, format code, look up types and API status, explain binding tiers, and run behaviour scenarios without opening a window.
- `ljos run --mcp` starts the live inspector inside a running application. An agent (or the [`ljos observe`](./observe.md) window) can read the accessibility outline, inspect properties and their binding dependencies, synthesise real pointer and keyboard input, mutate properties, evaluate JavaScript and profile frames and wakes.

### Fault isolation

Erlang-style `ErrorBoundary` items isolate faults at run time, so a failing subtree can restart or show a fallback item without taking down the window.

### 0 % CPU when idle

An application at rest runs nothing. The UI thread sleeps until input, a running `Timer`, a running animation, a message on a bounded inbox or the windowing system wakes it. Connecting an MCP inspector or opening `ljos observe` uses no polling timers; watches sleep on the server thread until the application actually does work.

### Accessibility

Interactive controls expose their role and accessible name through AccessKit, and the same tree drives screen readers, Gherkin behaviour tests and the MCP inspector. `ljos check` and `qml_lint` audit your QML source for unnamed interactive items, pointer-only focus traps, contrast and motion rules, and untranslated strings.

### Existing QML

Existing QML runs, so an app you already have is a place to start. Ljós compiles and runs QML files that use the `QtQuick` and `QtQuick.Controls` modules, along with `QtQuick.Layouts`, `QtQuick.LocalStorage`, `QtQml.Models`, `QtQml.XmlListModel`, `Qt.labs.folderlistmodel` and `QtTest`. Expressions that only read a property path run in tier 0, arithmetic and simple expressions run in the tier-1 binding VM, and full JavaScript runs in `rquickjs`.

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

The [porting guide](../language/porting-from-qt.md) covers bringing an existing application across.

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
