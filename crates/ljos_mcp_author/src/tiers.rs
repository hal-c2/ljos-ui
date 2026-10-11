//! `binding_tier_explain`: which tier an expression runs in, and why.
//!
//! The tier and the reason are the compiler's own (`ljos_compiler::tier1`,
//! the data behind `ljos check --stats`), read here and never worked out
//! again. Tier 0 is the one thing the compiler does not record: the engine
//! finds a bare property path in the generated function when it loads the
//! file, and [`tier0_exports`] reads the function the same way.
//!
//! A cheaper form is offered only after the compiler has agreed to it: the
//! file is compiled with the form in place, and the form is kept only if
//! the expression then runs in a lower tier (see `rewrite.rs`).

use std::{collections::HashSet, sync::Arc};

use ljos_compiler::tier1::{self, Outcome};
use ljos_ir::{BindingKind, CompiledFile, ExprId, Literal, Members, ObjectDef, TypeRef};
use ljos_mcp_core::{CallCtx, ErrorCode, SurfaceError, ToolHandler, typed};
use qml_front::QtTypeOracle;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Project, rewrite, tools};

/// Expressions in one answer, at most.
const PAGE: usize = 60;
/// What the expressions of one answer may add up to, so the answer stays
/// under the response cap (§19.6) without the cap cutting it.
const BUDGET: usize = 14 * 1024;
/// An expression's text is cut here: a handler can run to pages.
const SOURCE_BYTES: usize = 600;

type Compiler = ljos_compiler::Project<QtTypeOracle>;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TierArgs {
    /// A `.qml` file, relative to the project root.
    file: String,
    /// 1-based line: only what is written on that line.
    line: Option<u32>,
    /// 1-based column on `line`: only the expression there, or the nearest
    /// one on the line.
    column: Option<u32>,
    /// Only this property, handler or function: `width`, `anchors.fill`,
    /// `onClicked`. Put the object's id or type in front when several
    /// objects have it: `label.width`, `Text.width`.
    property: Option<String>,
    /// `next_cursor` of the previous answer.
    cursor: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Kind {
    /// A value that is written out (`width: 200`): nothing is evaluated.
    Assigned,
    /// A property bound to an expression.
    Binding,
    /// `onClicked: ...`: run when the signal is emitted.
    Handler,
    /// `function name() { ... }`.
    Function,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
enum Tier {
    /// A bare property path (`parent.width`): followed without running code.
    #[serde(rename = "0")]
    Zero,
    /// Run by the binding VM: no JavaScript.
    #[serde(rename = "1")]
    One,
    /// Left to the JavaScript engine.
    #[serde(rename = "js")]
    Js,
}

#[derive(Serialize, JsonSchema)]
struct ReasonOut {
    /// The construct that is outside the binding VM's subset, as `ljos
    /// check --stats` groups by it.
    kind: String,
    /// Which one: the statement, the function called. May be empty.
    detail: String,
}

#[derive(Serialize, JsonSchema)]
struct RewriteOut {
    /// What to write instead. It means the same.
    source: String,
    /// The tier the compiler gives the file with `source` in place.
    tier: Tier,
    /// Why the two mean the same.
    note: String,
}

#[derive(Serialize, JsonSchema)]
struct ExprOut {
    /// Where the value starts (1-based; the column counts characters).
    line: u32,
    column: u32,
    /// The type of the object it is written in, as written.
    object: String,
    /// That object's `id`, if it has one.
    id: Option<String>,
    /// The property, handler (`onClicked`) or function it is the value of.
    property: String,
    kind: Kind,
    /// Null for an assigned value, which has no tier.
    tier: Option<Tier>,
    /// The value as written, cut after 600 bytes (then it ends in `…`).
    source: String,
    /// The tier in a sentence.
    why: String,
    /// For tier `js`: what the compiler found outside the subset.
    reason: Option<ReasonOut>,
    /// For tier `js`: a form that means the same and runs in a lower tier,
    /// when one is known. Checked against the compiler before it is offered.
    rewrite: Option<RewriteOut>,
    /// For tier `js` with no rewrite: what an author can do about it.
    advice: Option<String>,
}

/// Counts over the whole file, whatever the answer was narrowed to.
#[derive(Serialize, JsonSchema)]
struct Summary {
    /// Bindings, handlers and functions: `tier0 + tier1 + js`.
    expressions: usize,
    tier0: usize,
    tier1: usize,
    js: usize,
    /// Values that are written out and so have no tier.
    assigned: usize,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct TierOut {
    file: String,
    summary: Summary,
    /// In the order they are written in the file.
    expressions: Vec<ExprOut>,
    /// Set when the file compiled with errors: what did compile is explained.
    note: Option<String>,
    next_cursor: Option<String>,
}

/// Something of the file that has a value: where it is and what it is of.
struct Row {
    /// Where the member starts (its name), for finding it by place.
    from: u32,
    /// The value.
    start: u32,
    end: u32,
    line: u32,
    column: u32,
    object: String,
    id: Option<String>,
    property: String,
    kind: Kind,
    expr: Option<ExprId>,
    translated: bool,
}

impl Row {
    /// `label.width`: how an agent names it when the property alone is
    /// not enough.
    fn qualified(&self) -> String {
        format!(
            "{}.{}",
            self.id.as_deref().unwrap_or(&self.object),
            self.property
        )
    }
}

fn type_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Native { name, .. } => name.clone(),
        TypeRef::Component {
            inline: Some(inline),
            ..
        } => inline.clone(),
        TypeRef::Component { file, inline: None } => {
            file.rsplit('/').next().unwrap_or(file).to_owned()
        }
        TypeRef::Unresolved(written) => written.clone(),
    }
}

/// `clicked` as the handler is written: `onClicked`.
fn handler_name(signal: &str) -> String {
    let mut chars = signal.chars();
    match chars.next() {
        Some(first) => format!("on{}{}", first.to_uppercase(), chars.as_str()),
        None => "on".to_owned(),
    }
}

/// Where the member whose value starts at `value` starts: back over the
/// colon and the (dotted) name. The compiler records where a value is, not
/// always where its name is.
fn member_start(source: &str, value: u32) -> u32 {
    let bytes = source.as_bytes();
    let mut at = (value as usize).min(bytes.len());
    let space = |b: u8| b == b' ' || b == b'\t';
    while at > 0 && space(bytes[at - 1]) {
        at -= 1;
    }
    if at == 0 || bytes[at - 1] != b':' {
        return value;
    }
    at -= 1;
    while at > 0 && space(bytes[at - 1]) {
        at -= 1;
    }
    while at > 0
        && (bytes[at - 1].is_ascii_alphanumeric() || matches!(bytes[at - 1], b'_' | b'.' | b'$'))
    {
        at -= 1;
    }
    at as u32
}

struct Collect<'f> {
    file: &'f CompiledFile,
    source: &'f str,
    rows: Vec<Row>,
    seen: HashSet<usize>,
}

