# Where script costs

Script is never forbidden in Ljós. A binding or handler the binding VM cannot run is left to JavaScript (QuickJS), which is correct and only slower, and most of the time the difference does not matter. The script-cost hint says where it does, from what the running application measured, and what is cheaper.

Two tools already say what is cheaper without running anything: `binding_tier_explain` names the tier of each expression and a verified rewrite, and the `decl-*` lints of `qml_lint` name the declaration a handler stands in for (docs/declarative.md). They cannot say which of the many expressions in JavaScript cost enough to be worth the change. The hint can, because it counts the time.

## Asking for it

`app_frames` with `script` (the inspector, `ljos run --mcp`). It reads the same recording as `bindings`: `record: true` starts it, held by the client as the frames are (`holder`), and it ends with `record: false` or when the holder goes away.

```json
{"script": true, "record": true}
```

Use the application, then ask again:

```json
{"script": true}
```

Nothing is measured until a client records: a binding run pays one flag check, and the recording asks for no work of its own. Off a recording the answer says so and lists nothing.

## What is counted

While the recording runs, the engine counts for each binding and handler the time its runs spent in their own JavaScript: from the time the run enters QuickJS until it returns, less the bindings and handlers it set off, which are measured on their own. Native work called from the script (a model's method, a text measured) counts as the script's. What runs in tier 0 (a property path), tier 1 (the binding VM) or as code compiled ahead of time never enters JavaScript and is never counted. A tier-1 program that gives up while it runs and falls back to JavaScript is counted from there.

## The threshold

A binding or handler is hinted when either holds:

- **run**: one of its runs spent at least **2 ms** in script (`min_run_ms`);
- **share**: its script took at least **10 %** of the time all bindings and handlers took together while recorded (`busy`), and at least **1 ms** in all (`min_share`).

DECISION: 2 ms is an eighth of a 60 Hz frame (16.7 ms): a handful of such runs in one frame, or on one click, miss it. It is half the slice after which the engine logs a script as slow (4 ms), so the hint comes before that warning. A small expression that is only written in JavaScript runs far below it, so a hint is never noise from script that is merely there.

DECISION: the share catches script that is cheap each time and costly in all: a delegate's binding run for every row, a handler on every frame of an animation. A tenth of the busy time is where moving it pays visibly; the 1 ms floor keeps a recording in which almost nothing ran from hinting anything.

DECISION: time per frame and per interaction is judged through the time of one run, which is what the engine measures without reading a clock for every frame; the trace file (`export`) shows how the runs fall into frames.

An agent can lower or raise both: `min_run_ms` (at least 0) and `min_share` (0 to 100). They go only with `script`.

## Each hint

```json
{
  "code": "script-cost", "severity": "hint",
  "kind": "binding", "type": "Item", "name": "heavy",
  "at": "file:///app/Main.qml:5:5", "file": "file:///app/Main.qml", "line": 5, "column": 5,
  "runs": 1, "script_runs": 1, "script_us": 6210, "longest_script_us": 6210,
  "share_percent": 97.4, "criteria": ["run", "share"],
  "cheaper": {"form": "rust", "text": "Move the heavy work into Rust …", "reason": "statement: for"},
  "guide": "docs/script-cost.md"
}
```

The answer lists the hints with the most script time first (20, `max` up to 200), with the recording's length, its busy time, its script time and the thresholds. The text says each hint as `hint script-cost: …` and its cheaper form.

A hint is never an error: `ljos check` and the audit do not report it, and nothing fails because of it.

## The cheaper form

Tried in this order:

1. **rewrite**: the form `binding_tier_explain` offers, which means the same and runs in tier 0 or 1, checked against the compiler before it is given (`source`, `tier`).
2. **declarative**: the declaration a `decl-*` lint names on the same lines (`code`, `guide`): a binding for a handler that only copies a value, a state for a handler that sets a mode.
3. **rust**: moving the heavy work into Rust, as a native type (ARCHITECTURE.md §5.5) whose property the binding reads, or, for a file the application ships, compiling it ahead of time (§5.4) where the expression keeps to the typed subset (docs/aot-coverage.md). `reason` says why the compiler leaves it to JavaScript, and the text adds what else helps.

The first two need the authoring tools, which `ljos run` links: it reads and compiles the file away from the application, after the UI thread has answered, so asking never holds up a frame. An application that links the inspector but not the authoring tools gives the third form.

## Not yet

- **Workers.** Script off the UI thread (`WorkerScript`, which has no runtime in Ljós yet) is not measured. A hint for it needs the worker's own JavaScript host to count its script time per handler as this engine does, switched on and off by the same recording, and to send its rows to the inspector over the bounded message path a worker talks to the engine by (`UiHandle`), so that measuring a worker never blocks the UI thread and nothing is measured while nobody records. Its cheaper form differs too: work already off the UI thread costs a frame only through the messages it posts, so the hint would weigh the time to the result rather than the frame.
- **The observer page.** The Bindings page can show the hints beside the profile; it asks `app_frames` with `script` as it asks `bindings`.
