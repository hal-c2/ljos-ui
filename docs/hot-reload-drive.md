# Editing corpus examples while they run

What `ljos run --watch` does with the saves a developer makes, measured by editing qtdoc corpus
examples while they run (ARCHITECTURE.md section 12.1). `mise run gallery:hot <id>…`
(`scripts/hot-drive.py`) does it again. Each example runs from a copy in the desktop:cua sandbox with the
inspector on, so nothing is shown on the developer's screen and the corpus is not written.

Before the edits, each example is put in a state that is not its first:
- text typed into its first field;
- its first list scrolled;
- a control clicked (a tab chosen, a dialog opened).

Then it is saved with one edit after another, through the watcher, as an editor saves:

| edit | what changes |
|---|---|
| colour | the entry file's first colour literal |
| text | the entry file's first `text: "…"` |
| add | an `Item` added to the root |
| remove | that item taken away again |
| binding | the entry file's first bound `width`/`height`/`spacing`/`radius`, plus one |
| sibling | a colour or a text in the largest other file |
| break | a syntax error (`property int : 3`) in the entry file |
| fix | the syntax error taken out again |

An example with nothing to change for an edit skips it.

## Results

Release build, 2026-10-25, llvmpipe on the sandbox's X server. Times are from the save to the
first frame that shows the change (the `shown … after the save` of the watcher's line). The colour, text, add,
remove and sibling saves of an example are given as one range, with a save that stood out named apart.
"State the edits found" is a mark the driver looked for in the outline before and after each save.
For coffee and todolist it is the object count: the pushed page's objects were all kept.

| example | kind | objects kept | state the edits found | colour, text, add, remove, sibling | binding | break | fix |
|---|---|---|---|---|---|---|---|
| todolist | `ListView`s of `ListModel`s, `StackView`, dialogs, several directories | 569 | the page a click opened | sibling 23 ms; add skipped (\*) | — | `Main.qml:10:18: Unexpected token`, app goes on | state kept |
| calqlatr | a pad of buttons, a `ListView` display, several files | 233 | after a click | 18–19 ms | — | told, app goes on | state kept |
| alarms | Controls `ListView`, a `Dialog`, a form | 274 | the dialog `+` opens, still open | 17–18 ms | — | told, app goes on | state kept |
| clocks | `ListView` of delegates, a `Timer` | 142 | — | 17 ms | — | told, app goes on | state kept |
| coffee | `StackView` of pages, `MultiEffect`s, several files | 266 | the page a click pushed | 23–24 ms | 191 ms before the fix below, 47–61 ms after | told, app goes on | state kept |
| stocqt | `ListView`s, a `TextField`, popups, several directories | 377 | typed text | 24 ms | 25 ms | told, app goes on | state kept |
| samegame | a game board of `Repeater`, `Loader` and `Timer` | 220 | — | 21–34 ms | 21 ms | told, app goes on | state kept |
| thermostat | `SwipeView`, `TabBar`, `StackView`, `MultiEffect`s, a module | 1426 | the mode `At home` chose | 18 ms; sibling 31 ms | — | told, app goes on | state kept |
| hangman | `StackView`, a `Loader`, several files | 52 | — | 20–22 ms | — | told, app goes on | state kept |
| photosurface | `Repeater` of images, `PinchHandler`, an animation | 36 | — | 17 ms | 17 ms | told, app goes on | state kept |

Across the 10 examples and 61 saves:
- **Nothing crashed and no window was lost.**
- **Every successful reload was applied in place** (0 made again), so the state the reload rules keep was kept wherever the driver had made some: typed text, a dialog that was open, a pushed page, a chosen mode.
- **A syntax error was told as the file and line** (`reload failed: Main.qml:7:18: Unexpected token`), and the app went on as it was. The save that took it out says `the files are what runs again`.
- **The warnings of every loaded file were told with the first reload**, the reload lint among them, and not again until they changed. They are now told as the app starts, as each file is loaded, and a reload tells only the new ones.

(\*) todolist's `Main.qml` imports only its own module, so it has no `Item` to add. The driver now
skips that edit in such a file.

## What a save costs

`ljos: reloaded in X ms (compile A ms, apply B ms): …; shown S ms after the save`.

The shown time is made of:
- the watcher's quiet time after the last write closes, 15 ms;
- the compile, 1–8 ms;
- the apply, usually under 1 ms;
- the frame.

| file saved | lines (project) | compile | apply | shown |
|---|---|---|---|---|
| clocks `Main.qml` (colour) | 53 (157) | 1 ms | 0 ms | 17 ms |
| photosurface `photosurface.qml` (text) | 192 (317) | 1 ms | 0 ms | 17 ms |
| stocqt `FavoriteView.qml` (colour) | 388 (3 673) | 8 ms | 1 ms | 24 ms |
| thermostat `ThermostatSettingsForm.ui.qml` (text) | 329 (6 812) | 15 ms | 1 ms | 31 ms |

The compile grows with what a save touches more than with the file itself: a session compiles the
saved files and the files that name their types. For a small file the quiet time is most of the
wait. It is kept: it is what lets an editor that writes several files at once give one reload
and not a reload of half the edit.

samegame's 34 ms for some saves is one frame more than its compile and apply. Why that frame is
waited for was not looked into (it has a running `Timer`).

**Measured and fixed: an edit that changes a size where `MultiEffect` draws a shadow.**
- **Before.** coffee's binding edit (`width: (parent.width) + 1` on the root's content) took 105 ms to apply and showed 191 ms after the save. The edit itself was cheap: the reload was applied in place, and its whole apply was the binding's own effect. Every shadow on the page (8 `MultiEffect`s) was composed again at its new size, and the shadow's Gaussian blur was 90 % of that.
- **Same cost without a reload.** A script making the window 3 px wider took the same time.
- **The fix.** The blur adds one tap at a time to a run of pixels, not one pixel's taps at a time. The numbers are the same to the bit (`the_blur_gives_the_same_numbers_as_one_pixel_at_a_time`).
- **After.** The same width change costs 11–21 ms instead of 60–110 ms. The reload applies in 12–22 ms and shows 47–61 ms after the save.