impl Collect<'_> {
    fn expr(
        &mut self,
        object: &ObjectDef,
        property: String,
        kind: Kind,
        expr: ExprId,
        from: Option<u32>,
    ) {
        if !self.seen.insert(expr.index()) {
            return;
        }
        let span = self.file.expr(expr).span;
        let from = match from {
            Some(from) if from <= span.start => from,
            _ => member_start(self.source, span.start),
        };
        self.rows.push(Row {
            from,
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
            object: type_name(&object.ty),
            id: object.id.clone(),
            property,
            kind,
            expr: Some(expr),
            translated: false,
        });
    }

    fn members(&mut self, object: &ObjectDef, prefix: &str, members: &Members) {
        for assigned in &members.assignments {
            self.rows.push(Row {
                from: member_start(self.source, assigned.span.start),
                start: assigned.span.start,
                end: assigned.span.end,
                line: assigned.span.line,
                column: assigned.span.column,
                object: type_name(&object.ty),
                id: object.id.clone(),
                property: format!("{prefix}{}", assigned.name),
                kind: Kind::Assigned,
                expr: None,
                translated: matches!(assigned.value, Literal::Translatable(_)),
            });
        }
        for bound in &members.bindings {
            self.expr(
                object,
                format!("{prefix}{}", bound.name),
                Kind::Binding,
                bound.expr,
                Some(bound.name_span.start),
            );
        }
        for handler in &members.handlers {
            let name = format!("{prefix}{}", handler_name(&handler.signal));
            self.expr(object, name, Kind::Handler, handler.expr, None);
        }
    }

    fn object(&mut self, object: &ObjectDef) {
        self.members(object, "", &object.members);
        for group in &object.grouped {
            let prefix = format!("{}.", group.path.join("."));
            self.members(object, &prefix, &group.members);
        }
        for attached in &object.attached {
            let prefix = format!("{}.", attached.written);
            self.members(object, &prefix, &attached.members);
        }
        for function in &object.functions {
            let start = self.file.expr(function.expr).span.start;
            self.expr(
                object,
                function.name.clone(),
                Kind::Function,
                function.expr,
                Some(start),
            );
        }
    }
}

