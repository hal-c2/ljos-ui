//! Structured edits of a QML file that keep every byte the edit does not
//! touch.
//!
//! A visual designer edits the `.qml` file itself: there is no scene format
//! beside it. So writing back must not reformat the file, move its comments
//! or drop the author's blank lines. This crate takes the text, an [`Edit`]
//! addressed by a [`NodePath`], and answers the new text together with the
//! [`Change`]s that made it. Everything outside those changes is the input,
//! byte for byte.
//!
//! # How
//!
//! There is no separate concrete syntax tree. The parser's tree already
//! carries byte spans for every member, and the comments come with spans of
//! their own, so an edit is worked out as a splice of the original text:
//!
//! - A member's **extent** is its span widened to whole lines when it sits
//!   on lines of its own: its indentation, a `;` and a comment trailing it
//!   on the same line, the line end, and the comment lines directly above it
//!   with no empty line between (they are about it, so they travel with it).
//!   A member that shares its line with another is edited inside the line.
//! - Text that is put in (a new binding, a new or moved object) is indented
//!   like the members it joins. Lines of a moved object keep their distance
//!   to its first line; a line indented less than the object (the inside of
//!   a template literal) is copied as it is.
//!
//! - **Grouped and dotted** names are one member. `font { bold: true }` is
//!   the binding `font.bold`, not an object: it is no child and has no
//!   path of its own. A new `font.bold` goes into the object's `font { }`
//!   if it has one, next to its `font.*` bindings if it has those, and
//!   otherwise into a new group when the file writes groups of two or more
//!   bindings more often than it dots them; dotted when not. A name with a
//!   capital first part is an attached property (`Layout.fillWidth`) and is
//!   never grouped.
//! - **Members that share a line** with each other or with a brace are
//!   edited inside the line while what is put in fits on one. Text of
//!   several lines opens the object up as `qml_fmt` lays it out: each
//!   member on a line of its own one level in, comments where they were
//!   relative to their member, one empty line where the author left any,
//!   and the `}` on a line of its own. Only the object's inside changes.
//!
//! # What it promises
//!
//! The answer is checked before it is given: the new text must parse. If it
//! does not, nothing is changed and [`Error::WouldBreak`] says why, so a
//! designer can never leave a file broken. A file that does not parse is
//! refused ([`Error::Syntax`]): a splice computed on a partial tree could
//! land in the part the parser skipped.
//!
//! # Where it lives
//!
//! A tool crate beside `qml_fmt`, on `qml_syntax` alone. No runtime crate
//! depends on it (ARCHITECTURE.md rule 5): an application that lets its users
//! change it links it through the same opt-in as the compiler.

use std::fmt;
use std::ops::Range;

use qml_syntax::{
    Allocator, GetSpan, Span,
    ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject, QmlQualifiedName},
};

/// One step from an object to an object written in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// The n-th object written as a member (`Rectangle { }` inside an
    /// item), counted from 0. A grouped property such as `font { }` is not
    /// an object: it is the bindings `font.*` written together.
    Child(usize),
    /// The object a binding or a property declaration holds:
    /// `contentItem: Rectangle { }`, also `layer.effect: ShaderEffect { }`
    /// whether written dotted or in a `layer { }` group.
    Value(String),
    /// The object of `component Name: Type { }`.
    Component(String),
}

/// Where an object is, from the root object of the file. The empty path is
/// the root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodePath(pub Vec<Step>);

impl NodePath {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// The path of the n-th child object of this one.
    #[must_use]
    pub fn child(&self, n: usize) -> Self {
        let mut steps = self.0.clone();
        steps.push(Step::Child(n));
        Self(steps)
    }

    fn split_child(&self) -> Option<(NodePath, usize)> {
        match self.0.last() {
            Some(Step::Child(n)) => Some((Self(self.0[..self.0.len() - 1].to_vec()), *n)),
            _ => None,
        }
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("root")?;
        for step in &self.0 {
            match step {
                Step::Child(n) => write!(f, "/{n}")?,
                Step::Value(name) => write!(f, "/{name}:")?,
                Step::Component(name) => write!(f, "/component {name}")?,
            }
        }
        Ok(())
    }
}

/// What to change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Give `name` the value `value` (QML as it would be written after the
    /// `:`). A dotted name is one member however it is written: `font.bold`
    /// finds `font.bold: x` and `bold: x` in `font { }` alike. An existing
    /// binding keeps its place. A new one follows the file's style (see
    /// the crate docs); otherwise it goes after the object's last binding
    /// or property that comes before its first child object, function or
    /// inline component.
    SetBinding {
        object: NodePath,
        name: String,
        value: String,
    },
    /// Take `name` out, found as [`SetBinding`](Self::SetBinding) finds
    /// it. A group it leaves empty goes too, unless a comment is in it.
    RemoveBinding {
        object: NodePath,
        name: String,
    },
    /// Put the object written in `text` in `parent` so that it becomes
    /// child `index` (equal to the number of children: after the last
    /// member).
    InsertObject {
        parent: NodePath,
        index: usize,
        text: String,
    },
    RemoveObject {
        object: NodePath,
    },
    /// Move a child object, with the comments above it, so that it becomes
    /// child `index` of `parent` (counted without it).
    MoveObject {
        object: NodePath,
        parent: NodePath,
        index: usize,
    },
}

