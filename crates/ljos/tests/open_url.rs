//! Opening a URL in the user's application for it, from Rust and from
//! QML, through the one service the host installs.

use std::rc::Rc;

use ljos::prelude::*;

/// Takes every URL, as a desktop's opener does, and keeps what it took.
struct Taken(RecordedUrls);

impl OpenUrlService for Taken {
    fn open(&self, url: &str) -> bool {
        self.0.open(url);
        true
    }
}

#[test]
fn rust_and_qml_reach_the_service_the_host_installs() {
    let engine = ljos::engine();
    let record = RecordedUrls::default();
    Platform::of(&engine).set_open_url(Rc::new(Taken(record.clone())));

    assert!(
        Platform::of(&engine)
            .open_url()
            .open("file:///home/user/notes")
    );
    let answer = engine
        .eval_js("Qt.openUrlExternally('mailto:someone@example.com')")
        .expect("Qt.openUrlExternally");
    assert_eq!(answer, Value::Bool(true));
    assert_eq!(
        record.urls(),
        ["file:///home/user/notes", "mailto:someone@example.com"]
    );
}

#[test]
fn without_a_host_service_nothing_is_opened() {
    let engine = ljos::engine();
    let answer = engine
        .eval_js("Qt.openUrlExternally('https://example.com')")
        .expect("Qt.openUrlExternally");
    assert_eq!(answer, Value::Bool(false));
}