/// Every value of the file, in the order they are written.
fn rows(file: &CompiledFile, source: &str) -> Vec<Row> {
    let mut collect = Collect {
        file,
        source,
        rows: Vec::new(),
        seen: HashSet::new(),
    };
    for component in &file.components {
        for object in &component.objects {
            collect.object(object);
        }
    }
    // An expression no member names is still an expression of the file:
    // it is counted and can be found by where it is.
    for index in 0..file.exprs.len() {
        let id = ExprId(index as u32);
        let binding = file.expr(id);
        let kind = match binding.kind {
            BindingKind::Property => Kind::Binding,
            BindingKind::Handler => Kind::Handler,
            BindingKind::Function => Kind::Function,
        };
        let object = file.object(binding.scope);
        collect.expr(object, "(expression)".to_owned(), kind, id, None);
    }
    let mut rows = collect.rows;
    rows.sort_by_key(|row| (row.start, row.end));
    rows
}

/// The functions of the file's module whose body is `return (p.a.b);` with
/// `p` a parameter: what the engine runs as tier 0. The engine finds them
/// in the generated text when it loads the file, and this reads the text
/// the same way, so the two cannot disagree while the compiler writes a
/// path that way.
fn tier0_exports(file: &CompiledFile) -> HashSet<&str> {
    let is_name = |s: &str| {
        let mut chars = s.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    };
    let mut found = HashSet::new();
    let mut lines = file.js.source.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("export function ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once('(') else {
            continue;
        };
        let Some((params, tail)) = rest.split_once(')') else {
            continue;
        };
        if tail.trim() != "{" {
            continue;
        }
        let Some(path) = lines
            .peek()
            .and_then(|body| body.trim().strip_prefix("return ("))
            .and_then(|body| body.strip_suffix(");"))
        else {
            continue;
        };
        let mut segments = path.split('.');
        let Some(first) = segments.next() else {
            continue;
        };
        let first = first
            .strip_prefix("__ljos_single(")
            .and_then(|f| f.strip_suffix(')'))
            .unwrap_or(first);
        if params.split(',').any(|p| p.trim() == first) && segments.all(is_name) {
            found.insert(name);
        }
    }
    found
}

/// The tiers of a compiled file.
struct Tiers<'f> {
    file: &'f CompiledFile,
    outcomes: Vec<Outcome>,
    tier0: HashSet<&'f str>,
}

impl<'f> Tiers<'f> {
    fn of(file: &'f CompiledFile) -> Self {
        Self {
            file,
            outcomes: tier1::analyze(file),
            tier0: tier0_exports(file),
        }
    }

    fn tier(&self, expr: ExprId) -> (Tier, Option<&tier1::Reason>) {
        let binding = self.file.expr(expr);
        let export = self
            .file
            .js
            .functions
            .get(binding.js_fn.index())
            .map(|f| f.export.as_str());
        // The engine follows a path only for a property's value, called
        // with nothing: a handler is run for what it does.
        if binding.kind == BindingKind::Property
            && binding.args.is_empty()
            && export.is_some_and(|export| self.tier0.contains(export))
        {
            return (Tier::Zero, None);
        }
        match self.outcomes.get(expr.index()) {
            Some(Outcome::Tier2(reason)) => (Tier::Js, Some(reason)),
            _ => (Tier::One, None),
        }
    }
}

/// Byte offsets of the starts of the lines, to turn an offset into a line
/// and a line and a column into an offset.
struct Lines<'s> {
    source: &'s str,
    starts: Vec<u32>,
}

