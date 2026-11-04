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

    /// The pasteboard type a MIME type goes out as, the other way from
    /// [`mime_of`]: the uniform type identifier AppKit's applications ask
    /// for where there is one, else the MIME type under its own name, which
    /// a Ljós target (and any that asks for it by name) reads back as
    /// itself. `text/uri-list` goes out an address an item
    /// ([`url_kind`]).
    pub fn kind_of(mime: &str) -> &str {
        match mime {
            "text/plain" => "public.utf8-plain-text",
            "text/html" => "public.html",
            "text/rtf" => "public.rtf",
            "image/png" => "public.png",
            "image/jpeg" => "public.jpeg",
            "image/tiff" => "public.tiff",
            "image/svg+xml" => "public.svg-image",
            "application/json" => "public.json",
            "application/xml" => "public.xml",
            "application/pdf" => "com.adobe.pdf",
            _ => mime,
        }
    }

    /// The pasteboard type of one address: a file's or any other.
    pub fn url_kind(url: &str) -> &'static str {
        if url.starts_with("file:") {
            "public.file-url"
        } else {
            "public.url"
        }
    }

    /// The pasteboard items a drag out of the application writes: one for
    /// each address, as Finder writes files, and the drag's other formats
    /// on the first, as their data is one thing. Each is its types with
    /// the text that goes under each; formats are text in Ljós, so every
    /// one is written as a string.
    pub fn items(data: &crate::DragData) -> Vec<Vec<(String, String)>> {
        let mut items: Vec<Vec<(String, String)>> = data
            .urls
            .iter()
            .map(|url| {
                let url = ljos_engine::encoded_url(url);
                vec![(url_kind(&url).to_owned(), url)]
            })
            .collect();
        let rest: Vec<(String, String)> = data
            .formats()
            .into_iter()
            .filter(|f| f != "text/uri-list")
            .filter_map(|f| {
                let text = data.data(&f)?;
                Some((kind_of(&f).to_owned(), text))
            })
            .collect();
        if items.is_empty() {
            if !rest.is_empty() {
                items.push(rest);
            }
        } else {
            items[0].extend(rest);
        }
        items
    }

    /// The operations a drag out of the application allows, from its
    /// `supportedActions`: a move is also generic, the operation Finder
    /// and others take for a move. A drag that allows none of the three
    /// is a copy, as on Wayland and X11.
    pub fn mask_of(allowed: DropActions) -> u64 {
        let mask = allowed.iter().fold(0, |mask, action| {
            mask | match action {
                DropAction::Copy => COPY,
                DropAction::Move => MOVE | GENERIC,
                DropAction::Link => LINK,
            }
        });
        if mask == 0 { COPY } else { mask }
    }

    /// The action of the operation a drag out ended with
    /// (`draggingSession:endedAtPoint:operation:`): `None` for none. A
    /// generic operation is a move, and so is delete, which the Trash
    /// answers: the source's copy is to go either way.
    pub fn action_of(operation: u64) -> Option<DropAction> {
        if operation & COPY != 0 {
            Some(DropAction::Copy)
        } else if operation & (MOVE | GENERIC | DELETE) != 0 {
            Some(DropAction::Move)
        } else if operation & LINK != 0 {
            Some(DropAction::Link)
        } else {
            None
        }
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

    pub const MK_LBUTTON: u32 = 1;
    pub const MK_RBUTTON: u32 = 2;
    pub const MK_MBUTTON: u32 = 16;

    /// A format a drag out of the application offers: one of the
    /// system's by number, or one registered by name
    /// (`RegisterClipboardFormatW`).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Format {
        System(u32),
        Named(String),
    }

    /// What a drag out of the application offers, each format with the
    /// bytes its global memory holds, in the order a target should take
    /// them: `CF_HDROP` for its files, `UniformResourceLocatorW` for its
    /// first other address (the format holds one), `CF_UNICODETEXT` for
    /// its text, `HTML Format` (CF_HTML) for its HTML, and every other
    /// format registered under its MIME type, which a Ljós target reads
    /// back as itself. An address list with files and other addresses
    /// both goes out as the files and the first other one.
    pub fn offered(data: &crate::DragData) -> Vec<(Format, Vec<u8>)> {
        let mut out = Vec::new();
        let files: Vec<String> = data.urls.iter().filter_map(|u| path_of(u)).collect();
        if !files.is_empty() {
            out.push((Format::System(CF_HDROP), hdrop(&files)));
        }
        if let Some(url) = data.urls.iter().find(|u| path_of(u).is_none()) {
            out.push((
                Format::Named("UniformResourceLocatorW".to_owned()),
                wide(&ljos_engine::encoded_url(url)),
            ));
        }
        if let Some(text) = &data.text {
            out.push((Format::System(CF_UNICODETEXT), wide(text)));
        }
        if let Some(html) = &data.html {
            out.push((Format::Named("HTML Format".to_owned()), cf_html(html)));
        }
        for (mime, text) in &data.other {
            let mut bytes = text.clone().into_bytes();
            bytes.push(0);
            out.push((Format::Named(mime.clone()), bytes));
        }
        out
    }

    /// Little-endian UTF-16 with its NUL, as `CF_UNICODETEXT` holds text.
    pub fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    /// The Windows path of a `file:` URL: `file:///C:/a%20b` (and
    /// `file://C:/a%20b`) is `C:\a b` and `file://host/share/x` is
    /// `\\host\share\x`. `None` for any
    /// other address, or a file URL that names no drive or host.
    pub fn path_of(url: &str) -> Option<String> {
        let rest = url.strip_prefix("file://")?;
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        // `file://C:/x`, which some applications write, has its drive
        // where the host would be.
        let bytes = rest.as_bytes();
        let drive_first = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        let (host, path) = match rest.strip_prefix('/') {
            Some(_) => ("", rest.to_owned()),
            None if drive_first => ("", format!("/{rest}")),
            None => {
                let (host, path) = rest.split_at(rest.find('/')?);
                (host, path.to_owned())
            }
        };
        let decoded = ljos_engine::url_path(&format!("file://{path}"))?;
        let decoded = decoded.to_string_lossy().replace('/', "\\");
        if host.is_empty() {
            let drive = decoded.as_bytes();
            (drive.len() >= 2 && drive[0].is_ascii_alphabetic() && drive[1] == b':')
                .then_some(decoded)
        } else {
            Some(format!("\\\\{host}{decoded}"))
        }
    }

    /// `CF_HDROP`'s global memory for `paths`: a `DROPFILES` (the offset
    /// of the list, a point, whether it is in client space, whether the
    /// paths are wide), then each path in UTF-16 with its NUL, then one
    /// more NUL.
    pub fn hdrop(paths: &[String]) -> Vec<u8> {
        const DROPFILES: u32 = 20;
        let mut out = Vec::new();
        out.extend(DROPFILES.to_le_bytes());
        out.extend([0u8; 8]);
        out.extend(0u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        for path in paths {
            out.extend(wide(path));
        }
        out.extend([0, 0]);
        out
    }

    /// CF_HTML for `fragment`: the header whose offsets say where the
    /// document and the fragment are, in bytes, as
    /// [`html_fragment`] reads it, then the document, with its NUL.
    pub fn cf_html(fragment: &str) -> Vec<u8> {
        const BEFORE: &str = "<html><body>\r\n<!--StartFragment-->";
        const AFTER: &str = "<!--EndFragment-->\r\n</body></html>";
        let header = |a: usize, b: usize, c: usize, d: usize| {
            format!(
                "Version:0.9\r\nStartHTML:{a:010}\r\nEndHTML:{b:010}\r\nStartFragment:{c:010}\r\nEndFragment:{d:010}\r\n"
            )
        };
        let head = header(0, 0, 0, 0).len();
        let start = head + BEFORE.len();
        let end = start + fragment.len();
        let total = end + AFTER.len();
        let mut out = header(head, total, start, end).into_bytes();
        out.extend(BEFORE.as_bytes());
        out.extend(fragment.as_bytes());
        out.extend(AFTER.as_bytes());
        out.push(0);
        out
    }

    /// The effects a drag out of the application allows, from its
    /// `supportedActions`; one that allows none is a copy, as on Wayland
    /// and X11.
    pub fn effects_of(allowed: DropActions) -> u32 {
        let effects = allowed
            .iter()
            .fold(DROPEFFECT_NONE, |all, action| all | effect(Some(action)));
        if effects == DROPEFFECT_NONE {
            DROPEFFECT_COPY
        } else {
            effects
        }
    }

    /// The format a target writes the effect it really performed in
    /// (`IDataObject::SetData`), a DWORD in global memory.
    pub const LOGICAL_PERFORMED: &str = "Logical Performed DropEffect";

    /// The DWORD a target wrote into the data object; `None` when it is
    /// shorter.
    pub fn dword(bytes: &[u8]) -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
    }

    /// The action of a drag out that `DoDragDrop` ended with `effect`,
    /// the target having written `logical` as the effect it performed;
    /// `None` for none, or a drag given up. Explorer moves a file within
    /// a volume itself (an optimized move) and answers none, so that the
    /// source deletes nothing, and writes the move as the logical effect:
    /// the drag heard none then, and the logical effect is the action.
    pub fn action_of(effect: u32, logical: Option<u32>, dropped: bool) -> Option<DropAction> {
        if !dropped {
            return None;
        }
        let of = |effect: u32| {
            [
                (DROPEFFECT_MOVE, DropAction::Move),
                (DROPEFFECT_COPY, DropAction::Copy),
                (DROPEFFECT_LINK, DropAction::Link),
            ]
            .into_iter()
            .find(|&(bit, _)| effect & bit != 0)
            .map(|(_, action)| action)
        };
        of(effect).or_else(|| of(logical?))
    }

    /// What `IDropSource::QueryContinueDrag` answers.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Continue {
        Go,
        Drop,
        Cancel,
    }

    /// Escape, or the application giving the drag up, cancels it; letting
    /// go of every button drops it; else it goes on.
    pub fn continues(escape: bool, keys: u32, given_up: bool) -> Continue {
        if escape || given_up {
            Continue::Cancel
        } else if keys & (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON) == 0 {
            Continue::Drop
        } else {
            Continue::Go
        }
    }

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

    #[test]
    fn a_drag_out_on_appkit_writes_an_item_an_address() {
        let data = DragData::from_formats([
            (
                "text/uri-list",
                "file:///tmp/a b.txt\r\nhttps://example.org/x",
            ),
            ("text/plain", "hello"),
            ("text/html", "<b>hi</b>"),
            ("application/x-example", "42"),
        ]);
        let items = appkit::items(&data);
        let kinds: Vec<Vec<&str>> = items
            .iter()
            .map(|i| i.iter().map(|(k, _)| k.as_str()).collect())
            .collect();
        assert_eq!(
            kinds,
            [
                vec![
                    "public.file-url",
                    "public.utf8-plain-text",
                    "public.html",
                    "application/x-example"
                ],
                vec!["public.url"],
            ]
        );
        assert_eq!(items[0][0].1, "file:///tmp/a%20b.txt");
        assert_eq!(items[1][0].1, "https://example.org/x");
        assert_eq!(items[0][1].1, "hello");
        assert_eq!(appkit::items(&DragData::text("t")).len(), 1);
        assert!(appkit::items(&DragData::default()).is_empty());
        // What goes out comes back in as the MIME types it went as.
        for (kind, _) in items.concat() {
            let back = appkit::mime_of(&kind).unwrap();
            assert!(data.formats().iter().any(|f| f == back), "{kind}");
        }
    }

    #[test]
    fn a_drag_out_on_appkit_allows_and_hears_operations() {
        let copy_move: DropActions = [DropAction::Copy, DropAction::Move].into_iter().collect();
        assert_eq!(
            appkit::mask_of(copy_move),
            appkit::COPY | appkit::MOVE | appkit::GENERIC
        );
        assert_eq!(appkit::mask_of(DropActions::NONE), appkit::COPY);
        assert_eq!(appkit::action_of(0), None);
        assert_eq!(appkit::action_of(appkit::COPY), Some(DropAction::Copy));
        assert_eq!(appkit::action_of(appkit::GENERIC), Some(DropAction::Move));
        assert_eq!(appkit::action_of(appkit::DELETE), Some(DropAction::Move));
        assert_eq!(appkit::action_of(appkit::LINK), Some(DropAction::Link));
        // Ended with the operation a target answered: the action it says.
        let all = appkit::mask_of(DropActions::from_qt(7));
        for action in [DropAction::Copy, DropAction::Move, DropAction::Link] {
            assert_eq!(
                appkit::action_of(appkit::operation(Some(action), all)),
                Some(action)
            );
        }
    }

    #[test]
    fn a_drag_out_on_win32_offers_clipboard_formats() {
        let data = DragData::from_formats([
            (
                "text/uri-list",
                "file:///C:/a%20b.txt\r\nfile://server/share/x\r\nhttps://example.org/\r\nhttps://other/",
            ),
            ("text/plain", "Þú"),
            ("text/html", "<b>x</b>"),
            ("application/x-example", "42"),
        ]);
        let offered = win32::offered(&data);
        let formats: Vec<&win32::Format> = offered.iter().map(|(f, _)| f).collect();
        assert_eq!(
            formats,
            [
                &win32::Format::System(win32::CF_HDROP),
                &win32::Format::Named("UniformResourceLocatorW".into()),
                &win32::Format::System(win32::CF_UNICODETEXT),
                &win32::Format::Named("HTML Format".into()),
                &win32::Format::Named("application/x-example".into()),
            ]
        );
        // Each reads back as it went, through what a target reads.
        let hdrop = &offered[0].1;
        assert_eq!(&hdrop[..4], &20u32.to_le_bytes());
        assert_eq!(&hdrop[16..20], &1u32.to_le_bytes(), "wide");
        let paths: Vec<String> = win32::utf16(&hdrop[20..])
            .split('\0')
            .map(str::to_owned)
            .collect();
        assert_eq!(paths, ["C:\\a b.txt"]);
        let list = String::from_utf16_lossy(
            &hdrop[20..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .collect::<Vec<_>>(),
        );
        assert_eq!(list, "C:\\a b.txt\0\\\\server\\share\\x\0\0");
        let url = &offered[1];
        assert_eq!(
            win32::utf8_of(0, Some("UniformResourceLocatorW"), &url.1),
            b"https://example.org/"
        );
        assert_eq!(
            win32::utf8_of(win32::CF_UNICODETEXT, None, &offered[2].1),
            "Þú".as_bytes()
        );
        assert_eq!(
            win32::utf8_of(0, Some("HTML Format"), &offered[3].1),
            b"<b>x</b>"
        );
        assert_eq!(
            win32::utf8_of(0, Some("application/x-example"), &offered[4].1),
            b"42"
        );
        assert_eq!(
            win32::mime_of(0, Some("application/x-example")).as_deref(),
            Some("application/x-example")
        );
    }

    #[test]
    fn file_urls_are_windows_paths() {
        assert_eq!(
            win32::path_of("file:///C:/x/y%20z").as_deref(),
            Some("C:\\x\\y z")
        );
        assert_eq!(
            win32::path_of("file://localhost/D:/q").as_deref(),
            Some("D:\\q")
        );
        assert_eq!(
            win32::path_of("file://host/share/f").as_deref(),
            Some("\\\\host\\share\\f")
        );
        assert_eq!(
            win32::path_of("file://C:/x/y").as_deref(),
            Some("C:\\x\\y"),
            "a drive where the host would be is the path's"
        );
        assert_eq!(win32::path_of("file:///tmp/unix"), None, "no drive");
        assert_eq!(win32::path_of("https://example.org/"), None);
    }

    #[test]
    fn cf_html_has_its_offsets() {
        let bytes = win32::cf_html("<p>Þ</p>");
        assert_eq!(win32::html_fragment(&bytes), "<p>Þ</p>");
        let text = String::from_utf8(bytes[..bytes.len() - 1].to_vec()).unwrap();
        let offset = |key: &str| -> usize {
            text.lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
                .unwrap()
                .parse()
                .unwrap()
        };
        assert!(text[offset("StartHTML")..].starts_with("<html>"));
        assert_eq!(offset("EndHTML"), text.len());
        assert_eq!(*bytes.last().unwrap(), 0);
    }

    #[test]
    fn a_drag_out_on_win32_allows_and_hears_effects() {
        let copy_move: DropActions = [DropAction::Copy, DropAction::Move].into_iter().collect();
        assert_eq!(
            win32::effects_of(copy_move),
            win32::DROPEFFECT_COPY | win32::DROPEFFECT_MOVE
        );
        assert_eq!(win32::effects_of(DropActions::NONE), win32::DROPEFFECT_COPY);
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_MOVE, None, true),
            Some(DropAction::Move)
        );
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_LINK, None, true),
            Some(DropAction::Link)
        );
        assert_eq!(win32::action_of(win32::DROPEFFECT_NONE, None, true), None);
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_NONE, Some(win32::DROPEFFECT_MOVE), true),
            Some(DropAction::Move),
            "Explorer's optimized move says none and writes the move it did"
        );
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_COPY, Some(win32::DROPEFFECT_MOVE), true),
            Some(DropAction::Copy),
            "the effect DoDragDrop gave wins when it says one"
        );
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_NONE, Some(win32::DROPEFFECT_MOVE), false),
            None
        );
        assert_eq!(win32::dword(&7u32.to_le_bytes()), Some(7));
        assert_eq!(win32::dword(&[1, 2]), None);
        assert_eq!(
            win32::action_of(win32::DROPEFFECT_COPY, None, false),
            None,
            "given up"
        );
        use win32::Continue;
        assert_eq!(
            win32::continues(false, win32::MK_LBUTTON, false),
            Continue::Go
        );
        assert_eq!(win32::continues(false, 0, false), Continue::Drop);
        assert_eq!(
            win32::continues(true, win32::MK_LBUTTON, false),
            Continue::Cancel
        );
        assert_eq!(
            win32::continues(false, win32::MK_LBUTTON, true),
            Continue::Cancel
        );
        assert_eq!(
            win32::continues(false, win32::MK_CONTROL, false),
            Continue::Drop,
            "a key is no button"
        );
    }
}
