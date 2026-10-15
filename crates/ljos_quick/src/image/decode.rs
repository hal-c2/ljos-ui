//! From a URL to pixels: fetching the bytes, decoding PNG, JPEG and GIF
//! or drawing an SVG, and scaling to the size an item asked for.
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
    /// Device pixels per logical pixel, in thousandths; 0 for one. Only a
    /// drawing is made at it, so that it stays sharp on a dense display:
    /// the pixels of a PNG or JPEG are what they are. Everything else in
    /// the request, and the size the item reports, stays in logical pixels.
    pub scale: u32,
    /// Every frame of an animation is wanted, not only the first: what an
    /// `AnimatedImage` asks for.
    pub animated: bool,
    /// The longest side, in pixels, the pixels kept need to have; 0 for
    /// all of them. A picture at least twice as long is kept halved, as
    /// often as it stays this long: no window shows more of it, and the
    /// paint asks for more when it is drawn larger. Nothing QML reads
    /// changes, since the picture keeps reporting its own size.
    pub limit: u32,
}

impl Request {
    /// The request at `scale` device pixels per logical pixel.
    pub fn at_scale(self, scale: f32) -> Self {
        let milli = (f64::from(scale) * 1000.0).round();
        let scale = if milli.is_finite() && milli > 0.0 && milli != 1000.0 {
            milli.min(f64::from(u32::MAX)) as u32
        } else {
            0
        };
        Self { scale, ..self }
    }

    /// Device pixels per logical pixel.
    pub fn scale_factor(&self) -> f64 {
        if self.scale == 0 {
            1.0
        } else {
            f64::from(self.scale) / 1000.0
        }
    }
}

/// A decoded picture, and whether it was a drawing that can be made again
/// at another size.
#[derive(Clone, Debug)]
pub(crate) struct Loaded {
    pub image: ImageHandle,
    /// The size of the picture, in logical pixels: that of `image` unless
    /// it was kept smaller for the request's `limit`.
    pub natural: (u32, u32),
    pub vector: bool,
    /// Every frame, for a request that is `animated`; `image` is the first.
    pub frames: Option<Arc<Frames>>,
}

/// The frames of an animation, each with how long it is shown.
#[derive(Debug)]
pub(crate) struct Frames {
    pub images: Vec<ImageHandle>,
    /// Milliseconds, at speed one.
    pub delays: Vec<u32>,
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
    /// A network source whose bytes did not arrive, in the words of the
    /// failure (a refused connection, a reply that was not a success).
    #[error("{0}")]
    Network(String),
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
    /// A URL to fetch (`http`, `https`): only ever in the background.
    Remote,
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
        Some("http" | "https") => Ok(Origin::Remote),
        // What has no handler gets the message Qt has for that.
        Some(other) => Err(LoadError::UnknownProtocol(other.to_owned())),
    }
}

/// Reads and decodes `url`, then scales and clips it to `request`.
pub(crate) fn load(url: &str, origin: &Origin, request: Request) -> Result<Loaded, LoadError> {
    load_bytes(url, &read(url, origin)?, request)
}

/// The bytes `origin` holds or names.
pub(crate) fn read<'a>(
    url: &str,
    origin: &'a Origin,
) -> Result<std::borrow::Cow<'a, [u8]>, LoadError> {
    match origin {
        Origin::Bytes(b) => Ok(std::borrow::Cow::Borrowed(b)),
        Origin::File(path) => read_file(path)
            .map(std::borrow::Cow::Owned)
            .ok_or_else(|| LoadError::CannotOpen(url.to_owned())),
        // The fetch happens before, on a thread of its own; whoever asks
        // for a network picture on the spot cannot have it.
        Origin::Remote => Err(LoadError::UnknownProtocol(scheme(url).unwrap_or_default())),
    }
}

/// About how many bytes decoding `bytes` takes at its height: the
/// picture at four bytes a pixel, as its header says. 0 for what has no
/// header read here (a drawing).
pub(crate) fn cost(bytes: &[u8]) -> usize {
    dimensions(bytes).map_or(0, |(w, h)| {
        (w as usize).saturating_mul(h as usize).saturating_mul(4)
    })
}

