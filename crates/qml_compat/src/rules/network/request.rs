//! XMLHttpRequest's usual shape written as `fetch` (docs/DIALECT.md §4.19).
//!
//! The shape: a local made of `new XMLHttpRequest()`; then, one after the
//! other, its `open`, its `setRequestHeader`s, its `responseType`, an
//! `onreadystatechange` of a function that waits for `readyState` 4, and
//! its `send`. It becomes one call where the handler was:
//!
//! ```js
//! fetch(url, { method, headers, body }).catch(() => Response.error()).then(async (http: Response) => {
//! ```
//!
//! and the handler's body, with `responseText` and `response` read from the
//! reply. A request that cannot be reached settles as `Response.error()`,
//! whose `status` is 0 and whose body is empty, as the request's were.
//! Every other shape keeps the file, and says what of it has no `fetch`.
//!
//! DECISION: every line stays where it was, as with every edit of the pass:
//! the statements `fetch` gathers are emptied, not removed, so a lowered
//! file has a blank line for each.
//! DECISION: `setRequestHeader`s become an object of their names where each
//! name is a different string, and a list of pairs otherwise, which joins
//! the values of a name as the request did.
//! DECISION: a body of a `GET` or `HEAD`, or a `null` one, is left out:
//! the request ignored it and `fetch` rejects it.

use std::ops::Range;

use oxc_ast::ast::{
    Argument, ArrowFunctionBody, ArrowFunctionExpression, AssignmentOperator, AssignmentTarget,
    BinaryOperator, BlockStatement, CallExpression, Expression, Function, FunctionBody,
    IdentifierReference, NewExpression, Statement, StaticMemberExpression, TemplateLiteral,
    ThisExpression, VariableDeclarationKind,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;
use qml_edit::Change;

use crate::bindings::{Bindings, Node};
use crate::cause::Xhr;
use crate::{Kept, member_extent};

const CLASS: &str = "XMLHttpRequest";

/// The edits that write each request of `node` of the usual shape as
/// `fetch`, or why the file is kept.
pub(crate) fn lower(source: &str, node: Node<'_, '_>, own: &Bindings) -> Result<Vec<Change>, Kept> {
    let mut find = Find {
        source,
        own,
        made: Vec::new(),
        changes: Vec::new(),
        kept: None,
    };
    node.visit(&mut find);
    match find.kept {
        Some(kept) => Err(kept),
        None => Ok(find.changes),
    }
}

struct Find<'s, 'o> {
    source: &'s str,
    own: &'o Bindings,
    /// Where each `new XMLHttpRequest()` that is a local's starts.
    made: Vec<u32>,
    changes: Vec<Change>,
    kept: Option<Kept>,
}

impl<'a> Find<'_, '_> {
    /// Whether `callee` is the global class, not a name of the script's.
    fn is_class(&self, callee: &Expression<'_>) -> bool {
        matches!(callee, Expression::Identifier(id)
            if id.name == CLASS && !self.own.binds(CLASS, id.span.start))
    }

    fn keep(&mut self, cause: Xhr, at: u32) {
        if self.kept.is_none() {
            self.kept = Some(Kept {
                cause: cause.into(),
                at: at as usize,
            });
        }
    }

    fn statements(&mut self, list: &[Statement<'a>]) {
        for (at, statement) in list.iter().enumerate() {
            let Some((name, new)) = local_of_new(statement) else {
                continue;
            };
            if !self.is_class(&new.callee) || !new.arguments.is_empty() {
                continue;
            }
            self.made.push(new.span.start);
            match Request::read(self.source, list, at, name, self.own) {
                Ok(changes) => self.changes.extend(changes),
                Err((cause, at)) => self.keep(cause, at),
            }
        }
    }
}

impl<'a> Visit<'a> for Find<'_, '_> {
    fn visit_function_body(&mut self, it: &FunctionBody<'a>) {
        self.statements(&it.statements);
        walk::walk_function_body(self, it);
    }

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.statements(&it.body);
        walk::walk_block_statement(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if self.is_class(&it.callee) && !self.made.contains(&it.span.start) {
            self.keep(Xhr::Made, it.span.start);
        }
        walk::walk_new_expression(self, it);
    }
}

/// `var name = new …(…)`, one declarator, as `(name, the new)`.
fn local_of_new<'n, 'a>(statement: &'n Statement<'a>) -> Option<(&'n str, &'n NewExpression<'a>)> {
    let Statement::VariableDeclaration(declaration) = statement else {
        return None;
    };
    if declaration.kind == VariableDeclarationKind::Using
        || declaration.kind == VariableDeclarationKind::AwaitUsing
    {
        return None;
    }
    let [declarator] = declaration.declarations.as_slice() else {
        return None;
    };
    let name = declarator.id.get_identifier_name()?;
    match declarator.init.as_ref()?.without_parentheses() {
        Expression::NewExpression(new) => Some((name.as_str(), new)),
        _ => None,
    }
}

/// What the body of the reply is read as, by `responseType`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    Text,
    Json,
    Bytes,
}

