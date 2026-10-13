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

/// The system's clipboard, or a stand-in. Reads are synchronous: the
/// system answers a clipboard read at once, and a paste cannot wait for a
/// frame anyway.
pub trait ClipboardService {
    /// Whether this is the system's clipboard, which the host must not
    /// also be told of (`ljos_quick`'s clipboard hands copies to a host
    /// only when the service is not the system's).
    fn is_system(&self) -> bool {
        false
    }

    fn text(&self, mode: ClipboardMode) -> Option<String>;
    fn set_text(&self, mode: ClipboardMode, text: &str);

    /// HTML that was copied, if the clipboard holds some.
    fn html(&self) -> Option<String>;
    /// Copies HTML; `alt_text` is what a paste into plain text gets.
    fn set_html(&self, html: &str, alt_text: &str);

    fn image(&self) -> Option<ClipboardImage>;
    fn set_image(&self, image: ClipboardImage);

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

    fn clear(&self, mode: ClipboardMode);
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

    fn set_text(&self, mode: ClipboardMode, text: &str) {
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
    }

    fn html(&self) -> Option<String> {
        self.clipboard.borrow().html.clone()
    }

    fn set_html(&self, html: &str, alt_text: &str) {
        *self.clipboard.borrow_mut() = Held {
            text: Some(alt_text.to_owned()),
            html: Some(html.to_owned()),
            image: None,
        };
    }

    fn image(&self) -> Option<ClipboardImage> {
        self.clipboard.borrow().image.clone()
    }

    fn set_image(&self, image: ClipboardImage) {
        *self.clipboard.borrow_mut() = Held {
            image: Some(image),
            ..Held::default()
        };
    }

    fn clear(&self, mode: ClipboardMode) {
        match mode {
            ClipboardMode::Clipboard => *self.clipboard.borrow_mut() = Held::default(),
            ClipboardMode::Selection => *self.selection.borrow_mut() = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_replaces_what_the_clipboard_held() {
        let c = MemoryClipboard::default();
        c.set_html("<b>a</b>", "a");
        assert_eq!(c.html().as_deref(), Some("<b>a</b>"));
        assert_eq!(c.text(ClipboardMode::Clipboard).as_deref(), Some("a"));
        c.set_text(ClipboardMode::Clipboard, "b");
        assert_eq!(c.html(), None);
        c.set_image(ClipboardImage {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        });
        assert_eq!(c.text(ClipboardMode::Clipboard), None);
        assert_eq!(c.image().map(|i| i.rgba), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn the_selection_is_a_clipboard_of_its_own() {
        let c = MemoryClipboard::default();
        c.set_text(ClipboardMode::Selection, "picked");
        c.set_text(ClipboardMode::Clipboard, "copied");
        assert_eq!(c.text(ClipboardMode::Selection).as_deref(), Some("picked"));
        c.clear(ClipboardMode::Selection);
        assert_eq!(c.text(ClipboardMode::Selection), None);
        assert_eq!(c.text(ClipboardMode::Clipboard).as_deref(), Some("copied"));
    }
}
