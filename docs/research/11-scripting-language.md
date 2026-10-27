# The typed scripting language for Ljós QML

Design note for a decision, 2026-10-27.

QML handlers, functions and bindings are JavaScript today. This note compares two directions for
a typed language that replaces JavaScript there:

- **A. A strict TypeScript subset.** TypeScript syntax, with ReScript-style discipline: no
  `any`, no implicit `null`/`undefined`, discriminated unions that a `switch` must cover,
  `readonly` by default, and no prototype or `this` games.
- **B. A ReScript-like language.** ReScript's syntax and semantics: sound Hindley-Milner types,
  variants, exhaustive pattern matching, `option` in place of null, immutability by default. Its
  parser and checker would be ours, in Rust.

JavaScript stays as the compatibility mode either way: a Qt application's scripts keep running,
and code transformations carry them to the new language. The decision is yours. The note ends
with a recommendation and the questions that decide it.

Claims about TypeScript, ReScript or OCaml not backed by their public docs are *(unverified)*.

## At a glance

| | A. Strict TS subset | B. ReScript-like | Hybrid (A's syntax, B's discipline) |
|---|---|---|---|
| Parser | Exists: the vendored oxc fork already parses TS in QML scripts (`crates/ljos_compiler/src/js.rs` parses with `SourceType::ts()`) | New, ours, in Rust | Exists |
| Type checker | New, ours; no TS checker exists in Rust | New, ours; HM is a known algorithm | New, ours (same as A) |
| Soundness | Unsound by TS's own design; the subset closes the known holes it can | Sound by design | As A |
| `null` | `T \| undefined`, narrowed; no `null` | `option<'a>` | `T \| undefined` |
| Exhaustiveness | An error in our checker (TS needs a `never` trick) | A warning in ReScript (warning 8); we would make it an error | An error |
| Inference | Local; parameters annotated | Whole-function HM; few annotations | Local |
| Lowering to JS (desktop tier 2, web) | Erase the types: done today | A JS backend of our own | Erase the types |
| Lowering to tier-1 IR | Same oxc AST the tier-1 generator reads today | A second front into the IR | Same as A |
| Moving off JS | Mostly mechanical: add types, tighten | Rewrite: statements to expressions, no `return`, loops | Mostly mechanical |
| Every accepted program is valid TS | Yes, if the subset only restricts | No | Yes, if it only restricts |
| Familiarity, coding models | High | Low | High |
| Cost, rough | Checker + rules + messages | Parser + checker + JS backend + formatter + messages | As A |

## 1. Compiling into the engine

### What runs today

Ljós resolves every name at compile time (tier 0) and runs a binding in one of two tiers
(ARCHITECTURE.md §5.4):

- **Tier 1** is a register bytecode over the dynamic `Value` (`crates/ljos_ir/src/tier1.rs`),
  generated from the oxc AST of the expression's tier-2 function. A program runs whole or not at
  all and makes at most one write, because the VM cannot take an effect back when it gives up.
- **Tier 2** is QuickJS: each QML file becomes one JS module.

`docs/tier1-coverage.md` says where this stands on Qt's examples: **97.8 %** of bindings are
tier 1, but only **21.8 %** of handlers and **29.0 %** of functions. The reasons that keep
handlers out are method calls (432), calls of tier-2 functions (178), more than one effect
(110), global calls (79) and statements such as `for`, `var` and `switch` (54). Opcodes are not
yet specialised by type, and `docs/tier1-coverage.md` shows why tier 1 wins: it avoids the 250
ns call into QuickJS, not because it computes faster.

`ARCHITECTURE.md` §5.4 rejected "typed-only AOT to Rust" because it needs annotations and fights
hot reload. A typed language removes the first objection. It does not remove the second, so in
both directions the target stays the IR, not generated Rust.

### What a typed language lets the compiler prove

