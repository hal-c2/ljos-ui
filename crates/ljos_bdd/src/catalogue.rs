//! The step catalogue: every step a scenario may use, read from the steps
//! themselves.
//!
//! A step is known to the runner by the attribute on its function in
//! `steps.rs`, so that file is the one place that cannot be out of date.
//! This reads the attributes out of its text and writes each pattern the
//! way a step is written in a `.feature` file. Tools that help people and
//! agents write scenarios show the result; nobody keeps it by hand.

use std::fmt::Write as _;

/// `Given`, `When` or `Then`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Keyword {
    Given,
    When,
    Then,
}

impl Keyword {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Given => "Given",
            Self::When => "When",
            Self::Then => "Then",
        }
    }
}

/// One step the runner knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub keyword: Keyword,
    /// The step as it is written in a scenario, with a placeholder where a
    /// value goes: `"…"` for a quoted text, `N` for a number, `<value>` for
    /// a value written as JSON, `<word>` for one word, `{a|b}` for a choice
    /// and `[s]` for letters that may be left out.
    pub text: String,
    /// The step ends in a colon and takes a doc string or a table.
    pub block: bool,
    /// The pattern as the runner matches it, when it is a regular
    /// expression: what `text` was written from.
    pub pattern: Option<String>,
}

/// The steps of `steps.rs`, in the order they are defined.
#[must_use]
pub fn steps() -> Vec<Step> {
    parse(include_str!("steps.rs"))
}

fn parse(source: &str) -> Vec<Step> {
    let mut out = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.trim().strip_prefix("#[") else {
            continue;
        };
        let Some((keyword, rest)) = [
            ("given(", Keyword::Given),
            ("when(", Keyword::When),
            ("then(", Keyword::Then),
        ]
        .into_iter()
        .find_map(|(prefix, keyword)| Some((keyword, rest.strip_prefix(prefix)?))) else {
            continue;
        };
        let Some(rest) = rest.strip_suffix(")]") else {
            continue;
        };
        let (literal, pattern) = match rest.strip_prefix("regex") {
            Some(rest) => (rest.trim_start().strip_prefix('=').map(str::trim), true),
            None => (Some(rest), false),
        };
        let Some(literal) = literal.and_then(string) else {
            continue;
        };
        let text = if pattern {
            written(&literal).unwrap_or_else(|| literal.clone())
        } else {
            literal.clone()
        };
        out.push(Step {
            keyword,
            block: text.ends_with(':'),
            text,
            pattern: pattern.then_some(literal),
        });
    }
    out
}

/// The value of a Rust string literal: `"…"`, `r"…"` or `r#"…"#`.
fn string(literal: &str) -> Option<String> {
    if let Some(raw) = literal.strip_prefix('r') {
        let hashes = raw.bytes().take_while(|b| *b == b'#').count();
        let close = format!("\"{}", "#".repeat(hashes));
        let inner = raw[hashes..]
            .strip_prefix('"')?
            .strip_suffix(close.as_str())?;
        return Some(inner.to_owned());
    }
    let inner = literal.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Where the group that opens at `open` (a `(`) closes.
fn closing(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut class = false;
    let mut at = open;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 1,
            '[' if !class => class = true,
            ']' if class => class = false,
            '(' if !class => depth += 1,
            ')' if !class => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// Whether a group's inside is words to choose from and nothing else.
fn choice(inside: &str) -> bool {
    !inside.is_empty()
        && inside
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '|' | '\''))
}

/// What stands for a captured value in the step as it is written.
fn placeholder(inside: &str, quoted: bool) -> String {
    if quoted {
        return if inside.starts_with('#') {
            "#rrggbb".to_owned()
        } else if inside.starts_with('/') {
            "/pointer".to_owned()
        } else if let Some((_, extension)) = inside.rsplit_once("\\.") {
            format!("….{extension}")
        } else {
            "…".to_owned()
        };
    }
    if choice(inside) {
        format!("{{{inside}}}")
    } else if inside.contains(".*") || inside.contains(".+") {
        "<value>".to_owned()
    } else if inside.contains("\\d") {
        "N".to_owned()
    } else if inside.contains("\\w") {
        "<word>".to_owned()
    } else {
        "<value>".to_owned()
    }
}

/// A step's regular expression as the step is written in a scenario, or
/// `None` if it uses something this does not know how to write.
fn written(pattern: &str) -> Option<String> {
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let pattern = pattern.strip_suffix('$').unwrap_or(pattern);
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let optional = |end: usize| chars.get(end + 1) == Some(&'?');
        match chars[at] {
            '(' => {
                let close = closing(&chars, at)?;
                let inside: String = chars[at + 1..close].iter().collect();
                let quoted = at > 0 && chars[at - 1] == '"' && chars.get(close + 1) == Some(&'"');
                if let Some(words) = inside.strip_prefix("?:") {
                    // A group that is not a value: letters of the sentence.
                    if !choice(words) {
                        return None;
                    }
                    if optional(close) {
                        write!(out, "[{words}]").ok()?;
                    } else {
                        write!(out, "{{{words}}}").ok()?;
                    }
                } else if optional(close) && choice(&inside) {
                    write!(out, "[{inside}]").ok()?;
                } else if optional(close) {
                    return None;
                } else {
                    out.push_str(&placeholder(&inside, quoted));
                }
                at = close + usize::from(optional(close));
            }
            '\\' => {
                let next = *chars.get(at + 1)?;
                if next.is_alphanumeric() {
                    return None;
                }
                out.push(next);
                at += 1;
            }
            c if c.is_alphanumeric() && optional(at) => {
                write!(out, "[{c}]").ok()?;
                at += 1;
            }
            '[' | ']' | '*' | '+' | '?' | '{' | '}' | '|' | '^' | '$' => return None,
            c => out.push(c),
        }
        at += 1;
    }
    Some(out)
}

