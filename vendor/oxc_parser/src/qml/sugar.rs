//! `match` and `is`, TC39's pattern matching, and the pipe `|>`, read and
//! written away. The pipe's lowering is in [`pipe`]'s own doc.
//!
//! The parser does not build nodes for them: oxc's tree has none, and a
//! fork of `oxc_ast` would be a second fork to keep. It records *text
//! edits* instead, as it reads. [`lower`] applies them to the source and
//! gives plain TypeScript, line for line, with a [`SpanMap`] from the text it
//! wrote back to the source. Whoever needs a tree parses that text again.
//!
//! An edit replaces a span of the source with pieces: text the desugar
//! writes, which stands at a span of the source for error positions, and
//! copies of spans of the source. A copy is lowered too, so a `match` inside
//! an arm of another is written away when the outer one copies the arm.
//!
//! The lowering, for `match (subject) { when p: e; default: d; }`:
//!
//! ```text
//! (() => { const $s = subject; if (TEST(p, $s)) return e; return d; })()
//! ```
//!
//! The subject is the body's first statement, not a parameter's default,
//! which may hold no `await` or `yield`; a `match` that awaits, in its
//! subject or an arm, is an `async` arrow, awaited. No arrow can `yield`,
//! so a `match` that yields is a generator, delegated to and handed `this`
//! and `arguments`: `(yield* (function* () { … }).apply(this, arguments))`,
//! an `async function*` in an `async` generator. A subject that is a name is tested by
//! name, with no `$s`, so a checker narrows the name itself. An arm whose
//! pattern binds names nests an `if` for each test and a `const` for each
//! name, so a guard after a binding sees it. With no `default`, the arrow
//! ends `return $nomatch(s as never)`. `x is p` is `(TEST(p, x))`, and its
//! bindings are `let`s (a `var` pattern's, `var`s) declared at the start of
//! the statement that holds it, assigned where the test reaches them; in a
//! `switch`'s clauses, which a jump enters at any of them, at the start of
//! the statement that holds the `switch`. `$is`
//! and `$nomatch` are the runtime's (see DIALECT.md §4.18).

use std::fmt::Write as _;

use oxc_ast::ast::{Expression, RegExpFlags};
use oxc_span::{GetSpan, Span};

use crate::{ParserConfig, ParserImpl, context::Context, diagnostics, lexer::Kind};

mod pipe;

const YIELDS: &str = "a `match` or `is` that yields is lowered to a generator, which has no `super` or `new.target` of the function around it: read it into a name before, as in `const base = super.size;`";

/// One piece of an edit's replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Text the desugar writes, which stands at a span of the source.
    Text(String, Span),
    /// A span of the source, itself lowered.
    Copy(Span),
}

impl Piece {
    fn anchor(&self) -> u32 {
        match self {
            Self::Text(_, at) => at.start,
            Self::Copy(span) => span.start,
        }
    }
}

/// `span` of the source is written as `pieces`. A span of length zero
/// inserts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub pieces: Vec<Piece>,
    /// The keyword of the `match`, `is` or `|>` the edit writes away; None for
    /// what an `is` declares at the start of its statement.
    pub keyword: Option<Span>,
}

/// The parser's state for sugar.
#[derive(Debug, Default)]
pub(crate) struct Sugar {
    /// Whether `match` and `is` are read: in a QML document and in a script
    /// given to [`Parser::parse_with_sugar`](crate::Parser::parse_with_sugar).
    pub on: bool,
    pub edits: Vec<Edit>,
    /// The start of the statement-list item being read: where an `is`
    /// declares what it binds.
    pub item: Option<u32>,
    /// The start of a statement that is a QML binding's value, which takes
    /// one statement and so no declaration before it.
    pub binding: Option<u32>,
    /// Where the `await`s and `yield`s the function being read has read so
    /// far are, its own and not those of a function inside it: a `match` or
    /// `is` that holds one is lowered to an awaited `async` arrow, or, for
    /// a `yield`, which no arrow can hold, to a generator delegated to.
    pub awaits: Vec<u32>,
    pub yields: Vec<u32>,
    /// The `super`s and `new.target`s of the function being read, an
    /// arrow's being its function's: a generator has its own, so a `match`
    /// that yields cannot hold one.
    pub supers: Vec<Span>,
    /// Each name an `is` has declared, with how many blocks deep: a block
    /// declares a name once, however many of its statements bind it.
    pub declared: Vec<(usize, String)>,
    /// For each block being read, outermost first, where its names start
    /// in `declared`.
    pub blocks: Vec<usize>,
    /// Where the clauses of the `switch` being read declare what an `is`
    /// binds, and how many blocks deep they are. The clauses are one scope
    /// that a jump enters at any of them, so a `let` in one would be in its
    /// dead zone in the others: it goes before the `switch`, at the item
    /// that holds it, its label too.
    pub switch: Option<(Option<u32>, usize)>,
    /// The topic of the pipe body being read, numbered by how many bodies
    /// deep it is (see `pipe.rs`).
    pub topic: Option<u32>,
    /// Each `%` read, with the topic it is and where it is.
    pub topics: Vec<(u32, u32)>,
    /// The span of the pipe just read and its parts, head first: a pipe
    /// whose body it is merges them into its own.
    pub chain: Option<(Span, Vec<Span>)>,
}