| Fact | What it buys in the engine | A | B |
|---|---|---|---|
| Types of operands | Specialised opcodes: `AddNum` and `ConcatStr` with no coercion path; no `toString` or `valueOf` call to guard against | Yes, where no hole lets a wrong type in | Yes |
| No null | `JumpIfNullish` and the `TypeError` path on `a.b` disappear; a read of a member cannot fail | Yes for checked code; values from `var` properties, JSON and JS libraries need a check at the boundary | Yes, with the same boundary |
| Totality | An exhaustive `switch` has no fall-through to `undefined`; a function provably returns its type | Yes, if the checker makes non-exhaustive an error | Yes |
| Cannot fail half-way | The "more than one effect" limit exists because the VM may give up after a write. A checked handler that calls only typed functions and builtins cannot throw a type error, so the VM can run it to the end with several writes | Mostly; native calls and out-of-range writes can still fail | Mostly, same caveats |
| Purity | Lets a binding call a user function in tier 1 (`Op::Invoke` today only for functions that write nothing) | Not in the types. A syntactic check works: no write, no call of a non-pure function. `readonly` by default makes it simpler | Not in the types either: OCaml and ReScript do not track effects. Immutability by default makes the same syntactic check simpler |
| Closed call targets | A call of a typed function can be inlined or compiled as its own program | Yes | Yes |

