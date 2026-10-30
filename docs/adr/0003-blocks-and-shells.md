# ADR 0003: Functional blocks, shells that skin them, and edits that keep the file

- **Status:** proposed, 2026-10-30. `qml_edit` (part 4) is built as a spike
  on this branch; `ljos_blocks` and the `Ljos.Blocks` module of section 4
  are built (ARCHITECTURE.md section 16); the rest is design.
- **Terms:** [CONTEXT.md](../../CONTEXT.md); ARCHITECTURE.md sections 3.3
  (messages), 10.2 (app core), 12 (hot code update), 16 (plugins and module
  aliases) and 19 (MCP).

## Context

An application's UI is usually two things at once: what it does (a player
that plays, a list of conversations that loads and sends) and how it looks
and is laid out. When both live in the same QML files, an author who wants
another look has to read and keep the behaviour, and a visual tool cannot
offer "swap the look" without understanding scripts.

Applications that keep them apart have converged on a split:

- **Controllers** own state and do the work. They publish what they know
  and take requests to change it.
- **Presentational pieces** hold no state of their own beyond view state.
  They show what was published and send requests.
- **A shell file** composes the presentational pieces and the theme. A user
  may replace it with their own; when theirs fails, the default comes back
  with the error shown.
- **Plugins** add pieces into named places of the shell.
- **A theme** of named roles reloads live, keeping the last good version
  when an edit breaks it.

Ljós should make this a general pattern any application can use, and a
visual designer (and an agent) needs three things from Ljós for it:

1. a list of the blocks an application offers, with what each publishes,
   accepts and offers room for, readable without running the application;
2. a way to preview a shell with real or made-up state;
3. a way to change the shell's QML structurally without rewriting the file.

## Decision

### 1. A block is a QML type with a contract read from its type

A **functional block** is a type registered in a module, written in Rust
(`#[derive(QmlObject)]`, section 5.5) or in QML. Its **contract** is its
public surface:

| Part | In Rust | In QML |
|---|---|---|
| **State** it publishes | `#[qml(readonly, notify)]` properties | `readonly property` |
| **Actions** it accepts | `#[qml(invokable)]` methods | functions |
| **Events** it reports | `#[qml(signal)]` | `signal` |
| **Slots** it offers | properties of type `list<Contribution>` (below) | the same |

DECISION: the contract is the type's own members, as `Engine::type_dump()`
and `ljostypes.json` already describe them, not a second interface
language and not a string-keyed state map with a `dispatch(name, payload)`
entry point. Typed members let the compiler resolve a skin's bindings at
tier 0 (section 5.4) and report a misspelt action at build time; a string map
would push both to run time. Where a caller does need names (MCP, the web
shell drop, a plugin written against a block it cannot import), the generic
adapter is `Block.invoke(name, args)` and `Block.read(name)` over the same
metaobject, not a second path.

**What makes a type a block** rather than any type: a `# block` comment in
the module's `qmldir` or `#[qml(block, category = "...", summary = "...")]`
on a Rust type. Either may carry a category and a one-line summary for a
palette:

```text
module App.Media
Player 1.0 Player.qml
# block Player 1.0 Player.qml Media "Plays one source"
```

