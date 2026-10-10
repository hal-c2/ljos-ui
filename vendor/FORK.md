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

`mise run oxc:diff` prints the full diff against the registry tarball. Everything outside `src/qml/` is about 65 lines in four files, and the error recovery below touches none of them:

| File | Change |
|---|---|
| `src/lib.rs` | `pub mod qml;`, and `Parser::parse_qml(self) -> qml::QmlParserReturn<'a>`, which builds a `ParserImpl` the way `Parser::parse` does and calls its `parse_qml`. |
| `src/diagnostics.rs` | One constructor, `qml(message: &'static str, span: Span)`, for QML-mode errors. |
| `src/lexer/mod.rs` | A `pub(crate) qml: bool` field on `Lexer`, initialised to `false`. QML mode sets it. |
| `src/lexer/string.rs` | When `qml` is set, a string literal may contain line breaks: two match arms keyed on `$lexer.qml` and a cold-path method `read_string_literal_over_lines`. Plain JavaScript lexing is unchanged. |
| `src/qml/ast.rs` (new, 172 lines) | The QML tree: `QmlDocument`, `QmlPragma`, `QmlImport`/`QmlImportSource`, `QmlQualifiedName`, `QmlObject`, `QmlMember`, `QmlBinding`/`QmlBindingValue`, `QmlPropertyDeclaration`, `QmlTypeName`, `QmlSignalDeclaration`/`QmlSignalParameter`, `QmlInlineComponent`, `QmlEnumDeclaration`/`QmlEnumMember`. All nodes are arena-allocated, and leaves are ordinary Oxc `Expression`, `Statement` and `Function` nodes. |
| `src/qml/mod.rs` (new, 530 lines) | The parser: `parse_qml_document` reads pragmas and imports, then exactly one root object. Contextual keywords (`property`, `signal`, `required`, `readonly`, `default`, `component`, `enum`) are found by lookahead. `{ ... }` after a binding is an object literal or a block, decided by checkpoint and rewind. Calls `qml_recover` for every import, pragma and object member. |
| `src/qml/recovery.rs` (new, 212 lines) | Member-level error recovery, see below. |

`Cargo.toml` is byte-for-byte the published one (same name and version), so `[patch.crates-io]` accepts it.

### Error recovery

Oxc reports almost every syntax error as *fatal*: `set_fatal_error` stores it, moves the lexer to the end of the file and the parser unwinds on dummy nodes. QML mode keeps that machinery and contains it. `ParserImpl::qml_recover` (`src/qml/recovery.rs`) runs one unit (an object member, an import, a pragma) under a checkpoint. If the unit ends with `fatal_error` set, it takes the error and files it as an ordinary diagnostic, rewinds to the unit's start, skips to a synchronisation point and returns `None`; the caller carries on with the next unit. A failed unit is omitted from the tree (no error node), so `qml_front` and the IR need no changes. Why rewind and re-scan: the skip then does not depend on how far the JavaScript parser got.

- **Member skip.** From the member's first token (always consumed, so progress is guaranteed), tracks `(`, `[`, `{` and template `${` on a stack; a closer pops to its matching opener (`onClicked: { foo( }` ends at the `}`). Stops after a `;` or before a `}` at depth zero, before a new-line token that starts a member (`name:`, `Type {`, `a.b`, `function`, `enum`, or a declaration word and a name), and, when brackets are still open, before such a token that is indented no deeper than the broken member.
- **Header skip** (import, pragma): to the end of the line or the next `;`.
- **Unclosed objects** at end of file are a non-fatal error and keep their members. The object loop tests `Eof` and `fatal_error` instead of `has_fatal_error()`, which is also true at an `Undetermined` token.
- **Root** without `{` keeps its members (non-fatal error); a second root object is a non-fatal error. `panicked` is now only set when the root has no type name.
- **Import names** must not continue on the next line (`parse_qml_import_name`), or a half-typed `import Foo.` would swallow the root object's type.
- **Diagnostics** are sorted by position and de-duplicated in `parse_qml`, because re-lexing a skipped region reports a lexer error a second time.
- **Rebase notes.** Recovery depends on `checkpoint_with_error_recovery`, `rewind`, `fatal_error`, `errors`, `lexer.errors`, `state.{cover_initialized_name,trailing_commas,not_parenthesized_arrow}` (cleaned of the failed attempt's keys), `re_lex_template_substitution_tail` and `Token::is_on_new_line`. If a rebase renames one, `src/qml/recovery.rs` is the only file to fix. Tests: `crates/qml_syntax/tests/recovery.rs` and `fuzz.rs`.

### Known limitations (not fixed yet)

- **Recovery limits.** A regex literal that contains a bracket or quote, inside a member that is itself broken, can confuse the skip (the skip does not re-lex `/` as a regex). An unterminated string swallows the rest of the file, because QML strings may span lines. A diagnostic is placed where the parser noticed the problem, which for an unclosed bracket can be a later line than the cause. A failed member is omitted whole, even when only its value is broken. A nested object whose header is broken (`Rect` without `{`) is omitted with its body.
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
