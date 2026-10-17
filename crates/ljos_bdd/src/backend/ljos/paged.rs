//! A list model that reads its rows a page at a time, each page a while
//! after a view asks for more, as a remote listing or a long log does.
//! Its rows are "row 0", "row 1" and so on.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{Atom, Engine, ItemModel, ModelChange, ModelListener, ModelListeners, Value};

pub(super) struct PagedModel(Rc<Inner>);

struct Inner {
    total: usize,
    page: usize,
    delay: Duration,
    read: Cell<usize>,
    /// A page is on the way: asking again until it comes reads nothing.
    reading: Cell<bool>,
    listeners: ModelListeners,
}

impl PagedModel {
    pub(super) fn new(total: usize, page: usize, delay: Duration) -> Self {
        Self(Rc::new(Inner {
            total,
            page: page.max(1),
            delay,
            read: Cell::new(0),
            reading: Cell::new(false),
            listeners: ModelListeners::default(),
        }))
    }
}

impl ItemModel for PagedModel {
    fn row_count(&self) -> usize {
        self.0.read.get()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::intern("display")]
    }

    fn data(&self, _: &Engine, row: usize, role: &str) -> Value {
        if role == "display" && row < self.0.read.get() {
            Value::from(format!("row {row}").as_str())
        } else {
            Value::Undefined
        }
    }

    fn can_fetch_more(&self) -> bool {
        !self.0.reading.get() && self.0.read.get() < self.0.total
    }

    fn fetch_more(&self, engine: &Engine) {
        if !self.can_fetch_more() {
            return;
        }
        self.0.reading.set(true);
        let me = self.0.clone();
        engine.call_at(engine.now() + self.0.delay, move |engine| {
            let first = me.read.get();
            let count = me.page.min(me.total - first);
            me.read.set(first + count);
            me.reading.set(false);
            me.listeners
                .emit(engine, &ModelChange::RowsInserted { first, count });
        });
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.0.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.0.listeners.remove(id);
    }
}