Both languages give the same engine wins. The difference is in how many holes each leaves for
the boundary checks to close (section 7 lists TypeScript's), not in what the IR could express. A
typed program needs new IR: typed opcodes, a tagged-union `Value` shape (today a record is
`Value::Map`), and `match` lowered to a jump table on the tag. That IR work is the same for A
and B.

### What stays dynamic

- **Plain JavaScript files** imported by path (`crates/ljos_compiler/src/script.rs`) and JS
  libraries. In both directions they are typed at the import by a declaration or not at all.
- **`var` properties.** `ARCHITECTURE.md` §5.4 notes 31 % of one large application's properties
  are `var`. Unless a QML declaration can name a script type (a question at the end), every read
  of one needs a check.
- **Model roles**, `ListModel` element proxies and context properties: exotic objects (§9.2).
  Their roles can be typed when the model declares them.
- **Data from outside**: JSON from the network, a plugin's answer, local storage. This must be
  decoded once, at the boundary, in both directions.
- **Dynamic creation** (`Qt.createQmlObject`, `Loader.item`): typed as its component when the
  URL is a literal, else as `Item`. And the compatibility mode's JS, as today.

## 2. ljos-web

ljos-web compiles QML to Solid 2 components (the ljos-web repository's `README.md`). The QML tree
is parsed by the same oxc fork. `lower.rs` builds the nodes Solid's JSX transform takes directly
in the arena. `resolve.rs` rewrites every free identifier to the binding that carries it
(`panel.input` becomes `panel$input()`), so that a reactive read is a call Solid's classifier
sees as dynamic. Then the vendored Solid compiler emits memos and effects (snapshots in
`crates/ljos_web/tests/snapshots/`). Rust types reach the page as wasm behind generated faces,
and they tell their changes into slots (`docs/LJOS_WEB.md` §4).

**A** lowers by erasure. The script is already an oxc TS AST. Stripping its types leaves the JS
AST that `resolve.rs` and Solid already take, with no new lowering. A tagged union is a plain
object with a `kind` field, which is readable in the browser's debugger. An exhaustive `switch`
is a `switch`. `readonly` costs nothing at run time. TypeScript's design goals say it adds no
run-time overhead and no run-time type information, and the subset keeps that.

**B** needs a JS backend of our own: ReScript's compiler is OCaml (section 4) and knows nothing
of QML scope or `resolve.rs`. The sane route lowers B to the same oxc JS AST and reuses
`resolve.rs` and Solid, copying ReScript's representations: a payload-less variant case as its
name string, one with a payload as `{TAG, _0, …}` (changed by `@tag`, `@as`, `@unboxed`), `None`
as `undefined` and `Some(x)` as `x`, and expression blocks turned into statements.

**Signals.** Neither language changes the reactive model: a binding is a getter Solid tracks,
and a handler is a function. Both give the compiler the same useful fact, that a member is not
reactive, so it need not be wrapped in a call. **Desktop, too:** what is not tier 1 runs in
QuickJS, so B's JS backend is needed on the desktop as well. A needs only `js.rs`'s erasure.

## 3. Moving off JavaScript: ten real cases

These come from the handlers and functions of a large desktop application written in QML
(anonymised: names of objects, endpoints and services are changed). Each is shown in JS as
written, in A and in B. "Mechanical" means a tool could do it given the types; "by hand" means a
person must decide something.

A migration tool runs the same three passes in both directions: add the types the compiler
knows, rewrite idioms (`Object.assign` to spread, `!x` on a non-boolean to a test), and leave a
`TODO(type)` where a value comes from outside undeclared.

### 3.1 A one-line handler

```js
onClicked: stopMode ? App.dispatch("editor.interrupt") : editor.submit("foreground")
```

```ts
// A: unchanged. If `submit` takes `"foreground" | "background"`, the literal is checked.
onClicked: stopMode ? App.dispatch("editor.interrupt") : editor.submit("foreground")
```

```rescript
// B: ReScript has the ternary; the mode becomes a variant if the API declares one.
onClicked: stopMode ? App.dispatch("editor.interrupt") : editor->submit(Foreground)
```

A: mechanical (nothing to do). B: mechanical once `submit`'s type is declared.

### 3.2 A list toggle (filter / concat)

```js
function toggled(list, value) {
    return list.includes(value) ? list.filter(each => each !== value) : list.concat([value]);
}
```

```ts
// A
function toggled(list: readonly string[], value: string): readonly string[] {
    return list.includes(value) ? list.filter(each => each !== value) : [...list, value];
}
```

```rescript
// B: inferred as array<'a> => 'a => array<'a>, with no annotation.
let toggled = (list, value) =>
  list->Array.includes(value) ? list->Array.filter(each => each != value) : list->Array.concat([value])
```

A: mechanical when the callers' types are known; otherwise the tool writes a generic `<T>` (if
the subset allows generics) or asks. B: mechanical, and HM finds the generic type by itself.
`!=` is ReScript's structural inequality, the right one here *(unverified for every type)*.

### 3.3 A function that branches on a mode

```js
function statusLabel(status) {
    switch (status) {
    case "todo": return qsTr("To do");
    case "doing": return qsTr("In progress");
    case "blocked": return qsTr("Blocked");
    case "done": return qsTr("Done");
    case "dropped": return qsTr("Dropped");
    default: return status ?? "";
    }
}
```

```ts
// A
type Status = "todo" | "doing" | "blocked" | "done" | "dropped";
function statusLabel(status: Status): string {
    switch (status) {
    case "todo": return qsTr("To do");
    // … one case per member; leaving one out is an error …
    case "dropped": return qsTr("Dropped");
    }
}
```

```rescript
// B: @as keeps the run-time value the string the server sends.
type status = | @as("todo") Todo | @as("doing") Doing | @as("blocked") Blocked
              | @as("done") Done | @as("dropped") Dropped
let statusLabel = status =>
  switch status {
  | Todo => qsTr("To do")
  // … one case per constructor …
  | Dropped => qsTr("Dropped")
  }
```

By hand in both. The original's `default` handles a status the server may add later. A typed
version must decide where unknown strings go, and that is a decode at the boundary, not in the
label function. A tool can write `status: string` and keep the `default`, which is mechanical
but proves nothing. The two directions are equal here. A sibling with an `undefined` fallback becomes `case undefined:` in A and `| None =>` in B.

### 3.4 A string-formatting helper with a plural

```js
function runLabel(run) {
    if (run.status !== "proposed")
        return view.runStatusLabel(run.status);
    const kept = (run.changes ?? []).filter(change => !change.dismissed).length;
    if (kept === 0 && run.health)
        return qsTr("Status update");
    return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").arg(kept);
}
```

```ts
// A
function runLabel(run: Run): string {
    if (run.status !== "proposed")
        return view.runStatusLabel(run.status);   // narrowed to the other statuses
    const kept = (run.changes ?? []).filter(change => !change.dismissed).length;
    if (kept === 0 && run.health !== undefined)
        return qsTr("Status update");
    return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").arg(kept);
}
```

```rescript
// B
let runLabel = (run: run) =>
  switch run.status {
  | Proposed =>
    let kept = run.changes->Option.getOr([])->Array.filter(c => !c.dismissed)->Array.length
    switch (kept, run.health) {
    | (0, Some(_)) => qsTr("Status update")
    | (1, _) => qsTr("1 change")
    | (n, _) => qsTr("%1 changes")->arg(n)
    }
  | other => view.runStatusLabel(other)
  }
```

A: mechanical once `Run` is declared. The only edit is truthiness on a string union becoming
`!== undefined`, a rule the subset enforces. B: a rewrite. Early `return`s become one `switch`,
because ReScript has no `return` *(unverified as a stated rule; its manual shows no `return`
statement)*. The B version reads well, but no tool produces it from the JS without understanding
the control flow.

### 3.5 Updating a record (immutable update)

```js
function edit(changes) { view.working = Object.assign({}, view.working, changes); }
function editFilter(changes) {
    view.edit({ filter: Object.assign({}, view.working.filter, changes) });
}
```

```ts
// A
function edit(changes: Partial<View>): void { view.working = { ...view.working, ...changes }; }
function editFilter(changes: Partial<Filter>): void {
    view.edit({ filter: { ...view.working.filter, ...changes } });
}
```

```rescript
// B: there is no Partial; callers pass the change as a function.
let edit = f => view.working = f(view.working)
let editFilter = f => edit(w => {...w, filter: f(w.filter)})
// a caller: editFilter(f => {...f, hideDone: true})
```

A: mechanical (`Object.assign({}, a, b)` becomes `{ ...a, ...b }`). Spreading a `Partial` can
write `undefined` into a required field unless the checker applies the rule of
`exactOptionalPropertyTypes`, so the subset must. B: by hand. Every call site changes, and
`Partial` has no equivalent. The B form is sound, and the A form is sound only with that rule.

### 3.6 Async: a call with a completion callback

```js
function ask(method, input, place, done) {
    view.refusal = "";
    view.busy += 1;
    view.service.call(method, input, (result, error) => {
        view.busy -= 1;
        if (error)
            view.refusal = error;
        if (done)
            done(result, error);
    }, place);
}
```

```ts
// A
function ask(method: string, input: Json, place: string,
             done?: (result: Json | undefined, error: string | undefined) => void): void {
    view.refusal = "";
    view.busy += 1;
    view.service.call(method, input, (result, error) => {
        view.busy -= 1;
        if (error !== undefined)
            view.refusal = error;
        done?.(result, error);
    }, place);
}
```

```rescript
// B: the (result, error) pair becomes result<json, string>, once, in the binding of `call`.
let ask = (method, input, place, ~done=?) => {
  view.refusal = ""
  view.busy = view.busy + 1
  view.service->call(method, input, place, res => {
    view.busy = view.busy - 1
    switch res { | Error(e) => view.refusal = e | Ok(_) => () }
    done->Option.forEach(d => d(res))
  })
}
```

A: mechanical. B: mostly mechanical, but `+=` goes, because ReScript has no compound assignment
*(unverified)*. The callback pair also becomes a `result`, which needs an adapter for the native
API, written once.

### 3.7 A callback that digs into an answer

```js
view.ask("addItem", { fields: fields }, place, (result, error) => {
    if (!error && result?.item?.id)
        view.selectedKey = place + "|" + result.item.id;
});
```

```ts
// A: the endpoint is declared with its answer type, so `result` is not Json.
view.ask(Api.addItem, { fields }, place, (result, error) => {
    const id = result?.item?.id;
    if (error === undefined && id !== undefined)
        view.selectedKey = `${place}|${id}`;
});
```

```rescript
// B
view->ask(Api.addItem, {fields}, place, res =>
  switch res {
  | Ok({item: Some({id: Some(id)})}) => view.selectedKey = `${place}|${id}`
  | _ => ()
  })
```

By hand in both, because someone must declare what `addItem` answers. After that, A keeps the
optional chaining and B's nested pattern is shorter and total. Without the declaration, A has
`result: Json` and the code needs a decoder call, and so does B.

### 3.8 A key handler

```js
Keys.onPressed: event => {
    const ctrl = event.modifiers & Qt.ControlModifier;
    if (ctrl && event.key >= Qt.Key_1 && event.key <= Qt.Key_9) {
        Palette.run(event.key - Qt.Key_1);
    } else if (event.key === Qt.Key_Down) {
        Palette.move(1);
    } else if (event.key === Qt.Key_Backspace && field.text.length === 0) {
        if (!Palette.leaveSubmenu()) return;
    } else {
        return;
    }
    event.accepted = true;
}
```

```ts
// A: the same handler; `event` is typed by the signal, and only one line changes.
    const ctrl = (event.modifiers & Qt.ControlModifier) !== 0;
```

```rescript
// B: Qt.Key_1 is a constructor, so the arithmetic needs a helper, and the early
// returns become a value.
Keys.onPressed: event => {
  let ctrl = event.modifiers->Flags.has(Qt.ControlModifier)
  let handled = switch event.key {
  | k if ctrl && Key.isDigit(k) => { Palette.run(Key.digit(k) - 1); true }
  | Down => { Palette.move(1); true }
  | Backspace if field.text->String.length == 0 => Palette.leaveSubmenu()
  | _ => false
  }
  if handled { event.accepted = true }
}
```

A: mechanical. In A, Qt's enums stay numbers with names (the engine's `Value::Enum` and the
`EnumKey`/`QtKey` opcodes). In B, the enum becomes a variant, so ranges and arithmetic over keys
need helpers, and the control flow is restructured by hand.

