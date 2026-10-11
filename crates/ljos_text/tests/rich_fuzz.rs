//! Rich text is untrusted: whatever the source is, reading it, laying it out
//! and asking about it must neither panic nor take long.
//!
//! Three things live here:
//!
//! * the corpus (`tests/corpus/rich/*.txt`): inputs the fuzzer broke
//!   something with, and hand-written ones of the same spirit. Each is run
//!   through every front end on every test run;
//! * property tests over generated input, a few hundred cases a run;
//! * the fuzzer itself (`fuzz_for_a_while`, ignored by default), which
//!   generates and mutates input for as long as it is told:
//!
//!   ```sh
//!   LJOS_FUZZ_SECONDS=180 LJOS_FUZZ_OUT=/some/scratch/dir \
//!       cargo test -p ljos_text --release --test rich_fuzz -- --ignored --nocapture
//!   ```
//!
//!   What it breaks is written to `LJOS_FUZZ_OUT`; move it into the corpus.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ljos_text::rich::{DocParams, HAlign, ParseOptions, RichDoc, TextFormat, View};
use ljos_text::{LayoutParams, TextEngine, TextStyle, WrapMode};
use proptest::prelude::*;

const FORMATS: [TextFormat; 4] = [
    TextFormat::Plain,
    TextFormat::Styled,
    TextFormat::Rich,
    TextFormat::Markdown,
];

const OPTIONS: ParseOptions = ParseOptions {
    paragraph_spacing: 6.0,
};

/// What every document must be, whatever it was read from.
fn check_blocks(doc: &RichDoc, source: &str) {
    assert!(!doc.blocks().is_empty(), "no block: {source:?}");
    for block in doc.blocks() {
        let text = &block.text;
        let mut end = 0;
        for span in &block.spans {
            assert!(span.range.start >= end, "spans overlap: {source:?}");
            assert!(span.range.start < span.range.end, "empty span: {source:?}");
            assert!(
                span.range.end <= text.len(),
                "span past the text: {source:?}"
            );
            assert!(
                text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end),
                "span inside a character: {source:?}"
            );
            assert!(span.style.scale.is_finite() && span.style.scale > 0.0);
            end = span.range.end;
        }
        for image in &block.images {
            assert!(image.index <= text.len(), "image past the text: {source:?}");
            assert!(
                text.is_char_boundary(image.index),
                "image inside a character: {source:?}"
            );
            for size in [image.width, image.height].into_iter().flatten() {
                assert!(size.is_finite() && size >= 0.0, "image size: {source:?}");
            }
        }
        if let Some(table) = &block.table {
            for cell in &table.cells {
                assert!(cell.column < table.columns, "cell column: {source:?}");
                assert!(
                    cell.range.start <= cell.range.end && cell.range.end <= text.len(),
                    "cell past the text: {source:?}"
                );
                assert!(
                    text.is_char_boundary(cell.range.start)
                        && text.is_char_boundary(cell.range.end),
                    "cell inside a character: {source:?}"
                );
            }
        }
        for v in [
            block.margin_top,
            block.margin_bottom,
            block.indent,
            block.indent_right,
        ] {
            assert!(v.is_finite() && v >= 0.0, "margins: {source:?}");
        }
    }
}