/// One splice of the original text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Bytes of the original text that were replaced.
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub text: String,
    /// In the order of `range`, not overlapping.
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("the file does not parse as QML: {0}")]
    Syntax(String),
    #[error("there is no object at {0}")]
    NotFound(NodePath),
    #[error("{0} is not a child object, so it cannot be moved or removed on its own")]
    NotAChild(NodePath),
    #[error("{object} has no binding `{name}`")]
    NoBinding { object: NodePath, name: String },
    #[error("an object cannot be moved into itself")]
    IntoItself,
    /// A layout the splicer does not handle yet. Nothing was changed.
    #[error("this edit is not supported yet: {0}")]
    Unsupported(&'static str),
    #[error("the edit would leave text that does not parse ({0}); nothing was changed")]
    WouldBreak(String),
}

/// Applies `edit` to `source`.
///
/// # Errors
/// See [`Error`]; on any error the caller's text stays as it was.
pub fn apply(source: &str, edit: &Edit) -> Result<Applied, Error> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return Err(Error::Syntax(first_problem(&parsed, source)));
    }
    let text = Text::new(source, &parsed);
    let mut changes = text.plan(&parsed.document.root, edit)?;
    changes.sort_by_key(|c| c.range.start);
    if changes
        .windows(2)
        .any(|pair| pair[0].range.end > pair[1].range.start)
    {
        return Err(Error::Unsupported("the edit's changes overlap"));
    }
    let out = splice(source, &changes);
    let again = qml_syntax::parse(&allocator, &out);
    if !again.is_ok() {
        return Err(Error::WouldBreak(first_problem(&again, &out)));
    }
    Ok(Applied { text: out, changes })
}

/// The path of the object whose `id` is `id`.
///
/// # Errors
/// [`Error::Syntax`] when `source` does not parse.
pub fn find_id(source: &str, id: &str) -> Result<Option<NodePath>, Error> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return Err(Error::Syntax(first_problem(&parsed, source)));
    }
    let mut found = None;
    walk(
        &parsed.document.root,
        &mut NodePath::root(),
        &mut |object, path| {
            if found.is_none()
                && binding(object, "id")
                    .is_some_and(|b| qml_syntax::text(source, value_span(source, b)) == id)
            {
                found = Some(path.clone());
            }
        },
    );
    Ok(found)
}

/// Every object of the file with its path, root first, in the order written.
///
/// # Errors
/// [`Error::Syntax`] when `source` does not parse.
pub fn objects(source: &str) -> Result<Vec<(NodePath, String)>, Error> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    if !parsed.is_ok() {
        return Err(Error::Syntax(first_problem(&parsed, source)));
    }
    let mut all = Vec::new();
    walk(
        &parsed.document.root,
        &mut NodePath::root(),
        &mut |object, path| {
            all.push((path.clone(), object.type_name.to_string()));
        },
    );
    Ok(all)
}

fn walk<'a>(
    object: &QmlObject<'a>,
    path: &mut NodePath,
    f: &mut impl FnMut(&QmlObject<'a>, &NodePath),
) {
    f(object, path);
    walk_members(object, "", &mut 0, path, f);
}

/// The objects written in `object`'s members; `prefix` is the name of the
/// groups they are in (`layer.`), `child` counts the child objects so far.
fn walk_members<'a>(
    object: &QmlObject<'a>,
    prefix: &str,
    child: &mut usize,
    path: &mut NodePath,
    f: &mut impl FnMut(&QmlObject<'a>, &NodePath),
) {
    for member in &object.members {
        let (step, inner) = match member {
            QmlMember::Object(group) if is_group(group) => {
                let prefix = format!("{prefix}{}.", group.type_name);
                // An object written in a group is no child of anything
                // (QML refuses it): only its bindings' values are walked.
                walk_members(group, &prefix, child, path, f);
                continue;
            }
            QmlMember::Object(inner) if prefix.is_empty() => {
                *child += 1;
                (Step::Child(*child - 1), inner)
            }
            QmlMember::Binding(b) => match &b.value {
                QmlBindingValue::Object(inner) => {
                    (Step::Value(format!("{prefix}{}", b.name)), inner)
                }
                _ => continue,
            },
            QmlMember::Property(p) if prefix.is_empty() => match &p.value {
                Some(QmlBindingValue::Object(inner)) => {
                    (Step::Value(p.name.name.to_string()), inner)
                }
                _ => continue,
            },
            QmlMember::InlineComponent(c) if prefix.is_empty() => {
                (Step::Component(c.name.name.to_string()), &c.object)
            }
            _ => continue,
        };
        path.0.push(step);
        walk(inner, path, f);
        path.0.pop();
    }
}

fn first_problem(parsed: &qml_syntax::Parsed<'_>, source: &str) -> String {
    parsed.diagnostics.first().map_or_else(
        || "the file has no root object".to_owned(),
        |d| {
            let (line, column) = d.position(source);
            format!("{line}:{column}: {}", d.message)
        },
    )
}

impl Applied {
    /// The changes that turn [`text`](Self::text) back into `source`, the
    /// text the edit was applied to: an undo that gives the bytes back
    /// whatever the edit did to spacing.
    #[must_use]
    pub fn inverse(&self, source: &str) -> Vec<Change> {
        let mut shift = 0isize;
        self.changes
            .iter()
            .map(|change| {
                let start = change.range.start.saturating_add_signed(shift);
                shift += change.replacement.len() as isize - change.range.len() as isize;
                Change {
                    range: start..start + change.replacement.len(),
                    replacement: source[change.range.clone()].to_owned(),
                }
            })
            .collect()
    }
}

/// `source` with `changes` (sorted, not overlapping) made.
#[must_use]
pub fn splice(source: &str, changes: &[Change]) -> String {
    let mut out = String::with_capacity(source.len() + 64);
    let mut at = 0;
    for change in changes {
        out.push_str(&source[at..change.range.start]);
        out.push_str(&change.replacement);
        at = change.range.end;
    }
    out.push_str(&source[at..]);
    out
}