### 3.9 Keeping a list model in step (model update)

```js
function sync() {
    const shown = new Set(items.map(item => item.id));
    for (let i = cards.count - 1; i >= 0; --i) {
        const id = cards.get(i).toastId;
        if (shown.has(id))
            continue;
        cards.remove(i);
        delete byId[id];
    }
    for (let i = 0; i < items.length; ++i) {
        // … find the card, move it, bump its revision, or insert it …
    }
}
```

```ts
// A: the same function, with the model typed by its roles; one line changes,
// because the dictionary becomes a Map<string, Toast>.
        byId.delete(id);
```

```rescript
// B: no `continue`, no `--i`; the loop is turned around or replaced.
let sync = () => {
  let shown = Set.fromArray(items->Array.map(item => item.id))
  for i in cards->ListModel.count - 1 downto 0 {
    let id = (cards->ListModel.get(i)).toastId
    if !(shown->Set.has(id)) {
      cards->ListModel.remove(i)
      byId->Map.delete(id)->ignore
    }
  }
  // … the second loop's inner search, a helper returning option<int> …
}
```

A: mechanical, except `delete obj[key]` on a dictionary becoming `Map`, which the subset
requires because a record's shape must be fixed. That edit is mechanical when the object is only
used as a dictionary. B: by hand. ReScript has `for … to/downto` and `while`, but no `break` or
`continue` *(unverified)*. This is the case where B costs most, and imperative model syncs like
this are common in real code.