/// Read `source`, lay it out both ways and ask everything there is to ask.
fn exercise(engine: &mut TextEngine, format: TextFormat, source: &str) {
    let mut doc = RichDoc::new();
    doc.update(source, format, &OPTIONS);
    check_blocks(&doc, source);
    let len = doc.len();
    assert_eq!(doc.text().len(), len);
    for (width, align) in [(None, HAlign::Left), (Some(90.0), HAlign::Center)] {
        let params = DocParams {
            style: TextStyle::sized(14.0),
            layout: LayoutParams {
                width,
                wrap: WrapMode::Wrap,
                max_lines: if width.is_some() { 3 } else { 0 },
                ..LayoutParams::default()
            },
        };
        let m = doc.layout_simple(engine, &params);
        for v in [
            m.natural_width,
            m.content_width,
            m.content_height,
            m.implicit_height,
            m.baseline,
        ] {
            assert!(v.is_finite() && v >= 0.0, "metrics {m:?}: {source:?}");
        }
        assert!(m.line_count >= 1);
        let view = View {
            width: width.unwrap_or(m.content_width),
            align,
        };
        for placed in doc.lines(&view) {
            assert!(placed.x.is_finite() && placed.y.is_finite());
        }
        for rect in doc.rules(&view) {
            assert!(rect.is_finite());
        }
        for image in doc.images(&view) {
            assert!(image.rect.is_finite());
        }
        let step_x = (m.content_width / 5.0).max(7.0);
        let step_y = (m.content_height / 5.0).max(5.0);
        for i in -1..7 {
            for j in -1..7 {
                let (x, y) = (f64::from(i) * step_x, f64::from(j) * step_y);
                let _ = doc.link_at(x, y, &view);
                assert!(doc.position_at(x, y, &view) <= len);
            }
        }
        let _ = doc.position_at(f64::NAN, f64::INFINITY, &view);
        let _ = doc.link_at(f64::NEG_INFINITY, f64::NAN, &view);
        for rect in doc.selection_rects(0..len, &view) {
            assert!(rect.is_finite());
        }
        assert_eq!(doc.text_of(0..len), doc.text());
        let _ = doc.text_of(len / 3..len / 2 + 1);
        let _ = doc.text_of(len..len + 5);
        for link in doc.links(&view) {
            assert!(link.range.end <= len);
        }
    }
    // Caches age in frames; without them the engine would keep it all.
    engine.end_frame();
}

/// Markdown that arrives in pieces is what Markdown read at once is: the
/// same blocks at every step. `step` is the size of a piece in bytes.
fn check_streamed(source: &str, step: usize) {
    let mut streamed = RichDoc::new();
    let mut end = 0;
    while end < source.len() {
        end = (end + step.max(1)).min(source.len());
        while !source.is_char_boundary(end) {
            end += 1;
        }
        let so_far = &source[..end];
        streamed.update(so_far, TextFormat::Markdown, &OPTIONS);
        let mut whole = RichDoc::new();
        whole.update(so_far, TextFormat::Markdown, &OPTIONS);
        let same =
            streamed.blocks().len() == whole.blocks().len()
                && streamed.blocks().iter().zip(whole.blocks()).all(|(a, b)| {
                    a == b && a.hash() == b.hash() && a.source_start == b.source_start
                });
        assert!(same, "in steps of {step}, after {end} bytes: {source:?}");
    }
}

