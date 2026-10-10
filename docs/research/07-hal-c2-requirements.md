# 07 — What HAL-C2 requires of Ljós UI

Research basis: read-only survey of `/home/olafura/dev/hal-c2` (`apps/desktop-qt`, `apps/mobile-qt`, `apps/tui`, `apps/server-ex`, `plugins/`, `docs/internals/*`). Counts exclude `build/` and `node_modules/`. Qt source was not read; Qt docs/behaviour only. Not fully reviewed: `mobile-qt` internals beyond its README, `TerminalController`/`terminal-runtime.md`, `devices.md` (noted where it matters).

---

## 1. Summary

**HAL-C2 is a thin QML view layer over a very large native controller layer.** The desktop app (`apps/desktop-qt`) is 182 QML files / ~31k lines of view code in `qml/` (+12 JS files, 2.6k lines) sitting on ~177 C++ files / ~42k lines of `.cpp` (~52k with headers) under `src/native/`. The QML never touches the network, the filesystem or the database; it reads `Shell.state[key]` view models and calls `Shell.dispatch(action, payload)`. That contract — state keys, actions, bricks, slots — is the real portable surface.

Consequences for Ljós:

1. **Ljós can run the QML only if it also supplies the native side.** A Rust equivalent of `ShellBridge` + ~11 QML singletons + 4 creatable native types + ~10 `QAbstractListModel` subclasses is mandatory. The ~60 controllers (composer 3.1k lines, timeline model 1.4k, command palette 1.3k, theme 1.25k, terminal 1.06k …) and the protocol-3 WebSocket client (`McClient`) are new Rust code or a re-host, not "QML features".
2. **The QML dialect used is modern Qt 6 QML, heavily typed.** `required property` ×342, `pragma ComponentBehavior: Bound` in 61 files, inline `component X:` ×90, `property var` ×518, `readonly property` ×885, typed functions, `Behavior on` ×45. A parser/binding engine that handles these is P0.
3. **The widget set is narrow and mostly Controls.Basic**, but rich-text `TextEdit` with cross-segment selection (Markdown), a variable-height `ListView` with scroll anchoring (Timeline), `Shape`/`ShapePath` icons, `Dialog/Popup`, `Flickable`, `Loader/Repeater/Instantiator`, and a terminal item are unavoidable.
4. **Hot reload is a headline feature** (user-editable `shell.qml` + `theme.json` in `~/.config/hal-c2/shell/`, with fallback to a default shell and an error overlay). Its semantics are precise and listed in §2.2.
5. **Tests are Qt Quick Test** (`tst_*.qml`: 354 `test_*` functions, ~8.4k lines). A Ljós `TestCase` runner needs `findChild` by `objectName`, `createTemporaryObject`, `tryCompare/tryVerify`, mouse/key synthesis, `when: windowShown`.
6. **The TUI does not share the desktop QML.** It uses its own dialect (`import OpenTUI`, flexbox props), implemented by a pure-TS interpreter (`opentui-qml`). What is shared is the *contract* (`Shell.state`, `dispatch`, slots, `Plugin/Contribution`). Ljós should treat the contract, not the widgets, as the portable layer.
7. **Gaps even in HAL-C2:** `qsTr` is used 1562× in 109 files but there is no `QTranslator`, `.ts`, or lupdate — HAL-C2 is English-only today. Accessibility is partial (`Accessible.name` ×297, role ×29). Plugin QML runs fully trusted with no sandbox.

Rough sizing of what Ljós owes (not what HAL-C2 owes Ljós): a QML engine subset (P0), ~25 visual types and ~15 non-visual types (P0), native-bridge crate (P0/P1), terminal widget + PTY (P1), media/camera/H.264 (P1/P2).

---

## 2. Detailed findings

### 2.1 Architecture of desktop-qt and mobile-qt

**Process model.** `hal-c2-qt` (C++/QML shell, `src/main.cpp`) launches `node apps/desktop-qt/host/main.ts` (`BackendProcess`), which launches the MC (Elixir release `bin/hal_c2 start`, or `mix hal_c2.server` in dev). The host prints newline-delimited JSON on stdout (`ready {mc:{origin,token}}`, `error`, `exit`); the shell closing the host's stdin tells it to stop the MC. The shell's own `McClient` then connects to the MC over a protocol-3 WebSocket. Ljós needs: spawn+supervise child process with stdin/stdout pipes (tokio::process), nothing exotic.