### 3.10 Parsing with a sentinel

```js
function day(iso) {
    if (!iso)
        return NaN;
    const parts = iso.split("-");
    return Math.round(Date.UTC(Number(parts[0]), Number(parts[1]) - 1, Number(parts[2])) / 86400000);
}
```

```ts
// A: with checked indexing, parts[0] is string | undefined.
function day(iso: string | undefined): number {
    if (iso === undefined || iso === "")
        return NaN;
    const [y = "", m = "", d = ""] = iso.split("-");
    return Math.round(Date.UTC(Number(y), Number(m) - 1, Number(d)) / 86400000);
}
```

```rescript
// B: the sentinel becomes an option, and every caller changes.
let day = iso =>
  switch iso->Option.map(s => s->String.split("-")) {
  | Some([y, m, d]) => Some(Math.round(Date.UTC(...) /. 86400000.))
  | _ => None
  }
```

A: mechanical, with friction from checked indexing (`noUncheckedIndexedAccess` semantics). It
keeps `NaN` as the "no day" value. B: by hand, but better: "no day" becomes a type and the
callers must handle it.

### How mechanical, in sum

| Case | A | B |
|---|---|---|
| 3.1 one-line handler | none needed | mechanical |
| 3.2 list toggle | mechanical | mechanical |
| 3.3 mode switch | by hand (boundary) | by hand (boundary) |
| 3.4 formatting helper | mechanical | rewrite |
| 3.5 record update | mechanical | rewrite of call sites |
| 3.6 async callback | mechanical | mostly mechanical, plus an adapter |
| 3.7 answer digging | by hand (declare the answer) | by hand (declare the answer) |
| 3.8 key handler | mechanical | rewrite |
| 3.9 model sync | mechanical, plus a `Map` | rewrite |
| 3.10 sentinel parse | mechanical | rewrite, callers too |

