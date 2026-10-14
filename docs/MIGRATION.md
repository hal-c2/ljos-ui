# Migrating a Qt Quick application to Ljós

Ljós runs QML (Qt Quick + Qt Quick Controls compatible) on a Rust engine
(`docs/ARCHITECTURE.md` §17). This guide is for the author of an existing
Qt Quick application who wants to know what of their QML Ljós already has,
what it lacks, and what to do about the difference. The inventory tool is
`ljos check --port`; everything below was checked against the built CLI
(`ljos check --help`).

## 1. Inventory: `ljos check --port`

`check` normally compiles projects. With `--port` it does not compile:
it reads every `.qml` file below each directory you name, collects the
types and members the files use, and reports which of them Ljós lacks,
by module, most used first. Directories are required arguments; several
may be given.

```bash
ljos check --port path/to/qml
ljos check --port --tree examples/      # each group/project dir is its own project
ljos check --port --limit 10 path/to/qml
```

`--limit N` lists at most `N` missing (and unknown) items per module,
followed by `... and K more`; the counts are never truncated.

Real output for a file that uses `Item` and `QtQuick.Controls`'
`Calendar`:

```text
1 files, 2 uses of 2 types and 0 members
Ljós has 1, lacks 1, cannot place 0

module            supported  missing  unknown  missing uses
QtQuick.Controls          0        1        0             1
QtQuick                   1        0        0             0

missing
QtQuick.Controls
  type     Calendar     1 use in 1 file  not in Ljós's type table
```

How to read it:

- The first line counts files read and distinct types/members used.
  Files that did not parse are named next; what they use is not counted.
- `Ljós has H, lacks M, cannot place U` counts distinct types/members,
  not uses. "Cannot place" (unknown) means the front end resolved the
  use to a name that is in neither Ljós's table of its own types nor
  Qt's: a slip, or a name the app itself declares. A use the front end
  could not resolve at all is also reported as unknown, with its reason.
- The module table is ordered by missing uses, so what blocks the port
  most is on top. `missing uses` weighs a much-used missing member over
  a rarely used missing type.
- The `missing` and `unknown` sections list each item with its kind
  (`type`, `property`, `signal`, `method`, `enum`), use and file counts,
  and the reason. Supported items are counted, never listed.

What decides "has" is the same function that answers the authoring MCP
server's `api_status` tool (`ljos mcp`), so the two never disagree:
Ljós's own table of the types it implements answers first; where a
table of Qt's types was compiled in, it says what Qt has that Ljós has
not. On a build without Qt's table (`LJOS_QT_TYPES=none`, or a checkout
where `mise run types` never ran), a name Ljós lacks is counted missing
with the note `not in Ljós's type table (this build has no Qt table to
confirm it)` instead of unknown.

The command exits 0 after printing the report, missing items or not,
unless reading a directory or the ledger fails, or `--deny-missing`
is given (section 4).

## 2. JSON output

`--json` (only with `--port`) prints the same report as JSON:

```bash
ljos check --port --json path/to/qml > port-report.json
```

The top-level keys are `files`, `unparsed`, `supported`, `missing`,
`unknown`, `uses` and `modules`. Each module has `module`, `supported`,
`missing`, `unknown`, `missing_uses`, `missing_items` and
`unknown_items`; each item has `type`, `member` (`null` for the type
itself), `kind`, `uses`, `files` (a count), `in` (the file names) and
`reason`. Supported items appear only in the counts.

## 3. The API ledger (`--ledger`)

`--ledger FILE` weighs uses against an API ledger first; where the
ledger has no row, the tables answer as above. Without `--ledger` no
ledger is read. The format is the one ledger format there is — the
`Ledger`/`LedgerRow` of §18.5, shared with `api_status` — a TOML file
of `[[row]]` entries with exactly these fields (unknown fields are
refused):

| Field | Meaning |
|---|---|
| `module` | QML module, e.g. `"QtQuick.Controls"` |
| `type` | Type name, e.g. `"Calendar"` |
| `member` | Member name; absent on the row for the type itself |
| `kind` | `type`, `property`, `signal`, `method` or `enum` |
| `status` | `supported`, `partial`, `dropped` or `unsupported` |
| `reason` | Why; required for `dropped` and `unsupported` |
| `scenarios` | Optional `@api-…` tags of scenarios proving the row |

`member` is set exactly when `kind` is not `type`, and every
`scenarios` tag must start with `@api-`; a file that breaks these rules
is an error, not a warning.

```toml
[[row]]
module = "QtQuick.Controls"
type = "Calendar"
kind = "type"
status = "unsupported"
reason = "the port draws its own calendar for now"

[[row]]
module = "QtQuick"
type = "Rectangle"
member = "color"
kind = "property"
status = "dropped"
reason = "the app colours through its palette instead"
scenarios = ["@api-rectangle-color"]
```

How a row is weighed:

- `supported` and `partial` count as *has* — even for a name no table
  has, which is how a row records "the app provides this itself".
- `dropped` and `unsupported` count as *missing*, with the row's
  `reason` printed in place of the table's. Recording a decision in
  the ledger does not make the use supported.
- A row is exact: a row for a member says nothing about its type or
  the type's other members, and a row for a type says nothing about
  its members. Where there is no exact row, the tables answer. To
  mark a whole type and the members your QML uses, write a row for
  each.

## 4. Failing the build on gaps (`--deny-missing`)

```bash
ljos check --port --deny-missing path/to/qml
ljos check --port --deny-missing --ledger port-ledger.toml path/to/qml
```

