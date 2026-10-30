//! Locators: how an agent says which node it means without a reference
//! (ARCHITECTURE.md §19.4). They are the widely known role-selector subset:
//!
//! ```text
//! role=button[name="Delete"]     #objectName        text=notes
//! role=toolbar >> role=button    name~=^Row \d+$    ref=e9
//! role=button[disabled] >> nth=0
//! at=40,40                       at=40,40 >> nth=1
//! ```
//!
//! A locator describes; it does not remember. It finds the item again after
//! a hot reload made it anew, which a reference may not.
//!
//! `at=X,Y` is not answered from the outline: it names the items drawn at
//! a point of the window, topmost first, which only the server that draws
//! them knows (see [`Locator::at`]).

use regex::Regex;

use crate::refs::RefTable;
use crate::snapshot::{NodeValue, SnapNode, SnapTree};
use crate::{ErrorCode, SurfaceError};

/// The longest pattern `~=` takes. With the size limit of the compiled
/// program this keeps a pattern from costing more than the search it is for.
const MAX_PATTERN: usize = 256;
const MAX_LOCATOR: usize = 1024;
const MAX_STEPS: usize = 8;

/// The states a bare `[state]` filter may name.
pub const STATES: &[&str] = &[
    "focused",
    "disabled",
    "enabled",
    "checked",
    "mixed",
    "expanded",
    "collapsed",
    "selected",
    "pressed",
    "modal",
    "readonly",
    "actionable",
    "unnamed",
];

/// A locator that could not be read. The message says which part and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("locator `{locator}`: {reason}")]
pub struct LocatorError {
    pub locator: String,
    pub reason: String,
}

impl From<LocatorError> for SurfaceError {
    fn from(e: LocatorError) -> Self {
        SurfaceError::new(ErrorCode::InvalidArgument, e.to_string()).with_details(
            serde_json::json!({
                "locator": e.locator,
                "examples": ["role=button[name=\"Save\"]", "#objectName", "text=Save", "ref=e3",
                             "role=list >> role=listitem >> nth=0", "at=40,40"],
            }),
        )
    }
}

/// How a piece of text is compared.
#[derive(Clone, Debug)]
pub enum NameTest {
    /// The whole text, case and all.
    Exact(String),
    /// Anywhere in the text, whatever the case. Kept lowercased.
    Contains(String),
    Regex(Regex),
}

impl NameTest {
    fn fits(&self, text: &str) -> bool {
        match self {
            NameTest::Exact(s) => text == s,
            NameTest::Contains(s) => text.to_lowercase().contains(s.as_str()),
            NameTest::Regex(r) => r.is_match(text),
        }
    }
}

/// One condition of a step.
#[derive(Clone, Debug)]
pub enum StepFilter {
    Role(String),
    Name(NameTest),
    /// The name or the text the node holds.
    Text(NameTest),
    Value(NameTest),
    ObjectName(NameTest),
    Id(NameTest),
    State(&'static str),
}

impl StepFilter {
    fn fits(&self, node: &SnapNode) -> bool {
        // What the agent may not read it may not test either, or a search
        // would tell it letter by letter.
        let value = |test: &NameTest| match &node.value {
            Some(NodeValue::Text(t)) => test.fits(t),
            Some(NodeValue::Number(n)) => test.fits(&crate::snapshot::number(*n)),
            Some(NodeValue::Redacted) | None => false,
        };
        match self {
            StepFilter::Role(r) => node.role == *r,
            StepFilter::Name(t) => t.fits(&node.name),
            StepFilter::Text(t) => t.fits(&node.name) || value(t),
            StepFilter::Value(t) => value(t),
            StepFilter::ObjectName(t) => node.object_name.as_deref().is_some_and(|o| t.fits(o)),
            StepFilter::Id(t) => node.qml_id.as_deref().is_some_and(|i| t.fits(i)),
            StepFilter::State(s) => node.has_state(s),
        }
    }
}

/// One part of a locator, between `>>`.
#[derive(Clone, Debug)]
pub enum Step {
    /// `ref=e9`: the node with that reference.
    Ref(String),
    /// `nth=1`: the second of what was found so far.
    Nth(usize),
    /// Every node that meets all the conditions.
    Match(Vec<StepFilter>),
    /// `at=40,40`: the items drawn at that point of the window.
    At(Point),
}

/// A point of the window in logical pixels, as `at=X,Y` gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// What an `at=` locator asks for: the items drawn at `point`, topmost
/// first, or only the `nth` of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct At {
    pub point: Point,
    pub nth: Option<usize>,
}