The boundary work in 3.3 and 3.7, declaring what comes from outside, is the same in both, and it
is where the real safety comes from. The rest is control flow and idiom. A keeps those, and B
rewrites them.

## 4. Cost to build

### What exists

- **Parsing.** `js.rs` already reparses each script as TypeScript, strips annotations, `as`,
  `!` and `satisfies`, and turns QML-type parameter annotations into coercions (its
  `typescript_is_stripped` test). Scenarios already write typed functions
  (`features/language/property-conversions.feature`).
- **Types of the world.** `TypeOracle`, `qt_types/src/ljos.txt`, an application's
  `ljostypes.json` and each `MetaObject` describe every property, signal, method and enum (§5.2,
  §5.5). Either checker reads these. Tier 0 already resolves every name.
- **Tooling.** `qml_fmt` lays out QML and copies script text unchanged. The authoring MCP server
  has `qml_check`, `qml_lint` (with a growing design lint) and `binding_tier_explain`. `ljos
  check` and the reload lint report diagnostics. Hot reload recompiles changed files and the
  files that name their types (§12.1).

### What we would write

Sizes are my estimates, in lines of Rust, for a first version that covers the QML script
surface. They are not measurements.

| Piece | A (or hybrid) | B |
|---|---|---|
| Parser | 0 (oxc). Reject what the subset forbids: a visitor, ~1k | New lexer and parser with error recovery, matching the QML fork's member recovery: ~6–9k |
| Type checker | Structural types, unions with narrowing, literal types, records, arrays, functions, optional members, maybe simple generics: ~10–15k. Narrowing by control flow is the hard part | HM with records, variants, `option`, `result`, let-polymorphism, record-field disambiguation: ~8–12k. The algorithm is well understood |
| Error messages | Local, because parameters are annotated: an error points at the line | HM errors surface far from the cause. ReScript invests heavily here *(unverified)*. This is where B costs most after the parser |
| JS output | 0 (erasure, done) | Expression-to-statement lowering, variants, options, pipes: ~3–5k, shared by desktop tier 2 and web |
| Tier-1 IR | Typed opcodes and union tags: same for both | Same, plus a second front into the IR (the generator reads the oxc AST today) |
| Formatter | Script text is kept today. Later, a TS formatter: oxc's own formatter is an option *(unverified state)* | New, ~2–3k |
| LSP / MCP | Type diagnostics, hover and completion in `qml_check`. Editors' TS support already highlights and parses the syntax | Same work, plus a grammar for editors |
| Second opinion | `tsc` (or its Go port) can check any program the subset accepts, given generated declarations for QML scope | None. ReScript's own compiler cannot see QML scope |
| Hot reload | Recheck changed files and those whose imported signatures changed. `ComponentSignature` already hashes property types | Same. HM inference across a file makes "what changed" wider unless public functions are annotated |
| Migration tool | Annotate, rewrite idioms, mark boundaries | Annotate, plus control-flow rewriting (sections 3.4, 3.8, 3.9) |

Neither direction has a checker to borrow: oxc has no type checker *(unverified for its latest
releases)*, and ReScript's is OCaml.

### What the three projects named in the brief offer

- **ReScript's compiler** is OCaml and outputs JS only (its build tool, rewatch, is Rust). It is
  a reference for semantics, JS representation and error messages, not a component: it cannot
  emit our IR, needs an OCaml toolchain in the dev loop, and does not know QML scope.
- **reativa** is a small, early OCaml/ReasonML library for reactive web UIs (dune, opam; to JS
  through Melange or js_of_ocaml) on fine-grained signals (`Signal`, `Computed`, `Effect`,
  `View`), whose graph runs natively. It shows an ML language wrapping a graph like
  `ljos_reactive`'s. It is OCaml, not ReScript, and gives us no parser, checker or backend.
- **Xote** is a ReScript UI library on signals, computeds and effects (JSX, no virtual-DOM diff,
  npm v7.1.0). It previews what B's scripts would read like, and gives us nothing to build on.

## 5. People and tools

- **Who writes QML scripts.** QML developers write JavaScript. TypeScript is the typed language
  most of them already know, and A asks them to unlearn little: `any`, `null`, classes and
  `this`. B asks them to learn variants, pipes, `option`, an expression-oriented `switch`, and
  life without `return`, `break` or compound assignment. Developers from OCaml, F#, Elm or Rust
  find B natural. Others find it foreign.