impl Read {
    /// The read of `name`'s body, a promise of what the request's
    /// `responseText` or `response` was.
    fn call(self, name: &str) -> String {
        match self {
            Read::Text => format!("{name}.text()"),
            // The request's `response` was null where the body was not JSON,
            // a failed request's or an error page's among them, and `json()`
            // rejects there: the handler's own check of `status` still runs.
            Read::Json => format!("{name}.json().catch(() => null)"),
            Read::Bytes => format!("{name}.arrayBuffer()"),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Read::Text => "Text",
            Read::Json => "Json",
            Read::Bytes => "Bytes",
        }
    }
}

/// One statement of the request's own.
enum Step<'n, 'a> {
    Open(&'n CallExpression<'a>),
    Header(&'n CallExpression<'a>),
    Send(&'n CallExpression<'a>),
    Kind(&'n Expression<'a>),
    Handler(&'n Expression<'a>),
    /// A member the usual shape has not, with where its name is.
    Other(String, u32),
}

fn step<'n, 'a>(statement: &'n Statement<'a>, name: &str) -> Option<Step<'n, 'a>> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let is_request =
        |object: &Expression<'_>| matches!(object, Expression::Identifier(id) if id.name == name);
    match statement.expression.without_parentheses() {
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(callee) = call.callee.without_parentheses()
            else {
                return None;
            };
            if !is_request(&callee.object) {
                return None;
            }
            Some(match callee.property.name.as_str() {
                "open" => Step::Open(call),
                "setRequestHeader" => Step::Header(call),
                "send" => Step::Send(call),
                other => Step::Other(other.to_owned(), callee.property.span.start),
            })
        }
        Expression::AssignmentExpression(assignment) => {
            let AssignmentTarget::StaticMemberExpression(target) = &assignment.left else {
                return None;
            };
            if !is_request(&target.object) {
                return None;
            }
            let member = target.property.name.as_str();
            Some(match member {
                _ if assignment.operator != AssignmentOperator::Assign => {
                    Step::Other(member.to_owned(), target.property.span.start)
                }
                "responseType" => Step::Kind(&assignment.right),
                "onreadystatechange" => Step::Handler(&assignment.right),
                other => Step::Other(other.to_owned(), target.property.span.start),
            })
        }
        _ => None,
    }
}

type Problem = (Xhr, u32);

/// A request of the usual shape, as read.
struct Request<'s> {
    source: &'s str,
    name: &'s str,
    problems: Vec<Problem>,
    changes: Vec<Change>,
}

impl<'s> Request<'s> {
    fn problem(&mut self, cause: Xhr, at: u32) {
        self.problems.push((cause, at));
    }

    fn text(&mut self, span: Span) -> String {
        let text = &self.source[span.start as usize..span.end as usize];
        if text.contains('\n') {
            self.problem(Xhr::Lines, span.start);
        }
        text.to_owned()
    }

    /// Empties `range`; each line it leaves blank is emptied whole.
    fn remove(&mut self, range: Range<usize>) {
        let mut start = range.start;
        for piece in self.source[range.clone()].split_inclusive('\n') {
            let end = start + piece.trim_end_matches(['\n', '\r']).len();
            let blank = self.source[start..end].trim();
            if !blank.is_empty() {
                let lead = start
                    + (self.source[start..end].len() - self.source[start..end].trim_start().len());
                let tail = lead + blank.len();
                self.changes.push(Change {
                    range: member_extent(self.source, lead, tail),
                    replacement: String::new(),
                });
            }
            start += piece.len();
        }
    }