/// Where a member is in the text, as far as removing or moving it goes.
#[derive(Clone, Copy, Debug)]
enum Extent {
    /// Whole lines, the last one's line end included.
    Lines { start: usize, end: usize },
    /// Inside a line it shares with other members. `after` is past a `;`
    /// following it and the blanks after that, if there is one.
    Inline {
        start: usize,
        end: usize,
        after: Option<usize>,
    },
}

/// Text put in, and the indentation its lines after the first were written
/// at (`""` for text the caller wrote).
struct Snippet {
    text: String,
    base: String,
    /// For text moved from elsewhere, whether an empty line stood above it
    /// and below it there (`None` below for the last member, which has no
    /// neighbour below). It is put down with the same rather than taking
    /// its new neighbours' spacing, the inverse of what [`Text::removal`]
    /// does to empty lines, so that moving it back mostly gives the file
    /// back. Not always: what was below the last member is not known.
    gaps: Option<(bool, Option<bool>)>,
}

struct Text<'s> {
    src: &'s str,
    /// Start and end of every comment, by start.
    comments: Vec<(usize, usize)>,
    nl: &'static str,
    unit: String,
}

impl<'s> Text<'s> {
    fn new(src: &'s str, parsed: &qml_syntax::Parsed<'_>) -> Self {
        let mut comments: Vec<(usize, usize)> = parsed
            .comments
            .iter()
            .map(|c| (c.span.start as usize, c.span.end as usize))
            .collect();
        comments.sort_unstable();
        let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
        let mut text = Self {
            src,
            comments,
            nl,
            unit: "    ".to_owned(),
        };
        text.unit = text.detect_unit(&parsed.document.root);
        text
    }

    /// The indentation one level adds, read off the first member written on
    /// a line of its own; four spaces when there is none.
    fn detect_unit(&self, object: &QmlObject<'_>) -> String {
        let outer = self.indent_at(object.span.start as usize);
        for member in &object.members {
            let span = member_span(member);
            if let Extent::Lines { .. } = self.extent(span) {
                let inner = self.indent_at(span.start as usize);
                if let Some(unit) = inner.strip_prefix(outer)
                    && !unit.is_empty()
                {
                    return unit.to_owned();
                }
            }
            if let QmlMember::Object(inner) = member {
                let unit = self.detect_unit(inner);
                if unit != "    " {
                    return unit;
                }
            }
        }
        "    ".to_owned()
    }