- **Learning curve of the discipline.** Both need the same new habits: decode at the boundary,
  model modes as unions, handle absence. In A those habits are rules on a syntax people already
  read. In B they are the language.
- **Coding models.** Models write TypeScript fluently, since it is among the most common
  languages in their training data. Their ReScript is weaker and drifts between versions: the
  standard library moved from `Js.*` and `Belt` to `Core`, and syntax and attributes changed
  across major versions, so models mix eras. They also confuse ReScript with Reason and OCaml
  *(unverified as a measurement; this is common experience, not a benchmark)*. For a dialect of
  our own, either way, models know only what the MCP server tells them. A strict subset of a
  language they know needs a short list of "not here", while a ReScript-like language of ours
  needs a full reference. Ljós treats agents as first-class users (§19), so this weighs more
  here than in most projects.
- **Reading.** B's code is shorter and its cases explicit (3.4, 3.7). A's reads like the
  compat-mode JS it will sit beside for years.

## 6. Declarative first

Ljós is pushing QML towards declarative code. A `decl-*` lint family is being written
(`decl-mirror-handler`, `decl-mode-by-handlers`, `decl-imperative-animation`,
`decl-visibility-juggling`). It finds script that should be a binding, a `State`, a `Transition`
or a `Behavior`. How much script does that leave?

- In Qt's examples, bindings outnumber handlers and functions nine to one (9594 bindings, 762
  handlers, 338 functions; `docs/tier1-coverage.md`), and bindings are already 97.8 % tier 1
  without types.
- In one large application of 204 QML files, the lint lane's first count found 490 JS functions,
  230 one-line handlers that assign a property, 25 `onXChanged: y = x` mirrors, and almost no
  `states`.

The declarative push removes most of the 230 one-liners and the mirrors. They become bindings
and states, which need no scripting language at all. What it does not remove is the 490
functions: data shaping, model syncs (3.9), network callbacks (3.6, 3.7), label and colour
tables (3.3), parsing (3.10). Those are logic, not presentation. Ljós also gives logic another
home: native types in Rust and an app core off the UI thread (§5.5, §10.2).

So the choice matters less than its size suggests. Once the declarative lints land and heavy
logic moves to Rust, the typed language covers handlers of a few lines, small helpers, and the
boundary code that decodes outside data. That favours whichever direction is cheapest to build
and to learn, as long as it gives totality and no-null. It does not favour the one with the
strongest theory. Where scripts stay heavy (3.9), A's mechanical migration is worth most.

The two efforts help each other. Typed unions for modes (3.3) are what `decl-mode-by-handlers`
wants to turn into `states`. A checker that knows a mode's cases can suggest the `State` list.

## 7. A hybrid?

The obvious hybrid is A's syntax with B's discipline. It is worth separating two versions.

**H1, restriction only.** TypeScript syntax, and every program the checker accepts is also a
valid TypeScript program with the same meaning. The discipline comes from rules, not new syntax:

- no `any`, and no `as` casts except to a declared decoder's result; no `!`; no `null` (absence
  is `undefined`, written `T | undefined` or `x?: T`);
- `switch` on a union must cover every case, as an error, with no `never` trick;
- records and arrays are `readonly` unless declared `mutable` (a comment pragma or a naming
  rule, since TS has no `mutable` keyword); writes to QML properties and `ListModel` stay
  allowed, because they are the engine's state, not values;
- tagged unions with a literal `kind` field, narrowed by `switch`, as TypeScript's handbook
  describes discriminated unions;
- no `class`, `this` (outside QML's scope object), prototypes, `delete`, `for…in`, `with`,
  `eval`; dictionaries are `Map`;
- no truthiness on non-booleans (3.4, 3.8);
- TypeScript's known unsound spots are closed by rule: indexing yields `T | undefined` (as
  `noUncheckedIndexedAccess`), optional members cannot be set to `undefined` explicitly (as
  `exactOptionalPropertyTypes`), method parameters are checked contravariantly, and mutable
  arrays are invariant. TypeScript's design goals list a sound type system as a non-goal. The
  last two holes are TypeScript behaviour *(unverified in its docs)*.

