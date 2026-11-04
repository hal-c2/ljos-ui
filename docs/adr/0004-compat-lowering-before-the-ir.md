# ADR 0004: Qt's QML is lowered to Ljós's API before the IR

- **Status:** proposed, 2026-11-04. The pass (`qml_compat`), group
  lowering, `--compat lower`, `ljos migrate` and the first slice of the API
  (`Ljos.Ui`) are built as a spike on `dialect/phase-1`.
- **Terms:** [DIALECT.md](../DIALECT.md); ARCHITECTURE.md sections 5 (front
  end) and 12 (hot code update); ADR 0002 (the IR does not hard-wire native
  indexes) and ADR 0003 part 4 (lossless edits).

## Context

Ljós gets an API of its own (DIALECT.md): cleaned-up types and members,
typed properties, and scripts in a strict TypeScript subset. Qt's QML must
keep running unchanged. Two APIs over one runtime could meet in one of
three places:

1. **In the engine.** Natives answer to both names, and the engine knows
   which API a file was written in. Every native, lookup and tool carries
   both, forever.
2. **In the IR.** The compiler translates Qt names while it lowers to IR.
   The IR is Qt-agnostic, but the translation is spread through name
   resolution, the checker and the tiers. Neither `ljos migrate` nor
   ljos-web can reuse it without the compiler.
3. **In front of the front end.** A pass turns a Qt file's text into a
   Ljós file's text, then the one compiler compiles it.

## Decision

Compat is **a lowering pass in front of a Qt-agnostic IR** (option 3):

- **Text to text.** `qml_compat::lower` parses a file, decides whether
  each Qt name it uses has a Ljós name, and edits the text in place. It
  keeps every line where it was, so diagnostics, breakpoints and source
  maps of the written file still hold.
- **The Qt surface is in one place.** The pass reads Qt's type table
  (`qt_types`), and the API's table (`ljos.txt`) for what exists. The IR,
  the engine and new native types know only the Ljós API. Until the natives
  are renamed (DIALECT.md M5), the API's types are faces of them, with the
  same native and the same member indices.
- **All or nothing, by group.** A file is lowered whole or kept as written.
  Files that name one another are lowered together or kept together
  (`qml_compat::group`): by type name in a directory, by directory import,
  and by a module of the project imported by name. A kept file runs on the
  same runtime as today.
- **One pass, two uses.** At load (`Project::set_compat`, `--compat
  lower`), the lowered text is compiled and the written text is kept for
  reload. Offline, `ljos migrate --write` writes the same text. A migrated
  file is byte for byte what a lowered run compiled.
- **The checker is the API's, not compat's.** It runs on files that import
  `Ljos.Ui` themselves. A lowered file says so:
  `pragma Scripts: JavaScript` goes on its import's line, the checker does
  not hold it to its rules, and a migrated file compiles as it did. A
  person types a file by taking the pragma out.
- **Opt-in while the API is small.** The default is `--compat keep`, where
  the pass does not run at all.

## Consequences

- The runtime cannot tell a lowered file from one written in the API. The
  spike checks exactly this: the same PNG kept, lowered and migrated; the
  same feature results; the same corpus render JSON (DIALECT.md §10, §12).
- `qml_compat` is pure Rust over `qml_syntax` and `qt_types`, with no
  engine. ljos-web and the MCP tools link it as they link the parser, and
  it builds to wasm. Under ARCHITECTURE.md rule 5 it is front-end code: a
  runtime crate never depends on it, and an app that links no parser links
  no compat. A shipped bundle carries lowered IR.
- Run time costs nothing. Load time costs one parse and one walk per file,
  plus a union-find over the project, and only with `--compat lower`.
- Coverage is what the API has. A file naming anything the API lacks is
  kept, so the pass is never wrong by being incomplete. It is only unused.
- Reload must lower again. `Project::put` and `Project::forget` lower the
  file and its same-directory namers, then settle every group, which is
  O(files) per edit.
- Some Qt programs build names at run time (`obj[name]`, QML text in
  `createQmlObject`, `.qml` URLs from template literals). The pass cannot
  see these. It keeps the files that do this, and DIALECT.md R1 records the
  gap for URLs.
- Columns on a line with a rename map back by line only, until a span map
  exists (DIALECT.md R3).

## Rejected

- **Both names in the engine (option 1).** It doubles every lookup table
  and every tool's view of a type, and it never lets the Qt names go.
- **Translation inside the compiler (option 2).** It hides the translation
  in name resolution, can't be run offline as `ljos migrate`, and makes
  ljos-web carry the compiler to get compat.
- **Per-file lowering without groups.** The corpus sweep found it wrong. A
  kept file set `color` on a lowered sibling that has only `fill`.
