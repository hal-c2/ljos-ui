//! The `ListModel`s of a file that are tables of constants, which Ljós
//! writes as a `readonly property list<T>` of objects with the row type `T`
//! in a module beside the file (docs/DIALECT.md §4.8).
//!
//! A table is written so only where nothing in the file could tell the
//! list from the model: no script edits it or reads it but as a view's
//! `model:`, and the delegates of the views given it read its rows only as
//! `model.<role>`, which a list gives as `modelData.<role>`. A delegate
//! that reads a role by its bare name, or a file whose other types may read
//! it so through the context, keeps the table a `ListModel`.

use oxc_ast::ast::{
    Expression, IdentifierReference, SimpleAssignmentTarget, StaticMemberExpression, UnaryOperator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

use qml_edit::Change;

/// The name a file's row module is imported under.
pub(crate) const ALIAS: &str = "Rows";

/// A table the pass writes as a list.
#[derive(Debug)]
pub(crate) struct Table {
    /// Where its `ListModel` starts.
    pub at: u32,
    /// The row type's name in the module.
    pub row: String,
    /// Its roles and their TypeScript types, as the elements first set them.
    pub roles: Vec<(String, &'static str)>,
    pub changes: Vec<Change>,
}

/// The module that declares the row types of `tables`, those of the file
/// `file` (`Main.qml`).
pub(crate) fn module(file: &str, tables: &[Table]) -> String {
    let mut text =
        format!("// The rows of the constant ListModels of {file} (docs/DIALECT.md §4.8).\n");
    for (i, table) in tables.iter().enumerate() {
        if i > 0 {
            text.push('\n');
        }
        text.push_str(&format!("export interface {} {{\n", table.row));
        for (role, ty) in &table.roles {
            text.push_str(&format!("  readonly {role}: {ty};\n"));
        }
        text.push_str("}\n");
    }
    text
}

/// The tables of the file whose root object is `root`.
pub(crate) fn tables(
    source: &str,
    root: &QmlObject<'_>,
    is_qt_type: &dyn Fn(&str) -> bool,
) -> Vec<Table> {
    let mut file = Reads::default();
    reads_of(root, &mut file);
    // A view's model read off it (`list.model.count`,
    // `ListView.view.model.get(0)`) is asked as a `ListModel`, which a list
    // does not answer: no table of the file is known not to be that model.
    if file.names.iter().any(|n| n == ALIAS) || file.model_read {
        return Vec::new();
    }
    let mut views = Vec::new();
    find_views(root, &mut views);
    let mut taken: Vec<String> = Vec::new();
    let mut found = Vec::new();
    // Ids first: a table with one is named for it.
    let others_reach = has_other_files(root, is_qt_type);
    for member in &root.members {
        let QmlMember::Object(model) = member else {
            continue;
        };
        let Some(id) = crate::object_id(model) else {
            continue;
        };
        if model.type_name.as_simple() != Some("ListModel") || others_reach {
            continue;
        }
        let Some(rows) = constants(model) else {
            continue;
        };
        let given: Vec<&View<'_, '_>> = views
            .iter()
            .filter(|v| matches!(v.model, Model::Id(name) if name == id))
            .collect();
        let read = file.names.iter().filter(|n| **n == id).count();
        if given.is_empty()
            || read != given.len()
            || !given
                .iter()
                .all(|v| delegate_reads_rows(v, &rows, is_qt_type))
            || declares(root, id)
        {
            continue;
        }
        let row = unique(&mut taken, &type_name(id));
        let head = format!("readonly property list<{ALIAS}.{row}> {id}: [");
        if let Some(table) = write(source, model, model.span.start, head, "]", &rows, row) {
            found.push(table);
        }
    }
    // A view's `rows` is a name the file does not already read.
    if file.names.iter().any(|n| n == "rows") {
        return found;
    }
    for view in &views {
        let Model::Inline(model, binding_at) = view.model else {
            continue;
        };
        if crate::object_id(model).is_some() {
            continue;
        }
        let Some(rows) = constants(model) else {
            continue;
        };
        if !delegate_reads_rows(view, &rows, is_qt_type) {
            continue;
        }
        let base = match crate::object_id(view.object) {
            Some(id) => type_name(id),
            None => "Row".to_string(),
        };
        let row = unique(&mut taken, &base);
        let head = format!("readonly property list<{ALIAS}.{row}> rows: [");
        if let Some(table) = write(
            source,
            model,
            binding_at,
            head,
            "]; model: rows",
            &rows,
            row,
        ) {
            found.push(table);
        }
    }
    found.sort_by_key(|t| t.at);
    found
}

/// `zones` → `ZonesRow`, `fruitModel` → `FruitRow`.
fn type_name(id: &str) -> String {
    let base = id
        .strip_suffix("Model")
        .filter(|b| !b.is_empty())
        .unwrap_or(id);
    let mut chars = base.chars();
    let first: String = chars
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    format!("{first}{}Row", chars.as_str())
}

fn unique(taken: &mut Vec<String>, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}{n}");
        n += 1;
    }
    taken.push(name.clone());
    name
}