/// A pattern as it was read.
enum Pat {
    /// `===` the literal: a string, a number, `true`, `null`, `undefined`.
    Same(Span),
    /// `$is(s, x)`: a name, whose value is compared or called, and `NaN`,
    /// `+0` and `-0`, which compare by `Object.is`.
    Is(Span),
    /// `String`, `Number`, `Boolean`.
    Type(&'static str, Span),
    Regex(Span),
    /// `let x`, `const x`, `var x`. `var` is kept as written, so that the
    /// rule against it applies; `let` and `const` both lower to `const`,
    /// since a pattern's binding is never written.
    Bind { keyword: Span, var: bool, name: Span },
    Array { span: Span, items: Vec<Pat>, rest: Rest },
    Object { span: Span, props: Vec<Prop> },
    And(Vec<Pat>),
    Or(Vec<Pat>),
    Not(Box<Pat>, Span),
    /// `if (c)`: the span is `(c)`.
    Guard(Span),
    /// What follows an error, which has been reported: matches nothing.
    Never(Span),
}

enum Rest {
    None,
    /// `...`
    Any,
    /// `...let rest`
    Bind(Box<Pat>),
}

struct Prop {
    key: Key,
    /// `None` for `{ key }`, which tests that the key is there.
    pat: Option<Pat>,
}

enum Key {
    /// An identifier name.
    Name(Span),
    /// A string or number literal.
    Literal(Span),
    /// `[expression]`: the span is the expression's.
    Computed(Span),
}

impl Key {
    fn span(&self) -> Span {
        match self {
            Self::Name(span) | Self::Literal(span) | Self::Computed(span) => *span,
        }
    }
}

impl Pat {
    fn span(&self) -> Span {
        match self {
            Self::Same(span)
            | Self::Is(span)
            | Self::Type(_, span)
            | Self::Regex(span)
            | Self::Array { span, .. }
            | Self::Object { span, .. }
            | Self::Not(_, span)
            | Self::Guard(span)
            | Self::Never(span) => *span,
            Self::Bind { keyword, name, .. } => Span::new(keyword.start, name.end),
            Self::And(list) | Self::Or(list) => {
                Span::new(list[0].span().start, list[list.len() - 1].span().end)
            }
        }
    }

    fn binds(&self) -> bool {
        match self {
            Self::Bind { .. } => true,
            Self::Array { items, rest, .. } => {
                items.iter().any(Self::binds) || matches!(rest, Rest::Bind(_))
            }
            Self::Object { props, .. } => props.iter().any(|p| p.pat.as_ref().is_some_and(Self::binds)),
            Self::And(list) | Self::Or(list) => list.iter().any(Self::binds),
            Self::Not(inner, _) => inner.binds(),
            _ => false,
        }
    }
}

/// A step of a pattern taken apart for an arm: tests and bindings in the
/// order the proposal makes them.
enum Step {
    Test(Vec<Piece>),
    Bind { keyword: Span, var: bool, name: Span, value: Vec<Piece> },
}

fn text(s: impl Into<String>, at: Span) -> Piece {
    Piece::Text(s.into(), at)
}

impl<'a, C: ParserConfig> ParserImpl<'a, C> {
    fn sugar_word(&self, word: &str) -> bool {
        self.cur_kind() == Kind::Ident && !self.cur_token().escaped() && self.cur_src() == word
    }

    /// At `match`: whether it starts a `match` expression,
    /// `match [no LineTerminator here] ( Expression ) [no LineTerminator here] {`.
    /// Anything else is the name `match`.
    pub(crate) fn at_sugar_match(&mut self) -> bool {
        self.sugar.on
            && self.sugar_word("match")
            && self.lookahead(|p| {
                p.bump_any();
                if !p.at(Kind::LParen) || p.cur_token().is_on_new_line() {
                    return false;
                }
                p.bump_any();
                p.context_add(Context::In, Self::parse_expr);
                if p.fatal_error.is_some() || !p.at(Kind::RParen) {
                    return false;
                }
                p.bump_any();
                p.at(Kind::LCurly) && !p.cur_token().is_on_new_line()
            })
    }

    /// At `is` after an operand, where a relational operator may stand.
    pub(crate) fn at_sugar_is(&self) -> bool {
        self.sugar.on
            && self.cur_kind() == Kind::Is
            && !self.cur_token().escaped()
            && !self.cur_token().is_on_new_line()
    }

    /// The syntax error at the current token, which stops the parse; the
    /// pattern is never read.
    fn sugar_unexpected(&mut self) -> Pat {
        let span = self.cur_token().span();
        self.set_unexpected();
        Pat::Never(span)
    }

    fn sugar_error(&mut self, message: &'static str, span: Span) {
        self.error(diagnostics::qml(message, span));
    }

    /// Where an `is` read now declares what it binds: the start of the item
    /// that holds it, or, in a `switch`'s clauses, of the one that holds
    /// the `switch`.
    pub(crate) fn sugar_declare_at(&self) -> Option<u32> {
        match self.sugar.switch {
            Some((at, depth)) if depth == self.sugar.blocks.len() => at,
            _ => self.sugar.item,
        }
    }

    /// Reads a `switch`'s clauses, whose `is` bindings are declared before
    /// it, in the block around it.
    pub(crate) fn sugar_switch<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> T {
        let at = (self.sugar_declare_at(), self.sugar.blocks.len());
        let outer = self.sugar.switch.replace(at);
        let out = read(self);
        self.sugar.switch = outer;
        out
    }

    /// Reads a block, whose names an `is` declares once.
    pub(crate) fn sugar_list<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> T {
        self.sugar.blocks.push(self.sugar.declared.len());
        let out = read(self);
        self.sugar.blocks.pop();
        out
    }

