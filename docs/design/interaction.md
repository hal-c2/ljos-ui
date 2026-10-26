# Interaction

Every action must work from a keyboard, a pointer and touch, and the user must always see where they are and what their input did. The rules here cover keyboard focus and input methods; the guidance after them covers where feedback goes.

## The keyboard, widget by widget

Tab moves between widgets; arrows move inside one. After the WAI-ARIA Authoring Practices:

| Widget | Tab | Inside | Activate | Close |
|---|---|---|---|---|
| Button | one stop | — | Space, Enter | — |
| Check box, switch | one stop | — | Space | — |
| Radio group | one stop, on the checked one | Arrows move and check | — | — |
| Tabs (`TabBar`) | one stop, on the current tab | Left/Right (wrap), Home/End | follows focus, or Enter/Space | — |
| List, tree | one stop | Up/Down, Home/End; Right/Left opens and closes a tree node | Enter | — |
| Menu | — | Up/Down (wrap), Right opens a submenu, Left closes it | Enter | Escape |
| Combo box | one stop | Up/Down change the value; Alt+Down opens | Enter | Escape |
| Slider | one stop | Arrows a step, Page Up/Down a big step, Home/End the ends | — | — |
| Dialog | Tab cycles inside it | — | Enter on the default button | Escape |

## Rules

<a id="focus-indicator-missing"></a>
### `focus-indicator-missing`

**Error, runtime.** A control that takes keyboard focus looks the same with and without it (WCAG 2.4.7), so a keyboard user is lost after one Tab. Draw it on `visualFocus`, not `activeFocus`, so a click does not leave a ring behind. A good ring is at least 2 px around the control at `limits::NONTEXT_CONTRAST` (WCAG 2.4.13).

```qml
// Bad: a custom background with no focus state
Button { background: Rectangle { color: palette.button } }
// Good
Button {
    id: b
    background: Rectangle { color: palette.button; border.width: b.visualFocus ? 2 : 0; border.color: palette.highlight }
}
```

Checked by: the runtime audit. It gives each Tab stop keyboard focus (reason Tab, so `visualFocus` is set), draws the stop and a margin of `limits::audit::FOCUS_MARGIN_PX` around it before and after, lets any animation the change started end, and reports a stop where fewer than a handful of pixels changed. Focus is put back where it was.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html , https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html

<a id="focus-order"></a>
### `focus-order`

**Warning, runtime.** Tab moves against the reading order: back up the window, or across columns and back. Users predict the next stop from what they see.

```qml
// Bad: declared out of order, so Tab jumps
ColumnLayout { Button { text: qsTr("Save"); Layout.row: 2 } TextField { placeholderText: qsTr("Title") } }
// Good: declared as read
ColumnLayout { TextField { placeholderText: qsTr("Title") } Button { text: qsTr("Save") } }
```

Checked by: the runtime audit, for the gross jumps only: the next stop is back along the same row (in the reading direction, so a mirrored layout reads right to left) or straight back up the same column. A move to the top of the next column is how columns are read and is not reported.

Sources: https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html

<a id="focus-popup-stranded"></a>
### `focus-popup-stranded`

**Warning, runtime.** A modal popup or dialog is open with keyboard focus left outside it, or Escape does not close it. Keystrokes go to a window the user cannot see behind the scrim.

```qml
// Bad
Popup { modal: true; closePolicy: Popup.NoAutoClose; TextField {} }
// Good
Popup { modal: true; focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside; TextField { focus: true } }
```

Checked by: the runtime audit, for the first half: the top open modal popup does not hold active focus. Whether focus comes back to the opener after it closes is not checked, since the audit cannot tell which control opened it. Escape is `focus-escape-dismiss`.

Sources: https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/

<a id="focus-pointer-only"></a>
### `focus-pointer-only`

**Warning, static.** An action is bound to a pointer handler on a plain item, with no key handler, so a keyboard (and a screen reader) cannot do it.

```qml
// Bad
Rectangle { MouseArea { anchors.fill: parent; onClicked: open() } }
// Good
ItemDelegate { text: qsTr("Open"); onClicked: open() }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/keyboard.html

<a id="focus-hover-only"></a>
### `focus-hover-only`

**Warning, static.** Something is shown only while the pointer hovers (a row's delete button, a hint), so touch and keyboard users never see it.

```qml
// Bad
ItemDelegate { id: row; ToolButton { visible: row.hovered; text: qsTr("Delete") } }
// Good
ItemDelegate { id: row; ToolButton { visible: row.hovered || row.visualFocus || row.highlighted; text: qsTr("Delete") } }
```

Sources: https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html

<a id="focus-escape-dismiss"></a>
### `focus-escape-dismiss`

**Warning, runtime.** A popup, dialog or overlay page that can be closed does not close on Escape wherever focus is inside it. Escape is the one key everyone tries.

```qml
// Bad: an overlay page with a close button only
Rectangle { anchors.fill: parent; Button { text: qsTr("Close"); onClicked: overlay.visible = false } }
// Good
FocusScope { anchors.fill: parent; focus: true
    Keys.onEscapePressed: overlay.visible = false
    Button { text: qsTr("Close"); onClicked: overlay.visible = false } }
