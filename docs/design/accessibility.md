# Accessibility

A screen reader, a switch, voice control and a keyboard see your interface through its accessibility tree and its tab walk, not its pixels. Each rule below is about what that tree says or where Tab goes. Contrast is in color.md, focus drawing in interaction.md.

In QML: a control (`Button`, `CheckBox`, `ItemDelegate`…) names itself from `text`; a plain `Item` or `Rectangle` needs `Accessible.role` and `Accessible.name`; decoration sets `Accessible.ignored: true`.

## Rules

<a id="a11y-unnamed-interactive"></a>
### `a11y-unnamed-interactive`

**Error, static and runtime.** Something reacts to a click, a tap or a key and has no accessible name. A screen reader says "button" and nothing else; voice control cannot address it.

```qml
// Bad: a clickable rectangle no assistive technology can name
Rectangle {
    width: 32; height: 32
    TapHandler { onTapped: player.togglePlay() }
}
// Good
Rectangle {
    width: 32; height: 32
    Accessible.role: Accessible.Button
    Accessible.name: player.playing ? qsTr("Pause") : qsTr("Play")
    Accessible.onPressAction: player.togglePlay()
    TapHandler { onTapped: player.togglePlay() }
}
```

Better still, use a `Button`; it has the role, the keys and focus already.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html , https://doc.qt.io/qt-6/qml-qtquick-accessible.html , axe `button-name`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md

<a id="a11y-unnamed-icon-button"></a>
### `a11y-unnamed-icon-button`

**Error, static and runtime.** A button shows only an icon and has no `text`, `action` or accessible name. The icon means something to the designer and nothing to a screen reader.

```qml
// Bad
ToolButton { icon.name: "edit-delete"; onClicked: rows.removeSelected() }
// Good: the text names it, the display hides it
ToolButton {
    text: qsTr("Delete selected rows")
    display: AbstractButton.IconOnly
    icon.name: "edit-delete"
    ToolTip.text: text
    ToolTip.visible: hovered || visualFocus
    onClicked: rows.removeSelected()
}
```

Checked by: the runtime audit, on a button with an icon whose text is empty or hidden by `display: AbstractButton.IconOnly` and whose node has no name. It says this instead of `a11y-unnamed-interactive`.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/non-text-content.html , https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#display-prop

<a id="a11y-unnamed-image"></a>
### `a11y-unnamed-image`

**Warning, static and runtime.** An image is neither named nor left out of the tree, so a screen reader stops on "image". Name the images that carry meaning; leave decoration out.