/// A role's value: a string, a number or a boolean, as written.
#[derive(Clone, Copy)]
struct Value {
    ty: &'static str,
}

/// The rows of `model` when it is a table of constants: each role with its
/// type, every element setting each role to a value of that one type.
fn constants(model: &QmlObject<'_>) -> Option<Vec<(String, &'static str)>> {
    let mut roles: Vec<(String, &'static str)> = Vec::new();
    let mut elements = 0;
    for member in &model.members {
        match member {
            QmlMember::Binding(b) if b.name.as_simple() == Some("id") => {}
            QmlMember::Object(element) if element.type_name.as_simple() == Some("ListElement") => {
                elements += 1;
                for member in &element.members {
                    let QmlMember::Binding(binding) = member else {
                        return None;
                    };
                    let name = binding.name.as_simple()?;
                    let value = value(&binding.value)?;
                    match roles.iter().find(|(r, _)| r == name) {
                        Some((_, ty)) if *ty != value.ty => return None,
                        Some(_) => {}
                        None => roles.push((name.to_string(), value.ty)),
                    }
                }
            }
            _ => return None,
        }
    }
    (elements > 0 && !roles.is_empty()).then_some(roles)
}

fn value(value: &QmlBindingValue<'_>) -> Option<Value> {
    let QmlBindingValue::Expression(expression) = value else {
        return None;
    };
    let ty = match expression {
        Expression::StringLiteral(_) => "string",
        Expression::NumericLiteral(_) => "number",
        Expression::BooleanLiteral(_) => "boolean",
        Expression::UnaryExpression(unary)
            if unary.operator == UnaryOperator::UnaryNegation
                && matches!(unary.argument, Expression::NumericLiteral(_)) =>
        {
            "number"
        }
        _ => return None,
    };
    Some(Value { ty })
}

/// What a role a row leaves out reads as in Qt: its type's default.
fn default_of(ty: &str) -> &'static str {
    match ty {
        "string" => "\"\"",
        "number" => "0",
        _ => "false",
    }
}