    fn change(&mut self, span: Span, replacement: String) {
        self.changes.push(Change {
            range: span.start as usize..span.end as usize,
            replacement,
        });
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one reading of the shape, in the order the statements run"
    )]
    fn read<'a>(
        source: &'s str,
        list: &[Statement<'a>],
        made: usize,
        name: &'s str,
        own: &Bindings,
    ) -> Result<Vec<Change>, Problem> {
        let mut request = Request {
            source,
            name,
            problems: Vec::new(),
            changes: Vec::new(),
        };
        let mut open = None;
        let mut headers = Vec::new();
        let mut kind = None;
        let mut handler: Option<(&Statement<'a>, &Expression<'a>)> = None;
        let mut send = None;
        let mut gathered = Vec::new();
        let mut started = false;
        for statement in &list[made + 1..] {
            let Some(found) = step(statement, name) else {
                if started {
                    request.problem(Xhr::Between, statement.span().start);
                }
                continue;
            };
            started = true;
            let again = match found {
                Step::Open(call) => open.replace(call).is_some(),
                Step::Header(call) => {
                    headers.push(call);
                    false
                }
                Step::Kind(value) => kind.replace(value).is_some(),
                Step::Handler(value) => handler.replace((statement, value)).is_some(),
                Step::Send(call) => send.replace(call).is_some(),
                Step::Other(member, at) => {
                    request.problem(Xhr::Member { name: member }, at);
                    false
                }
            };
            if again {
                request.problem(Xhr::Unfinished, statement.span().start);
            }
            if !found_handler(statement, name) {
                gathered.push(statement);
            }
            if send.is_some() {
                break;
            }
        }
        let made_at = list[made].span().start;
        let (Some(open), Some((handler_statement, handler)), Some(send)) = (open, handler, send)
        else {
            request.problem(Xhr::Unfinished, made_at);
            return Err(request.worst());
        };

        // Where the list's names are read, and what of them.
        let mut uses = Uses {
            name,
            ..Uses::default()
        };
        for statement in list {
            uses.visit_statement(statement);
        }

        // The handler: a function of no parameters that waits for the request.
        let Some((body, arrow)) = handler_body(handler) else {
            request.problem(Xhr::Handler, handler.span().start);
            return Err(request.worst());
        };
        let guard = request.guard(body, own);
        if guard.is_none() {
            // Said where the handler starts, before the reads in it, which
            // are outside a guard only because there is none.
            request.problem(Xhr::Unguarded, handler.span().start);
        }

        // What is sent.
        let mut init = Vec::new();
        let mut method_gets = false;
        match open.arguments.as_slice() {
            [method, url, rest @ ..] if rest.len() <= 1 => {
                match rest.first() {
                    None => {}
                    Some(Argument::BooleanLiteral(yes)) if yes.value => {}
                    Some(Argument::BooleanLiteral(no)) => {
                        request.problem(Xhr::Waits, no.span.start)
                    }
                    Some(other) => request.problem(
                        Xhr::Member {
                            name: "open".into(),
                        },
                        other.span().start,
                    ),
                }
                match method {
                    Argument::StringLiteral(literal) => {
                        let verb = literal.value.to_ascii_uppercase();
                        method_gets = verb == "GET" || verb == "HEAD";
                        if verb != "GET" {
                            let text = request.text(literal.span);
                            init.push(format!("method: {text}"));
                        }
                    }
                    Argument::Identifier(_) | Argument::StaticMemberExpression(_) => {
                        let text = request.text(method.span());
                        init.push(format!("method: {text}"));
                    }
                    other => request.problem(
                        Xhr::Member {
                            name: "open".into(),
                        },
                        other.span().start,
                    ),
                }
                if url.is_spread() {
                    request.problem(
                        Xhr::Member {
                            name: "open".into(),
                        },
                        url.span().start,
                    );
                }
            }
            _ => request.problem(
                Xhr::Member {
                    name: "open".into(),
                },
                open.span.start,
            ),
        }
        let url = match open.arguments.get(1) {
            Some(url) => request.text(url.span()),
            None => String::new(),
        };
        if !headers.is_empty() {
            let mut pairs = Vec::new();
            let mut names = Vec::new();
            let mut keyed = true;
            for call in &headers {
                let [key, value] = call.arguments.as_slice() else {
                    request.problem(
                        Xhr::Member {
                            name: "setRequestHeader".into(),
                        },
                        call.span.start,
                    );
                    continue;
                };
                if key.is_spread() || value.is_spread() {
                    request.problem(
                        Xhr::Member {
                            name: "setRequestHeader".into(),
                        },
                        call.span.start,
                    );
                    continue;
                }
                match key {
                    Argument::StringLiteral(literal)
                        if !names.contains(&literal.value.to_ascii_lowercase()) =>
                    {
                        names.push(literal.value.to_ascii_lowercase());
                    }
                    _ => keyed = false,
                }
                let (key, value) = (request.text(key.span()), request.text(value.span()));
                pairs.push((key, value));
            }
            let written = if keyed {
                let fields: Vec<_> = pairs.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                format!("{{ {} }}", fields.join(", "))
            } else {
                let fields: Vec<_> = pairs.iter().map(|(k, v)| format!("[{k}, {v}]")).collect();
                format!("[{}]", fields.join(", "))
            };
            init.push(format!("headers: {written}"));
        }
        match send.arguments.as_slice() {
            [] => {}
            [Argument::NullLiteral(_)] => {}
            [Argument::Identifier(id)] if id.name == "undefined" => {}
            [_] if method_gets => {}
            [body] if !body.is_spread() => {
                let text = request.text(body.span());
                init.push(format!("body: {text}"));
            }
            _ => request.problem(
                Xhr::Member {
                    name: "send".into(),
                },
                send.span.start,
            ),
        }
        let read = match kind {
            None => Read::Text,
            Some(Expression::StringLiteral(literal)) => match literal.value.as_str() {
                "" | "text" => Read::Text,
                "json" => Read::Json,
                "arraybuffer" => Read::Bytes,
                _ => {
                    request.problem(
                        Xhr::Member {
                            name: "responseType".into(),
                        },
                        literal.span.start,
                    );
                    Read::Text
                }
            },
            Some(other) => {
                request.problem(
                    Xhr::Member {
                        name: "responseType".into(),
                    },
                    other.span().start,
                );
                Read::Text
            }
        };

        // Every other use of the request is a read of the reply, in the
        // handler, past its guard.
        let content = guard.as_ref().map(|g| g.content.clone());
        let gathered_objects: Vec<u32> = gathered
            .iter()
            .chain(std::iter::once(&handler_statement))
            .filter_map(|s| request_object(s, name))
            .collect();
        let mut reads = Vec::new();
        for found in &uses.found {
            if gathered_objects.contains(&found.at.start) {
                continue;
            }
            if guard
                .as_ref()
                .is_some_and(|g| g.test.start <= found.at.start && found.at.end <= g.test.end)
            {
                continue;
            }
            let inside = content
                .as_ref()
                .is_some_and(|c| c.start <= found.at.start && found.at.end <= c.end);
            match (&found.member, inside) {
                (None, _) => request.problem(Xhr::HandedOn, found.at.start),
                (Some(member), _) if member.name == "readyState" => {
                    request.problem(Xhr::ReadyState, member.at.start);
                }
                (Some(member), true) => match (member.name.as_str(), member.called) {
                    ("status" | "statusText", false) => {}
                    ("responseURL", false) => request.change(member.at, "url".into()),
                    ("getResponseHeader", true) => {
                        request.change(member.at, "headers.get".into());
                    }
                    ("responseText", false) if read == Read::Text => reads.push(found.whole),
                    ("response", false) => reads.push(found.whole),
                    (other, _) => request.problem(
                        Xhr::Member {
                            name: other.to_owned(),
                        },
                        member.at.start,
                    ),
                },
                (Some(member), false) => request.problem(
                    Xhr::Member {
                        name: member.name.clone(),
                    },
                    member.at.start,
                ),
            }
        }
        if !arrow {
            for this in &uses.this {
                let in_handler = body.span.start <= this.start && this.end <= body.span.end;
                // A `function` inside the handler has a `this` of its own.
                let own_function = uses.functions.iter().any(|f| {
                    body.span.start < f.start && f.start <= this.start && this.end <= f.end
                });
                if in_handler && !own_function {
                    request.problem(Xhr::This, this.start);
                }
            }
        }
        if !request.problems.is_empty() {
            return Err(request.worst());
        }
        // No guard was said above, so this returns it.
        let Some(guard) = guard else {
            return Err(request.worst());
        };

        // The reply's body, read once: in place where it is read once, at
        // the top of the handler otherwise.
        let mut prelude = String::new();
        let inline = match reads.as_slice() {
            [only] => {
                let nested = uses.arrows_and_functions().any(|f| {
                    body.span.start < f.start && f.start <= only.start && only.end <= f.end
                });
                !nested && !starts_line(source, only.start as usize)
            }
            _ => false,
        };
        if inline {
            request.change(reads[0], format!("(await {})", read.call(name)));
        } else if !reads.is_empty() {
            let around =
                &source[list[0].span().start as usize..list[list.len() - 1].span().end as usize];
            let base = format!("{name}{}", read.suffix());
            let local = (1..)
                .map(|n| {
                    if n == 1 {
                        base.clone()
                    } else {
                        format!("{base}{n}")
                    }
                })
                .find(|candidate| !around.contains(candidate.as_str()))
                .unwrap_or(base);
            prelude = format!(" const {local} = await {};", read.call(name));
            for at in &reads {
                request.change(*at, local.clone());
            }
        }

        // The edits.
        let header = handler_statement.span().start..body.span.start + 1;
        if source[header.start as usize..header.end as usize].contains('\n') {
            request.problem(Xhr::Handler, header.start);
            return Err(request.worst());
        }
        let init = if init.is_empty() {
            String::new()
        } else {
            format!(", {{ {} }}", init.join(", "))
        };
        request.change(
            Span::new(header.start, header.end),
            format!(
                "fetch({url}{init}).catch(() => Response.error()).then(async ({name}: Response) => {{{prelude}"
            ),
        );
        request.change(Span::new(body.span.end - 1, body.span.end), "})".into());
        let made_span = list[made].span();
        request.remove(made_span.start as usize..made_span.end as usize);
        for statement in &gathered {
            let span = statement.span();
            request.remove(span.start as usize..span.end as usize);
        }
        for range in guard.removed.clone() {
            request.remove(range);
        }
        if let Some(dedent) = &guard.dedent {
            request.dedent(dedent, &uses.templates);
        }
        if !request.problems.is_empty() {
            return Err(request.worst());
        }
        let mut changes = request.changes;
        changes.sort_by_key(|c| c.range.start);
        Ok(changes)
    }

    /// The problem the pass reports: the first in the text.
    fn worst(self) -> Problem {
        self.problems
            .into_iter()
            .min_by_key(|(_, at)| *at)
            .expect("a problem was found")
    }

    /// How the handler waits for the request to be done, if it does.
    fn guard(&mut self, body: &FunctionBody<'_>, own: &Bindings) -> Option<Guard> {
        let done = |test: &Expression<'_>, equal: bool| self.is_done(test, equal, own);
        match body.statements.as_slice() {
            // `if (http.readyState === 4) { … }`, alone.
            [Statement::IfStatement(only)]
                if only.alternate.is_none() && done(&only.test, true) =>
            {
                let consequent = only.consequent.span();
                let (removed, content) = match &only.consequent {
                    Statement::BlockStatement(block) => (
                        vec![
                            only.span.start as usize..block.span.start as usize + 1,
                            block.span.end as usize - 1..block.span.end as usize,
                        ],
                        block.span.start + 1..block.span.end - 1,
                    ),
                    _ => (
                        std::iter::once(only.span.start as usize..consequent.start as usize)
                            .collect(),
                        consequent.start..consequent.end,
                    ),
                };
                Some(Guard {
                    test: only.test.span().start..only.test.span().end,
                    removed,
                    dedent: Some(Dedent {
                        guard: only.span.start as usize,
                        content: content.start as usize..content.end as usize,
                    }),
                    content,
                })
            }
            // `if (http.readyState !== 4) return`, first.
            [Statement::IfStatement(first), rest @ ..]
                if first.alternate.is_none()
                    && done(&first.test, false)
                    && returns_nothing(&first.consequent) =>
            {
                let end = rest.last().map_or(first.span.end, |s| s.span().end);
                Some(Guard {
                    test: first.test.span().start..first.test.span().end,
                    removed: std::iter::once(first.span.start as usize..first.span.end as usize)
                        .collect(),
                    dedent: None,
                    content: first.span.end..end,
                })
            }
            _ => None,
        }
    }

    /// Whether `test` says the request is done (`equal`) or is not.
    fn is_done(&self, test: &Expression<'_>, equal: bool, own: &Bindings) -> bool {
        let Expression::BinaryExpression(test) = test.without_parentheses() else {
            return false;
        };
        let operator_fits = if equal {
            matches!(
                test.operator,
                BinaryOperator::Equality | BinaryOperator::StrictEquality
            )
        } else {
            matches!(
                test.operator,
                BinaryOperator::Inequality | BinaryOperator::StrictInequality
            )
        };
        let state = |e: &Expression<'_>| {
            matches!(e.without_parentheses(), Expression::StaticMemberExpression(m)
                if m.property.name == "readyState"
                    && matches!(&m.object, Expression::Identifier(id) if id.name == self.name))
        };
        let four = |e: &Expression<'_>| match e.without_parentheses() {
            Expression::NumericLiteral(n) => n.value == 4.0,
            Expression::StaticMemberExpression(m) => {
                m.property.name == "DONE"
                    && matches!(&m.object, Expression::Identifier(id)
                        if id.name == CLASS && !own.binds(CLASS, id.span.start))
            }
            _ => false,
        };
        operator_fits
            && ((state(&test.left) && four(&test.right))
                || (four(&test.left) && state(&test.right)))
    }

    /// Takes the guard's indentation off each line of `dedent`'s content,
    /// but a line that starts inside a template literal, whose text it is.
    fn dedent(&mut self, dedent: &Dedent, templates: &[Span]) {
        let source = self.source;
        let line_of = |at: usize| source[..at].rfind('\n').map_or(0, |n| n + 1);
        let indent = |line: usize| {
            let rest = &source[line..];
            &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
        };
        let guard_line = line_of(dedent.guard);
        let guard_indent = indent(guard_line);
        // The content's first line after the guard's.
        let Some(first) = source[dedent.content.clone()]
            .find('\n')
            .map(|n| dedent.content.start + n + 1)
        else {
            return;
        };
        if first >= dedent.content.end {
            return;
        }
        let first_indent = indent(first);
        let Some(extra) = first_indent.strip_prefix(guard_indent) else {
            return;
        };
        if extra.is_empty() {
            return;
        }
        let mut line = first;
        while line < dedent.content.end {
            let end = source[line..].find('\n').map_or(source.len(), |n| line + n);
            let in_template = templates
                .iter()
                .any(|t| (t.start as usize) < line && line < t.end as usize);
            let text = &source[line..end];
            if !in_template
                && !text.trim().is_empty()
                && text.starts_with(&format!("{guard_indent}{extra}"))
            {
                let at = line + guard_indent.len();
                self.changes.push(Change {
                    range: at..at + extra.len(),
                    replacement: String::new(),
                });
            }
            line = end + 1;
        }
    }
}

