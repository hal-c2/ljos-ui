---
title: MCP Servers for Agents
description: Connect coding agents to Ljós with the authoring MCP server (ljos mcp) and the live application inspector (ljos run --mcp and ljos attach).
section: start
order: 7
---

# MCP Servers for Agents

This page explains how to connect coding agents to Ljós's two Model Context Protocol (MCP) servers: the authoring server (`ljos mcp`) for static checking, formatting and project queries, and the live inspector (`ljos run --mcp` and `ljos attach`) for inspecting and driving a running application.

## Two servers, one model

| | Authoring server (`ljos mcp`) | Live inspector (`ljos run --mcp`) |
|---|---|---|
| **Started by** | `ljos mcp --project <dir>` | `ljos run --mcp[=<caps>] <file>` (relayed to stdio by `ljos attach`) |
| **Transport** | stdio | Streamable HTTP on `127.0.0.1:0` or a `0600` Unix socket; stdio via `ljos attach` |
| **Needs a running window** | No | Yes |
| **Default rights** | Read-only analysis | `inspect` (read-only) |

Both servers use the same addressing model as screen readers and behaviour tests: accessible roles and names, `objectName`, QML `id` and snapshot references.

## The authoring server (`ljos mcp`)

Start the authoring server over standard input and output:

```sh
ljos mcp --project .
```

By default, `ljos mcp` is read-only and confined to the `--project` directory (symbolic links pointing outside the project directory are refused with `path_denied`). Grant `--caps fs` to let the agent write formatted files, apply fixes or scaffold components, and `--caps run` to let it execute test targets and feature scenarios:

```sh
ljos mcp --project . --caps fs,run
```

### Authoring tools

| Tool | What an agent can do | Capability needed |
|---|---|---|
| `qml_check` | Parse and compile QML files; returns structured diagnostics with file, line, column, severity, code and optional fixes | — |
| `qml_lint` | Run accessibility, internationalisation and design lints (`a11y-unnamed-interactive`, `motion-bounce`, `i18n-qstr-non-literal` and so on) with fix guidance | — |
| `qml_format` | Lay out QML files; returns a unified diff by default or writes files in place when `write: true` is passed | `fs` to write |
| `qml_edit_apply` | Make one structured edit of a QML file (set or remove a binding; insert, move or remove an object, named by `id` or path), leaving every other byte as written; answers the splices, or writes the file when `write: true` is passed | `fs` to write |
| `blocks_list` | List the project's functional blocks and their contracts (state, inputs, actions, events, slots), read from `# block` qmldir comments and `ljostypes.json` without running the app | — |
| `qml_apply_fix` | Apply a diagnostic's suggested fix (dry-run by default) | `fs` |
| `type_lookup` | Inspect a QML type's properties, signals, methods, enums, base chain, default property and attached types | — |
| `api_status` | Check the support status of `Module.Type[.member]` (`supported`, `partial`, `dropped` or `unsupported`), with the reason and proving `@api-` scenario | — |
| `docs_search` | Search Ljós documentation and ledger notes (links out go only to Qt documentation, never source code) | — |
| `binding_tier_explain` | Show which tier a binding expression runs in (`0` for property paths, `1` for the binding VM, or `JS` for JavaScript), why, and how to rewrite it for a cheaper tier | — |
| `design_palette`, `design_contrast`, `design_motion`, `design_rules` | Generate a colour palette from a seed, measure contrast ratios, get motion durations/curves and read design rule limits | — |
| `qt_compare` | Render and compare against a local Qt installation | `run` |
| `features_list` / `features_run` | List Gherkin scenarios by tag or path, or start a background run job | `run` to execute |
| `kittest_run` | Run a named Rust test target as a background job | `run` |
| `run_status` / `run_cancel` | Poll or cancel a background test run started by `features_run` or `kittest_run` | — |
| `component_scaffold` | Create a new QML component and its `.feature` skeleton | `fs` to write |
| `i18n_extract` | Report missing and stale translation catalogue entries | — |
| `project_info` | Return the Ljós version, available modules, ledger summary and granted capabilities | — |

The authoring server also exposes resources (`ljos://types/{module}/{type}`, `ljos://ledger[/{module}/{type}]`, `ljos://docs/{path}`, `ljos://features/{path}`, `ljos://features/steps`, `ljos://lints`, `ljos://project/qml-files`) and workflow prompts (`write-component`, `port-from-qtquick`, `add-accessibility`, `design-review`, `localise`, `write-feature`, `test-app`, `embed-in-rust`).