    fn plan(&self, root: &QmlObject<'_>, edit: &Edit) -> Result<Vec<Change>, Error> {
        match edit {
            Edit::SetBinding {
                object,
                name,
                value,
            } => {
                let target = resolve(root, object)?;
                if let Some(chain) = find_binding(target, name) {
                    let (holder, index) = chain[chain.len() - 1];
                    let QmlMember::Binding(b) = &holder.members[index] else {
                        unreachable!("find_binding gives bindings")
                    };
                    let span = value_span(self.src, b);
                    let indent = self.indent_at(b.span.start as usize).to_owned();
                    let replacement = self.reindent(
                        &Snippet {
                            text: value.clone(),
                            base: String::new(),
                            gaps: None,
                        },
                        &indent,
                    );
                    return Ok(vec![Change {
                        range: span.start as usize..span.end as usize,
                        replacement,
                    }]);
                }
                Ok(vec![self.new_binding(root, target, name, value)?])
            }
            Edit::RemoveBinding { object, name } => {
                let target = resolve(root, object)?;
                let chain = find_binding(target, name).ok_or_else(|| Error::NoBinding {
                    object: object.clone(),
                    name: name.clone(),
                })?;
                // A group the binding leaves empty goes with it, unless a
                // comment of its own is in it.
                let mut level = chain.len() - 1;
                while level > 0 {
                    let (group, index) = chain[level];
                    if group.members.len() > 1 || self.has_own_comment(group, index) {
                        break;
                    }
                    level -= 1;
                }
                let (holder, index) = chain[level];
                Ok(vec![self.removal(holder, index)])
            }
            Edit::InsertObject {
                parent,
                index,
                text,
            } => {
                let target = resolve(root, parent)?;
                let members: Vec<&QmlMember<'_>> = target.members.iter().collect();
                let at = child_member_index(&members, *index)
                    .ok_or_else(|| Error::NotFound(parent.child(*index)))?;
                let snippet = Snippet {
                    text: text.clone(),
                    base: String::new(),
                    gaps: None,
                };
                Ok(vec![
                    self.insertion(target, &members, at, &snippet, true, None)?,
                ])
            }
            Edit::RemoveObject { object } => {
                let (parent, n) = object
                    .split_child()
                    .ok_or_else(|| Error::NotAChild(object.clone()))?;
                let target = resolve(root, &parent)?;
                let index = nth_child(target, n).ok_or_else(|| Error::NotFound(object.clone()))?;
                Ok(vec![self.removal(target, index)])
            }
            Edit::MoveObject {
                object,
                parent: destination,
                index,
            } => {
                let (parent, n) = object
                    .split_child()
                    .ok_or_else(|| Error::NotAChild(object.clone()))?;
                if destination.0.starts_with(&object.0) {
                    return Err(Error::IntoItself);
                }
                let source = resolve(root, &parent)?;
                let from = nth_child(source, n).ok_or_else(|| Error::NotFound(object.clone()))?;
                let target = resolve(root, destination)?;
                if parent == *destination && *index == n {
                    return Ok(Vec::new());
                }
                let cut = self.removal(source, from);
                let moved = self.cut_text(&source.members[from]);
                let members: Vec<&QmlMember<'_>> = if parent == *destination {
                    target
                        .members
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != from)
                        .map(|(_, m)| m)
                        .collect()
                } else {
                    target.members.iter().collect()
                };
                let at = child_member_index(&members, *index)
                    .ok_or_else(|| Error::NotFound(destination.child(*index)))?;
                let put = self.insertion(target, &members, at, &moved, true, Some(&cut))?;
                if put.range.start <= cut.range.start && cut.range.end <= put.range.end {
                    // The destination was opened up and its new inside was
                    // written without what moves.
                    return Ok(vec![put]);
                }
                if put.range.start > cut.range.start && put.range.start < cut.range.end {
                    return Err(Error::Unsupported(
                        "the place to move to is inside what moves",
                    ));
                }
                Ok(vec![cut, put])
            }
        }
    }

    /// The change that writes the binding `name: value` into `object`,
    /// which has none of that name, in the file's style: into the group of
    /// its first part if `object` has one (`font { }` for `font.bold`), next
    /// to the bindings dotted the same way if it has those, or else as a new
    /// group when the file writes groups more often than dotted names.
    /// Other bindings go after the last binding or property that comes
    /// before the first child object, function or inline component.
    fn new_binding(
        &self,
        root: &QmlObject<'_>,
        object: &QmlObject<'_>,
        name: &str,
        value: &str,
    ) -> Result<Change, Error> {
        let members: Vec<&QmlMember<'_>> = object.members.iter().collect();
        let line = |name: &str| Snippet {
            text: format!("{name}: {value}"),
            base: String::new(),
            gaps: None,
        };
        let mut snippet = line(name);
        if let Some((first, rest)) = name.split_once('.')
            && starts_lower(first)
        {
            if let Some((group, rest)) = object.members.iter().find_map(|m| {
                let rest = group_rest(m, name)?;
                let QmlMember::Object(group) = m else {
                    unreachable!("group_rest gives groups")
                };
                Some((group, rest))
            }) {
                return self.new_binding(root, group, rest, value);
            }
            if let Some(last) = members.iter().rposition(|m| {
                matches!(m, QmlMember::Binding(b) if b.name.parts.len() > 1 && b.name.parts[0] == first)
            }) {
                return self.insertion(object, &members, last + 1, &snippet, false, None);
            }
            if prefers_groups(root) {
                // The value's own lines move in with it.
                let value = value.replace('\n', &format!("\n{}", self.unit));
                snippet.text = format!("{first} {{\n{}{rest}: {value}\n}}", self.unit);
            }
        }
        let first_structural = members
            .iter()
            .position(|m| {
                is_child(m) || matches!(m, QmlMember::Function(_) | QmlMember::InlineComponent(_))
            })
            .unwrap_or(members.len());
        let at = members[..first_structural]
            .iter()
            .rposition(|m| {
                matches!(
                    m,
                    QmlMember::Binding(_) | QmlMember::Property(_) | QmlMember::Object(_)
                )
            })
            .map_or(0, |i| i + 1);
        self.insertion(object, &members, at, &snippet, false, None)
    }

    /// Whether a comment in `group` is not about its member `index`: one
    /// that would be lost with the group.
    fn has_own_comment(&self, group: &QmlObject<'_>, index: usize) -> bool {
        let (open, close) = self.braces(group);
        let (start, end) = match self.extent(member_span(&group.members[index])) {
            Extent::Lines { start, end } => (start, end),
            Extent::Inline { start, end, after } => (start, after.unwrap_or(end)),
        };
        self.comments
            .iter()
            .any(|c| c.0 > open && c.0 < close && !(c.0 >= start && c.1 <= end))
    }

    /// The text of a member as it would be put somewhere else.
    fn cut_text(&self, member: &QmlMember<'_>) -> Snippet {
        let span = member_span(member);
        match self.extent(span) {
            Extent::Lines { start, end } => {
                let base = self.indent_at(span.start as usize).to_owned();
                let mut text = self.src[start..end].trim_start_matches([' ', '\t']);
                text = text.strip_suffix('\n').unwrap_or(text);
                text = text.strip_suffix('\r').unwrap_or(text);
                Snippet {
                    text: text.to_owned(),
                    base,
                    gaps: Some((
                        self.blank_line_above(start),
                        self.followed_by_member(end)
                            .then(|| self.blank_line_at(end)),
                    )),
                }
            }
            Extent::Inline { .. } => Snippet {
                text: qml_syntax::text(self.src, span).to_owned(),
                base: self.indent_at(span.start as usize).to_owned(),
                gaps: None,
            },
        }
    }

    /// The change that puts `snippet` so that it becomes `members[at]` of
    /// `object`.
    ///
    /// A binding joins the member before it, with no empty line. An object
    /// joins the group of the member after it: if an empty line separates
    /// that member from what is above, one separates the new object from it
    /// too, so that the object stands on its own as its neighbours do and
    /// [`removal`](Self::removal) gives the file back as it was.
    ///
    /// `cut` is a change made with this one (what a move takes out): if
    /// the object has to be opened up, its inside is written with `cut`
    /// made.
    fn insertion(
        &self,
        object: &QmlObject<'_>,
        members: &[&QmlMember<'_>],
        at: usize,
        snippet: &Snippet,
        grouped: bool,
        cut: Option<&Change>,
    ) -> Result<Change, Error> {
        let one_line = !snippet.text.contains('\n');
        if members.is_empty() {
            let (open, close) = self.braces(object);
            let outer = self.indent_at(open).to_owned();
            let close_line = line_start(self.src, close);
            if self.comments.iter().any(|c| c.0 > open && c.0 < close)
                || (self.src[open..close].contains('\n') && !is_blank(&self.src[close_line..close]))
            {
                return self.open_up(object, members, at, snippet, grouped, cut);
            }
            if !self.src[open..close].contains('\n') {
                let replacement = if one_line {
                    format!(" {} ", snippet.text)
                } else {
                    let inner = format!("{outer}{}", self.unit);
                    format!(
                        "{nl}{inner}{}{nl}{outer}",
                        self.reindent(snippet, &inner),
                        nl = self.nl
                    )
                };
                return Ok(Change {
                    range: open + 1..close,
                    replacement,
                });
            }
            let inner = format!("{}{}", self.indent_at(close), self.unit);
            return Ok(Change {
                range: close_line..close_line,
                replacement: format!("{inner}{}{}", self.reindent(snippet, &inner), self.nl),
            });
        }
        let before = if grouped { at < members.len() } else { at == 0 };
        let anchor = if before { members[at] } else { members[at - 1] };
        let span = member_span(anchor);
        let indent = self.indent_at(span.start as usize).to_owned();
        let nl = self.nl;
        match (self.extent(span), before) {
            (Extent::Lines { start, .. }, true) => {
                let spaced = grouped && self.blank_line_above(start);
                let (above, below) = snippet.gaps.map_or((spaced, spaced), |(above, below)| {
                    (above, below.unwrap_or(spaced))
                });
                let (at, gap) = match (spaced, above, below) {
                    (true, true, true) => (start, nl),
                    // It stood right under the member above, apart from
                    // this one: it goes above the empty lines.
                    (true, false, true) => (self.blank_run_above(start), ""),
                    _ => (start, ""),
                };
                Ok(Change {
                    range: at..at,
                    replacement: format!("{indent}{}{nl}{gap}", self.reindent(snippet, &indent)),
                })
            }
            (Extent::Lines { start, end }, false) => {
                let gap = if grouped
                    && snippet
                        .gaps
                        .map_or_else(|| self.blank_line_above(start), |gaps| gaps.0)
                {
                    nl
                } else {
                    ""
                };
                Ok(Change {
                    range: end..end,
                    replacement: format!("{gap}{indent}{}{nl}", self.reindent(snippet, &indent)),
                })
            }
            (Extent::Inline { .. }, _) if !one_line => {
                self.open_up(object, members, at, snippet, grouped, cut)
            }
            // QML takes no `;` after an object or a block: a blank
            // separates them.
            (Extent::Inline { start, .. }, true) => Ok(Change {
                range: start..start,
                replacement: format!("{}{}", snippet.text, if grouped { " " } else { "; " }),
            }),
            (Extent::Inline { end, .. }, false) => Ok(Change {
                range: end..end,
                replacement: format!(
                    "{}{}",
                    if self.src[..end].ends_with('}') {
                        " "
                    } else {
                        "; "
                    },
                    snippet.text
                ),
            }),
        }
    }

    /// Puts several lines into an object whose members share lines, by
    /// laying its inside out the way `qml_fmt` does: each member on a line
    /// of its own one level in, a comment that trailed a member still
    /// trailing it, one empty line where the author left any between two
    /// members, and the `}` on a line of its own. The object's inside is the
    /// edited span; everything around it stays.
    ///
    /// `members[at]` is the member the snippet goes before (`at` past the
    /// end: after the last member). `members` may leave out a member of
    /// `object` that moves; it goes, with what `cut` takes out with it.
    fn open_up(
        &self,
        object: &QmlObject<'_>,
        members: &[&QmlMember<'_>],
        at: usize,
        snippet: &Snippet,
        grouped: bool,
        cut: Option<&Change>,
    ) -> Result<Change, Error> {
        let (open, close) = self.braces(object);
        let cut = cut.filter(|c| c.range.start > open && c.range.end <= close);
        let taken = |start: usize, end: usize| {
            cut.is_some_and(|c| c.range.start <= start && end <= c.range.end)
        };
        // What the inside holds, in order: members, and comments that are
        // in no member.
        let mut pieces: Vec<Piece> = Vec::new();
        for member in &object.members {
            let span = member_span(member);
            let (start, end) = (span.start as usize, self.end_of(span));
            if !taken(start, end) {
                pieces.push(Piece::Code { start, end });
            }
        }
        let mut members_at = object.members.iter().map(member_span).peekable();
        for &(start, end) in self.comments.iter().filter(|c| c.0 > open && c.1 < close) {
            while members_at.peek().is_some_and(|s| s.end as usize <= start) {
                members_at.next();
            }
            let in_member = members_at.peek().is_some_and(|s| s.start as usize <= start);
            if !in_member && !taken(start, end) {
                pieces.push(Piece::Comment { start, end });
            }
        }
        pieces.sort_by_key(Piece::start);

        // Where the snippet goes: above the comment lines that are about the
        // member it goes before.
        let before = members.get(at).map(|m| {
            let span = member_span(m);
            match self.extent(span) {
                Extent::Lines { start, .. } => start.max(open + 1),
                Extent::Inline { start, .. } => start,
            }
        });
        let new_at = before.map_or(pieces.len(), |key| {
            pieces.partition_point(|p| p.start() < key)
        });

        let outer = self.indent_at(open);
        let inner = format!("{outer}{}", self.unit);
        let new_line = |out: &mut String, blank: bool| {
            out.push_str(self.nl);
            if blank {
                out.push_str(self.nl);
            }
            out.push_str(&inner);
        };
        let new_text = self.reindent(snippet, &inner);
        let mut out = String::new();
        let mut line_has_code = false;
        let mut previous_comment = false;
        let mut last_blank = false;
        for (index, piece) in pieces.iter().enumerate() {
            let (start, end) = (piece.start(), piece.end());
            let gap_start = index.checked_sub(1).map_or(open + 1, |i| pieces[i].end());
            let gap = self.copied(gap_start, start, cut);
            if !gap.bytes().all(|b| b.is_ascii_whitespace() || b == b';') {
                return Err(Error::Unsupported("text between the members of an object"));
            }
            let lines = gap.matches('\n').count();
            let is_comment = matches!(piece, Piece::Comment { .. });
            // No empty line is kept after `{`.
            let blank = index > 0 && lines >= 2;
            if index == new_at {
                // An empty line above stays above. A new object also stands
                // apart from the member below as that member stood apart.
                new_line(&mut out, blank);
                out.push_str(&new_text);
                new_line(&mut out, grouped && blank);
                line_has_code = false;
            } else if lines == 0 && (is_comment || (previous_comment && !line_has_code)) {
                // A comment trailing what is before it, or code after a
                // comment that ends on its line, stays on that line.
                out.push(' ');
            } else {
                new_line(&mut out, blank);
                line_has_code = false;
            }
            out.push_str(&self.reindent(
                &Snippet {
                    text: self.copied(start, end, cut),
                    base: self.indent_at(start).to_owned(),
                    gaps: None,
                },
                &inner,
            ));
            line_has_code |= !is_comment;
            previous_comment = is_comment;
            last_blank = blank;
        }
        if new_at == pieces.len() {
            new_line(&mut out, grouped && last_blank);
            out.push_str(&new_text);
        }
        out.push_str(self.nl);
        out.push_str(outer);
        Ok(Change {
            range: open + 1..close,
            replacement: out,
        })
    }

    /// `start..end` of the text, with `cut` made if it is inside.
    fn copied(&self, start: usize, end: usize, cut: Option<&Change>) -> String {
        match cut {
            Some(c) if start <= c.range.start && c.range.end <= end => format!(
                "{}{}{}",
                &self.src[start..c.range.start],
                c.replacement,
                &self.src[c.range.end..end]
            ),
            _ => self.src[start..end].to_owned(),
        }
    }

    /// The change that takes `object.members[index]` out.
    fn removal(&self, object: &QmlObject<'_>, index: usize) -> Change {
        let member = &object.members[index];
        let span = member_span(member);
        let extent = self.extent(span);
        let (open, close) = self.braces(object);
        let (start, end) = match extent {
            Extent::Lines { start, end } => (start, end),
            Extent::Inline { start, end, after } => (start, after.unwrap_or(end)),
        };
        // The only member: what is left is `Type {}`, the formatter's form.
        if object.members.len() == 1
            && is_blank(&self.src[open + 1..start])
            && is_blank(&self.src[end..close])
        {
            return Change {
                range: open + 1..close,
                replacement: String::new(),
            };
        }
        match extent {
            Extent::Lines { mut start, mut end } => {
                if index + 1 == object.members.len() && is_blank(&self.src[end..close]) {
                    // The last member: the empty lines before it would end
                    // up before the `}`.
                    while start > open + 1 {
                        let previous = line_start(self.src, start - 1);
                        if previous <= open || !is_blank(&self.src[previous..start]) {
                            break;
                        }
                        start = previous;
                    }
                } else if self.blank_line_above(start) {
                    // Empty lines on both sides: one group of them is
                    // enough.
                    while end < close {
                        let next = self.src[end..].find('\n').map_or(close, |i| end + i + 1);
                        if next > close || !is_blank(&self.src[end..next]) {
                            break;
                        }
                        end = next;
                    }
                }
                Change {
                    range: start..end,
                    replacement: String::new(),
                }
            }
            Extent::Inline { start, end, after } => {
                let next = skip_blanks(self.src.as_bytes(), end);
                let range = match after {
                    Some(after) => start..after,
                    // `MenuItem {} MenuItem {}`: blanks are the separator.
                    None if !matches!(
                        self.src.as_bytes().get(next),
                        None | Some(b'}' | b'\n' | b'\r')
                    ) && self.comment_from(next).is_none() =>
                    {
                        start..next
                    }
                    // `a: 1; b: 2 }`: the `; ` before goes with `b`.
                    None if index > 0 => self.end_of(member_span(&object.members[index - 1]))..end,
                    None => start..end,
                };
                Change {
                    range,
                    replacement: String::new(),
                }
            }
        }
    }

    fn extent(&self, span: Span) -> Extent {
        let (start, end) = (span.start as usize, self.end_of(span));
        let bytes = self.src.as_bytes();
        let mut after = skip_blanks(bytes, end);
        let semicolon = bytes.get(after) == Some(&b';');
        if semicolon {
            after = skip_blanks(bytes, after + 1);
        }
        let mut line_rest = after;
        if let Some(comment_end) = self.comment_from(line_rest)
            && !self.src[line_rest..comment_end].contains('\n')
        {
            line_rest = skip_blanks(bytes, comment_end);
        }
        let first = line_start(self.src, start);
        let at_eol = line_rest == bytes.len()
            || bytes[line_rest] == b'\n'
            || (bytes[line_rest] == b'\r' && bytes.get(line_rest + 1) == Some(&b'\n'));
        if !(is_blank(&self.src[first..start]) && at_eol) {
            return Extent::Inline {
                start,
                end,
                after: semicolon.then_some(after),
            };
        }
        let last = match bytes.get(line_rest) {
            None => line_rest,
            Some(b'\r') => line_rest + 2,
            Some(_) => line_rest + 1,
        };
        let mut top = first;
        while top > 0 {
            let previous = line_start(self.src, top - 1);
            let line = self.src[previous..top].trim_end();
            if line.is_empty() {
                break;
            }
            let line_end = previous + line.len();
            match self.comment_to(line_end) {
                Some(comment_start)
                    if is_blank(&self.src[line_start(self.src, comment_start)..comment_start]) =>
                {
                    top = line_start(self.src, comment_start);
                }
                _ => break,
            }
        }
        Extent::Lines {
            start: top,
            end: last,
        }
    }

    /// The end of a member without the `;` the parser counts in some
    /// members' spans, so that the separator is handled in one place.
    fn end_of(&self, span: Span) -> usize {
        let end = span.end as usize;
        if self.src.as_bytes()[end - 1] == b';' {
            end - 1
        } else {
            end
        }
    }

    /// Whether something other than empty lines and `}` comes after `end`.
    fn followed_by_member(&self, end: usize) -> bool {
        self.src[end..]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|c| c != '}')
    }

    /// Whether the line starting at `start` is empty.
    fn blank_line_at(&self, start: usize) -> bool {
        start < self.src.len() && {
            let end = self.src[start..]
                .find('\n')
                .map_or(self.src.len(), |i| start + i);
            is_blank(&self.src[start..end])
        }
    }

    /// The start of the empty lines right above the line at `start`.
    fn blank_run_above(&self, mut start: usize) -> usize {
        while self.blank_line_above(start) {
            start = line_start(self.src, start - 1);
        }
        start
    }

    /// Whether the line above the one starting at `start` is empty.
    fn blank_line_above(&self, start: usize) -> bool {
        start > 0 && {
            let previous = line_start(self.src, start - 1);
            is_blank(&self.src[previous..start])
        }
    }

    /// The end of the comment that starts at `at`.
    fn comment_from(&self, at: usize) -> Option<usize> {
        let i = self.comments.partition_point(|c| c.0 < at);
        self.comments.get(i).filter(|c| c.0 == at).map(|c| c.1)
    }

    /// The start of the comment that ends at `at`.
    fn comment_to(&self, at: usize) -> Option<usize> {
        self.comments.iter().find(|c| c.1 == at).map(|c| c.0)
    }

    /// Offsets of an object's `{` and `}`.
    fn braces(&self, object: &QmlObject<'_>) -> (usize, usize) {
        let head = object
            .on
            .as_ref()
            .map_or(object.type_name.span.end, |on| on.span.end) as usize;
        let open = head + self.src[head..].find('{').unwrap_or(0);
        let close = object.span.end as usize - 1;
        debug_assert_eq!(self.src.as_bytes()[close], b'}');
        (open, close)
    }

    fn indent_at(&self, at: usize) -> &'s str {
        let start = line_start(self.src, at);
        let line = &self.src[start..];
        let width = line.len() - line.trim_start_matches([' ', '\t']).len();
        &line[..width.min(at - start)]
    }

    /// The snippet with its lines after the first at `indent` instead of
    /// `snippet.base`; the first line is placed by the caller.
    fn reindent(&self, snippet: &Snippet, indent: &str) -> String {
        let mut out = String::with_capacity(snippet.text.len() + 16);
        for (i, line) in snippet.text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if i > 0 {
                out.push_str(self.nl);
                if !is_blank(line) {
                    match line.strip_prefix(snippet.base.as_str()) {
                        Some(rest) => {
                            out.push_str(indent);
                            out.push_str(rest);
                        }
                        None => out.push_str(line),
                    }
                    continue;
                }
            }
            out.push_str(line);
        }
        out
    }
}