/// The size a PNG, JPEG or GIF says it has, read from its header alone.
pub(crate) fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let be32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
        return Some((be32(16)?, be32(20)?));
    }
    if bytes.starts_with(GIF) {
        let le16 = |at: usize| {
            Some(u32::from(u16::from_le_bytes([
                *bytes.get(at)?,
                *bytes.get(at + 1)?,
            ])))
        };
        return Some((le16(6)?, le16(8)?));
    }
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    // Segment after segment up to the frame header, which holds the size.
    let mut at = 2;
    loop {
        while *bytes.get(at)? != 0xff {
            at += 1;
        }
        while *bytes.get(at)? == 0xff {
            at += 1;
        }
        let marker = *bytes.get(at)?;
        at += 1;
        match marker {
            0xd8 | 0x01 | 0xd0..=0xd7 => continue,
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some((be16(at + 5)?, be16(at + 3)?));
            }
            0xd9 | 0xda => return None,
            _ => at += be16(at)? as usize,
        }
    }
}

/// Decodes `bytes` of `url`, then scales and clips them to `request`.
pub(crate) fn load_bytes(url: &str, bytes: &[u8], request: Request) -> Result<Loaded, LoadError> {
    if is_svg(bytes) {
        let image = draw_svg(url, bytes, request)?;
        return Ok(Loaded {
            natural: (image.width(), image.height()),
            image,
            vector: true,
            frames: None,
        });
    }
    if request.animated && bytes.starts_with(GIF) {
        let (width, height, frames) =
            decode_gif(bytes, true).map_err(|reason| LoadError::decode(url, reason))?;
        let mut images = Vec::with_capacity(frames.len());
        let mut delays = Vec::with_capacity(frames.len());
        for (rgba, delay) in frames {
            let full = ImageHandle::from_straight(width, height, rgba)
                .map_err(|err| LoadError::decode(url, err.to_string()))?;
            images.push(fitted(url, full, request)?);
            delays.push(delay);
        }
        let image = images
            .first()
            .cloned()
            .ok_or_else(|| LoadError::decode(url, "Unable to read image data"))?;
        return Ok(Loaded {
            natural: (image.width(), image.height()),
            image,
            vector: false,
            frames: Some(Arc::new(Frames { images, delays })),
        });
    }
    let raw = decode_raw(url, bytes)?;
    let asks_nothing = request.width == 0 && request.height == 0 && request.clip.is_none();
    if asks_nothing {
        // Straight from the decoder's pixels: neither a copy at full size
        // with four bytes a pixel nor one premultiplied comes in between.
        let natural = (raw.width, raw.height);
        let image = raw.reduced(halvings(natural, request.limit), url)?;
        return Ok(Loaded {
            image,
            natural,
            vector: false,
            frames: None,
        });
    }
    let image = fitted(url, raw.reduced(0, url)?, request)?;
    let natural = (image.width(), image.height());
    let image = match halvings(natural, request.limit) {
        0 => image,
        k => Raw::of(&image).reduced(k, url)?,
    };
    Ok(Loaded {
        image,
        natural,
        vector: false,
        frames: None,
    })
}

/// How many times a picture of `natural` size is halved for `limit`: as
/// often as its longest side stays at least `limit` long.
pub(crate) fn halvings(natural: (u32, u32), limit: u32) -> u32 {
    let longest = natural.0.max(natural.1);
    if limit == 0 {
        return 0;
    }
    let mut k = 0;
    while k < 16 && (1 << k) < longest && longest.div_ceil(1 << (k + 1)) >= limit {
        k += 1;
    }
    k
}

/// Decoded pixels as the decoder gave them: RGB, straight RGBA, or RGBA
/// already premultiplied (a picture that was scaled).
pub(crate) struct Raw<'a> {
    width: u32,
    height: u32,
    channels: usize,
    premultiplied: bool,
    data: std::borrow::Cow<'a, [u8]>,
}

