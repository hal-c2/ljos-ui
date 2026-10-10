//! From a URL to pixels: fetching the bytes, decoding PNG and JPEG, and
//! scaling to the size an item asked for.
//!
//! Everything here is plain data in and out, with no engine in sight, so
//! the same function runs on the UI thread for a synchronous Image and on
//! the compute pool for an asynchronous one.

use std::sync::Arc;

use ljos_render::ImageHandle;

/// The size and part of a picture an item asked for. Part of the cache
/// key: two items share pixels only when they ask for the same thing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Request {
    /// `sourceSize.width`, 0 for "not asked".
    pub width: u32,
    /// `sourceSize.height`, 0 for "not asked".
    pub height: u32,
    /// The fill mode keeps the picture's shape (`PreserveAspectFit` and
    /// `PreserveAspectCrop`), which changes how a requested size is met.
    /// Always false when no size is asked, so that fill modes share pixels.
    pub cover: bool,
    /// `sourceClipRect` as `[x, y, width, height]`, in pixels of the
    /// scaled picture.
    pub clip: Option<[u32; 4]>,
}

/// Why a picture could not be shown. `Display` is the text Qt 6.11 prints
/// after `QML Image: `.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("Cannot open: {0}")]
    CannotOpen(String),
    #[error("Protocol \"{0}\" is unknown")]
    UnknownProtocol(String),
    #[error("Error decoding: {url}: {reason}")]
    Decode { url: String, reason: String },
}

impl LoadError {
    fn decode(url: &str, reason: impl Into<String>) -> Self {
        Self::Decode {
            url: url.to_owned(),
            reason: reason.into(),
        }
    }
}

/// Where the bytes of a URL come from.
#[derive(Clone, Debug)]
pub(crate) enum Origin {
    /// Bytes the host registered for the URL, or an inline `data:` URL.
    Bytes(Arc<[u8]>),
    /// A file to read.
    File(std::path::PathBuf),
}

/// The scheme of `url`, lower case, when it has one. A single letter is a
/// Windows drive, not a scheme.
pub(crate) fn scheme(url: &str) -> Option<String> {
    let (head, _) = url.split_once(':')?;
    let mut chars = head.chars();
    let ok = head.len() > 1
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    ok.then(|| head.to_ascii_lowercase())
}

/// Works out where `url` is read from. `registered` is what the host
/// registered under exactly this URL. Does no I/O, so the UI thread can
/// call it before handing the work to a pool thread.
pub(crate) fn locate(url: &str, registered: Option<Arc<[u8]>>) -> Result<Origin, LoadError> {
    if let Some(bytes) = registered {
        return Ok(Origin::Bytes(bytes));
    }
    match scheme(url).as_deref() {
        None => Ok(Origin::File(url.into())),
        Some("file") => {
            let rest = &url["file:".len()..];
            // `file:///path`, `file://host/path` (the host is ignored, as
            // it is for a local file) and the short `file:/path`.
            let path = match rest.strip_prefix("//") {
                Some(r) => r.find('/').map_or("", |i| &r[i..]),
                None => rest,
            };
            let path = percent_decode(path);
            Ok(Origin::File(
                String::from_utf8_lossy(&path).into_owned().into(),
            ))
        }
        Some("data") => data_url(url).map(|b| Origin::Bytes(b.into())),
        // Resource URLs name nothing on disk: only registered ones exist.
        Some("qrc") => Err(LoadError::CannotOpen(url.to_owned())),
        // `http` and `https` are not loaded in this milestone and get the
        // message Qt has for a scheme it has no handler for.
        Some(other) => Err(LoadError::UnknownProtocol(other.to_owned())),
    }
}

/// Reads and decodes `url`, then scales and clips it to `request`.
pub(crate) fn load(url: &str, origin: &Origin, request: Request) -> Result<ImageHandle, LoadError> {
    let read;
    let bytes: &[u8] = match origin {
        Origin::Bytes(b) => b,
        Origin::File(path) => {
            read = std::fs::read(path).map_err(|_| LoadError::CannotOpen(url.to_owned()))?;
            &read
        }
    };
    let full = decode(url, bytes)?;
    fit_request(url, full, request)
}