**CLI surface** (`main.cpp`): `--url` (attach to a pairing link), `--config-dir`, `--home-dir`, `--dev`, `--qml-dir` (bricks from disk), `--host-entry`, `--node`, `--app-id`, `--allow-local-folder-import`, positional folders (forwarded to a running instance via `SingleInstance`/`QLocalServer`), plus scripted-run options `--screenshot <png>` (grab window after first MC snapshot), `--action name[=json]`, `--key <chord>`. Env: `HAL_C2_HOME`, `HAL_C2_QML_DIR`, `HAL_C2_NODE_BIN`, `HAL_C2_MC_RELEASE`, `HAL_C2_MC_PORT`. Config dir `~/.config/hal-c2/shell/` (XDG), `%APPDATA%\hal-c2\config\shell\` (Windows), `~/.config/hal-c2-dev/shell/` for `--dev`. `--screenshot` implies Ljós needs an offscreen framebuffer capture (egui_kittest covers this).

**Qt modules (desktop):** Core, Gui, GuiPrivate, Network, Quick, Sql, WebSockets (+ DBus on Linux), Qt ≥ 6.9 (6.11.1 in use). Run-time style pinned to **Basic** (`QtQuick.Controls.Basic`). With no display, software scene graph is selected (`useSoftwareRenderingWithoutDisplay`). Mobile adds QtMultimedia.

**The bridge contract** (`src/ShellBridge.h`, `ShellQmlTypes.h`):

| Surface | Detail | Ljós Rust equivalent |
|---|---|---|
| `Shell.state` | `QQmlPropertyMap`; keys declared up-front; per-key notify; controllers `publish(key, QVariant)` | A `StateMap` object: dynamic string-keyed properties, each key a reactive dependency, values JSON-like (maps/lists/scalars) |
| `Shell.dispatch(action, payload)` | Controllers intercept; unhandled → `Shell.actionRequested` signal | Action bus with ordered interceptors + fallthrough signal |
| `windowCommand`, `openExternal`, `readImageFiles`, `readAttachmentFiles`, `directoryPaths`, `clipboardText`, `clipboardFiles`, `pasteAttaches`, `localDirectoryPath`, `protocolVersion`, `localFolderImportEnabled` | Invokables/properties on bridge | Plain Rust methods exposed via the Ljós native-object API |
| `sendToBricks` | native → QML request | A signal on the bridge |

**Singletons** registered per engine through factories that look up engine properties `halC2Bridge` / `halC2Theme` (never `qmlRegisterSingletonInstance` — multi-window needs one per engine): `Shell`, `Theme`, `Runtime`, `Threads`, `Themes`, `Terminals`, `Settings`, `Panel`, `PaletteModel`, `Onboarding`, `Keybindings`. → Ljós requirement: **registering Rust singleton types per engine with a factory that receives the engine context.**

**Creatable native types** (`qmlRegisterType<…>("HalC2.Shell",1,0,…)`):
- `LocalTranscriber` — spawns an absolute-path executable with argv, NDJSON stdio, kills the process group.
- `LocalFolderModel` — `QFileSystemModel` subclass with create/rename/move/Trash (`moveToTrash`); backs a `TreeView`.
- `ComposerHighlighter` — `QSyntaxHighlighter` attached to `input.textDocument` (`Composer.qml:359`, `rich: true`): inline markdown styling in a `TextEdit`. → Ljós needs a **text-document highlighting hook** (ranges→format) on its text editor.
- `DeviceScreen` — `QQuickItem` painting decoded H.264 frames from `DeviceStream` via `updatePaintNode` (FFmpeg dlopen'd at runtime). → a custom-painted native item API (upload RGBA texture to an egui paint callback).

**Uncreatable/anonymous types** exposed from C++: `DeviceStream`, `CommandRegistry`, `TerminalTabs`, `ThreadDiff`, `TimelineModel`.

**`QAbstractListModel` subclasses (10):** `TimelineModel`, `TextLinesModel`, `ThreadPreviews`, `TerminalTabs`, `FileTreeModel`, `DiffModel`, `CommandPaletteController`, `AgentsModel`, `CommandRegistry`, `ThreadPullRequests`. Role names are documented in the `Timeline.qml` header (rowId, kind, author, text, streaming, title, status, statusLabel, marker, entries, hiddenCount, expanded, files, time, icon, intent, attribution, meta, attachments, summary …). → Ljós needs a Rust **list-model trait** with role names, `rowsInserted/Removed/Moved/dataChanged/modelReset` (incl. `rowsAboutToBeInserted` — Timeline connects to it), and `data(row, role)` returning variants. No image providers, no `setContextProperty` anywhere.

**Native layer size** (`src/native/`): ~177 files, 41.9k lines cpp. Biggest: ComposerController 3128, TimelineModel 1409, CommandPaletteController 1293, ThemeController 1253, TerminalController 1058, ProviderSettingsController 1047, WorkspaceController 1033, SidebarModel 1016. ~60 controllers self-register through a static `NativeControllerRegistrar<T>(key, {stateKeys}, "QmlSingletonName"?)` (hence an OBJECT library). 203 `Q_INVOKABLE`, 221 `Q_PROPERTY`.

**Platform APIs the native layer uses:**
- Network/storage: `QWebSocket`, `QNetworkAccessManager` (HTTP POST, signed-URL upload/download, `traceparent` header), binary WS `/api/device-hub`; `QSql` SQLite client cache (`client-cache.sqlite`, written on a worker thread); `QFileSystemWatcher`; `QStandardPaths`; `SingleInstance` (local socket).
- Desktop integration: `QDesktopServices::openUrl`, `QClipboard` (text, files), `QProcess`, QtDBus notifications (freedesktop), xdg-desktop-portal screenshot (`PortalSnapShot`), macOS `NSVisualEffectView` blur (`PlatformWindow.mm`), `canberra-gtk-play` for sounds, taskbar/dock badge (`setBadgeNumber`), `QFile::moveToTrash`, `QGuiApplication` quit-on-last-window disabled, mac dock-reopen.
- Media: FFmpeg (libavcodec/avutil/swscale via `dlopen`), `qrcodegen` (QR generation), zxing-cpp + QtMultimedia camera (mobile only).

**Window (`ShellWindow.qml`, the root every shell starts from):** a QML `Window` (not `ApplicationWindow`) with `width 1280, height 820, minimumWidth 640, minimumHeight 400`, `color: Theme.windowTransparent ? "transparent" : …`, `opacity: Theme.windowOpacity`, `flags: Theme.frameless ? Qt.Window | Qt.FramelessWindowHint : Qt.Window`; commands `showMinimized/showMaximized/showNormal/startSystemMove/raise`, `visibility === Window.Maximized`; attached `Window.activeFocusItem`, `Window.width/height`. `Workspace`, `ShellTabs`, `TitleBar`, `Sidebar` call `window.startSystemMove()` on drag and toggle maximise on double-tap. `WindowControls` draws traffic lights. Needed from Ljós: frameless/transparent/opacity windows, system move, min/max/restore, active-focus-item attached property, **multiple top-level windows** (each its own engine + bridge, shared controllers; ids `[A-Za-z0-9_-]{1,32}`).

**mobile-qt** (README only): reuses `hal_c2_native` and `HalC2.Bricks` wholesale with its own root `HalC2.Mobile`. Desktop layout at ≥ 736×480, phone layout below; default window 412×915 (`HAL_C2_MOBILE_SIZE`). Adds `ScanCamera`/`Scanner` (zxing-cpp + QtMultimedia), `hal-c2://pair?pairingUrl=…` deep links, Android NDK 27, OpenSSL in the APK, Qt 6.11.1 arm64. Style pinned to Basic while `MobileShell.qml` uses a few `Material` attached properties (10 files import `QtQuick.Controls.Material`); Material layer effects do not render under the software scene graph so chrome draws its own surfaces. `Terminals.supported` ← `HAL_C2_HAS_TERMINAL`; no FFmpeg → Device tab unavailable. Mobile tests: `tst_Features`, `tst_Pairing`, `tst_Scanner` with `FakeCamera`, `PairableMc`. → Ljós must run egui on Android (and iOS eventually), handle safe areas/status bar, soft keyboard, touch (TapHandler/DragHandler), deep links.

