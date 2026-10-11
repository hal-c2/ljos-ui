//! Streaming: a long Markdown answer arrives a few bytes at a time, and
//! every piece is shown as it comes. Reading and laying out the whole
//! document for each piece would cost the square of its length; a
//! [`RichDoc`] reads again only from its last block and lays out only
//! blocks that changed. This test counts both.

use ljos_text::rich::{DocParams, ParseOptions, RichDoc, TextFormat};
use ljos_text::{LayoutParams, TextEngine, TextStyle, WrapMode};

/// What an assistant might answer: every kind of block, ordinary lengths.
fn answer() -> String {
    let mut out = String::from(
        "# Reading a file line by line\n\n\
         There are **three** common ways to do this, and which one is right \
         depends on how large the file is and on what you do with each line.\n\n",
    );
    for section in 1..=8 {
        out.push_str(&format!(
            "## Approach {section}\n\n\
             The idea is simple: open the file, wrap it in a *buffered* reader and \
             take the lines one at a time. Nothing but the current line is held in \
             memory, so a file of any size works. See [the documentation]\
             (https://example.com/doc/{section}) for the details.\n\n\
             - it allocates one string per line, which is fine until it is not;\n\
             - errors surface line by line, so a bad byte in line {section}000 does \
             not lose the lines before it;\n\
             - the line ending is stripped for you.\n\n\
             ```rust\n\
             let file = File::open(path)?;\n\
             for line in BufReader::new(file).lines() {{\n    \
             handle(&line?, {section});\n\
             }}\n\
             ```\n\n\
             > Measure before choosing: for small files the difference is noise.\n\n\
             | file size | time | memory |\n\
             |-----------|------|--------|\n\
             | 1 MB | {section} ms | 8 KB |\n\
             | 1 GB | {section} s | 8 KB |\n\n"
        ));
    }
    out.push_str(
        "1. Start with the simple one.\n2. Measure.\n3. Only then reach for the others.\n",
    );
    out
}

fn params() -> DocParams {
    DocParams {
        style: TextStyle::sized(14.0),
        layout: LayoutParams {
            width: Some(420.0),
            wrap: WrapMode::WordWrap,
            ..LayoutParams::default()
        },
    }
}

/// `source` cut into pieces of about `size` bytes, never inside a character.
fn pieces(source: &str, size: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let mut end = (start + size).min(source.len());
        while !source.is_char_boundary(end) {
            end += 1;
        }
        out.push(&source[start..end]);
        start = end;
    }
    out
}

#[test]
fn a_streamed_answer_is_read_and_laid_out_a_block_at_a_time() {
    let answer = answer();
    let options = ParseOptions::default();
    let params = params();
    let mut engine = TextEngine::new();

    let mut whole = RichDoc::new();
    whole.update(&answer, TextFormat::Markdown, &options);
    let whole_metrics = whole.layout_simple(&mut engine, &params);
    let blocks = whole.blocks().len() as u64;

    let mut doc = RichDoc::new();
    let mut so_far = String::new();
    let pieces = pieces(&answer, 7);
    let steps = pieces.len() as u64;
    // What reading everything again at every step would have cost.
    let mut naive = 0_u64;
    let mut most_in_a_step = 0_u64;
    let mut metrics = doc.layout_simple(&mut engine, &params);
    for piece in pieces {
        so_far.push_str(piece);
        let before = doc.stats();
        doc.update(&so_far, TextFormat::Markdown, &options);
        metrics = doc.layout_simple(&mut engine, &params);
        let after = doc.stats();
        naive += doc.blocks().len() as u64;
        most_in_a_step = most_in_a_step.max(after.laid_out - before.laid_out);
        engine.end_frame();
    }

    // The streamed document is the document.
    assert_eq!(doc.blocks().len(), whole.blocks().len());
    for (i, (a, b)) in doc.blocks().iter().zip(whole.blocks()).enumerate() {
        assert!(
            a == b && a.hash() == b.hash(),
            "block {i} differs: {:?} {:?} / {:?} {:?}",
            (
                a.kind,
                &a.text,
                &a.marker,
                a.margin_top,
                a.margin_bottom,
                a.indent,
                a.tight_above
            ),
            (&a.spans, &a.table),
            (
                b.kind,
                &b.text,
                &b.marker,
                b.margin_top,
                b.margin_bottom,
                b.indent,
                b.tight_above
            ),
            (&b.spans, &b.table),
        );
    }
    assert_eq!(metrics, whole_metrics);
    let tops = |d: &RichDoc| d.blocks().iter().map(|b| b.y()).collect::<Vec<_>>();
    assert_eq!(tops(&doc), tops(&whole));

    let stats = doc.stats();
    println!(
        "{} bytes in {steps} pieces, {blocks} blocks at the end: \
         parsed {} blocks (kept {}), laid out {} (found {} still good); \
         most laid out in one step {most_in_a_step}; \
         reading everything every time: {naive} blocks parsed and laid out",
        answer.len(),
        stats.parsed,
        stats.kept,
        stats.laid_out,
        stats.reused,
    );

    // A piece changes the last block and may start a new one or two (the
    // end of a list item is also the start of the next).
    assert!(most_in_a_step <= 3, "{most_in_a_step} blocks in one step");
    // Over the whole answer: about a block a piece, not a document a piece.
    assert!(
        stats.laid_out <= steps + 2 * blocks,
        "{} layouts for {steps} pieces",
        stats.laid_out
    );
    assert!(stats.laid_out * 10 < naive);
    // Reading is by top-level block: a list or a table is read whole while
    // it grows, everything before it is never read again.
    assert!(stats.parsed * 5 < naive, "{} parsed", stats.parsed);
    assert!(stats.kept > naive / 2);
}

#[test]
fn a_piece_that_changes_nothing_lays_out_nothing() {
    let options = ParseOptions::default();
    let mut engine = TextEngine::new();
    let mut doc = RichDoc::new();
    doc.update("one\n\ntwo", TextFormat::Markdown, &options);
    doc.layout_simple(&mut engine, &params());
    let before = doc.stats().laid_out;
    // A line break at the end shows nothing new.
    assert!(doc.update("one\n\ntwo\n", TextFormat::Markdown, &options));
    doc.layout_simple(&mut engine, &params());
    assert_eq!(doc.stats().laid_out, before);
    assert!(doc.update("one\n\ntwo\n\n", TextFormat::Markdown, &options));
    doc.layout_simple(&mut engine, &params());
    assert_eq!(doc.stats().laid_out, before);
    // The next paragraph is one more block.
    doc.update("one\n\ntwo\n\nthree", TextFormat::Markdown, &options);
    doc.layout_simple(&mut engine, &params());
    assert_eq!(doc.stats().laid_out, before + 1);
}

#[test]
fn a_resize_lays_out_what_wraps_differently_and_nothing_else() {
    let options = ParseOptions::default();
    let mut engine = TextEngine::new();
    let mut doc = RichDoc::new();
    doc.update(&answer(), TextFormat::Markdown, &options);
    let mut p = params();
    doc.layout_simple(&mut engine, &p);
    let blocks = doc.blocks().len() as u64;
    assert_eq!(doc.stats().laid_out, blocks);
    // The same width again: nothing.
    doc.layout_simple(&mut engine, &p);
    assert_eq!(doc.stats().laid_out, blocks);
    // Another width: every block that wraps to it.
    p.layout.width = Some(300.0);
    doc.layout_simple(&mut engine, &p);
    assert_eq!(doc.stats().laid_out, 2 * blocks);
}