/// A thing of an object's inside that [`Text::open_up`] lays out.
enum Piece {
    Code { start: usize, end: usize },
    Comment { start: usize, end: usize },
}

impl Piece {
    fn start(&self) -> usize {
        match self {
            Piece::Code { start, .. } | Piece::Comment { start, .. } => *start,
        }
    }

    fn end(&self) -> usize {
        match self {
            Piece::Code { end, .. } | Piece::Comment { end, .. } => *end,
        }
    }
}

/// Whether the file writes the bindings of a group more often together
/// (`font { family: f; bold: true }`) than dotted (`font.family: f` and
/// `font.bold: true` in one object). Only groups of two or more bindings
/// count: one alone says nothing about the author's habit.
fn prefers_groups(root: &QmlObject<'_>) -> bool {
    fn count(object: &QmlObject<'_>, groups: &mut usize, dotted: &mut usize) {
        let mut prefixes: Vec<(&str, usize)> = Vec::new();
        for member in &object.members {
            match member {
                QmlMember::Object(inner) => {
                    if is_group(inner) && inner.members.len() > 1 {
                        *groups += 1;
                    }
                    count(inner, groups, dotted);
                }
                QmlMember::Binding(b) => {
                    if b.name.parts.len() > 1 && starts_lower(b.name.parts[0]) {
                        match prefixes.iter_mut().find(|(p, _)| *p == b.name.parts[0]) {
                            Some((_, n)) => *n += 1,
                            None => prefixes.push((b.name.parts[0], 1)),
                        }
                    }
                    value(&b.value, groups, dotted);
                }
                QmlMember::Property(p) => {
                    if let Some(v) = &p.value {
                        value(v, groups, dotted);
                    }
                }
                QmlMember::InlineComponent(c) => count(&c.object, groups, dotted),
                _ => {}
            }
        }
        *dotted += prefixes.iter().filter(|(_, n)| *n > 1).count();
    }
    fn value(value: &QmlBindingValue<'_>, groups: &mut usize, dotted: &mut usize) {
        match value {
            QmlBindingValue::Object(o) => count(o, groups, dotted),
            QmlBindingValue::Objects(list) => {
                for o in list {
                    count(o, groups, dotted);
                }
            }
            _ => {}
        }
    }
    let (mut groups, mut dotted) = (0, 0);
    count(root, &mut groups, &mut dotted);
    groups > dotted
}

