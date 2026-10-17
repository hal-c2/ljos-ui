//! The desktop's own services: the system clipboard through arboard,
//! file, folder and message dialogs through rfd (the XDG desktop portal on
//! Linux, the system's panels on macOS and Windows), and the system's
//! opener of URLs.
//!
//! Each dialog waits for its answer on a thread of its own, so the Engine
//! thread never blocks on the user.

use std::cell::RefCell;
use std::path::PathBuf;

use arboard::{Clipboard, ImageData};

use crate::clipboard::{ClipboardError, ClipboardImage, ClipboardMode, ClipboardService};
use crate::dialogs::{
    CANCEL, DialogAnswer, DialogKind, DialogReply, DialogRequest, DialogService, NO, OK, YES,
    system_buttons,
};
use crate::open_url::OpenUrlService;

/// The system clipboard. `None` inside when there is no clipboard to talk
/// to (no display): then it holds nothing and every write is
/// [`ClipboardError::Unavailable`].
pub struct SystemClipboard {
    // Kept for the life of the application: on X11 and Wayland what was
    // copied is served by whoever copied it, so dropping this would empty
    // the clipboard.
    inner: RefCell<Option<Clipboard>>,
}

impl std::fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemClipboard")
            .field("connected", &self.inner.borrow().is_some())
            .finish()
    }
}

impl SystemClipboard {
    /// Connects to the system's clipboard. `None` when there is none.
    pub fn connect() -> Option<SystemClipboard> {
        Clipboard::new().ok().map(|c| SystemClipboard {
            inner: RefCell::new(Some(c)),
        })
    }

    /// A clipboard connected to nothing: what a host without a display
    /// has. It holds nothing and every write is
    /// [`ClipboardError::Unavailable`].
    pub fn disconnected() -> SystemClipboard {
        SystemClipboard {
            inner: RefCell::new(None),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Clipboard) -> Result<T, arboard::Error>) -> Option<T> {
        self.write(f).ok()
    }