/// Whether the statement sets the request's `onreadystatechange`.
fn found_handler(statement: &Statement<'_>, name: &str) -> bool {
    matches!(step(statement, name), Some(Step::Handler(_)))
}

/// Where the request's name starts in one of its own statements.
fn request_object(statement: &Statement<'_>, name: &str) -> Option<u32> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let object = match statement.expression.without_parentheses() {
        Expression::CallExpression(call) => match call.callee.without_parentheses() {
            Expression::StaticMemberExpression(m) => &m.object,
            _ => return None,
        },
        Expression::AssignmentExpression(a) => match &a.left {
            AssignmentTarget::StaticMemberExpression(m) => &m.object,
            _ => return None,
        },
        _ => return None,
    };
    match object {
        Expression::Identifier(id) if id.name == name => Some(id.span.start),
        _ => None,
    }
}

/// The body of a handler written there with no parameters, and whether it
/// is an arrow function.
fn handler_body<'n, 'a>(handler: &'n Expression<'a>) -> Option<(&'n FunctionBody<'a>, bool)> {
    match handler.without_parentheses() {
        Expression::FunctionExpression(f) => plain_function(f).map(|body| (body, false)),
        Expression::ArrowFunctionExpression(arrow) => plain_arrow(arrow).map(|body| (body, true)),
        _ => None,
    }
}

