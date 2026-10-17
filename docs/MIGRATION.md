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

Write the native side against the `ljos` facade alone —
`ljos::prelude` for the types, `ljos::App` to run — not against the
engine crates behind it. What the prelude does not re-export may
change without notice. `use ljos::prelude::*;` is all a crate that
derives native types needs: the derive, `qml_impl` and
`qml_singleton` find the engine through the facade by themselves.

The facade is the one dependency, by path or by git, and nothing else
goes in the application's manifest (no `[patch]` section: the parser
fork Ljós builds its QML tools with is a package of its own,
`ljos_oxc_parser`, reached by path). A shipped application turns the
default features off where it brings its own loop:

```toml
[dependencies]
ljos = { git = "https://github.com/olafura/ljos-ui", rev = "…" }
```

Every code block in this section is compiled on every test run: the
`Counter` is the doctest on the `ljos` crate's front page, and the
rest is `crates/ljos/examples/counter.rs`, the `App` doctests and
`crates/ljos_facade_aot` (the ahead-of-time start, its build
script and its type dump file). `crates/ljos_facade_app` is the same
native module as a crate of its own whose only dependency is `ljos`,
as an application's is.

A native type is a Rust struct with `#[derive(QmlObject)]`. Fields
are properties (`#[qml(notify)]` gives the change signal), and a
`#[qml_impl]` block adds signals and invokable methods:

```rust
use ljos::prelude::*;

#[derive(Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)
    }
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
}

let engine = ljos::engine();
register(&engine);
let counter = engine.create_by_name("Example", "Counter", None).unwrap();
engine.invoke_by_name(counter, "increment", &[]).unwrap();
assert_eq!(engine.get(counter, Counter::COUNT), Ok(1));
```

A type like this builds on `QtObject`, as every QML object does, so
QML may give it an `objectName`; the struct needs no field for it. A
visual type extends the toolkit's `Item` (in the prelude) instead:
`#[qml(module = "Example", name = "Panel", extends = Item)]` with a
`#[qml(base)] item: Item` field. It then takes `anchors`, `width` and
the rest as an `Item` does.

A visual type that paints itself, as a `QQuickPaintedItem` subclass
does (a chart, a terminal, a video frame), is written against
`ljos::item` alone. Its paint hook reads the item's own properties with
`item::prop`; that read is tracked, so the item repaints when one of
them changes. It paints into a display list in its own coordinates.
`item::register_key_item` and `item::register_wheel_item` give it keys
while it has the active focus (`item::force_active_focus`) and the
wheel over it. A `TextEdit` that colours its text takes a highlighter
instead (`item::set_highlighter`). This example is the `ljos::item`
doctest, so it compiles:

```rust
use std::rc::Rc;

use ljos::item::{self, DisplayListBuilder, ItemPaint, Rgba, TextEngine, kurbo};
use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Gauge", extends = Item)]
pub struct Gauge {
    #[qml(base)]
    pub item: Item,
    /// How full, 0 to 1.
    #[qml(notify)]
    pub value: f64,
}

/// What a gauge shows: a bar as long as its value.
#[derive(Debug)]
struct Bar(f64);

impl ItemPaint for Bar {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        let filled = kurbo::Rect::new(0.0, 0.0, size.width * self.0, size.height);
        out.fill_rect(filled, Rgba::BLACK);
    }
}

fn read(engine: &Engine, gauge: ObjectId) -> Rc<dyn ItemPaint> {
    Rc::new(Bar(item::prop(engine, gauge, Gauge::VALUE)))
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Gauge]));
    item::register_paint_item::<Gauge>(engine, read);
}
```

`crates/ljos_facade_app`, which depends on `ljos` alone, has the same
gauge with its key and wheel hooks.

A native singleton is the same kind of struct with `#[qml_singleton]`
under its derive, registered with
`engine.register_singleton::<T>("Example", Version::new(1, 0))`; the
attribute and `Version` are in the prelude too. The engine makes its
one object with `T::default()`, also when the type is `uncreatable`:
that only keeps QML from making a `T {}` of its own. A singleton that is
handed what it depends on, or is not `Default`, is registered with
`engine.register_singleton_with("Example", Version::new(1, 0), |engine| T { .. })`
instead; the closure runs when the singleton is first read.

