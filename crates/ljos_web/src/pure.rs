//! What has no browser in it: colours, font strings, line breaking.

/// A CSS colour of a premultiplied one, which is what the display list holds
/// and a canvas does not take.
pub fn css_color(r: f32, g: f32, b: f32, a: f32) -> String {
    let straight = |c: f32| {
        if a > 0.0 {
            ((c / a).clamp(0.0, 1.0) * 255.0).round() as u8
        } else {
            0
        }
    };
    format!(
        "rgba({},{},{},{})",
        straight(r),
        straight(g),
        straight(b),
        a.clamp(0.0, 1.0)
    )
}

/// A CSS `font` value. The family is one name whatever it holds, as in
/// Qt, so it is written as one CSS string with its backslashes, quotes and
/// line breaks escaped, not dropped: a raw line break (LF, CR or FF) ends a
/// CSS string, and the browser then refuses the whole value.
pub fn css_font(family: &str, pixel_size: f32, weight: f32, italic: bool) -> String {
    let style = if italic { "italic " } else { "" };
    let family = if family.is_empty() {
        "sans-serif".to_owned()
    } else {
        format!(
            "\"{}\"",
            family
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\a ")
                .replace('\r', "\\d ")
                .replace('\u{c}', "\\c ")
        )
    };
    format!("{style}{} {pixel_size}px {family}", weight.round() as u32)
}

/// What a web glyph run carries in the place of a font: a run has no glyph
/// ids here, only what `fillText` needs.
pub fn encode_run(font: &str, text: &str) -> Vec<u8> {
    format!("{font}\0{text}").into_bytes()
}

/// The other way round.
pub fn decode_run(bytes: &[u8]) -> Option<(&str, &str)> {
    std::str::from_utf8(bytes).ok()?.split_once('\0')
}

/// `text` broken into lines no wider than `width`, at spaces, a word wider
/// than that alone on its line. A newline always breaks. Ranges are bytes.
pub fn break_lines(
    text: &str,
    width: Option<f64>,
    measure: &mut dyn FnMut(&str) -> f64,
) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for paragraph in text.split_inclusive('\n') {
        let end = start + paragraph.trim_end_matches('\n').len();
        let Some(width) = width else {
            lines.push(start..end);
            start += paragraph.len();
            continue;
        };
        let mut from = start;
        let mut fits = None;
        let mut at = start;
        while at <= end {
            let next = text[at..end].find(' ').map_or(end, |i| at + i);
            if measure(&text[from..next]) > width
                && let Some(fit) = fits
            {
                lines.push(from..fit);
                from = fit + 1;
                fits = None;
                continue;
            }
            fits = Some(next);
            if next == end {
                break;
            }
            at = next + 1;
        }
        lines.push(from..end);
        start += paragraph.len();
    }
    if lines.is_empty() {
        lines.push(0..0);
    }
    lines
}

/// Whether an operation that this path cannot draw has been said once.
#[derive(Debug, Default)]
pub struct Warned(Vec<&'static str>);

impl Warned {
    /// True the first time `name` is asked about.
    pub fn first(&mut self, name: &'static str) -> bool {
        if self.0.contains(&name) {
            return false;
        }
        self.0.push(name);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, width: Option<f64>) -> Vec<&str> {
        let mut measure = |s: &str| s.len() as f64;
        break_lines(text, width, &mut measure)
            .into_iter()
            .map(|r| &text[r])
            .collect()
    }

    #[test]
    fn premultiplied_colours_are_undone() {
        assert_eq!(css_color(0.0, 0.0, 0.0, 1.0), "rgba(0,0,0,1)");
        assert_eq!(css_color(0.5, 0.0, 0.0, 0.5), "rgba(255,0,0,0.5)");
        assert_eq!(css_color(0.0, 0.0, 0.0, 0.0), "rgba(0,0,0,0)");
    }

    #[test]
    fn a_font_is_written_as_css_wants_it() {
        assert_eq!(css_font("", 12.0, 400.0, false), "400 12px sans-serif");
        assert_eq!(
            css_font("A \"B\"", 9.5, 700.0, true),
            r#"italic 700 9.5px "A \"B\"""#
        );
        assert_eq!(
            css_font("A, B\\\nC", 12.0, 400.0, false),
            r#"400 12px "A, B\\\a C""#
        );
        assert_eq!(
            css_font("A\r\nB\u{c}C", 12.0, 400.0, false),
            r#"400 12px "A\d \a B\c C""#
        );
    }

    #[test]
    fn a_run_comes_back_as_it_went() {
        let bytes = encode_run("400 12px x", "héllo");
        assert_eq!(decode_run(&bytes), Some(("400 12px x", "héllo")));
    }

    #[test]
    fn lines_break_at_spaces_and_newlines() {
        assert_eq!(lines("one two three", Some(7.0)), ["one two", "three"]);
        assert_eq!(lines("one\ntwo", None), ["one", "two"]);
        assert_eq!(lines("", Some(3.0)), [""]);
    }

    #[test]
    fn a_word_too_wide_keeps_its_line() {
        assert_eq!(lines("a enormous b", Some(4.0)), ["a", "enormous", "b"]);
    }

    #[test]
    fn a_warning_is_said_once() {
        let mut warned = Warned::default();
        assert!(warned.first("Shader"));
        assert!(!warned.first("Shader"));
    }
}