impl<'s> Lines<'s> {
    fn of(source: &'s str) -> Self {
        let starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at as u32 + 1))
            .collect();
        Self { source, starts }
    }

    /// The 1-based line `offset` is on.
    fn line(&self, offset: u32) -> u32 {
        self.starts.partition_point(|start| *start <= offset) as u32
    }

    /// The offset of a 1-based line and column; past the end of the line
    /// it is the end of the line.
    fn offset(&self, line: u32, column: u32) -> Option<u32> {
        let start = *self.starts.get(line.checked_sub(1)? as usize)? as usize;
        let text = self.source[start..].lines().next().unwrap_or("");
        let within = text
            .char_indices()
            .nth(column.saturating_sub(1) as usize)
            .map_or(text.len(), |(at, _)| at);
        Some((start + within) as u32)
    }
}

fn clipped(text: &str) -> String {
    if text.len() <= SOURCE_BYTES {
        return text.to_owned();
    }
    let mut cut = SOURCE_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &text[..cut])
}

/// What an author can do about an expression the binding VM does not run,
/// when no form of it is known that the VM does.
fn advice(reason: &tier1::Reason, kind: Kind) -> String {
    let what = if reason.detail.is_empty() {
        reason.kind.to_owned()
    } else {
        format!("`{}`", reason.detail)
    };
    let mut text = match reason.kind {
        "method call" => format!(
            "A method of a value ({what}) is JavaScript's to run, and no other form means the \
             same. If the result is read often, keep it in a property of its own, so it is \
             worked out when its inputs change and not once per reader."
        ),
        "global function call" | "global" | "member" | "member of a singleton" => format!(
            "{what} is not something the binding VM knows, so the call is JavaScript's. The VM \
             knows the arithmetic of `Math` (`Math.min`, `Math.max`, `Math.round`, `Math.floor`, \
             `Math.ceil`, `Math.abs`) and `qsTr`; anything else stays in JavaScript."
        ),
        "statement" => format!(
            "A {what} statement is outside the subset. Where the statements only choose a \
             value, an expression with `?:`, `&&` or `||` says the same and the binding VM \
             runs it; otherwise it stays in JavaScript."
        ),
        "function value" | "function" | "function shape" | "function call" | "parameter" => {
            "A function used as a value (a callback, a closure, or one called through a \
             variable) is JavaScript's to run. Where the function only computes from \
             properties, write the computation in the binding itself."
                .to_owned()
        }
        "object literal" | "new" | "spread" | "destructuring" | "literal" => {
            "Building a JavaScript value (an object, an array that is spread, a regular \
             expression) needs JavaScript. If the value does not depend on anything, give it \
             to a property once and bind to that property."
                .to_owned()
        }
        "too large" => "The expression is longer than the binding VM takes. Move a part of it \
                        to a property of its own and read that property here: each part is \
                        then small enough, and is only worked out again when its inputs change."
            .to_owned(),
        "the module does not parse" | "the script does not parse" => {
            "The JavaScript of the file does not parse as a whole, so none of it could be \
             classified; run `qml_check` on the file."
                .to_owned()
        }
        _ => format!(
            "No cheaper form is known that means the same ({}). An expression left to \
             JavaScript is correct, only slower to evaluate.",
            reason
        ),
    };
    match kind {
        Kind::Handler | Kind::Function => text.push_str(
            " In a handler or a function this seldom matters: it runs when it is called, \
             not each time a property changes.",
        ),
        _ => text.push_str(
            " It matters where the binding is evaluated often: in a delegate, or on every \
             frame of an animation.",
        ),
    }
    text
}

/// The compiler's word on `proposal` in place of the value of `row`: the
/// tier it then runs in, if that is lower than JavaScript and nothing new
/// is wrong with the file.
fn verified(
    compiler: &mut Compiler,
    key: &str,
    source: &str,
    row: &Row,
    proposal: &rewrite::Proposal,
    errors: usize,
) -> Option<Tier> {
    let mut text = String::with_capacity(source.len());
    text.push_str(&source[..row.start as usize]);
    text.push_str(&proposal.source);
    text.push_str(&source[row.end as usize..]);
    let compiled = ljos_compiler::compile_text(compiler, key, &text).ok()?;
    if error_count(&compiled) > errors {
        return None;
    }
    let tiers = Tiers::of(&compiled.file);
    let at = (0..compiled.file.exprs.len())
        .map(|i| ExprId(i as u32))
        .find(|id| {
            let binding = compiled.file.expr(*id);
            binding.kind == BindingKind::Property && binding.span.start == row.start
        })?;
    match tiers.tier(at).0 {
        Tier::Js => None,
        lower => Some(lower),
    }
}