The grammar is `# block <Name> <major.minor> <File.qml> [<Category>]
["<summary>"]`: the category is one word, the summary a double-quoted
string, both optional. DECISION: a comment, not a bare `block` line. Qt
6.11's `qml` and `qmllint` read an unknown first word as a type
declaration and fail the import ("a component declaration requires two or
three arguments, but 4 were provided"); they read past a comment. Ljós
also declares the type from the comment, so a qmldir only Ljós reads needs
no plain line, and one shared with Qt keeps its plain line for Qt.

**Purity of skins.** A skin may hold view state (`hovered`, a scroll
position, an open popup) but not application state. Ljós cannot know which
is which, so this is a convention with a lint: in a file the project marks
as a shell or skin (a `shell` directory in the manifest), a writable
`property` that is assigned from a handler and read by a block action's
argument is reported. It is a hint, never an error.

### 2. The registry is data, built without running the application

`ljos_blocks` (a new runtime-free library crate depending on `ljos_ir`
only, rule 5) holds:

```rust
pub struct BlockContract {
    pub module: String, pub name: String, pub version: (u8, u8),
    pub origin: Origin,                 // Rust | Qml { file }
    pub summary: Option<String>, pub category: Option<String>,
    pub state: Vec<StateEntry>,         // name, type, notify
    pub actions: Vec<ActionEntry>,      // name, params (name, type), returns
    pub events: Vec<EventEntry>,        // name, params
    pub slots: Vec<SlotEntry>,          // name, accepted delegate type
    pub mocks: Vec<MockRef>,            // see 3
}
pub struct BlockRegistry { /* by module and name; stable order */ }
impl BlockRegistry {
    pub fn from_type_dump(dump: &TypeDump, blocks: &[BlockDecl]) -> Self;
    pub fn to_json(&self) -> String;     // what tools and Studio read
}
```

DECISION: the registry is built from the type dump plus the `qmldir`
`block` lines, so a tool reads it from `ljostypes.json` and the module
directories, never by starting the application. The JSON is versioned and
stable-ordered, like the type dump.

### 3. Mock state is a mock module under a module alias

A preview without the application's core uses **mocks**: QML files with
the same type name and contract, in a module named `<Module>.Mock`
(`App.Media.Mock/Player.qml` with `readonly property bool playing: true`
and a `Timer` advancing `position`). A preview maps `App.Media` to
`App.Media.Mock` with a first-class module alias (section 16), so the
shell's text does not change and the same file previews with real or mocked
state.

DECISION: mocks are QML, not JSON fixtures, because a block's interesting
states are behaviour (progress, a list that loads in steps), and a QML mock
also proves the contract: the registry checks that each mock's surface
matches its block's, and a mismatch is a lint.

### 4. Shells and skins

- **`Ljos.Blocks`** (QML module, beside `Ljos.Design`) provides:
  - `Contribution { slot; mode: Append | Replace | Prepend; delegate:
    Component; order }`;
  - `Slot { name; contributions: list<Contribution>; default delegate }`,
    an item that instantiates the contributions for its name in order;
  - `ShellLoader { source; fallback: url; error: string }`, which loads a
    user shell and falls back to the default with `error` set when the
    user's fails to compile or throws while it is made. Hot reload
    (section 12.1) already keeps the old generation live on error; the
    loader adds the fallback for a shell that never loaded.
- **Theme tokens** are `Ljos.Design` (`Colors`, `Spacing`, `Typography`,
  `Motion`). A theme file (`theme.json`: roles to values, light and dark)
  overrides them live; on a bad file the last good one stays, as section
  12.1 asks. The file is watched; section 12.1 gives its format.
- **Plugins** contribute through `Contribution` under their own `ProcId`
  with `PluginSup` (section 16): a failing plugin's contributions are
  removed and its slot shows its default, nothing else changes.

### 5. Threads: blocks in Rust talk to the core through `UiHandle`

A Rust block whose work is off the UI thread (a player, a network list) is
a UI-thread object whose readonly properties are written from bounded
`UiMsg`s that the core posts through its `UiHandle` (section 3.3) and whose
actions post back on the core's own bounded channel. The block holds no
lock and polls nothing; it is woken by the message. Nothing new is needed
for this; the ADR names it so that block authors write it this way.

### 6. Structured edits keep every byte the edit does not touch

`qml_edit` (a tool crate on `qml_syntax`, like `qml_fmt`) applies an
`Edit` (set or remove a binding, insert, remove or move an object) to a
file's text and returns the new text with the list of changed ranges.
Outside those ranges the text is the input's, byte for byte. DECISION: it
splices over the parser's spans rather than keeping a separate concrete
syntax tree: the parser already gives every member its span, so the file
itself is the lossless tree, and there is no second structure to keep in
step. Comments and blank lines travel with the member they sit on (a
comment directly above, a comment after it on the same line). Every result
is parsed again and an edit that would not parse changes nothing
(`WouldBreak`); a file that does not parse is refused. Undo is exact:
`Applied::inverse` gives the changes that restore the input.

The spike's evidence is in its tests: golden cases and a corpus test
that, on every object of 581 QML files in this repository (2589 objects),
adds and removes a binding (plain and dotted), inserts and removes a child
at both ends and moves a child away and back. 10316 round trips give the
file back byte for byte; 1128 insertions open up an object whose members
shared lines and give the same document back once removed; 51 opposite
moves give the same document with a blank line or a binding order shifted
(the inverse changes still restore the bytes). None is refused.

## Consequences

- **New general pieces in Ljós:** `qml_edit` (tool), `ljos_blocks`
  (runtime-free), the `Ljos.Blocks` QML module, the `# block` qmldir comment and
  `#[qml(block)]`, theme-file watching, and inspector tools that read and
  drive blocks (`app_blocks`, `app_block_invoke`, `app_preview_alias`) and
  authoring tools that edit (`qml_edit_apply`, `blocks_list`). None of them
  names an application.
- **No new runtime dependency on the parser.** `qml_edit` links only where
  `qml_fmt` does (tools, and an app that opts into the compiler, goal 9).
- **A Qt-only author loses little:** the `# block` qmldir comment is a
  comment to Qt's tools, and a skin is ordinary QML.
- **Where the build differs:**
  - `Ljos.Blocks` is a native module of `ljos_quick`, not plain QML. A
    Slot finds its contributions in a registry the Engine keeps, with a
    reactive node per name, and a contribution leaves the registry when it
    is destroyed. A ShellLoader needs the compile error and the faults
    raised while its shell is made, with their file and line. Plain QML
    reaches none of these. So a Qt-only author cannot import it under
    `qml6`; a QML copy for that is not written.
  - `ShellLoader` also has `item` and `status` (`Null`, `Ready`, `Fallback`,
    `Error`). It is not a `Loader`. It falls back on a fault in the
    user's shell after the shell is made too, not only while it is made.
    A shell already shown that an edit breaks keeps the running generation
    and reports through the reload error (section 12.1), not through
    `error`. There is no application-wide `Runtime.lastError`.
  - `PluginSup` is not built for QML plugins. A Contribution goes from
    every Slot whatever destroys it, so a plugin's boundary that discards
    its content already takes the plugin's contributions away and gives
    the slot back its default. A plugin's own `ProcId` under `PluginSup`
    is the next piece.

## Open questions

- **Addresses.** Edits address objects by child-index paths (`root/0/2`)
  and by `id`. Paths change under other edits; a session of edits needs to
  re-resolve each address from an `id` or from the object key hot reload
  already uses (section 12.1, `ObjectKey`). A designer should add `id`s as
  it goes.
- **Files that do not parse.** A live code view has files that are briefly
  broken. Edits wait for the file to parse; a designer shows the error and
  holds structural edits until it does.
- **The purity lint** is a heuristic and may be noisy; it starts off by
  default.