/// A parsed locator.
#[derive(Clone, Debug)]
pub struct Locator {
    source: String,
    steps: Vec<Step>,
}

struct Parser<'a> {
    source: &'a str,
}

impl Parser<'_> {
    fn err(&self, reason: impl Into<String>) -> LocatorError {
        LocatorError {
            locator: self.source.chars().take(120).collect(),
            reason: reason.into(),
        }
    }

    /// Cuts `text` at each `sep` that is outside quotes and brackets.
    fn split<'t>(&self, text: &'t str, sep: &str) -> Vec<&'t str> {
        let (mut parts, mut start) = (Vec::new(), 0);
        let (mut quote, mut depth, mut escaped) = (None::<char>, 0usize, false);
        let mut it = text.char_indices();
        while let Some((i, c)) = it.next() {
            if escaped {
                escaped = false;
                continue;
            }
            match (quote, c) {
                (Some(_), '\\') => escaped = true,
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') if Self::opens_quote(text, i) => quote = Some(c),
                (None, '[') => depth += 1,
                (None, ']') => depth = depth.saturating_sub(1),
                (None, _) if depth == 0 && text[i..].starts_with(sep) => {
                    parts.push(&text[start..i]);
                    start = i + sep.len();
                    for _ in 1..sep.chars().count() {
                        it.next();
                    }
                }
                _ => {}
            }
        }
        parts.push(&text[start..]);
        parts
    }

    /// A quote starts a quoted value only right after an operator, so that
    /// `text=don't` is the text `don't`.
    fn opens_quote(text: &str, at: usize) -> bool {
        text[..at].trim_end().ends_with('=')
    }

    /// `"a \"b\""` is `a "b"`; anything not quoted is itself, trimmed.
    /// Returns whether it was quoted.
    fn value(&self, raw: &str) -> Result<(String, bool), LocatorError> {
        let raw = raw.trim();
        let Some(q) = raw.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            return Ok((raw.to_string(), false));
        };
        let body = &raw[1..];
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars();
        loop {
            match chars.next() {
                None => return Err(self.err(format!("`{raw}` has no closing {q}"))),
                Some('\\') => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(c) => out.push(c),
                    None => return Err(self.err(format!("`{raw}` ends in a backslash"))),
                },
                Some(c) if c == q => break,
                Some(c) => out.push(c),
            }
        }
        let rest = chars.as_str().trim();
        if !rest.is_empty() {
            return Err(self.err(format!("`{rest}` after the closing {q} of `{raw}`")));
        }
        Ok((out, true))
    }

    fn regex(&self, pattern: &str) -> Result<Regex, LocatorError> {
        if pattern.len() > MAX_PATTERN {
            return Err(self.err(format!(
                "a pattern may be {MAX_PATTERN} bytes long, this one is {}",
                pattern.len()
            )));
        }
        regex::RegexBuilder::new(pattern)
            .size_limit(1 << 18)
            .dfa_size_limit(1 << 18)
            .build()
            .map_err(|e| {
                let why = e.to_string();
                let why = why.lines().last().unwrap_or("").trim().to_string();
                self.err(format!("`{pattern}` is not a regular expression ({why})"))
            })
    }

    /// `40,40` or `12.5, 7`: finite numbers, the window's own pixels.
    fn point(&self, text: &str) -> Result<Point, LocatorError> {
        if text.contains('[') {
            return Err(self.err(format!(
                "`at={text}` takes no `[…]` condition: it names what is drawn at a point, \
                 whatever it is"
            )));
        }
        let number = |n: &str| n.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        match text.split_once(',') {
            Some((x, y)) => match (number(x), number(y)) {
                (Some(x), Some(y)) => Ok(Point { x, y }),
                _ => Err(self.err(format!(
                    "`at={text}` needs two numbers, x and y in the window's logical pixels, \
                     as in `at=40,40`"
                ))),
            },
            None => Err(self.err(format!("`at={text}` needs x and y, as in `at=40,40`"))),
        }
    }

    /// `name*="all"` is (`name`, `*=`, `"all"`).
    fn condition<'t>(&self, text: &'t str) -> Option<(&'t str, &'static str, &'t str)> {
        let key_end = text
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(text.len());
        let (key, rest) = text.split_at(key_end);
        let rest = rest.trim_start();
        for op in ["~=", "*=", "="] {
            if let Some(value) = rest.strip_prefix(op) {
                return Some((key, op, value));
            }
        }
        None
    }

    fn filter(&self, text: &str, bracket: bool) -> Result<StepFilter, LocatorError> {
        let text = text.trim();
        let Some((key, op, raw)) = self.condition(text) else {
            let word = text.to_ascii_lowercase();
            return match STATES.iter().find(|s| **s == word) {
                Some(s) if bracket => Ok(StepFilter::State(s)),
                _ if bracket => Err(self.err(format!(
                    "`[{text}]` is not a state. States are: {}",
                    STATES.join(", ")
                ))),
                _ => Err(self.err(format!(
                    "`{text}` is not a step. A step is `role=…`, `name=…`, `text=…`, `value=…`, \
                     `id=…`, `obj=…`, `#objectName`, `ref=e3`, `nth=0` or `at=40,40`"
                ))),
            };
        };
        let test = |exact_when_bare: bool| -> Result<NameTest, LocatorError> {
            if op == "~=" {
                // The pattern is taken as written, backslashes and all:
                // only the quotes around it are removed.
                let raw = raw.trim();
                let pattern = match raw.chars().next() {
                    Some(q @ ('"' | '\'')) if raw.len() >= 2 && raw.ends_with(q) => {
                        &raw[1..raw.len() - 1]
                    }
                    _ => raw,
                };
                return Ok(NameTest::Regex(self.regex(pattern)?));
            }
            let (value, quoted) = self.value(raw)?;
            Ok(match op {
                "*=" => NameTest::Contains(value.to_lowercase()),
                _ if quoted || exact_when_bare => NameTest::Exact(value),
                _ => NameTest::Contains(value.to_lowercase()),
            })
        };
        match key {
            "role" => {
                if op != "=" {
                    return Err(self.err("a role is given whole: `role=button`"));
                }
                let (value, _) = self.value(raw)?;
                if value.is_empty() {
                    return Err(self.err("`role=` needs a role, as in `role=button`"));
                }
                Ok(StepFilter::Role(value.to_ascii_lowercase()))
            }
            "name" => Ok(StepFilter::Name(test(true)?)),
            // `text=Save` is the forgiving one: any case, anywhere.
            "text" => Ok(StepFilter::Text(test(false)?)),
            "value" => Ok(StepFilter::Value(test(true)?)),
            "obj" | "objectName" => Ok(StepFilter::ObjectName(test(true)?)),
            "id" => Ok(StepFilter::Id(test(true)?)),
            _ => Err(self.err(format!(
                "`{key}` cannot be tested. What can: role, name, text, value, id, obj"
            ))),
        }
    }

    fn step(&self, text: &str) -> Result<Step, LocatorError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(self.err("there is an empty step (a `>>` with nothing on one side)"));
        }
        if RefTable::is_ref(text) {
            return Ok(Step::Ref(text.to_string()));
        }
        if let Some(r) = text.strip_prefix("ref=") {
            let r = r.trim();
            if !RefTable::is_ref(r) {
                return Err(self.err(format!(
                    "`{r}` is not a reference: references look like `e12`"
                )));
            }
            return Ok(Step::Ref(r.to_string()));
        }
        if let Some(point) = text.strip_prefix("at=") {
            return self.point(point.trim()).map(Step::At);
        }
        if let Some(n) = text.strip_prefix("nth=") {
            return match n.trim().parse::<usize>() {
                Ok(n) => Ok(Step::Nth(n)),
                Err(_) => Err(self.err(format!(
                    "`nth={}` needs a whole number counted from 0",
                    n.trim()
                ))),
            };
        }
        // The head, then any number of `[…]`.
        let head_end = {
            let mut end = text.len();
            let (mut quote, mut escaped) = (None::<char>, false);
            for (i, c) in text.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match (quote, c) {
                    (Some(_), '\\') => escaped = true,
                    (Some(q), c) if c == q => quote = None,
                    (Some(_), _) => {}
                    (None, '"' | '\'') if Self::opens_quote(text, i) => quote = Some(c),
                    (None, '[') => {
                        end = i;
                        break;
                    }
                    _ => {}
                }
            }
            end
        };
        let (head, mut rest) = text.split_at(head_end);
        let head = head.trim();
        let mut filters = Vec::new();
        if let Some(name) = head.strip_prefix('#') {
            if name.is_empty() {
                return Err(self.err("`#` needs an objectName, as in `#saveButton`"));
            }
            filters.push(StepFilter::ObjectName(NameTest::Exact(name.to_string())));
        } else if !head.is_empty() {
            filters.push(self.filter(head, false)?);
        }
        while !rest.is_empty() {
            let Some(inner) = rest.strip_prefix('[') else {
                return Err(self.err(format!("`{rest}` is not a `[…]` condition")));
            };
            let close = {
                let (mut quote, mut escaped, mut found) = (None::<char>, false, None);
                for (i, c) in inner.char_indices() {
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    match (quote, c) {
                        (Some(_), '\\') => escaped = true,
                        (Some(q), c) if c == q => quote = None,
                        (Some(_), _) => {}
                        (None, '"' | '\'') if Self::opens_quote(inner, i) => quote = Some(c),
                        (None, ']') => {
                            found = Some(i);
                            break;
                        }
                        _ => {}
                    }
                }
                found
            };
            let Some(close) = close else {
                return Err(self.err(format!("`{rest}` is not closed with `]`")));
            };
            filters.push(self.filter(&inner[..close], true)?);
            rest = inner[close + 1..].trim_start();
        }
        if filters.is_empty() {
            return Err(self.err("there is a step with no condition"));
        }
        Ok(Step::Match(filters))
    }
}

