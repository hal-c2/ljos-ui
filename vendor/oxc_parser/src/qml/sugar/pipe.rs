//! The pipe, `head |> body`, TC39's Hack pipe with `%` as the topic,
//! written away as `match` and `is` are.
//!
//! The proposal nests to the right: `a |> f(%) |> g(%)` is
//! `a |> (f(%) |> g(%))`, where the inner head reads the outer topic. A
//! chain is lowered to one arrow, its head the first statement and each
//! step a `const` the next reads:
//!
//! ```text
//! (() => { const $0 = a; const $1 = f($0); return g($1); })()
//! ```
//!
//! The parts keep their order, so the lowered text keeps the source's
//! lines, and one closure stands for the whole chain. Each evaluation has
//! its own `const`s, so a closure in a body keeps the topic it saw. The
//! topics are numbered by how many bodies deep they are, so a pipe inside
//! a body declares a name its head does not read. A chain that awaits is
//! an awaited `async` arrow, and one that yields a generator, as for
//! `match`.

use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

use super::{Edit, Piece, text};
use crate::{ParserConfig, ParserImpl, lexer::Kind};

const YIELDS: &str = "a pipe that yields is lowered to a generator, which has no `super` or `new.target` of the function around it: read it into a name before, as in `const base = super.size;`";

impl<'a, C: ParserConfig> ParserImpl<'a, C> {
    /// At `|>`: a `|` with `>` right after it, which is never JavaScript.
    pub(crate) fn at_sugar_pipe(&self) -> bool {
        self.sugar.on
            && self.at(Kind::Pipe)
            && self.source_text.as_bytes().get(self.cur_token().end() as usize) == Some(&b'>')
    }

    /// `%` where an operand stands: the topic of the body being read.
    pub(crate) fn parse_sugar_topic(&mut self) -> Expression<'a> {
        let span = self.cur_token().span();
        self.bump_any();
        let name = match self.sugar.topic {
            Some(index) => {
                self.sugar.topics.push((index, span.start));
                format!("${index}")
            }
            None => {
                self.sugar_error(
                    "`%` is the topic of a pipe's body, and there is no pipe here: write it after `|>`, as in `x |> f(%)`",
                    span,
                );
                // Reads on with no second error about the name.
                "undefined".to_string()
            }
        };
        self.sugar.edits.push(Edit { span, pieces: vec![text(name, span)], keyword: None });
        Expression::new_identifier(span, self.ident("$topic"), self)
    }

    /// `head |> body`, at `|>`, after a head that is a short-circuit
    /// expression. The body is an assignment expression, so a pipe in it
    /// takes the rest of the chain: it is merged into this one.
    pub(crate) fn parse_sugar_pipe(&mut self, start: u32, head: Expression<'a>) -> Expression<'a> {
        let pipe = Span::sized(self.cur_start(), 2);
        self.bump_any(); // `|`
        // `>` alone: `|>>` is not a pipe.
        self.expect(Kind::RAngle);
        let index = self.sugar.topic.map_or(0, |outer| outer + 1);
        let outer = self.sugar.topic.replace(index);
        let body = self.parse_assignment_expression_or_higher();
        self.sugar.topic = outer;
        let body_span = body.span();
        self.sugar_pipe_body(&body, body_span, index);

        let span = self.end_span(start);
        let mut parts = vec![head.span()];
        match self.sugar.chain.take() {
            // The body is a pipe as written, not in parentheses: its parts
            // follow this one's, and its topics are numbered on from it.
            Some((inner, rest))
                if inner == body_span && self.sugar.edits.last().is_some_and(|e| e.span == inner) =>
            {
                self.sugar.edits.pop();
                parts.extend(rest);
            }
            _ => parts.push(body_span),
        }

        let Some((open_text, close_text)) = self.sugar_function(span, YIELDS) else {
            self.sugar_unlowered(span, pipe);
            return Expression::new_parenthesized_expression(span, head, self);
        };
        let (first, last) = (parts[0], parts[parts.len() - 1]);
        let mut pieces = Vec::new();
        if self.sugar.item == Some(start) {
            // A statement that starts with `(` would call the line before it.
            pieces.push(text(";", Span::empty(start)));
        }
        pieces.push(text(open_text, first));
        for (i, part) in parts[..parts.len() - 1].iter().enumerate() {
            pieces.push(text(format!(" const ${} = ", index + i as u32), *part));
            pieces.push(Piece::Copy(*part));
            pieces.push(text(";", Span::empty(part.end)));
        }
        pieces.push(text(" return ", last));
        pieces.push(Piece::Copy(last));
        pieces.push(text(";", Span::empty(last.end)));
        pieces.push(text(close_text, Span::empty(last.end)));
        self.sugar.edits.push(Edit { span, pieces, keyword: Some(pipe) });
        self.sugar.chain = Some((span, parts));

        Expression::new_parenthesized_expression(span, head, self)
    }

    /// The proposal's early errors for a body: one that would read as part
    /// of the pipe's syntax is written in parentheses, and one that does
    /// not use its topic is a mistake.
    fn sugar_pipe_body(&mut self, body: &Expression<'a>, span: Span, index: u32) {
        let parenthesize = match body {
            Expression::ArrowFunctionExpression(_) => Some(
                "an arrow function as a pipe's body is written in parentheses: `|> (() => %)`",
            ),
            Expression::ConditionalExpression(_) => Some(
                "a conditional as a pipe's body is written in parentheses: `|> (% > 0 ? % : 0)`",
            ),
            Expression::AssignmentExpression(_) => {
                Some("an assignment as a pipe's body is written in parentheses: `|> (total = %)`")
            }
            Expression::YieldExpression(_) => {
                Some("a `yield` as a pipe's body is written in parentheses: `|> (yield %)`")
            }
            _ => None,
        };
        if let Some(message) = parenthesize {
            self.sugar_error(message, span);
            return;
        }
        let used =
            self.sugar.topics.iter().any(|&(at_index, at)| at_index == index && span.contains_inclusive(Span::empty(at)));
        if !used && self.fatal_error.is_none() {
            self.sugar_error("the pipe's body does not use its topic `%`", span);
        }
    }
}
