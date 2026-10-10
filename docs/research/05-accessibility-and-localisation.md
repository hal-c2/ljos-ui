# 05 — Accessibility, input/focus, text and localisation

Research for **Ljós UI** (QML on egui). Date of research: 2026-10-10. Clean-room: only public docs, specs, and permissively licensed Rust crate docs/source were read (egui, AccessKit, winit, Parley/Fontique, ICU4X). No Qt/Slint source was read. HAL-C2 and qml-solid were only read, never modified.

Claims marked **(verify)** come from memory or an incomplete fetch and must be checked before they drive a decision.

---

## 1. Summary of key findings

**Accessibility**
- AccessKit is healthy and moving fast: `accesskit 0.25.1` (2026-09-25), `accesskit_consumer 0.39.1`, `accesskit_winit 0.34.1`, `accesskit_windows 0.35.1`, `accesskit_macos 0.27.1`, `accesskit_unix 0.24.0`, **`accesskit_android 0.9.0`**, **`accesskit_ios 0.2.1`**. Every target Ljós needs now has an adapter. Android and iOS are young: Android got live regions and slider adjustment only in 0.9.0, and iOS is at 0.2.x.
- The model is provider/consumer. Ljós pushes a full tree once, then `TreeUpdate`s carrying only changed nodes (plus any parent whose child list changed). The adapter keeps the full tree. Ids are plain integers, so Ljós must supply stable ids.
- egui (0.36.2) rebuilds its AccessKit node set from scratch each pass, from widget responses. It has no retained tree. egui 0.37 (unreleased) is improving popups/menus/modals and hiding invisible widgets, and replaces `WidgetType` with `accesskit::Role`. egui_kittest 0.36.2 (on kittest 0.5) queries the AccessKit tree directly (`get_by_role`, `get_by_label`, …), which gives us a11y-first tests nearly for free.
- Ljós' retained item tree is a better fit for AccessKit than egui's immediate mode. The design should follow Masonry's pattern: per-item `needs_a11y` / `request_a11y` dirty flags, a pass that emits only changed nodes, and ids derived from stable item identity.
- Virtualised lists are an AccessKit gap. There is no "virtual child" concept, only `size_of_set`/`position_in_set`, `row_count`, scroll offsets/ranges and `Action::ScrollIntoView`. We need an "a11y-mode" cache buffer for delegates, plus handling of scroll/`ScrollIntoView` actions.
- Live regions exist (`Node::set_live`, `set_live_atomic`, `set_busy`, role `Log`/`Status`/`Alert`). Android got them in 0.9.0. HAL-C2's streaming agent output needs coalesced announcements, not per-token updates.

**Input, focus, IME**
- **Mobile IME is the biggest hole.** winit master's Android backend only toggles the soft keyboard (`ImeRequest::Enable/Disable`). It emits no `Ime::Preedit`/`Commit`, and `set_ime_cursor_area` / `set_ime_purpose` are no-ops. winit's UIKit view implements only `UIKeyInput` (no `UITextInput`, no marked text). egui 0.36 says IME/autocorrect work on iOS/Android only **on eframe web**. Ljós must own native text-input bridges: `android-activity` 0.6.1 exposes `GameTextInput` state (`set_text_input_state`, `set_ime_editor_info`), and on iOS we need a `UITextInput` view.
- Desktop IME is fine. winit 0.30.13 delivers `Ime::{Enabled, Preedit, Commit, Disabled}`; egui 0.35 added proper composition visuals; egui-winit forwards `ImeEvent::{Preedit, Commit}` and sets the cursor rect from `PlatformOutput.ime`.
- QML's focus model (`focus` + `FocusScope` → `activeFocus`, `KeyNavigation`, `Keys.onPressed`, `Shortcut`, `activeFocusOnTab`) is richer than egui's single `Memory::focus`. Ljós should own the focus tree and drive egui focus only to keep egui widgets in sync.

**Text**
- egui's text stack changed a lot in 0.34–0.36: `ab_glyph` → `skrifa` + `vello_cpu` (0.34), `harfrust` shaping for kerning/ligatures (0.35). It still has **no bidi**: issue #5069 is open since 2024-09, #8576 (Arabic/Hebrew caret broken since 0.35) is open. The Parley-in-egui PR #5784 is a stalled draft: last comment 2025-11, and the author calls the last 10% "basically impossible" with Parley's current API. egui 0.37 adds `Context::add_font_provider` using `fontique` for system fonts, so egui gets better fallback but not bidi/complex layout.
- **Parley 0.12.0 (2026-10-09)** is the right layout engine: shaping (harfrust), bidi, line breaking, alignment, inline boxes (in-flow/out-of-flow, floats), `TreeBuilder` nested spans, a `complex-scripts` feature for dictionary line/word breaking (CJK/Thai/Khmer/Lao/Myanmar), grapheme-aware carets, and `PlainEditor` with IME-compose support. It lacks built-in ellipsis/elide, hyphenation and undo (none found in docs/changelog). Fontique 0.12 has system backends for fontconfig (Linux), DirectWrite (Windows), CoreText (macOS/iOS via `target_vendor = "apple"`) and Android (parses the system font XML via `roxmltree`).
- Recommendation: **Parley + Fontique for layout, our own glyph-atlas painter feeding `egui::Shape::Mesh`**, bypassing egui's `Galley`. Keep egui's `TextEdit` only as an MVP stop-gap for single-line LTR fields.

**Localisation**
- HAL-C2 has **4,132 `qsTr(` calls in desktop-qt, no `.ts`/`.qm` files, no `lupdate` wiring, no `qsTrId`/`qsTranslate`, 212 `qsTr(...).arg(...)` and 3+ `%n` plural uses.** There is no existing translation corpus to import yet, so Ljós can pick a clean runtime format while staying `.ts`-compatible at the edges.
- Qt semantics to copy: context = QML file name (overridable by `pragma Translator` or `qsTranslate`), `disambiguation`, `%n` plurals, `%1..%9` + `.arg()`, `qsTrId`, `QT_TR_NOOP`, and `Qt.uiLanguage` triggering retranslation.
- ICU4X is at **2.3.x** (`icu 2.3.1`, 2026-08-20; CLDR 48). It provides plurals, decimal, datetime, collator, segmenter and locale fallback/expansion, with blob data providers and `icu4x-datagen` for locale trimming. It ships no MessageFormat 2 crate that I could find. `fluent-bundle` (0.16, May 2025) is slow-moving and uses an ID-keyed model that doesn't match `qsTr(sourceText)`.
- Recommendation: **a Qt-compatible catalog model** (`context + source + disambiguation [+ id]` → plural forms), with `.ts` import/export, a compact runtime binary, ICU4X `PluralRules` for category selection, and a reactive `I18n::revision` signal that every `tr()` call reads, so bindings re-evaluate on language change. Extraction is done by the qml-solid QML parser.
- Qt's own `.ts` plural model is "up to three rules + otherwise" (per-language tables), not CLDR's six categories. The mapping needs a per-language table validated against real `.ts` fixtures.

---

## 2. Detailed findings

### 2.1 AccessKit: model, versions, platform status (Oct 2026)

Versions from crates.io on 2026-10-10: accesskit 0.25.1, accesskit_consumer 0.39.1, accesskit_winit 0.34.1, accesskit_windows 0.35.1, accesskit_macos 0.27.1, accesskit_unix 0.24.0, accesskit_android 0.9.0, accesskit_ios 0.2.1. The 2026-09-25 batch changelog is on the GitHub releases page: `tree_id` property in the JSON schema, Android live regions + slider adjust, Unix Image interface, minimum Rust 1.87 for Windows.

**Tree model** (docs.rs `accesskit` 0.25.1):
- `TreeUpdate { nodes: Vec<(NodeId, Node)>, tree: Option<TreeInfo>, tree_id: TreeId, focus: NodeId }`.
  - `nodes` overwrite by id. A changed node must be sent **in full**, not as a partial diff.
  - A child may only be added alongside a parent update whose `children` contains it. Removing a child means updating the parent without it.
  - `focus` must be sent with every update (the root if nothing is focused).
  - `tree` (`TreeInfo`) is required on the first update.
  - **Subtrees** (new in the 0.2x line): a non-ROOT `TreeId` plus a "graft" node in the parent whose `tree_id` matches. This suits embedded surfaces, such as a future WebView or a separately updated popup layer.
- `Role` has 182 variants. Useful ones: `Button, DefaultButton, CheckBox, RadioButton, Switch, TextInput, MultilineTextInput, PasswordInput, SearchInput, ComboBox, EditableComboBox, SpinButton, Slider, ScrollBar, ScrollView, List, ListItem, ListBox, ListBoxOption, Tree, TreeItem, Table, Row, Cell, ColumnHeader, RowHeader, Grid, GridCell, Tab, TabList, TabPanel, Menu, MenuBar, MenuItem, MenuItemCheckBox, MenuItemRadio, Toolbar, Tooltip, Dialog, AlertDialog, Alert, Status, Log, Terminal, Heading, Paragraph, Label, Link, Image, Document, Article, Feed, Code, Blockquote, Group, Pane, Window, GenericContainer, ProgressIndicator, Meter, Splitter`. **There is no `ToggleButton`, `Dial` or `Separator`.**
- `Action` has 22 variants: `Click, Focus, Blur, Collapse, Expand, CustomAction, Decrement, Increment, HideTooltip, ShowTooltip, ReplaceSelectedText, ScrollDown/Left/Right/Up, ScrollIntoView, ScrollToPoint, SetScrollOffset, SetTextSelection, SetSequentialFocusNavigationStartingPoint, SetValue, ShowContextMenu`.
- `Node` setters worth knowing: label, description, value, role_description, tooltip, placeholder, keyboard_shortcut, toggled, expanded, selected, `position_in_set`/`size_of_set`, `level`, `row_count/column_count/row_index/column_index/row_span/column_span` (+ `*_text`), `scroll_x/y` with `_min/_max`, `live`, `live_atomic`, `busy`, `orientation`, `text_selection`, `text_direction`, `language`, `modal`, plus action management (`add_action`, `child_supports_action`).