/// The catalogue as a page: the steps under `Given`, `When` and `Then`,
/// each as it is written in a scenario.
#[must_use]
pub fn markdown() -> String {
    let steps = steps();
    let mut out = String::from(
        "# Steps\n\n\
         Every step the runner knows, read from the steps themselves. A scenario may use no \
         other; a behaviour that needs a new one gets it in `crates/ljos_bdd/src/steps.rs`.\n\n\
         A placeholder stands where a value goes: `\"…\"` is a text in quotes, `N` a number, \
         `<value>` a value written as JSON (`12`, `\"red\"`, `true`), `<word>` one word, \
         `{a|b}` one of the words, and `[s]` letters that may be left out. A step that ends in a \
         colon takes a doc string or a table on the lines below it. `And` and `But` repeat the \
         keyword before them.\n",
    );
    for keyword in [Keyword::Given, Keyword::When, Keyword::Then] {
        let _ = write!(out, "\n## {}\n\n", keyword.as_str());
        let mut own: Vec<&Step> = steps.iter().filter(|s| s.keyword == keyword).collect();
        own.sort_by_cached_key(|a| a.text.to_lowercase());
        own.dedup_by(|a, b| a.text == b.text);
        for step in own {
            let _ = writeln!(out, "- {} {}", keyword.as_str(), step.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(pattern: &str) -> String {
        written(pattern).unwrap_or_else(|| panic!("`{pattern}` is not written"))
    }

    #[test]
    fn a_pattern_is_written_as_the_step_is() {
        assert_eq!(
            one(r#"^the property "([^"]+)" of "([^"]+)" is set to (.+)$"#),
            r#"the property "…" of "…" is set to <value>"#
        );
        assert_eq!(
            one(r"^the snapshot lists (\d+) nodes?$"),
            "the snapshot lists N node[s]"
        );
        assert_eq!(
            one(r"^(\d+(?:\.\d+)?) seconds? pass(?:es)?$"),
            "N second[s] pass[es]"
        );
        assert_eq!(
            one(r"^the user turns the wheel ([\d.]+) notch(?:es)? (\w+) at ([-\d.]+), ([-\d.]+)$"),
            "the user turns the wheel N notch[es] <word> at N, N"
        );
        assert_eq!(
            one(r"^the window (loses|gains) focus$"),
            "the window {loses|gains} focus"
        );
        assert_eq!(
            one(r#"^the search finds an? "([^"]+)" named "([^"]*)"$"#),
            r#"the search finds a[n] "…" named "…""#
        );
        assert_eq!(
            one(r#"^the file "([^"]+\.qml)" is changed to:$"#),
            r#"the file "….qml" is changed to:"#
        );
        assert_eq!(
            one(r#"^the pixel at (\d+), (\d+) of the window is "(#\w{6})"$"#),
            r##"the pixel at N, N of the window is "#rrggbb""##
        );
        assert_eq!(
            one(r#"^the answer's "(/[^"]*)" is ([^a].*)$"#),
            r#"the answer's "/pointer" is <value>"#
        );
        assert_eq!(
            one(r#"^the "([^"]+)" named "([^"]*)" is (not )?(checked|pressed)$"#),
            r#"the "…" named "…" is [not ]{checked|pressed}"#
        );
        assert_eq!(
            one(r"^(\d+) reloads? ha(?:s|ve) happened$"),
            "N reload[s] ha{s|ve} happened"
        );
    }

    #[test]
    fn what_is_not_understood_is_left_as_it_is() {
        assert_eq!(written(r"^a+ b$"), None);
        let found = parse("#[then(regex = r\"^a+ b$\")]\nfn f() {}\n");
        assert_eq!(found[0].text, "^a+ b$");
    }

    #[test]
    fn attributes_are_read_in_each_of_their_forms() {
        let found = parse(
            "#[given(\"the QML:\")]\nfn a() {}\n    #[when(regex = r#\"^\"([^\"]+)\" is loaded$\"#)]\n\
             #[then(regex = r\"^(\\d+) pass$\")]\n#[derive(Debug)]\n",
        );
        let texts: Vec<(Keyword, &str, bool)> = found
            .iter()
            .map(|s| (s.keyword, s.text.as_str(), s.block))
            .collect();
        assert_eq!(
            texts,
            [
                (Keyword::Given, "the QML:", true),
                (Keyword::When, "\"…\" is loaded", false),
                (Keyword::Then, "N pass", false),
            ]
        );
    }

    /// Every step of the real file is read, and every pattern in it can be
    /// written as a step: a pattern that cannot would show up in the
    /// catalogue as a regular expression.
    #[test]
    fn every_step_of_the_runner_is_in_the_catalogue() {
        let source = include_str!("steps.rs");
        let attributes = source
            .lines()
            .map(str::trim)
            .filter(|l| {
                ["#[given(", "#[when(", "#[then("]
                    .iter()
                    .any(|p| l.starts_with(p))
            })
            .count();
        let steps = steps();
        assert_eq!(steps.len(), attributes);
        assert!(steps.len() > 100);
        let raw: Vec<&str> = steps
            .iter()
            .filter(|s| s.pattern.as_deref() == Some(s.text.as_str()))
            .map(|s| s.text.as_str())
            .collect();
        assert!(raw.is_empty(), "not written as steps: {raw:#?}");
    }

    #[test]
    fn the_page_lists_the_steps_under_their_keywords() {
        let page = markdown();
        assert!(page.contains("\n## Given\n"));
        assert!(page.contains("\n- Given the QML:\n"));
        assert!(page.contains("is set to <value>\n"));
        assert!(page.contains("\n- Then the snapshot lists N node[s]\n"));
    }
}