fn error_count(compiled: &ljos_compiler::Compiled) -> usize {
    compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == ljos_compiler::Severity::Error)
        .count()
}

/// The rows the arguments ask for, by index into `rows`.
fn chosen(
    args: &TierArgs,
    shown: &str,
    rows: &[Row],
    lines: &Lines<'_>,
) -> Result<Vec<usize>, SurfaceError> {
    if args.column.is_some() && args.line.is_none() {
        return Err(SurfaceError::invalid_argument(
            "`column` is a place on a line: give `line` with it",
        ));
    }
    let mut picked: Vec<usize> = (0..rows.len()).collect();
    if let Some(property) = args.property.as_deref().map(str::trim) {
        if property.is_empty() {
            return Err(SurfaceError::invalid_argument("`property` is empty"));
        }
        let exact: Vec<usize> = picked
            .iter()
            .copied()
            .filter(|i| rows[*i].property == property)
            .collect();
        // `label.width`: every dot may be the one between the object and
        // its property (`label.anchors.fill`).
        let qualified = || -> Vec<usize> {
            picked
                .iter()
                .copied()
                .filter(|i| {
                    let row = &rows[*i];
                    property.match_indices('.').any(|(dot, _)| {
                        let (object, member) = (&property[..dot], &property[dot + 1..]);
                        row.property == member
                            && (row.id.as_deref() == Some(object) || row.object == object)
                    })
                })
                .collect()
        };
        picked = if exact.is_empty() { qualified() } else { exact };
        if picked.is_empty() {
            let mut known: Vec<String> = rows.iter().map(Row::qualified).collect();
            known.dedup();
            let listed = if known.is_empty() {
                "the file binds nothing".to_owned()
            } else {
                let more = known.len().saturating_sub(12);
                let mut listed = format!("it has {}", known[..known.len().min(12)].join(", "));
                if more > 0 {
                    listed.push_str(&format!(" and {more} more"));
                }
                listed
            };
            return Err(SurfaceError::not_found(format!(
                "`{shown}` gives no value to `{property}`: {listed}"
            )));
        }
    }
    if let Some(line) = args.line {
        let on_line = |i: &usize| {
            let row = &rows[*i];
            lines.line(row.from) <= line
                && line <= lines.line(row.end.saturating_sub(1).max(row.start))
        };
        let here: Vec<usize> = picked.iter().copied().filter(on_line).collect();
        if here.is_empty() {
            let before = picked
                .iter()
                .map(|i| &rows[*i])
                .rfind(|row| lines.line(row.end) < line);
            let after = picked
                .iter()
                .map(|i| &rows[*i])
                .find(|row| lines.line(row.from) > line);
            let near: Vec<String> = [before, after]
                .into_iter()
                .flatten()
                .map(|row| format!("`{}` on line {}", row.qualified(), lines.line(row.from)))
                .collect();
            let near = if near.is_empty() {
                "the file binds nothing".to_owned()
            } else {
                format!("the nearest are {}", near.join(" and "))
            };
            return Err(SurfaceError::not_found(format!(
                "nothing is given a value on line {line} of `{shown}`; {near}"
            )));
        }
        picked = here;
        if let Some(column) = args.column
            && let Some(at) = lines.offset(line, column)
        {
            // The one the place is in, or the one nearest to it.
            let distance = |i: &usize| {
                let row = &rows[*i];
                if at < row.from {
                    row.from - at
                } else {
                    at.saturating_sub(row.end)
                }
            };
            if let Some(nearest) = picked.iter().copied().min_by_key(|i| distance(i)) {
                picked = vec![nearest];
            }
        }
        return Ok(picked);
    }
    if args.property.is_some() && picked.len() > 1 {
        let candidates: Vec<serde_json::Value> = picked
            .iter()
            .map(|i| {
                let row = &rows[*i];
                json!({ "property": row.qualified(), "line": row.line, "column": row.column })
            })
            .collect();
        let named: Vec<String> = picked
            .iter()
            .map(|i| format!("`{}` (line {})", rows[*i].qualified(), rows[*i].line))
            .collect();
        return Err(SurfaceError::new(
            ErrorCode::Ambiguous,
            format!(
                "{} values in `{shown}` are of a `{}`: {}. Name the object by its id or type, \
                 or give the `line`",
                picked.len(),
                args.property.as_deref().unwrap_or_default().trim(),
                named.join(", ")
            ),
        )
        .with_details(json!({ "candidates": candidates })));
    }
    Ok(picked)
}