impl<'a> Raw<'a> {
    pub(crate) fn of(image: &'a ImageHandle) -> Self {
        Raw {
            width: image.width(),
            height: image.height(),
            channels: 4,
            premultiplied: true,
            data: std::borrow::Cow::Borrowed(image.data()),
        }
    }

    /// The pixels made smaller `2^k` times on each side, every pixel the
    /// average of the block it covers (a block at the right or bottom edge
    /// may be cut short). Premultiplied before it is averaged, so that a
    /// transparent pixel lends no colour. Only a row of sums is kept
    /// besides the result.
    pub(crate) fn reduced(self, k: u32, url: &str) -> Result<ImageHandle, LoadError> {
        let bad = |err: ljos_render::ImageError| LoadError::decode(url, err.to_string());
        let (w, h, n) = (self.width as usize, self.height as usize, self.channels);
        if k == 0 {
            let rgba = match (n, self.premultiplied) {
                (4, true) => {
                    return ImageHandle::from_premultiplied(
                        self.width,
                        self.height,
                        self.data.into_owned(),
                    )
                    .map_err(bad);
                }
                (4, false) => self.data.into_owned(),
                _ => {
                    let mut rgba = Vec::new();
                    rgba.try_reserve_exact(w * h * 4)
                        .map_err(|_| too_large(url))?;
                    for c in self.data.as_chunks::<3>().0 {
                        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                    rgba
                }
            };
            return ImageHandle::from_straight(self.width, self.height, rgba).map_err(bad);
        }
        let step = 1usize << k;
        let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
        let mut out = Vec::new();
        out.try_reserve_exact(ow * oh * 4)
            .map_err(|_| too_large(url))?;
        let mut sums = vec![0u32; ow * 4];
        for by in 0..oh {
            sums.fill(0);
            let rows = (by * step)..((by + 1) * step).min(h);
            let tall = rows.len();
            for y in rows {
                let row = &self.data[y * w * n..(y + 1) * w * n];
                for (x, px) in row.chunks_exact(n).enumerate() {
                    let (r, g, b, a) = match n {
                        3 => (px[0], px[1], px[2], 255),
                        _ => (px[0], px[1], px[2], px[3]),
                    };
                    let (r, g, b) = if self.premultiplied || a == 255 {
                        (r, g, b)
                    } else {
                        let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
                        (pre(r), pre(g), pre(b))
                    };
                    let s = &mut sums[(x / step) * 4..(x / step) * 4 + 4];
                    s[0] += u32::from(r);
                    s[1] += u32::from(g);
                    s[2] += u32::from(b);
                    s[3] += u32::from(a);
                }
            }
            for bx in 0..ow {
                let wide = (w - bx * step).min(step);
                let count = (wide * tall) as u32;
                let s = &sums[bx * 4..bx * 4 + 4];
                out.extend(s.iter().map(|v| ((v + count / 2) / count) as u8));
            }
        }
        ImageHandle::from_premultiplied(ow as u32, oh as u32, out).map_err(bad)
    }
}

/// `full` scaled and clipped as `request` asks.
fn fitted(url: &str, full: ImageHandle, request: Request) -> Result<ImageHandle, LoadError> {
    let (w, h) = requested_size((full.width(), full.height()), request);
    let scaled = if (w, h) == (full.width(), full.height()) {
        full
    } else {
        full.resized(w, h).ok_or_else(|| too_large(url))?
    };
    clip(url, scaled, request.clip, 1.0)
}

/// The suffixes of the formats Qt reads, in the order it tries them on a
/// name that has none (Qt 6.11.2 on qml6: alphabetical, so `jpeg` before
/// `jpg` before `png`, and `svg` before `svgz`).
pub(crate) const SUFFIXES: &[&str] = &[
    "bmp", "gif", "jpeg", "jpg", "pbm", "pgm", "png", "ppm", "svg", "svgz", "xbm", "xpm",
];

/// Whether `name` (a path or URL) ends in a name without a suffix, which
/// Qt then looks for with the suffix of each format it reads.
pub(crate) fn has_no_suffix(name: &str) -> bool {
    let last = name.rsplit('/').next().unwrap_or(name);
    !last.is_empty() && !last.contains('.')
}

/// The bytes of the file at `path` or, when there is none and its name has
/// no suffix, of the first file there is with one of [`SUFFIXES`] added.
/// Only files count, and the suffix is matched as written: `logo.SVG` is
/// not found for `logo`.
fn read_file(path: &std::path::Path) -> Option<Vec<u8>> {
    if let Ok(bytes) = std::fs::read(path) {
        return Some(bytes);
    }
    if !has_no_suffix(&path.to_string_lossy()) || path.exists() {
        return None;
    }
    SUFFIXES.iter().find_map(|suffix| {
        let mut name = path.as_os_str().to_owned();
        name.push(".");
        name.push(suffix);
        let candidate = std::path::PathBuf::from(name);
        candidate
            .is_file()
            .then(|| std::fs::read(&candidate).ok())
            .flatten()
    })
}

fn too_large(url: &str) -> LoadError {
    LoadError::decode(url, "the picture is too large")
}

/// Whether `bytes` are an SVG drawing, plain or gzipped (SVGZ): told by
/// what they hold, as Qt tells every format, not by the file's name.
fn is_svg(bytes: &[u8]) -> bool {
    const GZIP: &[u8] = &[0x1f, 0x8b];
    if bytes.starts_with(GZIP) {
        return true;
    }
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    // The root element comes after the XML declaration, comments and a
    // DOCTYPE; a kilobyte of those is plenty to look through.
    let head = &text[..text.len().min(4096)];
    let head = String::from_utf8_lossy(head);
    let mut rest = head.trim_start();
    loop {
        if rest.starts_with("<svg") {
            return true;
        }
        let skipped = if let Some(r) = rest.strip_prefix("<?") {
            r.split_once("?>").map(|(_, r)| r)
        } else if let Some(r) = rest.strip_prefix("<!--") {
            r.split_once("-->").map(|(_, r)| r)
        } else if let Some(r) = rest.strip_prefix("<!") {
            // A DOCTYPE may hold an internal subset in brackets.
            match (r.find('['), r.find('>')) {
                (Some(open), Some(close)) if open < close => r.split_once("]>").map(|(_, r)| r),
                _ => r.split_once('>').map(|(_, r)| r),
            }
        } else {
            return false;
        };
        match skipped {
            Some(r) => rest = r.trim_start(),
            None => return false,
        }
    }
}

/// The size, in logical pixels, a drawing of `intrinsic` size is made at
/// for `request` (Qt 6.11.2, measured on qml6). Unlike a picture, a
/// drawing is enlarged as readily as it is shrunk, and with both sides
/// asked and a fill mode that does not keep the shape it is stretched to
/// exactly them.
pub fn drawing_size(intrinsic: (f64, f64), request: Request) -> (u32, u32) {
    let (iw, ih) = intrinsic;
    let side = |n: f64| n.round().max(0.0) as u32;
    if iw <= 0.0 || ih <= 0.0 {
        return (side(iw), side(ih));
    }
    let (w, h) = (f64::from(request.width), f64::from(request.height));
    let scale = match (request.width > 0, request.height > 0) {
        (false, false) => return (side(iw), side(ih)),
        (true, false) => w / iw,
        (false, true) => h / ih,
        (true, true) if request.cover => (w / iw).max(h / ih),
        (true, true) => return (request.width, request.height),
    };
    (side(iw * scale).max(1), side(ih * scale).max(1))
}

/// Parses an SVG and draws it at the size `request` asks for, in device
/// pixels.
fn draw_svg(url: &str, bytes: &[u8], request: Request) -> Result<ImageHandle, LoadError> {
    use resvg::{tiny_skia, usvg};
    // Qt's words for a drawing it cannot read, whatever the reason.
    let unreadable = || LoadError::decode(url, "Unable to read image data");
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|_| unreadable())?;
    let size = tree.size();
    let intrinsic = (f64::from(size.width()), f64::from(size.height()));
    let (lw, lh) = drawing_size(intrinsic, request);
    let scale = request.scale_factor();
    let device = |n: u32| (f64::from(n) * scale).round().max(1.0) as u32;
    let (pw, ph) = (device(lw), device(lh));
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or_else(|| too_large(url))?;
    let transform = tiny_skia::Transform::from_scale(
        (f64::from(pw) / intrinsic.0) as f32,
        (f64::from(ph) / intrinsic.1) as f32,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied RGBA, as an `ImageHandle` does.
    let image = ImageHandle::from_premultiplied(pw, ph, pixmap.take())
        .map_err(|err| LoadError::decode(url, err.to_string()))?;
    clip(url, image, request.clip, scale)
}

/// The first bytes of a GIF, either version.
const GIF: &[u8] = b"GIF8";

/// Decodes a PNG, JPEG or GIF (its first frame), told apart by their first
/// bytes as Qt does (the file name's ending plays no part), with the
/// pixels as the decoder gave them.
fn decode_raw<'a>(url: &str, bytes: &[u8]) -> Result<Raw<'a>, LoadError> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const JPEG: &[u8] = &[0xff, 0xd8, 0xff];
    let (width, height, channels, data) = if bytes.starts_with(PNG) {
        decode_png(bytes).map_err(|reason| LoadError::decode(url, reason))?
    } else if bytes.starts_with(JPEG) {
        decode_jpeg(bytes).map_err(|reason| LoadError::decode(url, reason))?
    } else if bytes.starts_with(GIF) {
        let (width, height, mut frames) =
            decode_gif(bytes, false).map_err(|reason| LoadError::decode(url, reason))?;
        let (rgba, _) = frames
            .pop()
            .ok_or_else(|| LoadError::decode(url, "Unable to read image data"))?;
        (width, height, 4, rgba)
    } else {
        return Err(LoadError::decode(url, "Unsupported image format"));
    };
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(channels));
    if expected != Some(data.len()) {
        return Err(LoadError::decode(url, "Unable to read image data"));
    }
    Ok(Raw {
        width,
        height,
        channels,
        premultiplied: false,
        data: std::borrow::Cow::Owned(data),
    })
}

