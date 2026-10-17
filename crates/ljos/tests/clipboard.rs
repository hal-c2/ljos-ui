//! A clipboard write says whether it happened, so a "Copy" command can
//! show "Copied" only when it was. The application's own clipboard always
//! takes a write; a host's service passes on why it did not.

use std::rc::Rc;

use ljos::prelude::*;

/// A host's clipboard that another program holds.
struct Held;

impl ClipboardService for Held {
    fn text(&self, _: ClipboardMode) -> Option<String> {
        None
    }
    fn set_text(&self, _: ClipboardMode, _: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Denied)
    }
    fn html(&self) -> Option<String> {
        None
    }
    fn set_html(&self, _: &str, _: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Denied)
    }
    fn image(&self) -> Option<ClipboardImage> {
        None
    }
    fn set_image(&self, _: ClipboardImage) -> Result<(), ClipboardError> {
        Err(ClipboardError::Denied)
    }
    fn clear(&self, _: ClipboardMode) -> Result<(), ClipboardError> {
        Err(ClipboardError::Other("held".into()))
    }
}

#[test]
fn the_application_s_own_clipboard_takes_every_write() {
    let engine = ljos::engine();
    let clipboard = Platform::of(&engine).clipboard();
    assert_eq!(clipboard.set_text(ClipboardMode::Clipboard, "a"), Ok(()));
    assert_eq!(clipboard.set_html("<b>a</b>", "a"), Ok(()));
    assert_eq!(clipboard.clear(ClipboardMode::Clipboard), Ok(()));
}

#[test]
fn a_write_the_host_refused_says_why() {
    let engine = ljos::engine();
    Platform::of(&engine).set_clipboard(Rc::new(Held));
    let clipboard = Platform::of(&engine).clipboard();
    let copied = clipboard.set_text(ClipboardMode::Clipboard, "a");
    assert_eq!(copied, Err(ClipboardError::Denied));
    assert_eq!(
        clipboard.clear(ClipboardMode::Clipboard),
        Err(ClipboardError::Other("held".into()))
    );
    assert_eq!(
        ClipboardError::Unavailable.to_string(),
        "there is no clipboard to write to"
    );
}
