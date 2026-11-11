# ljos.dev pages

The pages of the ljos.dev documentation, one directory per section: `start`, `language`, `ui`, `controls`, `data`, `rust` and `quality`. Each page is Markdown with front matter (`title`, `description`, `section`, `order`).

A change to behaviour fixes the page that describes it in the same commit.

## Code blocks

ljos.dev runs the examples beside their code, so every fence says what its block is:

- ` ```qml ` is a whole app: saved as `Main.qml`, it passes `ljos check` and runs. It is written in Ljós's own API (`import Ljos…`, no `import QtQuick`), so `ljos migrate` has nothing left to change in it. Write it with `ljos migrate --write` and `ljos fmt`; do not change their output by hand.
- ` ```qml harness=<name> ` is a part of an app: `harness/<name>.qml` is the app, with one `//! snippet` line where the block goes, indented to match. Prefer making the block whole; keep the harnesses few.
- ` ```qml before ` is QML written for Qt, on [Bringing existing QML](language/porting-from-qt.md) only. It passes `ljos check`, and the `qml` block after it is what `ljos migrate --write` makes of it.
- An example of several files gives each block `group=<id>`, and every block but the root `qml` one `file=<Name>`: ` ```ts file=filter.ts group=filter `. A `ts` block is always a file of an example. A `javascript` block with `group=` is one too; without it, it is script run by the host.
- Blocks that never run say what they are: `sh`, `rust`, `toml`, `json`, `text`, `xml`, `gherkin`, `javascript`, `diff`. A bare fence or another tag fails.

`cargo test -p ljos_cli --test docs_site` holds every page to these rules, CI runs it when `docs/site` changes, and `crates/ljos_cli/tests/docs_site.txt` lists the examples that are not there yet and why. The list only shrinks: an entry that no longer applies fails the test.

`<!-- check: ... -->` comments mark claims still to be verified against `features/` or the code.