/// Pieces rich text is made of, well formed and not.
const PIECES: &[&str] = &[
    // Styles, whole and in parts.
    "<span style=\"color: red; background-color: #f00\">",
    "<span style=\"text-decoration: underline overline line-through\">",
    "<a href=\"x\" style=\"text-decoration: none\">",
    "<p style='background: rgb(0, 255, 0)'>",
    "</span>",
    "<span style=\"",
    " style='",
    "\">",
    "'>",
    "color: red;",
    "background-color:",
    "text-decoration: none, underline;",
    "text-decoration-color: rgba(0,0,0,0.5)",
    "url(http://x/y.png)",
    ";",
    ":",
    "/*",
    "*/",
    "!important",
    // Tags of the styled and the rich subset.
    "<b>",
    "</b>",
    "<i>",
    "</i>",
    "<u>",
    "<s>",
    "<p>",
    "</p>",
    "<p align=\"center\">",
    "<br>",
    "<br/>",
    "<a href=\"http://x/\">",
    "<a href='",
    "<a href=x>",
    "</a>",
    "<ul>",
    "</ul>",
    "<ol type=\"a\">",
    "<ol type=\"I\">",
    "</ol>",
    "<li>",
    "</li>",
    "<table>",
    "<table border=\"1\" cellpadding=\"4\">",
    "<tr>",
    "<td>",
    "<th>",
    "</td>",
    "</th>",
    "</tr>",
    "</table>",
    "<img src=\"a.png\" width=\"10\" height=\"12\">",
    "<img src=\"file:///etc/passwd\">",
    "<img width=",
    "<img align=\"top\" height=\"",
    "<font size=\"7\" color=\"red\">",
    "<font size=\"+",
    "<font size=\"-",
    "</font>",
    "<h1>",
    "</h1>",
    "<h3>",
    "<h6>",
    "</h6>",
    "<pre>",
    "</pre>",
    "<!--",
    "-->",
    "<!",
    "<script>",
    "</script>",
    "<style>",
    "</style>",
    "<title>",
    "<div align=right>",
    "</div>",
    "<center>",
    "<blockquote>",
    "</blockquote>",
    "<hr>",
    "<dl>",
    "<dt>",
    "<dd>",
    "</dl>",
    "<sub>",
    "<sup>",
    "<code>",
    "</code>",
    "<big>",
    "<small>",
    "<em>",
    "<strong>",
    "<span style=\"color:red\">",
    "<body>",
    "<html>",
    "</ b>",
    "< b>",
    "<b/>",
    "<b x>",
    "</b x>",
    "<>",
    "</>",
    "<",
    ">",
    "\"",
    "'",
    "=",
    "/",
    // Character references.
    "&amp;",
    "&lt;",
    "&gt;",
    "&quot;",
    "&nbsp;",
    "&#65;",
    "&#x41;",
    "&#x110000;",
    "&#xD800;",
    "&#0;",
    "&#99999999999;",
    "&#x;",
    "&#;",
    "&bogus;",
    "&amp",
    "&",
    ";",
    // Numbers attributes may hold.
    "0",
    "1",
    "7",
    "8",
    "-1",
    "2147483647",
    "-2147483648",
    "4294967296",
    "1e39",
    "NaN",
    "inf",
    "-0",
    "16385",
    "0.5",
    // Markdown.
    "# ",
    "## ",
    "###### ",
    "\n\n",
    "\n",
    "- ",
    "* ",
    "+ ",
    "1. ",
    "2) ",
    "999999999. ",
    "9999999999999999999. ",
    "> ",
    "```\n",
    "```rust\n",
    "~~~",
    "    ",
    "**",
    "*",
    "_",
    "__",
    "~~",
    "`",
    "``",
    "[",
    "]",
    "](",
    ")",
    "![",
    "[x]",
    "[x]: http://y/\n",
    "[x]: <",
    "|",
    "---|---",
    "| a | b |\n|---|:-:|\n",
    "|-|\n",
    "- [ ] ",
    "- [x] ",
    "***\n",
    "===\n",
    "---\n",
    "\\",
    "<div>",
    "<!-- ",
    "&copy;",
    "\t",
    "  \n",
    "https://example.com",
    "<http://x>",
    "[a](b \"c\")",
    "![alt](img.png)",
    "[![a](b)](c)",
    ":--",
    "\r\n",
    "\r",
    // Text.
    "abc",
    "abc def",
    " ",
    "  ",
    "x",
    "\u{e9}",
    "\u{1F600}",
    "\u{200d}",
    "\u{202e}",
    "\u{5e9}\u{5dc}\u{5d5}\u{5dd}",
    "e\u{301}",
    "\0",
    "\u{feff}",
    "\u{a0}",
    "\u{2028}",
    "a\u{300}\u{301}\u{302}\u{303}",
    "\u{fffd}",
    "\u{10ffff}",
];

fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => proptest::sample::select(PIECES).prop_map(str::to_owned),
        1 => any::<char>().prop_map(String::from),
        1 => "[ -~]{0,6}",
    ]
}