    /// The text that opens and the text that closes the function the
    /// `match` or `is` at `span` is lowered to, called at once. It is an
    /// arrow, which shares `this`, `arguments`, `super` and `new.target`
    /// with the function around it; one that awaits is `async`, awaited.
    /// No arrow can `yield`, so one that yields is a generator, `async` in
    /// an `async` one, delegated to: `yield*` passes on what it yields and
    /// what is sent back, and is what it returns, the arm's value. `this`
    /// and `arguments` are handed to it; `super` and `new.target` cannot
    /// be, so one of them in it is an error, and it is `None`.
    fn sugar_function(
        &mut self,
        span: Span,
        yields: &'static str,
    ) -> Option<(&'static str, &'static str)> {
        let within = |at: &u32| span.start <= *at && *at < span.end;
        if !self.sugar.yields.iter().any(within) {
            return Some(if self.sugar.awaits.iter().any(within) {
                ("(await (async () => {", "})())")
            } else {
                ("(() => {", "})()")
            });
        }
        if let Some(at) = self.sugar.supers.iter().copied().find(|at| within(&at.start)) {
            self.sugar_error(yields, at);
            return None;
        }
        Some(if self.ctx.has_await() {
            ("(yield* (async function* () {", "}).apply(this, arguments))")
        } else {
            ("(yield* (function* () {", "}).apply(this, arguments))")
        })
    }

    /// What stands for a `match` or `is` that cannot be lowered, whose
    /// error is said: `undefined`, which reads on with no second one.
    fn sugar_unlowered(&mut self, span: Span, keyword: Span) {
        let pieces = vec![text("(undefined)", keyword)];
        self.sugar.edits.push(Edit { span, pieces, keyword: Some(keyword) });
    }

    /// The end of a clause: `;`, which may be left out before `}`.
    fn sugar_clause_end(&mut self) -> Span {
        if self.at(Kind::Semicolon) {
            let span = self.cur_token().span();
            self.bump_any();
            span
        } else {
            if !self.at(Kind::RCurly) {
                self.expect(Kind::Semicolon);
            }
            Span::empty(self.prev_token_end)
        }
    }

    /// `match (subject) { when pattern: value; … default: value; }`. The
    /// tree gets the subject, in parentheses that span the whole `match`;
    /// what it means is in the edit.
    pub(crate) fn parse_sugar_match(&mut self) -> Expression<'a> {
        let start = self.cur_start();
        let keyword = self.cur_token().span();
        self.bump_any(); // `match`
        self.expect(Kind::LParen);
        let subject = self.context_add(Context::In, Self::parse_expr);
        self.expect(Kind::RParen);
        let open = self.cur_token().span();
        self.expect(Kind::LCurly);

        let bare = matches!(subject, Expression::Identifier(_));
        let subject_span = subject.span();
        let path = if bare {
            vec![Piece::Copy(subject_span)]
        } else {
            vec![text("$s", keyword)]
        };

        let mut body = Vec::new();
        let mut has_default = false;
        while !self.at(Kind::RCurly) && !self.has_fatal_error() {
            if has_default {
                let span = self.cur_token().span();
                self.sugar_error("`default:` is the last arm of a `match`", span);
            }
            if self.at(Kind::Default) {
                let default = self.cur_token().span();
                self.bump_any();
                self.expect(Kind::Colon);
                let value = self.context_add(Context::In, Self::parse_assignment_expression_or_higher);
                let end = self.sugar_clause_end();
                body.push(text(" return ", default));
                body.push(Piece::Copy(value.span()));
                body.push(text(";", end));
                has_default = true;
            } else if self.sugar_word("when") {
                let when = self.cur_token().span();
                self.bump_any();
                let pat = self.parse_sugar_pattern();
                let colon = self.cur_token().span();
                self.expect(Kind::Colon);
                let value = self.context_add(Context::In, Self::parse_assignment_expression_or_higher);
                let end = self.sugar_clause_end();
                let mut steps = Vec::new();
                self.sugar_steps(&pat, &path, &mut steps);
                sugar_arm(&mut body, &steps, when, colon, value.span(), end);
            } else {
                return self.unexpected();
            }
        }
        let close = self.cur_token().span();
        self.expect(Kind::RCurly);
        let span = self.end_span(start);

        let Some((open_text, close_text)) = self.sugar_function(span, YIELDS) else {
            self.sugar_unlowered(span, keyword);
            return Expression::new_parenthesized_expression(span, subject, self);
        };
        let mut pieces = Vec::new();
        if self.sugar.item == Some(start) {
            // A statement that starts with `(` would call the line before it.
            pieces.push(text(";", Span::empty(start)));
        }
        pieces.push(text(open_text, keyword));
        if !bare {
            // The subject is the first statement of the function's body,
            // where an `await` or `yield` in it is the function's own and
            // it is read where it is written; a parameter's default may
            // hold neither.
            pieces.push(text(" const $s = ", keyword));
            pieces.push(Piece::Copy(subject_span));
            pieces.push(text(";", open));
        }
        pieces.extend(body);
        if !has_default {
            pieces.push(text(" return $nomatch(", keyword));
            // At the subject, for a checker that names what is not covered.
            if bare {
                pieces.push(Piece::Copy(subject_span));
            } else {
                pieces.push(text("$s", subject_span));
            }
            pieces.push(text(" as never);", keyword));
        }
        pieces.push(text(close_text, close));
        self.sugar.edits.push(Edit { span, pieces, keyword: Some(keyword) });

        Expression::new_parenthesized_expression(span, subject, self)
    }

