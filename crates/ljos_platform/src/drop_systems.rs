//! What macOS and Windows call a drag's formats and actions, in the terms
//! of [`DragData`](crate::DragData) and [`DragActions`]. They are plain
//! mappings, kept apart from the hosts that call them so that they are
//! tested on every platform.

use crate::drop::{DragActions, DropAction, DropActions};

/// AppKit: `NSDragOperation` and the types of an `NSPasteboard`.
pub mod appkit {
    use super::{DragActions, DropAction, DropActions};

    pub const COPY: u64 = 1;
    pub const LINK: u64 = 2;
    pub const GENERIC: u64 = 4;
    pub const PRIVATE: u64 = 8;
    pub const MOVE: u64 = 16;
    pub const DELETE: u64 = 32;

    /// The types a window takes drags of. AppKit sends a drag to a view
    /// only when one of the types it carries is among these, by name.
    pub const TAKEN: &[&str] = &[
        "public.file-url",
        "public.url",
        "public.utf8-plain-text",
        "public.utf16-plain-text",
        "public.utf16-external-plain-text",
        "public.plain-text",
        "public.text",
        "public.html",
        "public.rtf",
        "public.png",
        "public.jpeg",
        "public.tiff",
        "public.svg-image",
        "public.json",
        "public.xml",
        "com.adobe.pdf",
        "NSFilenamesPboardType",
        "NSStringPboardType",
        "public.data",
        "public.content",
        "public.item",
    ];

    /// The actions a source's operation mask allows, and the one it would
    /// like. AppKit narrows the mask to what the user's modifiers ask
    /// (Option: copy; Command: generic, which is a move; both: link), so a
    /// mask with copy in it proposes a copy, else a move, else a link.
    /// `None`: it allows none of the three.
    pub fn actions(mask: u64) -> Option<DragActions> {
        let allowed: DropActions = [
            (COPY, DropAction::Copy),
            (MOVE | GENERIC, DropAction::Move),
            (LINK, DropAction::Link),
        ]
        .into_iter()
        .filter(|&(bits, _)| mask & bits != 0)
        .map(|(_, action)| action)
        .collect();
        let proposed = allowed.iter().next()?;
        Some(DragActions { allowed, proposed })
    }

    /// The operation a target that takes the drag with `action` answers
    /// a source that allows `mask`: a move is `NSDragOperationMove`, or
    /// generic when the source allows only that.
    pub fn operation(action: Option<DropAction>, mask: u64) -> u64 {
        match action {
            None => 0,
            Some(DropAction::Copy) => COPY,
            Some(DropAction::Link) => LINK,
            Some(DropAction::Move) if mask & MOVE == 0 && mask & GENERIC != 0 => GENERIC,
            Some(DropAction::Move) => MOVE,
        }
    }

    /// The MIME type a pasteboard type is offered as. A type already
    /// written as a MIME type is itself, a uniform type identifier with
    /// no MIME type is offered by its own name, and `None` is a type
    /// not offered: a dynamic one (`dyn.…`), which names no format a
    /// program could ask for, and the old file-name list, which says
    /// again what `public.file-url` says.
    pub fn mime_of(kind: &str) -> Option<&str> {
        Some(match kind {
            "public.file-url" | "public.url" => "text/uri-list",
            "public.utf8-plain-text"
            | "public.utf16-plain-text"
            | "public.utf16-external-plain-text"
            | "public.plain-text"
            | "NSStringPboardType" => "text/plain",
            "public.html" | "Apple HTML pasteboard type" => "text/html",
            "public.rtf" | "NeXT Rich Text Format v1.0 pasteboard type" => "text/rtf",
            "public.png" => "image/png",
            "public.jpeg" => "image/jpeg",
            "public.tiff" | "NeXT TIFF v4.0 pasteboard type" => "image/tiff",
            "public.svg-image" => "image/svg+xml",
            "public.json" => "application/json",
            "public.xml" => "application/xml",
            "com.adobe.pdf" => "application/pdf",
            "NSFilenamesPboardType" => return None,
            _ if kind.starts_with("dyn.") => return None,
            _ => kind,
        })
    }

    /// Whether a pasteboard type is read as a string (`stringForType:`,
    /// which AppKit converts from any encoding) rather than as bytes.
    pub fn is_text(kind: &str) -> bool {
        matches!(
            mime_of(kind),
            Some("text/uri-list" | "text/plain" | "text/html")
        )
    }
}

