# QML usage inventory (Ljós UI research 08)

Generated 2026-10-10 by `docs/research/tools/qml_inventory.py` (run with `python3 -I`). Raw results: `docs/research/data/qml_inventory.json`.

## Scope and method

- **A** (Corpus A: hal-c2 product QML): `hal-c2/apps/desktop-qt`, `hal-c2/apps/mobile-qt`, `hal-c2/plugins`. All `*.qml` under these trees, plus `.js`/`.mjs` for import resolution and JS-feature counts.
- **B** (Corpus B: Qt's own examples (qtdoc)): `qml-solid/corpus/qtdoc`.
- Excluded at any depth: `build/`, `node_modules/`, `target/`, `.git/`.
- Comments and string/regex/template contents are masked before analysis. Object instantiations are `TypeName {` (including dotted names such as `Controls.Button {`) and value sources `Type on prop {`. Keys are attributed to the innermost instantiation with a brace stack; `key:` statements inside plain blocks (handler bodies, object literals, functions) are ignored.
- Types are classed as `qt` (default), `local` (a `<Name>.qml` in the same directory, an imported relative directory, or an inline `component`), or `third-party` (qualified by a non-`Qt*` module). Local types are shown with a suffix and excluded from the Qt-type tables.
- `files` means distinct files containing the item. Shares use the number of `.qml` files (QML items) or all `.qml`/`.js`/`.mjs` files (JS features and Qt API calls) as denominator.
- Heuristic, not a QML parser: expect small error bars, mostly from ternaries split across lines, unusual formatting, and unqualified types that actually come from third-party modules.

Denominators: A has **198 .qml** / **216 source** files; B has **524 .qml** / **529 source** files.

## 1. Corpus size

| Metric | A | B |
|---|---|---|
| `.qml` files | 198 | 524 |
| total lines in `.qml` | 45032 | 64762 |
| `.js` files in corpus | 12 | 5 |
| `.mjs` files in corpus | 6 | 0 |
| distinct `.js`/`.mjs` files imported by QML (incl. `Qt.include`) | 12 | 3 |
| `.js`/`.mjs` import statements (resolved + unresolved) | 23 | 11 |
| unresolved `.js` imports | 0 | 1 |
| distinct relative directory imports | 1 | 10 |
| relative directory import statements | 39 | 14 |
| object instantiations (all types) | 3991 | 7280 |
| distinct instantiated type names | 294 | 659 |
| `.qml` files with ≥1 instantiation | 198 | 524 |

## 2. Imports

### 2a. Module imports (`import X [version] [as Y]`)

| Module | A stmts | A files | A versions | B stmts | B files | B versions |
|---|---|---|---|---|---|---|
| `QtQuick` | 196 | 196 | none (196) | 502 | 502 | none (496), 6.5 (3), 2.15 (3) |
| `QtQuick.Layouts` | 98 | 98 | none (98) | 100 | 100 | none (100) |
| `HalC2.Shell` | 172 | 172 | none (172) | 0 | 0 | — |
| `QtQuick.Controls.Basic` | 87 | 87 | none (87) | 45 | 45 | none (45) |
| `QtQuick.Controls` | 2 | 2 | none (2) | 94 | 94 | none (91), 2.15 (3) |
| `QtQuick3D` | 0 | 0 | — | 91 | 91 | none (91) |
| `Thermostat` | 0 | 0 | — | 42 | 42 | none (42) |
| `QtTest` | 39 | 39 | none (39) | 0 | 0 | — |
| `ToDoList` | 0 | 0 | — | 38 | 38 | none (38) |
| `QtQml` | 2 | 2 | none (2) | 19 | 19 | none (19) |
| `QtQuick3D.Particles3D` | 0 | 0 | — | 21 | 21 | none (21) |
| `HalC2.Bricks` | 20 | 20 | none (20) | 0 | 0 | — |
| `QtQuick.Templates` | 0 | 0 | — | 20 | 20 | none (20) |
| `QtQuick.Effects` | 0 | 0 | — | 19 | 19 | none (19) |
| `QtQuick.Controls.Material` | 10 | 10 | none (10) | 7 | 7 | none (7) |
| `Config` | 0 | 0 | — | 16 | 16 | none (16) |
| `QtQuick.Controls.Fusion` | 0 | 0 | — | 16 | 16 | none (16) |
| `QtMultimedia` | 1 | 1 | none (1) | 14 | 14 | none (14) |
| `QtQuick3D.Physics` | 0 | 0 | — | 15 | 15 | none (15) |
| `QtQuick.Particles` | 0 | 0 | — | 14 | 14 | none (14) |
| `QtQuick.Dialogs` | 7 | 7 | none (7) | 6 | 6 | none (6) |
| `QtQuick.Shapes` | 6 | 6 | none (6) | 6 | 6 | none (6) |
| `QtQuick.Window` | 2 | 2 | none (2) | 10 | 10 | none (10) |
| `ThermostatCustomControls` | 0 | 0 | — | 12 | 12 | none (12) |
| `QtQuick3D.Helpers` | 0 | 0 | — | 10 | 10 | none (10) |
| `StocQt` | 0 | 0 | — | 10 | 10 | none (10) |
| `QtQuick.Controls.impl` | 0 | 0 | — | 8 | 8 | none (8) |
| `QtQuick.Studio.Components` | 0 | 0 | — | 8 | 8 | none (8) |
| `QtCore` | 0 | 0 | — | 7 | 7 | none (7) |
| `MediaControls` | 0 | 0 | — | 6 | 6 | none (6) |
| `content` | 0 | 0 | — | 6 | 6 | none (6) |
| `examples.Maroon` | 0 | 0 | — | 6 | 6 | none (6) |
| `qtgraphscsv` | 0 | 0 | — | 6 | 6 | none (6) |
| `QtExampleStyle` | 0 | 0 | — | 5 | 5 | none (5) |
| `QtGraphs` | 0 | 0 | — | 5 | 5 | none (5) |
| `CustomControls` | 0 | 0 | — | 4 | 4 | none (4) |
| `Ghostty` | 4 | 4 | none (4) | 0 | 0 | — |
| `QtQuick3D.Effects` | 0 | 0 | — | 4 | 4 | none (4) |
| `QtLocation` | 0 | 0 | — | 3 | 3 | none (3) |
| `QtQuick3D.Xr` | 0 | 0 | — | 3 | 3 | none (3) |
| `Quick3DAssets.Snow` | 0 | 0 | — | 3 | 3 | none (3) |
| `examples.Hangman` | 0 | 0 | — | 3 | 3 | none (3) |
| `CarRendering` | 0 | 0 | — | 2 | 2 | none (2) |
| `PocketDemo` | 0 | 0 | — | 2 | 2 | none (2) |
| `Qt.labs.folderlistmodel` | 0 | 0 | — | 2 | 2 | none (2) |
| `QtPositioning` | 0 | 0 | — | 2 | 2 | none (2) |
| `QtQml.Models` | 1 | 1 | none (1) | 1 | 1 | none (1) |
| `QtQuick.LocalStorage` | 0 | 0 | — | 2 | 2 | none (2) |
| `QtQuick.Studio.DesignEffects` | 0 | 0 | — | 2 | 2 | none (2) |
| `QtQuick.Timeline` | 0 | 0 | — | 2 | 2 | none (2) |
| `QtQuick3D.AssetUtils` | 0 | 0 | — | 2 | 2 | none (2) |
| `RobotArm` | 0 | 0 | — | 2 | 2 | none (2) |
| `Backend` | 0 | 0 | — | 1 | 1 | none (1) |
| `ComponentBundles.MaterialBundle` | 0 | 0 | — | 1 | 1 | none (1) |
| `Example` | 0 | 0 | — | 1 | 1 | none (1) |
| `LightningViewerModule` | 0 | 0 | — | 1 | 1 | none (1) |
| `LightningViewerStyle` | 0 | 0 | — | 1 | 1 | none (1) |
| `Qt.VisualTest` | 0 | 0 | — | 1 | 1 | none (1) |
| `Qt.labs.assetdownloader` | 0 | 0 | — | 1 | 1 | none (1) |
| `Qt.labs.lottieqt` | 1 | 1 | none (1) | 0 | 0 | — |
| `Qt.labs.synchronizer` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtCharts` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtQml.XmlListModel` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtQuick.Controls.Basic.impl` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtQuick.Controls.Universal` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtQuick.Controls.macOS` | 1 | 1 | none (1) | 0 | 0 | — |
| `QtQuick.Studio.Application` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtQuick.VectorImage` | 0 | 0 | — | 1 | 1 | none (1) |
| `QtSensors` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.EV_SportsCar_low` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.InteriorShadow` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.LightDecal` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.Pebbles` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.ShadowPlane` | 0 | 0 | — | 1 | 1 | none (1) |
| `Quick3DAssets.Venodhb_LOD0` | 0 | 0 | — | 1 | 1 | none (1) |
| `io.qt.fdmaploader` | 0 | 0 | — | 1 | 1 | none (1) |
| `io.qt.filenameprovider` | 0 | 0 | — | 1 | 1 | none (1) |
| `qtjenny_consumer` | 0 | 0 | — | 1 | 1 | none (1) |

### 2b. Relative directory imports (`import "dir"`)

| Import string | A stmts | B stmts |
|---|---|---|
| `"../qml/HalC2/Bricks"` | 39 | 0 |
| `"components"` | 0 | 3 |
| `"doorIcon"` | 0 | 3 |
| `"Figma_Assets"` | 0 | 1 |
| `"Pocket_Demo_SkylightUI"` | 0 | 1 |
| `"WallEffect1"` | 0 | 1 |
| `"WallEffect2"` | 0 | 1 |
| `"WallEffect3"` | 0 | 1 |
| `"content"` | 0 | 1 |
| `"resources"` | 0 | 1 |
| `"subdirectory"` | 0 | 1 |

### 2c. JS files imported by QML (top 20 per corpus, resolved paths)

| A file | A imports | B file | B imports |
|---|---|---|---|
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/settingsRows.js` | 6 | `qml-solid/corpus/qtdoc/examples/demos/maroon/content/logic.js` | 7 |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/settingsPages.js` | 5 | `qml-solid/corpus/qtdoc/examples/demos/samegame/content/samegame.js` | 2 |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/markdown.js` | 2 | `qml-solid/corpus/qtdoc/examples/demos/calqlatr/content/calculator.js` | 1 |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/usageChart.js` | 2 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/centreViews.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/changedFilesTree.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/lucide.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/modelPicker.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/panelTabs.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/providerIcons.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/scheduledTasks.js` | 1 | — | — |
| `hal-c2/apps/desktop-qt/qml/HalC2/Bricks/js/terminalLinks.js` | 1 | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |
| — | — | — | — |

## 3. Object type instantiations (top 150 per corpus, ranked by instance count)

`Type on prop {` value sources are counted as instantiations of `Type`. `(local)` and `(third-party)` types are excluded from the Qt tables.

| Rank | A type | A inst. | A files | B type | B inst. | B files |
|---|---|---|---|---|---|---|
| 1 | `ShellButton (local)` | 383 | 75 | `PropertyChanges` | 1249 | 88 |
| 2 | `Label` | 368 | 67 | `Rectangle` | 364 | 183 |
| 3 | `RowLayout` | 292 | 88 | `State` | 322 | 90 |
| 4 | `Rectangle` | 274 | 95 | `QtObject` | 303 | 49 |
| 5 | `Text` | 241 | 42 | `Item` | 277 | 176 |
| 6 | `ColumnLayout` | 238 | 83 | `Image` | 205 | 92 |
| 7 | `Item` | 200 | 114 | `NumberAnimation` | 203 | 58 |
| 8 | `Repeater` | 149 | 55 | `Model` | 200 | 53 |
| 9 | `Component` | 110 | 52 | `Node` | 197 | 67 |
| 10 | `ShellTextField (local)` | 70 | 32 | `Text` | 189 | 92 |
| 11 | `ShellIcon (local)` | 59 | 28 | `Texture` | 177 | 52 |
| 12 | `ShellMenuItem (local)` | 54 | 15 | `ListElement` | 163 | 12 |
| 13 | `Behavior` | 45 | 15 | `Keyframe` | 155 | 2 |
| 14 | `Note (local)` | 44 | 5 | `Label` | 105 | 57 |
| 15 | `ShellComboBox (local)` | 42 | 27 | `RowLayout` | 102 | 60 |
| 16 | `TapHandler` | 40 | 20 | `Connections` | 90 | 49 |
| 17 | `Caption (local)` | 40 | 6 | `PrincipledMaterial` | 90 | 31 |
| 18 | `TestCase` | 39 | 39 | `Behavior` | 77 | 33 |
| 19 | `HoverHandler` | 36 | 19 | `ColumnLayout` | 63 | 39 |
| 20 | `NumberAnimation` | 35 | 16 | `MouseArea` | 61 | 47 |
| 21 | `Loader` | 34 | 23 | `SequentialAnimation` | 57 | 24 |
| 22 | `Heading (local)` | 29 | 8 | `GradientStop` | 47 | 11 |
| 23 | `Row` | 28 | 11 | `KeyframeGroup` | 47 | 2 |
| 24 | `ListView` | 27 | 23 | `Column` | 46 | 40 |
| 25 | `RowText (local)` | 27 | 1 | `Row` | 43 | 28 |
| 26 | `Connections` | 25 | 20 | `ToyLabel (local)` | 40 | 13 |
| 27 | `Flow` | 25 | 17 | `CustomModel (local)` | 39 | 6 |
| 28 | `Column` | 25 | 8 | `LayoutItemProxy` | 39 | 6 |
| 29 | `QtObject` | 21 | 17 | `Button` | 37 | 25 |
| 30 | `Switch` | 21 | 13 | `ListModel` | 36 | 23 |
| 31 | `AbstractButton` | 18 | 15 | `Repeater` | 35 | 30 |
| 32 | `ShellMenu (local)` | 17 | 15 | `CustomMaterial` | 35 | 24 |
| 33 | `ShellButton` | 17 | 4 | `TextureInput` | 34 | 19 |
| 34 | `Dialog` | 16 | 16 | `Timer` | 33 | 26 |
| 35 | `Flickable` | 16 | 16 | `Transition` | 33 | 25 |
| 36 | `ScrollBar` | 16 | 11 | `PropertyAnimation` | 33 | 12 |
| 37 | `ListModel` | 16 | 10 | `AccessoryMaterial (local)` | 33 | 1 |
| 38 | `Field (local)` | 16 | 1 | `GridLayout` | 32 | 25 |
| 39 | `Popup` | 15 | 12 | `ParticleEmitter3D` | 32 | 18 |
| 40 | `Timer` | 14 | 9 | `ParticleSystem3D` | 31 | 18 |
| 41 | `RailButton (local)` | 14 | 2 | `RoundButton` | 31 | 13 |
| 42 | `SettingsPage (local)` | 13 | 13 | `PauseAnimation` | 30 | 11 |
| 43 | `MouseArea` | 13 | 12 | `SvgPathItem` | 30 | 7 |
| 44 | `Row (local)` | 13 | 3 | `ListView` | 29 | 26 |
| 45 | `Toggle (local)` | 12 | 4 | `TapHandler` | 29 | 20 |
| 46 | `ColorAnimation` | 11 | 9 | `ParallelAnimation` | 26 | 18 |
| 47 | `ShellCard (local)` | 11 | 7 | `Key (local)` | 26 | 1 |
| 48 | `Detail (local)` | 11 | 1 | `ModelParticle3D` | 25 | 13 |
| 49 | `DragHandler` | 10 | 9 | `State_Idle` | 25 | 3 |
| 50 | `TextField` | 10 | 7 | `Pane` | 23 | 23 |
| 51 | `ProviderIcon (local)` | 10 | 5 | `VectorDirection3D` | 23 | 12 |
| 52 | `IconButton (local)` | 10 | 2 | `ArcItem` | 22 | 3 |
| 53 | `TextArea` | 9 | 9 | `MultiEffect` | 20 | 18 |
| 54 | `Shortcut` | 9 | 8 | `Component` | 19 | 9 |
| 55 | `Instantiator` | 9 | 7 | `StaticRigidBody` | 19 | 7 |
| 56 | `ShapePath` | 9 | 6 | `Sprite` | 19 | 5 |
| 57 | `MobileButton (local)` | 9 | 5 | `KissButton (local)` | 19 | 1 |
| 58 | `Choice (local)` | 9 | 2 | `Gradient` | 17 | 11 |
| 59 | `Composer (local)` | 8 | 8 | `BoxShape` | 17 | 6 |
| 60 | `ItemDelegate` | 8 | 8 | `OperatorButton (local)` | 17 | 1 |
| 61 | `Image` | 8 | 7 | `ParticleModelShape3D` | 16 | 12 |
| 62 | `CheckBox` | 8 | 5 | `Button (local)` | 16 | 6 |
| 63 | `PathSvg` | 8 | 5 | `RainMaterial (local)` | 16 | 1 |
| 64 | `TextEdit` | 8 | 5 | `Popup` | 15 | 13 |
| 65 | `Transition` | 8 | 4 | `Slider` | 14 | 9 |
| 66 | `Hint (local)` | 8 | 3 | `ToyButton (local)` | 14 | 9 |
| 67 | `ShellCard` | 8 | 1 | `AnchorChanges` | 14 | 6 |
| 68 | `SettingsScopeSentence (local)` | 7 | 7 | `CustomButton (local)` | 14 | 5 |
| 69 | `Shape` | 7 | 6 | `Window` | 13 | 13 |
| 70 | `FilterBox (local)` | 7 | 1 | `Gravity3D` | 13 | 11 |
| 71 | `Slide (local)` | 7 | 1 | `TextField` | 13 | 11 |
| 72 | `CentreHost` | 6 | 6 | `Wander3D` | 13 | 10 |
| 73 | `Composer` | 6 | 6 | `View3D` | 13 | 9 |
| 74 | `Notifications` | 6 | 6 | `CustomRoundButton` | 13 | 7 |
| 75 | `ScrollView` | 6 | 6 | `ToyImage (local)` | 13 | 7 |
| 76 | `SettingsHost` | 6 | 6 | `DigitButton (local)` | 13 | 1 |
| 77 | `SettingsNav` | 6 | 6 | `ApplicationWindow` | 12 | 12 |
| 78 | `ShellWindow` | 6 | 6 | `SmoothedAnimation` | 12 | 4 |
| 79 | `Sidebar` | 6 | 6 | `FrameAnimation` | 11 | 10 |
| 80 | `SpinBox` | 6 | 5 | `ImageParticle` | 11 | 4 |
| 81 | `GridLayout` | 6 | 4 | `DynamicBurst3D` | 11 | 3 |
| 82 | `Markdown (local)` | 6 | 4 | `TemplateBase (local)` | 10 | 10 |
| 83 | `ShellTextField` | 6 | 2 | `DragHandler` | 10 | 9 |
| 84 | `Cells (local)` | 6 | 1 | `ScriptAction` | 10 | 7 |
| 85 | `ChartLabel (local)` | 6 | 1 | `PropertyAction` | 10 | 4 |
| 86 | `DeviceButton (local)` | 6 | 1 | `Page` | 9 | 9 |
| 87 | `ListElement` | 6 | 1 | `SoundEffect` | 9 | 7 |
| 88 | `ScopedRow (local)` | 6 | 1 | `SceneEnvironment` | 9 | 6 |
| 89 | `Segment (local)` | 6 | 1 | `DirectionalLight` | 9 | 5 |
| 90 | `RightPanel` | 5 | 5 | `ToolButton` | 9 | 5 |
| 91 | `TerminalDrawer` | 5 | 5 | `ItemModelSurfaceDataProxy` | 9 | 2 |
| 92 | `Workspace` | 5 | 5 | `Surface3DSeries` | 9 | 2 |
| 93 | `WindowControls (local)` | 5 | 4 | `IconLabel` | 8 | 8 |
| 94 | `Workspace (local)` | 5 | 4 | `PerspectiveCamera` | 8 | 8 |
| 95 | `MobileIconButton (local)` | 5 | 3 | `Loader` | 8 | 7 |
| 96 | `ShellIcon` | 5 | 3 | `PointLight` | 8 | 6 |
| 97 | `Translate` | 5 | 3 | `ToolBar` | 8 | 6 |
| 98 | `GradientStop` | 5 | 2 | `SimpleButton (local)` | 8 | 4 |
| 99 | `DashCard (local)` | 5 | 1 | `PathLine` | 8 | 2 |
| 100 | `Separator (local)` | 5 | 1 | `Emitter` | 7 | 6 |
| 101 | `Stamp (local)` | 5 | 1 | `SpriteSequence` | 7 | 5 |
| 102 | `Button` | 4 | 4 | `RadioButton` | 7 | 4 |
| 103 | `FileDialog` | 4 | 4 | `StateChangeScript` | 7 | 4 |
| 104 | `MobileBar (local)` | 4 | 4 | `DefaultMaterial` | 7 | 3 |
| 105 | `PluginPart (local)` | 4 | 4 | `GraphsTheme` | 7 | 3 |
| 106 | `ProjectIcon (local)` | 4 | 4 | `SpringAnimation` | 7 | 3 |
| 107 | `Scale` | 4 | 4 | `Tumbler` | 7 | 2 |
| 108 | `PluginSlot (local)` | 4 | 3 | `Value3DAxis` | 7 | 2 |
| 109 | `Sidebar (local)` | 4 | 3 | `BasicButton (local)` | 7 | 1 |
| 110 | `SignalSpy` | 4 | 3 | `SamegameText (local)` | 7 | 1 |
| 111 | `ShellWindow (local)` | 4 | 2 | `NavBar (local)` | 6 | 6 |
| 112 | `Timeline (local)` | 4 | 2 | `ScrollView` | 6 | 6 |
| 113 | `Window` | 4 | 2 | `ShaderEffect` | 6 | 6 |
| 114 | `Chip (local)` | 4 | 1 | `CustomRadioButton` | 6 | 5 |
| 115 | `Failure (local)` | 4 | 1 | `ItemDelegate` | 6 | 5 |
| 116 | `LineNumber (local)` | 4 | 1 | `SpriteParticle3D` | 6 | 5 |
| 117 | `MacToolbarButton (local)` | 4 | 1 | `CustomSlider (local)` | 6 | 4 |
| 118 | `Meter (local)` | 4 | 1 | `CustomTextField` | 6 | 4 |
| 119 | `RowAction (local)` | 4 | 1 | `CustomButton` | 6 | 3 |
| 120 | `Section (local)` | 4 | 1 | `ExtendedSceneEnvironment` | 6 | 3 |
| 121 | `Title (local)` | 4 | 1 | `XrInputAction` | 6 | 2 |
| 122 | `DropArea` | 3 | 3 | `GridOrderItem (local)` | 6 | 1 |
| 123 | `FolderDialog` | 3 | 3 | `Room (local)` | 6 | 1 |
| 124 | `Notifications (local)` | 3 | 3 | `Binding` | 5 | 5 |
| 125 | `SettingsBreadcrumb (local)` | 3 | 3 | `CheckBox` | 5 | 5 |
| 126 | `SidebarThreadRow (local)` | 3 | 3 | `ColorIcon (local)` | 5 | 5 |
| 127 | `Terminal` | 3 | 3 | `Flickable` | 5 | 5 |
| 128 | `TerminalMenu (local)` | 3 | 3 | `Shape` | 5 | 5 |
| 129 | `ComboBox` | 3 | 2 | `ShapePath` | 5 | 5 |
| 130 | `ShellComboBox` | 3 | 2 | `StackView` | 5 | 5 |
| 131 | `ShellSplitButton (local)` | 3 | 2 | `Switch` | 5 | 4 |
| 132 | `CandidateRow (local)` | 3 | 1 | `WheelHandler` | 5 | 4 |
| 133 | `ColorField (local)` | 3 | 1 | `AbstractButton` | 5 | 3 |
| 134 | `Count (local)` | 3 | 1 | `Rotation` | 5 | 3 |
| 135 | `EntryRow (local)` | 3 | 1 | `AccessoryTabButton (local)` | 5 | 1 |
| 136 | `Fact (local)` | 3 | 1 | `Color` | 5 | 1 |
| 137 | `FooterButton (local)` | 3 | 1 | `NodeIndicator (local)` | 5 | 1 |
| 138 | `LineText (local)` | 3 | 1 | `TumblerDelegate (local)` | 5 | 1 |
| 139 | `Stats (local)` | 3 | 1 | `App` | 4 | 4 |
| 140 | `TitleBand (local)` | 3 | 1 | `BlockEmitter (local)` | 4 | 4 |
| 141 | `WorkLine (local)` | 3 | 1 | `BusyIndicator` | 4 | 4 |
| 142 | `CommandPalette (local)` | 2 | 2 | `ButtonGroup` | 4 | 4 |
| 143 | `ComposerAttachment (local)` | 2 | 2 | `CapsuleShape` | 4 | 4 |
| 144 | `ComposerVimKeys (local)` | 2 | 2 | `Cup` | 4 | 4 |
| 145 | `ConnectionStatusRow (local)` | 2 | 2 | `DynamicRigidBody` | 4 | 4 |
| 146 | `DiffPanel (local)` | 2 | 2 | `Fog` | 4 | 4 |
| 147 | `FocusScope` | 2 | 2 | `Hangman (local)` | 4 | 4 |
| 148 | `GitActions (local)` | 2 | 2 | `ProgressBar` | 4 | 4 |
| 149 | `Gradient` | 2 | 2 | `Smoke (local)` | 4 | 4 |
| 150 | `Grid` | 2 | 2 | `StackLayout` | 4 | 4 |

## 4. Property binding names on the top 40 Qt types

Types are ranked by the Implications score (3 × A share + B share, `.qml` files containing the type). Keys are `name:` or `name.sub:` bindings directly in the object body; the number in parentheses is the number of distinct files using the key on that type. `id` is excluded.

| Type | A inst / files | A top keys | B inst / files | B top keys |
|---|---|---|---|---|
| `Item` | 200 / 114 | `height` (55), `width` (54), `implicitHeight` (23), `visible` (20), `objectName` (15), `implicitWidth` (13), `anchors.fill` (7), `x` (3) | 277 / 176 | `width` (72), `height` (69), `implicitHeight` (32), `states` (32), `implicitWidth` (30), `state` (18), `visible` (9), `anchors.fill` (9) |
| `Rectangle` | 274 / 95 | `color` (95), `radius` (70), `border.color` (57), `objectName` (40), `implicitHeight` (39), `visible` (31), `border.width` (30), `height` (29) | 364 / 183 | `color` (164), `radius` (118), `height` (93), `width` (91), `border.color` (60), `implicitHeight` (34), `anchors.fill` (33), `visible` (31) |
| `RowLayout` | 292 / 88 | `spacing` (86), `visible` (31), `objectName` (24), `anchors.fill` (18), `anchors.leftMargin` (12), `anchors.rightMargin` (12), `anchors.margins` (5), `width` (5) | 102 / 60 | `spacing` (37), `anchors.fill` (24), `width` (13), `anchors.rightMargin` (9), `anchors.leftMargin` (8), `anchors.topMargin` (6), `visible` (5), `anchors.bottomMargin` (4) |
| `ColumnLayout` | 238 / 83 | `spacing` (83), `visible` (29), `objectName` (28), `anchors.fill` (28), `width` (25), `anchors.margins` (13), `anchors.leftMargin` (12), `x` (10) | 63 / 39 | `spacing` (29), `anchors.fill` (20), `width` (8), `anchors.margins` (4), `visible` (4), `anchors.horizontalCenter` (3), `height` (3), `anchors.leftMargin` (2) |
| `Label` | 368 / 67 | `text` (67), `font.pixelSize` (66), `color` (65), `wrapMode` (55), `font.weight` (55), `objectName` (50), `visible` (45), `elide` (36) | 105 / 57 | `text` (51), `color` (49), `font.pixelSize` (46), `font.family` (20), `font.weight` (20), `font.bold` (13), `horizontalAlignment` (13), `anchors.horizontalCenter` (8) |
| `Repeater` | 149 / 55 | `model` (55), `delegate` (50) | 35 / 30 | `model` (29), `delegate` (7) |
| `Text` | 241 / 42 | `color` (42), `text` (40), `font.pixelSize` (39), `visible` (28), `elide` (25), `objectName` (24), `font.family` (22), `font.weight` (21) | 189 / 92 | `text` (87), `color` (81), `font.pixelSize` (57), `horizontalAlignment` (43), `verticalAlignment` (43), `wrapMode` (33), `font.weight` (32), `font.family` (31) |
| `Component` | 110 / 52 | — | 19 / 9 | — |
| `TestCase` | 39 / 39 | `name` (39), `when` (39) | 0 / 0 | — |
| `ListView` | 27 / 23 | `clip` (23), `model` (23), `boundsBehavior` (22), `delegate` (22), `objectName` (16), `anchors.fill` (6), `reuseItems` (6), `spacing` (6) | 29 / 26 | `delegate` (23), `model` (23), `clip` (15), `width` (14), `spacing` (13), `height` (11), `orientation` (10), `anchors.fill` (7) |
| `Connections` | 25 / 20 | `target` (20), `ignoreUnknownSignals` (1) | 90 / 49 | `target` (31) |
| `Loader` | 34 / 23 | `sourceComponent` (16), `active` (15), `source` (8), `anchors.fill` (6), `visible` (6), `width` (5), `objectName` (2), `x` (2) | 8 / 7 | `anchors.fill` (3), `source` (3), `sourceComponent` (2), `anchors.centerIn` (1), `asynchronous` (1), `visible` (1) |
| `NumberAnimation` | 35 / 16 | `duration` (15), `easing.type` (13), `property` (5), `from` (4), `to` (4), `easing.bezierCurve` (1), `objectName` (1), `target` (1) | 203 / 58 | `duration` (53), `easing.type` (34), `to` (28), `from` (23), `target` (20), `property` (17), `properties` (17), `running` (8) |
| `QtObject` | 21 / 17 | — | 303 / 49 | — |
| `TapHandler` | 40 / 20 | `acceptedButtons` (10), `enabled` (7), `acceptedDevices` (4), `acceptedModifiers` (1), `gesturePolicy` (1), `objectName` (1) | 29 / 20 | `gesturePolicy` (3), `longPressThreshold` (1) |
| `HoverHandler` | 36 / 19 | `cursorShape` (6), `enabled` (4) | 3 / 2 | — |
| `Behavior` | 45 / 15 | `enabled` (1) | 77 / 33 | `enabled` (4) |
| `Image` | 8 / 7 | `fillMode` (7), `source` (7), `sourceSize` (5), `visible` (5), `anchors.fill` (4), `asynchronous` (4), `objectName` (4), `anchors.margins` (2) | 205 / 92 | `source` (82), `visible` (27), `anchors.horizontalCenter` (20), `fillMode` (16), `anchors.centerIn` (16), `sourceSize.width` (14), `anchors.verticalCenter` (14), `width` (13) |
| `MouseArea` | 13 / 12 | `anchors.fill` (10), `cursorShape` (5), `objectName` (4), `acceptedButtons` (3), `hoverEnabled` (3), `preventStealing` (3), `anchors.bottom` (2), `anchors.top` (2) | 61 / 47 | `anchors.fill` (45), `preventStealing` (5), `anchors.rightMargin` (3), `drag.axis` (3), `enabled` (3), `height` (3), `cursorShape` (2), `drag.target` (2) |
| `Flow` | 25 / 17 | `spacing` (17), `visible` (7), `objectName` (2), `anchors.right` (2), `anchors.left` (1), `anchors.leftMargin` (1), `anchors.margins` (1), `anchors.rightMargin` (1) | 3 / 3 | `spacing` (2), `anchors.fill` (1), `anchors.margins` (1), `visible` (1), `width` (1) |
| `Flickable` | 16 / 16 | `boundsBehavior` (16), `contentHeight` (16), `clip` (15), `anchors.fill` (10), `objectName` (6), `visible` (4), `anchors.margins` (2), `interactive` (2) | 5 / 5 | `contentHeight` (5), `clip` (4), `contentWidth` (3), `anchors.fill` (2), `flickableDirection` (2), `height` (2), `width` (2), `anchors.bottom` (1) |
| `Dialog` | 16 / 16 | `anchors.centerIn` (16), `background` (16), `modal` (16), `padding` (16), `width` (16), `objectName` (15), `scale` (15), `transformOrigin` (15) | 3 / 3 | `contentItem` (2), `modal` (2), `standardButtons` (2), `title` (2), `anchors.centerIn` (1), `background` (1), `footer` (1), `implicitHeight` (1) |
| `AbstractButton` | 18 / 15 | `background` (13), `objectName` (13), `hoverEnabled` (12), `contentItem` (9), `height` (8), `width` (7), `implicitHeight` (6), `enabled` (6) | 5 / 3 | `contentItem` (2), `transitions` (2), `anchors.horizontalCenter` (1), `anchors.verticalCenter` (1), `checkable` (1), `enabled` (1), `height` (1), `hoverEnabled` (1) |
| `Row` | 28 / 11 | `spacing` (11), `anchors.right` (5), `anchors.rightMargin` (5), `height` (3), `anchors.centerIn` (3), `anchors.verticalCenter` (3), `visible` (3), `y` (3) | 43 / 28 | `spacing` (24), `anchors.horizontalCenter` (5), `height` (5), `anchors.right` (4), `anchors.top` (4), `width` (4), `anchors.rightMargin` (3), `anchors.bottom` (3) |
| `Popup` | 15 / 12 | `background` (12), `padding` (12), `width` (12), `contentItem` (12), `scale` (10), `transformOrigin` (10), `y` (10), `objectName` (10) | 15 / 13 | `anchors.centerIn` (9), `padding` (9), `background` (8), `modal` (7), `width` (7), `focus` (5), `height` (5), `closePolicy` (4) |
| `Switch` | 21 / 13 | `checked` (13), `objectName` (12), `enabled` (8), `visible` (2) | 5 / 4 | `anchors.left` (1), `anchors.rightMargin` (1), `checked` (1), `indicator` (1), `states` (1) |
| `Column` | 25 / 8 | `width` (8), `spacing` (6), `visible` (4), `x` (3), `anchors.centerIn` (3), `objectName` (3), `y` (2), `height` (2) | 46 / 40 | `spacing` (30), `width` (9), `anchors.left` (6), `anchors.top` (6), `anchors.centerIn` (4), `anchors.right` (4), `padding` (4), `anchors.fill` (3) |
| `ListModel` | 16 / 10 | `dynamicRoles` (1) | 36 / 23 | — |
| `Timer` | 14 / 9 | `interval` (9), `repeat` (5), `running` (5), `objectName` (1), `triggeredOnStart` (1) | 33 / 26 | `interval` (25), `running` (19), `repeat` (15), `triggeredOnStart` (2) |
| `ScrollBar` | 16 / 11 | `background` (1), `contentItem` (1), `height` (1), `policy` (1) | 4 / 3 | `anchors.right` (1), `scrollArea` (1), `width` (1), `anchors.bottom` (1), `anchors.top` (1), `background` (1), `contentItem` (1), `height` (1) |
| `State` | 0 / 0 | — | 322 / 90 | `name` (89), `when` (54), `extend` (13) |
| `PropertyChanges` | 0 / 0 | — | 1249 / 88 | `target` (75), `visible` (21), `color` (12), `width` (12), `font.pixelSize` (10), `leftPadding` (9), `height` (7), `rowSpacing` (7) |
| `DragHandler` | 10 / 9 | `target` (9), `enabled` (4), `grabPermissions` (4), `acceptedButtons` (1), `acceptedDevices` (1), `xAxis.enabled` (1) | 10 / 9 | `target` (7), `enabled` (5), `acceptedButtons` (2), `acceptedModifiers` (2), `grabPermissions` (2), `xAxis.enabled` (1), `xAxis.onActiveValueChanged` (1), `yAxis.onActiveValueChanged` (1) |
| `ColorAnimation` | 11 / 9 | `duration` (9) | 2 / 2 | `duration` (2), `easing.type` (1) |
| `TextArea` | 9 / 9 | `background` (9), `color` (9), `font.pixelSize` (9), `wrapMode` (9), `objectName` (6), `placeholderText` (6), `placeholderTextColor` (5), `enabled` (3) | 2 / 2 | `background` (1), `color` (1), `font.pixelSize` (1), `padding` (1), `wrapMode` (1) |
| `ItemDelegate` | 8 / 8 | `contentItem` (7), `width` (7), `background` (6), `objectName` (6), `enabled` (3), `height` (3), `implicitHeight` (3), `leftPadding` (3) | 6 / 5 | `width` (4), `background` (3), `contentItem` (3), `icon.color` (2), `height` (1), `checkable` (1), `font.pixelSize` (1), `highlighted` (1) |
| `Node` | 0 / 0 | — | 197 / 67 | `rotation` (11), `position` (9), `scale` (8), `objectName` (6), `eulerRotation` (5), `y` (3), `scale.x` (3), `scale.y` (3) |
| `TextField` | 10 / 7 | `objectName` (6), `placeholderText` (6), `text` (3), `color` (3), `font.pixelSize` (3), `placeholderTextColor` (3), `enabled` (2), `background` (2) | 13 / 11 | `background` (6), `font.pixelSize` (6), `color` (5), `placeholderText` (5), `verticalAlignment` (5), `implicitHeight` (4), `placeholderTextColor` (4), `text` (4) |
| `Shortcut` | 9 / 8 | `enabled` (6), `sequence` (5), `context` (3), `sequences` (3), `autoRepeat` (1) | 2 / 1 | `sequence` (1) |
| `Button` | 4 / 4 | `font.pixelSize` (3), `background` (2), `contentItem` (2), `flat` (2), `font.family` (2), `implicitHeight` (2), `implicitWidth` (2), `objectName` (2) | 37 / 25 | `text` (17), `background` (9), `checked` (6), `opacity` (6), `contentItem` (6), `display` (6), `icon.color` (5), `icon.source` (5) |

### 4b. Attached properties and grouped keys (top 60 per corpus)

Attached = `Upper.name` (`Layout.fillWidth`, `Keys.onPressed`, `Component.onCompleted`, `ListView.view`). Grouped lowercase keys (`anchors.fill`, `border.color`) are in 4a above.

| Rank | A attached (files) | B attached (files) |
|---|---|---|
| 1 | `Layout.fillWidth` (97) | `Layout.fillWidth` (63) |
| 2 | `Accessible.name` (83) | `Layout.alignment` (58) |
| 3 | `Layout.topMargin` (37) | `Component.onCompleted` (48) |
| 4 | `Component.onCompleted` (31) | `Layout.preferredWidth` (44) |
| 5 | `Layout.fillHeight` (30) | `Layout.preferredHeight` (42) |
| 6 | `Layout.preferredHeight` (26) | `Layout.fillHeight` (30) |
| 7 | `ToolTip.text` (25) | `Layout.leftMargin` (19) |
| 8 | `ToolTip.visible` (25) | `Layout.minimumWidth` (18) |
| 9 | `Accessible.role` (24) | `Layout.topMargin` (13) |
| 10 | `Layout.preferredWidth` (21) | `Layout.minimumHeight` (12) |
| 11 | `Layout.alignment` (20) | `Layout.rightMargin` (12) |
| 12 | `Layout.leftMargin` (19) | `Layout.columnSpan` (10) |
| 13 | `Layout.bottomMargin` (15) | `Layout.bottomMargin` (7) |
| 14 | `Layout.rightMargin` (13) | `Layout.maximumWidth` (7) |
| 15 | `Layout.maximumWidth` (13) | `Layout.rowSpan` (6) |
| 16 | `Keys.onPressed` (12) | `Keys.onPressed` (5) |
| 17 | `ScrollBar.vertical` (10) | `Layout.row` (4) |
| 18 | `ToolTip.delay` (9) | `Layout.column` (4) |
| 19 | `Layout.minimumWidth` (8) | `ButtonGroup.group` (4) |
| 20 | `Accessible.ignored` (8) | `Accessible.name` (3) |
| 21 | `Keys.onEscapePressed` (8) | `Layout.margins` (3) |
| 22 | `Layout.margins` (6) | `Keys.onReleased` (2) |
| 23 | `Keys.onReturnPressed` (6) | `Layout.maximumHeight` (2) |
| 24 | `Keys.onShortcutOverride` (5) | `Material.theme` (2) |
| 25 | `Keys.onEnterPressed` (5) | `ScrollBar.vertical` (2) |
| 26 | `Component.onDestruction` (4) | `Component.onDestruction` (1) |
| 27 | `Accessible.onPressAction` (3) | `Drag.active` (1) |
| 28 | `ScrollBar.horizontal` (3) | `Drag.hotSpot.x` (1) |
| 29 | `Keys.onDownPressed` (2) | `Drag.hotSpot.y` (1) |
| 30 | `Keys.onReleased` (2) | `Keys.onAsteriskPressed` (1) |
| 31 | `Keys.onUpPressed` (2) | `Keys.onBackPressed` (1) |
| 32 | `Layout.minimumHeight` (2) | `Keys.onReturnPressed` (1) |
| 33 | `Overlay.modal` (2) | `Keys.onSpacePressed` (1) |
| 34 | `Layout.column` (1) | `LayoutMirroring.enabled` (1) |
| 35 | `Layout.row` (1) | `Material.background` (1) |
| 36 | `Layout.columnSpan` (1) | `Overlay.modal` (1) |
| 37 | `Accessible.checkable` (1) | `Screen.onPrimaryOrientationChanged` (1) |
| 38 | `Accessible.checked` (1) | `ScrollBar.horizontal.policy` (1) |
| 39 | `Accessible.description` (1) | `ScrollIndicator.vertical` (1) |
| 40 | `Keys.onSpacePressed` (1) | `T.Overlay.modal` (1) |
| 41 | `Layout.maximumHeight` (1) | `T.ScrollIndicator.vertical` (1) |
| 42 | `Material.accent` (1) | `ToolTip.delay` (1) |
| 43 | `Material.background` (1) | `ToolTip.text` (1) |
| 44 | `Material.foreground` (1) | `ToolTip.visible` (1) |
| 45 | `Material.primary` (1) | `Universal.theme` (1) |
| 46 | `Material.theme` (1) | — |
| 47 | `ScrollBar.horizontal.policy` (1) | — |
| 48 | — | — |
| 49 | — | — |
| 50 | — | — |
| 51 | — | — |
| 52 | — | — |
| 53 | — | — |
| 54 | — | — |
| 55 | — | — |
| 56 | — | — |
| 57 | — | — |
| 58 | — | — |
| 59 | — | — |
| 60 | — | — |

## 5. Signal handlers (top 60 per corpus, direct `onX:` in object bodies)

| Rank | A handler (files) | B handler (files) |
|---|---|---|
| 1 | `onClicked` (95) | `onClicked` (66) |
| 2 | `onActivated` (34) | `onTriggered` (30) |
| 3 | `onAccepted` (30) | `onTapped` (9) |
| 4 | `onTriggered` (22) | `onActiveChanged` (7) |
| 5 | `onToggled` (18) | `onAccepted` (6) |
| 6 | `onTextEdited` (16) | `onCentroidChanged` (5) |
| 7 | `onTapped` (15) | `onCurrentIndexChanged` (5) |
| 8 | `onRejected` (11) | `onFinished` (4) |
| 9 | `onActiveChanged` (10) | `onPositionChanged` (4) |
| 10 | `onActiveFocusChanged` (8) | `onPressed` (4) |
| 11 | `onLinkActivated` (7) | `onRunningChanged` (4) |
| 12 | `onOpened` (7) | `onCheckedChanged` (3) |
| 13 | `onObjectAdded` (6) | `onVisibleChanged` (3) |
| 14 | `onObjectRemoved` (6) | `onBodyEntered` (3) |
| 15 | `onDoubleTapped` (6) | `onButtonClicked` (3) |
| 16 | `onTextChanged` (6) | `onReleased` (3) |
| 17 | `onEditingFinished` (5) | `onWheel` (3) |
| 18 | `onValueModified` (5) | `onValueChanged` (2) |
| 19 | `onClosed` (5) | `onBodyExited` (2) |
| 20 | `onBackRequested` (4) | `onGameOverChanged` (2) |
| 21 | `onLoaded` (4) | `onHeightChanged` (2) |
| 22 | `onVisibleChanged` (4) | `onMetaDataChanged` (2) |
| 23 | `onCanceled` (4) | `onModeChanged` (2) |
| 24 | `onPressed` (4) | `onOpened` (2) |
| 25 | `onRequestChanged` (4) | `onRejected` (2) |
| 26 | `onMenuRequested` (3) | `onScaleChanged` (2) |
| 27 | `onDropped` (3) | `onSceneRotationChanged` (2) |
| 28 | `onInput` (3) | `onTimeFrameChanged` (2) |
| 29 | `onPositionChanged` (3) | `onToySelected` (2) |
| 30 | `onPressedChanged` (3) | `onKeyActivated` (1) |
| 31 | `onQuestionChanged` (3) | `onIsOnChanged` (1) |
| 32 | `onReleased` (3) | `onCancelled` (1) |
| 33 | `onResized` (3) | `onConfirmed` (1) |
| 34 | `onWheel` (3) | `onSelectedTrackChanged` (1) |
| 35 | `onAboutToShow` (2) | `onActivated` (1) |
| 36 | `onCited` (2) | `onBoundsChanged` (1) |
| 37 | `onContentHeightChanged` (2) | `onHideRequested` (1) |
| 38 | `onCountChanged` (2) | `onPressedChanged` (1) |
| 39 | `onDoubleClicked` (2) | `onPurchaseFailed` (1) |
| 40 | `onEditorChanged` (2) | `onPurchaseSucceeded` (1) |
| 41 | `onExpandedChanged` (2) | `onToggled` (1) |
| 42 | `onHeightChanged` (2) | `onActiveTracksChanged` (1) |
| 43 | `onImplicitHeightChanged` (2) | `onAlphaChanged` (1) |
| 44 | `onModelChanged` (2) | `onAlphaModeChanged` (1) |
| 45 | `onMovementStarted` (2) | `onAssetPreLoadCompleteChanged` (1) |
| 46 | `onOpenChanged` (2) | `onAttachedToChanged` (1) |
| 47 | `onRemoveRequested` (2) | `onAudioOutputsChanged` (1) |
| 48 | `onRetryRequested` (2) | `onBaseColorMapChanged` (1) |
| 49 | `onRowsChanged` (2) | `onBlockFileChanged` (1) |
| 50 | `onTargetSeqChanged` (2) | `onBodyContact` (1) |
| 51 | `onThreadChanged` (2) | `onBtnAnimatedCheckStateChanged` (1) |
| 52 | `onPicked` (1) | `onBuildingsDataReady` (1) |
| 53 | `onCommit` (1) | `onCameraPositionChanged` (1) |
| 54 | `onEdited` (1) | `onCharacterControllerPositionChanged` (1) |
| 55 | `onCommittedChanged` (1) | `onChooseRequested` (1) |
| 56 | `onRecorded` (1) | `onClearcoatAmountVertexColorMaskChannelChanged` (1) |
| 57 | `onSelectedTextChanged` (1) | `onClearcoatAmountVertexColorMaskInfluenceChanged` (1) |
| 58 | `onAskedChanged` (1) | `onClearcoatNormalMapChanged` (1) |
| 59 | `onAtYEndChanged` (1) | `onClearcoatNormalStrengthVertexColorMaskChannelChanged` (1) |
| 60 | `onAttachmentChanged` (1) | `onClearcoatNormalStrengthVertexColorMaskInfluenceChanged` (1) |

## 6. JS features and Qt APIs

### 6a. JS language features (all `.qml`, `.js`, `.mjs`; denominator = source files)

| Feature | A count | A files | A share | B count | B files | B share |
|---|---|---|---|---|---|---|
| function declaration | 1048 | 137 | 63% | 483 | 155 | 29% |
| `function expression (anonymous)` | 57 | 4 | 2% | 93 | 22 | 4% |
| `arrow function =>` | 648 | 105 | 49% | 51 | 28 | 5% |
| let | 275 | 43 | 20% | 231 | 58 | 11% |
| const | 1309 | 112 | 52% | 141 | 35 | 7% |
| var | 952 | 137 | 63% | 273 | 75 | 14% |
| JSON.parse | 17 | 5 | 2% | 3 | 2 | 0% |
| JSON.stringify | 42 | 10 | 5% | 0 | 0 | 0% |
| Promise | 4 | 2 | 1% | 0 | 0 | 0% |
| async | 4 | 3 | 1% | 0 | 0 | 0% |
| await | 16 | 4 | 2% | 0 | 0 | 0% |
| `qsTr()` | 1788 | 124 | 57% | 445 | 119 | 22% |
| `qsTrId()` | 0 | 0 | 0% | 0 | 0 | 0% |
| `qsTranslate()` | 0 | 0 | 0% | 0 | 0 | 0% |
| `qsTrN()/qsTrIdN()` | 0 | 0 | 0% | 0 | 0 | 0% |
| regex literal | 124 | 21 | 10% | 9 | 7 | 1% |
| template literal | 47 | 5 | 2% | 27 | 13 | 2% |

### 6b. Qt.* API calls (requested names plus the top 30 by score)

| API | A calls | A files | B calls | B files |
|---|---|---|---|---|
| `Qt.binding` | 18 | 10 | 29 | 4 |
| `Qt.callLater` | 20 | 8 | 3 | 3 |
| `Qt.createComponent` | 1 | 1 | 19 | 4 |
| `Qt.rgba` | 7 | 6 | 24 | 11 |
| `Qt.formatDateTime` | 3 | 2 | 1 | 1 |
| `Qt.include` | 0 | 0 | 0 | 0 |
| `Qt.alpha()` | 0 | 0 | 0 | 0 |
| `Qt.binding()` | 0 | 0 | 0 | 0 |
| `Qt.callLater()` | 0 | 0 | 0 | 0 |
| `Qt.rgba()` | 0 | 0 | 0 | 0 |
| `Qt.size()` | 0 | 0 | 0 | 0 |
| `Qt.resolvedUrl()` | 0 | 0 | 0 | 0 |
| `Qt.vector3d()` | 0 | 0 | 0 | 0 |
| `Qt.locale()` | 0 | 0 | 0 | 0 |
| `Qt.openUrlExternally()` | 0 | 0 | 0 | 0 |
| `Qt.tint()` | 0 | 0 | 0 | 0 |
| `Qt.darker()` | 0 | 0 | 0 | 0 |
| `Qt.font()` | 0 | 0 | 0 | 0 |
| `Qt.lighter()` | 0 | 0 | 0 | 0 |
| `Qt.vector2d()` | 0 | 0 | 0 | 0 |
| `Qt.color()` | 0 | 0 | 0 | 0 |
| `Qt.formatDateTime()` | 0 | 0 | 0 | 0 |
| `Qt.rect()` | 0 | 0 | 0 | 0 |
| `Qt.quaternion()` | 0 | 0 | 0 | 0 |
| `Qt.createComponent()` | 0 | 0 | 0 | 0 |
| `Qt.createQmlObject()` | 0 | 0 | 0 | 0 |
| `Qt.point()` | 0 | 0 | 0 | 0 |
| `Qt.formatDate()` | 0 | 0 | 0 | 0 |
| `Qt.formatTime()` | 0 | 0 | 0 | 0 |
| `Qt.quit()` | 0 | 0 | 0 | 0 |
| `Qt.vector4d()` | 0 | 0 | 0 | 0 |
| `Qt.matrix4x4()` | 0 | 0 | 0 | 0 |
| `Qt.url()` | 0 | 0 | 0 | 0 |

`Qt.include` is also counted as a JS import in section 1; the `()` suffix is the lookup key.

### 6c. Qt enum/constant references (`Qt.AlignLeft`, `Qt.Key_*`, ...; top 20 per corpus)

| Rank | A constant (refs) | B constant (refs) |
|---|---|---|
| 1 | `Qt.Key_Return` (38) | `Qt.AlignHCenter` (99) |
| 2 | `Qt.ControlModifier` (30) | `Qt.AlignVCenter` (64) |
| 3 | `Qt.AlignHCenter` (30) | `Qt.AlignTop` (52) |
| 4 | `Qt.AlignVCenter` (27) | `Qt.AlignLeft` (46) |
| 5 | `Qt.ShiftModifier` (25) | `Qt.AlignRight` (31) |
| 6 | `Qt.Key_Down` (24) | `Qt.AlignCenter` (17) |
| 7 | `Qt.Key_Escape` (22) | `Qt.Vertical` (16) |
| 8 | `Qt.PointingHandCursor` (16) | `Qt.AlignBottom` (12) |
| 9 | `Qt.Key_Up` (15) | `Qt.Horizontal` (7) |
| 10 | `Qt.RightButton` (14) | `Qt.Key_Down` (5) |
| 11 | `Qt.MetaModifier` (14) | `Qt.Key_Left` (5) |
| 12 | `Qt.NoFocus` (13) | `Qt.Key_Right` (5) |
| 13 | `Qt.Key_Enter` (11) | `Qt.Key_Up` (5) |
| 14 | `Qt.AltModifier` (10) | `Qt.LandscapeOrientation` (5) |
| 15 | `Qt.Key_Backtab` (9) | `Qt.ShiftModifier` (5) |
| 16 | `Qt.Key_Space` (8) | `Qt.NoFocus` (5) |
| 17 | `Qt.Key_Tab` (8) | `Qt.Dark` (4) |
| 18 | `Qt.LeftButton` (8) | `Qt.Key_A` (4) |
| 19 | `Qt.Window` (6) | `Qt.Key_D` (4) |
| 20 | `Qt.Key_Backspace` (5) | `Qt.Key_F` (4) |

## 7. Pragmas and QML language features

| Feature | A count | A files | B count | B files |
|---|---|---|---|---|
| `pragma ComponentBehavior: Bound` | 62 | 62 | 71 | 71 |
| `pragma Singleton` | 11 | 11 | 31 | 31 |
| `.pragma library (js)` | 12 | 12 | 2 | 2 |
| declared properties (`property T name: v`) | 1525 | — | 4937 | — |
| `required property` | 350 | 81 | 314 | 128 |
| `default property` | 15 | 15 | 0 | 0 |
| `readonly property` | 941 | 127 | 3346 | 80 |
| `property alias` | 52 | 24 | 578 | 161 |
| `inline component (component X: Y {)` | 97 | 39 | 15 | 10 |
| `enum declaration` | 0 | 0 | 8 | 8 |

## Implications: what to implement first

Score = **3 × A share + B share**, where *share* is the fraction of that corpus's files that use the item. Shares, not raw counts, keep the ×3 weight meaningful (B has more files than A). Local and third-party types are excluded.

### Qt types

| Rank | Item | A files (share) | B files (share) | Score |
|---|---|---|---|---|
| 1 | `Item` | 114 (58%) | 176 (34%) | 2.06 |
| 2 | `Rectangle` | 95 (48%) | 183 (35%) | 1.79 |
| 3 | `RowLayout` | 88 (44%) | 60 (11%) | 1.45 |
| 4 | `ColumnLayout` | 83 (42%) | 39 (7%) | 1.33 |
| 5 | `Label` | 67 (34%) | 57 (11%) | 1.12 |
| 6 | `Repeater` | 55 (28%) | 30 (6%) | 0.89 |
| 7 | `Text` | 42 (21%) | 92 (18%) | 0.81 |
| 8 | `Component` | 52 (26%) | 9 (2%) | 0.81 |
| 9 | `TestCase` | 39 (20%) | 0 (0%) | 0.59 |
| 10 | `ListView` | 23 (12%) | 26 (5%) | 0.40 |
| 11 | `Connections` | 20 (10%) | 49 (9%) | 0.40 |
| 12 | `Loader` | 23 (12%) | 7 (1%) | 0.36 |
| 13 | `NumberAnimation` | 16 (8%) | 58 (11%) | 0.35 |
| 14 | `QtObject` | 17 (9%) | 49 (9%) | 0.35 |
| 15 | `TapHandler` | 20 (10%) | 20 (4%) | 0.34 |
| 16 | `HoverHandler` | 19 (10%) | 2 (0%) | 0.29 |
| 17 | `Behavior` | 15 (8%) | 33 (6%) | 0.29 |
| 18 | `Image` | 7 (4%) | 92 (18%) | 0.28 |
| 19 | `MouseArea` | 12 (6%) | 47 (9%) | 0.27 |
| 20 | `Flow` | 17 (9%) | 3 (1%) | 0.26 |
| 21 | `Flickable` | 16 (8%) | 5 (1%) | 0.25 |
| 22 | `Dialog` | 16 (8%) | 3 (1%) | 0.25 |
| 23 | `AbstractButton` | 15 (8%) | 3 (1%) | 0.23 |
| 24 | `Row` | 11 (6%) | 28 (5%) | 0.22 |
| 25 | `Popup` | 12 (6%) | 13 (2%) | 0.21 |

### Attached properties (and attached handlers)

| Rank | Item | A files (share) | B files (share) | Score |
|---|---|---|---|---|
| 1 | `Layout.fillWidth` | 97 (49%) | 63 (12%) | 1.59 |
| 2 | `Accessible.name` | 83 (42%) | 3 (1%) | 1.26 |
| 3 | `Layout.topMargin` | 37 (19%) | 13 (2%) | 0.59 |
| 4 | `Component.onCompleted` | 31 (16%) | 48 (9%) | 0.56 |
| 5 | `Layout.fillHeight` | 30 (15%) | 30 (6%) | 0.51 |
| 6 | `Layout.preferredHeight` | 26 (13%) | 42 (8%) | 0.47 |
| 7 | `Layout.alignment` | 20 (10%) | 58 (11%) | 0.41 |
| 8 | `Layout.preferredWidth` | 21 (11%) | 44 (8%) | 0.40 |
| 9 | `ToolTip.text` | 25 (13%) | 1 (0%) | 0.38 |
| 10 | `ToolTip.visible` | 25 (13%) | 1 (0%) | 0.38 |
| 11 | `Accessible.role` | 24 (12%) | 0 (0%) | 0.36 |
| 12 | `Layout.leftMargin` | 19 (10%) | 19 (4%) | 0.32 |
| 13 | `Layout.bottomMargin` | 15 (8%) | 7 (1%) | 0.24 |
| 14 | `Layout.rightMargin` | 13 (7%) | 12 (2%) | 0.22 |
| 15 | `Layout.maximumWidth` | 13 (7%) | 7 (1%) | 0.21 |

### Qt APIs and JS built-ins used by Ljós-relevant code

| Rank | Item | A files (share) | B files (share) | Score |
|---|---|---|---|---|
| 1 | `qsTr()` | 124 (57%) | 119 (22%) | 1.95 |
| 2 | `Qt.alpha()` | 33 (15%) | 0 (0%) | 0.46 |
| 3 | `Qt.binding()` | 10 (5%) | 4 (1%) | 0.15 |
| 4 | `JSON.stringify` | 10 (5%) | 0 (0%) | 0.14 |
| 5 | `Qt.callLater()` | 8 (4%) | 3 (1%) | 0.12 |
| 6 | `Qt.rgba()` | 6 (3%) | 11 (2%) | 0.10 |
| 7 | `Qt.size()` | 7 (3%) | 2 (0%) | 0.10 |
| 8 | `Qt.resolvedUrl()` | 6 (3%) | 8 (2%) | 0.10 |
| 9 | `Qt.vector3d()` | 0 (0%) | 46 (9%) | 0.09 |
| 10 | `Qt.locale()` | 5 (2%) | 8 (2%) | 0.08 |
| 11 | `JSON.parse` | 5 (2%) | 2 (0%) | 0.07 |
| 12 | `Qt.openUrlExternally()` | 5 (2%) | 1 (0%) | 0.07 |
| 13 | `Qt.tint()` | 4 (2%) | 0 (0%) | 0.06 |
| 14 | `Qt.darker()` | 3 (1%) | 5 (1%) | 0.05 |
| 15 | `Qt.font()` | 3 (1%) | 3 (1%) | 0.05 |

### Signal handlers

| Rank | Item | A files (share) | B files (share) | Score |
|---|---|---|---|---|
| 1 | `onClicked` | 95 (48%) | 66 (13%) | 1.57 |
| 2 | `onActivated` | 34 (17%) | 1 (0%) | 0.52 |
| 3 | `onAccepted` | 30 (15%) | 6 (1%) | 0.47 |
| 4 | `onTriggered` | 22 (11%) | 30 (6%) | 0.39 |
| 5 | `onToggled` | 18 (9%) | 1 (0%) | 0.27 |
| 6 | `onTapped` | 15 (8%) | 9 (2%) | 0.24 |
| 7 | `onTextEdited` | 16 (8%) | 1 (0%) | 0.24 |
| 8 | `onRejected` | 11 (6%) | 2 (0%) | 0.17 |
| 9 | `onActiveChanged` | 10 (5%) | 7 (1%) | 0.16 |
| 10 | `onActiveFocusChanged` | 8 (4%) | 0 (0%) | 0.12 |
| 11 | `onOpened` | 7 (4%) | 2 (0%) | 0.11 |
| 12 | `onLinkActivated` | 7 (4%) | 1 (0%) | 0.11 |
| 13 | `onDoubleTapped` | 6 (3%) | 1 (0%) | 0.09 |
| 14 | `onTextChanged` | 6 (3%) | 1 (0%) | 0.09 |
| 15 | `onObjectAdded` | 6 (3%) | 0 (0%) | 0.09 |