    /// `lhs is pattern`, after `is`'s precedence was checked.
    pub(crate) fn parse_sugar_is(&mut self, lhs_start: u32, lhs: Expression<'a>) -> Expression<'a> {
        let keyword = self.cur_token().span();
        self.bump_any(); // `is`
        let pat = self.parse_sugar_pattern();
        let span = self.end_span(lhs_start);
        let bare = matches!(lhs, Expression::Identifier(_));
        let path = if bare { vec![Piece::Copy(lhs.span())] } else { vec![text("$s", keyword)] };

        let mut names = Vec::new();
        if pat.binds() {
            if !bare {
                self.sugar_error(
                    "an `is` that binds a name tests a name: give the value one first, as in `const r = …; if (r is { let x })`",
                    pat.span(),
                );
            } else if self.sugar_declare_at().is_none_or(|at| Some(at) == self.sugar.binding) {
                self.sugar_error(
                    "an `is` that binds a name is part of a statement, where what it binds is declared: test with `match` here",
                    pat.span(),
                );
            }
        }
        let mut steps = Vec::new();
        self.sugar_steps(&pat, &path, &mut steps);
        let mut test = Vec::new();
        for step in steps {
            if !test.is_empty() {
                test.push(text(" && ", keyword));
            }
            match step {
                Step::Test(pieces) => test.extend(pieces),
                Step::Bind { keyword: written, var, name, value } => {
                    names.push((name, var, written));
                    test.push(text("((", name));
                    test.push(Piece::Copy(name));
                    test.push(text(" = ", name));
                    test.extend(value);
                    test.push(text("), true)", name));
                }
            }
        }
        if test.is_empty() {
            test.push(text("true", keyword));
        }

        let mut pieces = Vec::new();
        if self.sugar.item == Some(lhs_start) {
            pieces.push(text(";", Span::empty(lhs_start)));
        }
        if bare {
            pieces.push(text("(", keyword));
            pieces.extend(test);
            pieces.push(text(")", keyword));
        } else {
            // As a `match`'s subject: the left side is read in the
            // function's body, where it may await or yield.
            let Some((open_text, close_text)) = self.sugar_function(span, YIELDS) else {
                self.sugar_unlowered(span, keyword);
                return Expression::new_parenthesized_expression(span, lhs, self);
            };
            pieces.push(text(open_text, keyword));
            pieces.push(text(" const $s = ", keyword));
            pieces.push(Piece::Copy(lhs.span()));
            pieces.push(text("; return ", keyword));
            pieces.extend(test);
            pieces.push(text(close_text, keyword));
        }
        if !names.is_empty()
            && bare
            && let Some(item) = self.sugar_declare_at()
            && Some(item) != self.sugar.binding
        {
            // A `let` or `const` of the pattern is a `let` of the block
            // around the statement, assigned where the test reaches it, and
            // declared once however many `is`es of the block bind it; one
            // the block declares itself is the checker's to say. A `var`
            // stays the function's.
            let depth = self.sugar.blocks.len();
            let from = self.sugar.blocks.last().copied().unwrap_or(0).min(self.sugar.declared.len());
            let mut pieces = Vec::new();
            for var in [false, true] {
                let mut list: Vec<(&str, Span)> = Vec::new();
                let mut at = Span::empty(item);
                for (span, _, keyword) in names.iter().filter(|(_, v, _)| *v == var) {
                    let name = &self.source_text[span.start as usize..span.end as usize];
                    let seen = |(d, n): &(usize, String)| *d == depth && n == name;
                    if self.sugar.declared[from..].iter().any(seen)
                        || list.iter().any(|(n, _)| *n == name)
                    {
                        continue;
                    }
                    if list.is_empty() && var {
                        at = *keyword;
                    }
                    list.push((name, *span));
                }
                if list.is_empty() {
                    continue;
                }
                // Each name is written at the name in the pattern, where a
                // finding about it belongs; a `var` is the pattern's own, so
                // the rule against it is said as for one written.
                if var {
                    pieces.push(Piece::Copy(at));
                    pieces.push(text(" ", at));
                } else {
                    pieces.push(text("let ", at));
                }
                for (i, (name, span)) in list.iter().enumerate() {
                    if i > 0 {
                        pieces.push(text(", ", at));
                    }
                    pieces.push(text(*name, *span));
                }
                pieces.push(text("; ", at));
                let declared: Vec<_> =
                    list.iter().map(|(name, _)| (depth, (*name).to_string())).collect();
                self.sugar.declared.extend(declared);
            }
            if !pieces.is_empty() {
                let at = Span::empty(item);
                self.sugar.edits.push(Edit { span: at, pieces, keyword: None });
            }
        }
        self.sugar.edits.push(Edit { span, pieces, keyword: Some(keyword) });
        Expression::new_parenthesized_expression(span, lhs, self)
    }

    /// `pattern`: `and` and `or` of unary patterns, not mixed.
    fn parse_sugar_pattern(&mut self) -> Pat {
        let first = self.parse_sugar_unary();
        let mut list = vec![first];
        let mut joined: Option<&'static str> = None;
        loop {
            let word = if self.sugar_word("and") {
                "and"
            } else if self.sugar_word("or") {
                "or"
            } else {
                break;
            };
            let op = self.cur_token().span();
            if joined.is_some_and(|j| j != word) {
                self.sugar_error(
                    "`and` and `or` mixed need parentheses, as in `(a and b) or c`",
                    op,
                );
            }
            joined = Some(word);
            self.bump_any();
            list.push(self.parse_sugar_unary());
        }
        match joined {
            None => list.pop().unwrap(),
            Some("and") => Pat::And(list),
            Some(_) => {
                if let Some(bound) = list.iter().find(|p| p.binds()) {
                    let span = bound.span();
                    self.sugar_error(
                        "a name bound under `or` may not be bound at all: bind it outside, as in `when let x and (1 or 2):`",
                        span,
                    );
                    return Pat::Never(span);
                }
                Pat::Or(list)
            }
        }
    }

    fn parse_sugar_unary(&mut self) -> Pat {
        if self.sugar_word("not") {
            let start = self.cur_start();
            self.bump_any();
            let inner = self.parse_sugar_unary();
            let span = self.end_span(start);
            if inner.binds() {
                self.sugar_error(
                    "a name bound under `not` is never bound: bind it outside, as in `when let x and not 0:`",
                    inner.span(),
                );
                return Pat::Never(span);
            }
            return Pat::Not(Box::new(inner), span);
        }
        self.parse_sugar_primary()
    }

    fn parse_sugar_primary(&mut self) -> Pat {
        let span = self.cur_token().span();
        match self.cur_kind() {
            Kind::LParen => {
                self.bump_any();
                let inner = self.parse_sugar_pattern();
                self.expect(Kind::RParen);
                inner
            }
            Kind::If => {
                self.bump_any();
                let open = self.cur_start();
                self.expect(Kind::LParen);
                self.context_add(Context::In, Self::parse_expr);
                self.expect(Kind::RParen);
                Pat::Guard(self.end_span(open))
            }
            Kind::Let | Kind::Const | Kind::Var => {
                let var = self.at(Kind::Var);
                self.bump_any();
                let name = self.parse_binding_identifier().span;
                Pat::Bind { keyword: span, var, name }
            }
            Kind::LBrack => self.parse_sugar_array(),
            Kind::LCurly => self.parse_sugar_object(),
            Kind::Slash | Kind::SlashEq => {
                let regex = self.parse_literal_regexp();
                if regex.regex.flags.intersects(RegExpFlags::G | RegExpFlags::Y) {
                    self.sugar_error(
                        "a regular expression with the `g` or `y` flag keeps its place between tests: leave the flag out",
                        regex.span,
                    );
                    return Pat::Never(regex.span);
                }
                Pat::Regex(regex.span)
            }
            Kind::Minus | Kind::Plus => {
                self.bump_any();
                if !self.cur_kind().is_number() {
                    return self.sugar_unexpected();
                }
                let zero = self.parse_literal_number().value == 0.0;
                let span = self.end_span(span.start);
                // `-0` and `+0` match only themselves, by `Object.is`.
                if zero { Pat::Is(span) } else { Pat::Same(span) }
            }
            Kind::NoSubstitutionTemplate => {
                self.parse_template_literal_expression(false);
                Pat::Same(self.end_span(span.start))
            }
            Kind::Undefined if !self.cur_token().escaped() && !self.sugar_path_follows() => {
                self.bump_any();
                Pat::Same(span)
            }
            kind if kind.is_literal() => {
                self.parse_literal_expression();
                Pat::Same(self.end_span(span.start))
            }
            Kind::LAngle | Kind::RAngle | Kind::LtEq | Kind::GtEq | Kind::Eq2 | Kind::Eq3
            | Kind::Neq | Kind::Neq2 => {
                self.sugar_error(
                    "a relational pattern is not in the proposal: write a guard, as in `when let n and if (n < 10):`",
                    span,
                );
                self.bump_any();
                let operand = self.parse_unary_expression_or_higher(self.cur_start());
                Pat::Never(Span::new(span.start, operand.span().end))
            }
            Kind::This => self.parse_sugar_path(),
            kind if kind.is_identifier_reference(false, false) => {
                let word = self.cur_src();
                if !self.cur_token().escaped() && !self.sugar_path_follows() {
                    let type_test = match word {
                        "String" => Some("string"),
                        "Number" => Some("number"),
                        "Boolean" => Some("boolean"),
                        _ => None,
                    };
                    if let Some(ty) = type_test {
                        self.bump_any();
                        return Pat::Type(ty, span);
                    }
                    if word == "NaN" {
                        self.bump_any();
                        return Pat::Is(span);
                    }
                }
                self.parse_sugar_path()
            }
            _ => self.sugar_unexpected(),
        }
    }

    /// Whether `.` or `[` follows the current token, on the same line for `[`.
    fn sugar_path_follows(&mut self) -> bool {
        self.lookahead(|p| {
            p.bump_any();
            p.at(Kind::Dot) || (p.at(Kind::LBrack) && !p.cur_token().is_on_new_line())
        })
    }

    /// A variable pattern: `name`, `a.b`, `a[b]`. One followed by `(` is an
    /// extractor, which Ljós leaves out.
    fn parse_sugar_path(&mut self) -> Pat {
        let start = self.cur_start();
        if self.at(Kind::This) {
            self.bump_any();
        } else {
            self.parse_identifier_reference();
        }
        loop {
            if self.at(Kind::Dot) {
                self.bump_any();
                self.parse_identifier_name();
            } else if self.at(Kind::LBrack) && !self.cur_token().is_on_new_line() {
                self.bump_any();
                self.context_add(Context::In, Self::parse_expr);
                self.expect(Kind::RBrack);
            } else {
                break;
            }
        }
        let span = self.end_span(start);
        if self.at(Kind::LParen) {
            self.sugar_error(
                "an extractor is not in Ljós's `match`: test the value with a predicate, as in `when let x and isSome:`",
                span,
            );
            self.sugar_skip_group();
            return Pat::Never(self.end_span(start));
        }
        Pat::Is(span)
    }

    /// Past a bracketed group whose contents are not read: an extractor's
    /// arguments, after its error.
    fn sugar_skip_group(&mut self) {
        let mut depth = 0u32;
        loop {
            match self.cur_kind() {
                Kind::LParen | Kind::LBrack | Kind::LCurly => depth += 1,
                Kind::RParen | Kind::RBrack | Kind::RCurly => depth = depth.saturating_sub(1),
                Kind::Eof => return,
                _ => {}
            }
            self.bump_any();
            if depth == 0 {
                return;
            }
        }
    }

    fn parse_sugar_array(&mut self) -> Pat {
        let start = self.cur_start();
        self.bump_any(); // `[`
        let mut items = Vec::new();
        let mut rest = Rest::None;
        while !self.at(Kind::RBrack) && !self.has_fatal_error() {
            if self.at(Kind::Dot3) {
                self.bump_any();
                rest = if matches!(self.cur_kind(), Kind::Let | Kind::Const | Kind::Var) {
                    Rest::Bind(Box::new(self.parse_sugar_primary()))
                } else {
                    Rest::Any
                };
                let _ = self.eat(Kind::Comma);
                break;
            }
            items.push(self.parse_sugar_pattern());
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RBrack);
        Pat::Array { span: self.end_span(start), items, rest }
    }

    fn parse_sugar_object(&mut self) -> Pat {
        let start = self.cur_start();
        self.bump_any(); // `{`
        let mut props = Vec::new();
        while !self.at(Kind::RCurly) && !self.has_fatal_error() {
            let span = self.cur_token().span();
            match self.cur_kind() {
                Kind::Dot3 => {
                    self.sugar_error(
                        "a rest in an object pattern is not in Ljós's `match`: read the keys you need",
                        span,
                    );
                    self.bump_any();
                    if matches!(self.cur_kind(), Kind::Let | Kind::Const | Kind::Var) {
                        self.bump_any();
                    }
                    self.bump_any();
                }
                Kind::Let | Kind::Const | Kind::Var => {
                    let bind = self.parse_sugar_primary();
                    let Pat::Bind { name, .. } = bind else { unreachable!() };
                    props.push(Prop { key: Key::Name(name), pat: Some(bind) });
                }
                _ => {
                    let key = if self.at(Kind::LBrack) {
                        self.bump_any();
                        let expression = self.context_add(Context::In, Self::parse_assignment_expression_or_higher);
                        self.expect(Kind::RBrack);
                        Key::Computed(expression.span())
                    } else if self.at(Kind::Str) || self.cur_kind().is_number() {
                        self.parse_literal_expression();
                        Key::Literal(span)
                    } else if self.cur_kind().is_identifier_name() {
                        self.parse_identifier_name();
                        Key::Name(span)
                    } else {
                        return self.sugar_unexpected();
                    };
                    let pat = if self.eat(Kind::Colon) {
                        Some(self.parse_sugar_pattern())
                    } else {
                        if !matches!(key, Key::Name(_)) {
                            self.expect(Kind::Colon);
                        }
                        None
                    };
                    props.push(Prop { key, pat });
                }
            }
            if !self.eat(Kind::Comma) {
                break;
            }
        }
        self.expect(Kind::RCurly);
        Pat::Object { span: self.end_span(start), props }
    }

    fn src(&self, span: Span) -> &'a str {
        &self.source_text[span.start as usize..span.end as usize]
    }

