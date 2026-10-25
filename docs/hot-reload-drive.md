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
| clocks | `ListView` of delegates, a `Timer` | 142 | where its row of clocks was scrolled to (†) | 17 ms; colour 33 ms once | — | told, app goes on | state kept |
| coffee | `StackView` of pages, `MultiEffect`s, several files | 266 | the page a click pushed | 23–24 ms | 191 ms before the fix below, 47–61 ms after | told, app goes on | state kept |
| stocqt | `ListView`s, a `TextField`, popups, several directories | 377 | typed text | 24 ms | 25 ms | told, app goes on | state kept |
| samegame | a game board of `Repeater`, `Loader` and `Timer` | 2560 | the game a click on the menu started (†) | 23–24 ms; 21–34 ms on the menu (‡) | 22 ms | told, app goes on | state kept |
| thermostat | `SwipeView`, `TabBar`, `StackView`, `MultiEffect`s, a module | 1426 | the mode `At home` chose | 18 ms; sibling 31 ms | — | told, app goes on | state kept |
| hangman | `StackView`, a `Loader`, several files | 52 | none: its page needs the application's own module (†) | 21–22 ms | — | told, app goes on | state kept |
| photosurface | `Repeater` of images, `PinchHandler`, an animation | 36 | where the surface was scrolled to | 17 ms; 24–27 ms scrolled | 17 ms; 25 ms scrolled | told, app goes on | state kept |

Across the 10 examples and 61 saves:
- **Nothing crashed and no window was lost.**
- **Every successful reload was applied in place** (0 made again), so the state the reload rules keep was kept wherever the driver had made some: typed text, a dialog that was open, a pushed page, a chosen mode, a scroll position.
- **A syntax error was told as the file and line** (`reload failed: Main.qml:7:18: Unexpected token`), and the app went on as it was. The save that took it out says `the files are what runs again`.
- **The warnings of every loaded file were told with the first reload**, the reload lint among them, and not again until they changed. They are now told as the app starts, as each file is loaded, and a reload tells only the new ones.

(\*) todolist's `Main.qml` imports only its own module, so it has no `Item` to add. The driver now
skips that edit in such a file.

(†) clocks, samegame and hangman are drawn with `Rectangle`s, `Image`s and `MouseArea`s that have no
accessible role or name, so their plain outline is a window with hidden nodes only. The driver now asks
the snapshot for `pointer` lines too: each shown item that takes the pointer, by the text in it or its
id, with a reference a click takes. The accessibility tree is not changed by it.
- **clocks** lists one line, `pointer [e2] id=clockview`. Its list runs across, so the driver turns the
  wheel sideways when the vertical turn moved nothing, and reads where the list stopped with `app_eval`
  (`contentX` 213.33, one clock along). Every save found it there.
- **samegame**'s menu buttons are `pointer` lines. The first click starts a game, and the board's 35
  hidden nodes are the mark (13 before). The driver stops at the first click that changes the outline:
  a later one ended the game and the app with it.
- **hangman** shows nothing past its window: its page uses `Store`, the application's own in-app
  purchase module, which a QML-only run does not have. Once (of 2 runs) the save of `LetterSelector.qml`,
  12 kB written in place, was told as `Expected } but found EOF` at its 8192nd byte and reloaded right
  after. 30 such saves by hand and a test that pauses between the parts
  (`a_file_written_in_place_in_parts_is_built_once_it_is_closed`) did not show it again.
  A change that does not compile is now held back and the files are read again after a second
  quiet time. Only an error then is told, so the first part of such a save is no longer told
  (`a_file_closed_after_its_first_part_is_built_once_the_rest_is_written`).

The windows now take the root's size, as `qml6` opens them: clocks 640x320, hangman 320x480.

photosurface's scroll brings no named line into view either: its pictures have no names. The
driver now takes the scroll bar that moved as the mark (`scrollbar value=0.24`), read once the
flick has stopped. Every save found it where it was.

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

(‡) **samegame's 34 ms on its menu is the display's frame, not the reload.**
- **Why.** Its menu runs a `SequentialAnimation` with `loops: -1`, so its frames come at the display's pace. A reload ready 17–21 ms after the save is shown at the next of those frames, up to one frame later. Any change waits for that frame while an animation runs.
- **Why it is the same each time.** The driver takes a snapshot just before each save, and a snapshot is a pass. That puts the save at the same point of the frame each time.
- **Measured.** Off-screen, four saves 1 s apart with no snapshot between them were shown 22.3, 20.9, 24.4 and 24.2 ms after the save. With a snapshot before each save they were shown 26.1, 21.0, 34.7 and 26.1 ms after. The wakes show the watcher's message at 5287.1 ms and the reload's pass at the next paced frame, 5293.7 ms.
- **Not changed.** Showing the reload before the paced frame would draw two frames in one interval.

**Measured and fixed: an edit that changes a size where `MultiEffect` draws a shadow.**
- **Before.** coffee's binding edit (`width: (parent.width) + 1` on the root's content) took 105 ms to apply and showed 191 ms after the save. The edit itself was cheap: the reload was applied in place, and its whole apply was the binding's own effect. Every shadow on the page (8 `MultiEffect`s) was composed again at its new size, and the shadow's Gaussian blur was 90 % of that.
- **Same cost without a reload.** A script making the window 3 px wider took the same time.
- **The fix.** The blur adds one tap at a time to a run of pixels, not one pixel's taps at a time. The numbers are the same to the bit (`the_blur_gives_the_same_numbers_as_one_pixel_at_a_time`).
- **After.** The same width change costs 11–21 ms instead of 60–110 ms. The reload applies in 12–22 ms and shows 47–61 ms after the save.
- **Then the shadows themselves.** A resize still blurred again every shadow whose size changed, 1.8–2.1 ms for each card and 0.65 ms for each circle. Now:
  - a shadow is kept by what it is made of: its source's alpha, its size and padding, and its offset, scale and blur. A source that changes only its colours or text, a shadow that changes only its colour, and two sources of one shape all share one blurred shadow;
  - a row the same as the row before is blurred once, and so is a row whose taps all fall on the same row: the straight sides and the empty padding of a card cost one row each;
  - an opaque pixel of the source leaves the shadow under it unworked, and a pixel the same as the one before comes out the same.

  The numbers are still the same to the bit (`rows_that_repeat_blur_to_the_numbers_one_pixel_at_a_time_gives`). A moved shadow is not drawn again, and a resize draws again only the shadows whose size changed (`crates/ljos_quick/tests/effects.rs`).
- **Measured** with `frame_profile` on coffee's choosing page at 1000x600, the window 3 px wider every other frame (`--grow 3`):

  | | a card's shadow | a circle's shadow | shadow work per resize frame | a frame |
  |---|---|---|---|---|
  | before | 1.8–2.1 ms | 0.65–0.85 ms | 5–6 ms | 19.6 ms |
  | after | 0.38–0.87 ms | 0.25–0.70 ms | 2–2.5 ms | 7.8 ms |

- **The GPU.** The blur stays on the CPU for now. The GL backend draws the window, and there is no wgpu backend yet to run a blur pass in. With the work above a resize is under the 3 ms aimed for.
