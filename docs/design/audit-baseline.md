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

## Batch 2a: colour, focus, motion

### How to reproduce it

```text
cargo build --release -p ljos_cli
ljos audit corpus/qtdoc/<dir>/<entry> --matrix current \
  --rules color,focus,motion --json --example <id>
```

once for each example in `corpus/examples.json`. Each run loads the example,
lets it settle, and acts on it: focuses each Tab stop, flips each control's
state, reopens each open popup, switches the colour scheme and turns reduced
motion on.

### Counts

`corpus/` (Qt documentation demos and tutorials): 45 examples, 41 audited.
Four did not load because they need a native type or module the corpus
does not ship: android-feature-delivery (`AssetDownloader`),
car-configurator (`MapLoader`), colorpaletteclient (`QtExamples.ColorPalette`)
and mediaplayer (`PlaybackSeekControl`).

| code | severity | findings | examples |
|---|---|---:|---:|
| `color-accent-weight` | hint | 1 | 1 |
| `color-nontext-contrast` | warning | 0 | 0 |
| `color-semantic-hue` | hint | 0 | 0 |
| `color-state-only` | warning | 0 | 0 |
| `color-surface-merge` | warning | 0 | 0 |
| `color-translucent-surface` | warning | 0 | 0 |
| `focus-arrow-keys` | hint | 1 | 1 |
| `focus-escape-dismiss` | warning | 0 | 0 |
| `focus-indicator-missing` | error | 24 | 5 |
| `focus-order` | warning | 4 | 3 |
| `focus-popup-stranded` | warning | 0 | 0 |
| `focus-shortcut-under-modal` | warning | 0 | 0 |
| `motion-dropped-frames` | hint | 0 | 0 |
| `motion-duration` | warning | 2 | 1 |
| `motion-ease-in-enter` | error | 0 | 0 |
| `motion-frequent` | warning | 0 | 0 |
| `motion-reduced-ignored` | hint | 7 | 2 |
| `motion-scale-from-zero` | error | 0 | 0 |
| `motion-stagger` | warning | 0 | 0 |
| `motion-theme-switch` | hint | 0 | 0 |

Every finding was read; all are true. A rule at 0 is not shown to be
precise by this corpus, only not noisy: its fixtures in
`crates/ljos_design/fixtures/<code>/` are what show it finds what it says.

### What they find

- `color-accent-weight`: qtjennydemo `Main.qml:151`, two buttons on the
  window are highlighted, "Vibrate" first.
- `focus-arrow-keys`: alarms `Main.qml:17`, Tab stops five times inside the
  `ListView`, once per alarm's switch.
- `focus-indicator-missing`: calqlatr `NumberPad.qml:117` and on (each key
  draws its own background and none changes with `visualFocus`);
  android-splashscreen `Main.qml:16`, the button "Close"; toycustomizer
  `MainPage.qml:59`, the button "Tap to Start".
- `focus-order`: robotarm `MainScreen.ui.qml:106`, Tab goes from the claw's
  switch back up the column to the button "Pose 1"; thermostat
  `HomePageForm.ui.qml:94`, from "Auto" in the content back to the sidebar;
  todolist `HomePageForm.ui.qml:71`, from the text field up to the header's
  settings button (a `Page` header comes after its content in Tab order).
- `motion-duration`: thermostat `CustomSwitch.qml:54`, checking and
  unchecking the switch runs a 250 ms `OutBack` move of its handle, over the
  micro limit.
- `motion-reduced-ignored`: maroon `Main.qml:71`, `Main.qml:109` (rows that
  bob and images that rock, with reduced motion on); samegame
  `LogoAnimation.qml:15`, the logo still drops in.

### False positives fixed

The first run found these, all fixed before the counts above:

- `color-state-only` on a switch whose handle moves (thermostat, qtjennydemo)
  and on a button that shows a mark when checked (thermostat's rooms): a
  part that moves or shows is a change of shape, not of colour only.
- `motion-duration` on a model's `SmoothedAnimation` in robotarm's
  `BackendMock`: feedback to a check is only what animates the control.
- The same finding once per instance of a delegate or component: identical
  findings now collapse to the one place in the source.

### An application

A desktop application's QML (129 files) was audited too. Its `Main.qml`
does not load without the application's binary, which registers its native
module; of its components, the 6 that load by themselves (an icon, a
wordmark, a QR code, a chevron, a plugin slot, a centring host) have no
findings. The rest import the native module. Auditing such an application
needs it running: `app_audit` on a live session does that.