/// Width, height, channels (3 or 4) and the straight pixels.
type Decoded = (u32, u32, usize, Vec<u8>);

fn decode_png(bytes: &[u8]) -> Result<Decoded, String> {
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
    let (channels, pixels): (usize, Vec<u8>) = match info.color_type {
        png::ColorType::Rgba => (4, buf),
        png::ColorType::Rgb => (3, buf),
        png::ColorType::Grayscale => (3, buf.iter().flat_map(|g| [*g, *g, *g]).collect()),
        png::ColorType::GrayscaleAlpha => (
            4,
            buf.as_chunks::<2>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
        ),
        other => return Err(format!("unexpected PNG colour type {other:?}")),
    };
    Ok((info.width, info.height, channels, pixels))
}

/// How long a frame that says nothing (a delay of 0) is shown, as
/// browsers and most viewers do.
const GIF_DEFAULT_DELAY_MS: u32 = 100;

/// A picture's straight RGBA pixels and how many milliseconds it is shown.
type Frame = (Vec<u8>, u32);

/// The frames of a GIF as whole pictures (straight RGBA, the size of the
/// logical screen), each with its delay in milliseconds: every frame is
/// drawn on what the ones before left, as its disposal says. Only the
/// first unless `all`.
fn decode_gif(bytes: &[u8], all: bool) -> Result<(u32, u32, Vec<Frame>), String> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = options
        .read_info(std::io::Cursor::new(bytes))
        .map_err(|e| e.to_string())?;
    let (w, h) = (usize::from(dec.width()), usize::from(dec.height()));
    if w == 0 || h == 0 {
        return Err("Unable to read image data".to_owned());
    }
    let mut canvas = vec![0u8; w * h * 4];
    let mut out = Vec::new();
    while let Some(frame) = dec.read_next_frame().map_err(|e| e.to_string())? {
        let before = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (left, top) = (usize::from(frame.left), usize::from(frame.top));
        let fw = usize::from(frame.width);
        let rows = frame.buffer.chunks_exact(fw.max(1) * 4);
        for (y, row) in rows.enumerate() {
            let cy = top + y;
            if cy >= h {
                break;
            }
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let cx = left + x;
                // A transparent index leaves what was there.
                if cx >= w || px[3] == 0 {
                    continue;
                }
                let at = (cy * w + cx) * 4;
                canvas[at..at + 4].copy_from_slice(px);
            }
        }
        let delay = match u32::from(frame.delay) * 10 {
            0 => GIF_DEFAULT_DELAY_MS,
            ms => ms,
        };
        out.push((canvas.clone(), delay));
        if !all {
            break;
        }
        match (frame.dispose, before) {
            (gif::DisposalMethod::Previous, Some(before)) => canvas = before,
            (gif::DisposalMethod::Background, _) => {
                for y in top..(top + usize::from(frame.height)).min(h) {
                    let from = (y * w + left.min(w)) * 4;
                    let to = (y * w + (left + fw).min(w)) * 4;
                    canvas[from..to].fill(0);
                }
            }
            _ => {}
        }
    }
    Ok((w as u32, h as u32, out))
}