**Platform adapters** (README plus the 2026-09 releases):
| Platform | Crate | Status |
|---|---|---|
| Windows (UIA) | `accesskit_windows` | Mature. Single- and multi-line text edit supported. |
| macOS (NSAccessibility) | `accesskit_macos` | Mature. Same feature level. |
| Linux (AT-SPI over zbus) | `accesskit_unix` | Good; Image interface added 0.24. |
| Android | `accesskit_android` 0.9.0 | `InjectingAdapter` attaches an `AccessibilityDelegate` to an existing host `View` without Java changes (the lower-level `Adapter` is also available). Live regions + slider adjust added in 0.9.0. A third-party GPUI-mobile PR (Oct 2026) reports it working on an Android 14 emulator but only via a custom surface view, not NativeActivity. |
| iOS | `accesskit_ios` 0.2.1 | `Adapter` (you forward `isAccessibilityElement`, `accessibilityElements`, `accessibilityHitTest:`) or `SubclassingAdapter` (dynamic ObjC subclassing of an existing `UIView`). Only `aarch64-apple-ios`. |
| Web | planned | n/a. |

Gaps (README): no rich text/hypertext, so there is no per-run formatting and no embedded links/images inside text. Not every element type or property is covered. Assume lists, combo boxes, tabs and trees are still being hardened on newer platforms. Android and iOS were "planned, not started" in the 2022 roadmap post and depend on funding.

Since adapters are "provider-pushed", the right way to avoid cost is egui's pattern: `Adapter::update_if_active(|| update)` runs the closure only when an assistive technology is attached. With no AT running, the a11y pass costs nothing.

### 2.2 How egui integrates AccessKit, and its gaps