fn explain(project: &Project, args: &TierArgs) -> Result<TierOut, SurfaceError> {
    let path = project.confine(&args.file)?;
    let shown = project.display(&path);
    let Some(key) = shown.strip_suffix(".qml").filter(|_| path.is_file()) else {
        return Err(SurfaceError::invalid_argument(format!(
            "`{shown}` is not a `.qml` file"
        )));
    };
    let source = std::fs::read_to_string(&path)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, format!("`{shown}`: {e}")))?;
    let to_checker = |first: String, details: Vec<String>| {
        SurfaceError::invalid_argument(format!(
            "`{shown}` does not compile ({first}), so it has no tiers to explain; run \
             `qml_check` on it and fix what that reports first"
        ))
        .with_details(json!({ "diagnostics": details }))
    };
    {
        // The compiler keeps what it can of a file with syntax errors, and
        // the tiers of half a file would mislead.
        let allocator = qml_syntax::Allocator::default();
        let parsed = qml_syntax::parse(&allocator, &source);
        if !parsed.is_ok() {
            let details: Vec<String> = parsed
                .diagnostics
                .iter()
                .take(10)
                .map(|d| {
                    let (line, column) = d.position(&source);
                    format!("{line}:{column}: error: {}", d.message)
                })
                .collect();
            let first = details
                .first()
                .cloned()
                .unwrap_or_else(|| "it does not parse".to_owned());
            return Err(to_checker(first, details));
        }
    }
    let mut compiler = Compiler::load(project.root(), QtTypeOracle)
        .map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))?;
    // A walk of the project leaves build output and hidden directories
    // out; a file that is named is explained wherever it is.
    if compiler.source(key) != Some(source.as_str()) {
        compiler.add(key, source.clone());
    }
    let compiled = ljos_compiler::compile_file(&compiler, key).map_err(|diagnostics| {
        let details: Vec<String> = diagnostics
            .iter()
            .take(10)
            .map(ToString::to_string)
            .collect();
        let first = details
            .first()
            .cloned()
            .unwrap_or_else(|| "no diagnostics".to_owned());
        to_checker(first, details)
    })?;
    let errors = error_count(&compiled);
    let file = &compiled.file;
    let tiers = Tiers::of(file);
    let rows = rows(file, &source);
    let lines = Lines::of(&source);

    let mut summary = Summary {
        expressions: 0,
        tier0: 0,
        tier1: 0,
        js: 0,
        assigned: 0,
    };
    for row in &rows {
        match row.expr.map(|expr| tiers.tier(expr).0) {
            None => summary.assigned += 1,
            Some(Tier::Zero) => summary.tier0 += 1,
            Some(Tier::One) => summary.tier1 += 1,
            Some(Tier::Js) => summary.js += 1,
        }
    }
    summary.expressions = summary.tier0 + summary.tier1 + summary.js;

    let picked = chosen(args, &shown, &rows, &lines)?;
    let start = tools::cursor(args.cursor.as_deref())?;
    let mut expressions = Vec::new();
    let mut spent = 0;
    let mut next = start.min(picked.len());
    for index in picked.iter().skip(start) {
        if expressions.len() == PAGE || spent >= BUDGET {
            break;
        }
        let row = &rows[*index];
        let text = &source[row.start as usize..row.end as usize];
        let mut out = ExprOut {
            line: row.line,
            column: row.column,
            object: row.object.clone(),
            id: row.id.clone(),
            property: row.property.clone(),
            kind: row.kind,
            tier: None,
            source: clipped(text),
            why: String::new(),
            reason: None,
            rewrite: None,
            advice: None,
        };
        match row.expr.map(|expr| tiers.tier(expr)) {
            None if row.translated => {
                out.why = "A text to translate, written out: it is looked up when the object \
                           is made and again when the language changes. Nothing is evaluated."
                    .to_owned();
            }
            None => {
                out.why = "A value that is written out: it is set once, when the object is \
                           made, and nothing is evaluated."
                    .to_owned();
            }
            Some((Tier::Zero, _)) => {
                out.tier = Some(Tier::Zero);
                out.why = "It only reads a property path, which the engine follows without \
                           running any code: the cheapest a binding can be."
                    .to_owned();
            }
            Some((Tier::One, _)) => {
                out.tier = Some(Tier::One);
                out.why = "Everything in it is in the subset the binding VM runs (property \
                           reads, arithmetic, comparisons, conditionals and the calls it \
                           knows), so no JavaScript is run."
                    .to_owned();
            }
            Some((Tier::Js, reason)) => {
                out.tier = Some(Tier::Js);
                let reason = reason.cloned().unwrap_or(tier1::Reason {
                    kind: "expression",
                    detail: String::new(),
                });
                out.why = format!(
                    "`{reason}` is outside the subset the binding VM runs, so the whole \
                     expression is left to the JavaScript engine."
                );
                if row.kind == Kind::Binding
                    && let Some(proposal) = rewrite::propose(&source, row.start)
                    && let Some(tier) =
                        verified(&mut compiler, key, &source, row, &proposal, errors)
                {
                    out.rewrite = Some(RewriteOut {
                        source: proposal.source,
                        tier,
                        note: proposal.note.to_owned(),
                    });
                } else {
                    out.advice = Some(advice(&reason, row.kind));
                }
                out.reason = Some(ReasonOut {
                    kind: reason.kind.to_owned(),
                    detail: reason.detail,
                });
            }
        }
        spent += out.source.len()
            + out.why.len()
            + out.advice.as_ref().map_or(0, String::len)
            + out
                .rewrite
                .as_ref()
                .map_or(0, |r| r.source.len() + r.note.len())
            + 256;
        expressions.push(out);
        next += 1;
    }
    Ok(TierOut {
        file: shown.clone(),
        summary,
        expressions,
        note: (errors > 0).then(|| {
            format!(
                "`{shown}` compiled with {errors} error(s); what did compile is explained. \
                 Run `qml_check` on it."
            )
        }),
        next_cursor: (next < picked.len()).then(|| next.to_string()),
    })
}

