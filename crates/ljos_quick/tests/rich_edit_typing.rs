//! Typing into a rich `TextEdit` does not write the document out: the HTML
//! is made when `text` is read or a handler follows it, not on every key.
//! A key in a long rich document then costs about what one in the same
//! plain document does.
//!
//! The numbers:
//!
//! `cargo test --release -p ljos_quick --test rich_edit_typing -- --ignored --nocapture`

mod common;

use std::time::Instant;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::text_input::{TextEditor, accept_text};

/// "Some words and bold ones and slanted ones too." is 47 characters.
const PARAGRAPH: usize = 47;

/// A rich text of about `chars` characters: paragraphs with a bold and an
/// italic word each.
fn rich_document(chars: usize) -> String {
    let paragraph = "<p>Some words and <b>bold ones</b> and <i>slanted ones</i> too.</p>";
    paragraph.repeat(chars / PARAGRAPH + 1)
}

/// The characters of [`rich_document`] as plain text.
fn plain_document(chars: usize) -> String {
    "Some words and bold ones and slanted ones too.\n".repeat(chars / PARAGRAPH + 1)
}

/// An editor holding a document of `chars` characters, with the cursor in
/// the middle; `observed`: a handler follows `text`.
fn editor(chars: usize, rich: bool, observed: bool) -> (Engine, ObjectId) {
    let e = engine();
    let handler = if observed {
        "property int seen: 0; onTextChanged: seen++"
    } else {
        ""
    };
    let format = if rich { "RichText" } else { "PlainText" };
    let source = format!(
        r#"import QtQuick
TextEdit {{
  width: 600; textFormat: TextEdit.{format}
  {handler}
}}
"#
    );
    let root = load(&e, &source);
    let document = if rich {
        rich_document(chars)
    } else {
        plain_document(chars)
    };
    set(&e, root, "text", document.as_str().into());
    let length = num(&e, root, "length");
    set(&e, root, "cursorPosition", (length / 2.0).into());
    (e, root)
}

/// Microseconds per key, typing `keys` characters one at a time.
fn us_per_key(chars: usize, rich: bool, keys: usize, observed: bool) -> f64 {
    let (e, root) = editor(chars, rich, observed);
    // The first key makes what the editor keeps beside the text.
    assert!(accept_text(&e, root, "x"));
    let t = Instant::now();
    for _ in 0..keys {
        assert!(accept_text(&e, root, "y"));
    }
    let us = t.elapsed().as_secs_f64() * 1e6 / keys as f64;
    // What was typed is in the text once it is read.
    let text = get(&e, root, "text").to_js_string();
    assert!(text.as_str().contains(&"y".repeat(keys)), "{text}");
    us
}

#[test]
#[ignore]
fn bench_typing_in_a_rich_document() {
    us_per_key(1_000, true, 20, false);
    for chars in [1_000, 10_000, 100_000] {
        let plain = us_per_key(chars, false, 200, false);
        let lazy = us_per_key(chars, true, 200, false);
        let eager = us_per_key(chars, true, 200, true);
        println!(
            "{chars} characters: plain {plain:.1} us per key, rich {lazy:.1}, \
             rich with onTextChanged {eager:.1}"
        );
    }
}

/// While nothing reads `text`, a key leaves the HTML unwritten; reading
/// writes it, and the next key leaves it unwritten again.
#[test]
fn typing_leaves_the_html_unwritten_until_text_is_read() {
    let (e, root) = editor(10_000, true, false);
    let text = TextEditor::TEXT.index;
    for _ in 0..20 {
        assert!(accept_text(&e, root, "y"));
        assert!(e.is_made_on_read(root, text));
    }
    let read = get(&e, root, "text").to_js_string();
    assert!(read.as_str().contains(&"y".repeat(20)));
    assert!(!e.is_made_on_read(root, text));
    assert!(accept_text(&e, root, "z"));
    assert!(e.is_made_on_read(root, text));
}

/// The timing guard: a key in a 10,000-character rich document costs
/// little more than one in the same plain document. Loose, so a busy
/// machine does not fail it; writing the HTML on every key made it some
/// thirty times dearer.
#[test]
fn a_key_in_a_long_rich_document_costs_about_what_a_plain_one_does() {
    us_per_key(1_000, true, 20, false);
    let best = |rich| {
        (0..3)
            .map(|_| us_per_key(10_000, rich, 100, false))
            .fold(f64::MAX, f64::min)
    };
    let plain = best(false);
    let rich = best(true);
    assert!(
        rich < plain * 5.0,
        "a key took {plain:.1} us in plain text and {rich:.1} us in rich text"
    );
}

/// What reads `text` sees every key, made when it reads.
#[test]
fn text_read_after_typing_has_what_was_typed() {
    let (e, root) = editor(100, true, false);
    set(&e, root, "cursorPosition", 0.0.into());
    assert!(accept_text(&e, root, "A"));
    assert!(accept_text(&e, root, "B"));
    let text = get(&e, root, "text").to_js_string();
    assert!(text.as_str().contains(">ABSome words"), "{text}");
    assert!(accept_text(&e, root, "C"));
    let text = get(&e, root, "text").to_js_string();
    assert!(text.as_str().contains(">ABCSome words"), "{text}");
}

/// A handler of `textChanged` hears of every key and reads the new text.
#[test]
fn an_observed_text_changes_with_every_key() {
    let (e, root) = editor(100, true, true);
    let before = num(&e, root, "seen");
    assert!(accept_text(&e, root, "A"));
    assert!(accept_text(&e, root, "B"));
    assert_eq!(num(&e, root, "seen"), before + 2.0);
}