```qml
// Bad
Image { source: "status-ok.svg" }
// Good: meaning has a name, decoration is ignored
Image { source: "status-ok.svg"; Accessible.name: qsTr("Connected") }
Image { source: "wave.svg"; Accessible.ignored: true }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/non-text-content.html , axe `image-alt`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md

<a id="a11y-duplicate-name"></a>
### `a11y-duplicate-name`

**Warning, runtime.** Two actionable siblings have the same name. A list of controls read aloud ("Retry, Retry") gives no way to choose.

```qml
// Bad
Row { Button { text: qsTr("Retry") }  Button { text: qsTr("Retry") } }
// Good
Row { Button { text: qsTr("Retry upload") }  Button { text: qsTr("Retry download") } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/headings-and-labels.html , https://www.w3.org/WAI/WCAG22/Understanding/link-purpose-in-context.html

<a id="a11y-ambiguous-name"></a>
### `a11y-ambiguous-name`

**Warning, runtime.** Two actionable items anywhere in one window share a name but do different things (a "More" button on every row). Name each by what it acts on; keep the visible text short if you must and add the object to the name.

```qml
// Bad, in a delegate
ToolButton { text: qsTr("More"); display: AbstractButton.IconOnly; icon.name: "overflow" }
// Good
ToolButton {
    text: qsTr("More options for %1").arg(model.title)
    display: AbstractButton.IconOnly; icon.name: "overflow"
}
```

Checked by: the runtime audit, on two enabled controls of one role and one name anywhere in the window. A row, cell, menu or dialog around one gives it its context, as a list's rows do, so those are left out; two siblings are `a11y-duplicate-name`'s, and two that share one `action` do the same thing.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/link-purpose-in-context.html , https://www.w3.org/WAI/WCAG22/Understanding/headings-and-labels.html

<a id="a11y-icon-button-no-tooltip"></a>
### `a11y-icon-button-no-tooltip`

**Warning, static and runtime.** An icon-only button has no tooltip, so a sighted user has to guess. Use the same words as its name, and show it on keyboard focus too.

```qml
// Bad
ToolButton { text: qsTr("Refresh"); display: AbstractButton.IconOnly; icon.name: "view-refresh" }
// Good
ToolButton {
    text: qsTr("Refresh"); display: AbstractButton.IconOnly; icon.name: "view-refresh"
    ToolTip.text: text
    ToolTip.visible: hovered || visualFocus
    ToolTip.delay: 500
}
```

Checked by: the runtime audit, on a button with an icon and no shown text that has neither `ToolTip.text` nor a `ToolTip` child with text.

Sources: https://developer.apple.com/design/human-interface-guidelines/accessibility , https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/ , https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html

<a id="a11y-target-size"></a>
### `a11y-target-size`

**Error, runtime.** A pointer target is smaller than the platform minimum and has no spacing that makes up for it. People with tremor, on a trackpad or on a bus miss it.

Numbers: `limits::TARGET_MIN_DESKTOP` (24 px, WCAG 2.2 SC 2.5.8: a 24 px circle around the target's centre may not overlap another target) and `limits::TARGET_MIN_TOUCH` (44 px, Apple's default; Material asks 48 dp). Apple's macOS default is 28 pt. Inline links in a sentence are exempt.

```qml
// Bad: a 16 px icon is the whole target
Image {
    source: "close.svg"; width: 16; height: 16
    TapHandler { onTapped: banner.close() }
}
// Good: the drawing stays 16 px, the target grows
ToolButton {
    text: qsTr("Close banner"); display: AbstractButton.IconOnly
    icon.source: "close.svg"; icon.width: 16; icon.height: 16
    implicitWidth: 28; implicitHeight: 28
    onClicked: banner.close()
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html , https://developer.apple.com/design/human-interface-guidelines/accessibility , https://m3.material.io/foundations/designing/structure

<a id="a11y-tab-trap"></a>
### `a11y-tab-trap`

**Error, runtime.** Tab walks into a part of the interface and cannot leave. A keyboard user is stuck there for good. A modal popup may keep focus, but must close on Escape.

```qml
// Bad: Tab is swallowed
TextArea { Keys.onTabPressed: (event) => event.accepted = true }
// Good: Tab moves on before the area can take it as a character
TextArea { KeyNavigation.priority: KeyNavigation.BeforeItem; KeyNavigation.tab: sendButton }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/no-keyboard-trap.html , https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/

<a id="a11y-tab-unreachable"></a>
### `a11y-tab-unreachable`

**Error, runtime.** Something the pointer can operate is never reached by Tab, so a keyboard user cannot do it.

```qml
// Bad
Rectangle { Accessible.role: Accessible.Button; Accessible.name: qsTr("Add"); TapHandler { onTapped: add() } }
// Good
Rectangle {
    Accessible.role: Accessible.Button; Accessible.name: qsTr("Add")
    activeFocusOnTab: true
    Keys.onReturnPressed: add()
    Keys.onSpacePressed: add()
    TapHandler { onTapped: add() }
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/keyboard.html , https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocusOnTab-prop

<a id="a11y-set-size"></a>
### `a11y-set-size`

**Warning, runtime.** A row of a virtualised list does not say where it is ("3 of 120"). With only part of the list built, a screen reader user cannot tell how long it is.

```qml
// Bad: rows drawn in a Column of a Repeater inside a Flickable, built by hand
// Good: a view's delegate tells its position and the count itself
ListView { model: files; delegate: ItemDelegate { text: model.name; width: ListView.view.width } }
```

Sources: https://www.w3.org/TR/wai-aria-1.2/#aria-setsize , https://www.w3.org/WAI/ARIA/apg/patterns/listbox/

<a id="a11y-hidden-exposed"></a>
### `a11y-hidden-exposed`

**Error, runtime.** An item nobody can see (opacity 0, zero size, clipped away) is still in the tree, so a screen reader reads, and Tab may reach, things that are not there.

```qml
// Bad: faded out, still exposed
Pane { opacity: drawerOpen ? 1 : 0 }
// Good
Pane { opacity: drawerOpen ? 1 : 0; visible: opacity > 0 }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html , axe `aria-hidden-focus`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md

<a id="a11y-current-unmarked"></a>
### `a11y-current-unmarked`

**Warning, runtime.** The current or selected item of a list, menu or combo box popup looks like the others, so neither a sighted user nor a screen reader can tell which it is. The mark needs `limits::NONTEXT_CONTRAST` against its neighbours.

```qml
// Bad
delegate: ItemDelegate { text: model.name }
// Good
delegate: ItemDelegate {
    text: model.name
    highlighted: ListView.isCurrentItem
}
```

Checked by: the runtime audit, comparing how the rows look (what is shown, colours, font weight and style, and a button's `highlighted`, `checked` and `down`): the list a combo box opens, and a view of rows one presses whose `currentIndex` the application sets. A view with its own `highlight` marks its row. It reports when the current row looks like all the others.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html , https://www.w3.org/WAI/ARIA/apg/patterns/listbox/

<a id="a11y-name-mismatch"></a>
### `a11y-name-mismatch`

**Warning, static and runtime.** A control's accessible name does not contain the text it shows. A voice-control user says "click Send" and nothing happens, because the name is "Submit message".

```qml
// Bad
Button { text: qsTr("Send"); Accessible.name: qsTr("Submit message") }
// Good: the name starts with what is seen
Button { text: qsTr("Send"); Accessible.name: qsTr("Send message") }
```

Checked by: the runtime audit, on a button whose shown text's words are not in its accessible name in the same order (case and punctuation aside).

Sources: https://www.w3.org/WAI/WCAG22/Understanding/label-in-name.html , axe `label-content-name-mismatch`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md

<a id="a11y-ignored-focusable"></a>
### `a11y-ignored-focusable`

**Error, static and runtime.** An item left out of the tree with `Accessible.ignored: true` takes focus or holds a control. Focus lands somewhere the screen reader says nothing about.

```qml
// Bad: the whole card is ignored, its button included
Pane { Accessible.ignored: true; Button { text: qsTr("Open") } }
// Good: only the decoration is ignored
Pane { Image { source: "card-art.svg"; Accessible.ignored: true }  Button { text: qsTr("Open") } }
```

Checked by: the runtime audit, on a shown item with `Accessible.ignored: true` that is a Tab stop. It says this instead of `a11y-unnamed-interactive`'s silent Tab stop.

Sources: axe `aria-hidden-focus`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md , https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html

<a id="a11y-nested-interactive"></a>
### `a11y-nested-interactive`

**Warning, static and runtime.** A control sits inside another item that also reacts to a click or a key. A press may reach either; a screen reader exposes one inside the other and cannot activate the inner one cleanly.

```qml
// Bad: a button inside a clickable row
ItemDelegate { text: model.name; onClicked: open(model.id); Button { text: qsTr("Delete"); anchors.right: parent.right } }
// Good: side by side
RowLayout {
    ItemDelegate { text: model.name; Layout.fillWidth: true; onClicked: open(model.id) }
    Button { text: qsTr("Delete %1").arg(model.name) }
}
```

Checked by: the runtime audit, on a control that is pressed whose node sits inside another pressed node, up to the first composite control (a combo box, spin box, list, menu, tab bar or dialog), which holds its parts on purpose.

Sources: axe `nested-interactive`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md , https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html

<a id="a11y-unnamed-dialog"></a>
### `a11y-unnamed-dialog`

**Warning, static and runtime.** A dialog or modal popup has no title and no name; a screen reader announces only "dialog".

```qml
// Bad
Dialog { modal: true; standardButtons: Dialog.Ok | Dialog.Cancel; Label { text: qsTr("Discard changes?") } }
// Good
Dialog { title: qsTr("Discard changes"); modal: true; standardButtons: Dialog.Ok | Dialog.Cancel }
```

Checked by: the runtime audit, on an open dialog or modal popup with no name and no `Accessible.labelledBy`. A drawer is a panel, not a dialog, and is left out.

Sources: axe `aria-dialog-name`: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md , https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/

<a id="a11y-placeholder-label"></a>
### `a11y-placeholder-label`

**Warning, static.** A `TextField` or `TextArea` has `placeholderText` and nothing else that names it: no `Accessible.name`, no `Accessible.labelledBy`, and no `Label` or `Text` beside it. `placeholderText` disappears as soon as one types, so the field loses its label when it is most needed, and it is drawn below text contrast by design (APCA puts it at Lc 30). A screen reader may read the placeholder, but a sighted user who has typed cannot check what the field asks for (WCAG 2.2 3.3.2).

```qml
// Bad: only the placeholder says what this is
TextField { placeholderText: qsTr("Email address") }
// Good: a label beside it, and the placeholder as an example
ColumnLayout {
    Label { text: qsTr("Email address") }
    TextField { placeholderText: qsTr("name@example.org") }
}
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/labels-or-instructions.html , https://www.w3.org/WAI/tutorials/forms/instructions/#placeholder-text

<a id="a11y-live-status"></a>
### `a11y-live-status`

**Hint, runtime.** A text the application calls a status, toast, notice or error changes and is not marked as a status, so a screen reader never says that it changed. Mark a polite status with `Accessible.role: Accessible.StatusBar` and an urgent one with `Accessible.AlertMessage`, or call `Accessible.announce`.

```qml
// Bad
Label { id: statusLine; text: qsTr("Saved %1 files").arg(count) }
// Good
Label {
    id: statusLine; text: qsTr("Saved %1 files").arg(count)
    Accessible.role: Accessible.StatusBar
    Accessible.name: text
}
```

Checked by: the runtime audit, on a shown text outside a list whose `text` the application binds or sets and whose `id` or `objectName` says status, toast, notice, notification, snackbar, banner or error, when neither it nor anything around it is announced. It is a Hint: the name is the only sign of what the text is for.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html , https://doc.qt.io/qt-6/qml-qtquick-accessible.html

## Not checked yet

- **Every drag has another way.** No rule checks it: the window cannot tell whether a drag also has keys, a menu or buttons. Reorder by drag also gets "Move up/Move down" actions or keys. https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html
- **Let text grow to 200 %.** Apple asks for at least 200 %; WCAG 1.4.4 the same. The audit runs at `limits::audit::FONT_SCALES`. https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html

## Sources

- WCAG 2.2: https://www.w3.org/TR/WCAG22/
- WAI-ARIA APG patterns: https://www.w3.org/WAI/ARIA/apg/patterns/
- axe-core rules: https://github.com/dequelabs/axe-core/blob/develop/doc/rule-descriptions.md
- Apple HIG, accessibility: https://developer.apple.com/design/human-interface-guidelines/accessibility
- Qt `Accessible`: https://doc.qt.io/qt-6/qml-qtquick-accessible.html