pub(crate) const DESCRIPTION: &str = "Say which tier each binding, handler and function of a QML file runs in (0: property path, 1: binding VM, js: JavaScript), why, and a cheaper form that means the same when one exists. Narrow with `line` (and `column`) or `property` (`width`, `label.width`, `onClicked`).";

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |_: &CallCtx, args: TierArgs| explain(&project, &args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_starts_at_its_name() {
        let source = "Item {\n    anchors.fill: parent\n    width : 10\n}\n";
        let value = source.find("parent").unwrap() as u32;
        assert_eq!(
            member_start(source, value),
            source.find("anchors").unwrap() as u32
        );
        let value = source.find("10").unwrap() as u32;
        assert_eq!(
            member_start(source, value),
            source.find("width").unwrap() as u32
        );
        // A value nothing names starts where it starts.
        assert_eq!(member_start("function f() {}", 0), 0);
    }

    #[test]
    fn lines_and_columns_are_one_based_and_count_characters() {
        let lines = Lines::of("ab\nþó: 1\n\nx");
        assert_eq!(lines.line(0), 1);
        assert_eq!(lines.line(2), 1);
        assert_eq!(lines.line(3), 2);
        // `þ` and `ó` are two bytes each.
        assert_eq!(lines.offset(2, 3), Some(3 + 4));
        assert_eq!(lines.offset(2, 99), Some(3 + 7));
        assert_eq!(lines.offset(4, 1), Some(12));
        assert_eq!(lines.offset(9, 1), None);
        assert_eq!(lines.offset(0, 1), None);
    }

    #[test]
    fn a_handler_is_named_as_it_is_written() {
        assert_eq!(handler_name("clicked"), "onClicked");
        assert_eq!(handler_name("textChanged"), "onTextChanged");
    }

    #[test]
    fn a_long_expression_is_cut_at_a_character() {
        let long = "þ".repeat(400);
        let cut = clipped(&long);
        assert!(cut.ends_with('…'));
        assert!(cut.len() <= SOURCE_BYTES + '…'.len_utf8());
        assert_eq!(clipped("width"), "width");
    }
}
