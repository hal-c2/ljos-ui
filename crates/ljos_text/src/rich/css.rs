//! The CSS of a `style`: its declarations, and the list of families of
//! `font-family`, read and written so that every family name survives the
//! trip whatever it holds.
//!
//! What Qt 6.12 does was found by looking (qml6, offscreen): a quoted name
//! is one family, commas and semicolons in it too; unquoted words are one
//! family, each run of blanks between them one space; a family that is
//! empty, first or between two others, leaves the declaration out, while a
//! comma at the end is passed over. Escapes are read as CSS has them,
//! where Qt differs: it reads `\\B` as U+000B and keeps the blank after a
//! hexadecimal escape, so that neither a backslash nor a name with both
//! kinds of quote comes back from what it writes.

use std::fmt::Write as _;

/// The declarations of `css`: what lies between semicolons outside quotes.
pub(super) fn declarations(css: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(css);
    std::iter::from_fn(move || {
        let s = rest?;
        let mut quote = None;
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            match (quote, c) {
                _ if escaped => escaped = false,
                (_, '\\') => escaped = true,
                (None, '\'' | '"') => quote = Some(c),
                (Some(q), _) if q == c => quote = None,
                (None, ';') => {
                    rest = Some(&s[i + 1..]);
                    return Some(&s[..i]);
                }
                _ => {}
            }
        }
        rest = None;
        Some(s)
    })
}

/// The families a `font-family` value lists, in order. None when the value
/// is not a list of them: a family is empty, or a string never ends.
pub(super) fn families(value: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut name = String::new();
    let mut words = 0;
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if is_blank(c) => {}
            ',' => {
                if name.is_empty() {
                    return None;
                }
                out.push(std::mem::take(&mut name));
                words = 0;
            }
            '\'' | '"' => {
                let word = string(&mut chars, c)?;
                push_word(&mut name, &mut words, &word);
            }
            c => {
                let mut word = String::new();
                let mut next = Some(c);
                while let Some(c) = next {
                    if c == '\\' {
                        escape(&mut chars, &mut word);
                    } else {
                        word.push(c);
                    }
                    next = chars.next_if(|&c| !is_blank(c) && !matches!(c, ',' | '\'' | '"'));
                }
                push_word(&mut name, &mut words, &word);
            }
        }
    }
    if !name.is_empty() {
        out.push(name);
    } else if words > 0 {
        return None;
    }
    // A comma at the end names no family; Qt passes over it.
    (!out.is_empty()).then_some(out)
}

/// Words of one family are one name, a space between each.
fn push_word(name: &mut String, words: &mut usize, word: &str) {
    if *words > 0 {
        name.push(' ');
    }
    name.push_str(word);
    *words += 1;
}

/// The rest of a string `quote` opened. None when it never ends, or a line
/// ends in it first.
fn string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, quote: char) -> Option<String> {
    let mut out = String::new();
    loop {
        match chars.next()? {
            c if c == quote => return Some(out),
            '\n' | '\r' | '\u{c}' => return None,
            '\\' => match chars.peek()? {
                // A line break escaped in a string continues it.
                '\n' | '\u{c}' => {
                    chars.next();
                }
                '\r' => {
                    chars.next();
                    chars.next_if_eq(&'\n');
                }
                _ => escape(chars, &mut out),
            },
            c => out.push(c),
        }
    }
}

/// What follows a backslash: up to six hexadecimal digits and a blank that
/// ends them, the character they number; any other character, itself.
fn escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    let mut code = 0u32;
    let mut digits = 0;
    while digits < 6
        && let Some(d) = chars.peek().and_then(|c| c.to_digit(16))
    {
        code = code * 16 + d;
        digits += 1;
        chars.next();
    }
    if digits == 0 {
        out.push(chars.next().unwrap_or(char::REPLACEMENT_CHARACTER));
        return;
    }
    if chars.next_if_eq(&'\r').is_some() {
        chars.next_if_eq(&'\n');
    } else {
        chars.next_if(|&c| is_blank(c));
    }
    out.push(match char::from_u32(code) {
        Some(c) if code != 0 => c,
        _ => char::REPLACEMENT_CHARACTER,
    });
}

fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// `families` as a `font-family` value, before the HTML attribute is
/// escaped: each in quotes and joined by commas, as Qt writes it. A name is
/// in single quotes unless it holds one; what would end the string is
/// escaped, so every name reads back as it is.
pub(super) fn write_families<'a>(out: &mut String, families: impl IntoIterator<Item = &'a str>) {
    for (i, name) in families.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let quote = if name.contains('\'') { '"' } else { '\'' };
        out.push(quote);
        for c in name.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                c if c == quote => {
                    out.push('\\');
                    out.push(c);
                }
                // A blank ends the number, so a digit after it is the name's.
                '\n' | '\r' | '\u{c}' => {
                    let _ = write!(out, "\\{:x} ", u32::from(c));
                }
                c => out.push(c),
            }
        }
        out.push(quote);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(value: &str) -> Option<Vec<String>> {
        families(value)
    }

    fn list(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn a_quoted_name_is_one_family_and_commas_part_the_others() {
        assert_eq!(read("'A, B'"), list(&["A, B"]));
        assert_eq!(read("\"A, B\""), list(&["A, B"]));
        assert_eq!(read("A, B"), list(&["A", "B"]));
        assert_eq!(read("'A, B' , 'C'"), list(&["A, B", "C"]));
        assert_eq!(read("'A','B'"), list(&["A", "B"]));
    }

    #[test]
    fn words_of_a_family_are_one_name() {
        assert_eq!(read("  Liberation \t  Mono  "), list(&["Liberation Mono"]));
        assert_eq!(read("'A' B, C"), list(&["A B", "C"]));
        assert_eq!(read("A 'B'"), list(&["A B"]));
        assert_eq!(
            read("3D Font, -A, A-1_b"),
            list(&["3D Font", "-A", "A-1_b"])
        );
        assert_eq!(read("Fönt"), list(&["Fönt"]));
    }

    #[test]
    fn an_empty_family_or_a_string_that_never_ends_is_no_list() {
        assert_eq!(read(""), None);
        assert_eq!(read(", A"), None);
        assert_eq!(read("A,,B"), None);
        assert_eq!(read("'', A"), None);
        assert_eq!(read("'A"), None);
        assert_eq!(read("'A\\'"), None);
        assert_eq!(read("'A\nB'"), None);
        // Qt passes over a comma at the end.
        assert_eq!(read("'A',"), list(&["A"]));
    }

    #[test]
    fn escapes_are_read_as_css_has_them() {
        assert_eq!(read(r"'O\'Brien'"), list(&["O'Brien"]));
        assert_eq!(read(r"A\, B"), list(&["A, B"]));
        assert_eq!(read(r"A\ B"), list(&["A B"]));
        assert_eq!(read(r"'A\\B'"), list(&["A\\B"]));
        assert_eq!(read(r"'A\41 B'"), list(&["AAB"]));
        assert_eq!(read(r"'A\41  B'"), list(&["AA B"]));
        assert_eq!(read(r"'A\000041B'"), list(&["AAB"]));
        assert_eq!(read(r"'A\x'"), list(&["Ax"]));
        assert_eq!(read(r"'A\0 B'"), list(&["A\u{FFFD}B"]));
        assert_eq!(read(r"'A\d800 B'"), list(&["A\u{FFFD}B"]));
        assert_eq!(read("'A\\\nB'"), list(&["AB"]));
        assert_eq!(read("'A\\\r\nB'"), list(&["AB"]));
        assert_eq!(read(r"A\"), list(&["A\u{FFFD}"]));
    }

    #[test]
    fn a_semicolon_in_quotes_does_not_end_a_declaration() {
        let found: Vec<_> = declarations("font-family:'A; B'; b: \"c;\"; d: e\\;f").collect();
        assert_eq!(found, ["font-family:'A; B'", " b: \"c;\"", " d: e\\;f"]);
        assert_eq!(declarations("a;").collect::<Vec<_>>(), ["a", ""]);
    }

    #[test]
    fn every_name_written_reads_back() {
        let names = [
            "No Such Family Ljos, Liberation Mono",
            "O'Brien, X",
            "A; B",
            "A'B\"C",
            "A\\B",
            "A\\41 B",
            "A\nB\r\u{c}1",
            " A ",
        ];
        for name in names {
            let mut css = String::new();
            write_families(&mut css, [name, "Next"]);
            let read = families(&css).unwrap_or_default();
            assert_eq!(read, [name, "Next"], "{name:?} was written {css}");
        }
    }

    #[test]
    fn families_are_written_as_qt_writes_them() {
        let mut css = String::new();
        write_families(&mut css, ["No Such Family Ljos", "Liberation Mono"]);
        assert_eq!(css, "'No Such Family Ljos','Liberation Mono'");
        css.clear();
        write_families(&mut css, ["O'Brien, X"]);
        assert_eq!(css, "\"O'Brien, X\"");
    }
}