/// Decodes a PNG or JPEG, told apart by their first bytes as Qt does (the
/// file name's ending plays no part).
pub(crate) fn decode(url: &str, bytes: &[u8]) -> Result<ImageHandle, LoadError> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const JPEG: &[u8] = &[0xff, 0xd8, 0xff];
    let (width, height, rgba) = if bytes.starts_with(PNG) {
        decode_png(bytes).map_err(|reason| LoadError::decode(url, reason))?
    } else if bytes.starts_with(JPEG) {
        decode_jpeg(bytes).map_err(|reason| LoadError::decode(url, reason))?
    } else {
        return Err(LoadError::decode(url, "Unsupported image format"));
    };
    ImageHandle::from_straight(width, height, rgba)
        .map_err(|err| LoadError::decode(url, err.to_string()))
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palettes become RGB, 16 bits become 8, and a transparent colour key
    // becomes an alpha channel: four layouts are left to convert.
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "the picture is too large".to_owned())?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        other => return Err(format!("unexpected PNG colour type {other:?}")),
    };
    Ok((info.width, info.height, rgba))
}

fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgba = dec.decode().map_err(|e| e.to_string())?;
    let info = dec
        .info()
        .ok_or_else(|| "the JPEG has no header".to_owned())?;
    Ok((u32::from(info.width), u32::from(info.height), rgba))
}

/// The size a picture of `natural` pixels is decoded to for `request`
/// (Qt 6.11.2, measured on qml6).
///
/// One side asked: the other follows the picture's shape. Both asked: the
/// picture keeps its shape and fits inside them, or covers them when the
/// fill mode keeps the shape too. A fill mode that keeps the shape may
/// also enlarge; the others only ever shrink.
pub fn requested_size(natural: (u32, u32), request: Request) -> (u32, u32) {
    let (iw, ih) = (f64::from(natural.0), f64::from(natural.1));
    if iw <= 0.0 || ih <= 0.0 {
        return natural;
    }
    let (w, h) = (f64::from(request.width), f64::from(request.height));
    let scale = match (request.width > 0, request.height > 0) {
        (false, false) => return natural,
        (true, false) => w / iw,
        (false, true) => h / ih,
        (true, true) if request.cover => (w / iw).max(h / ih),
        (true, true) => (w / iw).min(h / ih),
    };
    if !request.cover && scale >= 1.0 {
        return natural;
    }
    let side = |n: f64| ((n * scale).round() as u32).max(1);
    (side(iw), side(ih))
}

/// Scales `full` to the requested size, then cuts out the requested part.
fn fit_request(url: &str, full: ImageHandle, request: Request) -> Result<ImageHandle, LoadError> {
    let too_large = || LoadError::decode(url, "the picture is too large");
    let (w, h) = requested_size((full.width(), full.height()), request);
    let scaled = if (w, h) == (full.width(), full.height()) {
        full
    } else {
        full.resized(w, h).ok_or_else(too_large)?
    };
    let Some([x, y, cw, ch]) = request.clip else {
        return Ok(scaled);
    };
    // The part of the clip that lies on the picture.
    let x = x.min(scaled.width());
    let y = y.min(scaled.height());
    let cw = cw.min(scaled.width() - x);
    let ch = ch.min(scaled.height() - y);
    if cw == 0 || ch == 0 || (cw, ch) == (scaled.width(), scaled.height()) {
        return Ok(scaled);
    }
    scaled.cropped(x, y, cw, ch).ok_or_else(too_large)
}