Structured data crosses as Qt's `QVariantMap` and `QVariantList` do,
without JSON text. A field, parameter or return of `Vec<T>`,
`Option<T>` (`None` is `null`), `HashMap<String, T>` or
`serde_json::Value` is data: an invokable returning
`Vec<HashMap<String, Value>>` gives QML a list of plain objects
(`rows[1].path`), and a `serde_json::Value` parameter receives the
script's nested object whole. `Value::map([("path", ..), ("size", ..)])`
builds one record, and `engine.deep_copy(&v)` reads a `var` a method
was handed. A `Serialize` type goes through `serde_json::to_value`.

A `serde_json::Value` parameter reads what `JSON.stringify` would
write: a function or `undefined` field is left out. A QML object or a
value type (`point`, `font`) in it is refused, and the error names the
method, the parameter and the path (`Sink.json: payload.item is a QML
object, not JSON`); take a `Value` to be handed one. A `Vec<T>`
parameter left out, or given `null`, is empty, and one value that is
not a list is a list of it (`names("a")` is `["a"]`), as a QML `list`
property takes one; `Option<Vec<T>>` tells a list left out from an
empty one.

`register` is the one seam a native module sits at: a host calls it
on its Engine, and nothing else about the module needs to be
public. Methods read and write through the `Cx` they are handed and
the typed handles the derive generates (`Counter::COUNT`), which is
also how Rust code outside reads a property back
(`engine.get(obj, Counter::COUNT)`).

An application runs through the `App` builder, which registers each
module, hands the Engine its compiled components and creates the
root:

```rust
ljos::App::new("Hello")
    .size(640.0, 420.0)
    .register(natives::register)
    .source(ljos::include_qml!("qml.pack"))
    .root("Main")
    .run()
    .unwrap();
```

A shipped application's components are compiled ahead of time, so it
links no QML parser. Its build script compiles the QML — imports of
the application's own native modules included — into a pack in
`OUT_DIR`, through the facade's `build` feature in
`[build-dependencies]`; the program embeds the pack with
`ljos::include_qml!`, as above.

A build script cannot run the code of the crate it builds, so the
compiler learns the native types from a file instead: the type dump
`ljostypes.json`, which the application's own test writes from its
`register` function and checks on every run after that:

```rust
// tests/types.rs
#[test]
fn the_type_dump_file_is_current() {
    ljos::types::check_file("ljostypes.json".as_ref(), my_app::register).unwrap();
}
```

```rust
// build.rs
let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
ljos::build::compile_dir_with_types(
    "qml".as_ref(),
    "ljostypes.json".as_ref(),
    &out.join("qml.pack"),
)
.unwrap();
```

```sh
LJOS_TYPES=write cargo test --no-fail-fast   # writes ljostypes.json; commit it
cargo test                    # fails, naming that command, when it is stale
```

The natives stay in the application's crate, and only `ljos` with
`build` is a build dependency. A file with an error fails the build,
and so does a binding of a property or signal handler a native type
does not have (`lable: 1`, `onBumpd:`), including one written through
its group property or attached type (`limits.mx: 1`,
`Counter.onTalied:`), or an enum key it lacks (`Counter.Eleven`). A
group or attached type that the toolkit's own types reach too, such as
`anchors`, is left for the runtime as the toolkit's types are. While `LJOS_TYPES=write` is set the build lets
such files through as components that fail when made, so that the
test can run and write the file. `crates/ljos_facade_aot` is this
application.

A native module that is a crate of its own can skip the file:
`ljos::build::compile_dir("qml".as_ref(), natives::register, &out)`
registers it on a throwaway Engine in the build script, with the
same checks. `crates/ljos_facade_app` is such a module.

Compiling a directory at start-up exists for development behind the
non-default `dev-qml` feature (`App::qml_dir`,
`ljos::dev::compile_dir`) and is never in a shipped binary. As
under Qt, a file with an error fails only where it is made: the
directory compiles, the diagnostics are printed, and making the
broken component reports the compiler's message
(`App::qml_errors(OnError::Fail)` fails the whole directory instead,
for a build step or CI). QML in several trees — the counterpart of
`addImportPath` — goes in as `App::qml_dirs([app, shared], "Main")`:
each tree's files are keyed from their own root, and a file of one
may import a module whose `qmldir` is in another.

Each file compiled from a directory is at its own URL, `file://` and
its path, in every tree, so `Qt.resolvedUrl("icons/a.svg")` and an
`Image` or `Loader` source written relative to the file find what is
beside it, as a QML file loaded from disk does. A `url` property keeps
the relative text it is given, as in Qt 6; what uses it resolves it. A
pack says nothing of where its files were: a shipped application whose
QML names its assets relatively says where they live with
`MapSource::with_base_url`.

