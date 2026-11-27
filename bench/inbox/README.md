# Inbox

A team's issue inbox: 10 000 reports in a list that is sorted, filtered and
searched as you type, a detail pane with the record, an edit form that
refuses bad input, a bar chart, a settings dialog, light and dark looks,
English and Japanese. It is written in Ljós's own dialect (`Ljos.*`
imports, typed TypeScript in `logic/inbox.ts`, `` t`…` `` for every
string), and it is the application the same team would write in any other
toolkit, so the same scripted session can measure each of them.

```sh
python3 bench/inbox/fixture/generate.py bench/inbox/data   # once; --check verifies it
mise run run bench/inbox/Main.qml --release                 # --check: 2 s, no window
LANGUAGE=ja ljos run bench/inbox/Main.qml                   # in Japanese
ljos render bench/inbox --out inbox.png --system-fonts --language ja
```

The fixture is generated rather than committed (7 MB of JSON): seed
20261127, sha256 `1bba809e…86365` for `issues.json`, and "now" pinned at
2026-11-27 12:00 UTC so relative dates and the chart are the same on every
day. `data/` is ignored by git.

## The layout is pinned

The window is 1280 × 800 logical pixels and every region sits at a fixed
place, so a session that clicks by window fraction hits the same control in
every toolkit. Styling inside a region is free; moving one is not.

| control | centre (x, y) | fraction |
|---|---|---|
| Search field (x 236–636, y 12–44) | (436, 28) | (0.3406, 0.0350) |
| Sort selector | (732, 28) | (0.5719, 0.0350) |
| New issue | (888, 28) | (0.6938, 0.0350) |
| Theme toggle | (1242, 28) | (0.9703, 0.0350) |
| Sidebar rows, 40 px from y 72: Inbox, Assigned to me, Starred, Closed, then one per label | Inbox (110, 92) | (0.0859, 0.1150) |
| Settings | (110, 764) | (0.0859, 0.9550) |
| List rows, 64 px from y 88 (x 220–700) | row *k* (460, 120 + 64 *k*) | row 0 (0.3594, 0.1500) |
| Tabs: Conversation, Edit, Insights | (780, 78), (940, 78), (1100, 78) | (0.6094 / 0.7344 / 0.8594, 0.0975) |
| Edit: title, tags, description | (990, 160), (990, 250), (990, 460) | (0.7734, 0.2000 / 0.3125 / 0.5750) |
| Edit: Cancel, Save | (1090, 660), (1196, 660) | (0.8516 / 0.9344, 0.8250) |
| Close issue (Conversation tab) | (1196, 132) | (0.9344, 0.1650) |
| Settings dialog (480 × 360 at 400, 220): 日本語, English, Reduce motion, Close | (560, 316), (460, 316), (470, 460), (820, 548) | |

PageUp and PageDown move the selection by exactly 10 rows, not by a page,
so that "200 × PageDown" is 2 000 rows in every toolkit.

## How it is built

- **Records.** `JsonListModel` reads the file; `Main.load` decodes a
  thousand records a turn, so no turn of script runs long, and hands them
  to the `Store` at once when the last is read.
- **One pass for what the list shows.** `Inbox.shown` filters and
  searches the sorted records in one pass and returns both the records
  (for the chart) and slim `{id, at}` keys (for the list). The list is a
  keyed `ListView` over the keys, so a sort or a search keeps every row
  that stays and animates only the rows that come and go; a row reads its
  record through `Store.issue(key)`, which is one array index.
- **Work only where it shows.** The labels and people are gathered in the
  turns after the records arrive; the chart counts only while its tab is
  open; `selected` does not scan with nothing selected. The turn that puts
  10 000 records in front of the bindings takes 42–64 ms in a release
  build.
- **No debounce.** Search runs on every key.
- **Translation.** Every visible string is a `` t`…` `` and
  `i18n/ja.po` holds the Japanese; counts go through `plural`, numbers
  and dates through the locale. The fixture's own text is not translated.
- **Chart.** Rectangles and text: a bar chart needs nothing more.
- **Avatars** are masked round with `MultiEffect`, because `clip` cuts
  to the item's box, not its radius.
- **Motion.** Rows fade in and out, the rows around them slide, a tab's
  page slides in from its tab's side, the record fades when another is
  chosen, the dialog fades and scales. With reduced motion the fades
  keep their length and only the sliding goes.

## What writing it turned up

Each of these is a finding about Ljós, not a workaround to copy.

- An array of `` t`…` `` literals in a binding keeps the language it was
  first built in. The translated lists (sorts, priorities, "Nobody") are
  functions on `Store` so they are built again on a language change.
- A `TabBar` built with `currentIndex` above 0 reports a binding loop, so
  the app starts on the first tab.
- An icon name `Icon` does not know draws nothing and says nothing.
- `ljos check` let a `list<Key>` pass where `Issue[]` was declared.
- A keyed `ListView` copies every field of every row's record it shows;
  rows of full records cost about 10.7 µs each to make, slim keys 2.3 µs.
- Promises cannot carry a cooperative load: the pump that runs them stops
  a chain that outlasts its window, so the load is chained with `later`.