H1 is not sound in the formal sense B is. Generics, `typeof` narrowing and declarations for JS
libraries leave room. But it has no `any`, no casts, no null, totality and readonly records.
That covers the failures QML scripts actually have: an `undefined` member read, a mode not
handled, a value mutated behind a binding's back.

**H2, with extensions.** Add a `match` expression, `option`, or HM inference to TS syntax. Each
extension breaks "valid TypeScript". Editors, `tsc`, formatters and models then see errors in
correct code, and the main reason to choose TS syntax is gone. A `match` expression is the most
tempting extension. TS's statement `switch` with exhaustiveness as an error gets most of its
value. Lowering an expression-`switch` is not hard, but no tool outside Ljós would understand
it.

Weighed honestly: H1 gives most of B's safety with A's cost and familiarity. What it gives up is
real, though. It has no global inference (parameters need types), no exhaustive matching on
nested shapes (3.7 needs `?.` and a test, not one pattern), no let-polymorphism without generic
syntax, and its soundness rests on our rule list being complete. H2 gets none of B's soundness,
loses A's ecosystem, and costs the most. Avoid it.

## Recommendation

Take **A, built as the restriction-only hybrid (H1)**. Scripts are TypeScript syntax, parsed by
the oxc fork already in the tree, and checked by a checker of our own in Rust. The checker
enforces no `any`, no null, exhaustive `switch` as an error, readonly records, tagged unions,
and the closed soundness holes listed in section 7. Keep "every accepted program is valid
TypeScript" as a hard rule, so editors, `tsc` and coding models stay useful. Spend the money B
would have needed for a parser and JS backend on the parts both need: typed tier-1 opcodes,
typed QML declarations in place of `var`, boundary decoders, and the migration tool.

Choose B instead only if formal soundness and whole-program inference are worth a second front
end, a JS backend for both desktop and web, and a harder migration. B makes most sense if you
expect scripts to stay large and logic-heavy rather than shrink under the declarative push and
the move of logic into Rust.

## Questions that decide it

1. **Is formal soundness a requirement, or is "no `any`, no null, total, readonly, holes closed
   by rule" enough?** If soundness is required, B.
2. **Where should application logic live in five years?** If it lives in Rust native types, with
   QML scripts thin, A. If it lives in QML scripts, B gains.
3. **Must every typed script be valid TypeScript**, so `tsc`, editors and models work on it
   unmodified? If yes, A or H1, and never H2.
4. **Who writes the scripts?** QML/JS developers and coding models point to A. A team fluent in
   ML-family languages makes B cheap to learn.
5. **How do QML declarations name script types?** Both directions need `property Filter working`
   in place of `property var working`, with `Filter` declared in the script language. Is that
   one shared type namespace, and does it reach the `ljostypes.json` of native types?
6. **Does readonly-by-default cover QML properties?** Section 7 proposes no: property and model
   writes stay imperative, and only values are immutable. If you want properties covered too,
   every handler changes in both directions.
7. **Must compat-mode JS and typed scripts call each other freely?** If yes, the boundary
   (decoders, declarations for JS functions) is the main work in either direction, and A's
   erasure keeps the two closest at run time.
8. **How much budget is there for the front end?** A's cost is a checker. B's is a parser, a
   checker, a JS backend, a formatter and the error messages.

## Sources

- Ljós: `docs/ARCHITECTURE.md` §5.3–5.5, §6, §9.2, §12; `docs/tier1-coverage.md`;
  `crates/ljos_ir/src/tier1.rs`; `crates/ljos_compiler/src/{js,script,tier1}.rs`;
  `crates/qml_fmt/src/lib.rs`; `vendor/FORK.md`.
- ljos-web: the ljos-web repository's `README.md` and `docs/LJOS_WEB.md`,
  `crates/ljos_web/src/{lower,resolve}.rs`, `crates/ljos_web/tests/snapshots/tui/`.
- TypeScript: handbook "Narrowing"; wiki "TypeScript Design Goals".
- ReScript: manual pages "Introduction", "Pattern Matching / Destructuring", "Null, Undefined
  and Option", "Variant"; CONTRIBUTING.md and the rewatch README.
- reativa: https://github.com/brnrdog/reativa. Xote: https://xote.dev/.