The printed diagnostics are one line each,
`path:line:column: severity: message`. An application that shows them
its own way (a `--check` command, a log, a test that wants no
warnings) takes them as values: `App::qml_diagnostics(handler)` before
`qml_dir`, or `ljos::dev::compile_quiet(&dirs, Some(&engine))`, which
returns the `MapSource` with every `Diagnostic` and prints nothing.
`MapSource::keys()` and `MapSource::failures()` list what compiled and
what did not, with where its first error is.

What the builder does not cover stays reachable: `App::run_with`
runs the same start and then hands over the shell's full `Setup`
(the Engine, the `UiHandle` worker threads post through, the
message handler), and `ljos::run(config, setup)` is that shell loop
without the builder. `App::build` makes the Engine and root
headlessly, which is how an application tests its own start.
With the facade's `render` feature it can look at the result too:
`ljos::render::render_png(&engine, root, (width, height), settle_ms)`
draws what `App::build` returned to a PNG after letting `settle_ms`
pass on a virtual clock — a screenshot test, a visual regression
check, a picture for documentation — and `ljos render` is a user of
it. `ljos::render::Picture` installed before the root is made runs the
root's own timers and animations on that clock, and
`ljos::render::set_fonts` chooses the bundled font (the same picture
on every machine) or the system's fonts.

State the QML reads before it has items of its own goes in by name,
the counterpart of `setContextProperty` — directly with
`engine.set_context_property(engine.root_context(), name, value)`,
or through the builder, where the value is made after the modules
are registered so it can be an object of a native type:

```rust
let app = ljos::App::new("Hello")
    .register(backend::register)
    .context_property("backend", |engine| {
        Value::Object(engine.create_by_name("Example", "Backend", None).unwrap())
    });
```

An application's native types do not reach the compiler's table of
types, but QML compiled through the application can `import` them:
`App::qml_dir` (behind the development-only `dev-qml` feature)
compiles against a snapshot of the modules `register`ed before the
`qml_dir` call, so `.register(m).qml_dir(dir, root)` just works when
the QML in `dir` imports `m`'s module — and the same snapshot is
available directly as `ljos::dev::compile_dir_with(dir, &engine)`.
The table-only tools still do not know them: to `ljos::dev::compile_dir`
such an import is an unknown type, and `check --port` will
not know them from a table; give the ones your QML uses `supported`
ledger rows (section 3). Created objects (context properties, list
models) remain the other way across. Where the shape is
not known ahead of time at all, `PropertyMap` (in the prelude) is
the `QQmlPropertyMap` counterpart: per-key reactive properties on
an object Rust creates.

For list data — Qt's `QAbstractListModel` role — implement
`QmlListModel` (roles plus the row-signal set) and attach it to an
object with `Engine::set_list_model`, which returns the
`ModelSignals` the model reports its changes with
(`rows_about_to_be_inserted`, `rows_inserted`, `rows_removed`,
`rows_moved`, `data_changed`, `roles_changed`, `model_reset`):

```rust
use std::cell::RefCell;
use std::rc::Rc;
use ljos::prelude::*;

#[derive(Debug, Default)]
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

let titles = Rc::new(Titles::default());
let holder = engine.create_native(QtObject::default(), None).unwrap();
let signals = engine
    .set_list_model(holder, Rc::clone(&titles) as Rc<dyn QmlListModel>)
    .unwrap();
let at = titles.rows.borrow().len();
signals.rows_about_to_be_inserted(&engine, at, 1);
titles.rows.borrow_mut().push("First".to_owned());
signals.rows_inserted(&engine, at, 1);
```

Giving `holder` as the `model` of a `ListView`, `GridView`,
`Repeater` or `Instantiator` then shows its rows, and the views
follow what `signals` reports. `QmlListModel::set_data` defaults to
refusing writes from delegates, and `row_key` defaults to the row's
position — override it with a stable key if rows move. The
lower-level `ItemModel` trait and `Engine::set_object_model` are
what views read through; `set_list_model` adapts a `QmlListModel`
to them.

Network access is a host service: `Platform::of(engine).fetch(url,
done)` (`Platform`, `Fetcher` and `FetchError` are in the prelude)
fetches on a thread of its own and runs `done` on the Engine thread
— say, for an XML feed behind a list model. An application run
through `App` or `ljos::run` already has the desktop shell's
HTTPS fetcher (redirects followed, a 30 s limit on the whole fetch,
replies capped at 16 MiB, failures as `FetchError::Failed` naming
the status or cause). A headless host — a test, a renderer —
installs its own with `set_fetcher`, or keeps the default under
which every fetch fails with `FetchError::NoNetwork`. For more than a
read, `Platform::request` carries a whole `Request` (method, headers,
body; `Request::new(Method::Post, url).json(body)` in the prelude as `Method`,
`Request` and `Response`) and answers with the reply's status as data —
a 404 is a `Response`, only transport failures are `Err`.