/// Win32: `DROPEFFECT`, the key state OLE passes, and clipboard formats.
pub mod win32 {
    use std::path::PathBuf;

    use super::{DragActions, DropAction, DropActions};

    pub const DROPEFFECT_NONE: u32 = 0;
    pub const DROPEFFECT_COPY: u32 = 1;
    pub const DROPEFFECT_MOVE: u32 = 2;
    pub const DROPEFFECT_LINK: u32 = 4;

    pub const MK_SHIFT: u32 = 4;
    pub const MK_CONTROL: u32 = 8;
    pub const MK_ALT: u32 = 32;

    pub const CF_TEXT: u32 = 1;
    pub const CF_OEMTEXT: u32 = 7;
    pub const CF_UNICODETEXT: u32 = 13;
    pub const CF_HDROP: u32 = 15;

    /// The actions a source's effects allow, and the one the user's keys
    /// ask for: Control a copy, Shift a move, both or Alt a link, as the
    /// Windows shell has them; no key a copy, else what the source allows
    /// first. `None`: it allows none.
    pub fn actions(effects: u32, keys: u32) -> Option<DragActions> {
        let allowed: DropActions = [
            (DROPEFFECT_COPY, DropAction::Copy),
            (DROPEFFECT_MOVE, DropAction::Move),
            (DROPEFFECT_LINK, DropAction::Link),
        ]
        .into_iter()
        .filter(|&(bit, _)| effects & bit != 0)
        .map(|(_, action)| action)
        .collect();
        let first = allowed.iter().next()?;
        let asked = match (keys & MK_CONTROL != 0, keys & MK_SHIFT != 0) {
            (true, true) => DropAction::Link,
            _ if keys & MK_ALT != 0 => DropAction::Link,
            (true, false) => DropAction::Copy,
            (false, true) => DropAction::Move,
            (false, false) => DropAction::Copy,
        };
        let proposed = if allowed.contains(asked) {
            asked
        } else {
            first
        };
        Some(DragActions { allowed, proposed })
    }

    /// The effect a target that takes the drag with `action` answers.
    pub fn effect(action: Option<DropAction>) -> u32 {
        match action {
            None => DROPEFFECT_NONE,
            Some(DropAction::Copy) => DROPEFFECT_COPY,
            Some(DropAction::Move) => DROPEFFECT_MOVE,
            Some(DropAction::Link) => DROPEFFECT_LINK,
        }
    }

    /// The MIME type a clipboard format is offered as: `format` is its
    /// number and `name` the name a registered one has. A registered name
    /// written as a MIME type is itself, another registered name is
    /// offered by its own name, and `None` is a format not offered: one of
    /// the system's own (a bitmap, a palette, a locale) with no MIME type,
    /// or one of the shell's that describe a drag rather than carry it.
    pub fn mime_of(format: u32, name: Option<&str>) -> Option<String> {
        let mime = match (format, name) {
            (CF_UNICODETEXT | CF_TEXT | CF_OEMTEXT, _) => "text/plain",
            (CF_HDROP, _) => "text/uri-list",
            (_, Some("HTML Format")) => "text/html",
            (_, Some("UniformResourceLocatorW" | "UniformResourceLocator")) => "text/uri-list",
            (_, Some("Rich Text Format")) => "text/rtf",
            (_, Some("PNG")) => "image/png",
            (_, Some(n)) if n.starts_with("Shell ") || SHELL_OWN.contains(&n) => return None,
            (_, Some(n)) => n,
            (_, None) => return None,
        };
        Some(mime.to_owned())
    }

    /// The shell's formats that say how a drag goes, not what it carries.
    const SHELL_OWN: &[&str] = &[
        "Preferred DropEffect",
        "Performed DropEffect",
        "Paste Succeeded",
        "DragContext",
        "DragImageBits",
        "DragSourceHelperFlags",
        "DisableDragText",
        "IsShowingLayered",
        "IsShowingText",
        "IsComputingImage",
        "UsingDefaultDragImage",
        "DropDescription",
        "FileName",
        "FileNameW",
        "FileGroupDescriptor",
        "FileGroupDescriptorW",
        "FileContents",
    ];

