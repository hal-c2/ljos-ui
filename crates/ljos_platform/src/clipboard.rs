//! The clipboard as a service: text, HTML, an image, files, and on Linux
//! the primary selection (what is selected, pasted with the middle button).
//!
//! Without a host's service the clipboard is one of the application's own
//! ([`MemoryClipboard`]), which is what tests and headless hosts want.

use std::cell::RefCell;
use std::path::PathBuf;

use ljos_engine::file_url;

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

    /// The files on the clipboard, as a file manager copies them: the
    /// system's list of files (`text/uri-list` on X11 and Wayland,
    /// `public.file-url` on macOS, `CF_HDROP` on Windows). Empty when it
    /// holds none, or for a service that keeps no files.
    fn files(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Copies `files`, so that a file manager pastes them.
    fn set_files(&self, files: &[PathBuf]) -> Result<(), ClipboardError> {
        let _ = files;
        Err(ClipboardError::Unavailable)
    }

    /// The `file://` URLs of [`ClipboardService::files`], as Qt's
    /// `QMimeData::urls()` gives them.
    fn urls(&self) -> Vec<String> {
        self.files().iter().map(|p| file_url(p)).collect()
    }

    /// Whether the clipboard holds files (`QMimeData::hasUrls()`).
    fn has_urls(&self) -> bool {
        !self.files().is_empty()
    }

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
    files: Vec<PathBuf>,
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
            ..Held::default()
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

    fn files(&self) -> Vec<PathBuf> {
        self.clipboard.borrow().files.clone()
    }

    fn set_files(&self, files: &[PathBuf]) -> Result<(), ClipboardError> {
        *self.clipboard.borrow_mut() = Held {
            files: files.to_vec(),
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
    fn copied_files_read_as_file_urls_until_something_else_is_copied() {
        let c = MemoryClipboard::default();
        assert!(!c.has_urls());
        let files = [
            PathBuf::from("/home/user/rec.mov"),
            PathBuf::from("/tmp/a b#1"),
        ];
        c.set_files(&files).unwrap();
        assert!(c.has_urls());
        assert_eq!(c.files(), files);
        assert_eq!(
            c.urls(),
            ["file:///home/user/rec.mov", "file:///tmp/a b%231"]
        );
        assert_eq!(c.text(ClipboardMode::Clipboard), None);
        c.set_text(ClipboardMode::Clipboard, "typed").unwrap();
        assert!(!c.has_urls());
    }

    #[test]
    fn a_service_that_keeps_no_files_has_none_and_takes_none() {
        struct Plain;
        impl ClipboardService for Plain {
            fn text(&self, _: ClipboardMode) -> Option<String> {
                None
            }
            fn set_text(&self, _: ClipboardMode, _: &str) -> Result<(), ClipboardError> {
                Ok(())
            }
            fn html(&self) -> Option<String> {
                None
            }
            fn set_html(&self, _: &str, _: &str) -> Result<(), ClipboardError> {
                Ok(())
            }
            fn image(&self) -> Option<ClipboardImage> {
                None
            }
            fn set_image(&self, _: ClipboardImage) -> Result<(), ClipboardError> {
                Ok(())
            }
            fn clear(&self, _: ClipboardMode) -> Result<(), ClipboardError> {
                Ok(())
            }
        }
        assert!(Plain.urls().is_empty());
        assert_eq!(
            Plain.set_files(&[PathBuf::from("/a")]),
            Err(ClipboardError::Unavailable)
        );
    }

    #[test]
    fn a_write_to_the_application_s_own_clipboard_always_happens() {
        let c = MemoryClipboard::default();
        assert_eq!(c.set_text(ClipboardMode::Selection, "a"), Ok(()));
        assert_eq!(c.set_html("<b>a</b>", "a"), Ok(()));
        assert_eq!(c.clear(ClipboardMode::Clipboard), Ok(()));
    }
}
