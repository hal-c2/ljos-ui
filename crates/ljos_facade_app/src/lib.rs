//! An application's native module, written as `docs/MIGRATION.md`
//! section 6 tells an application to write one: `ljos` is its only
//! dependency and `use ljos::prelude::*;` its only import of Ljós. It
//! builds with the workspace, so the macros' generated paths, the
//! prelude and the facade's manifest (no `[patch]` to copy) stay
//! enough for a crate outside this repository.

use ljos::prelude::*;

/// A native type with a property, a signal and an invokable method.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)?;
        Counter::emit_bumped(cx)
    }
}

/// A native singleton: one object per engine, made when a script first
/// names it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Greeter")]
#[qml_singleton]
pub struct Greeter {
    #[qml(notify)]
    pub greeting: Atom,
}

#[qml_impl]
impl Greeter {
    #[qml(invokable)]
    fn greet(&self, name: Atom) -> Atom {
        Atom::from(format!("{} {name}", self.greeting))
    }
}

/// The one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
    engine.register_singleton::<Greeter>("Example", Version::new(1, 0));
}