fn source() -> impl Strategy<Value = String> {
    proptest::collection::vec(piece(), 0..48).prop_map(|pieces| pieces.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]

    #[test]
    fn no_source_breaks_a_front_end(source in source()) {
        let mut engine = TextEngine::new();
        for format in FORMATS {
            exercise(&mut engine, format, &source);
        }
    }

    #[test]
    fn markdown_read_in_pieces_is_markdown_read_at_once(
        source in source(),
        step in 1_usize..9,
    ) {
        check_streamed(&source, step);
    }

    #[test]
    fn auto_text_decides_about_anything(source in source()) {
        let _ = TextFormat::detect(&source);
    }
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/rich")
}

fn corpus() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(corpus_dir()).expect("the corpus directory");
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        let bytes = std::fs::read(&path).expect("a corpus file");
        // A crash file is whatever the fuzzer had; text is all a front end
        // ever sees.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        out.push((path.display().to_string(), text));
    }
    out.sort();
    out
}

#[test]
fn the_corpus_breaks_nothing() {
    let corpus = corpus();
    assert!(corpus.len() >= 10, "the corpus went missing");
    let mut engine = TextEngine::new();
    for (name, source) in &corpus {
        for format in FORMATS {
            let ok = catch_unwind(AssertUnwindSafe(|| exercise(&mut engine, format, source)));
            assert!(ok.is_ok(), "{name} as {format:?}");
        }
        let _ = TextFormat::detect(source);
        for step in [1, 2, 5] {
            check_streamed(source, step);
        }
    }
}

/// Input that grows must not cost its square: a hostile message of a few
/// hundred kilobytes may not freeze the interface.
#[test]
fn hostile_repetition_is_read_in_time() {
    let unit = [
        "&",
        "<",
        "&#",
        "<b>",
        "</b>",
        "<b",
        "<a href=\"",
        "<!--",
        "<!",
        "&amp",
        "<ul><li>",
        "<table><tr><td>",
        "<blockquote>",
        "> ",
        "- ",
        "* ",
        "[",
        "![",
        "](",
        "`",
        "*",
        "_",
        "**a",
        "[a](",
        "<div>",
        "|",
        "| a ",
        "\n",
        "\n\n",
        "1. ",
        "```",
        "~",
        "\\",
        "<font size=\"7\">",
        "<img src=\"",
        "a ",
        "<x>",
        "<p>",
        "</p>",
        "<li>",
        "[x]: y\n",
        "<<",
        ">>",
        "&lt;",
    ];
    for piece in unit {
        let source = piece.repeat(200_000 / piece.len());
        for format in FORMATS {
            let started = Instant::now();
            let mut doc = RichDoc::new();
            doc.update(&source, format, &OPTIONS);
            check_blocks(&doc, piece);
            let took = started.elapsed();
            // Generous: an unoptimised build on a busy machine reads this in
            // well under a second, a quadratic reader needs minutes.
            assert!(
                took < Duration::from_secs(20),
                "{piece:?} as {format:?} took {took:?}"
            );
        }
    }
}

/// A small generator of its own, so a run is its seed and nothing else.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn generate(&mut self) -> String {
        let mut out = String::new();
        for _ in 0..self.below(40) {
            match self.below(12) {
                0 => {
                    if let Some(c) = char::from_u32((self.next() % 0x11_0000) as u32) {
                        out.push(c);
                    }
                }
                1 => out.push((b' ' + self.below(95) as u8) as char),
                _ => out.push_str(PIECES[self.below(PIECES.len())]),
            }
        }
        out
    }

    /// Change `source` a little: what a generator alone seldom reaches is
    /// often one step from something that was already interesting.
    fn mutate(&mut self, source: &str) -> String {
        let at = |rng: &mut Self| {
            let mut i = rng.below(source.len() + 1);
            while !source.is_char_boundary(i) {
                i -= 1;
            }
            i
        };
        let (a, b) = (at(self), at(self));
        let (a, b) = (a.min(b), a.max(b));
        match self.below(5) {
            0 => format!("{}{}", &source[..a], &source[b..]),
            1 => format!("{}{}{}", &source[..b], &source[a..b], &source[a..]),
            2 => format!(
                "{}{}{}",
                &source[..a],
                PIECES[self.below(PIECES.len())],
                &source[a..]
            ),
            3 => source[..a].to_owned(),
            _ => format!("{}{}", &source[b..], &source[..b]),
        }
    }
}