### 2.2 The QML: corpus and mechanics

**Size:** desktop-qt 182 `.qml` (123 in `qml/`, 8 in `examples/`, 51 in `tests/`) — ~31,067 lines in `qml/`; 12 `.js` (2,586 lines: `markdown.js` 1374, `modelPicker.js` 305, `settingsRows` 244, `settingsPages` 188, `lucide.js` 132); `Bricks/` has 127 files; mobile-qt 12 qml files.

**Imports (files):** `QtQuick` 141, `HalC2.Shell` 130, `QtQuick.Layouts` 94, `QtQuick.Controls.Basic` 85 (Controls overall ~98), `HalC2.Bricks` 17, `QtQuick.Controls.Material` 10, `QtQuick.Dialogs` 7 (FileDialog 4, FolderDialog 3), `QtQuick.Shapes` 6, `Ghostty` 4, `QtQml` 2, `QtQml.Models` 1, `QtQuick.Controls.macOS` 1, `QtMultimedia` 1, `Qt.labs.lottieqt` 1, and `import "js/x.js" as X` for the 12 JS files.
**Never used:** `QtQuick.Window` import, `MultiEffect/DropShadow/ShaderEffect/Canvas`, `WorkerScript`, `XMLHttpRequest`/`fetch`, `Promise/async/await`, `StackView/SwipeView/SplitView/GridView`, `states/State`. (Good news: no shaders, no effects, no async JS.)

**Instantiated types, by frequency:** ShellButton 354, Label 319, RowLayout 214, Text 184, ColumnLayout 154, Rectangle 139, Repeater 137, Item 121, ShellTextField 69, ShellIcon 61, ShellMenuItem 46, ShellComboBox 40, Component 37, Row 36, HoverHandler 36, TapHandler 35, Loader 26, NumberAnimation 24, ListView 23, Flow 21, Connections 21, Switch 20, Dialog 15, Popup 14, Flickable 13, Timer 12, TextField 10, MouseArea 10, ShapePath 9, Instantiator 9, TextArea 7, Image 7, CheckBox 7, SpinBox 6, ListModel 6, DragHandler 6, Shortcut 5, Shape 5, ScrollView 5, TextEdit 4, GridLayout 4, DropArea 3, TextMetrics 2, Terminal 2, FocusScope 2. HAL-C2's own primitives in `Bricks/`: `ShellButton`, `ShellCard`, `ShellComboBox`, `ShellTextField`, `ShellMenu(Item)`, `ShellIcon`, `SplitButton` — Ljós can supply Basic-style built-ins and let those compose.

**Language features (counts):** `required property` 342 (79 files), `pragma ComponentBehavior: Bound` 61 files, inline `component` 90 (38 files), `property alias` 46, `Qt.callLater` 22, `Qt.binding` 17, `Qt.alpha` 99, `Qt.rgba` 7, `Qt.tint` 4, `Qt.openUrlExternally` 4, `Qt.createQmlObject` 1 (PluginRegistry), `Qt.createComponent` 1 (PluginPart), `Qt.platform.os` in 13 files, `Application.*` 41 (e.g. `Application.font.family`), `Layout.*` attached throughout, `Behavior on` 45, `Transition` 8, `ToolTip` 120, `ScrollBar` 32, `cursorShape` 19, `Keys.` 47, `Component.onCompleted` 30, typed signatures such as `function segmentAt(i: int): var`. Delegates rely on `required property` + `ComponentBehavior: Bound` for model roles and parent-scope access, so **bound-component scoping rules** must be implemented, not just parsed.

**JS:** ES2020+ (arrow fns, `??`, `?.`, template literals, spread, `Object.assign`, `Object.create(null)`, `JSON.*`, regexes). 12 files are `.pragma library` (shared, stateless). `rquickjs` suffices; needs `.pragma library` semantics (one shared instance), `import "x.js" as X` namespaces, and binding-aware access to QML objects from JS expressions (dependency tracking inside property bindings).

**Theming (`ThemeStore`, `ThemeController`).** `<config>/theme.json` (watched) uses the web `ThemeFile` shape `{version,id,name,appearance,colors,variants,window{opacity,transparent,blur,frameless,followSystemAppearance}}` plus shell-only `radius` and `fonts`. A malformed file keeps the previous theme and sets `Theme.lastError`. Built-in themes: `src/native/themes.json` (generated from `packages/shared`; oklch converted to sRGB natively). Resolution: theme.json → resolved theme → brick fallback colour. `Theme.palette.color(role, fallback)` is reactive only because `palette` is a notified property returning `this` — i.e. **bindings that call a method of a notified object depend on that property.** Other `Theme` properties: `link`, `radius`, `fontUi/Mono/Prompt/Terminal`, `fontScale`, `fontSize*`, `windowOpacity/Transparent/Blur/LiquidGlass`, `frameless`, `appearance`, `id`, `name`, `lastError`. Device prefs: `<config>/preferences.json`. Zoom is implemented by scaling the `ShellWindow` body (Chromium steps `1.2^level`); menus/overlays stay in window coords. On macOS the Qt `QPalette::Link` colour is set so markdown links colour correctly.