fn plain_function<'n, 'a>(f: &'n Function<'a>) -> Option<&'n FunctionBody<'a>> {
    let plain = f.params.items.is_empty()
        && f.params.rest.is_none()
        && !f.r#async
        && !f.generator
        && f.this_param.is_none();
    if plain { f.body.as_deref() } else { None }
}

fn plain_arrow<'n, 'a>(arrow: &'n ArrowFunctionExpression<'a>) -> Option<&'n FunctionBody<'a>> {
    let plain = arrow.params.items.is_empty() && arrow.params.rest.is_none() && !arrow.r#async;
    match &arrow.body {
        ArrowFunctionBody::FunctionBody(body) if plain => Some(body),
        _ => None,
    }
}

/// `return`, or `{ return }`.
fn returns_nothing(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ReturnStatement(r) => r.argument.is_none(),
        Statement::BlockStatement(block) => {
            matches!(block.body.as_slice(), [Statement::ReturnStatement(r)] if r.argument.is_none())
        }
        _ => false,
    }
}

/// Whether only blanks are before `at` on its line.
fn starts_line(source: &str, at: usize) -> bool {
    let line = source[..at].rfind('\n').map_or(0, |n| n + 1);
    source[line..at].trim().is_empty()
}

/// How a handler waits for its request.
struct Guard {
    /// The test, whose read of `readyState` is the guard's own.
    test: Range<u32>,
    /// What of the text the guard takes, emptied.
    removed: Vec<Range<usize>>,
    /// What runs once the request is done.
    content: Range<u32>,
    dedent: Option<Dedent>,
}