/// What a fuzz run wrote out, tried again: after a fix, to see that every
/// input it found is answered.
#[test]
#[ignore = "replays the directory LJOS_FUZZ_REPLAY names"]
fn replay_what_a_fuzz_run_found() {
    let Some(dir) = std::env::var_os("LJOS_FUZZ_REPLAY").map(PathBuf::from) else {
        return;
    };
    std::panic::set_hook(Box::new(|_| {}));
    let mut engine = TextEngine::new();
    let (mut tried, mut failed) = (0_usize, Vec::new());
    for entry in std::fs::read_dir(&dir).expect("the directory to replay") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|e| e != "txt") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("a fuzz file");
        let source = String::from_utf8_lossy(&bytes).into_owned();
        tried += 1;
        let ok = catch_unwind(AssertUnwindSafe(|| {
            for format in FORMATS {
                exercise(&mut engine, format, &source);
            }
            for step in 1..=4 {
                check_streamed(&source, step);
            }
        }));
        if ok.is_err() {
            engine = TextEngine::new();
            failed.push(path.display().to_string());
        }
    }
    let _ = std::panic::take_hook();
    println!("{tried} inputs tried again, {} still fail", failed.len());
    assert!(
        failed.is_empty(),
        "still failing: {:#?}",
        &failed[..failed.len().min(20)]
    );
}

/// See the top of the file. Ignored: it runs for as long as it is told.
#[test]
#[ignore = "the fuzzer: run it on purpose, with LJOS_FUZZ_SECONDS"]
fn fuzz_for_a_while() {
    let seconds: u64 = std::env::var("LJOS_FUZZ_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let seed: u64 = std::env::var("LJOS_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let out = std::env::var_os("LJOS_FUZZ_OUT").map(PathBuf::from);
    // Panics are counted, not printed a thousand times.
    std::panic::set_hook(Box::new(|_| {}));
    let seeds: Vec<String> = corpus().into_iter().map(|(_, text)| text).collect();
    let mut failed = 0_usize;
    for format in FORMATS {
        let mut rng = Rng(seed | 1);
        let mut engine = TextEngine::new();
        let mut pool = seeds.clone();
        let (mut runs, mut crashes) = (0_u64, 0_usize);
        let mut slowest = (Duration::ZERO, String::new());
        let until = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < until {
            let source = if !pool.is_empty() && rng.below(3) > 0 {
                let base = pool[rng.below(pool.len())].clone();
                rng.mutate(&base)
            } else {
                rng.generate()
            };
            if source.len() > 4096 {
                continue;
            }
            let started = Instant::now();
            let ok = catch_unwind(AssertUnwindSafe(|| {
                exercise(&mut engine, format, &source);
                if format == TextFormat::Markdown && source.len() <= 512 {
                    check_streamed(&source, 1 + source.len() % 4);
                }
            }));
            let took = started.elapsed();
            runs += 1;
            if took > slowest.0 {
                slowest = (took, source.clone());
            }
            if ok.is_err() {
                crashes += 1;
                // A broken engine is not trusted with the next input.
                engine = TextEngine::new();
                if let Some(dir) = &out {
                    let name = format!("crash-{format:?}-{runs}.txt").to_lowercase();
                    let _ = std::fs::write(dir.join(name), &source);
                }
            } else if pool.len() < 4096 && rng.below(64) == 0 {
                pool.push(source);
            }
        }
        if let Some(dir) = &out {
            let name = format!("slowest-{format:?}.txt").to_lowercase();
            let _ = std::fs::write(dir.join(name), &slowest.1);
        }
        println!(
            "{format:?}: {runs} inputs in {seconds} s, {crashes} failures, slowest {:?} ({} bytes)",
            slowest.0,
            slowest.1.len()
        );
        failed += crashes;
    }
    let _ = std::panic::take_hook();
    assert_eq!(failed, 0, "inputs that broke something were written out");
}
