# ljos.dev pages

The pages of the ljos.dev documentation, one directory per section: `start`, `language`, `ui`, `controls`, `data`, `rust` and `quality`. Each page is Markdown with front matter (`title`, `description`, `section`, `order`).

A change to behaviour fixes the page that describes it in the same commit.

## Code blocks

ljos.dev runs the examples beside their code, so every fence says what its block is:

- ` ```qml ` is a whole app: saved as `Main.qml`, it passes `ljos check` and runs. It is written in Ljós's own API (`import Ljos…`, no `import QtQuick`), so `ljos migrate` has nothing left to change in it. Write it with `ljos migrate --write` and `ljos fmt`; do not change their output by hand.
- ` ```qml harness=<name> ` is a part of an app: `harness/<name>.qml` is the app, with one `//! snippet` line where the block goes, indented to match. Prefer making the block whole; keep the harnesses few.
- ` ```qml before ` is QML written for Qt, on [Bringing existing QML](language/porting-from-qt.md) only. It passes `ljos check`, and the `qml` block after it is what `ljos migrate --write` makes of it.
- An example of several files gives each block `group=<id>`, and every block but the root `qml` one `file=<Name>`: ` ```ts file=filter.ts group=filter `. A `ts` block is always a file of an example. A `javascript` block with `group=` is one too; without it, it is script run by the host.
- A file an example reads that a reader should see (an XML feed, a JSON list) is a `json`, `xml` or `text` block of its group, with `file=`: ` ```xml file=feed.xml group=feed `. Its text is the file's.
- A file an example needs that is not worth showing (a picture, a sound) sits in `files/`, and the example's root block names it: ` ```qml files=photo.jpg,beep.wav `. Every example that names it gets it beside `Main.qml`. An example names only files it has, so ljos.dev never asks a server for one.
- Blocks that never run say what they are: `sh`, `rust`, `toml`, `json`, `text`, `xml`, `gherkin`, `javascript`, `diff`. A bare fence or another tag fails.

`cargo test -p ljos_cli --test docs_site` holds every page to these rules, CI runs it when `docs/site` changes, and `crates/ljos_cli/tests/docs_site.txt` lists the examples that are not there yet and why. The list only shrinks: an entry that no longer applies fails the test.

The examples teach Ljós's dialect ([DIALECT.md](../DIALECT.md)): types from `Ljos.Core`, `Ljos.Ui`, `Ljos.Layout` and `Ljos.Models`, controls from the styled `Ljos.Controls`, text translated with `` t`…` ``, and TypeScript where an example has logic. Qt's forms (`import QtQuick…`, `qsTr`, `Ljos.Controls.Basic`) belong on the porting page, beside the Ljós form, and in an example `docs_site.txt` keeps in Qt's QML for a gap. `mise run docs:dialect` (`scripts/docs_dialect.py`) checks this; it lists the examples that keep `Ljos.Controls.Basic` for a control the style does not draw yet, and `tasks:smoke` runs it in CI.

`<!-- check: ... -->` comments mark claims still to be verified against `features/` or the code.