/// The bytes of a `data:` URL (RFC 2397): `data:[<type>][;base64],<data>`.
fn data_url(url: &str) -> Result<Vec<u8>, LoadError> {
    let bad = || LoadError::CannotOpen(url.to_owned());
    let (head, body) = url["data:".len()..].split_once(',').ok_or_else(bad)?;
    let raw = percent_decode(body);
    let is_base64 = head
        .rsplit(';')
        .next()
        .is_some_and(|p| p.trim().eq_ignore_ascii_case("base64"));
    if is_base64 {
        base64(&raw).ok_or_else(bad)
    } else {
        Ok(raw)
    }
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let hex = |b: u8| char::from(b).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Standard and URL-safe base64, with or without padding; white space is
/// skipped, as browsers and Qt do for `data:` URLs.
fn base64(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(width: u32, height: u32, cover: bool) -> Request {
        Request {
            width,
            height,
            cover,
            clip: None,
        }
    }

    #[test]
    fn a_requested_size_keeps_the_shape_of_the_picture() {
        let natural = (40, 20);
        assert_eq!(requested_size(natural, ask(0, 0, false)), (40, 20));
        assert_eq!(requested_size(natural, ask(20, 0, false)), (20, 10));
        assert_eq!(requested_size(natural, ask(0, 5, false)), (10, 5));
        // Both sides: inside them, unless the fill mode covers.
        assert_eq!(requested_size(natural, ask(20, 20, false)), (20, 10));
        assert_eq!(requested_size(natural, ask(20, 20, true)), (40, 20));
        assert_eq!(requested_size(natural, ask(10, 10, true)), (20, 10));
    }

    #[test]
    fn only_a_shape_keeping_fill_mode_enlarges() {
        let natural = (40, 20);
        assert_eq!(requested_size(natural, ask(80, 0, false)), (40, 20));
        assert_eq!(requested_size(natural, ask(80, 0, true)), (80, 40));
        assert_eq!(requested_size(natural, ask(80, 80, true)), (160, 80));
    }

    #[test]
    fn schemes_are_told_from_drive_letters_and_paths() {
        assert_eq!(scheme("file:///a.png").as_deref(), Some("file"));
        assert_eq!(scheme("QRC:/a.png").as_deref(), Some("qrc"));
        assert_eq!(scheme("c:/pictures/a.png"), None);
        assert_eq!(scheme("/tmp/a:b.png"), None);
        assert_eq!(scheme("a.png"), None);
    }

    #[test]
    fn file_urls_name_paths() {
        let path = |url| match locate(url, None) {
            Ok(Origin::File(p)) => p.to_string_lossy().into_owned(),
            other => panic!("{other:?}"),
        };
        assert_eq!(path("file:///a/b%20c.png"), "/a/b c.png");
        assert_eq!(path("file:/a/b.png"), "/a/b.png");
        assert_eq!(path("file://host/a/b.png"), "/a/b.png");
        assert_eq!(path("pictures/a.png"), "pictures/a.png");
    }

    #[test]
    fn unknown_schemes_and_unregistered_resources_fail_with_qts_words() {
        let err = |url| locate(url, None).unwrap_err().to_string();
        assert_eq!(err("nope:/a.png"), "Protocol \"nope\" is unknown");
        assert_eq!(err("qrc:/a.png"), "Cannot open: qrc:/a.png");
        // A registered URL needs no scheme anybody knows.
        assert!(locate("nope:/a.png", Some(Arc::from(&b"x"[..]))).is_ok());
    }

    #[test]
    fn data_urls_carry_their_bytes() {
        let bytes = |url| match locate(url, None) {
            Ok(Origin::Bytes(b)) => b.to_vec(),
            other => panic!("{other:?}"),
        };
        assert_eq!(bytes("data:text/plain;base64,aGVsbG8="), b"hello");
        assert_eq!(bytes("data:;base64,aGVsbG8"), b"hello");
        assert_eq!(bytes("data:,a%20b"), b"a b");
        assert!(locate("data:image/png;base64,***", None).is_err());
        assert!(locate("data:no-comma", None).is_err());
    }

    #[test]
    fn bytes_that_are_no_picture_are_an_unsupported_format() {
        let err = decode("file:///letter.png", b"Dear reader,").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Error decoding: file:///letter.png: Unsupported image format"
        );
        // A PNG signature with nothing behind it is a decoding error too.
        let err = decode("file:///cut.png", b"\x89PNG\r\n\x1a\n").unwrap_err();
        assert!(
            err.to_string()
                .starts_with("Error decoding: file:///cut.png: ")
        );
    }

    #[test]
    fn a_missing_file_cannot_be_opened() {
        let origin = Origin::File("/nonexistent/ljos/missing.png".into());
        let err = load(
            "file:///nonexistent/ljos/missing.png",
            &origin,
            Request::default(),
        );
        assert_eq!(
            err.unwrap_err().to_string(),
            "Cannot open: file:///nonexistent/ljos/missing.png"
        );
    }
}