What the Engine and the platform hand out is named in the prelude
too, so it can be kept in a field: the `Time` of `engine.now()` and
the `TimerId` that `engine.call_at` returns and `cancel_call` takes
(a debounce), the `ClipboardService` of `Platform::clipboard()` with
its `ClipboardMode` and `ClipboardImage`, the `Application` singleton
behind `Qt.application` (`engine.singleton::<Application>()`, whose
`active` and `state` say whether the application has the user's
focus), and the `ColorScheme` that `ljos::color_scheme(engine)`
gives. `ljos::CoalesceKey` sits beside `ljos::UiHandle`, for
`send_coalesced`. A host that brings its own system dialogs or
permission prompts writes a `DialogService` (taking a `DialogRequest`
and answering through its `DialogAnswer` with a `DialogReply`) or a
`PermissionService` (a `Permission`, a `PermissionStatus`, a
`PermissionAnswer`) with the prelude's names; the `PathEnv` of
`Platform::env()` is the home and environment the standard paths come
from; and a host hands a drag of files from another application on as
an `ExternalDrag` to `external_drag(engine, &drag)`.

A clipboard write says whether it happened: `set_text`, `set_html`,
`set_image` and `clear` return `Result<(), ClipboardError>`, `Ok` only
when the clipboard took the data, so a "Copy" command can show
"Copied" only then. The system's clipboard passes on why it did not
(`Unavailable` where there is no clipboard to write to, `Denied` while
another program holds it, `Other` with the system's words); the
application's own clipboard, which a host without one keeps, always
takes the write. Qt's `QClipboard` returns nothing, and text items and
QML's `copy()` ignore the result as Qt's do.

Heavy work of your own follows the same shape: `ljos::task::spawn`
runs a closure on the bounded compute pool and runs its `done` with the
value on the Engine thread, so nothing heavy ever runs where QML lives.
`done` gets `Ok(value)`, or `Err(TaskPanic)` when the work panicked:
the panic is also reported to the engine's fault handler, and `done`
still runs so the application can put back what it set before
spawning.
Cancelling the returned `Task` before the work starts keeps it from
running; cancelling later discards the result. And because a native
method called from QML still runs on the UI thread, a development host
can set `Engine::set_slow_call_budget` to have every native call that
runs over it recorded and named in the log (`slow native call:
Type.method took N ms`), which is how a freeze gets a name instead of
a mystery.

A JS function Rust holds (a handler a script registered, say) is
called with `Engine::try_call_js`, the counterpart of `QJSValue::call`
with `isError()` checked. A throw comes back as `JsError::Thrown`,
which carries the value thrown as well as what it prints as: `value`
is the `Error` or object itself, whose fields (`message`, `name`, a
`code` the script added) `Engine::js_member` reads, or the primitive
thrown. `JsError::message(&engine)` is what to show a person: the
thrown value's `message` when it has one, else what it prints as —
so there is no `Error: ` to strip. Code that builds or matches the
variant field by field adds `value` (or `..`); the same holds for
`ljos_js::JsFault::JsException`, which gained `thrown`.

## 7. Running and testing the port

- `ljos check path/to/qml` (without `--port`) compiles the project and
  reports compiler errors, warnings and the static lints; it exits
  with an error if anything failed to compile.
- `ljos run path/to/Main.qml` opens a window on the file; `--watch`
  applies every save to the running application. `--mcp[=<CAPS>]`
  serves the running application to MCP clients (capabilities
  `inspect`, `input`, `mutate`, `eval`, each granted by name;
  `--mcp` alone means `inspect`). This runs QML on its own; an
  application with native types runs its own binary through
  `ljos::App` (section 6), whose `run_with` keeps the same shell
  setup `ljos run` builds on.
- `ljos test [PATHS]...` runs the `TestCase`s of `tst_*.qml` files on a
  virtual clock, printing results as Qt's QML test runner does; the
  exit status is the number of failed tests. `--filter <NAME>` runs
  only a named case, function or data tag (it may be given more than
  once), and `--import <DIR>` adds a directory of modules only the
  tests import. Native types are tested in Rust through the same
  interface QML uses them through: `ljos::App::build` headlessly,
  or `ljos::engine()` with the module's `register` called on it.
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