fn resolve<'b, 'a>(root: &'b QmlObject<'a>, path: &NodePath) -> Result<&'b QmlObject<'a>, Error> {
    let mut object = root;
    for step in &path.0 {
        let next = match step {
            Step::Child(n) => nth_child(object, *n).map(|i| match &object.members[i] {
                QmlMember::Object(inner) => inner,
                _ => unreachable!("nth_child gives objects"),
            }),
            Step::Value(name) => find_binding(object, name)
                .and_then(|chain| {
                    let (holder, index) = chain[chain.len() - 1];
                    match &holder.members[index] {
                        QmlMember::Binding(QmlBinding {
                            value: QmlBindingValue::Object(inner),
                            ..
                        }) => Some(inner),
                        _ => None,
                    }
                })
                .or_else(|| {
                    object.members.iter().find_map(|m| match m {
                        QmlMember::Property(p) if p.name.name == name.as_str() => match &p.value {
                            Some(QmlBindingValue::Object(inner)) => Some(inner),
                            _ => None,
                        },
                        _ => None,
                    })
                }),
            Step::Component(name) => object.members.iter().find_map(|m| match m {
                QmlMember::InlineComponent(c) if c.name.name == name.as_str() => Some(&c.object),
                _ => None,
            }),
        };
        object = next.ok_or_else(|| Error::NotFound(path.clone()))?;
    }
    Ok(object)
}

