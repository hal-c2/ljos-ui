//! A list model that reads its rows a page at a time, as a remote listing
//! or a long log is written: a view that reaches its last row asks for
//! more after it has laid its rows out, once for however many passes
//! reached the end before it asked.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos::prelude::*;
use ljos_runtime::{Clock as _, VirtualClock};

#[derive(Default)]
struct Log {
    total: usize,
    page: usize,
    /// Read the page at once, rather than 100 ms later.
    at_once: bool,
    read: Cell<usize>,
    fetches: Cell<u32>,
    listeners: ModelListeners,
}

/// The model as the engine holds it; the test keeps the `Rc<Log>`.
struct Paged(Rc<Log>);

impl Log {
    fn read_page(&self, engine: &Engine) {
        let first = self.read.get();
        let count = self.page.min(self.total - first);
        self.read.set(first + count);
        let change = ModelChange::RowsInserted { first, count };
        self.listeners.emit(engine, &change);
    }
}

impl ItemModel for Paged {
    fn row_count(&self) -> usize {
        self.0.read.get()
    }

    fn role_names(&self) -> Vec<Atom> {
        vec![Atom::intern("line")]
    }

    fn data(&self, _: &Engine, row: usize, _: &str) -> Value {
        Value::from(format!("line {row}").as_str())
    }

    // Says so even while a page is on the way: the view must not ask
    // again for every pass before it is read.
    fn can_fetch_more(&self) -> bool {
        self.0.read.get() < self.0.total
    }

    fn fetch_more(&self, engine: &Engine) {
        self.0.fetches.set(self.0.fetches.get() + 1);
        if self.0.at_once {
            self.0.read_page(engine);
            return;
        }
        let me = self.0.clone();
        engine.call_at(engine.now() + Duration::from_millis(100), move |engine| {
            me.read_page(engine);
        });
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.0.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.0.listeners.remove(id);
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Lines")]
pub struct Lines {}

#[qml_impl]
impl Lines {}

const MAIN: &str = r#"
import QtQuick
import Example
Item {
    property alias lines: l
    property alias view: lv
    Lines { id: l }
    ListView {
        id: lv; width: 100; height: 100; cacheBuffer: 0
        delegate: Item { width: 100; height: 20; required property string line }
    }
    function show() { lv.model = l }
}
"#;

struct App {
    engine: Engine,
    clock: Arc<VirtualClock>,
    view: ObjectId,
}

impl App {
    fn new(log: &Rc<Log>) -> Self {
        let engine = ljos::engine();
        let clock = Arc::new(VirtualClock::new());
        engine.set_clock(Rc::new(Arc::clone(&clock)));
        engine.register_module(ljos::module!("Example" 1.0 => [Lines]));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
        let source = ljos::dev::compile_dir_with(dir.path(), &engine).unwrap();
        let root = ljos::create_root(&engine, source, "Main").unwrap();
        let lines = engine.read_by_name(root, "lines").as_object().unwrap();
        let view = engine.read_by_name(root, "view").as_object().unwrap();
        engine
            .set_object_model(lines, Some(Rc::new(Paged(log.clone()))))
            .unwrap();
        engine.invoke_by_name(root, "show", &[]).unwrap();
        Self {
            engine,
            clock,
            view,
        }
    }

    fn pass_ms(&self, ms: u64) {
        self.clock.set(self.clock.now() + Duration::from_millis(ms));
        self.engine.advance_timers(self.clock.now());
    }

    fn count(&self) -> i32 {
        self.engine.read_by_name(self.view, "count").to_number() as i32
    }
}

#[test]
fn a_list_view_asks_once_for_however_many_passes_reach_the_end() {
    let log = Rc::new(Log {
        total: 50,
        page: 20,
        ..Log::default()
    });
    let app = App::new(&log);
    // The view asks after its pass, not in it.
    assert_eq!(log.fetches.get(), 0);
    // More passes that reach the end before it asks.
    for h in [120.0, 140.0, 100.0] {
        let h = Value::from(h);
        app.engine.write_by_name(app.view, "height", h).unwrap();
    }
    app.pass_ms(0);
    assert_eq!(log.fetches.get(), 1);
    app.pass_ms(100);
    assert_eq!(app.count(), 20);
    // The view does not reach row 19, so it does not ask.
    app.pass_ms(200);
    assert_eq!((log.fetches.get(), app.count()), (1, 20));

    app.engine
        .invoke_by_name(app.view, "positionViewAtEnd", &[])
        .unwrap();
    app.pass_ms(0);
    assert_eq!(log.fetches.get(), 2);
    app.pass_ms(100);
    assert_eq!(app.count(), 40);
}

#[test]
fn a_model_that_reads_at_once_fills_the_view_and_no_more() {
    let log = Rc::new(Log {
        total: 50,
        page: 2,
        at_once: true,
        ..Log::default()
    });
    let app = App::new(&log);
    for _ in 0..10 {
        app.pass_ms(0);
    }
    // Five rows fill the view; a page more is read only while the last
    // row read is in sight.
    let count = app.count();
    assert!((5..=8).contains(&count), "{count} rows");
    assert_eq!(log.fetches.get() as i32, count / 2);
}