/// Lines that were inside the guard, which are one level less deep once
/// it is gone.
struct Dedent {
    guard: usize,
    content: Range<usize>,
}

/// A read of the request's name.
struct Found {
    at: Span,
    /// The member read off it, if it is read so.
    member: Option<Member>,
    /// The whole `name.member`, or the name.
    whole: Span,
}

struct Member {
    name: String,
    at: Span,
    called: bool,
}

/// Where a list's statements read the request's name, `this`, and where
/// its functions and template literals are.
#[derive(Default)]
struct Uses<'n> {
    name: &'n str,
    found: Vec<Found>,
    this: Vec<Span>,
    /// The spans of the `function`s, which have a `this` of their own.
    functions: Vec<Span>,
    arrows: Vec<Span>,
    templates: Vec<Span>,
    /// The callee of the call being walked: its whole span, since
    /// `http.responseText.toString()` starts where `http.responseText` does.
    callee: Option<Span>,
}

impl Uses<'_> {
    fn arrows_and_functions(&self) -> impl Iterator<Item = &Span> {
        self.functions.iter().chain(&self.arrows)
    }
}

impl<'a> Visit<'a> for Uses<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::StaticMemberExpression(m) = it.callee.without_parentheses() {
            self.callee = Some(m.span);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Expression::Identifier(id) = &it.object
            && id.name == self.name
        {
            let called = self.callee == Some(it.span);
            self.found.push(Found {
                at: id.span,
                member: Some(Member {
                    name: it.property.name.to_string(),
                    at: it.property.span,
                    called,
                }),
                whole: it.span,
            });
            return;
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.name == self.name {
            self.found.push(Found {
                at: it.span,
                member: None,
                whole: it.span,
            });
        }
    }

    fn visit_this_expression(&mut self, it: &ThisExpression) {
        self.this.push(it.span);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.functions.push(it.span);
        walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.arrows.push(it.span);
        walk::walk_arrow_function_expression(self, it);
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.templates.push(it.span);
        walk::walk_template_literal(self, it);
    }
}