### Skills

The same workflows ship as [Agent Skills](https://agentskills.io) in the repository's `skills/` directory, one directory per skill, so that an agent picks one by what you ask for instead of you naming a prompt:

| Skill | Prompt |
|---|---|
| `writing-qml-components` | `write-component` |
| `porting-from-qt-quick` | `port-from-qtquick` |
| `adding-accessibility` | `add-accessibility` |
| `reviewing-qml-design` | `design-review` |
| `localising-qml` | `localise` |
| `writing-scenarios` | `write-feature` |
| `testing-qml-apps` | `test-app` |
| `embedding-qml-in-rust` | `embed-in-rust` |

A skill calls the authoring server's tools, so it needs the `ljos` command on `PATH` and the server connected under the name `ljos`. To install the skills for Claude Code in one project, copy the directories into the project's `.claude/skills/`:

```sh
mkdir -p .claude/skills
for skill in <ljos-ui>/skills/*/SKILL.md; do cp -r "$(dirname "$skill")" .claude/skills/; done
```

Copy them into `~/.claude/skills/` instead to have them in every project. Other agents that read Agent Skills take the same directories; see where your agent looks for them.

## The live inspector (`ljos run --mcp` and `ljos attach`)

Start your application with `--mcp` to enable the live inspector:

```sh
ljos run --mcp=inspect,input,mutate,eval Main.qml
```

Passing `--mcp` with no value enables `inspect` only, where every offered tool is read-only. Each capability unlocks a group of actions:

- `inspect`: Read application status, accessibility snapshots, object trees, properties, binding graphs, screenshots, logs, metrics, processes, mailboxes, models, traces, wakes and design audits.
- `input`: Synthesise clicks, typing, key presses, scrolling, touch gestures, window resizing, appearance changes and virtual clock advances (`app_advance`, on hosts with a virtual clock).
- `mutate`: Set property values, edit writable model cells, invoke methods or signals, trigger a hot reload (`app_reload`), and import a module from another directory (`app_alias`).
- `eval`: Run JavaScript expressions in the context of an object (`app_eval`) or bind a property to a live JavaScript expression (`app_set_property` with `binding`).

When the application starts, it binds a loopback HTTP listener (or a Unix socket with `--mcp-socket <PATH>`) and writes the address and a bearer token to `target/ljos/mcp-<pid>.json` (mode `0600`). Only local clients that present this bearer token are answered; requests without the current token receive HTTP `401`.

### Running with no window (`--headless`)

`ljos run --headless` runs the application with no window and needs no display, so an agent can drive it on a server, in a container or in CI:

```sh
ljos run Main.qml --headless --watch --mcp=inspect,input,mutate,eval
```

The root is laid out at `--size WxH`, or else at its own size, and every inspector tool works as it does on a window: `app_screenshot` returns the scene's picture, input acts on the scene, `app_resize` sets the size of the next picture, and `--watch` applies saves and keeps the application's state. Timers and animations run on the real clock, and the process sleeps when nothing is due. The inspector line is written to standard error as usual. There is no system clipboard, file dialog, URL opener or screen reader, and a `Window` is an item of the scene, not a platform window. A run with neither `--mcp` nor `--watch` lasts until the application calls `Qt.quit()`.

### Connecting an stdio MCP client with `ljos attach`

Most coding agents start MCP servers as stdio subprocesses. Point your agent's MCP configuration at `ljos attach`:

```sh
ljos attach .
```

`ljos attach` finds the newest `target/ljos/mcp-<pid>.json` under the directory, relays JSON-RPC messages over stdio to the running application, and re-reads the token file on every message so it automatically follows the application across restarts.

### Live inspector tools

| Tool | What an agent can do | Capability |
|---|---|---|
| `app_status` | Read windows, size, scale, appearance, focus, clock kind (`real` or `virtual`), active animations, painter backend, startup timings, reload state and granted capabilities | `inspect` |
| `app_snapshot` | Get an indented accessibility-tree outline with short refs (`[e3]`), `id=` and `obj=` labels, virtual list windows and optional `pointer: true` lines for items with a `MouseArea` or `TapHandler` that have no accessible role | `inspect` |
| `app_find` | Search for nodes by `role`, `name` (exact, substring or regex), `objectName`, `text` or state | `inspect` |
| `app_inspect` | Inspect one object in depth: type chain, source location, properties with provenance (`assigned`, `bound` at tier `0`/`1`/`JS`, `animated`, `host`, `default`), geometry, window singletons, and binding `dependencies` or `bindings` | `inspect` |
| `app_binding_graph` | Read the sources and dependents of a property and why it last evaluated | `inspect` |
| `app_get_property` / `app_set_property` | Read or write a property, set a live `binding` expression (with `eval`), cover an existing binding with `keep_binding`, `revert` a devtools edit, or write a cell of a model with `row` | `inspect` / `mutate` (+ `eval` for `binding`) |
| `app_invoke` | Emit a signal or call an invokable QML/Rust method on an object | `mutate` |
| `app_click`, `app_type`, `app_press`, `app_scroll`, `app_touch` | Send pointer, keyboard, wheel and touch input through the scene's real input router and focus tree; returns a snapshot delta after the next frame | `input` |
| `app_batch` | Run a sequence of tool steps in one call, stopping at the first error and returning one combined outline delta at the end | per step |
| `app_screenshot` | Render the window or a specific node to a PNG (up to 1280 px) using the R backend, even when the window is occluded or headless | `inspect` |
| `app_resize` / `app_set_appearance` | Resize the window or switch between light/dark appearance, high contrast, reduced motion and font scale | `input` |
| `app_reload` | Force a recompile and hot reload; returns the patch summary or compiler diagnostics | `mutate` |
| `app_alias` | Import a module from another directory inside the project (`{module, dir}`), or go back to its own (`dir: null`), reloading and keeping state; returns the reload `generation` | `mutate` |
| `app_logs` / `app_metrics` | Read console logs, JS errors with QML file and line, frame and paint timings, and memory breakdowns (`memory: {}`, `owners`, `mappings`) | `inspect` |
| `app_processes` | List threads, running `Timer`s (`kind: "timer"`), animations, supervisors (`kind: "supervisor"`), recent faults (`kind: "fault"`), scheduled UI wakers (`kind: "waker"`) and bounded `UiInbox` mailboxes (`kind: "mailbox"`) | `inspect` |
| `app_objects` | Walk every object below a root with `@handle`, type and approximate bytes; list `models` or read rows of a `model`; list loaded `components` and their instances; or `mark` live objects and check `since_mark` to find leaks after an undone action | `inspect` |
| `app_trace` | Record a bounded ring of an object's emitted signals, property changes, destruction and (with `bindings: true`) binding evaluations with their cause, cost and whether the value changed | `inspect` |
| `app_frames` | Record up to 600 frames, UI thread `wakes` (or inspect `last_input` / a single `wake` in order), profile hot `bindings` and binding loops, sample component `growth`, or `export` a Chrome/Perfetto trace file to `~/.local/state/<app>/traces/` | `inspect` |
| `app_audit` | Run the live design and accessibility audit across the rendered scene (optionally testing across a matrix such as `font-scale=2` or with `act: true` under `input`) | `inspect` (`input` for `act`) |
| `app_watch` | Park on the server thread until `frames`, `passes`, `logs`, `reloads`, `trace`, `wakes`, `models` or `rests` advance past `since`, waking nothing in the application | `inspect` |
| `app_eval` | Evaluate JavaScript in the context of a target object (with access to `pragma Singleton` and native singletons) under a time deadline and 64 KB output cap | `eval` |

### Addressing targets

Every tool that takes a `target` accepts:

- **Snapshot refs** (`e3` or `ref=e3`): Short identifiers from the latest `app_snapshot`. If the object was destroyed, the tool returns `stale_ref`.
- **Playwright-style locators**: Durable across hot reloads, such as `role=button[name="Delete"]`, `text=Save`, `#objectName` (which also matches a QML `id` when no object has that `objectName`, such as `#app` on the root window), chained with `>>`, `nth=` and `name~=regex`.
- **Object handles** (`@12.3`): Generation-tagged handles returned by `app_objects`, `app_inspect` and `app_processes` that name any object, including non-visual `Timer`s, models and animations.

### Secret redaction

Fields with a hidden echo mode (such as `TextInput { echoMode: TextInput.Password }`) have their `text`, `displayText`, `selectedText` and `preeditText` masked as `<redacted>` automatically. If that secret string appears inside another property, an `objectName`, a QML `id`, a window title, a log line, a fault or a script return value, the inspector masks it there too, and paints over the item's bounds in `app_screenshot`. Starting `ljos run` with `--mcp-redact` masks all editable text fields and disables `app_eval` and script invocation completely.

## See also

- [Command-line reference](./cli.md)
- [Observer](./observe.md)
- [Hot reload](./hot-reload.md)
- [Testing](./testing.md)