`--deny-missing` prints the report as usual and then exits with an
error (`ljos: N types or members Ljós lacks`, exit code 1) when the
missing count is above zero. Unknown items do not fail it. Because
`dropped`/`unsupported` ledger rows are still missing, a ledger does
not turn the ratchet off: in CI it passes only when every use is
supported — by Ljós, or by a `supported`/`partial` row for something
the app itself provides. Use the ledger without `--deny-missing` while
the port is in progress, to keep the reasons next to the gaps, and add
`--deny-missing` once the missing count reaches zero.

## 5. What to do about a missing type or member

Work the report top-down; it is already ordered by how much each gap
is used.

1. **Check what you actually use.** A missing member of a type Ljós has
   leaves the rest of the file working; often another member, or a
   small QML wrapper in the app, covers it.
2. **Replace Qt 5-era modules.** Effects from
   `Qt5Compat.GraphicalEffects` and other compat modules have no Ljós
   counterpart; rewrite them in the app's QML (plain items, or
   `QtQuick.Shapes`) and, if the compat name stays anywhere, record it
   with an `unsupported` row saying what replaced it.
3. **Provide it in the app.** A project-local QML component or a native
   Rust type (below) can stand in for an app-specific control. Add a
   `supported` row naming what you provide so the report agrees.
4. **Drop it deliberately.** If the QML keeps a use Ljós will not have,
   a `dropped` row keeps the reason in every report instead of the
   bare `not in Ljós's type table`.
5. **Wait, but say so.** An `unsupported` row with a reason is the
   tracked-gap record; it still counts missing and still fails
   `--deny-missing`, which is the point.

Unknown items are usually the app's own types from a module the front
end was not given, or name slips. Resolve them by checking the import
and the spelling before treating them as gaps.

## 6. Bridging native state from Rust

Ljós has no `QQmlPropertyMap` equivalent. The supported ways to expose
Rust state to QML are native types and native list models, from
`ljos_engine`.

A native type is a Rust struct with `#[derive(QmlObject)]` (this is the
engine's own documented example):

```rust
use ljos_engine::{Engine, QmlObject, qml_impl};

#[derive(Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(invokable)]
    fn increment(cx: &ljos_engine::Cx) -> Result<(), ljos_engine::EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

let engine = Engine::new();
engine.register_module(ljos_engine::module!("Example" 1.0 => [Counter]));
```

An object created from such a type can be put in scope by name with
`Engine::set_context_property(engine.root_context(), name, value)`,
the counterpart of Qt's `setContextProperty`.

For list data — Qt's `QAbstractListModel` role — implement
`ljos_engine::QmlListModel` (roles plus the row-signal set) and attach
it to an object with `Engine::set_list_model`, which returns the
`ModelSignals` the model reports its changes with
(`rows_about_to_be_inserted`, `rows_inserted`, `rows_removed`,
`rows_moved`, `data_changed`, `roles_changed`, `model_reset`):

```rust
use std::cell::RefCell;
use std::rc::Rc;
use ljos_engine::{Atom, QmlListModel, QtObject, Value};

struct Titles {
    rows: RefCell<Vec<String>>,
}

impl QmlListModel for Titles {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("title")]
    }

    fn data(&self, row: usize, role: &str) -> Value {
        match (self.rows.borrow().get(row), role) {
            (Some(s), "title") => Value::from(s.as_str()),
            _ => Value::Undefined,
        }
    }
}

let holder = engine.create_native(QtObject::default(), None).unwrap();
let signals = engine
    .set_list_model(holder, Rc::new(Titles { rows: RefCell::new(vec![]) }))
    .unwrap();
```

Giving `holder` as the `model` of a `ListView`, `GridView`, `Repeater`
or `Instantiator` then shows its rows; after changing the rows, say so
through `signals` (for example `signals.rows_inserted(&engine, first,
count)`) and the views follow. `QmlListModel::set_data` defaults to
refusing writes from delegates, and `row_key` defaults to the row's
position — override it with a stable key if rows move. The lower-level
`ItemModel` trait and `Engine::set_object_model` are what views read
through; `set_list_model` adapts a `QmlListModel` to them.

Native types the application registers are known to the engine at run
time; they do not reach the compiler's table of types yet, so
`check --port` will not know them from a table — give the ones your
QML imports `supported` ledger rows (section 3).

## 7. Running and testing the port

- `ljos check path/to/qml` (without `--port`) compiles the project and
  reports compiler errors, warnings and the static lints; it exits
  with an error if anything failed to compile.
- `ljos run path/to/Main.qml` opens a window on the file; `--watch`
  applies every save to the running application. `--mcp[=<CAPS>]`
  serves the running application to MCP clients (capabilities
  `inspect`, `input`, `mutate`, `eval`, each granted by name;
  `--mcp` alone means `inspect`).
- `ljos test [PATHS]...` runs the `TestCase`s of `tst_*.qml` files on a
  virtual clock, printing results as Qt's QML test runner does; the
  exit status is the number of failed tests. `--filter <NAME>` runs
  only a named case, function or data tag (it may be given more than
  once), and `--import <DIR>` adds a directory of modules only the
  tests import.
- `ljos mcp --project .` serves the authoring MCP server on stdio,
  read-only by default; its `api_status` tool gives the per-type answer
  behind every `check --port` verdict.

## Checklist

- [ ] Run `ljos check --port` and read the module table top-down.
- [ ] For each gap: rewrite, provide it in the app, drop it, or record
      it `unsupported` with a reason in the ledger.
- [ ] Give types the app provides itself `supported` ledger rows.
- [ ] Add `--deny-missing` to CI once the missing count is zero.
- [ ] Move native controllers to `QmlObject` types and list data to
      `QmlListModel`.
- [ ] Run `ljos check`, then `ljos test` on the existing `tst_*.qml`.
