//! The delegates a file gives a view by id: `delegate: d`, where `d` is a
//! `Component` of the file. Such a `Component` is the delegate of those
//! views as if it were written in their `delegate:`, so long as nothing
//! else in the file names it: what else might make it, and of what rows,
//! the file does not show.

use oxc_ast::ast::{Expression, IdentifierReference};
use oxc_ast_visit::Visit;
use oxc_syntax::scope::ScopeFlags;
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

use crate::rows::{self, Rows};

/// The ids that the views `is_view` picks out name as their delegate, each
/// with the rows of those views (`Unknown` where they differ), leaving out
/// an id the file reads anywhere else.
pub(crate) fn by_id(
    root: &QmlObject<'_>,
    is_view: &dyn Fn(&QmlObject<'_>) -> bool,
    listed: &[(String, Rows)],
    tables: &[u32],
) -> Vec<(String, Rows)> {
    let mut found = Vec::new();
    views(root, is_view, listed, tables, &mut found);
    if found.is_empty() {
        return Vec::new();
    }
    let mut names = Names::default();
    each_script(root, &mut names);
    found
        .into_iter()
        .filter(|g| names.count(&g.id) == g.named)
        .map(|g| (g.id, g.rows))
        .collect()
}

struct Given {
    id: String,
    rows: Rows,
    /// How many `delegate:`s name it.
    named: usize,
}

fn views(
    object: &QmlObject<'_>,
    is_view: &dyn Fn(&QmlObject<'_>) -> bool,
    listed: &[(String, Rows)],
    tables: &[u32],
    found: &mut Vec<Given>,
) {
    for member in &object.members {
        match member {
            QmlMember::Binding(binding) => {
                if binding.name.as_simple() == Some("delegate")
                    && let QmlBindingValue::Expression(Expression::Identifier(id)) = &binding.value
                    && is_view(object)
                {
                    let rows = rows::of_view(object, listed, tables);
                    match found.iter_mut().find(|g| g.id == id.name.as_str()) {
                        Some(g) => {
                            g.named += 1;
                            if g.rows != rows {
                                g.rows = Rows::Unknown;
                            }
                        }
                        None => found.push(Given {
                            id: id.name.to_string(),
                            rows,
                            named: 1,
                        }),
                    }
                }
                each_object(&binding.value, |o| views(o, is_view, listed, tables, found));
            }
            QmlMember::Property(property) => {
                if let Some(value) = &property.value {
                    each_object(value, |o| views(o, is_view, listed, tables, found));
                }
            }
            QmlMember::Object(child) => views(child, is_view, listed, tables, found),
            QmlMember::InlineComponent(component) => {
                views(&component.object, is_view, listed, tables, found);
            }
            QmlMember::Signal(_) | QmlMember::Function(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn each_object<'o, 'a>(value: &'o QmlBindingValue<'a>, mut f: impl FnMut(&'o QmlObject<'a>)) {
    match value {
        QmlBindingValue::Object(object) => f(object),
        QmlBindingValue::Objects(objects) => objects.iter().for_each(f),
        QmlBindingValue::Expression(_) | QmlBindingValue::Statement(_) => {}
    }
}

/// Every script of `object` and what it holds; an `id:` names, not reads.
fn each_script<'a>(object: &QmlObject<'a>, v: &mut Names) {
    for member in &object.members {
        match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("id") => {}
            QmlMember::Binding(binding) => each_value(&binding.value, v),
            QmlMember::Property(property) => {
                if let Some(value) = &property.value {
                    each_value(value, v);
                }
            }
            QmlMember::Function(function) => v.visit_function(function, ScopeFlags::Function),
            QmlMember::Object(child) => each_script(child, v),
            QmlMember::InlineComponent(component) => each_script(&component.object, v),
            QmlMember::Signal(_) | QmlMember::Enum(_) => {}
        }
    }
}

fn each_value<'a>(value: &QmlBindingValue<'a>, v: &mut Names) {
    match value {
        QmlBindingValue::Expression(expression) => v.visit_expression(expression),
        QmlBindingValue::Statement(statement) => v.visit_statement(statement),
        QmlBindingValue::Object(object) => each_script(object, v),
        QmlBindingValue::Objects(objects) => objects.iter().for_each(|o| each_script(o, v)),
    }
}

/// The names the scripts read.
#[derive(Default)]
struct Names(Vec<String>);

impl Names {
    fn count(&self, name: &str) -> usize {
        self.0.iter().filter(|n| *n == name).count()
    }
}

impl<'a> Visit<'a> for Names {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.0.push(it.name.to_string());
    }
}
