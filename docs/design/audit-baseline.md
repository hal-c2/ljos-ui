# Runtime audit baseline

What the runtime design audit (`ljos audit`) finds in QML nobody wrote for
it, per rule. As with the [static lint baseline](lint-baseline.md), a change
to a check shows up as a change in its count. A count far above the rest
says to read that rule's findings for false positives before trusting it.
These are not targets: code that is not your own is reported, never gated.

Each batch of rules keeps its own section, between its markers.

<!-- batch-2b: begin -->

## Batch 2b: layout and content in the live tree

### How to reproduce it

```text
ljos audit <entry.qml> --example <id> --json --rules \
  layout-overlap,layout-misaligned,layout-shift,layout-stacked-scrims,\
layout-window-controls,layout-resize-handle,layout-wrap-separator,\
layout-menu-ungrouped,layout-reflow,content-raw-identifier,text-raw-markup,\
content-empty-state,content-repeated-text,content-identical-rows,\
content-inconsistent-words,content-disabled-no-reason,text-too-small,\
text-line-length
```

This uses the default matrix (`full`) and the default 500 ms settle, so
`layout-shift` compares the scene at first rest with the scene 500 ms later.
The entries and ids come from `corpus/examples.json`.

Audited, 42 scenes:

- 36 from `corpus/qtdoc`: calqlatr, clocks, coffee, dice,
  fx-material-showroom, graphs-csv, hangman, maroon, osmbuildings,
  qtjennydemo, robotarm, rssnews, samegame, stocqt, thermostat, todolist,
  toycustomizer, windowembedding, photosurface, lightningviewer,
  android-splashscreen, alarms, and 14 documentation snippets.
- 6 bricks of a desktop shell: CentreHost, HalC2Wordmark, PluginSlot,
  QrCode, ShellIcon, ShellChevron.

Four qtdoc examples do not load without their C++ types or style module:
car-configurator, colorpaletteclient, mediaplayer and
android-feature-delivery. The shell's entry and its other bricks import
that application's native module, so `ljos audit` cannot make them.

### Counts

| code | severity | findings | scenes |
|---|---|---:|---:|
| `content-disabled-no-reason` | hint | 20 | 2 |
| `content-empty-state` | warning | 4 | 3 |
| `layout-overlap` | warning | 1 | 1 |
| `layout-misaligned`, `layout-shift`, `layout-stacked-scrims`, `layout-window-controls`, `layout-resize-handle`, `layout-wrap-separator`, `layout-menu-ungrouped`, `layout-reflow` | | 0 | 0 |
| `content-raw-identifier`, `text-raw-markup`, `content-repeated-text`, `content-identical-rows`, `content-inconsistent-words`, `text-too-small`, `text-line-length` | | 0 | 0 |

A finding counts once however many matrix points it shows at.

### What was read

- `content-disabled-no-reason`. Thermostat: a room's mode buttons ("Cool",
  "Heat", "Dry", …, `RoomItemForm.ui.qml:188`) are switched off while the
  room is off, and nothing on them says so (18 findings, 6 buttons in 3
  rooms). Coffee: the toolbar's back and theme buttons
  (`CustomToolBarForm.ui.qml:18`, `:40`) are switched off by the flow's
  state. They have no name either.
- `content-empty-state`. rssnews: the feed list (`Main.qml:72`) under
  "RSS Feed from Yahoo News" is blank until the network answers. Stocqt:
  the stock list (`StockListView.qml:57`) and the favourites
  (`FavoriteView.qml:306`) look the same whether empty or still loading.
  The fourth finding is calqlatr's history list (`Display.qml:143`). It is
  true to the rule, but a blank history is less of a surprise.
- `layout-overlap`. lightningviewer's map switcher
  (`SwitchMap.qml:49`). This one is false, and the rule is not at fault.
  The buttons are shown by comparing `mapType` with a C++ enumeration
  (`MapTypeConfig`) the corpus does not have. Both comparisons come out
  true, so both buttons are shown. Qt shows one.

No rule above fired three times or more on true findings, so the corpus
gives fewer than three true examples per rule. The rules' fixtures
(`crates/ljos_design/fixtures/<code>/`) and
`features/a11y/design-audit-layout-content.feature` hold a true example of
each.

### Fixed on the way

- `layout-shift` reported 175 findings in 18 scenes. Each was the same
  item at a larger font scale or the small window, compared with where it
  was at the scene as it opened. That is the other setting, not a shift.
  The audit now judges a shift only at the scene as it is, where the first
  look was taken.
- `content-raw-identifier` panicked on text whose second character is not
  ASCII, when looking for a Windows path.

<!-- batch-2b: end -->
