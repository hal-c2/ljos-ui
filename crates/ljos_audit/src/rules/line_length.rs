//! `text-line-length`: a paragraph's lines run past
//! `limits::LINE_LENGTH_MAX_CHARS` characters, so the eye loses its place
//! going back to the start of the next one (WCAG 2.2 SC 1.4.8).
//!
//! A paragraph is wrapped text of `limits::LINE_LENGTH_MIN_LINES` lines
//! or more; its characters a line are its characters over its lines, so
//! a short last line counts against it, not for it.

use ljos_design::limits::{LINE_LENGTH_MAX_CHARS, LINE_LENGTH_MIN_LINES};

use super::controls::shown_texts;
use crate::cx::Cx;

/// `Text.NoWrap`, as Qt documents the enumeration.
const NO_WRAP: i64 = 0;

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    for (item, text) in shown_texts(cx) {
        let (wrap, lines) = e.untrack(|| {
            (
                e.read_by_name(item, "wrapMode").to_number() as i64,
                e.read_by_name(item, "lineCount").to_number(),
            )
        });
        if wrap == NO_WRAP || !lines.is_finite() || lines < LINE_LENGTH_MIN_LINES as f64 {
            continue;
        }
        let chars = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .count();
        let per_line = (chars as f64 / lines).round() as usize;
        if per_line <= LINE_LENGTH_MAX_CHARS {
            continue;
        }
        cx.report(
            "text-line-length",
            Some(item),
            format!(
                "the paragraph's lines hold about {per_line} characters, more than the \
                 {LINE_LENGTH_MAX_CHARS} one reads without losing the line; bound its width \
                 (a maximum width, a column, margins) to 45 to 75 characters"
            ),
        )
        .measured = Some(format!("{per_line} characters a line over {lines} lines"));
    }
}
