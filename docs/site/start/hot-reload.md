---
title: Hot Reload
description: Update QML, scripts and images in a running application with ljos run --watch, understand what state is kept across saves, and ship signed production bundles.
section: start
order: 5
---

# Hot Reload

This page explains how `ljos run --watch` updates a running application when you save a file, how Ljós matches objects to preserve their state, what forces a remount or restart, and how production bundles work.

## Running with `--watch`

Start your application with `--watch` to enable hot reload:

```sh
ljos run --watch Main.qml
```

The file watcher monitors `.qml`, `.js`, `.mjs`, `qmldir` and `CMakeLists.txt` files, as well as image files under the project directory. After your editor finishes writing (following a 15 ms quiet window so multi-file saves batch into a single reload), Ljós recompiles the changed files and any files that reference their types, rebuilds the scene roots and transfers state from the old tree to the new one.

The old roots are destroyed only after all new roots have been created, so a reload applies as a whole or not at all.

When you also pass `--mcp=inspect,mutate`, a coding agent can trigger a reload explicitly with the `app_reload` tool or inspect the reload status in `app_status` (`/reload/watched`, `/reload/generation` and `/reload/last/outcome`).

## Running against another module directory

```sh
ljos run --watch --alias App.Media=mocks/Media Main.qml
```

`--alias MODULE=DIR` (repeatable) imports `MODULE` from `DIR` rather than from the module's own directory, for example a stand-in that answers as the real module would. The directory must describe the module in a `qmldir` or `CMakeLists.txt`; otherwise Ljós refuses it and names both. Its files are watched like the project's. While the application runs, an agent can switch with `app_alias` (`{"module": "App.Media", "dir": "mocks/Media"}`, or `"dir": null` to go back); the switch reloads, the objects made from the module are made anew and the rest of the state is kept. `app_status` lists the aliases in force under `/reload/aliases`.

## How objects are matched across a save

Ljós identifies each object across a reload by its `ObjectKey` (`(ComponentId, key)`), choosing the key in this priority order:

1. `id` (the QML identifier inside the component);
2. `objectName` (if it is a string literal);
3. `(TypeId, sibling_index_among_same_type)` as a structural fallback.

If you insert a new sibling of the same type above an existing item that has neither an `id` nor an `objectName`, its sibling index shifts. Both `ljos check` and the reload compiler warn when a stateful item lacks an `id` or `objectName` so you can give it a stable identity.

## What is kept across a save

When an object matches its counterpart from before the save, Ljós carries its live state across:

- **Script-assigned properties:** Any property you assigned from a signal handler, JavaScript function or `Component.onCompleted` keeps its assigned value, even if the literal default in the QML file changed.
- **Bindings and handlers:** Property bindings and signal handler bodies always come from the newly saved file and re-evaluate against the preserved state.
- **Focus and text editing:** A focused `TextInput` or `TextField` keeps keyboard focus, its `text`, `cursorPosition` and text selection.
- **Controls and views:** Checked states, slider values, `SwipeView` and `TabBar` `currentIndex`, `ScrollView` and `Flickable` scroll positions (`contentX`, `contentY`), `StackView` navigation depth and open `Popup` or `Dialog` instances stay in place.
- **Dynamic items and loaders:** `Repeater` delegates, `ListModel` rows modified at run time and active `Loader` items preserve the state of their children.
- **Running timers:** A running `Timer` keeps its elapsed time towards its next trigger rather than starting its interval over from zero.
- **Pictures:** When you save an image file (such as a `.png` or `.jpg`) under the project, the image cache drops the old entry after the quiet window and every `Image`, `BorderImage` and `AnimatedImage` using that file or resource URL reloads the picture without reloading the QML tree.
- **Rust state:** Native Rust types that implement `Migrate { VERSION, snapshot, migrate }` or register properties with `register_reload_state` carry their state across reloads. If `migrate` fails, the object remounts with its default values and reports the failure.

For example, if you run this file, click the area to set `label.text = "clicked"`, and then save a change to `label`'s default `text` and `font.pixelSize`:

```qml
import QtQuick

Item {
    width: 200
    height: 100

    Text {
        id: label
        text: "initial"
        font.pixelSize: 14
    }

    MouseArea {
        anchors.fill: parent
        onClicked: label.text = "clicked"
    }
}
```

After saving `text: "updated"` and `font.pixelSize: 24` on `label`, `label.text` remains `"clicked"` (because script-assigned values win over literal defaults), while `label.font.pixelSize` updates to `24`.

## Lifecycle signals and resetting state

- `Component.onCompleted` does **not** run again on an object that was preserved across a reload. It only runs on newly added objects.
- `Component.onReloaded` is a development-only signal handler that runs on preserved objects after each reload when you need custom refresh logic:

```qml
import QtQuick

Item {
    id: root
    property int reloads: 0
    Component.onReloaded: root.reloads += 1
}
```

- Adding `pragma LjosReload: reset` at the top of a QML file forces that component to remount from scratch on reload instead of preserving its state:

```qml
pragma LjosReload: reset
import QtQuick

Item {
    id: root
    property int count: 0
}
```

## What is not kept or forces a restart

- **Removed or re-typed objects:** If you delete an item from a QML file or change its QML type (for example, changing a `Rectangle` with `id: box` into an `Item` with `id: box`), the old object is destroyed (`Component.onDestruction` runs) and the new object is mounted fresh (`Component.onCompleted` runs).
- **Syntax and compile errors:** If a save fails to parse or compile, the watcher waits one additional quiet window in case your editor wrote the file in parts. If the file still fails to compile, the running scene is untouched, `Runtime.lastError` is set, and an error banner shows the diagnostic until a valid save clears it.
- **What forces a full restart:** Changing a singleton's shape, adding a new native Rust module or a failed JavaScript module fallback requires restarting the process. Ordinary JavaScript modules (`.js`, `.mjs`) reload through generation-tagged specifiers.

## Production bundles

The QML parser and hot-reload watcher live behind the `dev` feature of `ljos_shell`; a release shell built without `dev` links no QML parser at all.

For shipped applications, Ljós uses a content-addressed production bundle containing the compiled intermediate representation (IR), JavaScript source, optional `rquickjs` bytecode cache and assets.

Each bundle carries a `BundleManifest` with:

- `native_api`: a semantic version requirement on the exported Rust↔QML surface;
- `qjs_bc_version`: the `rquickjs` bytecode format version;
- `min_app_version`: the minimum host application version;
- a BLAKE3 hash for every file in the archive;
- an `ed25519` cryptographic signature.

When loading a bundle, the loader verifies the `ed25519` signature first, then checks `native_api` and `qjs_bc_version`. If the bytecode version does not match the host, Ljós compiles the JavaScript from the bundled source.

Updated bundles deploy into A/B slots with a trial mode: a new bundle is promoted only after the first frame renders, the server connects and 30 seconds pass with no supervisor escalation. If the trial crashes, the next launch rolls back to the previous slot automatically. Because bundles contain only interpreted QML IR, JavaScript and assets—never native machine code—they stay within Apple App Store and Google Play rules for interpreted updates.

<!-- check: verify whether there is a CLI subcommand (such as `ljos bundle`) to build or sign a production bundle, as `ljos --help` in the bundle does not list one -->

## See also

- [Your first app](./first-app.md)
- [Command-line reference](./cli.md)
- [MCP servers for agents](./mcp.md)