    /// `path` with `key` read off it.
    fn sugar_member(&self, path: &[Piece], key: &Key) -> Vec<Piece> {
        let mut out = path.to_vec();
        match key {
            Key::Name(span) => out.push(text(format!(".{}", self.src(*span)), *span)),
            Key::Literal(span) | Key::Computed(span) => {
                out.push(text("[", *span));
                out.push(Piece::Copy(*span));
                out.push(text("]", *span));
            }
        }
        out
    }

    /// The test that `key` is in `path`'s object.
    fn sugar_has(&self, path: &[Piece], key: &Key) -> Vec<Piece> {
        let mut out = Vec::new();
        match key {
            Key::Name(span) => out.push(text(format!("\"{}\"", self.src(*span)), *span)),
            Key::Literal(span) => out.push(Piece::Copy(*span)),
            Key::Computed(span) => {
                out.push(text("(", *span));
                out.push(Piece::Copy(*span));
                out.push(text(")", *span));
            }
        }
        out.push(text(" in ", key.span()));
        out.extend(path.iter().cloned());
        out
    }

    /// `pat` taken apart at `path` into tests and bindings, in order.
    fn sugar_steps(&self, pat: &Pat, path: &[Piece], out: &mut Vec<Step>) {
        match pat {
            Pat::And(list) => {
                for pat in list {
                    self.sugar_steps(pat, path, out);
                }
            }
            Pat::Bind { keyword, var, name } => {
                out.push(Step::Bind { keyword: *keyword, var: *var, name: *name, value: path.to_vec() });
            }
            Pat::Array { span, items, rest } => {
                out.push(Step::Test(self.sugar_array_shape(path, *span, items.len(), rest)));
                for (i, item) in items.iter().enumerate() {
                    let mut at = path.to_vec();
                    at.push(text(format!("[{i}]"), item.span()));
                    self.sugar_steps(item, &at, out);
                }
                if let Rest::Bind(bind) = rest {
                    let mut at = path.to_vec();
                    at.push(text(format!(".slice({})", items.len()), bind.span()));
                    self.sugar_steps(bind, &at, out);
                }
            }
            Pat::Object { span, props } => {
                out.push(Step::Test(sugar_object_shape(path, *span)));
                for prop in props {
                    out.push(Step::Test(self.sugar_has(path, &prop.key)));
                    if let Some(pat) = &prop.pat {
                        let at = self.sugar_member(path, &prop.key);
                        self.sugar_steps(pat, &at, out);
                    }
                }
            }
            _ => out.push(Step::Test(self.sugar_test(pat, path))),
        }
    }

