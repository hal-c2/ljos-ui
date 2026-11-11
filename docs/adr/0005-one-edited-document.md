# ADR 0005: A rich-text editor edits one document

- **Status:** proposed, 2026-11-10.
- **Terms:** [ARCHITECTURE.md](../ARCHITECTURE.md) §7.5 (text, rich
  text, editing); [ADR 0004](0004-compat-lowering-before-the-ir.md)
  (compat is lowered in front of the IR); [CONTEXT.md](../../CONTEXT.md).

## Context

A `TextEdit` with rich text holds the text twice:

- **`TextModel`** (`ljos_text::model`) holds the characters, the cursor
  and the undo history as edits.
- **`RichEdit`** (`ljos_quick::text_input::rich`) holds a `RichText` with
  the same characters and their formats.

Every edit reaches the model first. `sync_doc` then diffs the two strings
(the same start, then the same end) to work out what the edit did, and
splices that into the document. Two side channels cover what a diff
cannot tell: a `hint` says where the edit starts when a run of equal
characters makes that ambiguous, and a `pending` fragment carries the
formats of a fragment a program inserts.

Undo runs twice as well. The model keeps edits. `RichEdit` keeps a whole
cloned document before every step, so a long editing session holds one
copy of the document per undo step.

This costs three things:

- **Rules defined twice.** `TextModel` and `RichText` each have their
  own `ends_paragraph`, and `RichText::block_range` splits on `'\n'`
  alone. Copies like these drift: `prev_word` stopped on the `'\n'` byte
  while `paragraph_bytes` started after it (#308, fixed since).
- **Derived meaning.** Formats follow characters by inference from a diff
  rather than by operation, so what format an edit leaves is decided after
  the fact, from two strings.
- **Memory.** Undo memory grows with the size of the document times the
  number of steps, not with the size of the edits. This works against
  beating qml6 on memory.

## Decision

1. **One document.** While a `TextEdit` edits rich text, its `RichText`
   is the model. An edit is an operation on that document (insert, remove,
   format, split, join), and the operation says what changed. Nothing
   diffs strings. Plain text is the same document with one format.
2. **One undo log, of operations.** An undo step holds the operations
   and what they replaced, never a copy of the document.
3. **One set of text rules.** Words, paragraphs, UTF-16 units and
   grapheme boundaries each have one definition, over the document's text.
   `TextModel`'s navigation becomes those functions, or calls them.
4. **The formats are adapters.** Plain, styled, HTML and Markdown readers
   build the document, and the HTML and Markdown writers write it out.
   Changing `textFormat` converts the document; it does not read the text
   again. This already holds since #313, and stays so.
5. **Qt-measured quirks are compat, not the design.** Where Qt 6.11
   differs from HTML or CommonMark for no reason a document needs, the
   quirk is marked where it is decided with one greppable comment,
   `// Qt 6.11: … (6.12 the same)`, for example:
   - a fenced block does not wrap, while an indented one does;
   - `-qt-paragraph-type:empty`;
   - the `toHtml` boilerplate.

   Today there is one mode, and it keeps Qt's behaviour. When the Ljós
   text API differs, each quirk becomes a member of the Ljós type whose
   default is the spec's behaviour. The compat pass (ADR 0004) writes
   Qt's value into the files it lowers, the way it already writes Qt's
   layout spacing. The runtime still cannot tell a lowered file from one
   written in the API, so there is no mode switch and no profile.
6. **Documents, not pages.** Rich text covers paragraphs, inline styles,
   lists, quotes, code, tables, images and links, plus the CSS that
   applies to those (a cascade at the `Sheet` seam is in scope). It does
   not cover positioning, floats, flex or grid, scripts, forms, or fetching
   anything. A web page goes to `WebEngineView`.

## Consequences

- `sync_doc`, `same_start`/`same_end`, `hint`, `pending` and
  `RichEdit::{undo, redo}` go away. ARCHITECTURE.md §7.5 "Editing (M2)" is
  rewritten to describe this.
- Format inheritance (what format a typed character takes, measured on
  qml6) becomes a property of the insert operation, so it is tested by
  calling that operation rather than through the diff.
- The change is staged behind the cell paragraph record (#364) and the
  single paragraph writer (#372), which come first so that the document is
  a complete model before it becomes the only one. Every existing
  rich-text editing scenario must pass unchanged, and `cargo test -p
  ljos_quick --test budget` must not regress.
- Plain `TextInput` and `TextEdit` may keep `TextModel` as their document
  if that is cheaper. Rule 3 still holds: one definition of each text
  rule.
- Rule 6 gives future work a test to apply. A CSS cascade at the `Sheet`
  seam fits within it, and a layout engine does not.
