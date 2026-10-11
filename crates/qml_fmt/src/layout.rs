//! The printer. It walks the QML tree and the comments together, writes
//! the structure in its own layout and copies everything else.

use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::{
    Comment, GetSpan, Parsed, Span,
    ast::{
        QmlBinding, QmlBindingValue, QmlImport, QmlImportSource, QmlInlineComponent, QmlMember,
        QmlObject, QmlPragma, QmlPropertyDeclaration,
    },
    js::{StringLiteral, TemplateLiteral},
};

/// Spaces a level.
const INDENT: usize = 4;
/// What a tab counts for when the distance of a copied line to its member
/// is measured.
const TAB: usize = 4;

pub(crate) fn print(source: &str, parsed: &Parsed<'_>) -> String {
    let mut protected = Protected {
        source,
        spans: Vec::new(),
    };
    protected.object(&parsed.document.root);
    protected.spans.sort_by_key(|s| s.start);
    let mut printer = Printer {
        src: source,
        comments: &parsed.comments,
        protected: protected.spans,
        out: String::with_capacity(source.len() + source.len() / 8),
        line_has_code: false,
    };
    printer.document(parsed);
    printer.out
}

/// Text that runs over lines inside the JavaScript: its lines are not the
/// formatter's to indent or trim.
struct Protected<'s> {
    source: &'s str,
    spans: Vec<Span>,
}

impl Protected<'_> {
    fn keep(&mut self, span: Span) {
        if self.source[span.start as usize..span.end as usize].contains('\n') {
            self.spans.push(span);
        }
    }

    fn object<'a>(&mut self, object: &QmlObject<'a>) {
        for member in &object.members {
            match member {
                QmlMember::Object(o) => self.object(o),
                QmlMember::Binding(b) => self.value(&b.value),
                QmlMember::Property(p) => {
                    if let Some(v) = &p.value {
                        self.value(v);
                    }
                }
                QmlMember::Function(f) => self.visit_function(f, ScopeFlags::Function),
                QmlMember::InlineComponent(c) => self.object(&c.object),
                QmlMember::Signal(_) | QmlMember::Enum(_) => {}
            }
        }
    }

    fn value<'a>(&mut self, value: &QmlBindingValue<'a>) {
        match value {
            QmlBindingValue::Expression(e) => self.visit_expression(e),
            QmlBindingValue::Statement(s) => self.visit_statement(s),
            QmlBindingValue::Object(o) => self.object(o),
            QmlBindingValue::Objects(list) => {
                for o in list {
                    self.object(o);
                }
            }
        }
    }
}

impl<'a> Visit<'a> for Protected<'_> {
    // Not walked into: what is nested in a template is inside its span.
    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.keep(it.span);
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        self.keep(it.span);
    }
}

/// One thing of a sequence: of the top of the file, or of an object's body.
enum Node<'t, 'a> {
    Comment(&'t Comment),
    Pragma(&'t QmlPragma<'a>),
    Import(&'t QmlImport<'a>),
    Root(&'t QmlObject<'a>),
    /// A member and the start of the one after it, if any.
    Member(&'t QmlMember<'a>, Option<u32>),
}

impl Node<'_, '_> {
    fn span(&self) -> Span {
        match self {
            Node::Comment(c) => c.span,
            Node::Pragma(p) => p.span,
            Node::Import(i) => i.span,
            Node::Root(o) => o.span,
            Node::Member(m, _) => member_span(m),
        }
    }

    fn is_comment(&self) -> bool {
        matches!(self, Node::Comment(_))
    }

    fn is_header(&self) -> bool {
        matches!(self, Node::Pragma(_) | Node::Import(_))
    }
}

pub(crate) fn member_span(member: &QmlMember<'_>) -> Span {
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

struct Printer<'s> {
    src: &'s str,
    comments: &'s [Comment],
    /// Sorted, not overlapping.
    protected: Vec<Span>,
    out: String,
    /// The line being written holds something that is not a comment, so a
    /// member cannot be put after it.
    line_has_code: bool,
}

impl<'s> Printer<'s> {
    fn text(&self, span: Span) -> &'s str {
        &self.src[span.start as usize..span.end as usize]
    }