    fn sugar_array_shape(&self, path: &[Piece], span: Span, len: usize, rest: &Rest) -> Vec<Piece> {
        let mut out = vec![text("Array.isArray(", span)];
        out.extend(path.iter().cloned());
        out.push(text(") && ", span));
        out.extend(path.iter().cloned());
        let op = if matches!(rest, Rest::None) { "===" } else { ">=" };
        out.push(text(format!(".length {op} {len}"), span));
        out
    }

    /// `pat`'s test at `path` as one expression, for a pattern that binds
    /// nothing (or binds in an `is`, through `sugar_steps`).
    fn sugar_test(&self, pat: &Pat, path: &[Piece]) -> Vec<Piece> {
        let mut out = Vec::new();
        match pat {
            Pat::Same(span) => {
                out.extend(path.iter().cloned());
                out.push(text(" === ", *span));
                out.push(Piece::Copy(*span));
            }
            Pat::Is(span) => {
                out.push(text("$is(", *span));
                out.extend(path.iter().cloned());
                out.push(text(", ", *span));
                out.push(Piece::Copy(*span));
                out.push(text(")", *span));
            }
            Pat::Type(ty, span) => {
                out.push(text("typeof ", *span));
                out.extend(path.iter().cloned());
                out.push(text(format!(" === \"{ty}\""), *span));
            }
            Pat::Regex(span) => {
                out.push(text("(typeof ", *span));
                out.extend(path.iter().cloned());
                out.push(text(" === \"string\" && ", *span));
                out.push(Piece::Copy(*span));
                out.push(text(".test(", *span));
                out.extend(path.iter().cloned());
                out.push(text("))", *span));
            }
            Pat::Guard(span) => out.push(Piece::Copy(*span)),
            Pat::Never(span) => out.push(text("false", *span)),
            Pat::Not(inner, span) => {
                out.push(text("!(", *span));
                out.extend(self.sugar_test(inner, path));
                out.push(text(")", *span));
            }
            Pat::Or(list) => {
                let span = pat.span();
                out.push(text("(", span));
                for (i, pat) in list.iter().enumerate() {
                    if i > 0 {
                        out.push(text(" || ", pat.span()));
                    }
                    out.extend(self.sugar_test(pat, path));
                }
                out.push(text(")", span));
            }
            Pat::And(_) | Pat::Array { .. } | Pat::Object { .. } | Pat::Bind { .. } => {
                // A pattern under `or` or `not` binds nothing (that is an
                // error), so its steps are all tests.
                let span = pat.span();
                let mut steps = Vec::new();
                self.sugar_steps(pat, path, &mut steps);
                out.push(text("(", span));
                for (i, step) in steps.into_iter().enumerate() {
                    if i > 0 {
                        out.push(text(" && ", span));
                    }
                    match step {
                        Step::Test(pieces) => out.extend(pieces),
                        Step::Bind { .. } => out.push(text("true", span)),
                    }
                }
                out.push(text(")", span));
            }
        }
        out
    }
}