- Since egui 0.34, `accesskit` is a mandatory dependency (the cargo feature was removed, PR #7701). egui 0.36.2 pins `accesskit 0.25.0` and `accesskit_winit 0.34.0`.
- `Context::begin_pass` allocates a fresh root node; widgets lazily create nodes via `accesskit_node_builder`, which attaches each to the nearest ancestor with a node (default root) and skips widgets that aren't visible. State lives in `PassState::accesskit_state { nodes: IdMap<Node>, parent_map: IdMap<Id> }` and is reset every pass. The result goes out as `PlatformOutput.accesskit_update: Option<TreeUpdate>`, which egui-winit applies with `update_if_active`.
- Node id = `egui::Id::accesskit_id()`, a hash. Stable only if the widget `Id` is stable.
- Input side: AccessKit action requests arrive as `egui::Event::AccessKitActionRequest`. egui consumes `Click` (turned into a fake primary click) and the `ScrollIntoView`/`ScrollUp/Down/Left/Right` family. Other actions (increment/decrement, set-value, set-text-selection) are only partly wired.
- Recent egui a11y work: 0.34 scroll bars and splitters visible (#7804); 0.35 pressed state of selectable buttons (#8130) and an `egui_inspection` protocol that can read a running app's AccessKit tree (#8234); unreleased 0.37: panel resize handles, better popup/menu/modal/panel trees (#8581), invisible widgets hidden (#8621), `WidgetType` → `accesskit::Role` (#8555).
- **Gaps for Ljós:** no retained identity; no virtual list semantics; no live-region helper (the `events_description()` on `PlatformOutput` is a legacy screen-reader hook); no multi-tree use; mobile IME and AT wiring is absent from `egui-winit` (its Android path has no Android-specific handling, and iOS handling is limited to safe-area insets).

### 2.3 Masonry/Xilem (Linebender) as the architectural precedent

Masonry's accessibility pass (`masonry_core/src/passes/accessibility.rs`, read from the permissively licensed source):
- Walks the widget tree but returns early when `!state.needs_accessibility`. It builds a node only for widgets with `request_accessibility && !is_stashed`. Both flags are cleared after the visit and merged upward.
- Node id = `WidgetId` converted to `NodeId`. A separate window node is the root, and the root widget is its only child.
- A node is pushed before its children. Stashed (hidden) widgets get no node and are filtered out of the parent's child list. Focus = the focused widget's id, or the window node.
- Adds `Action::Focus` for focusable widgets, `Blur` for the focus target, and lets the widget's own `accessibility()` hook mutate the node.

This is the pattern to copy: **dirty flags that propagate to ancestors, id from stable widget identity, and a node pushed only when the item or its subtree changed.** One subtlety: because AccessKit requires a changed parent's full child list, any structural change dirties the parent's node as well as the child's.

### 2.4 QML `Accessible` and Qt Quick Controls

Source: doc.qt.io `Accessible` QML type. Properties: `name, description, role, id, focusable, focused, checkable, checked, checkStateMixed, pressed, selectable, selected, editable, readOnly, multiLine, passwordEdit, searchEdit, selectableText, expandable (6.12), expanded (6.12), ignored, defaultButton, labelFor (6.10), labelledBy (6.10)`. Signals: `pressAction, toggleAction, increaseAction, decreaseAction, scrollUp/Down/Left/RightAction, nextPageAction, previousPageAction, showOnScreenAction`. Method: `announce(message, politeness)` (6.8; politeness Polite|Assertive). `focusable` defaults false except for the roles CheckBox, RadioButton, Switch, Button, MenuItem, PageTab, EditableText, SpinBox, ComboBox, Terminal, ScrollBar. Guidance: a button's `name` is its visible text and must not repeat the control type.

Qt Quick Controls set roles by control type internally (Button → Button, CheckBox → CheckBox, Switch → Switch, TextField → EditableText, ComboBox → ComboBox, TabButton → PageTab, ItemDelegate → ListItem, etc.). The Qt docs don't enumerate these, so treat the defaults as **(verify)** with Qt's accessibility inspector against the qml-solid gallery.

**Role mapping table (QAccessible::Role → AccessKit `Role`)**

| QML `Accessible.*` | AccessKit | Notes |
|---|---|---|
| Button | `Button` (`DefaultButton` if `defaultButton`) | `pressAction` → `Action::Click`. A toggle button is a `Button` with `toggled`. |
| CheckBox | `CheckBox` | `checked`/`checkStateMixed` → `toggled` True/False/Mixed |
| RadioButton | `RadioButton` | selected state via `toggled` |
| Switch | `Switch` | |
| Slider | `Slider` | Increment/Decrement; `numeric_value`, min, max, step |
| Dial | `Slider` + `role_description("dial")` | No `Dial` role |
| SpinBox | `SpinButton` | |
| ScrollBar | `ScrollBar` | `orientation` |
| ComboBox | `ComboBox` (`EditableComboBox` if `editable`) | `expanded` via Expand/Collapse |
| EditableText | `TextInput`; `MultilineTextInput` if `multiLine`; `PasswordInput` if `passwordEdit`; `SearchInput` if `searchEdit` | `readOnly` → read-only flag |
| StaticText | `Label` | The default for `Text` items |
| Heading | `Heading` + `level` | Qt has no level property. Add one in Ljós (Ljós extension `Accessible.level`). |
| Paragraph, BlockQuote, Note | `Paragraph`, `Blockquote`, `Note` | |
| Link | `Link` | |
| Graphic, Animation, Chart | `Image` (Chart: `Image` + description) | Needs `name` |
| Canvas | `Canvas` | |
| List / ListItem | `List` / `ListItem` (`ListBox`/`ListBoxOption` when `selectable`) | `position_in_set`, `size_of_set` |
| Tree / TreeItem | `Tree` / `TreeItem` | `level`, `expanded` |
| Table, Row, Cell, ColumnHeader, RowHeader | `Table`, `Row`, `Cell`, `ColumnHeader`, `RowHeader` | `row_count`, `column_count`, indices |
| PageTab / PageTabList / PropertyPage | `Tab` / `TabList` / `TabPanel` | |
| MenuBar / PopupMenu / MenuItem | `MenuBar` / `Menu` / `MenuItem` (`MenuItemCheckBox`/`MenuItemRadio` when `checkable`) | |
| ButtonMenu, ButtonDropDown | `Button` + has-popup **(verify)** | |
| ToolBar / ToolTip / HelpBalloon | `Toolbar` / `Tooltip` | |
| ProgressBar | `ProgressIndicator` | |
| Indicator | `Meter` | |
| Dialog | `Dialog` (`AlertDialog` for message dialogs) + `modal` | |
| AlertMessage | `Alert` | Implies assertive live region |
| Notification, StatusBar | `Status` | Polite live region |
| Terminal | `Terminal` | |
| Window, TitleBar, Application | `Window`, `TitleBar`, `Application` | |
| Pane, Client, Assistant, LayeredPane | `Pane` / `GenericContainer` | |
| Grouping, Section, Form, Footer, ComplementaryContent | `Group`, `Section`, `Form`, `Footer`, `Complementary` | |
| Document, WebDocument | `Document` | |
| Separator, Splitter, Grip | `Splitter` for draggable; static separators are hidden | No static separator role |
| Equation | `Math` | |
| ColorChooser | `ColorWell` | |
| HotkeyField | `TextInput` + `keyboard_shortcut` | |
| Sound | `Audio` | |
| NoRole, Whitespace, Cursor, Caret | no node | Ignored; their children are promoted |
| UserRole | `Unknown` + `role_description` | |

State and signal mapping: `ignored` → don't emit a node and promote children; `focusable`/`focused` → `Action::Focus` + tree `focus`; `selectable`/`selected` → `selected`; `expandable`/`expanded` → Expand/Collapse actions + `expanded`; `labelledBy`/`labelFor` → `labelled_by`/`described_by` relations; `increaseAction`/`decreaseAction` ← `Increment`/`Decrement`; `scroll*Action` ← `ScrollUp/Down/Left/Right`; `showOnScreenAction` ← `ScrollIntoView`; `nextPageAction`/`previousPageAction` ← page-granularity scroll (no direct AccessKit action); `toggleAction` ← `Click` on a checkable. `announce()` is implemented through a live-region sink (see 3.2).

### 2.5 System accessibility preferences per platform

| Preference | Windows | macOS | iOS | Android | Linux |
|---|---|---|---|---|---|
| Reduced motion | `UISettings.AnimationsEnabled` / `SPI_GETCLIENTAREAANIMATION` (verify) | `NSWorkspace.accessibilityDisplayShouldReduceMotion` | `UIAccessibility.isReduceMotionEnabled` + change notification | `Settings.Global.ANIMATOR_DURATION_SCALE == 0` (the "remove animations" switch), or `ValueAnimator.areAnimatorsEnabled()` | **xdg-desktop-portal `org.freedesktop.appearance` `reduced-motion`** (0/1, documented), gsettings `enable-animations` as fallback |
| High contrast | `SPI_GETHIGHCONTRAST`, `UISettings.HighContrast` + system colour set | `accessibilityDisplayShouldIncreaseContrast` | `UIAccessibility.isDarkerSystemColorsEnabled` | no single flag; `UiModeManager.getContrast()` on API 34+ and the hidden high-text-contrast setting **(verify)** | portal `contrast` (0/1, documented) |
| Dark/light | `UISettings` colour values | `NSApp.effectiveAppearance` | `UITraitCollection.userInterfaceStyle` | `Configuration.uiMode` | portal `color-scheme` (0/1/2, documented) |
| Font scale | `UISettings.TextScaleFactor` | none system-wide | content size category (`UIFontMetrics`) | `Configuration.fontScale` (float, can exceed 1.3) | gsettings `text-scaling-factor` / KDE |
| Screen reader active | adapter `is_active` | adapter | adapter | `AccessibilityManager.isEnabled()` / `isTouchExplorationEnabled()`; `getRecommendedTimeoutMillis()` (API 29+) | AT-SPI activated by the adapter |

Qt's own mirror of these is `Qt.styleHints.accessibility.contrastPreference` (Qt ≥ 6.10; HAL-C2's bundled Basic style already uses it) and `colorScheme`. Ljós should expose the same names as QML-visible properties (`Qt.styleHints.accessibility.contrastPreference`, `Qt.styleHints.colorScheme`) plus its own `Qt.styleHints.accessibility.reducedMotion` and `fontScale`.

### 2.6 Keyboard and focus

**QML semantics to implement** (from the Qt Quick keyboard focus docs and the HAL-C2 audit):
- `Item.focus` marks the item that wants focus *within its nearest `FocusScope`*. `activeFocus` is true only if the item is focused within its scope **and** every ancestor scope is itself focused/active in the window. `forceActiveFocus(reason)` walks up and sets `focus` on each scope along the chain. `FocusScope` creates a focus-scope boundary so a reusable component can have a default focused child without knowing about its parents.
- Tab chain: depth-first over visible, enabled items with `activeFocusOnTab: true`. Controls set it true by default. `KeyNavigation.tab/backtab/left/right/up/down` override with explicit targets. `KeyNavigation.priority` controls ordering versus `Keys`.
- Key dispatch: first `ShortcutOverride` (HAL-C2 uses `Keys.onShortcutOverride` 18×), then registered `Shortcut`s (by `Qt.WindowShortcut`/`ApplicationShortcut` context), then `Keys.onPressed` on the focus item, bubbling to parents until `event.accepted`. `Keys.forwardTo` and `Keys.priority` (BeforeItem/AfterItem) alter order.
- `Shortcut`: `sequence`/`sequences`, `StandardKey.*` (platform-specific; HAL-C2 uses `StandardKey.Find` etc. 15×), `context`, `enabled`, `autoRepeat`, `onActivated`, `onActivatedAmbiguously`.
- Focus must also be reflected in the a11y tree (`TreeUpdate.focus`), and AT `Action::Focus` / `SetSequentialFocusNavigationStartingPoint` must move the QML focus.

**How this lands on egui:** egui has one `Memory::focus` id and implicit tab order by widget order. We **replace** its policy, not extend it: Ljós owns a `FocusTree`, translates egui `Event::Key`/`Text` into QML key events before widgets see them, and mirrors `activeFocusItem` into egui with `ctx.memory_mut(|m| m.request_focus(id))` for widgets drawn by egui itself. We also set egui's focus-lock filter on focused text items so Tab/arrows/Escape reach QML handlers instead of egui's navigation.

**IME** (see §1): winit 0.30 `Ime::Preedit(String, Option<(usize, usize)>)` carries byte offsets; `Commit` is preceded by an empty `Preedit`; `DeleteSurrounding {before_bytes, after_bytes}` exists in 0.31 beta; the app must apply events in order. winit 0.31 (beta.3, 2026-09) adds `Window::request_ime_update(ImeRequest)` with `ImeCapabilities` (Enable/Update/Disable, surrounding text). egui-winit 0.36 maps `Ime::Preedit` → `ImeEvent::Preedit` and `Commit` → `ImeEvent::Commit` and drives `set_ime_cursor_area`/`set_ime_purpose` from `PlatformOutput.ime`. Mobile is the gap: Android (winit) = soft keyboard toggle only; iOS (winit-uikit) = `UIKeyInput` only, no marked text. HAL-C2's mobile composer is a core feature, so mobile text input is a day-one requirement for the Android port, not a polish item.

**Text-editing feature needs** (from QML `TextInput`/`TextEdit`/`TextField`/`TextArea`): selection (mouse, keyboard, word/line, `selectByMouse`), undo/redo (`undo()`, `canUndo`), clipboard (cut/copy/paste, middle-click paste on X11/Wayland), `inputMask`, `validator` (`IntValidator`, `DoubleValidator`, `RegularExpressionValidator`), `echoMode` (Password, PasswordEchoOnEdit with `passwordMaskDelay` — the mobile brief reveal of the last typed char), `inputMethodHints` (HAL-C2: 30 uses: `ImhDigitsOnly`, `ImhFormattedNumbersOnly`, `ImhNoPredictiveText`, `ImhNoAutoUppercase`, `ImhUrlCharactersOnly`), `maximumLength`, `cursorPosition`, `preeditText`, `Qt.inputMethod` (visible, keyboardRectangle, show/hide).

### 2.7 Text: shaping, layout, painting

**Crates (crates.io 2026-10-10):** parley 0.12.0 (2026-10-09), fontique 0.12.0, harfrust 0.14.0, swash 0.2.10, skrifa 0.48.0, cosmic-text 0.19.0, unicode-bidi 0.3.18 (2024), unicode-linebreak 0.1.5 (2023), unicode-segmentation 1.13.3, icu_segmenter 2.3.0.

**egui's own text (0.36.2).** Layout is in `epaint`; deps are `harfrust`, `skrifa`, `vello_cpu`, `unicode-segmentation`, `unicode-properties`, `unicode-general-category`; `color_fonts` feature enables COLR/PNG glyphs via vello_cpu; `monochrome_emoji_fonts` ships a mono emoji fallback. Limitations:
- No bidi reordering. The open issue #5069 has no maintainer comment. #8576 (Arabic/Hebrew caret since 0.35, "rows hold more glyphs than characters") shows shaping runs ahead of cursor logic.
- Font fallback is by an ordered list per `FontFamily` (egui ≥ 0.37 adds a `FontProvider` backed by Fontique for system fonts; PR #8688, "Fix missing Chinese characters from system fonts on Windows", is a draft as of 2026-10-09).
- No `elide`, no text overflow policy beyond `LayoutJob::wrap.max_rows` + `break_anywhere`/overflow character.
- PR #5784 "Render text with Parley" (draft, by valadaptive): last push 2025-06, last comment 2025-11. Blocker: Parley's API expects a full rectangle; funding fell through. No sign of resumption. **Do not plan on egui adopting Parley.**

**Parley 0.12.0 / Fontique 0.12.0.** Key facts from docs.rs and the Parley changelog:
- `Layout`: shaping, line breaking, bidi reordering, alignment; re-break and re-align repeatedly without re-shaping (width changes), but any text/style change needs a new layout.
- Builders: `RangedBuilder`, `TreeBuilder` (nested spans, CSS `white-space-collapse`), `StyleRunBuilder`. Inline boxes: in-flow/out-of-flow/custom, floats and per-line exclusions (0.9). CSS-like `vertical-align`.
- `complex-scripts` feature: dictionary line/word breaking for CJK/Thai/Khmer/Lao/Myanmar (0.10); `CHROMIUM_LINE_BREAK_OVERRIDE` preset (0.11).
- Grapheme-cluster-aware carets/selection (0.12). `PlainEditor` has `raw_compose`/`is_composing`/`ime_cursor_area`, selection geometry, cursor geometry, `set_base_direction`, `generation()` for redraw tracking. No undo/redo or input-mask support found. The AccessKit feature was **removed** in 0.12; the integration is now an example.
- Fontique: backends fontconfig (Linux/FreeBSD, `fontconfig-dlopen` available), DirectWrite, CoreText (all Apple including iOS), Android via `roxmltree` parsing the system font config; script/locale-keyed fallback (`FallbackKey`); `FontInfoOverride` for `@font-face`-like registration; `SourceKind` for in-memory/file fonts (needed for app-bundled fonts and hot-reloaded font assets).
- Version alignment: egui 0.36.2 pins `harfrust 0.12.0`, `skrifa 0.44.0`, `fontique 0.11.1` (workspace). Parley 0.11.1 uses exactly harfrust 0.12 / skrifa 0.44; Parley 0.12 is newer than crates.io's latest harfrust 0.14/skrifa 0.48 neighbours. Use parley 0.11.1 against egui 0.36 for a single dependency graph, or accept duplicate skrifa/harfrust versions (binary size only, no API conflict, since they are separate crates).

**cosmic-text 0.19.0** (swash/harfrust-based, in COSMIC DE; has its own editor) is a viable alternative but couples layout, font DB and editing in one crate; inline-box and rich-span support is less flexible than Parley's, and Android system font discovery is weaker. Rejected as primary; keep as fallback if Parley's Android path disappoints.

**Painting Parley into egui.** Parley produces glyph runs (font, glyph id, position, size, synthesis). egui offers `Shape::Mesh` + user textures (`ctx.load_texture`/`tex_manager`) and `Shape::Callback` for wgpu. Options:
1. **Own glyph atlas (chosen).** Rasterise with skrifa outlines + the rasteriser egui already depends on (`vello_cpu`), into an R8/RGBA atlas keyed by `(font_id, glyph, size_px_bucket, subpixel_x, variation_hash)`; emit textured quads into a `Mesh`. Colour glyphs (COLR/CBDT/sbix) take the RGBA path. This is the approach egui itself uses for its internal atlas, but it takes an arbitrary glyph id instead of a `char`.
2. Tessellate outlines to `Shape::Path`: too slow for body text. Rejected.
3. Render text through Vello via `egui_wgpu` callbacks: heavy, and conflicts with egui's clip/z-order model. Rejected for now.
4. Keep egui's galley and use Parley only for measurement: yields two sources of truth, and still no bidi in painting. Rejected.

**Rich text.** QML `Text.textFormat`: `PlainText`, `StyledText` (HTML 3.2-ish strict subset: `b strong i u del s font(color,size 1–7) a img br p h1–h6 ol ul li pre`, all entities), `RichText` (HTML 4 subset), `MarkdownText` (CommonMark + GitHub tables and task lists; no code highlight box, no blockquote rule), `AutoText`. Links via `linkActivated`, `linkHovered`, `hoveredLink`, `linkAt(x, y)`. `elide` works only for Plain/Styled text, requires a width, and with wrapping only `ElideRight` with `maximumLineCount`/`height`. `horizontalAlignment` defaults to the text's natural direction; with mirroring, `effectiveHorizontalAlignment` shows the real value. Markdown: `pulldown-cmark 0.13.4` (MIT, CommonMark + tables/tasklists/strikethrough/footnotes, event-based and easy to run incrementally) is the parser; `comrak 0.56` is a heavier AST alternative.

**Selectable text.** QML `Text` isn't selectable; `TextEdit { readOnly: true; selectByMouse: true }` is (HAL-C2: 8 files). HAL-C2's `Markdown.qml` renders markdown by escaping and converting to HTML via a JS module, then shows each segment in a `TextEdit.RichText`, tracking a `selection` TextEdit and a `cited(selector)` signal with `{text,start,end,prefix,suffix}`. In Ljós a rich-text document must support selection **across blocks** natively, with a stable "as drawn" text offset space.

**Elide / ellipsis.** Not provided by Parley. Implement `elide` in Ljós: lay out single-line at infinite width; if it fits, done (fast path, cache by `(text hash, style, width)`). Otherwise binary-search the cut point on grapheme-cluster boundaries (Parley `Cluster`), build `prefix + "…"` (U+2026, `ElideLeft`/`ElideMiddle` analogous), choose the ellipsis' logical position from the paragraph's base direction, and re-layout once. HAL-C2 uses `elide` 460×, so it must be cheap and cached.

**Hyphenation.** Qt `Text` has none. Not needed for parity; `hypher` or `hyphenation` crates can be added later via a `Parley` hook (none now).

**Fonts on mobile.** Android: Fontique reads the system font XML (needs on-device verification for OEM fonts and `NotoColorEmoji`'s format). iOS: CoreText covers it. Bundle one UI font plus a mono font through `FontInfoOverride`/in-memory sources so layout is deterministic across platforms and testable in CI snapshots.

### 2.8 Localisation

**Qt API surface** (doc.qt.io `Qt` QML type, Writing Source Code for Translation, Localizing Applications, Translation Rules for Plural Forms, `.ts` format):
- `qsTr(source, disambiguation, n)`, `qsTranslate(context, source, disambiguation, n)`, `qsTrId(id, n)`; markers `QT_TR_NOOP`, `QT_TRANSLATE_NOOP`, `QT_TRID_NOOP`. `%n` is replaced by `n`; `%1..%9` through `String.arg()`.
- **Context = QML file name**; override with `pragma Translator` or `qsTranslate`. QML translation files need the `qml_` prefix with `qt_add_translations`. ID-based translations need `lrelease -idbased`.
- `Qt.uiLanguage`: settable, usable in bindings; with `QQmlApplicationEngine` changing it triggers `retranslate()`. Empty = no translation.
- `Qt.locale(name)` → `Locale` object (`dayName`, `monthName`, `toString(number, 'f', prec)`, `decimalPoint`, `currencySymbol`, `firstDayOfWeek`, `timeFormat/dateFormat`, `textDirection`, …); `Date.toLocaleString(locale, format)`, `Number.toLocaleString(locale, 'f', n)`, `Qt.formatDateTime/Date/Time(value, format|Locale.ShortFormat)` with Qt's own token language (`yyyy-MM-dd hh:mm:ss`, `AP`, `t`, quoted literals).
- **`.ts` XML:** `<TS version="2.1" language sourcelanguage>` → `<context><name/>` → `<message id numerus><location/><source/><comment/><extracomment/><translation type="unfinished|vanished|obsolete"><numerusform/>`. The docs warn the format may change. `.qm` is a binary format and is not documented in the public docs read here; we will not parse it.
- **Plurals:** Qt uses per-language rule tables of "up to three conditional rules plus *otherwise*" (English: `n==1`; Russian: `n%10==1&&n%100!=11`, `n%10∈2..4 && n%100∉10..20`; Japanese: only *otherwise*). The `.ts` `numerusform` order and Arabic/Slavic/Celtic mappings aren't specified on that page. **Do not assume CLDR's six categories map 1:1; validate with real fixtures (open question).**
- RTL: `LayoutMirroring.enabled` / `childrenInherit` mirror anchors (left↔right), `Row`/`Grid` order, `GridView`/horizontal `ListView`, and can be attached to a `Window`; item `x` coordinates are *not* mirrored. Typical use: `LayoutMirroring.enabled: Qt.application.layoutDirection === Qt.RightToLeft`. Text alignment default follows text direction; `effectiveHorizontalAlignment` reports the mirrored value.

**Rust options**

| Option | For | Against |
|---|---|---|
| ICU4X 2.3.x (`icu`, `icu_locale 2.3.1`, `icu_plurals 2.3.0`, `icu_decimal 2.3.0`, `icu_datetime 2.3.0`, `icu_collator 2.3.1`, `icu_segmenter 2.3.0`, `icu_provider_blob 2.3.0`) | Proper CLDR 48 data, locale negotiation/fallback, plurals, number/date/list/relative-time/collation, Rust-native, `icu4x-datagen` can build baked or blob data for a locale/marker subset; blob data can be swapped at runtime (hot update) | No arbitrary Qt-style date pattern strings (field-set based); no MessageFormat 2 crate found; data size must be measured and trimmed; MSRV 1.86 |
| `fluent-bundle 0.16.0` / `fluent 0.17.0` (+ `unic-langid 0.9.6`, `fluent-langneg 0.14.2`) | Grammar-aware messages with selectors | Key-by-ID, not source-text; releases from May 2025, little 2026 activity; plural logic duplicates CLDR; does not match the `qsTr("English text")` style used in 4,132 places |
| gettext (`gettext-rs 0.8.0` = bindings to libintl, not pure Rust; `rust-i18n 4.2.4` `t!`) | Familiar `.po` workflow | C dependency on mobile; plural expression evaluator; no QML integration |
| Read Qt `.ts` directly (`quick-xml 0.42`) | Zero friction for Linguist/translators; matches `qsTr` keys exactly | Needs our own runtime lookup + plural mapping; XML is slow to parse at startup (compile to binary) |

**Locale detection.** `sys-locale 0.3.2` (last release 2024-11) covers desktop + Android system locale; Android per-app language (API 33+) and iOS preferred-languages order need small JNI/ObjC shims. `icu_locale` supplies negotiation (`LocaleFallbacker`, `LocaleExpander`).

### 2.9 HAL-C2 QML audit (counts)

Counts via `grep` over `apps/desktop-qt` (944 `.qml` files, ~158.8k lines, including `build/`, `examples/`, `tests/` copies) and `apps/mobile-qt` (12 files, ~1.05k lines). Some counts are inflated by `build/` copies of the same files, so treat them as order-of-magnitude. The `desktop-qt` totals are:

| Area | desktop-qt | mobile-qt |
|---|---:|---:|
| `qsTr(` | **4,132** (212 with `.arg`, 3+ with `%n`) | 33 |
| `qsTranslate`/`qsTrId`/`QT_TR_NOOP`/`lupdate` wiring/`.ts`/`.qm` | 0 (none exist) | 0 |
| `Accessible.name` | **919** | 4 |
| `Accessible.role` | 86 (Button 43, Graphic 12, PageTab 6, ListItem 6, List 3, Heading 3, ComboBox 3, Client 3, StaticText 6) | 1 |
| `Accessible.ignored` | 26 | 0 |
| `Accessible.onPressAction` | 9 | 0 |
| `Accessible.description` | 3 | 0 |
| live regions / `Accessible.announce` / `contrastPreference` use in app QML | 0 | 0 |
| `FocusScope` | 8 | 0 |
| `forceActiveFocus` | 147 | 1 |
| `focus: true` | 24 | 1 |
| `activeFocusOnTab` | 19 | 0 |
| `KeyNavigation` | 18 | 0 |
| `Keys.*` handlers | 143 in 65 files (`onPressed` 39, `onEscapePressed` 22, `onReturnPressed` 21, `onShortcutOverride` 18, `onEnterPressed` 15, `onUp/Down` 12, `onReleased` 6, `onSpacePressed` 3) | 0 |
| `Shortcut` | 55 (e.g. `Ctrl+N`, `Ctrl+W`, `Ctrl+Shift+M`, `/`, `Escape`, `Ctrl+Return`, `StandardKey.Find`, data-driven `modelData.sequence`) | 1 |
| `StandardKey` | 15 | 0 |
| `TextField` / `TextEdit` / `TextArea` / `TextInput` | 305 / 198 / 47 / 41 | 2 / 0 / 0 / 0 |
| `selectByMouse` | 33 | 0 |
| `inputMethodHints` | 30 (digits, formatted numbers, no-predictive, no-auto-uppercase, URL) | 1 |
| `Qt.inputMethod` | 0 | 2 |
| `textFormat` | 52 (`MarkdownText` 6, `StyledText` 6, `RichText` 3, plus `TextEdit.RichText` in `Markdown.qml`) | 0 |
| `elide` | **460** | 3 |
| `wrapMode` | 647 | 9 |
| `font.pixelSize` / `font.pointSize` | **1,886 / 0** | 14 / 0 |
| `horizontalAlignment` | 176 | 7 |
| `LayoutMirroring` / `layoutDirection` / `Qt.uiLanguage` | 0 / 3 (one `Qt.RightToLeft` on a `Row` in `Timeline.qml:656`) / 0 | 0 |
| `Qt.locale()` / `toLocaleString` | 10 (`Locale.ShortFormat` date; `Number.toLocaleString(Qt.locale("en_US"), "f", 2)` for USD) | 0 |
| `ListView` / `Repeater` | 387 / 440 | 2 / 0 |
| `TapHandler` / `MouseArea` | 132 / 35 | 0 / 0 |

Observations that shape the design:
1. **Accessible.name is applied to ~919 items but `TapHandler`+`MouseArea` are 167**, so naming coverage is decent. There is no live-region/announce use at all, which is a real gap for a chat/agent UI, so Ljós should supply it as an improvement even before the port.
2. **Accessible roles used are a small set**: Button, Graphic, PageTab, ListItem, List, Heading, ComboBox, StaticText, Client. Core mapping table above covers them.
3. **All translation is source-text-keyed with `.arg()`**: 212 `.arg` calls and `%n` plurals mean `String.prototype.arg` and `%n` handling must exist in the JS engine from the first milestone.
4. **Pixel font sizes everywhere (1,886)**: font scaling must be a global multiplier applied in the text layer, not an `sp` unit that needs QML edits.
5. **Markdown path is custom** (`js/markdown.js` → HTML → `TextEdit.RichText`, streaming-aware: "a delta re-lays out only the last block"). Ljós' native `MarkdownText` + block-level incremental layout is both the replacement and an optimisation.
6. **Keyboard code is heavy** (143 `Keys.*` handlers, 55 `Shortcut`s, 147 `forceActiveFocus`). Focus semantics parity (`FocusScope`, `forceActiveFocus`, `ShortcutOverride`) is a compatibility requirement, not a nice-to-have.
7. **Virtualised content is pervasive** (387 `ListView`); the a11y tree for lists is a day-one design item.

---

## 3. Recommendations for Ljós

### 3.1 Accessibility architecture

**Decision A1 — AccessKit tree generated by a dedicated *a11y pass* over the retained item tree, only when an AT is attached.**
- Each item has `a11y: Option<A11yProps>` resolved from `Accessible.*` plus control-type defaults, and two bits `needs_a11y` (subtree) and `request_a11y` (this item). Changing a bound a11y property, geometry (bounds), visibility, or child list sets `request_a11y` on the item and, for structural changes, on the parent, then bubbles `needs_a11y` to the root (Masonry's pattern).
- The pass walks only `needs_a11y` subtrees, emits `(NodeId, Node)` for items with `request_a11y`, and always emits the parent with its full `children` when a child was added/removed/reordered. Offscreen/hidden-subtree items emit nothing and are removed from their parent's child list (AccessKit prunes them).
- Gate with `Adapter::update_if_active`; keep `needs_a11y` bits set while inactive so the first activation does one full-tree build.
- **NodeId** = 64-bit `(arena_index, generation)` for static items. For **delegates**, derive the id from `(list_node_id, model_key)` where `model_key` is the model's stable row key (fallback: index), *not* the recycled delegate instance. TalkBack/NVDA keep their virtual cursor on a node across scroll, so recycling must not change the id.
- Use `Role::GenericContainer` (no label, not focusable) for structural wrappers that carry no semantics; omit "ignored" items and promote their children.

**Decision A2 — Virtualised lists/tables.**
- `ListView` node = `List` (or `ListBox` if selectable) with `scroll_y`, `scroll_y_min/max`, `size_of_set`; each instantiated delegate gets `position_in_set = index + 1`, `size_of_set = model.count`. Tables: `row_count`, `column_count`, `row_index`, `column_index`.
- While `Adapter` is active, raise `cacheBuffer` (delegates created beyond the viewport) to a configurable ±N rows (default ±20 on desktop, ±10 on mobile) so a screen reader's "next item" has somewhere to go. This is cheap relative to a screen-reader session and is *not* paid when no AT is attached.
- Implement `ScrollDown/ScrollUp/ScrollIntoView/ScrollToPoint/SetScrollOffset` on the list. When the AT focus reaches the last materialised row, `ScrollIntoView`/page scroll materialises the next batch. For jump-to-end operations on huge models, surface a "Row 12,345 of 80,000" via `position_in_set` plus a `CustomAction` ("Go to latest"), and test with TalkBack/NVDA.
- Stream/chat lists (HAL-C2 Timeline) additionally keep the **last item** reachable via a synthetic "Jump to latest" button node.

**Decision A3 — Live regions and `Accessible.announce()`.**
- `announce(message, politeness)` writes to a hidden `Role::Status` node (polite) or `Role::Alert` (assertive) near the window root with `live = Polite|Assertive` and `live_atomic`. Updating its `label` with a new string each time triggers the platform announcement. (Android live regions: accesskit_android ≥ 0.9.0; confirm macOS/AT-SPI behaviour per adapter — verify.)
- Agent transcript container = `Role::Log` with `live = Polite`, `busy = true` *while streaming* (suppress per-token chatter), and flush at block boundaries or at most every ~1–2 s. The announcer coalesces into sentence/paragraph chunks. User setting: "Announce agent output: off / on completion / live".
- Streaming content nodes (paragraphs) are updated in place; do not recreate nodes per token.

**Decision A4 — Focus.** See 3.2. The a11y pass sets `TreeUpdate.focus` from the focus tree, and implements `Action::Focus/Blur/SetSequentialFocusNavigationStartingPoint` by moving the focus tree.

**Decision A5 — Preferences service.** One `SystemPrefs` struct exposing `color_scheme`, `contrast`, `reduced_motion`, `font_scale`, `screen_reader_active` as `Signal`s, with platform backends (Windows `UISettings` via `windows` crate, macOS/iOS via `objc2`, Android via JNI + `AccessibilityManager` listeners, Linux via the xdg-desktop-portal `Settings` interface — `org.freedesktop.appearance` keys `color-scheme`, `contrast`, `reduced-motion` are standardised). Wire:
- `reduced_motion` → global `AnimationClock::scale = 0` (Behaviors/Animations jump to end state; transitions complete in one frame); also exposed as `Qt.styleHints.accessibility.reducedMotion` for QML.
- `contrast` → `Qt.styleHints.accessibility.contrastPreference` (name-compatible with Qt's own Controls styles) and a Ljós high-contrast palette hook.
- `font_scale` → multiplier applied at text layout time to `font.pixelSize` (per-item opt-out `font.scaleWithSystem: false`).

**Decision A6 — a11y testing in CI** (egui_kittest 0.36.2 + kittest 0.5.0):
- Layer `ljos_test` over `egui_kittest::Harness`: QML snippet → mount → `harness.run()` → query by AccessKit (`get_by_role_and_label(Role::Button, "Send")`, `get_by_label_contains`, custom predicate `get_by(|n| …)`), plus our helpers `press_key`, `type_text`, `focus_next()`.
- Automated tree lints, executed on every mounted test component (a `ljos_test::a11y::audit(&tree)` function):
  1. every node exposing `Click` or `Focus` has a non-empty name or `labelled_by`;
  2. `Image`/`Graphic` has a name or is hidden;
  3. no duplicate sibling `(role, label)` among actionable nodes without `position_in_set` (confusing lists);
  4. interactive node bounds ≥ 44×44 dp on mobile profile, ≥ 24×24 dp on desktop (WCAG 2.2 target size);
  5. tab-order walk: pressing Tab N times visits every focusable node exactly once and cycles (no keyboard trap);
  6. contrast: for text nodes, compare resolved foreground/background (from the theme, not from pixels) against 4.5:1 (3:1 large text);
  7. virtualised lists expose `size_of_set` on all rows.
- Static QML lint with the qml-solid parser: `MouseArea`/`TapHandler`/`Keys` item without `Accessible.name` and without a descendant `Text`; `Image` without `Accessible.name` or `Accessible.ignored`; use of `qsTr` with non-literal argument; missing `Accessible.role` on custom clickable items.
- Manual/automated platform matrix (outside unit CI): NVDA + Narrator + Accessibility Insights (Windows), VoiceOver + Accessibility Inspector (macOS/iOS), Orca + `accerciser`/`atspi` scripts (Linux), TalkBack + Accessibility Scanner/ATF (Android).

### 3.2 Focus, keys, IME

**Decision F1 — Ljós owns focus and key dispatch; egui is a *rendering and raw-input* layer.** Types (sketch):

```rust
pub struct FocusTree {
    scopes: SlotMap<ScopeId, FocusScopeData>,   // FocusScope boundaries
    active_window: bool,
    active_item: Option<ItemId>,                // == Window.activeFocusItem
}
struct FocusScopeData { owner: ItemId, focus_child: Option<ItemId> }

pub enum FocusReason { Tab, Backtab, Mouse, Shortcut, Popup, Other, ActiveWindow, Programmatic }

pub struct KeyEvent { key: Key, text: SmolStr, modifiers: Modifiers, is_auto_repeat: bool,
                      native_scan: u32, accepted: Cell<bool> }

pub enum KeyDispatchStage { ShortcutOverride, Shortcut, BeforeItem, Item, AfterItem, Forward }
```

- Dispatch: build a `KeyEvent` from `egui::Event::Key/Text/Ime`, run `ShortcutOverride` up the focus chain, then match registered `Shortcut`s (context-aware, ambiguity → `activatedAmbiguously`), then deliver `Keys.onPressed` along the focus chain honouring `Keys.priority`/`forwardTo`. Unaccepted keys bubble. Accepted keys are removed from egui's input so egui widgets don't double-handle them.
- `activeFocus` is a derived, cached property: recomputed only for items on the old and new focus chains (no global recompute).
- Tab navigation: a precomputed per-scope `tab_order` (DFS over visible, enabled, `activeFocusOnTab` items, `KeyNavigation` overrides), invalidated on structure changes, not per frame.
- `StandardKey` = table of platform sequences (Windows/Linux Ctrl, macOS Cmd; `Find`, `Copy`, `Paste`, `Undo`, `Redo`, `SelectAll`, `Quit`, `Close`, …).

**Decision F2 — Text input architecture.**
```rust
pub trait TextInputClient {            // implemented by TextInput/TextEdit items
    fn surrounding(&self) -> TextSnapshot;          // text, selection, preedit range (UTF-8)
    fn replace(&mut self, range: Range<usize>, text: &str, composing: Option<Range<usize>>);
    fn set_selection(&mut self, a: usize, b: usize);
    fn cursor_rect(&self) -> Rect;                   // for IME candidate window
    fn hints(&self) -> InputMethodHints;            // maps Qt.ImhXxx
}
pub trait PlatformTextInput {          // one backend per platform
    fn activate(&mut self, client: &dyn TextInputClient);
    fn update(&mut self, client: &dyn TextInputClient);
    fn deactivate(&mut self);
}
```
- Desktop backend = winit `Ime` events (0.30.13 now; migrate to 0.31's `ImeRequest/ImeCapabilities` when it releases, which also adds surrounding-text and `DeleteSurrounding`).
- Android backend = `android-activity` 0.6.1 with the **GameActivity** feature: `AndroidApp::set_text_input_state`/`text_input_state`, `set_ime_editor_info(input_type, action, options)`, `show_soft_input`/`hide_soft_input`; the platform sends back `TextInputState { text, selection, compose_region }`. This is the shortest path to composing-region/autocorrect semantics. Fallback plan: a small Kotlin `InputConnection` view if GameTextInput proves inadequate (e.g., for Gboard cursor-control gestures or per-character deletion). This must be built by us: neither winit nor egui provides it natively.
- iOS backend = a `UIView` subclass conforming to `UITextInput` (`setMarkedText`, `insertText`, `selectedTextRange`, `firstRect(for:)`) via `objc2`, placed over the wgpu surface. Staged after Android.
- Until mobile IME is ready, `TextInput` on mobile can fall back to egui's web-only path (not applicable) → **there is no stop-gap; schedule the Android IME bridge in the first mobile milestone**.

**Decision F3 — Text editing core.** `TextModel` (UTF-8 `String` + gap or rope for large `TextArea`) with an operation-log undo stack (coalescing typing runs, selection snapshots), `Validator`/`InputMask` trait objects, `EchoMode::{Normal, Password, PasswordEchoOnEdit, NoEcho}` with a mask-delay timer from `SystemPrefs`, clipboard via `arboard` (desktop) and JNI/ObjC (mobile). Layout/selection/caret geometry via Parley `Layout` + `Cursor`/`Selection` (grapheme aware, bidi aware), not `PlainEditor` as the model owner (it has no undo/mask/validator hooks). Keep `PlainEditor` as a reference/starting point and consider upstreaming gaps.

### 3.3 Text stack

**Decision T1 — Parley (0.11.1 or 0.12.0) + Fontique + HarfRust for shaping/fallback; Ljós-owned painter.**
```rust
pub struct TextEngine {                 // one per app; shared caches
    fonts: parley::FontContext,         // Fontique collection + SourceCache
    layout_cx: parley::LayoutContext<Brush>,
    layouts: LruCache<LayoutKey, Arc<ParleyLayout>>,   // key: (text hash, style hash, width bucket, scale, locale)
    atlas: GlyphAtlas,                  // skrifa outlines -> vello_cpu raster -> egui TextureHandle(s)
}
pub struct TextItemState {              // per visible Text item
    layout: Option<Arc<ParleyLayout>>,  // dropped when item goes offscreen
    metrics: TextMetrics,               // implicitWidth/Height retained even when offscreen
    elide_cache: Option<ElideResult>,
}
```
- **Off-screen discipline:** keep only `implicit_size` for hidden/offscreen text; free the `Layout` after N frames offscreen (LRU); never rasterise glyphs for clipped-out runs; glyph atlas has an eviction budget (e.g., 16–32 MB desktop, 8 MB mobile) and is per-scale.
- **Locale-aware fallback:** pass the *content* language (BCP-47, via `lang` property or detection by script) when shaping so CJK unification picks the right font and Fontique's `FallbackKey` picks the right script/locale fallback. Provide `Text.lang` (Ljós extension) defaulting to the UI language.
- **Features:** `parley/system`, `parley/complex-scripts` (default on desktop, optional on mobile because dictionaries enlarge the binary — measure).
- **Colour fonts / emoji:** atlas RGBA path for COLR/sbix/CBDT; bundle Noto Color Emoji (or Twemoji COLR) in tests for deterministic CI snapshots; use system emoji at runtime.
- **Rejected:** cosmic-text (less flexible spans/inline boxes, weaker Android discovery); staying on egui's `Galley` (no bidi, #5069 open ≥ 2 years); waiting for egui→Parley (#5784 stalled, maintainers' own blocker).
- **MVP stop-gap:** egui `Label`/`TextEdit` through a thin adapter lets the first demos run; mark as "LTR-only, no bidi" and replace before any RTL/Arabic/Hebrew/Indic claim.

**Decision T2 — Rich text.**
```rust
pub enum Block { Paragraph(Inlines), Heading{level:u8, inlines:Inlines}, Quote(Vec<Block>),
                 Code{lang:Option<SmolStr>, text:Arc<str>}, List{ordered:bool, items:Vec<Vec<Block>>},
                 Table{head:Vec<Inlines>, rows:Vec<Vec<Inlines>>}, Rule, Image{src:Url, alt:SmolStr} }
pub struct RichDoc { blocks: Vec<Arc<BlockEntry>>, text_len: usize }   // "as drawn" offset space
struct BlockEntry { block: Block, hash: u64, layout: Option<Arc<ParleyLayout>>, y: f32, h: f32 }
```
- Three front ends produce `RichDoc`: `MarkdownText` (`pulldown-cmark` 0.13.4, with tables/tasklists/strikethrough), `StyledText` (strict tag subset from Qt docs: b strong i u del s font a img br p h1–h6 ol ul li pre), `RichText` (superset, ignore unknown). HTML-subset parser: a small tokenizer (no `html5ever`), never produces raw HTML passthrough (HAL-C2 treats the text as untrusted).
- **Streaming:** `RichDoc::update(new_text)` re-parses from the last unfinished block (the last block's byte offset is remembered); earlier `BlockEntry`s keep their hash and cached layout. This is exactly the optimisation HAL-C2's JS does by hand.
- **Selection:** `SelectionScope` spans many blocks / many `Text` items (even virtualised) with a document-order offset space; `selectedText`, `selectionStart/End`, `positionAt(x,y)`, `linkAt(x,y)` exposed to QML so HAL-C2's "Cite selection" `{text,start,end,prefix,suffix}` can port directly.
- **A11y for rich text:** AccessKit has no rich-text/hypertext support, so each block is a node (`Paragraph`/`Heading`+level/`ListItem`/`Blockquote`/`Code`/`Table`/`Cell`/`Link`/`Image`), with text in `label`/`value`. Don't expose per-run formatting. Selection exposed through `text_selection` only for editable text.
- **Elide:** implemented as in §2.7; document that, like Qt, it applies to single-style text and needs a width.

### 3.4 Localisation

**Decision L1 — Qt-compatible catalog model, own runtime format, ICU4X for rules and formatting.**
```rust
pub struct MsgKey<'a> { context: &'a str, source: &'a str, disambiguation: &'a str }   // id optional
pub enum Entry { Single(Box<str>), Plural(SmallVec<[Box<str>; 6]>) }                  // CLDR-category slots after import
pub struct Catalog { lang: LanguageTag, map: PerfectHashMap<MsgKey, Entry>, plural: PluralTable }
pub struct I18n {
    chain: Vec<Arc<Catalog>>,          // negotiated fallback chain, e.g. [is-IS, is, en]
    revision: Signal<u64>,             // bumped on language change; every tr() reads it
    ui_language: Signal<LanguageTag>,  // backs Qt.uiLanguage
    locale: Signal<Arc<LocaleData>>,   // ICU4X formatters for Qt.locale()
}
impl I18n {
    pub fn tr(&self, ctx: &str, src: &str, dis: &str, n: Option<i64>) -> Cow<'_, str>;
}
```
- `qsTr("…")` compiles (in the QML→IR step) to `I18n::tr(file_context, …)`, where `file_context` = the QML file stem (Qt's rule), or the `pragma Translator` value. The call **reads `I18n::revision`** through the binding dependency tracker, so every binding containing `qsTr` is invalidated on language change with no explicit retranslation walk. Because Ljós evaluates bindings lazily and the item tree knows what's on screen, only visible items re-evaluate immediately; hidden ones are marked dirty and evaluate when shown (this directly serves the off-screen requirement; HAL-C2 has ~4.1k `qsTr`).
- `Qt.uiLanguage` setter swaps `chain`, bumps `revision`, and updates `Qt.application.layoutDirection` from `locale.text_direction()` (ICU4X `icu_locale` direction data); apps may override. `LayoutMirroring` is pure layout (3.5).
- **Placeholders:** `%n` via plural selection, `%1..%9` via `String.prototype.arg()` (QString::arg semantics: replace the lowest-numbered remaining placeholder) implemented as an rquickjs prototype extension (212 occurrences in HAL-C2). Optionally `arg(number, fieldWidth, fmt, prec)` overloads later.
- **Plural mapping:** at import, convert each `.ts` `<numerusform>` list into CLDR slots using a per-language table built from CLDR rules and **validated against real Qt-generated `.ts` fixtures** (Qt's "≤3 rules + otherwise" model differs from CLDR 6 categories). At runtime select with `icu_plurals::PluralRules::try_new_cardinal(locale)` → `PluralCategory` → slot. Qt-style `%n` substitution uses `icu_decimal` so `%n` is localised digits/grouping, if the user opts in; default matches Qt (plain integer).
- **Formats:**
  - *Source of truth for translators:* Qt `.ts` (Linguist/Weblate/Crowdin all read it); Ljós reads/writes it with `quick-xml` 0.42 (round-trip preserving `location`, `extracomment`, `translatorcomment`, `type`).
  - *Runtime:* a compact binary `.ljl` (sorted-key table or FST + string pool, `postcard`/hand-rolled, mmap-able) built by `ljos i18n compile`. In dev builds, read `.ts` directly and watch for changes (hot code/translation update with no restart; also fits the hot-update requirement).
  - We will **not** read `.qm`: binary format is not publicly specified in the docs reviewed, and parsing it would drift into Qt's source. A `.ts` is always available to translators.
- **Extraction:** `ljos i18n extract` reuses qml-solid's Oxc QML parser: walk AST for calls `qsTr/qsTranslate/qsTrId/QT_TR_NOOP/QT_TRANSLATE_NOOP/QT_TRID_NOOP` with string-literal args, honour `//:` (extracomment), `//~` (meta), `//=` (id), `pragma Translator`, plus Ljós' own `tr()` aliases; emit/merge `.ts` with `unfinished`/`vanished` handling, `location` lines, and a `--check` mode for CI (fails on non-literal first arguments and new untranslated strings). qml-solid already references `qsTr` in `resolve.rs`; extend rather than reimplement.
- **Locale / formatting (`Qt.locale()`):** build `LocaleData` from `icu_locale` + `icu_decimal::DecimalFormatter` + `icu_datetime` (month/day names via `FixedCalendarDateTimeNames`, field-set formatters for `Locale.ShortFormat/LongFormat/NarrowFormat`) + `icu_collator` for `Locale.collation`/sorting. Qt's token strings (`yyyy-MM-dd hh:mm`) are implemented by our own small formatter that pulls names from ICU4X data (ICU4X has no free-form Qt pattern API). `toLocaleString(locale, 'f', n)` → `icu_decimal`. Currency: symbol table from `icu` (or minimal), formatting via decimal + placement rule.
- **ICU4X data:** use `compiled_data` for dev and a `BlobDataProvider` (`icu_provider_blob 2.3.0`) generated by `icu4x-datagen` restricted to the app's shipped locales and only the needed markers (plurals, decimal, datetime, collator, segmenter/line/word/grapheme, locale fallback/direction). Measure binary-size deltas on Android arm64 before choosing blob vs baked; ICU4X docs state the data-trimming workflow but I found no published per-feature size numbers (open question). Blob data is also the hot-update path for locale data.
- **Rejected:** Fluent as the primary (ID-keyed, mismatched with `qsTr(source)`; adopting it forces rewriting 4.1k call sites); gettext (C dependency on mobile, poor QML story); ICU MessageFormat 2 (no ICU4X crate found; revisit when stable); direct `.qm` loading (see above).

**Decision L2 — RTL/mirroring.** Resolve an `effective_mirrored: bool` per item at layout time (`LayoutMirroring.enabled`, `childrenInherit`, cascade cached; default from `Qt.application.layoutDirection`). Layout engine applies it to anchors (left↔right), `Row`/`Grid`/`Flow` order, horizontal `ListView`/`GridView`; not to raw `x`. Text: alignment `Start/End` resolved against Parley's base direction (`set_base_direction`) and mirroring; expose `Text.effectiveHorizontalAlignment`. AccessKit `text_direction`/`language` set per text node. Add RTL snapshot tests (Hebrew/Arabic strings in Parley, plus a mirrored layout) to the gallery comparison against real Qt.

### 3.5 Phasing

1. **M0:** `SystemPrefs`, `FocusTree`, key dispatch, a11y pass (static roles, names, actions) for desktop, kittest-based audit lib. Role table above as data (`phf` map).
2. **M1:** Parley text engine + glyph atlas, plain `Text`, `elide`, `TextInput`/`TextEdit` on Parley with desktop IME, `qsTr` + `.ts` import + `Qt.uiLanguage` live switch, `String.arg`, `%n`.
3. **M2:** Android: AccessKit Android adapter integration (with GameActivity view), GameTextInput bridge, font scale, TalkBack test pass. Rich text / Markdown with streaming + cross-block selection (HAL-C2 chat).
4. **M3:** iOS (UITextInput, accesskit_ios), RTL/mirroring, ICU4X locale formatting, virtualised-list a11y hardening, live-region tuning with HAL-C2 agent output.

---

## 4. Risks and open questions

1. **AccessKit on Android/iOS is young** (0.9.x / 0.2.x). Real-world coverage of `ListView`, combo boxes, text editing, and live regions on TalkBack/VoiceOver needs spikes early. The Android `InjectingAdapter` needs a `View`; the GameActivity/NativeActivity wiring is unproven publicly (a third-party PR tested only a custom surface view). Mitigation: a spike in M2 week 1; budget for upstream fixes (MIT/Apache — we can contribute).
2. **Mobile IME is not provided by winit or egui.** The GameTextInput bridge and a `UITextInput` view are real engineering (composing regions, autocorrect, cursor gestures, handwriting, Samsung/Gboard quirks). Highest-risk item for the HAL-C2 mobile composer.
3. **winit 0.31 timing** (beta.3 as of 2026-09; egui pins 0.30.13). IME API changes (`ImeRequest`) arrive with it; accesskit_winit tracks it. Plan an abstraction (`PlatformTextInput`) so we are not blocked.
4. **Parley API shape vs egui's immediate-mode layout.** egui's own Parley attempt stalled on that. Ljós sidesteps by owning layout; the cost is our own painter, atlas, hit-testing and clipping, plus keeping up with Parley 0.x churn (0.9→0.12 in 6 months, MSRV 1.88, new `parley_engine`/`parlance` crates). Pin versions; gate upgrades with the render-comparison gallery.
5. **Version skew** between egui 0.36's pinned `skrifa 0.44`/`harfrust 0.12` and Parley's newer pins (duplicate crates, binary size). Pin Parley 0.11.1 against egui 0.36, or upgrade with egui 0.37.
6. **Text performance**: ~460 `elide`d and ~1,900 sized texts per HAL-C2 screen set → layout caching, width-bucketing and offscreen release must be designed up-front. Needs benchmarks on a low-end Android device.
7. **Android/iOS system font coverage** (Fontique's Android backend parses system font XML; OEM variance, colour emoji formats, Indic/Arabic fallback). Needs on-device tests; fallback = bundled Noto subsets (size trade-off).
8. **ICU4X data size on mobile is unmeasured here.** `complex-scripts` dictionaries (Parley) and datetime/collator data can be megabytes. Needs size budget per feature; use blob + datagen trimming.
9. **Qt `.ts` plural mapping**: Qt's rule tables ("up to three rules + otherwise") vs CLDR categories; `numerusform` ordering is not specified in the pages I could read. Needs real-world fixtures (including Arabic, Slavic, Celtic) before declaring `.ts` interop.
10. **Qt Quick Controls' default roles/states** are inferred, not documented in the pages read. Validate against the qml-solid gallery with a platform inspector.
11. **Live region behaviour across platforms** (UIA `LiveRegionChanged`, AT-SPI, NSAccessibility announcement, Android) differs, and screen readers throttle. Needs a streaming-output prototype with the real HAL-C2 timeline early.
12. **Accessibility of selection/rich text**: AccessKit has no hypertext, so link navigation inside paragraphs (screen reader rotor "links") is limited. Mitigation: expose each link as a child `Link` node when a paragraph contains links.
13. **Key-model compat**: QML's `Keys`/`Shortcut`/`FocusScope` edge cases (ambiguous shortcuts, `ShortcutOverride`, `forceActiveFocus` in popups) are subtle; build a conformance test-suite from HAL-C2's 143 `Keys.*` uses and Qt's documented examples.
14. **Legal/clean-room:** keep all Qt-behaviour specs derived from docs; for `.ts` format we use the documented XML format; do not consult `qtbase`/`qtdeclarative`/`qttools` sources for plural tables — derive from CLDR and verify with black-box fixtures produced by running `lupdate`/`lrelease` as tools.

---

## 5. Sources

AccessKit
- AccessKit README: https://raw.githubusercontent.com/AccessKit/accesskit/main/README.md
- AccessKit releases (Aug–Sep 2026): https://github.com/AccessKit/accesskit/releases
- AccessKit "Looking back; looking forward": https://accesskit.dev/looking-back-looking-forward/
- accesskit 0.25.1 docs: https://docs.rs/accesskit/0.25.1/accesskit/struct.Node.html , `.../enum.Role.html`, `.../enum.Action.html`, `.../struct.TreeUpdate.html
- accesskit_android 0.9.0: https://docs.rs/accesskit_android/0.9.0/accesskit_android/
- accesskit_ios 0.2.1: https://docs.rs/accesskit_ios/0.2.1/accesskit_ios/
- GPUI-mobile Android AccessKit PR (third-party evidence): https://github.com/longbridge/gpui-mobile/pull/25
- AccessKit `accesskit_adapter` / winit 0.31 PR: https://github.com/AccessKit/accesskit/pull/656
- Masonry accessibility pass: https://raw.githubusercontent.com/linebender/xilem/main/masonry_core/src/passes/accessibility.rs
- kittest `Queryable`: https://docs.rs/kittest/0.5.0/kittest/trait.Queryable.html ; egui_kittest: https://docs.rs/egui_kittest/0.36.2/egui_kittest/

egui / winit / Android / iOS
- egui workspace manifest: https://raw.githubusercontent.com/emilk/egui/master/Cargo.toml ; epaint manifest: https://raw.githubusercontent.com/emilk/egui/master/crates/epaint/Cargo.toml
- egui CHANGELOG: https://raw.githubusercontent.com/emilk/egui/master/CHANGELOG.md
- egui `context.rs`, `pass_state.rs`: https://raw.githubusercontent.com/emilk/egui/master/crates/egui/src/context.rs , https://raw.githubusercontent.com/emilk/egui/master/crates/egui/src/pass_state.rs
- egui-winit: https://raw.githubusercontent.com/emilk/egui/master/crates/egui-winit/src/lib.rs
- `PlatformOutput` 0.36.2: https://docs.rs/egui/0.36.2/egui/struct.PlatformOutput.html
- egui PR #5784 (Parley, draft): https://github.com/emilk/egui/pull/5784 ; issue #5069 (bidi): https://github.com/emilk/egui/issues/5069 ; issue search (bidi/RTL): https://github.com/emilk/egui/issues?q=is%3Aissue+bidi+OR+RTL+OR+%22right-to-left%22
- winit `Ime` 0.31.0-beta.3: https://docs.rs/winit/0.31.0-beta.3/winit/event/enum.Ime.html
- winit Android backend (IME status): https://raw.githubusercontent.com/rust-windowing/winit/master/winit-android/src/event_loop.rs
- winit UIKit view (UIKeyInput only): https://raw.githubusercontent.com/rust-windowing/winit/master/winit-uikit/src/view.rs
- android-activity (text input API): https://docs.rs/android-activity/latest/android_activity/struct.AndroidApp.html , https://docs.rs/android-activity/latest/android_activity/input/index.html
- xdg-desktop-portal Settings: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html
- Android `AccessibilityManager` (details recalled from general knowledge, page not retrievable): https://developer.android.com/reference/android/view/accessibility/AccessibilityManager

Text
- Parley docs: https://docs.rs/parley/0.12.0/parley/ ; `PlainEditor`: https://docs.rs/parley/0.12.0/parley/editing/struct.PlainEditor.html
- Parley manifest + changelog: https://raw.githubusercontent.com/linebender/parley/main/parley/Cargo.toml , https://raw.githubusercontent.com/linebender/parley/main/CHANGELOG.md
- Fontique docs + manifest: https://docs.rs/fontique/0.12.0/fontique/ , https://raw.githubusercontent.com/linebender/parley/main/fontique/Cargo.toml
- crates.io versions queried 2026-10-10 via https://crates.io/api/v1/crates/<name>

Qt (documentation only)
- Accessible QML type: https://doc.qt.io/qt-6/qml-qtquick-accessible.html
- QAccessible roles/events: https://doc.qt.io/qt-6/qaccessible.html
- Qt Quick accessibility overview: https://doc.qt.io/qt-6/accessible-qtquick.html
- QStyleHints: https://doc.qt.io/qt-6/qstylehints.html
- Qt QML type (qsTr, uiLanguage, locale): https://doc.qt.io/qt-6/qml-qtqml-qt.html
- LayoutMirroring: https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
- Text QML type: https://doc.qt.io/qt-6/qml-qtquick-text.html
- TS file format: https://doc.qt.io/qt-6/linguist-ts-file-format.html
- Plural rules: https://doc.qt.io/qt-6/i18n-plural-rules.html
- Writing source code for translation: https://doc.qt.io/qt-6/i18n-source-translation.html ; Localizing applications: https://doc.qt.io/qt-6/localization.html ; Using lupdate: https://doc.qt.io/qt-6/linguist-lupdate.html ; qt_add_translations: https://doc.qt.io/qt-6/qtlinguist-cmake-qt-add-translations.html ; Internationalization overview: https://doc.qt.io/qt-6/internationalization.html

ICU4X / i18n
- ICU4X changelog: https://raw.githubusercontent.com/unicode-org/icu4x/main/CHANGELOG.md
- ICU4X data management tutorial: https://icu4x.unicode.org/2_0/tutorials/data-management/
- ICU4X 2.2 announcement (CLDR 48.2): https://blog.unicode.org/2026/04/
- fluent-rs repo listing (activity data): https://www.goodfirstissue.org/projectfluent/fluent-rs

Local audits
- HAL-C2 QML counts: `grep` over `/home/olafura/dev/hal-c2/apps/desktop-qt` and `/apps/mobile-qt` (sections 2.9); notable files `qml/HalC2/Bricks/Markdown.qml`, `Timeline.qml`, `PullRequestReviewPanel.qml`, `ProjectRemovalDialog.qml`, `DiffPanel.qml`.
- qml-solid parser/resolver: `/home/olafura/dev/qml-solid/crates/qml_solid/src/resolve.rs` (existing `qsTr` handling).
