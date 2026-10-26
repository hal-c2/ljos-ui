//! The rules of a `<style>` element of rich text.
//!
//! A selector is an element (`p`), classes (`.name`, `.a.b`), an element
//! with classes (`p.name`) or `*`, and a rule may list several with commas.
//! A selector with anything else in it (a descendant, a child, an id, an
//! attribute, a pseudo-class) matches nothing, and an `@` rule is left out
//! whole. The declarations of a rule are kept as written: they are read
//! as a `style` attribute is, by whoever applies them.
//!
//! Which rule wins is as in CSS, as Qt 6.11 does
//! (features/text/rich-text-stylesheet.feature): the more classes a
//! selector names the later it applies, then naming the element, then
//! being written later. All of them apply before the element's own `style`.
//!
//! A sheet is bounded whatever it is given: at most [`MAX_RULES`] selectors
//! are kept, and a selector longer than [`MAX_SELECTOR`] is not read.

use std::sync::Arc;

/// How many selectors a document's sheets keep, all of them together.
pub(crate) const MAX_RULES: usize = 256;

/// The longest selector that is read, in bytes.
pub(crate) const MAX_SELECTOR: usize = 128;

/// How many classes a selector may name.
const MAX_CLASSES: usize = 8;

struct Rule {
    element: Option<Box<str>>,
    classes: Vec<Box<str>>,
    /// (classes, element): what is more specific applies later.
    specificity: (usize, usize),
    declarations: Arc<str>,
}

impl Rule {
    fn matches(&self, element: &str, classes: &str) -> bool {
        self.element.as_deref().is_none_or(|e| e == element)
            && self
                .classes
                .iter()
                .all(|c| classes.split_ascii_whitespace().any(|has| has == &**c))
    }
}

/// The rules of the `<style>` elements read so far, in the order written.
#[derive(Default)]
pub(crate) struct Sheet {
    rules: Vec<Rule>,
}

impl Sheet {
    /// Add the rules of one `<style>` element.
    pub(crate) fn add(&mut self, css: &str) {
        let css = super::html::without_comments(css);
        let mut rest = &*css;
        while self.rules.len() < MAX_RULES {
            let Some(open) = rest.find(['{', '}']) else {
                return;
            };
            let prelude = rest[..open].trim();
            if rest.as_bytes()[open] == b'}' {
                // A brace that closes nothing.
                rest = &rest[open + 1..];
                continue;
            }
            let body_start = open + 1;
            let Some(len) = block_len(&rest[body_start..]) else {
                // A rule that never ends is not read.
                return;
            };
            let body = &rest[body_start..body_start + len];
            rest = &rest[(body_start + len + 1).min(rest.len())..];
            if prelude.starts_with('@') || body.contains('{') {
                continue;
            }
            let declarations: Arc<str> = Arc::from(body.trim());
            for selector in prelude.split(',') {
                if self.rules.len() >= MAX_RULES {
                    return;
                }
                if let Some(rule) = selector_of(selector.trim(), &declarations) {
                    self.rules.push(rule);
                }
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The declarations of each rule an element matches, the one to apply
    /// first first. `classes` is its `class` attribute.
    pub(crate) fn matching(&self, element: &str, classes: &str) -> Vec<Arc<str>> {
        let mut found: Vec<(&Rule, usize)> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.matches(element, classes))
            .map(|(i, r)| (r, i))
            .collect();
        found.sort_by_key(|(r, i)| (r.specificity, *i));
        found
            .into_iter()
            .map(|(r, _)| r.declarations.clone())
            .collect()
    }
}

/// How long the block after a `{` is up to its `}`, counting braces in it.
fn block_len(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(i),
            b'}' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn selector_of(selector: &str, declarations: &Arc<str>) -> Option<Rule> {
    if selector.is_empty() || selector.len() > MAX_SELECTOR {
        return None;
    }
    let mut parts = selector.split('.');
    let head = parts.next()?;
    let element = match head {
        "" | "*" => None,
        name if is_ident(name) => Some(name.to_ascii_lowercase().into_boxed_str()),
        _ => return None,
    };
    let mut classes = Vec::new();
    for class in parts {
        if !is_ident(class) || classes.len() == MAX_CLASSES {
            return None;
        }
        classes.push(Box::from(class));
    }
    if element.is_none() && classes.is_empty() && head.is_empty() {
        return None;
    }
    Some(Rule {
        specificity: (classes.len(), usize::from(element.is_some())),
        element,
        classes,
        declarations: declarations.clone(),
    })
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matching(css: &str, element: &str, classes: &str) -> Vec<String> {
        let mut s = Sheet::default();
        s.add(css);
        s.matching(element, classes)
            .iter()
            .map(|d| d.to_string())
            .collect()
    }

    #[test]
    fn selectors_name_an_element_classes_or_both() {
        let css = "p{a:1} .big{b:2} p.big{c:3} li,p{d:4} *{e:5} .big.red{f:6}";
        assert_eq!(matching(css, "p", ""), ["e:5", "a:1", "d:4"]);
        assert_eq!(
            matching(css, "p", "big"),
            ["e:5", "a:1", "d:4", "b:2", "c:3"]
        );
        assert_eq!(matching(css, "li", "red big"), ["e:5", "d:4", "b:2", "f:6"]);
        assert_eq!(matching(css, "span", "Big"), ["e:5"]);
    }

    #[test]
    fn what_is_not_understood_matches_nothing() {
        let css = "p a{a:1} p>a{b:2} #x{c:3} a:hover{d:4} a[href]{e:5} @media print{p{f:6}} /* p{g:7} */ p{h:8}";
        assert_eq!(matching(css, "p", ""), ["h:8"]);
        assert_eq!(matching(css, "a", ""), Vec::<String>::new());
        assert!(matching("p{a:1", "p", "").is_empty());
        assert_eq!(matching("} p{a:1}", "p", ""), ["a:1"]);
    }

    #[test]
    fn a_sheet_is_bounded() {
        let mut s = Sheet::default();
        s.add(&"p{a:1}".repeat(MAX_RULES * 4));
        assert_eq!(s.matching("p", "").len(), MAX_RULES);
        let long = format!(".{}{{a:1}}", "x".repeat(MAX_SELECTOR));
        assert!(matching(&long, "p", &"x".repeat(MAX_SELECTOR)).is_empty());
        let many = format!("p{}{{a:1}}", ".x".repeat(MAX_CLASSES + 1));
        assert!(matching(&many, "p", "x").is_empty());
    }

    #[test]
    fn a_hostile_sheet_is_read_in_time() {
        let mut s = Sheet::default();
        s.add(&"{".repeat(100_000));
        s.add(&"a,".repeat(100_000));
        s.add(&"}".repeat(100_000));
        assert!(s.matching("a", "").len() <= MAX_RULES);
    }
}