fn sugar_object_shape(path: &[Piece], span: Span) -> Vec<Piece> {
    let mut out = vec![text("typeof ", span)];
    out.extend(path.iter().cloned());
    out.push(text(" === \"object\" && ", span));
    out.extend(path.iter().cloned());
    out.push(text(" !== null", span));
    out
}

/// One arm, `when pattern: value;`, as `if`s that return the value.
fn sugar_arm(out: &mut Vec<Piece>, steps: &[Step], when: Span, colon: Span, value: Span, end: Span) {
    let binds = steps.iter().any(|s| matches!(s, Step::Bind { .. }));
    if !binds {
        out.push(text(" if (", when));
        join_tests(out, steps.iter(), colon);
        out.push(text(") return ", colon));
        out.push(Piece::Copy(value));
        out.push(text(";", end));
        return;
    }
    out.push(text(" {", when));
    let mut opened = 0;
    let mut i = 0;
    while i < steps.len() {
        match &steps[i] {
            Step::Test(_) => {
                let from = i;
                while i < steps.len() && matches!(steps[i], Step::Test(_)) {
                    i += 1;
                }
                out.push(text(" if (", colon));
                join_tests(out, steps[from..i].iter(), colon);
                out.push(text(") {", colon));
                opened += 1;
            }
            Step::Bind { keyword, var, name, value } => {
                if *var {
                    out.push(text(" ", *keyword));
                    out.push(Piece::Copy(*keyword));
                    out.push(text(" ", *keyword));
                } else {
                    out.push(text(" const ", *keyword));
                }
                out.push(Piece::Copy(*name));
                out.push(text(" = ", *name));
                out.extend(value.iter().cloned());
                out.push(text(";", *name));
                i += 1;
            }
        }
    }
    out.push(text(" return ", colon));
    out.push(Piece::Copy(value));
    let mut close = String::from(";");
    for _ in 0..opened {
        close.push_str(" }");
    }
    close.push_str(" }");
    out.push(text(close, end));
}

fn join_tests<'s>(out: &mut Vec<Piece>, steps: impl Iterator<Item = &'s Step>, at: Span) {
    let mut first = true;
    for step in steps {
        if let Step::Test(pieces) = step {
            if !first {
                out.push(text(" && ", at));
            }
            first = false;
            out.extend(pieces.iter().cloned());
        }
    }
    if first {
        out.push(text("true", at));
    }
}

/// Where a stretch of the lowered text comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    /// Start in the lowered text.
    out: u32,
    len: u32,
    from: From,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum From {
    /// Copied from the source, starting here.
    Copy(u32),
    /// Written by the desugar, standing at this span of the source.
    Written(Span),
}

/// From offsets in the lowered text to offsets in the source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanMap {
    segments: Vec<Segment>,
    out_len: u32,
    source_len: u32,
}

impl SpanMap {
    /// The map of a text the desugar did not change.
    pub fn identity(len: u32) -> Self {
        Self {
            segments: vec![Segment { out: 0, len, from: From::Copy(0) }],
            out_len: len,
            source_len: len,
        }
    }

    /// Whether the lowered text is the source.
    pub fn is_identity(&self) -> bool {
        self.out_len == self.source_len
            && self.segments.iter().all(|s| s.from == From::Copy(s.out))
    }

    fn at(&self, pos: u32) -> Option<&Segment> {
        let i = self.segments.partition_point(|s| s.out <= pos);
        let segment = self.segments.get(i.checked_sub(1)?)?;
        (pos < segment.out + segment.len).then_some(segment)
    }