    /// The order formats are read in, so that the first text that wins
    /// is the Unicode one (a source may offer the same text as the code
    /// page's too).
    pub fn rank(format: u32) -> u8 {
        match format {
            CF_UNICODETEXT => 0,
            CF_TEXT => 1,
            CF_OEMTEXT => 2,
            _ => 0,
        }
    }

    /// The bytes a format's global memory holds, as UTF-8 text where the
    /// format is text: `CF_UNICODETEXT` and `UniformResourceLocatorW` are
    /// UTF-16, `HTML Format` (CF_HTML) is the fragment its header points
    /// at, and a URL format is one address, which a list of one is.
    pub fn utf8_of(format: u32, name: Option<&str>, bytes: &[u8]) -> Vec<u8> {
        match (format, name) {
            (CF_UNICODETEXT, _) | (_, Some("UniformResourceLocatorW")) => utf16(bytes).into_bytes(),
            (_, Some("HTML Format")) => html_fragment(bytes).into_bytes(),
            _ => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                bytes[..end].to_vec()
            }
        }
    }

    /// `CF_HDROP`'s paths as a `text/uri-list`.
    pub fn uri_list_of(paths: impl IntoIterator<Item = PathBuf>) -> String {
        paths
            .into_iter()
            .map(|p| ljos_engine::encoded_url(&ljos_engine::file_url(&p)))
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    /// Little-endian UTF-16 up to its NUL.
    pub fn utf16(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }

    /// The fragment of a CF_HTML: its header's `StartFragment` and
    /// `EndFragment` are byte offsets into the whole; without them, the
    /// `StartHTML` to `EndHTML` part, and without those the whole.
    pub fn html_fragment(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let bytes = &bytes[..end];
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
        let offset = |key: &str| -> Option<usize> {
            let line = head
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?;
            line.trim().parse::<usize>().ok()
        };
        let part = |from: &str, to: &str| {
            let (a, b) = (offset(from)?, offset(to)?);
            (a <= b && b <= bytes.len()).then(|| &bytes[a..b])
        };
        let body = part("StartFragment", "EndFragment")
            .or_else(|| part("StartHTML", "EndHTML"))
            .unwrap_or(bytes);
        String::from_utf8_lossy(body).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::DragData;

    fn names(actions: DropActions) -> Vec<&'static str> {
        actions.iter().map(DropAction::name).collect()
    }

    #[test]
    fn appkit_masks_are_qt_actions() {
        let all = appkit::actions(u64::MAX).unwrap();
        assert_eq!(names(all.allowed), ["copy", "move", "link"]);
        assert_eq!(all.proposed, DropAction::Copy);
        // Command held: generic alone, a move.
        let generic = appkit::actions(appkit::GENERIC).unwrap();
        assert_eq!(names(generic.allowed), ["move"]);
        assert_eq!(generic.proposed, DropAction::Move);
        let link = appkit::actions(appkit::LINK).unwrap();
        assert_eq!(link.proposed, DropAction::Link);
        assert_eq!(
            appkit::actions(appkit::PRIVATE | appkit::DELETE),
            None,
            "a mask of none of the three is no drop"
        );
    }

    #[test]
    fn appkit_is_answered_with_an_operation_the_source_allows() {
        let all = appkit::COPY | appkit::LINK | appkit::GENERIC | appkit::MOVE;
        assert_eq!(appkit::operation(None, all), 0);
        assert_eq!(appkit::operation(Some(DropAction::Copy), all), appkit::COPY);
        assert_eq!(appkit::operation(Some(DropAction::Link), all), appkit::LINK);
        assert_eq!(appkit::operation(Some(DropAction::Move), all), appkit::MOVE);
        assert_eq!(
            appkit::operation(Some(DropAction::Move), appkit::GENERIC),
            appkit::GENERIC
        );
    }

    #[test]
    fn pasteboard_types_are_offered_as_mime_types() {
        let offered = [
            "public.file-url",
            "NSFilenamesPboardType",
            "public.utf8-plain-text",
            "public.html",
            "dyn.ah62d4rv4gu8y",
            "com.apple.webarchive",
            "application/x-color",
        ];
        let mimes: Vec<&str> = offered.iter().filter_map(|t| appkit::mime_of(t)).collect();
        assert_eq!(
            DragData::formats_of(mimes),
            [
                "text/uri-list",
                "text/plain",
                "text/html",
                "com.apple.webarchive",
                "application/x-color"
            ]
        );
        assert!(appkit::is_text("public.file-url"));
        assert!(!appkit::is_text("public.png"));
        for kind in appkit::TAKEN {
            assert!(
                appkit::mime_of(kind).is_some() || *kind == "NSFilenamesPboardType",
                "{kind}"
            );
        }
    }

    #[test]
    fn win32_effects_and_keys_are_qt_actions() {
        let all = win32::DROPEFFECT_COPY | win32::DROPEFFECT_MOVE | win32::DROPEFFECT_LINK;
        let plain = win32::actions(all, 0).unwrap();
        assert_eq!(names(plain.allowed), ["copy", "move", "link"]);
        assert_eq!(plain.proposed, DropAction::Copy);
        assert_eq!(
            win32::actions(all, win32::MK_SHIFT).unwrap().proposed,
            DropAction::Move
        );
        assert_eq!(
            win32::actions(all, win32::MK_CONTROL).unwrap().proposed,
            DropAction::Copy
        );
        assert_eq!(
            win32::actions(all, win32::MK_CONTROL | win32::MK_SHIFT)
                .unwrap()
                .proposed,
            DropAction::Link
        );
        assert_eq!(
            win32::actions(all, win32::MK_ALT).unwrap().proposed,
            DropAction::Link
        );
        // A key asking for what the source does not allow: its first.
        let moves = win32::actions(win32::DROPEFFECT_MOVE, win32::MK_CONTROL).unwrap();
        assert_eq!(moves.proposed, DropAction::Move);
        assert_eq!(win32::actions(win32::DROPEFFECT_NONE, 0), None);
        assert_eq!(
            win32::effect(Some(DropAction::Link)),
            win32::DROPEFFECT_LINK
        );
        assert_eq!(win32::effect(None), win32::DROPEFFECT_NONE);
    }

    #[test]
    fn clipboard_formats_are_offered_as_mime_types() {
        let offered = [
            (win32::CF_HDROP, None),
            (49_000, Some("Shell IDList Array")),
            (49_001, Some("Preferred DropEffect")),
            (win32::CF_UNICODETEXT, None),
            (win32::CF_TEXT, None),
            (49_002, Some("HTML Format")),
            (2, None),
            (49_003, Some("application/x-color")),
        ];
        let mimes: Vec<String> = offered
            .iter()
            .filter_map(|&(f, n)| win32::mime_of(f, n))
            .collect();
        assert_eq!(
            DragData::formats_of(mimes),
            [
                "text/uri-list",
                "text/plain",
                "text/html",
                "application/x-color"
            ]
        );
        assert!(win32::rank(win32::CF_UNICODETEXT) < win32::rank(win32::CF_TEXT));
    }

    #[test]
    fn clipboard_text_is_utf8_once_read() {
        let wide: Vec<u8> = "Þú\0junk"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            win32::utf8_of(win32::CF_UNICODETEXT, None, &wide),
            "Þú".as_bytes()
        );
        assert_eq!(win32::utf8_of(win32::CF_TEXT, None, b"hi\0\0"), b"hi");
        let html = "<html><body><!--StartFragment--><b>x</b><!--EndFragment--></body></html>";
        let head_len = 105;
        let start = head_len + html.find("<b>").unwrap();
        let end = head_len + html.find("<!--EndF").unwrap();
        let header = format!(
            "Version:0.9\r\nStartHTML:{head_len:010}\r\nEndHTML:{:010}\r\nStartFragment:{start:010}\r\nEndFragment:{end:010}\r\n",
            head_len + html.len()
        );
        assert_eq!(header.len(), head_len);
        let cf_html = format!("{header}{html}\0");
        assert_eq!(
            win32::utf8_of(49_002, Some("HTML Format"), cf_html.as_bytes()),
            b"<b>x</b>"
        );
        assert_eq!(win32::html_fragment(b"<p>bare</p>"), "<p>bare</p>");
        let data = DragData::from_formats([(
            "text/uri-list",
            win32::uri_list_of([PathBuf::from("/tmp/a b.txt")]),
        )]);
        assert_eq!(data.urls, ["file:///tmp/a%20b.txt"]);
    }
}
