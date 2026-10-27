---
title: Porting from Qt Quick
description: Audit a Qt Quick codebase with ljos check --port, track gaps in an API ledger, gate CI with --deny-missing, and handle missing types.
section: language
order: 8
---

# Porting from Qt Quick

After reading this page, you will be able to inventory the Qt Quick types and members your application uses with `ljos check --port`, export machine-readable JSON reports, record decisions in a TOML API ledger, gate your build with `--deny-missing`, and handle any missing types or modules.

## Inventory your codebase with `ljos check --port`

Point `ljos check --port` at a directory of `.qml` files (or a single `.qml` file) to compare every type, property, signal, and method your QML uses against Ljós's built-in type table and Qt 6's `qmltypes` table:

```sh
ljos check --port path/to/qml
ljos check --port --tree examples/      # each group/project dir is its own project
ljos check --port --limit 10 path/to/qml
```

For a file that uses `Item` and `QtQuick.Controls`'s `Calendar`, `ljos check --port` prints:

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

Each item your QML touches is classified into one of three buckets:

- **`supported`**: Present in Ljós's type table (or marked `supported` in your ledger), or defined in your own QML project.
- **`missing`**: Present in Qt's `qmltypes` table, or in a known `Qt*` module that Ljós does not ship, and absent from Ljós's type table (or marked `partial`, `dropped`, or `unsupported` in your ledger).
- **`unknown`** ("cannot place"): Neither in Ljós nor in Qt's table—typically a C++ or Rust type registered at run time, or an unresolvable expression receiver.

Modules are ordered by `missing uses` so the module blocking the most call sites appears first; within a module, types and members with the most uses appear first. `--limit N` caps the `missing` list to the top `N` items per module. The `api_status` MCP tool in `ljos mcp` returns the same classification when an editor or agent queries a symbol.

By default, `ljos check --port` exits with `0` even when missing items are found; syntax errors are counted in `parse_errors` so a half-ported tree still produces a complete inventory.

## JSON output

Add `--json` to emit a single machine-readable JSON object on stdout:

```sh
ljos check --port --json path/to/qml > port-report.json
```

Top-level fields in the JSON report are:

- `files`, `parse_errors`, `type_uses`, `member_uses`
- `distinct`: `{ "types", "members", "supported", "missing", "unknown" }`
- `uses`: `{ "supported", "missing", "unknown" }`
- `modules`: array of `{ "module", "supported", "missing", "unknown", "missing_uses" }`
- `missing`, `partial`, `unknown`: arrays of `{ "module", "type", "member", "kind", "status", "uses", "files", "reason" }`

When you run `ljos check --port --tree --json`, the output has `{ "projects": [ { "name", "path", ...report }, ... ] }`.

## The API ledger (`--ledger`)

A ledger is a TOML file of `[[row]]` entries that records your team's decisions about specific QML APIs:

```toml
[[row]]
module = "QtQuick"
type = "Item"
member = "antialiasing"
kind = "property"
status = "supported"
scenarios = ["@api-QtQuick.Item.antialiasing"]

[[row]]
module = "QtQuick"
type = "Text"
member = "fontSizeMode"
kind = "property"
status = "partial"
reason = "parsed and stored; layout still uses pixelSize"

[[row]]
module = "Qt5Compat.GraphicalEffects"
type = "DropShadow"
kind = "type"
status = "dropped"
reason = "Qt 5 compat; use MultiEffect or Rectangle shadow"
```

Pass the ledger with `--ledger path/to/ledger.toml`. Each row requires:

- `module` and `type`, plus `member` for a property, method, signal, or attached property (`member` is omitted when `kind = "type"`).
- `kind`: `"type"`, `"property"`, `"method"`, `"signal"`, or `"attached"`.
- `status`: `"supported"`, `"partial"`, `"dropped"`, or `"unsupported"`.
- `reason`: required for `"partial"`, `"dropped"`, and `"unsupported"`, and shown in `ljos check --port` output.
- `scenarios`: optional list of `@api-...` tags linking the entry to feature tests.

A member row (`module` + `type` + `member`) takes precedence over a type row (`module` + `type`), which takes precedence over the built-in tables. A `partial` row still counts as `missing` (and is listed in its own `partial` section) because a half-implemented property can silently misrender an app until verified.

## Failing the build on gaps (`--deny-missing`)

Use `--deny-missing` in CI to fail the build whenever your QML uses an unapproved missing type or member:

```sh
ljos check --port --deny-missing path/to/qml
ljos check --port --deny-missing --ledger port-ledger.toml path/to/qml
```

When `distinct.missing > 0`, the command prints the report, writes `ljos: N types or members Ljós lacks` to stderr, and exits with code `1`. Unknown items (`cannot place`) do not fail the check.

## What to do about a missing type or member

When `ljos check --port` reports a missing type or member, you have five options:

1. **Check what you actually use**: Many QML files set properties that have no visual effect in their context (such as `smooth: true` on a solid-colour `Rectangle`). Remove the unused assignment or mark it `supported` in your project ledger if it is benign for your app.
2. **Replace Qt 5 compatibility modules**: Modules such as `Qt5Compat.GraphicalEffects` (`ColorOverlay`, `DropShadow`, `LinearGradient`) can often be replaced with plain `Rectangle` gradients, opacity, or `QtQuick.Shapes`.
3. **Provide the type in your application**: Drop a `.qml` file of the same name into your QML directory, or register a native Rust type with `#[derive(QmlObject)]`, and add a `supported` row to your ledger.
4. **Let non-goal 3D or timeline modules stand in as empty items**: When an application imports a module that Ljós does not draw—such as `QtQuick3D` or `QtQuick.Timeline`—its types automatically stand in as empty `Item`s. Their geometry, `id`s, children, and custom properties, signals, and functions remain intact, Ljós logs a single warning per module, and the rest of the window loads and runs around them.
5. **Drop or defer the feature**: If you remove the feature from your QML, record `status = "dropped"` with a `reason` in your ledger so future runs explain why; if the feature is waiting on Ljós support, record `status = "unsupported"` with a `reason` so the gap is tracked in one place.

## See also

- [QML in Ljós](overview.md)
- [Modules and imports](modules-and-imports.md)
- [Components and dynamic objects](components.md)
- [Properties and bindings](properties-and-bindings.md)