fn decode_jpeg(bytes: &[u8]) -> Result<Decoded, String> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    // Three bytes a pixel: a JPEG has no alpha to keep.
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let rgb = dec.decode().map_err(|e| e.to_string())?;
    let info = dec
        .info()
        .ok_or_else(|| "the JPEG has no header".to_owned())?;
    Ok((u32::from(info.width), u32::from(info.height), 3, rgb))
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

/// Cuts the part `clip` names, in logical pixels, out of `scaled`, which
/// has `scale` device pixels to the logical one.
fn clip(
    url: &str,
    scaled: ImageHandle,
    clip: Option<[u32; 4]>,
    scale: f64,
) -> Result<ImageHandle, LoadError> {
    let Some(clip) = clip else {
        return Ok(scaled);
    };
    let [x, y, cw, ch] = clip.map(|n| (f64::from(n) * scale).round() as u32);
    // The part of the clip that lies on the picture.
    let x = x.min(scaled.width());
    let y = y.min(scaled.height());
    let cw = cw.min(scaled.width() - x);
    let ch = ch.min(scaled.height() - y);
    if cw == 0 || ch == 0 || (cw, ch) == (scaled.width(), scaled.height()) {
        return Ok(scaled);
    }
    scaled.cropped(x, y, cw, ch).ok_or_else(|| too_large(url))
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
            scale: 0,
            animated: false,
            limit: 0,
        }
    }

    /// A 3 by 2 GIF of one colour a frame, each frame `delay` hundredths.
    fn gif(colours: &[[u8; 3]], delay: u16) -> Vec<u8> {
        let palette: Vec<u8> = colours.iter().flatten().copied().collect();
        let mut out = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut out, 3, 2, &palette).unwrap();
            for i in 0..colours.len() {
                let frame = gif::Frame {
                    width: 3,
                    height: 2,
                    delay,
                    buffer: std::borrow::Cow::Owned(vec![u8::try_from(i).unwrap(); 6]),
                    ..gif::Frame::default()
                };
                enc.write_frame(&frame).unwrap();
            }
        }
        out
    }

    const RGB: [[u8; 3]; 3] = [[255, 0, 0], [0, 255, 0], [0, 0, 255]];

    const WIDE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
        <rect width="40" height="20" fill="#ff0000"/>
        <rect width="20" height="20" fill="#0000ff"/>
    </svg>"##;

    fn draw(bytes: &[u8], request: Request) -> Result<ImageHandle, LoadError> {
        let loaded = load("file:///wide.svg", &Origin::Bytes(bytes.into()), request)?;
        assert!(loaded.vector);
        Ok(loaded.image)
    }

    #[test]
    fn svg_is_known_by_its_content() {
        assert!(is_svg(WIDE));
        assert!(is_svg(
            b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<!-- c -->\n<svg/>"
        ));
        assert!(is_svg(
            b"<!DOCTYPE svg [<!ENTITY a \"b\">]><svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(!is_svg(b"Dear reader,"));
        assert!(!is_svg(b"<html><svg/></html>"));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn a_drawing_is_made_at_the_size_asked_for() {
        let natural = (40.0, 20.0);
        assert_eq!(drawing_size(natural, ask(0, 0, false)), (40, 20));
        assert_eq!(drawing_size(natural, ask(80, 0, false)), (80, 40));
        assert_eq!(drawing_size(natural, ask(0, 5, false)), (10, 5));
        assert_eq!(drawing_size(natural, ask(80, 80, false)), (80, 80));
        assert_eq!(drawing_size(natural, ask(80, 80, true)), (160, 80));
        assert_eq!(drawing_size(natural, ask(20, 30, true)), (60, 30));
    }

    #[test]
    fn a_drawing_is_drawn_stretched_and_at_device_scale() {
        let image = draw(WIDE, ask(80, 80, false)).unwrap();
        assert_eq!((image.width(), image.height()), (80, 80));
        assert_eq!(image.pixel(5, 75), [0, 0, 255, 255]);
        assert_eq!(image.pixel(45, 5), [255, 0, 0, 255]);
        let sharp = draw(WIDE, Request::default().at_scale(2.0)).unwrap();
        assert_eq!((sharp.width(), sharp.height()), (80, 40));
        assert_eq!(sharp.pixel(39, 20), [0, 0, 255, 255]);
        assert_eq!(sharp.pixel(41, 20), [255, 0, 0, 255]);
    }

    #[test]
    fn a_drawings_clip_is_in_logical_pixels() {
        let request = Request {
            clip: Some([10, 0, 20, 10]),
            ..ask(80, 0, false)
        };
        let image = draw(WIDE, request).unwrap();
        assert_eq!((image.width(), image.height()), (20, 10));
        let sharp = draw(WIDE, request.at_scale(1.5)).unwrap();
        assert_eq!((sharp.width(), sharp.height()), (30, 15));
    }

    #[test]
    fn a_broken_drawing_cannot_be_read() {
        let err = draw(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect",
            Request::default(),
        );
        assert_eq!(
            err.unwrap_err().to_string(),
            "Error decoding: file:///wide.svg: Unable to read image data"
        );
    }

    #[test]
    fn one_device_pixel_per_logical_pixel_is_no_scale() {
        assert_eq!(Request::default().at_scale(1.0).scale, 0);
        assert_eq!(Request::default().at_scale(2.0).scale, 2000);
        assert_eq!(Request::default().at_scale(f32::NAN).scale, 0);
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
        let err = decode_raw("file:///letter.png", b"Dear reader,")
            .err()
            .unwrap();
        assert_eq!(
            err.to_string(),
            "Error decoding: file:///letter.png: Unsupported image format"
        );
        // A PNG signature with nothing behind it is a decoding error too.
        let err = decode_raw("file:///cut.png", b"\x89PNG\r\n\x1a\n")
            .err()
            .unwrap();
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

    #[test]
    fn a_gif_is_its_first_frame_unless_every_frame_is_asked_for() {
        let origin = Origin::Bytes(gif(&RGB, 20).into());
        let still = load("file:///a.gif", &origin, Request::default()).unwrap();
        assert!(still.frames.is_none());
        assert_eq!((still.image.width(), still.image.height()), (3, 2));
        assert_eq!(&still.image.data()[..4], &[255, 0, 0, 255]);

        let request = Request {
            animated: true,
            ..Request::default()
        };
        let all = load("file:///a.gif", &origin, request).unwrap();
        let frames = all.frames.expect("every frame");
        assert_eq!(frames.delays, [200, 200, 200]);
        let firsts: Vec<&[u8]> = frames.images.iter().map(|i| &i.data()[..4]).collect();
        assert_eq!(
            firsts,
            [&[255, 0, 0, 255][..], &[0, 255, 0, 255], &[0, 0, 255, 255]]
        );
    }

    #[test]
    fn a_frame_without_a_delay_is_shown_for_a_tenth_of_a_second() {
        let request = Request {
            animated: true,
            ..Request::default()
        };
        let origin = Origin::Bytes(gif(&RGB[..2], 0).into());
        let all = load("file:///a.gif", &origin, request).unwrap();
        assert_eq!(all.frames.unwrap().delays, [100, 100]);
    }

    fn png_of(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, width, height);
            enc.set_color(color);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(data).unwrap();
        }
        out
    }

    #[test]
    fn a_header_says_the_size_without_decoding() {
        let png = png_of(5, 3, png::ColorType::Rgb, &[0; 45]);
        assert_eq!(dimensions(&png), Some((5, 3)));
        assert_eq!(cost(&png), 60);
        assert_eq!(dimensions(&gif(&RGB, 10)), Some((3, 2)));
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 90)
            .encode(&[0; 7 * 4 * 3], 7, 4, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        assert_eq!(dimensions(&jpeg), Some((7, 4)));
        assert_eq!(dimensions(WIDE), None);
        assert_eq!(cost(WIDE), 0);
        assert_eq!(dimensions(&[0xff, 0xd8, 0xff]), None, "cut short");
    }

    #[test]
    fn a_picture_is_halved_while_it_stays_as_long_as_the_limit() {
        assert_eq!(halvings((3000, 2000), 0), 0);
        assert_eq!(halvings((3000, 2000), 4096), 0);
        assert_eq!(halvings((3000, 2000), 1024), 1);
        assert_eq!(halvings((3000, 2000), 512), 2);
        assert_eq!(halvings((1024, 10), 512), 1);
        assert_eq!(halvings((1022, 10), 512), 0);
        assert_eq!(halvings((1, 1), 1), 0);
    }

    #[test]
    fn halving_averages_each_block_and_keeps_the_edges() {
        // 3 by 1, RGB: the second block is the last pixel alone.
        let raw = Raw {
            width: 3,
            height: 1,
            channels: 3,
            premultiplied: false,
            data: std::borrow::Cow::Owned(vec![200, 0, 0, 100, 0, 0, 0, 0, 90]),
        };
        let image = raw.reduced(1, "file:///a.png").unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(image.data(), &[150, 0, 0, 255, 0, 0, 90, 255]);
    }

    #[test]
    fn a_transparent_pixel_lends_no_colour_when_halved() {
        let raw = Raw {
            width: 2,
            height: 2,
            channels: 4,
            premultiplied: false,
            data: std::borrow::Cow::Owned(
                [
                    [255, 0, 0, 255],
                    [0, 255, 0, 0],
                    [0, 255, 0, 0],
                    [255, 0, 0, 255],
                ]
                .concat(),
            ),
        };
        let image = raw.reduced(1, "file:///a.png").unwrap();
        // Half covered, all red: premultiplied 128 of red at alpha 128.
        assert_eq!(image.data(), &[128, 0, 0, 128]);
    }

    #[test]
    fn a_limit_keeps_fewer_pixels_and_the_natural_size() {
        let data: Vec<u8> = (0..64 * 32).flat_map(|_| [32, 96, 160]).collect();
        let bytes = png_of(64, 32, png::ColorType::Rgb, &data);
        let request = Request {
            limit: 16,
            ..Request::default()
        };
        let loaded = load_bytes("file:///a.png", &bytes, request).unwrap();
        assert_eq!(loaded.natural, (64, 32));
        assert_eq!((loaded.image.width(), loaded.image.height()), (16, 8));
        assert_eq!(&loaded.image.data()[..4], &[32, 96, 160, 255]);

        // With a sourceSize the natural size is the one asked for.
        let request = Request {
            width: 32,
            limit: 8,
            ..Request::default()
        };
        let loaded = load_bytes("file:///a.png", &bytes, request).unwrap();
        assert_eq!(loaded.natural, (32, 16));
        assert_eq!((loaded.image.width(), loaded.image.height()), (8, 4));
    }
}
