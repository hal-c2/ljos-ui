//! `PluginParameter` (doc.qt.io/qt-6/qml-qtpositioning-pluginparameter.html):
//! a name and a value for a backend. QtLocation's `Plugin` takes them as
//! children too, which is why a map that is only drawn as empty Items still
//! needs the type to be here.

use ljos::prelude::*;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtPositioning", name = "PluginParameter", extends = QtObject)]
pub struct PluginParameter {
    #[qml(base)]
    pub object: QtObject,
    /// Kept as first given, as qml6 does: a later write is ignored.
    #[qml(notify, write = Self::first_name)]
    pub name: Atom,
    #[qml(notify, write = Self::first_value)]
    pub value: Value,
}

impl PluginParameter {
    fn first_name(&self, name: Atom) -> Option<Atom> {
        self.name.as_str().is_empty().then_some(name)
    }

    fn first_value(&self, value: Value) -> Option<Value> {
        matches!(self.value, Value::Undefined).then_some(value)
    }

    fn is_given(&self) -> bool {
        !self.name.as_str().is_empty() && !matches!(self.value, Value::Undefined)
    }
}

#[qml_impl]
impl PluginParameter {
    /// Sent when the second of the two is given after the parameter is
    /// made; one declared with both never sends it.
    #[qml(signal)]
    fn initialized();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        for prop in [Self::NAME.index, Self::VALUE.index] {
            e.watch(me, prop, move |e, _| {
                if e.with(me, Self::is_given).unwrap_or(false) {
                    let _ = e.emit_by_name(me, "initialized", &[]);
                }
            })?;
        }
        Ok(())
    }
}