    /// As `with`, keeping the system's reason when it fails.
    fn write<T>(
        &self,
        f: impl FnOnce(&mut Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, ClipboardError> {
        match self.inner.borrow_mut().as_mut() {
            Some(c) => f(c).map_err(clipboard_error),
            None => Err(ClipboardError::Unavailable),
        }
    }
}

fn clipboard_error(e: arboard::Error) -> ClipboardError {
    match e {
        arboard::Error::ClipboardNotSupported => ClipboardError::Unavailable,
        arboard::Error::ClipboardOccupied => ClipboardError::Denied,
        other => ClipboardError::Other(other.to_string()),
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod selection {
    use arboard::{ClearExtLinux, Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};

    pub(super) const PRESENT: bool = true;

    pub(super) fn text(c: &mut Clipboard) -> Result<String, arboard::Error> {
        c.get().clipboard(LinuxClipboardKind::Primary).text()
    }

    pub(super) fn set_text(c: &mut Clipboard, text: &str) -> Result<(), arboard::Error> {
        c.set().clipboard(LinuxClipboardKind::Primary).text(text)
    }

    pub(super) fn clear(c: &mut Clipboard) -> Result<(), arboard::Error> {
        c.clear_with().clipboard(LinuxClipboardKind::Primary)
    }
}

#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
mod selection {
    use arboard::Clipboard;

    pub(super) const PRESENT: bool = false;

    pub(super) fn text(_: &mut Clipboard) -> Result<String, arboard::Error> {
        Err(arboard::Error::ClipboardNotSupported)
    }

    pub(super) fn set_text(_: &mut Clipboard, _: &str) -> Result<(), arboard::Error> {
        Err(arboard::Error::ClipboardNotSupported)
    }

    pub(super) fn clear(_: &mut Clipboard) -> Result<(), arboard::Error> {
        Err(arboard::Error::ClipboardNotSupported)
    }
}

impl ClipboardService for SystemClipboard {
    fn is_system(&self) -> bool {
        true
    }

    fn text(&self, mode: ClipboardMode) -> Option<String> {
        match mode {
            ClipboardMode::Clipboard => self.with(Clipboard::get_text),
            ClipboardMode::Selection => self.with(selection::text),
        }
    }

    fn set_text(&self, mode: ClipboardMode, text: &str) -> Result<(), ClipboardError> {
        match mode {
            ClipboardMode::Clipboard => self.write(|c| c.set_text(text)),
            ClipboardMode::Selection => self.write(|c| selection::set_text(c, text)),
        }
    }

    fn html(&self) -> Option<String> {
        self.with(|c| c.get().html())
    }

    fn set_html(&self, html: &str, alt_text: &str) -> Result<(), ClipboardError> {
        self.write(|c| c.set_html(html, Some(alt_text)))
    }

    fn image(&self) -> Option<ClipboardImage> {
        self.with(Clipboard::get_image).map(|i| ClipboardImage {
            width: i.width,
            height: i.height,
            rgba: i.bytes.into_owned(),
        })
    }

    fn set_image(&self, image: ClipboardImage) -> Result<(), ClipboardError> {
        self.write(|c| {
            c.set_image(ImageData {
                width: image.width,
                height: image.height,
                bytes: image.rgba.into(),
            })
        })
    }

    fn has_selection(&self) -> bool {
        selection::PRESENT
    }

    fn clear(&self, mode: ClipboardMode) -> Result<(), ClipboardError> {
        match mode {
            ClipboardMode::Clipboard => self.write(Clipboard::clear),
            ClipboardMode::Selection => self.write(selection::clear),
        }
    }
}

/// The system's file, folder and message dialogs. It has no colour or font
/// chooser, so those are drawn by the application.
#[derive(Debug)]
pub struct SystemDialogs {
    /// Whether the system shows a message. On Linux rfd shows one with
    /// `zenity`, which not every desktop has.
    messages: bool,
}

impl Default for SystemDialogs {
    fn default() -> Self {
        SystemDialogs {
            messages: !cfg!(target_os = "linux") || on_path("zenity"),
        }
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(program).is_file()))
}

impl DialogService for SystemDialogs {
    fn can_show(&self, request: &DialogRequest) -> bool {
        match request.kind {
            Some(DialogKind::Message) => self.messages && system_buttons(request.buttons),
            Some(DialogKind::Color | DialogKind::Font) | None => false,
            Some(_) => true,
        }
    }

    fn show(&self, request: DialogRequest, answer: DialogAnswer) {
        // If the thread cannot start, the answer goes down with it and
        // counts as a dismissal.
        let _ = std::thread::Builder::new()
            .name("ljos-dialog".to_owned())
            .spawn(move || {
                let reply = futures::executor::block_on(ask(request));
                answer.send(reply);
            });
    }

    fn close(&self, _ticket: u64) {
        // rfd cannot take a dialog away. QML has already seen the close
        // (rejected, not visible); the system's dialog (the portal's file
        // chooser, zenity's message, the panel or common dialog elsewhere)
        // stays on screen until the user ends it, and that answer is not
        // heard: the QML dialog has forgotten the ticket. It was shown
        // without a parent window, so it blocks nothing meanwhile.
    }
}

async fn ask(request: DialogRequest) -> DialogReply {
    let Some(kind) = request.kind else {
        return DialogReply::Dismissed;
    };
    if kind == DialogKind::Message {
        return message(request).await;
    }
    let mut dialog = rfd::AsyncFileDialog::new();
    if !request.title.is_empty() {
        dialog = dialog.set_title(&request.title);
    }
    if let Some(folder) = &request.folder {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = &request.file_name {
        dialog = dialog.set_file_name(name);
    }
    for filter in &request.filters {
        let extensions = filter.extensions();
        // A filter of every file is what the dialog shows without one.
        if !extensions.is_empty() {
            dialog = dialog.add_filter(&filter.name, &extensions);
        }
    }
    let one = |h: Option<rfd::FileHandle>| h.map(|h| vec![h.path().to_path_buf()]);
    let chosen: Option<Vec<PathBuf>> = match kind {
        DialogKind::OpenFile => one(dialog.pick_file().await),
        DialogKind::SaveFile => one(dialog.save_file().await),
        DialogKind::Folder => one(dialog.pick_folder().await),
        DialogKind::OpenFiles => dialog
            .pick_files()
            .await
            .map(|hs| hs.iter().map(|h| h.path().to_path_buf()).collect()),
        DialogKind::Message | DialogKind::Color | DialogKind::Font => None,
    };
    match chosen {
        Some(paths) if !paths.is_empty() => DialogReply::Files(paths),
        _ => DialogReply::Dismissed,
    }
}

async fn message(request: DialogRequest) -> DialogReply {
    use rfd::{MessageButtons, MessageDialogResult, MessageLevel};
    let buttons = match request.buttons {
        b if b == OK | CANCEL => MessageButtons::OkCancel,
        b if b == YES | NO => MessageButtons::YesNo,
        b if b == YES | NO | CANCEL => MessageButtons::YesNoCancel,
        _ => MessageButtons::Ok,
    };
    let mut description = request.text.clone();
    for more in [&request.informative_text, &request.detailed_text] {
        if !more.is_empty() {
            description.push_str("\n\n");
            description.push_str(more);
        }
    }
    let result = rfd::AsyncMessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(&request.title)
        .set_description(description)
        .set_buttons(buttons)
        .show()
        .await;
    match result {
        MessageDialogResult::Ok => DialogReply::Button(OK),
        MessageDialogResult::Yes => DialogReply::Button(YES),
        MessageDialogResult::No => DialogReply::Button(NO),
        MessageDialogResult::Cancel => DialogReply::Button(CANCEL),
        MessageDialogResult::Custom(_) => DialogReply::Dismissed,
    }
}

/// Opens a URL with the system's own opener: `xdg-open` on Linux and the
/// BSDs, `open` on macOS, the URL protocol handler on Windows. Android and
/// iOS have no such program; there it opens nothing yet
/// ([`SystemOpener::AVAILABLE`] is false).
#[derive(Debug, Default)]
pub struct SystemOpener;

impl SystemOpener {
    /// Whether this system has an opener to start.
    pub const AVAILABLE: bool = !cfg!(any(target_os = "android", target_os = "ios"));
}

impl OpenUrlService for SystemOpener {
    fn open(&self, url: &str) -> bool {
        // An opener would read a leading dash as an option of its own.
        if !Self::AVAILABLE || url.starts_with('-') {
            return false;
        }
        let mut command = if cfg!(target_os = "windows") {
            let mut c = std::process::Command::new("rundll32");
            c.arg("url.dll,FileProtocolHandler");
            c
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open")
        } else {
            std::process::Command::new("xdg-open")
        };
        // The opener is another program: hand it the strict form.
        command
            .arg(ljos_engine::encoded_url(url))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match command.spawn() {
            Ok(mut child) => {
                // Starting it is all the Engine thread waits for. Some
                // openers stay until the application they start quits, so
                // a thread of its own collects the exit.
                let _ = std::thread::Builder::new()
                    .name("ljos-open-url".to_owned())
                    .spawn(move || child.wait());
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clipboard_connected_to_nothing_says_so_on_every_write() {
        let c = SystemClipboard::disconnected();
        let unavailable = Err(ClipboardError::Unavailable);
        assert_eq!(c.set_text(ClipboardMode::Clipboard, "a"), unavailable);
        assert_eq!(c.set_text(ClipboardMode::Selection, "a"), unavailable);
        assert_eq!(c.set_html("<b>a</b>", "a"), unavailable);
        let image = ClipboardImage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        assert_eq!(c.set_image(image), unavailable);
        assert_eq!(c.clear(ClipboardMode::Clipboard), unavailable);
        assert_eq!(c.text(ClipboardMode::Clipboard), None);
    }

    #[test]
    fn the_system_s_reasons_keep_their_kind() {
        assert_eq!(
            clipboard_error(arboard::Error::ClipboardNotSupported),
            ClipboardError::Unavailable
        );
        assert_eq!(
            clipboard_error(arboard::Error::ClipboardOccupied),
            ClipboardError::Denied
        );
        assert!(matches!(
            clipboard_error(arboard::Error::ConversionFailure),
            ClipboardError::Other(_)
        ));
    }
}
