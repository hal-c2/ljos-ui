//! A native type and a native list model written against the facade
//! alone, run headlessly: the code `docs/MIGRATION.md` section 6
//! walks through. It is compiled and run by the tests, so the guide
//! cannot drift from it.
//!
//! `cargo run -p ljos --example counter`

use std::cell::RefCell;
use std::rc::Rc;

use ljos::prelude::*;

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
        cx.set(Counter::COUNT, n + 1)
    }
}

pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
}

/// Row titles behind a `QmlListModel`: Qt's `QAbstractListModel`
/// role, as roles and row signals.
#[derive(Debug, Default)]
struct Titles {
    rows: RefCell<Vec<String>>,
}

impl QmlListModel for Titles {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::from("title")]
    }

    fn data(&self, row: usize, role: &str) -> Value {
        match (self.rows.borrow().get(row), role) {
            (Some(s), "title") => Value::from(s.as_str()),
            _ => Value::Undefined,
        }
    }
}

fn main() {
    let engine = ljos::engine();
    register(&engine);

    let counter = engine.create_by_name("Example", "Counter", None).unwrap();
    // In scope for everything created afterwards, as `counter`.
    engine.set_context_property(engine.root_context(), "counter", Value::Object(counter));
    engine.invoke_by_name(counter, "increment", &[]).unwrap();
    assert_eq!(engine.get(counter, Counter::COUNT), Ok(1));

    let titles = Rc::new(Titles::default());
    let holder = engine.create_native(QtObject::default(), None).unwrap();
    let signals = engine
        .set_list_model(holder, Rc::clone(&titles) as Rc<dyn QmlListModel>)
        .unwrap();
    let at = titles.rows.borrow().len();
    signals.rows_about_to_be_inserted(&engine, at, 1);
    titles.rows.borrow_mut().push("First".to_owned());
    signals.rows_inserted(&engine, at, 1);
    assert_eq!(titles.data(0, "title"), Value::from("First"));

    println!("counter is at 1 and the model has its first row");
}
