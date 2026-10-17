//! The clipboard as a service: text, HTML, an image, and on Linux the
//! primary selection (what is selected, pasted with the middle button).
//!
//! Without a host's service the clipboard is one of the application's own
//! ([`MemoryClipboard`]), which is what tests and headless hosts want.

use std::cell::RefCell;

/// Which of the system's clipboards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardMode {
    /// What copy and paste use.
    Clipboard,
    /// The X11 and Wayland primary selection. Other systems have none.
    Selection,
}

/// A picture on the clipboard, as unpremultiplied RGBA rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Why a write to the clipboard did not happen.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    /// There is no clipboard to write to: no display, or not this
    /// clipboard on this system (the primary selection where there is
    /// none).
    #[error("there is no clipboard to write to")]
    Unavailable,
    /// The system would not take the write now, such as while another
    /// program holds the clipboard.
    #[error("the system would not take the write")]
    Denied,
    /// Anything else, in the system's words.
    #[error("the clipboard write failed: {0}")]
    Other(String),
}

/// The system's clipboard, or a stand-in. Reads are synchronous: the
/// system answers a clipboard read at once, and a paste cannot wait for a
/// frame anyway.
///
/// A write says whether it happened: `Ok` only when the clipboard took
/// the data. Qt's `QClipboard` writes return nothing; text items and
/// QML's `copy()` ignore the result as Qt's do.
pub trait ClipboardService {
    /// Whether this is the system's clipboard, which the host must not
    /// also be told of (`ljos_quick`'s clipboard hands copies to a host
    /// only when the service is not the system's).
    fn is_system(&self) -> bool {
        false
    }

    fn text(&self, mode: ClipboardMode) -> Option<String>;
    fn set_text(&self, mode: ClipboardMode, text: &str) -> Result<(), ClipboardError>;

    /// HTML that was copied, if the clipboard holds some.
    fn html(&self) -> Option<String>;
    /// Copies HTML; `alt_text` is what a paste into plain text gets.
    fn set_html(&self, html: &str, alt_text: &str) -> Result<(), ClipboardError>;

    fn image(&self) -> Option<ClipboardImage>;
    fn set_image(&self, image: ClipboardImage) -> Result<(), ClipboardError>;

    /// Whether there is a primary selection at all.
    fn has_selection(&self) -> bool {
        cfg!(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))
    }

    fn clear(&self, mode: ClipboardMode) -> Result<(), ClipboardError>;
}

#[derive(Debug, Default)]
struct Held {
    text: Option<String>,
    html: Option<String>,
    image: Option<ClipboardImage>,
}

/// A clipboard inside the application, shared by nothing else.
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    clipboard: RefCell<Held>,
    selection: RefCell<Option<String>>,
}

impl ClipboardService for MemoryClipboard {
    fn text(&self, mode: ClipboardMode) -> Option<String> {
        match mode {
            ClipboardMode::Clipboard => self.clipboard.borrow().text.clone(),
            ClipboardMode::Selection => self.selection.borrow().clone(),
        }
    }

    fn set_text(&self, mode: ClipboardMode, text: &str) -> Result<(), ClipboardError> {
        match mode {
            ClipboardMode::Clipboard => {
                // What is copied replaces all the clipboard held, as the
                // system's does.
                *self.clipboard.borrow_mut() = Held {
                    text: Some(text.to_owned()),
                    ..Held::default()
                };
            }
            ClipboardMode::Selection => *self.selection.borrow_mut() = Some(text.to_owned()),
        }
        Ok(())
    }

    fn html(&self) -> Option<String> {
        self.clipboard.borrow().html.clone()
    }

    fn set_html(&self, html: &str, alt_text: &str) -> Result<(), ClipboardError> {
        *self.clipboard.borrow_mut() = Held {
            text: Some(alt_text.to_owned()),
            html: Some(html.to_owned()),
            image: None,
        };
        Ok(())
    }

    fn image(&self) -> Option<ClipboardImage> {
        self.clipboard.borrow().image.clone()
    }

    fn set_image(&self, image: ClipboardImage) -> Result<(), ClipboardError> {
        *self.clipboard.borrow_mut() = Held {
            image: Some(image),
            ..Held::default()
        };
        Ok(())
    }

    fn clear(&self, mode: ClipboardMode) -> Result<(), ClipboardError> {
        match mode {
            ClipboardMode::Clipboard => *self.clipboard.borrow_mut() = Held::default(),
            ClipboardMode::Selection => *self.selection.borrow_mut() = None,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_replaces_what_the_clipboard_held() {
        let c = MemoryClipboard::default();
        c.set_html("<b>a</b>", "a").unwrap();
        assert_eq!(c.html().as_deref(), Some("<b>a</b>"));
        assert_eq!(c.text(ClipboardMode::Clipboard).as_deref(), Some("a"));
        c.set_text(ClipboardMode::Clipboard, "b").unwrap();
        assert_eq!(c.html(), None);
        c.set_image(ClipboardImage {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        })
        .unwrap();
        assert_eq!(c.text(ClipboardMode::Clipboard), None);
        assert_eq!(c.image().map(|i| i.rgba), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn the_selection_is_a_clipboard_of_its_own() {
        let c = MemoryClipboard::default();
        c.set_text(ClipboardMode::Selection, "picked").unwrap();
        c.set_text(ClipboardMode::Clipboard, "copied").unwrap();
        assert_eq!(c.text(ClipboardMode::Selection).as_deref(), Some("picked"));
        c.clear(ClipboardMode::Selection).unwrap();
        assert_eq!(c.text(ClipboardMode::Selection), None);
        assert_eq!(c.text(ClipboardMode::Clipboard).as_deref(), Some("copied"));
    }

    #[test]
    fn a_write_to_the_application_s_own_clipboard_always_happens() {
        let c = MemoryClipboard::default();
        assert_eq!(c.set_text(ClipboardMode::Selection, "a"), Ok(()));
        assert_eq!(c.set_html("<b>a</b>", "a"), Ok(()));
        assert_eq!(c.clear(ClipboardMode::Clipboard), Ok(()));
    }
}
