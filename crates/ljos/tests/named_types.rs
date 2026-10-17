//! Every type a facade method takes or returns is one the facade names,
//! so a crate that depends on `ljos` alone can write it down: keep it in
//! a field, name it in a signature. Each test writes the types down
//! through `ljos::` paths only; a type that leaves the facade fails to
//! compile here.

use std::time::Duration;

use ljos::prelude::*;

/// A debounce keeps its pending timer to cancel it, and when it began.
struct Debounce {
    pending: Option<TimerId>,
    since: Time,
}

#[test]
fn a_debounce_keeps_its_timer_and_its_time() {
    let engine = ljos::engine();
    let mut d = Debounce {
        pending: None,
        since: engine.now(),
    };
    for _ in 0..2 {
        if let Some(id) = d.pending.take() {
            engine.cancel_call(id);
        }
        d.since = engine.now();
        d.pending = Some(engine.call_at(d.since + Duration::from_millis(300), |_| {}));
    }
    assert!(d.pending.is_some());
}

#[test]
fn the_clipboard_is_read_and_written_in_a_named_mode() {
    let engine = ljos::engine();
    let clipboard: std::rc::Rc<dyn ClipboardService> = Platform::of(&engine).clipboard();
    let mode: ClipboardMode = ClipboardMode::Clipboard;
    clipboard.set_text(mode, "copied").unwrap();
    assert_eq!(clipboard.text(mode).as_deref(), Some("copied"));
    let image = ClipboardImage {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 255],
    };
    clipboard.set_image(image).unwrap();
    assert!(clipboard.image().is_some());
}

#[test]
fn the_application_singleton_is_read_by_its_type() {
    let engine = ljos::engine();
    let app = engine.singleton::<Application>().unwrap();
    let state: Result<i32, _> = engine.with(app, |a: &Application| a.state);
    assert!(state.is_ok());
    let _: ApplicationState = ApplicationState::Active;
}

#[test]
fn the_colour_scheme_has_a_facade_spelling() {
    let engine = ljos::engine();
    let scheme: ColorScheme = ljos::color_scheme(&engine);
    assert_ne!(scheme, ColorScheme::Unknown);
}

#[cfg(feature = "shell")]
#[test]
fn a_coalesced_send_is_keyed_by_a_named_key() {
    fn wake(handle: &ljos::UiHandle<()>) -> Result<(), ljos::SendError<()>> {
        handle.send_coalesced(ljos::CoalesceKey(1), ())
    }
    let _ = wake;
}