```

Checked by: the runtime audit, for popups that can be closed some other way (pressing outside, a standard button, a button that says close, cancel or done) and whose `closePolicy` leaves Escape out: it gives the popup focus, presses Escape, and reports it when the popup is still open. A popup that closed is opened again.

Sources: https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/ , https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html

<a id="focus-shortcut-under-modal"></a>
### `focus-shortcut-under-modal`

**Warning, runtime.** A shortcut acts on the window behind an open modal popup: Ctrl+W closes a tab while a dialog about that tab is open.

```qml
// Bad
Shortcut { sequence: StandardKey.Close; onActivated: closeTab() }
// Good
Shortcut { sequence: StandardKey.Close; enabled: !confirmDialog.visible; onActivated: closeTab() }
```

Checked by: the runtime audit, which presses nothing: with a modal popup open, it reports each enabled window or application `Shortcut` with a sequence that is declared outside the popup.

Sources: https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/

<a id="focus-arrow-keys"></a>
### `focus-arrow-keys`

**Hint, runtime.** A composite widget (a tab bar, a radio group, a list) puts every item in the Tab order, so getting past it takes a Tab per item. It should be one stop, with arrows inside, as in the table above. Qt gives each radio button and each tab button a Tab stop of its own, so a small group of them (up to `limits::keyboard::COMPOSITE_TAB_STOPS_MAX`) is not reported.

```qml
// Bad: every row of the list is a Tab stop
ListView { model: folders; delegate: CheckBox { text: modelData } }
// Good: the list is one stop, and arrows move inside it
ListView { model: folders; activeFocusOnTab: true; delegate: CheckBox { text: modelData; focusPolicy: Qt.NoFocus } }
```

Checked by: the runtime audit. It walks the Tab order, groups the stops by the tab bar, list, grid or path view they are in (or the item that holds a run of radio buttons), and reports a group with more stops than the limit. Menus and what the style makes are left out.

Sources: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/#kbd_general_within

## Where feedback goes

These no machine can check; review them by hand.

**Show what Enter does.** If Enter does something in a field (sends, searches, adds a row), show it: a button beside the field that does the same, or a hint (`Press Enter to add`). Otherwise nobody knows the field acts until they try, and nobody using a pointer can act at all.

**A collapsed panel can always be reopened.** When a sidebar or panel collapses, leave something visible that brings it back, in the same place: a button, a tab on the edge, or a toggle in the toolbar. A shortcut alone is not enough, because nobody can see it.

**Status goes beside its control.** "Saved", "Connecting…" or "Invalid address" belongs next to the field or switch it describes, not in a status bar at the far edge of the window, where the user is not looking and cannot tell which control it is about.

**Say when a setting is overridden.** If a setting has no effect because something else decides it (a system setting, a policy, another option), say so next to it ("Set by your system theme") and, where you can, link to the place that decides. A control that silently does nothing looks broken.

<a id="focus-destructive-undo"></a>
### `focus-destructive-undo`

**Hint, static.** A `Button`, `MenuItem` or `Action` whose literal text starts with `Delete`, `Remove`, `Discard`, `Erase` or `Clear` does it from its handler at once: the handler opens no dialog or popup (`x.open()`) and calls nothing named like `confirm` or `undo`. A mis-click then costs the user their work. Ask first, or do it and offer `Undo` for a while (better, when it can be undone). A button inside a `Dialog` or a popup is the confirmation and is not reported. Keep a deliberate one with `// ljos-allow(focus-destructive-undo): <why>`.

```qml
// Bad: gone on the first click
Button { text: qsTr("Delete project"); onClicked: project.remove() }
// Good: ask first
Button { text: qsTr("Delete project"); onClicked: confirmDialog.open() }
```

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ (error prevention, user control and freedom) , https://alistapart.com/article/neveruseawarning/

## Not checked yet

- **Busy feedback** (proposed `focus-busy-feedback`): 0.1 s feels instant; up to 1 s keeps the flow but needs no indicator; beyond 1 s show that work is going on, and beyond 10 s show progress and a way to cancel (Nielsen). Do not flash a spinner for quick work: show it after 150–300 ms and keep it for at least 300–500 ms (Vercel). Not checked by the audit: it cannot tell which action is slow, or what the user is waiting for, without the application saying so.
- **Press feedback within 100 ms**, and the target is at least `limits::TARGET_MIN_DESKTOP` (`a11y-target-size`).

## Sources

- WCAG 2.2, keyboard and focus: https://www.w3.org/WAI/WCAG22/Understanding/keyboard.html , https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html , https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html , https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html , https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html
- WAI-ARIA Authoring Practices, patterns and keyboard interface: https://www.w3.org/WAI/ARIA/apg/patterns/ , https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
- Nielsen, response times: https://www.nngroup.com/articles/response-times-3-important-limits/ ; heuristics (visibility of system status): https://www.nngroup.com/articles/ten-usability-heuristics/
- Vercel web interface guidelines: https://vercel.com/design/guidelines
