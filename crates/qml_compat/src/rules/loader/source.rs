//! A `Loader`'s `source`. `source: "Page.qml"` is `component: Page`
//! (DIALECT §4.11): the file is a type of the project, and a component of
//! it is what the Loader made of the string. Anything else a `source` says
//! is only known when it runs, so the file is kept.

use oxc_ast::ast::Expression;
use qml_edit::Change;
use qml_syntax::ast::{QmlBinding, QmlBindingValue, QmlMember, QmlObject};

use crate::rules::{File, Rule};
use crate::{Kept, Loader, is_type_name, names_member, object_id};

/// The Loader's members a script may name unqualified in its scope.
const MEMBERS: &[&str] = &[
    "source",
    "setSource",
    "sourceChanged",
    "sourceComponent",
    "sourceComponentChanged",
];
/// `source`, `setSource` and the signal of its change; then
/// `sourceComponent` and the signal of its change.
const SOURCE_AND_COMPONENT: (&[&str], &[&str]) = MEMBERS.split_at(3);

/// A property of the Loader's and what a file reads or hears it by.
struct Property {
    /// Its name, which an object aimed at the Loader sets.
    name: &'static str,
    /// The members a script reads or calls it by.
    read_by: &'static [&'static str],
    /// The handler of its change.
    handler: &'static str,
}

const SOURCE: Property = Property {
    name: "source",
    read_by: SOURCE_AND_COMPONENT.0,
    handler: "onSourceChanged",
};

const COMPONENT: Property = Property {
    name: "sourceComponent",
    read_by: SOURCE_AND_COMPONENT.1,
    handler: "onSourceComponentChanged",
};

/// The rule of Qt's `Loader`.
pub(crate) struct LoaderRule;

pub(crate) const LOADER: LoaderRule = LoaderRule;

impl Rule for LoaderRule {
    fn owns(&self) -> &'static [&'static str] {
        &["Loader"]
    }

    fn unqualified(&self) -> &'static [&'static str] {
        MEMBERS
    }

    fn object(&self, object: &QmlObject<'_>, file: &dyn File) -> Result<Vec<Change>, Kept> {
        let Some(binding) = own(object, "source") else {
            return Ok(Vec::new());
        };
        let at = binding.name.span.start as usize;
        let no = |cause: Loader| Kept {
            cause: cause.into(),
            at,
        };
        let QmlBindingValue::Expression(Expression::StringLiteral(url)) = &binding.value else {
            return Err(no(Loader::SourceMade));
        };
        let path = url.value.as_str();
        let name = path
            .strip_prefix("./")
            .unwrap_or(path)
            .strip_suffix(".qml")
            .filter(|name| !name.is_empty() && !name.contains(['/', '\\', ':']));
        let Some(name) = name.filter(|name| file.is_beside(name)) else {
            return Err(no(Loader::SourceNotBeside {
                file: path.to_owned(),
            }));
        };
        // Written where a component goes, the name is read as a type only
        // if it is one: `page` would be an id, `my-page` a subtraction.
        if !is_type_name(name) {
            return Err(no(Loader::SourceNotType {
                file: path.to_owned(),
            }));
        }
        if own(object, "sourceComponent").is_some() {
            return Err(no(Loader::Both));
        }
        // The Loader's `source` is then empty: whatever reads it, or hears
        // it change, would be told otherwise.
        if reads(object, file, &SOURCE) {
            return Err(no(Loader::SourceRead));
        }
        // Qt's `sourceComponent` stays empty beside a `source`; lowered, it
        // is the component, and whatever reads it or hears it change would
        // be told so.
        if reads(object, file, &COMPONENT) {
            return Err(no(Loader::ComponentRead));
        }
        let span = binding.name.span;
        Ok(vec![
            Change {
                range: span.start as usize..span.end as usize,
                replacement: "component".to_owned(),
            },
            Change {
                range: url.span.start as usize..url.span.end as usize,
                replacement: name.to_owned(),
            },
        ])
    }
}

/// The binding of `object`'s own member `name`.
fn own<'o, 'a>(object: &'o QmlObject<'a>, name: &str) -> Option<&'o QmlBinding<'a>> {
    object.members.iter().find_map(|member| match member {
        QmlMember::Binding(binding) if binding.name.as_simple() == Some(name) => Some(binding),
        _ => None,
    })
}

/// Whether anything reads `property` of the Loader `object`, by its id,
/// unqualified in its scope, or as an object aimed at it does, or hears it
/// change.
fn reads(object: &QmlObject<'_>, file: &dyn File, property: &Property) -> bool {
    let id = object_id(object);
    id.is_some_and(|id| {
        property
            .read_by
            .iter()
            .any(|member| names_member(file.source(), id, member))
    }) || own(object, property.handler).is_some()
        || id.is_some_and(|id| {
            // An object aimed at it names the member it sets: the property,
            // not its signal.
            file.aimed_at(id, property.name) || file.aimed_at(id, property.handler)
        })
        || file.names_unqualified(object, property.read_by)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_property_is_read_and_heard_by_its_own_name() {
        for property in [SOURCE, COMPONENT] {
            assert!(
                property.read_by.contains(&property.name),
                "{}",
                property.name
            );
            let signal = format!("{}Changed", property.name);
            assert!(property.read_by.contains(&signal.as_str()), "{signal}");
            let (first, rest) = property.name.split_at(1);
            let handler = format!("on{}{rest}Changed", first.to_uppercase());
            assert_eq!(property.handler, handler);
        }
    }
}