**Hot reload (`ShellRuntime.cpp`, 371 lines) — exact behaviour Ljós must reproduce:**
1. Watch the config dir plus (non-release) the in-repo qml dir, recursively: directories and `shell.qml`.
2. Any change → 120 ms single-shot debounce → `rebuildWatchList()`.
3. Compute SHA1 fingerprint over (path, mtime, size) of `.qml`, `.js`, `qmldir` files; unchanged → no-op.
4. `reload()`: `engine->clearComponentCache()`; load `<configDir>/shell.qml`; on failure fall back to `DefaultShell.qml` (disk if present, else `qrc:/qt/qml/...`); `lastError` drives `ShellErrorOverlay`.
5. If both fail, previous roots stay alive.
6. Each URL gets `?generation=N` because Qt 6.9 serves the old compile for the same URL even after `clearComponentCache`.
7. New window is created in the same engine before old roots get `deleteLater()`. Root must be a window. C++ singleton state persists; QML-created objects never cross generations.
8. Import paths: source dirs plus `<configDir>/qml`; user modules live under `<config>/shell/qml/`.
`QmlLive` was evaluated and rejected. No sandbox: `shell.qml` composes `HalC2.Bricks` (a user may replace any brick). `DefaultShell` exposes slots (sidebar, composer, workspace, centreView, terminalDrawer, rightPanel, toolbar, navigationPanel).

**Shortcuts.** `ShellWindow` instantiates one `Shortcut` per bound sequence (via Instantiator) and calls `Keybindings.press`. They're gated by focus (terminal, composer, text field); unmodified keys are never window shortcuts; `Popup.closePolicy` must not swallow them. `QuitController` filters app-level key events (hold-to-quit using auto-repeat). Bindings come from `config.keybindings` pushed by the MC merged with `Keybindings.cpp` defaults; `--key <chord>` injects `QKeySequence` PortableText.

**Models/virtualised lists.** `ListView` ×23. The Timeline (`Timeline.qml`, 1713 lines, `objectName: "timelineRows"`) is the stress case: variable-height rows, `positionViewAtEnd`, `positionViewAtIndex(…, ListView.Beginning)`, `indexAt`, `itemAtIndex`, `forceLayout`, `originY`, `contentY`, `contentHeight`, `atYBeginning`, `moving/dragging`, `ScrollBar.vertical`, and `Connections` to the C++ model's `onRowsAboutToBeInserted/onRowsInserted` to preserve the scroll anchor when earlier turns are prepended. Follow-the-tail; `loadEarlier()` at top; hover reveal that doesn't shift layout; `alwaysShowMeta` on Android/iOS. Other large bricks: Composer 1639, Sidebar 883, DiffPanel 719, Markdown 833.

**Text.** `Markdown.qml` renders prose as **Qt RichText HTML** (`<html><head><style>…`) in `TextEdit`s with cross-segment selection (`selectedText`, `getText`, `deselect`, `selectionStart/End`) and a "Cite" feature; code blocks, tables, quotes are drawn natively; copy uses a hidden TextEdit clipboard hack. A segment `ListModel` (append/set/setProperty/remove) is updated incrementally while streaming. `markdown.js` is a 1374-line hand-written markdown→escaped-HTML segmenter with incremental state. `TextMetrics` ×2, fonts from `Application.font.family` and Theme fonts. Icons: `Shape`/`ShapePath` from `PathSvg` strings in `js/lucide.js`. → Ljós needs a **rich-text subset** (HTML `<style>`, spans, links, `<code>`, lists, tables as used) *or* an alternative the Bricks are adapted to; plus selectable text across multiple items.

**Animation.** `Behavior on` ×45, `NumberAnimation` ×24, `Transition` ×8, `Lottie` ×1. Reduced-motion is read from the system (`LayoutController::setSystemReducedMotion`).

**Accessibility.** `Accessible.name` 297, `.role` 29 (Button 16, Graphic 4, PageTab 2, ListItem 2, Heading 2, List, ComboBox, Client, StaticText), `.ignored` 8, `.description` 1, `.onPressAction` 3. → map onto AccessKit through egui.

**Localisation.** `qsTr` 1562× with `%1` `.arg()` chaining; no translation files. Ljós must provide `qsTr`/`qsTrId`/plural at minimum as pass-through, and ideally real catalogues — this would be an improvement over the reference.

### 2.3 Client ↔ MC protocol (protocol 3)

Implemented entirely in C++ (`McClient`, ~504 lines cpp) against `apps/server-ex/lib/hal_c2/web/protocol.ex` (414 lines), `wire.ex`, `socket.ex`, `router.ex`; docs `sync.md`, `connection-runtime.md`. **No QML or JS touches the wire** — so Ljós's *QML* engine needs nothing protocol-specific; a Rust crate does.

- **Transport:** JSON text frames over WebSocket. Descriptor from `/.well-known/hal-c2/environment`. Auth: MC token as `?token=`; paired (remote) sessions get a ticket from `/api/auth/websocket-ticket`, then `?wsTicket=`. HTTP POST `/api/pull-requests/diff`; signed-URL attachment up/download; `traceparent`; binary WS `/api/device-hub`.
- **Client→server:** `{"t":"sub","id","shape":{…},"offset","handle","window","have"}`, `more`, `unsub`, `ping`, `rpc {id,environment,method,payload}`.
- **Server→client:** hello, shell, shell.rows, shell.mc, shell.environment, snapshot (parts), events, live, page, resync, error, config*, terminal(s), vcs, gitAction, scheduledTasks, preview, devices, plugins, plugin, authAccess, localServers, providerAuth/Install, `rpc.result`/`rpc.error`, pong, end.
- ~25 shape types (shell, stream, config, terminal, vcs, plugins …), routed by `environment` or `mc`.
- **Sync:** Electric-style `offset`/`handle`/`window`; catch-up sends one event per changed entity so a client receives only what it lacks; thread rows open at the newest turns and grow with `loadEarlier`.
- **Resilience:** single retry owner (500/1000/2000/4000/8000 ms backoff), waits for network, ping/pong, `wake()` on foreground; phases Closed/Connecting/Ready/Retrying/Offline/Refused/Blocked.
- **Cache:** SQLite, written on a worker thread, invariant "whole as of cursor" (transactional).

