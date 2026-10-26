# Content

Words are interface. A label is read before anything else; when it is vague, inconsistent or internal, people guess, and a wrong guess costs more than any animation saves. The rules check what can be measured in the drawn text; the guidance after them is for review by hand.

## Principles

- **One word per action, everywhere.** If it is `Delete` in a menu, it is not `Remove` in the dialog.
- **Verbs and objects on buttons.** `Save draft`, `Delete 3 files`, not `OK`.
- **People's words, not the program's.** Names and titles, not identifiers.
- **Say why.** Empty, disabled and failed states say why, and what to do next.

## Rules

<a id="content-inconsistent-words"></a>
### `content-inconsistent-words`

**Hint, runtime.** One window names the same action two ways (`Retry` and `Try again`, `Settings` and `Preferences`). People wonder whether the two do different things (Nielsen's consistency heuristic).

```qml
// Bad
Button { text: qsTr("Retry") }
MenuItem { text: qsTr("Try again") }
// Good
Button { text: qsTr("Retry") }
MenuItem { text: qsTr("Retry") }
```

Checked by: the runtime audit, on the names of controls in one window that use two words of one group of `ljos_design::words::SYNONYMS` (`Retry` and `Try again`, `Delete` and `Remove`); each control that uses one of them is reported, naming the other.

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ , https://developer.apple.com/design/human-interface-guidelines/writing

<a id="content-raw-identifier"></a>
### `content-raw-identifier`

**Warning, runtime.** Visible text shows an internal identifier: a UUID, an absolute path, a commit hash, an object dump (`QQuickItem(0x55d…)`). It means nothing to the reader and pushes out what would.

```qml
// Bad
Label { text: model.id }
// Good
Label { text: model.title
    ToolTip.visible: hover.hovered; ToolTip.text: model.id
    HoverHandler { id: hover } }
```

Checked by: the runtime audit, on a shown text that holds an object dump (`QQuickItem(0x…)`), a UUID, a URL with a query, an absolute path of two or more parts, or a run of `limits::HASH_MIN_CHARS` or more hexadecimal letters and digits.

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ (match between the system and the real world)

<a id="content-repeated-text"></a>
### `content-repeated-text`

**Hint, runtime.** Two text items side by side say the same thing (a title `Downloads` above a subtitle `Your downloads`). The second line is a place for what the first does not say.

```qml
// Bad
Column { Label { text: qsTr("Downloads") } Label { text: qsTr("Your downloads") } }
// Good
Column { Label { text: qsTr("Downloads") } Label { text: qsTr("%n file(s), 2 in progress", "", count) } }
```

Checked by: the runtime audit, on two shown texts of the application, two words or more, that say the same under one parent.

Sources: https://www.refactoringui.com/ , https://www.nngroup.com/articles/ten-usability-heuristics/

<a id="content-empty-state"></a>
### `content-empty-state`

**Warning, runtime.** A list or view with no rows shows nothing in their place, or only bare headings. The user cannot tell empty from loading from broken.

```qml
// Bad
ListView { model: results }
// Good
ListView {
    model: results
    BusyIndicator { anchors.centerIn: parent; running: loading }
    Label {
        anchors.centerIn: parent; visible: !loading && parent.count === 0
        text: qsTr("No results for “%1”. Try a shorter search.").arg(query)
    }
}
```

Checked by: the runtime audit, on a shown list or grid with no rows, larger than `limits::EMPTY_VIEW_MIN_PX` both ways, over which no text other than its header and footer is shown and no busy indicator runs. A combo box's list is left out.

Sources: https://vercel.com/design/guidelines , https://m3.material.io/foundations/content-design/overview

<a id="content-identical-rows"></a>
### `content-identical-rows`

**Hint, runtime.** At least `limits::IDENTICAL_ROWS_MIN` rows of one list carry the same label (`Command`, `Command`, `Command`), so they cannot be told apart.

```qml
// Bad
delegate: ItemDelegate { text: qsTr("Command") }
// Good
delegate: ItemDelegate { required property string command; text: command }
```

Checked by: the runtime audit, on a view where `limits::IDENTICAL_ROWS_MIN` or more rows carry one label (their accessible name, or the words they show); it is reported on the view.

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ (recognition rather than recall)

<a id="content-duplicate-control"></a>
### `content-duplicate-control`

**Warning, static.** Two controls in one view edit the same property (a switch in the toolbar and a check box in a panel). People ask which one wins, and they can disagree while one updates.

```qml
// Bad
ToolBar { Switch { checked: settings.wrap; onToggled: settings.wrap = checked } }
CheckBox { text: qsTr("Wrap lines"); checked: settings.wrap; onToggled: settings.wrap = checked }
// Good: one control, and the value shown elsewhere
CheckBox { text: qsTr("Wrap lines"); checked: settings.wrap; onToggled: settings.wrap = checked }
Label { text: settings.wrap ? qsTr("Wrap") : qsTr("No wrap") }
```

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/

<a id="content-disabled-no-reason"></a>
### `content-disabled-no-reason`

**Hint, runtime.** A disabled control or menu item gives no reason. The user cannot tell whether to fix something or give up.

```qml
// Bad
Button { text: qsTr("Publish"); enabled: form.valid }
// Good
// A disabled item takes no hover, so the reason sits beside it
RowLayout {
    Button { text: qsTr("Publish"); enabled: form.valid }
    Label { visible: !form.valid; text: qsTr("Add a title to publish"); color: palette.placeholderText }
}
```

Checked by: the runtime audit, on a control whose own `enabled` is set or bound false while its parent is enabled, with no `ToolTip.text`, no tooltip child and no `Accessible.description`. A control switched off with all its siblings by a disabled parent is the parent's matter.

Sources: https://www.nngroup.com/articles/ten-usability-heuristics/ (help users recognize, diagnose and recover), https://vercel.com/design/guidelines

## Guidance for review

These no machine can check; review them by hand. Where feedback is placed on screen (Enter hints, reopening panels, status beside its control, overridden settings) is in [interaction.md](interaction.md#where-feedback-goes).

**A toast says what happened, to how much, and how to undo it.** `Deleted` alone makes the user wonder what was deleted. Write `Moved 3 conversations to the archive` with an `Undo` action, and keep it up long enough to read (3 to 5 s, longer with an action, and never gone while the pointer or focus is on it).

**Settings pages follow one title pattern.** Every page of a settings area is titled the same way — all nouns (`Appearance`, `Notifications`, `Accounts`) or all verbs, never a mix — and the title matches the entry that opened it.

**A row shows its own state.** If an item is pinned, muted, shared or archived, the row says so (an icon with a name, or a short label), not only a menu that has to be opened to find out.

**Never mention what the build lacks.** Copy describes the program the user has. A desktop program does not say `Follows your browser`, a build without sync does not offer `Sync to all devices`, and an offline build does not link to an account page.

**One-word status suffixes must be unambiguous.** `Sync — Paused` can mean the user paused it or that it stopped on an error; `Backups — None` can mean none were made or none are wanted. Prefer a phrase that says what is true and who did it (`Sync paused by you`, `No backup yet`), or a word with only one reading in that place.

<a id="content-vague-action"></a>
### `content-vague-action`

**Hint, static.** A `Button` (and its kin), or a `MenuItem`, has a literal `text` of `OK`, `Yes`, `No`, `Continue`, `Submit`, `Done`, `Go`, `Click here` or `Proceed` (case and `&` ignored, a `qsTr` of one too). The reader has to read the question above to know what the button does. Say the verb and its object: `Save draft`, `Delete 3 files`. A `Dialog`'s `standardButtons` are the style's own words and are not reported.

```qml
// Bad
Button { text: qsTr("OK"); onClicked: save() }
// Good
Button { text: qsTr("Save draft"); onClicked: save() }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/alerts , https://m3.material.io/components/dialogs/guidelines

<a id="content-menu-ellipsis"></a>
### `content-menu-ellipsis`

**Hint, static.** A `MenuItem` or `Action` whose handler opens a `Dialog`, `FileDialog` or `Popup` of the file (`x.open()`) asks for more input, and its literal text should end with `…` (or `...`): `Rename…`. An item that acts at once has none. The ellipsis tells the user there is one more step before anything happens.

```qml
// Bad
MenuItem { text: qsTr("Rename"); onTriggered: renameDialog.open() }
// Good
MenuItem { text: qsTr("Rename…"); onTriggered: renameDialog.open() }
```

Sources: https://developer.apple.com/design/human-interface-guidelines/menus , https://learn.microsoft.com/en-us/windows/apps/design/controls/menus-and-context-menus

## Sources

- Nielsen's ten usability heuristics: https://www.nngroup.com/articles/ten-usability-heuristics/
- Apple HIG writing: https://developer.apple.com/design/human-interface-guidelines/writing
- Material 3 content design: https://m3.material.io/foundations/content-design/overview
- Microsoft Writing Style Guide, UI text: https://learn.microsoft.com/en-us/style-guide/welcome/
- Vercel web interface guidelines: https://vercel.com/design/guidelines ; Refactoring UI: https://www.refactoringui.com/