/// The edits that write `model` as a list: `head` from `from` to its `{`,
/// each element an object, and its `}` as `tail`. None when an edit would
/// cross a line.
fn write(
    source: &str,
    model: &QmlObject<'_>,
    from: u32,
    head: String,
    tail: &str,
    roles: &[(String, &'static str)],
    row: String,
) -> Option<Table> {
    let mut changes = Vec::new();
    let mut edit = |range: std::ops::Range<usize>, text: String| {
        let crosses = source.get(range.clone()).is_none_or(|t| t.contains('\n'));
        changes.push(Change {
            range,
            replacement: text,
        });
        !crosses
    };
    let open = brace_after(source, model.type_name.span.end as usize)?;
    if !edit(from as usize..open + 1, head) {
        return None;
    }
    let elements: Vec<&QmlObject<'_>> = model
        .members
        .iter()
        .filter_map(|m| match m {
            QmlMember::Object(o) => Some(o),
            _ => None,
        })
        .collect();
    for member in &model.members {
        if let QmlMember::Binding(id) = member {
            let (start, end) = (id.span.start as usize, id.span.end as usize);
            let end = after_semicolon(source, end);
            // Alone on its line, the line stays, empty.
            let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = source[end..].find('\n').map_or(source.len(), |i| end + i);
            let range = if source[line_start..start].trim().is_empty()
                && source[end..line_end].trim().is_empty()
            {
                line_start..line_end
            } else {
                start..skip_spaces(source, end)
            };
            if !edit(range, String::new()) {
                return None;
            }
        }
    }
    for (i, element) in elements.iter().enumerate() {
        let name = element.type_name.span;
        let brace = brace_after(source, name.end as usize)?;
        let range = if source[name.end as usize..brace].contains('\n') {
            name.start as usize..name.end as usize
        } else {
            name.start as usize..brace
        };
        if !edit(range, String::new()) {
            return None;
        }
        let bindings: Vec<_> = element
            .members
            .iter()
            .filter_map(|m| match m {
                QmlMember::Binding(b) => Some(b),
                _ => None,
            })
            .collect();
        let set: Vec<&str> = bindings.iter().filter_map(|b| b.name.as_simple()).collect();
        let missing: Vec<String> = roles
            .iter()
            .filter(|(r, _)| !set.contains(&r.as_str()))
            .map(|(r, ty)| format!("{r}: {}", default_of(ty)))
            .collect();
        for (j, binding) in bindings.iter().enumerate() {
            let QmlBindingValue::Expression(expression) = &binding.value else {
                return None;
            };
            let end = expression.span().end as usize;
            let semicolon = source[end..]
                .find(|c: char| !c.is_whitespace())
                .filter(|&k| source[end + k..].starts_with(';'))
                .map(|k| end + k);
            let last = j + 1 == bindings.len();
            match (last, semicolon) {
                (false, Some(at)) => {
                    if !edit(at..at + 1, ",".into()) {
                        return None;
                    }
                }
                (false, None) => {
                    if !edit(end..end, ",".into()) {
                        return None;
                    }
                }
                (true, Some(at)) => {
                    if !edit(at..at + 1, String::new()) {
                        return None;
                    }
                }
                (true, None) => {}
            }
            if last && !missing.is_empty() && !edit(end..end, format!(", {}", missing.join(", "))) {
                return None;
            }
        }
        if bindings.is_empty() && !edit(brace + 1..brace + 1, format!(" {} ", missing.join(", "))) {
            return None;
        }
        if i + 1 < elements.len() {
            let end = element.span.end as usize;
            if !edit(end..end, ",".into()) {
                return None;
            }
        }
    }
    let close = model.span.end as usize - 1;
    if source.get(close..close + 1) != Some("}") || !edit(close..close + 1, tail.into()) {
        return None;
    }
    Some(Table {
        at: model.span.start,
        row,
        roles: roles.to_vec(),
        changes,
    })
}

fn brace_after(source: &str, from: usize) -> Option<usize> {
    let rest = source.get(from..)?;
    let at = rest.find(|c: char| !c.is_whitespace())?;
    rest[at..].starts_with('{').then_some(from + at)
}

fn after_semicolon(source: &str, end: usize) -> usize {
    let rest = &source[end..];
    match rest.find(|c: char| c != ' ' && c != '\t') {
        Some(k) if rest[k..].starts_with(';') => end + k + 1,
        _ => end,
    }
}

fn skip_spaces(source: &str, end: usize) -> usize {
    let rest = &source[end..];
    end + rest
        .find(|c: char| c != ' ' && c != '\t')
        .unwrap_or(rest.len())
}

/// Whether the file declares `name` itself, or its root's type has it: the
/// list would then not be what `name` reads.
fn declares(root: &QmlObject<'_>, name: &str) -> bool {
    if qt_types::module("QtQuick")
        .and_then(|m| m.type_named(root.type_name.as_simple()?))
        .is_some_and(|t| t.property(name).is_some())
    {
        return true;
    }
    let mut found = false;
    each_object(root, &mut |object| {
        for member in &object.members {
            let declared = match member {
                QmlMember::Property(p) => p.name.name == name,
                QmlMember::Function(f) => f.id.as_ref().is_some_and(|id| id.name == name),
                QmlMember::Signal(s) => s.name.name == name,
                _ => false,
            };
            found |= declared;
        }
    });
    found
}

/// Whether a type of another file, an inline component or a `Loader` is in
/// the file: what such a file reads through the context, the pass does not
/// see.
fn has_other_files(root: &QmlObject<'_>, is_qt_type: &dyn Fn(&str) -> bool) -> bool {
    let mut found = false;
    each_object(root, &mut |object| {
        found |= object
            .members
            .iter()
            .any(|m| matches!(m, QmlMember::InlineComponent(_)));
        match object.type_name.as_simple() {
            Some("Loader") => found = true,
            Some(name) if name.starts_with(|c: char| c.is_ascii_lowercase()) => {}
            Some(name) => found |= !is_qt_type(name),
            None => found = true,
        }
    });
    found
}

fn each_object<'o, 'a>(object: &'o QmlObject<'a>, f: &mut dyn FnMut(&'o QmlObject<'a>)) {
    f(object);
    for member in &object.members {
        match member {
            QmlMember::Object(child) => each_object(child, f),
            QmlMember::Binding(b) => each_value(&b.value, f),
            QmlMember::Property(p) => {
                if let Some(value) = &p.value {
                    each_value(value, f);
                }
            }
            QmlMember::InlineComponent(c) => each_object(&c.object, f),
            QmlMember::Signal(_) | QmlMember::Function(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn each_value<'o, 'a>(value: &'o QmlBindingValue<'a>, f: &mut dyn FnMut(&'o QmlObject<'a>)) {
    match value {
        QmlBindingValue::Object(o) => each_object(o, f),
        QmlBindingValue::Objects(os) => os.iter().for_each(|o| each_object(o, f)),
        QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
    }
}

/// A `Repeater` or `ListView` and what it is given.
struct View<'o, 'a> {
    object: &'o QmlObject<'a>,
    model: Model<'o, 'a>,
    delegate: Option<&'o QmlObject<'a>>,
}

enum Model<'o, 'a> {
    /// `model: zones`.
    Id(&'o str),
    /// `model: ListModel { … }`, and where the binding starts.
    Inline(&'o QmlObject<'a>, u32),
    Other,
}

fn find_views<'o, 'a>(root: &'o QmlObject<'a>, views: &mut Vec<View<'o, 'a>>) {
    each_object(root, &mut |object| {
        let Some(name @ ("Repeater" | "ListView")) = object.type_name.as_simple() else {
            return;
        };
        let mut model = Model::Other;
        let mut delegate = None;
        let mut children = Vec::new();
        let mut plain = true;
        for member in &object.members {
            match member {
                QmlMember::Binding(b) => {
                    let first = b.name.parts[0];
                    if first == "section" {
                        plain = false;
                    }
                    match (b.name.as_simple(), &b.value) {
                        (
                            Some("model"),
                            QmlBindingValue::Expression(Expression::Identifier(id)),
                        ) => {
                            model = Model::Id(id.name.as_str());
                        }
                        (Some("model"), QmlBindingValue::Object(o))
                            if o.type_name.as_simple() == Some("ListModel") =>
                        {
                            model = Model::Inline(o, b.span.start);
                        }
                        (Some("delegate"), QmlBindingValue::Object(o)) => delegate = Some(o),
                        (Some("delegate"), _) => plain = false,
                        _ => {}
                    }
                }
                // `section { property: "kind" }` is the grouped form of
                // `section.property`.
                QmlMember::Object(group) if group.type_name.as_simple() == Some("section") => {
                    plain = false;
                }
                QmlMember::Object(child)
                    if child
                        .type_name
                        .as_simple()
                        .is_some_and(|n| n.starts_with(|c: char| c.is_ascii_uppercase())) =>
                {
                    children.push(child);
                }
                _ => {}
            }
        }
        // A Repeater's child is its delegate.
        if name == "Repeater" && delegate.is_none() && children.len() == 1 {
            delegate = Some(children[0]);
        }
        if plain {
            views.push(View {
                object,
                model,
                delegate,
            });
        }
    });
}

/// Whether the delegate of `view` reads the rows of a table of `roles` only
/// as a list gives them too.
fn delegate_reads_rows(
    view: &View<'_, '_>,
    roles: &[(String, &'static str)],
    is_qt_type: &dyn Fn(&str) -> bool,
) -> bool {
    let Some(delegate) = view.delegate else {
        return false;
    };
    // A delegate that declares required properties is given them by name, by
    // a list as by a model, and the files in it read those; one that does
    // not reads the roles through the context, which a list does not fill.
    let required = delegate
        .members
        .iter()
        .any(|m| matches!(m, QmlMember::Property(p) if p.is_required));
    let mut ok = true;
    let mut first = true;
    each_object(delegate, &mut |object| {
        let is_root = std::mem::take(&mut first);
        if object
            .members
            .iter()
            .any(|m| matches!(m, QmlMember::InlineComponent(_)))
        {
            ok = false;
        }
        match object.type_name.as_simple() {
            Some(name) if name.starts_with(|c: char| c.is_ascii_lowercase()) => {}
            Some(name) if is_qt_type(name) => {
                if !is_root
                    && qt_types::module("QtQuick")
                        .and_then(|m| m.type_named(name))
                        .is_some_and(|t| {
                            t.property("model").is_some() && t.property("delegate").is_some()
                        })
                {
                    ok = false;
                }
            }
            Some(_) => ok &= required,
            None => ok = false,
        }
    });
    if !ok {
        return false;
    }
    let mut reads = Reads::default();
    reads_of(delegate, &mut reads);
    let one_role = roles.len() == 1;
    !reads.model_written
        && !reads.names.iter().any(|n| {
            n == "model"
                || (one_role && n == "modelData")
                || (!required && roles.iter().any(|(r, _)| r == n))
        })
        && !(required && reads.model_member)
}

/// What the scripts of an object and those inside it read.
#[derive(Default)]
struct Reads {
    /// The names read bare, but `model` before a member.
    names: Vec<String>,
    /// Whether some `.model` is read off something.
    model_read: bool,
    /// Whether `model.<member>` is read.
    model_member: bool,
    /// Whether `model.<member>` is written.
    model_written: bool,
}

fn reads_of(object: &QmlObject<'_>, reads: &mut Reads) {
    for member in &object.members {
        match member {
            QmlMember::Binding(b) if b.name.as_simple() == Some("id") => {}
            QmlMember::Binding(b) => value_reads(&b.value, reads),
            QmlMember::Property(p) => {
                if let Some(value) = &p.value {
                    value_reads(value, reads);
                }
            }
            QmlMember::Function(f) => reads.visit_function(f, ScopeFlags::Function),
            QmlMember::Object(child) => reads_of(child, reads),
            QmlMember::InlineComponent(c) => reads_of(&c.object, reads),
            QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn value_reads(value: &QmlBindingValue<'_>, reads: &mut Reads) {
    match value {
        QmlBindingValue::Expression(e) => reads.visit_expression(e),
        QmlBindingValue::Statement(s) => reads.visit_statement(s),
        QmlBindingValue::Object(o) => reads_of(o, reads),
        QmlBindingValue::Objects(os) => os.iter().for_each(|o| reads_of(o, reads)),
    }
}

fn is_model(expression: &Expression<'_>) -> bool {
    matches!(expression, Expression::Identifier(id) if id.name == "model")
}

impl<'a> Visit<'a> for Reads {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.names.push(it.name.to_string());
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if it.property.name == "model" {
            self.model_read = true;
        }
        if is_model(&it.object) {
            self.model_member = true;
            return;
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
        if let SimpleAssignmentTarget::StaticMemberExpression(member) = it
            && is_model(&member.object)
        {
            self.model_written = true;
        }
        walk::walk_simple_assignment_target(self, it);
    }
}
