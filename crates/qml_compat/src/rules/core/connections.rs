//! A `Connections`'s handlers are its target's: `target: button;
//! onClicked: …` handles `button`'s `clicked`, by the name Ljós gives it.

use oxc_ast::ast::Expression;
use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

use crate::Ty;
use crate::rules::{Aim, Aimed, File, Rule};

/// The rule of Qt's `Connections`.
pub(crate) struct ConnectionsRule;

pub(crate) const CONNECTIONS: ConnectionsRule = ConnectionsRule;

impl Rule for ConnectionsRule {
    fn owns(&self) -> &'static [&'static str] {
        &["Connections"]
    }

    fn aim(&self, object: &QmlObject<'_>, on: Ty, file: &dyn File) -> Option<Aim> {
        if !matches!(on, Ty::Exact(t) if self.owns().contains(&t.name)) {
            return None;
        }
        let target = object.members.iter().find_map(|member| match member {
            QmlMember::Binding(binding) if binding.name.as_simple() == Some("target") => {
                Some(match &binding.value {
                    QmlBindingValue::Expression(Expression::Identifier(id)) => {
                        file.id(&id.name).unwrap_or(Ty::Unknown)
                    }
                    _ => Ty::Unknown,
                })
            }
            _ => None,
        });
        Some(Aim {
            // Without a target, a `Connections` is on its parent, which
            // the pass does not follow.
            at: target.unwrap_or(Ty::Unknown),
            aimed: Aimed::Handlers,
            through: None,
        })
    }
}