    /// The comments that start in `start..end`.
    fn comments_in(&self, start: u32, end: u32) -> &'s [Comment] {
        let from = self.comments.partition_point(|c| c.span.start < start);
        let to = self.comments.partition_point(|c| c.span.start < end);
        &self.comments[from..to]
    }

    fn has_comment(&self, start: u32, end: u32) -> bool {
        !self.comments_in(start, end).is_empty()
    }

    /// The comments in `start..end` that are in none of `inner` (sorted).
    fn loose_comments(
        &self,
        start: u32,
        end: u32,
        inner: impl Iterator<Item = Span>,
    ) -> Vec<&'s Comment> {
        let mut inner = inner.peekable();
        let mut out = Vec::new();
        for comment in self.comments_in(start, end) {
            while inner.peek().is_some_and(|s| s.end <= comment.span.start) {
                inner.next();
            }
            if inner.peek().is_none_or(|s| comment.span.start < s.start) {
                out.push(comment);
            }
        }
        out
    }

    fn inside_protected(&self, offset: usize) -> bool {
        let offset = offset as u32;
        let at = self.protected.partition_point(|s| s.end <= offset);
        self.protected
            .get(at)
            .is_some_and(|s| s.start < offset && offset < s.end)
    }

    /// The columns of white space the source line of `offset` starts with.
    fn base_of(&self, offset: u32) -> usize {
        let line_start = self.src[..offset as usize]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        columns(&self.src[line_start..]).0
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.line_has_code = false;
    }

    fn indent(&mut self, level: usize) {
        self.out.extend(std::iter::repeat_n(' ', level * INDENT));
    }

    /// Copies `start..end` of the source. The first line continues the line
    /// being written; each later one keeps its distance to the line the
    /// copied thing started on (`base` columns in the source), now `level`
    /// levels in.
    fn verbatim(&mut self, start: u32, end: u32, base: usize, level: usize) {
        let (start, end) = (start as usize, end as usize);
        let text = &self.src[start..end];
        let mut offset = start;
        for (index, line) in text.split('\n').enumerate() {
            let line_end = offset + line.len();
            let raw = index > 0 && self.inside_protected(offset);
            let keep_tail = self.inside_protected(line_end);
            let body = if keep_tail { line } else { line.trim_end() };
            if index > 0 {
                self.out.push('\n');
            }
            if index == 0 || raw {
                self.out.push_str(body);
            } else if !body.trim_start().is_empty() {
                let (cols, bytes) = columns(line);
                let rel = cols.saturating_sub(base);
                self.out
                    .extend(std::iter::repeat_n(' ', level * INDENT + rel));
                self.out.push_str(&body[bytes.min(body.len())..]);
            }
            offset = line_end + 1;
        }
    }

    fn document(&mut self, parsed: &Parsed<'_>) {
        let doc = &parsed.document;
        let mut nodes: Vec<Node<'_, '_>> = Vec::new();
        nodes.extend(doc.pragmas.iter().map(Node::Pragma));
        nodes.extend(doc.imports.iter().map(Node::Import));
        nodes.push(Node::Root(&doc.root));
        nodes.sort_by_key(|n| n.span().start);
        let spans: Vec<Span> = nodes.iter().map(Node::span).collect();
        let loose = self.loose_comments(0, self.src.len() as u32, spans.into_iter());
        nodes.extend(loose.into_iter().map(Node::Comment));
        nodes.sort_by_key(|n| n.span().start);
        self.sequence(&nodes, None, 0);
        self.newline();
    }

    /// Writes `nodes` one under the other. `open` is the offset after the
    /// `{` of the body they are in; `None` at the top of the file.
    fn sequence(&mut self, nodes: &[Node<'_, '_>], open: Option<u32>, level: usize) {
        let last_header = nodes.iter().rposition(Node::is_header);
        let mut blank_owed = false;
        let mut prev: Option<(usize, u32)> = None;
        for (index, node) in nodes.iter().enumerate() {
            let span = node.span();
            match (prev, open) {
                (None, None) => {}
                (None, Some(open)) => {
                    let gap = &self.src[open as usize..span.start as usize];
                    if node.is_comment() && !gap.contains('\n') {
                        self.out.push(' ');
                    } else {
                        self.newline();
                        self.indent(level);
                    }
                }
                (Some((prev_index, prev_end)), _) => {
                    let gap = &self.src[prev_end as usize..span.start as usize];
                    let lines = gap.matches('\n').count();
                    let joins = lines == 0
                        && (node.is_comment()
                            || (nodes[prev_index].is_comment() && !self.line_has_code));
                    if joins {
                        self.out.push(' ');
                    } else {
                        self.newline();
                        if lines >= 2 || blank_owed {
                            self.newline();
                        }
                        blank_owed = false;
                        self.indent(level);
                    }
                }
            }
            match node {
                Node::Comment(c) => {
                    let base = self.base_of(c.span.start);
                    self.verbatim(c.span.start, c.span.end, base, level);
                }
                Node::Pragma(p) => {
                    self.pragma(p);
                    self.line_has_code = true;
                }
                Node::Import(i) => {
                    self.import(i);
                    self.line_has_code = true;
                }
                Node::Root(o) => {
                    self.object(o, level);
                    self.line_has_code = true;
                }
                Node::Member(m, next) => {
                    self.member(m, *next, level);
                    self.line_has_code = true;
                }
            }
            if last_header == Some(index) {
                blank_owed = true;
            }
            prev = Some((index, span.end));
        }
    }

    /// A header line as the tree has it, if that says all the source says
    /// (the same text but for white space and a closing `;`); the source's
    /// own text otherwise.
    fn header(&mut self, span: Span, canonical: String) {
        let written = self.text(span).trim_end();
        let written = written.strip_suffix(';').unwrap_or(written).trim_end();
        let squeeze = |s: &str| s.split_whitespace().collect::<String>();
        if !self.has_comment(span.start, span.end) && squeeze(written) == squeeze(&canonical) {
            self.out.push_str(&canonical);
        } else {
            let base = self.base_of(span.start);
            self.verbatim(span.start, span.end, base, 0);
        }
    }

    fn pragma(&mut self, pragma: &QmlPragma<'_>) {
        let canonical = match pragma.value {
            Some(value) => format!("pragma {}: {value}", pragma.name),
            None => format!("pragma {}", pragma.name),
        };
        self.header(pragma.span, canonical);
    }

    fn import(&mut self, import: &QmlImport<'_>) {
        let mut canonical = String::from("import ");
        match &import.source {
            QmlImportSource::Module(name) => canonical.push_str(&name.to_string()),
            QmlImportSource::Path(_) => {
                // The literal as it was written: its quotes and escapes
                // are the author's.
                let text = self.text(import.span);
                let rest = text["import".len().min(text.len())..].trim_start();
                canonical.push_str(string_literal(rest).unwrap_or(rest));
            }
        }
        if let Some(version) = import.version {
            canonical.push(' ');
            canonical.push_str(version);
        }
        if let Some(alias) = &import.alias {
            canonical.push_str(" as ");
            canonical.push_str(alias.name.as_str());
        }
        self.header(import.span, canonical);
    }

    /// `Type {`, `Type on target {`, the members and the `}`.
    fn object(&mut self, object: &QmlObject<'_>, level: usize) {
        let name_end = object
            .on
            .as_ref()
            .map_or(object.type_name.span.end, |on| on.span.end);
        let Some(open) = self.brace_after(name_end, object.span.end) else {
            // Not an object as this printer knows one: leave it alone.
            let base = self.base_of(object.span.start);
            self.verbatim(object.span.start, object.span.end, base, level);
            return;
        };
        if self.has_comment(object.span.start, open) {
            // A comment between the name and the brace stays where it is.
            let base = self.base_of(object.span.start);
            self.verbatim(object.span.start, open + 1, base, level);
        } else {
            self.out.push_str(&object.type_name.to_string());
            if let Some(on) = &object.on {
                self.out.push_str(" on ");
                self.out.push_str(&on.to_string());
            }
            self.out.push_str(" {");
        }
        self.line_has_code = true;

        let close = object.span.end - 1;
        let mut nodes: Vec<Node<'_, '_>> = Vec::with_capacity(object.members.len());
        for (index, member) in object.members.iter().enumerate() {
            let next = object.members.get(index + 1).map(|m| member_span(m).start);
            nodes.push(Node::Member(member, next));
        }
        let loose = self.loose_comments(open + 1, close, object.members.iter().map(member_span));
        if nodes.is_empty() && loose.is_empty() {
            self.out.push('}');
            return;
        }
        nodes.extend(loose.into_iter().map(Node::Comment));
        nodes.sort_by_key(|n| n.span().start);
        self.sequence(&nodes, Some(open + 1), level + 1);
        self.newline();
        self.indent(level);
        self.out.push('}');
    }

    /// The `{` that follows `from`, over white space and comments.
    fn brace_after(&self, from: u32, end: u32) -> Option<u32> {
        let bytes = self.src.as_bytes();
        let mut at = from as usize;
        let end = end as usize;
        while at < end {
            match bytes[at] {
                b'{' => return Some(at as u32),
                b if b.is_ascii_whitespace() => at += 1,
                b'/' => {
                    let comment = self.comments_in(at as u32, at as u32 + 1).first()?;
                    at = comment.span.end as usize;
                }
                _ => return None,
            }
        }
        None
    }

    fn member(&mut self, member: &QmlMember<'_>, next: Option<u32>, level: usize) {
        let span = member_span(member);
        let base = self.base_of(span.start);
        match member {
            QmlMember::Object(o) => self.object(o, level),
            QmlMember::Binding(b) => self.binding(b, next, level),
            QmlMember::Property(p) => self.property(p, next, level),
            QmlMember::InlineComponent(c) => self.inline_component(c, level),
            QmlMember::Function(_) => self.verbatim(span.start, span.end, base, level),
            QmlMember::Signal(_) | QmlMember::Enum(_) => {
                let end = self.without_semicolon(span);
                self.verbatim(span.start, end, base, level);
            }
        }
    }

    /// The end of `span` without the `;` it may close with.
    fn without_semicolon(&self, span: Span) -> u32 {
        let text = self.text(span).trim_end();
        let text = text.strip_suffix(';').unwrap_or(text).trim_end();
        span.start + text.len() as u32
    }

    /// The member as it was written, moved to its place.
    fn as_written(&mut self, span: Span, level: usize) {
        let base = self.base_of(span.start);
        let end = span.start + self.text(span).trim_end().len() as u32;
        self.verbatim(span.start, end, base, level);
    }

    fn binding(&mut self, binding: &QmlBinding<'_>, next: Option<u32>, level: usize) {
        let head = binding.name.to_string();
        if !self.value(binding.span, &head, &binding.value, next, level) {
            self.as_written(binding.span, level);
        }
    }

    fn property(&mut self, property: &QmlPropertyDeclaration<'_>, next: Option<u32>, level: usize) {
        let span = property.span;
        let name_end = property.name.span.end;
        if self.has_comment(span.start, name_end) {
            return self.as_written(span, level);
        }
        // The modifiers are written in the author's order, which the tree
        // does not keep.
        let head = self.src[span.start as usize..name_end as usize]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        match &property.value {
            Some(value) => {
                if !self.value(span, &head, value, next, level) {
                    self.as_written(span, level);
                }
            }
            None => {
                if self.has_comment(name_end, span.end) {
                    return self.as_written(span, level);
                }
                self.out.push_str(&head);
            }
        }
    }

    fn inline_component(&mut self, component: &QmlInlineComponent<'_>, level: usize) {
        let (span, object) = (component.span, &component.object);
        if self.has_comment(span.start, object.span.start)
            || self.has_comment(object.span.end, span.end)
        {
            return self.as_written(span, level);
        }
        self.out.push_str("component ");
        self.out.push_str(component.name.name.as_str());
        self.out.push_str(": ");
        self.object(object, level);
    }

    /// `head: value`. False, with nothing written, when a comment sits
    /// where this would write its own text.
    fn value(
        &mut self,
        member: Span,
        head: &str,
        value: &QmlBindingValue<'_>,
        next: Option<u32>,
        level: usize,
    ) -> bool {
        let base = self.base_of(member.start);
        match value {
            QmlBindingValue::Expression(e) => {
                let v = e.span();
                if self.has_comment(member.start, v.start) || self.has_comment(v.end, member.end) {
                    return false;
                }
                self.out.push_str(head);
                self.out.push_str(": ");
                self.verbatim(v.start, v.end, base, level);
                // Without the `;` the next line would carry the expression on.
                if next.is_some_and(|at| self.continues_expression(at)) {
                    self.out.push(';');
                }
            }
            QmlBindingValue::Statement(s) => {
                let v = s.span();
                if self.has_comment(member.start, v.start) || self.has_comment(v.end, member.end) {
                    return false;
                }
                self.out.push_str(head);
                self.out.push_str(": ");
                self.verbatim(v.start, v.end, base, level);
            }
            QmlBindingValue::Object(o) => {
                if self.has_comment(member.start, o.span.start)
                    || self.has_comment(o.span.end, member.end)
                {
                    return false;
                }
                self.out.push_str(head);
                self.out.push_str(": ");
                self.object(o, level);
            }
            QmlBindingValue::Objects(list) => {
                let loose =
                    self.loose_comments(member.start, member.end, list.iter().map(|o| o.span));
                if !loose.is_empty() || list.is_empty() {
                    return false;
                }
                self.out.push_str(head);
                self.out.push_str(": [");
                for (index, object) in list.iter().enumerate() {
                    self.newline();
                    self.indent(level + 1);
                    self.object(object, level + 1);
                    if index + 1 < list.len() {
                        self.out.push(',');
                    }
                }
                self.newline();
                self.indent(level);
                self.out.push(']');
            }
        }
        true
    }

    /// Whether the member at `at` starts with a word that is also an
    /// operator: after a line end it would join the expression before it.
    fn continues_expression(&self, at: u32) -> bool {
        let rest = &self.src[at as usize..];
        ["instanceof", "in"].iter().any(|word| {
            rest.strip_prefix(word).is_some_and(|after| {
                !after
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
            })
        })
    }
}

/// The columns and the bytes of the white space `line` starts with.
fn columns(line: &str) -> (usize, usize) {
    let mut cols = 0;
    for (bytes, c) in line.char_indices() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += TAB,
            _ => return (cols, bytes),
        }
    }
    (cols, line.len())
}

/// The string literal `text` starts with, quotes included.
fn string_literal(text: &str) -> Option<&str> {
    let quote = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let mut escaped = false;
    for (at, c) in text.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            return Some(&text[..=at]);
        }
    }
    None
}
