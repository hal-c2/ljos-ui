# The oxc_parser fork

`vendor/oxc_parser` is [`oxc_parser`](https://crates.io/crates/oxc_parser) **0.144.0** from crates.io with a QML mode added. The root `Cargo.toml` puts it in place of the registry copy:

```toml
[patch.crates-io]
oxc_parser = { path = "vendor/oxc_parser" }
```

So every crate that names `oxc_parser = "0.144"` gets the fork, and the QML front end and anything else built on Oxc share one lexer, one arena and one AST. Nobody depends on the fork by path. `crates/qml_syntax` is its public face: callers write `qml_syntax::parse` and `qml_syntax::ast::QmlDocument`, so a rebase or a later rename of the fork (for example `oxc_parser_qml`, if it is ever published) only touches that crate.

This is the only copy of the fork. qml-solid will depend on `qml_syntax` from this repository by git `rev` and not keep its own copy (ARCHITECTURE.md §4, decision D7).

## Why a fork

QML mode is a recursive-descent parser for QML's declarative shell (imports, pragmas, objects, bindings, declarations). It calls straight into Oxc's JavaScript parser for every binding value, handler and function. To do that it needs `ParserImpl`, the lexer, `checkpoint_with_error_recovery`, `fatal_error` and `lookahead`, which are all `pub(crate)`. It cannot be a separate crate on Oxc's public API (06 §2.1).

## What the fork changes

`mise run oxc:diff` prints the full diff against the registry tarball. Everything outside `src/qml/` is about 65 lines in four files:

| File | Change |
|---|---|
| `src/lib.rs` | `pub mod qml;`, and `Parser::parse_qml(self) -> qml::QmlParserReturn<'a>`, which builds a `ParserImpl` the way `Parser::parse` does and calls its `parse_qml`. |
| `src/diagnostics.rs` | One constructor, `qml(message: &'static str, span: Span)`, for QML-mode errors. |
| `src/lexer/mod.rs` | A `pub(crate) qml: bool` field on `Lexer`, initialised to `false`. QML mode sets it. |
| `src/lexer/string.rs` | When `qml` is set, a string literal may contain line breaks: two match arms keyed on `$lexer.qml` and a cold-path method `read_string_literal_over_lines`. Plain JavaScript lexing is unchanged. |
| `src/qml/ast.rs` (new, 172 lines) | The QML tree: `QmlDocument`, `QmlPragma`, `QmlImport`/`QmlImportSource`, `QmlQualifiedName`, `QmlObject`, `QmlMember`, `QmlBinding`/`QmlBindingValue`, `QmlPropertyDeclaration`, `QmlTypeName`, `QmlSignalDeclaration`/`QmlSignalParameter`, `QmlInlineComponent`, `QmlEnumDeclaration`/`QmlEnumMember`. All nodes are arena-allocated, and leaves are ordinary Oxc `Expression`, `Statement` and `Function` nodes. |
| `src/qml/mod.rs` (new, 461 lines) | The parser: `parse_qml_document` reads pragmas and imports, then exactly one root object. Contextual keywords (`property`, `signal`, `required`, `readonly`, `default`, `component`, `enum`) are found by lookahead. `{ ... }` after a binding is an object literal or a block, decided by checkpoint and rewind. |

`Cargo.toml` is byte-for-byte the published one (same name and version), so `[patch.crates-io]` accepts it.

### Known limitations (not fixed yet)

- **Error recovery is fatal-only.** The first syntax error sets `panicked` and parsing stops, with no partial tree. Member-level recovery (resynchronise at `;`, at a newline followed by `name:`, at `}`) is the first planned improvement (ARCHITECTURE.md §5.1).
- **Annotations** (`@Foo`) are a fatal "not supported yet" error.
- **Diagnostics** are message plus span: no codes, no severity, one label.

These belong in the fork itself, under `src/qml/`, with tests in `crates/qml_syntax/tests/`.

## Rebase policy

From 06 §3.4:

- **Cadence.** Rebase every 4–8 oxc releases, or straight away when a security or parser-correctness fix lands. Do not rebase weekly. The workspace pins one exact version.
- **Containment.** Keep the patch confined to `lib.rs` (the `qml` module and `parse_qml`), `diagnostics.rs`, `lexer/{mod,string}.rs` and the `src/qml/` directory, so conflicts stay local. New QML work goes in `src/qml/`.
- **Drift check.** `mise run oxc:check` downloads the newest `oxc_parser` release into a scratch directory, applies the patch (the diff of oxc's own files plus a copy of `src/qml/`) and builds it standalone. It reports whether a rebase would be mechanical. Nothing in the repository changes.
- **Upstream.** The multi-line string flag in the lexer is the only change outside `qml/` that could be generic. Offer it upstream as a lexer option; if accepted, the fork shrinks to `src/qml/` plus visibility. Do not count on it.

### How to rebase

1. `mise run oxc:check` to see whether the patch applies and builds on the target release.
2. Replace `vendor/oxc_parser` with the new release's crate contents (`Cargo.toml`, `Cargo.toml.orig`, `README.md`, `src/`, `examples/`).
3. Re-apply the four-file patch (save `mise run oxc:diff` output from before step 2) and copy `src/qml/` back.
4. Update the `oxc_*` pins in the root `[workspace.dependencies]` to the same release; they have to match exactly.
5. Fix `src/qml/mod.rs` where it touches parser internals.
6. `cargo test -p qml_syntax -p qml_front` and `mise run parse` (495/495 on `corpus/qtdoc/examples`) must stay green.
7. Update the version in this file.

### Last drift check

2026-10-10: fork base 0.144.0, newest release 0.153.0. The patch applies with line offsets only, and the patched 0.153.0 builds.