Rust mapping: `tokio-tungstenite` + `serde_json`, `reqwest` (or hyper) for HTTP, `rusqlite`/`sqlx` on a worker; all state published into the `StateMap` on the UI thread via a channel + `egui::Context::request_repaint`. The server could also be reused unchanged — the MC is language-neutral Elixir.

### 2.4 Qt Quick Test surface (`apps/desktop-qt/tests/`)

39 `tst_*.qml` (+`tst_Scenarios.qml`), 51 qml files in `tests/`, ~8,436 lines, **354 `function test_…`**. Run by `qmltestrunner -input tests -import tests/imports` with `QT_QPA_PLATFORM=offscreen`, `QT_QPA_PLATFORMTHEME=""` through `scripts/test-qml.mjs` (finds the runner via `qtpaths6`/`qmake6`). Tests import `QtQuick`, `QtTest`, `HalC2.Shell` (resolved to **pure-QML stub singletons** in `tests/imports/HalC2/Shell/`, 617 lines: `pragma Singleton` + `QtObject` for Shell, Theme, Terminals, Keybindings, Settings, Themes, Threads, Panel, PaletteModel, Runtime, plus stub types DeviceScreen, ComposerHighlighter) and a relative **directory import** `"../qml/HalC2/Bricks"` (qmldir). Tests set `Shell.state = {…}` and call `Shell.reset()` in `cleanup`.