/// Whether a member written `name { }` is a grouped property, the bindings
/// `name.*` written together, rather than an object: type names start with
/// a capital, property names do not.
fn is_group(object: &QmlObject<'_>) -> bool {
    starts_lower(object.type_name.parts[0])
}

fn starts_lower(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_lowercase)
}

/// Whether `member` is an object of its own: a child, not a group.
fn is_child(member: &QmlMember<'_>) -> bool {
    matches!(member, QmlMember::Object(o) if !is_group(o))
}

/// The member index of the n-th child object.
fn nth_child(object: &QmlObject<'_>, n: usize) -> Option<usize> {
    object
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| is_child(m))
        .nth(n)
        .map(|(i, _)| i)
}

/// The member index to insert at so that the new object is child `n`.
fn child_member_index(members: &[&QmlMember<'_>], n: usize) -> Option<usize> {
    let children: Vec<usize> = members
        .iter()
        .enumerate()
        .filter(|(_, m)| is_child(m))
        .map(|(i, _)| i)
        .collect();
    match n.cmp(&children.len()) {
        std::cmp::Ordering::Less => Some(children[n]),
        std::cmp::Ordering::Equal => Some(members.len()),
        std::cmp::Ordering::Greater => None,
    }
}

fn binding<'b, 'a>(object: &'b QmlObject<'a>, name: &str) -> Option<&'b QmlBinding<'a>> {
    object.members.iter().find_map(|m| match m {
        QmlMember::Binding(b) if is_named(&b.name, name) => Some(b),
        _ => None,
    })
}

fn is_named(name: &QmlQualifiedName<'_>, dotted: &str) -> bool {
    dotted.split('.').eq(name.parts.iter().copied())
}

/// Where the binding `name` (dotted: `font.bold`) is written in `object`,
/// as `font.bold: x` or as `bold: x` in a `font { }` group: each object on
/// the way, `object` first, with the index of the member that leads on, the
/// binding's own last. A name that starts with a capital is an attached
/// property (`Layout.fillWidth`), which cannot be grouped.
fn find_binding<'b, 'a>(
    object: &'b QmlObject<'a>,
    name: &str,
) -> Option<Vec<(&'b QmlObject<'a>, usize)>> {
    if let Some(index) = object
        .members
        .iter()
        .position(|m| matches!(m, QmlMember::Binding(b) if is_named(&b.name, name)))
    {
        return Some(vec![(object, index)]);
    }
    object
        .members
        .iter()
        .enumerate()
        .find_map(|(index, member)| {
            let rest = group_rest(member, name)?;
            let QmlMember::Object(group) = member else {
                unreachable!("group_rest gives groups")
            };
            let mut chain = find_binding(group, rest)?;
            chain.insert(0, (object, index));
            Some(chain)
        })
}

/// What is left of `name` inside `member` if that is the group it starts
/// with: `bold` for `font.bold` in `font { }`.
fn group_rest<'n>(member: &QmlMember<'_>, name: &'n str) -> Option<&'n str> {
    let QmlMember::Object(group) = member else {
        return None;
    };
    if !is_group(group) {
        return None;
    }
    let mut rest = name;
    for part in &group.type_name.parts {
        rest = rest.strip_prefix(part)?.strip_prefix('.')?;
    }
    Some(rest)
}

fn member_span(member: &QmlMember<'_>) -> Span {
    match member {
        QmlMember::Object(o) => o.span,
        QmlMember::Binding(b) => b.span,
        QmlMember::Property(p) => p.span,
        QmlMember::Signal(s) => s.span,
        QmlMember::Function(f) => f.span,
        QmlMember::InlineComponent(c) => c.span,
        QmlMember::Enum(e) => e.span,
    }
}

/// The text after a binding's `:`.
fn value_span(source: &str, b: &QmlBinding<'_>) -> Span {
    match &b.value {
        QmlBindingValue::Expression(e) => e.span(),
        QmlBindingValue::Statement(s) => s.span(),
        QmlBindingValue::Object(o) => o.span,
        QmlBindingValue::Objects(_) => {
            let colon = b.name.span.end as usize
                + source[b.name.span.end as usize..].find(':').unwrap_or(0);
            let open = colon + source[colon..].find('[').unwrap_or(0);
            let close = source[..b.span.end as usize]
                .rfind(']')
                .map_or(b.span.end as usize, |i| i + 1);
            Span::new(open as u32, close as u32)
        }
    }
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn skip_blanks(bytes: &[u8], mut at: usize) -> usize {
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

fn is_blank(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_whitespace())
}