impl Locator {
    pub fn parse(source: &str) -> Result<Locator, LocatorError> {
        let p = Parser { source };
        if source.trim().is_empty() {
            return Err(p.err("it is empty"));
        }
        if source.len() > MAX_LOCATOR {
            return Err(p.err(format!("it is longer than {MAX_LOCATOR} bytes")));
        }
        let parts = p.split(source, ">>");
        if parts.len() > MAX_STEPS {
            return Err(p.err(format!("it has more than {MAX_STEPS} steps")));
        }
        let steps = parts
            .into_iter()
            .map(|s| p.step(s))
            .collect::<Result<Vec<_>, _>>()?;
        // The items at a point are a stack, not a part of the outline: no
        // step before it narrows what is drawn, and no step after it but a
        // place in the stack means anything for items that may have no
        // role, name or text.
        let starts_at = matches!(steps.first(), Some(Step::At(_)));
        for (i, step) in steps.iter().enumerate().skip(1) {
            match step {
                Step::At(_) => {
                    return Err(p.err(
                        "`at=` is the first step: it names the items drawn at a point of the \
                         window, which no step before it can narrow",
                    ));
                }
                Step::Nth(_) if i == 1 => {}
                _ if starts_at => {
                    return Err(p.err(
                        "after `at=` only `nth=` may follow: the items at a point are taken \
                         topmost first, and `nth=1` is the one under the topmost",
                    ));
                }
                _ => {}
            }
        }
        Ok(Locator {
            source: source.trim().to_string(),
            steps,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The reference, when the locator is nothing but one.
    pub fn as_ref_only(&self) -> Option<&str> {
        match self.steps.as_slice() {
            [Step::Ref(r)] => Some(r),
            _ => None,
        }
    }

    /// The point, when the locator names what is drawn there. Such a
    /// locator is answered by the server that draws, not by
    /// [`Locator::find`].
    pub fn at(&self) -> Option<At> {
        match self.steps.as_slice() {
            [Step::At(point)] => Some(At {
                point: *point,
                nth: None,
            }),
            [Step::At(point), Step::Nth(n)] => Some(At {
                point: *point,
                nth: Some(*n),
            }),
            _ => None,
        }
    }

    /// The `objectName`, when the locator is nothing but `#name` (or
    /// `obj=name`). A server can then look among the objects it does not
    /// expose as nodes.
    pub fn as_object_name(&self) -> Option<&str> {
        match self.steps.as_slice() {
            [Step::Match(f)] => match f.as_slice() {
                [StepFilter::ObjectName(NameTest::Exact(name))] => Some(name),
                _ => None,
            },
            _ => None,
        }
    }

    /// `id=name`, when the locator is nothing but `#name`: what is meant
    /// by `#app` when no object is named `app` is the object whose QML id
    /// that is, as `#` names an element by its id on the web, and a window
    /// written with an id seldom has an `objectName` too.
    pub fn id_instead(&self) -> Option<Locator> {
        let name = self.as_object_name()?;
        self.source.trim_start().starts_with('#').then(|| Locator {
            source: format!("id={name}"),
            steps: vec![Step::Match(vec![StepFilter::Id(NameTest::Exact(
                name.to_string(),
            ))])],
        })
    }

    /// The QML `id`, when the locator is nothing but `id=name`.
    pub fn as_id(&self) -> Option<&str> {
        match self.steps.as_slice() {
            [Step::Match(f)] => match f.as_slice() {
                [StepFilter::Id(NameTest::Exact(id))] => Some(id),
                _ => None,
            },
            _ => None,
        }
    }

    /// The nodes the locator describes, in outline order. Finding gives no
    /// references: the caller gives them to what it shows.
    pub fn find(&self, tree: &SnapTree, refs: &mut RefTable) -> Result<Vec<usize>, SurfaceError> {
        // `None`: nothing was narrowed yet, every node is in.
        let mut found: Option<Vec<usize>> = None;
        for step in &self.steps {
            let next = match step {
                Step::Ref(r) => {
                    let index = refs.resolve(r, tree)?;
                    match &found {
                        None => vec![index],
                        Some(f) => below(tree, f).filter(|i| *i == index).collect(),
                    }
                }
                Step::Nth(n) => match &found {
                    None => (*n < tree.len()).then_some(*n).into_iter().collect(),
                    Some(f) => f.get(*n).copied().into_iter().collect(),
                },
                Step::At(_) => {
                    return Err(SurfaceError::invalid_argument(format!(
                        "`{}`: `at=` names what is drawn at a point, which the outline does \
                         not say",
                        self.source
                    )));
                }
                Step::Match(filters) => {
                    let fits = |i: &usize| filters.iter().all(|f| f.fits(&tree.nodes()[*i]));
                    match &found {
                        None => (0..tree.len()).filter(fits).collect(),
                        Some(f) => below(tree, f).filter(fits).collect(),
                    }
                }
            };
            found = Some(next);
        }
        Ok(found.unwrap_or_default())
    }
}

/// The nodes below any of `of`, each once, in outline order.
fn below<'a>(tree: &'a SnapTree, of: &'a [usize]) -> impl Iterator<Item = usize> + 'a {
    // `of` is in outline order, so a node inside an earlier one's subtree
    // adds nothing: its range is inside that one's.
    let mut covered = 0usize;
    of.iter().flat_map(move |i| {
        let range = tree.descendants(*i);
        let start = range.start.max(covered);
        covered = covered.max(range.end);
        start..range.end.max(start)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SnapTree {
        let mut t = SnapTree::new();
        let w = t.push(None, SnapNode::new(0, "window"));
        let mut bar = SnapNode::new(1, "toolbar");
        bar.name = "Tools".into();
        bar.object_name = Some("tools".into());
        let bar = t.push(Some(w), bar);
        let mut del = SnapNode::new(2, "button");
        del.name = "Delete".into();
        del.object_name = Some("delete".into());
        del.qml_id = Some("del".into());
        t.push(Some(bar), del);
        let mut del2 = SnapNode::new(3, "button");
        del2.name = "Delete".into();
        del2.states = vec!["disabled"];
        t.push(Some(w), del2);
        let mut all = SnapNode::new(4, "button");
        all.name = "Delete all".into();
        t.push(Some(w), all);
        let mut label = SnapNode::new(5, "label");
        label.name = "3 notes".into();
        t.push(Some(w), label);
        let mut field = SnapNode::new(6, "textinput");
        field.name = "Search".into();
        field.value = Some(NodeValue::Text("milk and \"honey\"".into()));
        t.push(Some(w), field);
        let mut pw = SnapNode::new(7, "textinput");
        pw.name = "Password".into();
        pw.value = Some(NodeValue::Redacted);
        t.push(Some(w), pw);
        t
    }

    fn find(locator: &str) -> Vec<usize> {
        let t = tree();
        Locator::parse(locator)
            .unwrap_or_else(|e| panic!("{e}"))
            .find(&t, &mut RefTable::new(16))
            .unwrap()
    }

    #[test]
    fn steps_find_what_they_describe() {
        assert_eq!(find("role=button"), [2, 3, 4]);
        assert_eq!(find("role=Button"), [2, 3, 4]);
        assert_eq!(find("role=button[name=\"Delete\"]"), [2, 3]);
        assert_eq!(find("role=button[name=Delete]"), [2, 3]);
        assert_eq!(find("role=button[name*=\"all\"]"), [4]);
        assert_eq!(find("role=button[name~=\"^Delete( all)?$\"]"), [2, 3, 4]);
        assert_eq!(find("role=button[disabled]"), [3]);
        assert_eq!(find("role=button[enabled][name*=delete]"), [2, 4]);
        assert_eq!(find("#delete"), [2]);
        assert_eq!(find("obj=delete"), [2]);
        assert_eq!(find("id=del"), [2]);
        assert_eq!(find("text=notes"), [5]);
        assert_eq!(find("text=DELETE"), [2, 3, 4]);
        assert_eq!(find("text=\"Delete\""), [2, 3]);
        assert_eq!(find("name=\"Delete\""), [2, 3]);
        assert_eq!(find("name~=all$"), [4]);
        assert_eq!(find("role=window"), [0]);
        assert_eq!(find("role=checkbox"), Vec::<usize>::new());
    }

    #[test]
    fn chains_look_below_and_nth_picks() {
        assert_eq!(find("role=toolbar >> role=button"), [2]);
        assert_eq!(find("role=toolbar >> text=all"), Vec::<usize>::new());
        assert_eq!(find("role=button[name=\"Delete\"] >> nth=1"), [3]);
        assert_eq!(find("role=button >> nth=7"), Vec::<usize>::new());
        assert_eq!(find("role=window >> role=toolbar >> #delete"), [2]);
        // Both the window and the toolbar hold the button: it is found once.
        assert_eq!(find("name~=^(Tools)?$ >> role=button[name=Delete]"), [2, 3]);
        assert_eq!(find("nth=1"), [1]);
    }

    #[test]
    fn values_are_tested_and_hidden_ones_are_not() {
        assert_eq!(find("text=milk"), [6]);
        assert_eq!(find("value*=honey"), [6]);
        assert_eq!(find("value=\"milk and \\\"honey\\\"\""), [6]);
        assert_eq!(find("role=textinput[value~=.]"), [6]);
        assert_eq!(find("value~=.*"), [6]);
        assert_eq!(find("text=redacted"), Vec::<usize>::new());
    }

    #[test]
    fn text_with_separators_in_it_is_one_value() {
        let mut t = SnapTree::new();
        let w = t.push(None, SnapNode::new(0, "window"));
        let mut odd = SnapNode::new(1, "button");
        odd.name = "a >> b [c] don't".into();
        t.push(Some(w), odd);
        let found = |l: &str| {
            Locator::parse(l)
                .unwrap()
                .find(&t, &mut RefTable::new(4))
                .unwrap()
        };
        assert_eq!(found("name=\"a >> b [c] don't\""), [1]);
        assert_eq!(found("role=button[name=\"a >> b [c] don't\"]"), [1]);
        assert_eq!(found("text=don't"), [1]);
        assert_eq!(found("role=window >> name*=\"[c]\""), [1]);
    }

    #[test]
    fn references_are_steps() {
        let t = tree();
        let mut refs = RefTable::new(16);
        let r = refs.assign(&t, 1);
        let l = Locator::parse(&format!("ref={r}")).unwrap();
        assert_eq!(l.as_ref_only(), Some(r.as_str()));
        assert_eq!(l.find(&t, &mut refs).unwrap(), [1]);
        let l = Locator::parse(&format!("{r} >> role=button")).unwrap();
        assert_eq!(l.find(&t, &mut refs).unwrap(), [2]);
        let e = Locator::parse("ref=e99").unwrap().find(&t, &mut refs);
        assert_eq!(e.unwrap_err().code, ErrorCode::StaleRef);
    }

    #[test]
    fn single_names_are_recognised() {
        assert_eq!(
            Locator::parse("#page").unwrap().as_object_name(),
            Some("page")
        );
        assert_eq!(Locator::parse("id=card").unwrap().as_id(), Some("card"));
        assert_eq!(
            Locator::parse("#page[disabled]").unwrap().as_object_name(),
            None
        );
        assert_eq!(
            Locator::parse("role=a >> #page").unwrap().as_object_name(),
            None
        );
    }

    #[test]
    fn what_cannot_be_read_says_why() {
        for (locator, part) in [
            ("role=button[name", "not closed"),
            ("name~=(", "not a regular expression"),
            ("", "empty"),
            ("role=button >> ", "empty step"),
            ("button", "not a step"),
            ("role=button[shiny]", "not a state"),
            ("colour=red", "cannot be tested"),
            ("nth=two", "whole number"),
            ("ref=nine", "not a reference"),
            ("role*=but", "given whole"),
            ("name=\"open", "no closing"),
            ("#", "needs an objectName"),
            ("role=button[disabled]x", "is not a `[…]` condition"),
        ] {
            let e = Locator::parse(locator).expect_err(locator);
            assert!(e.reason.contains(part), "{locator}: {}", e.reason);
            let s: SurfaceError = e.into();
            assert_eq!(s.code, ErrorCode::InvalidArgument);
            assert!(s.message.starts_with("locator `"), "{}", s.message);
        }
        let long = format!("name~={}", "a".repeat(300));
        assert!(Locator::parse(&long).unwrap_err().reason.contains("256"));
        let many = ["role=a"; 9].join(" >> ");
        assert!(Locator::parse(&many).unwrap_err().reason.contains("steps"));
    }

    #[test]
    fn a_point_is_read_and_goes_with_nth_alone() {
        let at = |l: &str| Locator::parse(l).unwrap_or_else(|e| panic!("{e}")).at();
        let p = |x, y| Point { x, y };
        assert_eq!(
            at("at=40,40"),
            Some(At {
                point: p(40.0, 40.0),
                nth: None
            })
        );
        assert_eq!(
            at(" at= 12.5 , -3 >> nth=1"),
            Some(At {
                point: p(12.5, -3.0),
                nth: Some(1)
            })
        );
        assert_eq!(at("role=button"), None);
        for (locator, part) in [
            ("at=40", "needs x and y"),
            ("at=forty,40", "two numbers"),
            ("at=1,NaN", "two numbers"),
            ("at=1,inf", "two numbers"),
            ("at=1,2,3", "two numbers"),
            ("at=40,40[disabled]", "no `[…]` condition"),
            ("role=button >> at=40,40", "first step"),
            ("at=1,1 >> at=2,2", "first step"),
            ("at=40,40 >> #chip", "only `nth=`"),
            ("at=40,40 >> nth=0 >> nth=0", "only `nth=`"),
        ] {
            let e = Locator::parse(locator).expect_err(locator);
            assert!(e.reason.contains(part), "{locator}: {}", e.reason);
            assert!(e.reason.contains("at="), "{locator}: {}", e.reason);
        }
        // The outline cannot answer it: the server that draws does.
        let e = Locator::parse("at=1,1")
            .unwrap()
            .find(&tree(), &mut RefTable::new(4))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn a_costly_pattern_is_refused_not_run() {
        let e = Locator::parse("name~=(a{100}){100}{100}").unwrap_err();
        assert!(
            e.reason.contains("not a regular expression"),
            "{}",
            e.reason
        );
    }
}
