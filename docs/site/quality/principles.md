---
title: Goals and principles
description: Learn what Ljós aims to do, what it leaves out, and the engineering principles behind the runtime.
section: quality
order: 1
---

# Goals and principles

This page explains what Ljós does, what it chooses not to do, and the rules that guide how the runtime works, so you can decide whether it fits your application.

## Goals

Ljós is a Rust UI toolkit for malleable software: apps that people and their agents can reshape while they run. Interfaces are declarative, scripts are typed TypeScript, and every part of a running app can be observed and driven. Existing QML runs as a way in. Ten goals follow from that.

### Malleable software

People tailor the applications they use, alone or with a coding agent, instead of accepting what they are given. An application can let its user change its interface and scripts on their own device and see the change live, checked by the same compiler and lints the development tools use. Where a platform forbids code generated at run time, the changed code still runs, interpreted by the engine.

### Typed scripts

Scripts are a strict subset of TypeScript, checked before anything runs. Every program the checker accepts is valid TypeScript with the same meaning, so `tsc`, editors and coding models keep working on it. A property can have a type declared in TypeScript.

### Observable and drivable, with MCP out of the box

Every part of a running application can be inspected and driven: its accessibility tree, objects, properties and their bindings, processes, mailboxes, faults, frames and wakes. `ljos observe` shows the same in a window. Coding agents can check, look up, lint, and test QML without starting an application, and can inspect and drive a running development build over the Model Context Protocol (MCP), using the same accessible roles and names that screen readers use.

### Fault isolation and fair scheduling

Inspired by ideas from Erlang/BEAM, Ljós isolates faults and schedules work fairly so that a runaway handler, binding, or plugin cannot take over a frame or crash the process.

### Existing QML runs, as the way in

QML you already have runs. You can use typed and `required` properties, inline components, `pragma ComponentBehavior: Bound`, attached and grouped properties, `Binding` and `State` objects, and JavaScript modules, along with the controls and their behaviour and API that applications expect. This is tested against the documentation example corpus and real desktop applications. The [porting guide](../language/porting-from-qt.md) covers bringing an application across.

### Hot code update

During development, saving a file patches the running application while keeping its state intact. In production, a signed bundle can update QML, JavaScript, and assets without waiting for a store release, within store policy.

### Desktop and mobile from one engine

One engine targets Linux, macOS, and Windows first, followed by Android and iOS.

### Off-screen content costs close to nothing

Anything that is not painted does no layout, paint, or GPU work. Anything that is not visible does no binding work unless something observes it. When an application is idle, it uses 0 % CPU.

### Accessible and localisable from the start

Every item can produce an AccessKit node for assistive technologies. Text is shaped with bidirectional layout support. Translation calls with `qsTr` work against real catalogues and support switching languages at runtime.

### Behaviour specified in Gherkin

Every behaviour is specified in a Gherkin scenario before it is built, and each scenario runs as a test.

## Non-goals

Some modules and features are outside Ljós's scope for now:

- **3D, web, location, and graphing modules:** `Qt3D`, `QtLocation`, `QtGraphs`, and `Qt.labs` modules are not supported. `QtQuick3D` is being built, drawn with wgpu. `QtWebEngine` is an opt-in build feature (`webengine`) that shows pages with the Chromium Embedded Framework. `QtQuick.Particles` supports systems, emitters, image particles, and seeded simulations, while affectors, trails, and sprite particles are not included. `ShaderEffect` with custom GLSL shaders is supported on the GL painter only.
- **C++ `QObject` source compatibility:** Native types are written in Rust rather than C++.
- **A JavaScript debugger:** Because `quickjs-ng` has no debugger API, Ljós focuses on logging, stack traces with QML file and line numbers, and development overlays instead.
- **Non-QtQuick QML dialects:** Terminal UI dialects or other non-QtQuick QML vocabularies are out of scope.

## Engineering principles

### Clean room

No source code is copied from or read in Qt or Slint. Ljós is built solely from public documentation, published specifications, and black-box testing against running Qt binaries (`qml6`, `lupdate`, and `lrelease`). All control styles are authored from scratch.

### Efficiency by structure or opt-in

Ljós never changes what a program observably does to save work. Setting `visible: false` keeps bindings and timers running, and `reuseItems` defaults to `false`. Ljós saves work only in three ways:

1. When the saving is unobservable, such as lazy render bindings or paint culling.
2. When static analysis proves a structural optimisation safe, such as freezing covered `StackView` pages that have no observable handlers.
3. When your application explicitly opts in, such as with `Timer.suspendWhen` or `Frozen`. <!-- check: exact property syntax for Timer.suspendWhen and Frozen in user QML -->

Where Ljós deliberately behaves differently from what existing QML expects, a test scenario records the difference and its reason; [Bringing existing QML](../language/porting-from-qt.md) lists them.

### Ljós owns the retained tree; egui is the host

QML items are never egui widgets. Ljós owns the object tree, layout, render tree, input routing, focus, text shaping, and accessibility tree. It uses egui only for the window host, raw input, painting, and texture upload behind a small internal boundary crate (`ljos_egui`).

### One owner per piece of state; messages between owners

An engine is single-threaded and `!Send`. Background threads, compute pools, and plugin runtimes communicate with it only through bounded messages, so no locks are taken on the property binding path.

### Work happens on demand

Frames are scheduled only when state becomes dirty, or when animations, timers, or user input demand a frame. Bindings that feed only rendering are pulled when needed rather than pushed eagerly. Delegates, layouts, glyph runs, and textures exist only near the viewport.

### Measure before building the expensive version

Architectural additions are gated on concrete measurements rather than built speculatively. Each optimisation has a stated measurement threshold before it is introduced.

### Testable through the accessibility tree

Tests locate interactive elements by accessible role and name. An item that a test cannot find is also invisible to a screen reader, so a failing test points directly to an accessibility bug:

```qml
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    width: 320
    height: 200
    visible: true
    title: qsTr("Player")

    ToolButton {
        text: qsTr("Play")
        display: AbstractButton.IconOnly
        icon.name: "media-playback-start"
        ToolTip.text: text
        ToolTip.visible: hovered || visualFocus
    }
}
```

### Determinism in tests

A single injectable `Clock` drives every timer and animation during tests. Test runs never rely on sleeps or wall-clock time.

### General purpose

No crate, type, API, or tool in Ljós is coupled to a single consumer application. Application-specific needs become generic mechanisms such as window-scoped singletons, external acceptance suites, or the MCP inspector, while application glue stays in the application's own repository.

### Agents are first-class users

Both authoring tools (`ljos mcp`) and running applications (`ljos run --mcp`) expose MCP tools out of the box, using the same role-and-name addressing that tests and assistive technologies rely on.

## See also

- [Design guardrails](design-guardrails.md)
- [Accessibility](accessibility.md)
- [Performance and benchmarks](performance.md)
- [First app](../start/first-app.md)