    /// The source offset where what starts at `pos` of the lowered text
    /// starts.
    pub fn start(&self, pos: u32) -> u32 {
        match self.at(pos) {
            Some(Segment { out, from: From::Copy(from), .. }) => from + (pos - out),
            Some(Segment { from: From::Written(at), .. }) => at.start,
            None => self.source_len,
        }
    }

    /// The source offset where what ends at `pos` of the lowered text ends.
    pub fn end(&self, pos: u32) -> u32 {
        if pos == 0 {
            return 0;
        }
        match self.at(pos - 1) {
            Some(Segment { out, from: From::Copy(from), .. }) => from + (pos - out),
            Some(Segment { from: From::Written(at), .. }) => at.end,
            None => self.source_len,
        }
    }

    /// The span of the source that `span` of the lowered text stands for.
    pub fn span(&self, span: Span) -> Span {
        let start = self.start(span.start);
        if span.is_empty() {
            return Span::empty(start);
        }
        Span::new(start, self.end(span.end).max(start))
    }

    /// Whether the desugar wrote the text at `pos`, rather than copied it.
    pub fn is_written(&self, pos: u32) -> bool {
        matches!(self.at(pos), Some(Segment { from: From::Written(_), .. }))
    }
}

/// The source with `edits` applied, and the map back.
pub fn lower(source: &str, edits: &[Edit]) -> (String, SpanMap) {
    let mut order: Vec<usize> = (0..edits.len()).collect();
    // Outer before inner where two start together, and an insertion first.
    order.sort_by_key(|&i| {
        let span = edits[i].span;
        (span.start, !span.is_empty(), std::cmp::Reverse(span.end))
    });
    let mut out = Lowering {
        source,
        edits,
        order,
        done: vec![false; edits.len()],
        text: String::with_capacity(source.len() + source.len() / 4),
        segments: Vec::new(),
    };
    out.range(0, source.len() as u32);
    let map = SpanMap {
        out_len: out.text.len() as u32,
        source_len: source.len() as u32,
        segments: out.segments,
    };
    (out.text, map)
}

struct Lowering<'s> {
    source: &'s str,
    edits: &'s [Edit],
    /// Indices of `edits`, sorted by start.
    order: Vec<usize>,
    done: Vec<bool>,
    text: String,
    segments: Vec<Segment>,
}

impl Lowering<'_> {
    fn copy(&mut self, start: u32, end: u32) {
        if start >= end {
            return;
        }
        let out = self.text.len() as u32;
        self.text.push_str(&self.source[start as usize..end as usize]);
        self.push(Segment { out, len: end - start, from: From::Copy(start) });
    }

    fn write(&mut self, s: &str, at: Span) {
        if s.is_empty() {
            return;
        }
        let out = self.text.len() as u32;
        self.text.push_str(s);
        self.push(Segment { out, len: s.len() as u32, from: From::Written(at) });
    }

    fn push(&mut self, segment: Segment) {
        if let Some(last) = self.segments.last_mut()
            && last.out + last.len == segment.out
        {
            match (last.from, segment.from) {
                (From::Copy(a), From::Copy(b)) if a + last.len == b => {
                    last.len += segment.len;
                    return;
                }
                (From::Written(a), From::Written(b)) if a == b => {
                    last.len += segment.len;
                    return;
                }
                _ => {}
            }
        }
        self.segments.push(segment);
    }

    /// The source from `lo` to `hi`, with the edits in it applied.
    fn range(&mut self, lo: u32, hi: u32) {
        let mut pos = lo;
        let first = self.order.partition_point(|&i| self.edits[i].span.start < lo);
        let mut k = first;
        while k < self.order.len() {
            let i = self.order[k];
            k += 1;
            let span = self.edits[i].span;
            if span.start > hi || (span.start == hi && !span.is_empty()) {
                break;
            }
            if self.done[i] || span.start < pos || span.end > hi {
                continue;
            }
            self.done[i] = true;
            self.copy(pos, span.start);
            self.edit(i);
            pos = span.end;
        }
        self.copy(pos, hi);
    }

    fn edit(&mut self, i: usize) {
        let edit = &self.edits[i];
        let span = edit.span;
        // The lowered text keeps the source's lines: every line break the
        // edit replaces is written where the next piece of a later line is.
        let mut last = span.start;
        for piece in &edit.pieces {
            let anchor = piece.anchor();
            if anchor > last {
                self.breaks(last, anchor.min(span.end));
                last = anchor;
            }
            match piece {
                Piece::Text(s, at) => self.write(s, *at),
                Piece::Copy(copied) => {
                    self.range(copied.start, copied.end);
                    last = last.max(copied.end);
                }
            }
        }
        if span.end > last {
            self.breaks(last, span.end);
        }
    }

    fn breaks(&mut self, from: u32, to: u32) {
        // A piece may stand for text outside its edit, as what an `is`
        // declares is written at its names: no line of the edit is past it.
        if from >= to {
            return;
        }
        let skipped = &self.source[from as usize..to as usize];
        for (i, _) in skipped.match_indices('\n') {
            let at = from + i as u32;
            self.write("\n", Span::new(at, at + 1));
        }
    }
}

/// `edits` as text, one per line, for tests.
pub fn describe(edits: &[Edit]) -> String {
    let mut s = String::new();
    for edit in edits {
        let _ = write!(s, "{}..{}:", edit.span.start, edit.span.end);
        for piece in &edit.pieces {
            match piece {
                Piece::Text(t, _) => {
                    let _ = write!(s, " {t:?}");
                }
                Piece::Copy(span) => {
                    let _ = write!(s, " [{}..{}]", span.start, span.end);
                }
            }
        }
        s.push('\n');
    }
    s
}