API usage counts (what Ljós's `TestCase` must implement): `compare` 753, `findChild` 570 (by `objectName`, e.g. `"threadRow:thread"`, `"list"`, `"timelineRows"`), `verify` 462, `createTemporaryObject` 243, `mouseClick` 213, `tryCompare` 163, `tryVerify` 160, `keyClick` 105, `when: windowShown` 39, `mouseMove` 35, `waitForRendering` 32, `init()` 28, `cleanup()` 16, `fuzzyCompare` 12, `keySequence` 7, `mouseWheel` 6, `wait` 6, `waitForPolish` 4, `SignalSpy` 4, `mouseRelease/mousePress` 4, `mouseDrag` 3, `isPolishScheduled` 3, `grabImage` 3, `touchEvent` 2, `skip` 2, `waitForItemPolished` 1, `keyPress/Release` 1, `fail` 1. Test data tags reference scenarios in `features/*.feature` (Gherkin); native tests (`tests/native/tst_Features.cpp`, 26 cpp tests, rapidcheck property tests in `tests/prop`) drive the real app against a fake MC by `objectName`.

Implication: `objectName` is part of the public test contract; items must expose it and `findChild` must traverse both visual and QObject-children trees (including `Loader` items and delegates). Tests rely on layout being resolved synchronously at `waitForRendering`. egui_kittest's `Harness::run`/`step` can back `tryVerify/wait`, but pixel dimensions from Qt layout will not match egui's — tests assert on item geometry in several places, so layout semantics must follow Qt (anchors, `Layout.*`, implicit sizes).

### 2.5 TUI and QML sharing

`apps/tui` is Bun/TS using `@opentui/core`, `@opentui/react`, and `opentui-qml` (github `hal-c2/opentui-qml`, MIT). Its bricks live in `apps/tui/qml/HalC2/Tui` (~34 files, 2,576 lines, all `import OpenTUI`, flexbox props `flexDirection/justifyContent/paddingX/flexShrink/overflow`, types `Rectangle/Text/Slot`, `Theme.colors.*`, `Shell.state`/`Shell.dispatch`). **Desktop `HalC2.Bricks` is not shared with the TUI.** What is shared: the `Shell.state` keys and actions (sidebar, layout, theme, notifications …), the "shell.qml replaces/rices the default shell" model, `Plugin`/`Contribution`/`Slot`, slot names (`sidebar.footer`, `composer.actions`, `statusbar`). `opentui-qml` is a pure-TS QML interpreter supporting bindings, aliases, signals, Loader/Repeater/Instantiator/Connections/ListModel, Shortcut/Keymap/Action, Terminal/EmbeddedTerminal, `runShell` with hot reload, `ShellErrorOverlay`, and a headless `testQml`; its `docs/DESIGN.md` (744 lines), `PARITY.md` (105), `SHELL.md` (166) document the subset — a useful precedent and a free checklist for Ljós's "minimum viable QML".

Implication: Ljós should keep its engine independent of widget sets: a **Ljós TUI backend is optional** (P2), and portable plugins must restrict themselves to `Text/Row/Column/Rectangle/Timer`.

### 2.6 Plugins

**UI plugins** (`docs/user/plugins.md`): a `.qml` file in `<config>/plugins/` whose root is `Plugin { pluginId; Contribution { slot: "statusbar" … } }`. Slots: `sidebar.footer`, `composer.actions`, `statusbar`. `PluginController` (native) owns files, `plugins.json`, enabling and text. `PluginRegistry.qml` (a per-engine QML singleton) instantiates with `Qt.createQmlObject`, after a **regex rewrite** of `import OpenTUI` → `import QtQuick; import HalC2.Bricks; import "<pluginTypes>"` (`PluginRegistry.qml:43`). A contribution uses `property var slotData` (Item already owns `data`). A broken reload keeps the running version; reports go out via `Qt.callLater` only when changed; plugins can load from a URL after a user prompt. **Fully trusted, same engine, no sandbox.**

**MC plugins** (`plugins/code-review/`): `plugin.json` (id, version, `apiVersion: 1`, permissions with reasons, settings schema, screenshots), `mc/*.ex` (Elixir: `checkout.ex` 234, `code_review.ex` 1132 lines) running in the server, and `ui/*.qml` (ReviewHeader 171, ReviewRowMark 79, ReviewSettings 713, ReviewsPage 671) importing `QtQuick`, `Controls.Basic`, `Layouts`, `HalC2.Shell`, `HalC2.Bricks`. `McPluginController` follows each environment's `plugins` shape and fetches manifest-named files via `plugins.file` (cached per environment/plugin/revision), publishes `mcPlugins` (pages, threadKinds, slots). QML loaded by `Qt.createComponent(part.url)` (`PluginPart.qml:41`). `McPlugins` singleton: `call/saveSettings/watch/unwatch`, signals `answered/published`, wrapped by `PluginContext.qml`. Permissions are enforced only server-side; UI parts run with the app's full access — i.e. **QML delivered over the network is executed with full trust.**

Implications for Ljós: (a) runtime component creation from a string or URL (`createQmlObject`, `createComponent(url)` incl. file/network sources); (b) import rewriting/aliasing hook or a first-class module alias; (c) a decision on sandboxing (capability-limited engine for plugin QML) which HAL-C2 itself lacks — an opportunity but also a divergence risk.

### 2.7 Terminal and devices

`Terminal` item from `qml-ghostty` (libghostty-vt, Zig-built; separate `Ghostty` module) with no process of its own. Properties: font, foreground/background/cursor/selectionColor, ansiColors, padding, scrollbackLimit, columns/rows/cellWidth/cellHeight, title, workingDirectory, hasSelection, scrollTotal/scrollOffset/atBottom. Methods: `write`, `restore`, `reset`, `paste`, `copy`, `selectAll`, `clearSelection`, `selectedText`, `text`, `scrollLines`, `scrollTo`, `scrollToBottom`. Signals: `input`, `resized`, `bell`. A Unix `Pty` type also exists. Terminals run **on the MC** via `terminal.*` RPCs (`TerminalSession.write/resize/transcript/output`), so the item is a pure renderer + input encoder. Hidden drawers stay attached; the last 10 threads' sessions are parked; click→link mapping from text. Ljós options: `alacritty_terminal`/`vte`+`egui` painter, or bind libghostty-vt from Rust.

Devices (`devices.md` not fully reviewed): H.264 frames over `/api/device-hub` binary WS → FFmpeg decode → `DeviceScreen` paints. P2 for Ljós; mobile has no FFmpeg already.

---

## 3. Requirements for Ljós to run HAL-C2

Priority: **P0** = needed to boot the shell (`shell.qml`/`DefaultShell` renders, connects, shows threads); **P1** = daily use; **P2** = nice-to-have.

### P0 — boot the shell

**Language/engine**
1. QML parser + runtime with: `import` of namespaced modules, relative directory imports with `qmldir` (including `singleton` entries and type-file mappings), JS-file imports (`import "x.js" as X`), `pragma Singleton`, `pragma ComponentBehavior: Bound`, `.pragma library`.
2. Property system: `property <type>`, `property var`, `readonly`, `required`, `alias`, default property (`default property alias content: body.data`), property change handlers (`onFooChanged`), grouped/attached properties (`Layout.*`, `Keys.*`, `Window.*`, `ScrollBar.*`, `Accessible.*`, `Component.*`), `Behavior on`, `Transition`.
3. Bindings with automatic dependency tracking across objects, JS functions, notified-property method calls (the `Theme.palette` trick), and `Qt.binding()` re-binding.
4. Inline `component X: Type {}` (90 uses), typed function signatures, `required property` + delegates with bound scoping, `Component {}`, `Loader` (source/sourceComponent/active/item), `Repeater`, `Instantiator`, `Connections` (incl. `ignoreUnknownSignals`), `Timer`, `ListModel` (append/set/setProperty/remove/clear), `TextMetrics`.
5. JS runtime (rquickjs): ES2020+, `Qt.callLater/alpha/rgba/tint/binding/createQmlObject/createComponent/openUrlExternally/platform/application/font`, `JSON`, `console.*`. `Application.font`, `Application.active`.
6. Runtime object creation: `Qt.createQmlObject(str, parent, path)`, `Component.createObject`, `Qt.createComponent(url)` + `status` handling.

**Visual types (Basic style look + behaviour)**
7. `Item`, `Rectangle` (radius, border, gradient not used), `Image`, `Text`/`Label`, `Flickable`, `ScrollView`/`ScrollBar`, `Row/Column/Flow/Grid`, `RowLayout/ColumnLayout/GridLayout` with `Layout.fillWidth/Height/preferred*/minimum*/maximum*/alignment/margins`, `anchors.*`, `clip`, `z`, `opacity`, `visible`, `enabled`, `transform` scale (zoom).
8. Controls: `Button`, `ComboBox`, `TextField`, `TextArea`, `CheckBox`, `Switch`, `SpinBox`, `ToolTip`, `Popup`, `Dialog`, `Menu/MenuItem` (own `ShellMenu` may suffice), `Label`; `ListView` (see 16).
9. Input handlers: `MouseArea`, `TapHandler`, `HoverHandler`, `DragHandler`, `DropArea`, `Keys.*`, `FocusScope`, focus/`forceActiveFocus`/`activeFocusItem`, `Shortcut` (sequence, context, enabled), `cursorShape`.
10. `Shape`/`ShapePath`/`PathSvg` (+`PathLine/Arc` as used) for icons — 9 `ShapePath` plus the lucide icon set (`js/lucide.js`).
11. `Window` as root with: size/min size, `title`, `visible`, `color` incl. transparent, `opacity`, `flags` (frameless), `visibility`, `showMinimized/Maximized/Normal`, `startSystemMove`, `raise`, `close`; `Window.activeFocusItem` attached.

**Native bridge (Rust)**
12. Rust-implemented QML types: singletons (per-engine factories with engine context), creatable types, `QObject`-style properties with notify, invokable methods, signals, enums, `QVariant`-like value type (maps/lists/bools/numbers/strings/urls/colors).
13. `StateMap` (`Shell.state`) with per-key reactivity and whole-object assignment (tests do `Shell.state = {…}`).
14. List-model trait with roles + all row signals (`rowsAboutToBeInserted/Inserted/Removed/Moved`, `dataChanged`, `modelReset`) consumable by `ListView`, `Repeater`, `Instantiator`, `TreeView`, and QML `model:` bindings of JS arrays/ints.
15. Ability to register `HalC2.Shell` natively and `HalC2.Bricks` from a directory with `qmldir`.

**Lists**
16. `ListView`: delegate recycling/virtualisation, variable-height rows, `model` roles via `required property`, `positionViewAtEnd/Index/Beginning`, `indexAt`, `itemAtIndex`, `forceLayout`, `contentY/originY/contentHeight/contentX`, `atYBeginning/atYEnd`, `moving/dragging/flicking`, `header/footer`, `spacing`, scroll anchoring across prepend (the Timeline relies on it), `cacheBuffer`/off-screen delegate release for efficiency.

17. **Hot reload** (all of §2.2): file watch, 120 ms debounce, fingerprint of qml/js/qmldir, component-cache invalidation with per-generation URL versioning, fallback to default shell, error overlay driven by `Runtime.lastError`, keep-old-on-double-failure, preserve native singleton state, discard QML objects, multi-engine support. `theme.json` live watch with keep-previous-on-error and `Theme.lastError`.

**Process/IO**
18. Child process spawn + stdin/stdout NDJSON (BackendProcess), clean shutdown via stdin close.
19. WebSocket client (JSON frames) + HTTP client + the retry/ping state machine; **or** a reuse strategy (see risks).
20. `qsTr`/`qsTrId`/`.arg()` (`%1`…) pass-through at minimum.

**Test runner** (needed to adopt the existing 354 tests, P0 for CI): `TestCase` with `name`, `when`, `windowShown`, `init/cleanup/initTestCase/cleanupTestCase`, `test_*` + `_data` tag functions, `compare/verify/fuzzyCompare/fail/skip/tryCompare/tryVerify/wait`, `createTemporaryObject`, `findChild`, `waitForRendering/waitForPolish`, `mouseClick/Press/Release/Move/Drag/Wheel`, `keyClick/Press/Release/keySequence`, `touchEvent`, `grabImage`, `SignalSpy`; directory imports and test-only module paths (`-import tests/imports`).

### P1 — daily use

21. Rich text: HTML subset in `Text`/`TextEdit` (`textFormat: RichText`, `<style>` head, spans, `<a href>`, code, tables, lists), link hover/click (`onLinkActivated`, `hoveredLink`), `QPalette::Link`-equivalent link colour; `TextEdit` selection API (`selectedText`, `selectionStart/End`, `select/deselect`, `getText`, `cursorPosition`, `selectAll`) and multi-item selection coordination.
22. Text editing: `TextArea`/`TextEdit` with undo, IME, `textDocument` + highlighter hook (ranges with fonts/colors), `cursorRectangle`, `positionAt`, `implicitHeight` autosizing, vim-mode key handling in the composer, paste/drag-drop of files and images.
23. `Dialogs`: `FileDialog`, `FolderDialog` (native via rfd), `Dialog` modal stack, `Popup.closePolicy`, overlay/`Overlay.modal`.
24. Terminal item with VT parser, selection, scrollback, link mapping, colour palette, input encoding; PTY type (Unix) for local use.
25. System integration: clipboard (text/files/images), `openUrl`, drag-and-drop in/out, notifications (D-Bus/UserNotifications/WinRT), sounds, dock/taskbar badge, tray, single-instance forward, deep links (`hal-c2://pair`), reduced-motion/system appearance/dark mode query, window blur/transparency/liquid-glass, `NSVisualEffectView`-like backdrop where available, trash.
26. SQLite client cache (worker thread), QR generation (`qrcodegen`), file-system model + `TreeView`/`TableView` item-model API (`LocalFolderModel`).
27. Multi-window (per-window engine + bridge, shared native controllers), window state restoration.
28. Animations: `Behavior`, `NumberAnimation`, `ColorAnimation`, `Transition`, easing curves, `reduced motion` honouring; `Timer`.
29. Accessibility via AccessKit: `Accessible.name/role/description/ignored/onPressAction`, focus order, keyboard navigation of `Basic` controls, screen-reader labels on 297 named items.
30. Localisation: real catalogues (Fluent/gettext/`.ts`-compatible) with runtime language switch, pluralisation, RTL layout mirroring (`LayoutMirroring`); HAL-C2 doesn't have this yet, so it's a Ljós value-add and must not break `qsTr` strings.
31. Plugins: `createQmlObject` + URL components, import-rewrite hook, `Plugin/Contribution/Slot` types, broken-plugin tolerance (keep running version on reload error).
32. Image support: PNG/JPEG/SVG/GIF `Image` (source URL incl. `file://`, `qrc`-equivalent resource scheme, data URLs), `sourceSize`, `asynchronous`, `fillMode`, cache.
33. Platform: Linux (Wayland+X11), macOS, Windows desktop; Android arm64 build of the same engine (soft keyboard, safe-area insets, touch/flick physics, back button, camera permission).
34. Screenshot/offscreen mode (`--screenshot`, kittest `Harness`) and `--action`/`--key` scripted runs.

### P2 — nice to have

35. `DeviceScreen`-style custom-painted item API; H.264 decode (ffmpeg or `openh264`) for the Device tab.
36. QR scanning (zxing-cpp/`rxing`) + camera (nokhwa/Android CameraX) for mobile pairing.
37. `Qt.labs.lottieqt` Lottie playback (1 use).
38. `QtQuick.Controls.Material` attached properties on mobile chrome (10 files) — treat as ignored/no-op or provide minimal.
39. OpenTUI backend (`import OpenTUI` dialect) as a second renderer for the same engine; shared plugin subset.
40. Sandboxed plugin engine (capability-gated singletons, no `Qt.createQmlObject` of arbitrary text, resource quotas).
41. `qmltestrunner` CLI compat shim (`-input`, `-import`, junitxml) and Gherkin scenario runner.
42. `QmlLive`-style inspector / live property editing; QML language server compat for `ShellQmlTypes.h`-style declarations (a Ljós `.qmltypes` equivalent so editors don't flag `HalC2.Shell`).
43. macOS `QtQuick.Controls.macOS` native style (1 use), traffic-light window controls natively.

---

## 4. Risks and open questions

**R1 — The native layer is the real cost.** ~52k lines of C++ (composer, timeline fold, sidebar model, command palette, keybindings, theme resolver, terminal controller, provider settings …) back ~130 QML files. Open: does HAL-C2 port those controllers to Rust (big, but also enables a shared core with TUI/mobile), or does Ljós host a Qt-less bridge protocol (e.g. controllers move behind the MC or a Rust sidecar)? A migration of the C++ controllers into a `hal-c2-core` Rust crate used by both Qt and Ljós is the path with least duplication. Decide before measuring Ljós "compatibility".

**R2 — Exact reactivity semantics.** QML's binding dependency tracking (including method calls on notified objects like `Theme.palette.color(...)`, and `QQmlPropertyMap` per-key notifications) is subtle; HAL-C2 relies on it knowingly. If Ljós's immediate-mode egui core re-evaluates everything each frame, it will mask bugs but waste CPU on the 354+ buttons / 137 repeaters; if retained, it must copy Qt's rules. Likely requires a retained scene tree on top of egui.

**R3 — Layout fidelity.** Anchors, `Layout.*` implicit-size negotiation, `Flow`, and `implicitHeight` propagation through `Loader`/`Repeater` must follow Qt closely; tests assert on geometry. egui's layout is not Qt's; a custom layout pass is needed.

**R4 — Rich text.** `Markdown.qml` depends on Qt's RichText HTML subset in `TextEdit`, plus cross-segment selection and link colour via `QPalette`. Rebuilding that on egui's `LayoutJob` is possible but large; alternatives: restrict Markdown to a Ljós-native `RichText` segment model and adapt the brick.

**R5 — Virtualised variable-height ListView with prepend anchoring.** This is the single hardest widget (Timeline). Without exact `contentY/originY` behaviour under `rowsAboutToBeInserted` the viewport jumps when loading earlier turns. Needs a purpose-built test (HAL-C2 has Timeline tests we can reuse).

**R6 — Qt 6.9 component-cache bug workaround.** HAL-C2 appends `?generation=N` to every load URL; Ljós's own cache semantics should make this unnecessary, but compat requires that unknown URL queries are ignored for file resolution.

**R7 — Hot-reload object lifecycle.** New root must come up before the old one is deleted, singleton state persists, QML-created objects must not leak across generations, and `Runtime.lastError`/overlay must work when the *default* shell itself fails. Ljós hot update is a stated goal so this is a good shared test-bed.

**R8 — Test portability.** 354 tests rely on `findChild` + `objectName`, `createTemporaryObject`, Qt's synthetic events and stub singletons expressed as QML. Some tests assert Qt-specific behaviour (Popup close policy, Keys vs Shortcut precedence, focus rules, `ScrollBar` metrics). Expect a long tail; plan a curated subset and a compatibility ledger.

**R9 — Plugins and trust.** Both UI-plugin loaders and MC plugin parts execute downloaded QML with full app privileges (permissions only enforced by the server). If Ljós adds sandboxing, HAL-C2 plugins that call into arbitrary bricks may break; if it doesn't, Ljós inherits an RCE-shaped design. The `import OpenTUI` regex rewrite in `PluginRegistry.qml:43` is fragile and is a candidate for a first-class module alias.

**R10 — Two QML dialects.** OpenTUI QML (flexbox props, `Slot`) diverges from Qt QML. A Ljós TUI backend would be a third dialect unless it reuses `opentui-qml`'s subset. Open: does the project want one engine for all of desktop/mobile/TUI, or leave the TUI on its TS runtime and share only contracts?

**R11 — Localisation and accessibility are aspirational in the reference.** There are no catalogues (1562 `qsTr` strings are English-only) and accessibility roles are sparse. Ljós's goals (accessible, localisable) go beyond the consumer; to test them we will need to add real translation files and a11y assertions to HAL-C2 or an upstream fork.

**R12 — Platform shims with no cross-platform Rust equivalent:** QtDBus notifications, xdg-portal screenshots, `NSVisualEffectView` blur/liquid glass, dock/taskbar badge, `QFile::moveToTrash`, `canberra-gtk-play`, single-instance via local socket. Each needs a small crate or `cfg` module; compositor-specific blur (Wayland) has no protocol standard — HAL-C2 relies on `--app-id` for compositor rules.

**R13 — Terminal.** libghostty-vt is Zig-built; Ljós can link it via C ABI or switch to `alacritty_terminal`. Rendering fidelity (ligatures, wide chars, selection, scrollback restore via `restore()`) needs a decision; `Pty` is only used for local sessions (most run on the MC).

**R14 — Media.** FFmpeg is dlopen'd; Rust equivalents (`ffmpeg-next`, `openh264`) change licensing and packaging. Mobile already excludes the feature; ship it as optional.

**R15 — Android/iOS.** egui-on-Android (winit + `android-activity`) must carry soft keyboard, IME composition, safe areas, camera permission, deep links, foreground/background `wake()` hooks for the sync client. HAL-C2's mobile app is only a layout switch (`MobileShell.qml`) over the same bricks, so mobile parity is mostly the same engine plus those integrations — but Material layer effects don't render under software scene graph, which Ljós should avoid by never depending on them.

**R16 — Offscreen/unattended rendering.** HAL-C2 forces the software scene graph when no display exists; Ljós's CI must render with egui's software path (wgpu fallback / `kittest` snapshot). Edge cases found upstream (empty-clip `Shape` under software rendering) hint at clipping bugs that Ljós should cover with its own snapshots.

**Open questions for the maintainers**
1. Reuse the C++ controllers via a C ABI, port to Rust, or put them behind a new MC-side API?
2. Is Ljós required to execute `HalC2.Bricks` unmodified, or may bricks be forked/adapted (Markdown, Timeline)?
3. Which QML test suite subset is the acceptance gate for P0 (e.g. `tst_Sidebar`, `tst_Timeline`, `tst_Composer`)?
4. What is the policy for third-party plugin QML (trusted, capability-gated, signed)?
5. Is the TUI in scope for the Ljós engine?
6. Who owns localisation (catalogue format and runtime switching) — HAL-C2 or Ljós?
