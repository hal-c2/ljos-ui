//! From a URL to pixels: fetching the bytes, decoding PNG, JPEG and GIF
//! or drawing an SVG or a page of a PDF, and scaling to the size an item asked for.
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
    /// The page of a document asked for (`currentFrame`): 0, the first,
    /// for whatever has no pages.
    pub page: u32,
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
    /// How many pages the document has, 1 for a picture or a drawing.
    pub pages: u32,
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
    let bytes = read(url, origin)?;
    let loaded = load_bytes(url, &bytes, request)?;
    match origin {
        Origin::File(path) => made_again_from_file(&loaded, url, path, request),
        Origin::Bytes(bytes) => made_again_from_bytes(&loaded, url, bytes, request),
        Origin::Remote => {}
    }
    Ok(loaded)
}

/// Says how the pixels of `loaded` are made again once they are let go
/// after their upload: by reading the file again. Not for an animation,
/// whose frames are their item's alone.
pub(crate) fn made_again_from_file(
    loaded: &Loaded,
    url: &str,
    path: &std::path::Path,
    request: Request,
) {
    if loaded.frames.is_some() {
        return;
    }
    let (url, path) = (url.to_owned(), path.to_owned());
    loaded.image.make_again_with(ljos_render::MakeAgain {
        holds: 0,
        make: Arc::new(move || {
            let bytes = read_file(&path)?;
            Some(load_bytes(&url, &bytes, request).ok()?.image.into_data())
        }),
    });
}

/// As [`made_again_from_file`], from `bytes` it was decoded from (fetched,
/// registered or inline) when they are fewer than the pixels; otherwise
/// the pixels stay.
pub(crate) fn made_again_from_bytes(
    loaded: &Loaded,
    url: &str,
    bytes: &Arc<[u8]>,
    request: Request,
) {
    if loaded.frames.is_some() || bytes.len() >= loaded.image.byte_len() {
        return;
    }
    let (url, bytes) = (url.to_owned(), Arc::clone(bytes));
    loaded.image.make_again_with(ljos_render::MakeAgain {
        holds: bytes.len(),
        make: Arc::new(move || Some(load_bytes(&url, &bytes, request).ok()?.image.into_data())),
    });
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

/// About how many bytes decoding `bytes` for `request` holds at its
/// height, as the header says: the pixels kept, and what they are made
/// from. That is the picture whole at up to four bytes a pixel; for a
/// JPEG decoded smaller the smaller pixels, plus for a progressive one
/// the coefficients of the whole picture, which its decoder keeps until
/// the last scan; for a PNG made smaller as it is decoded, a row. 0 for what has no header read here (a drawing).
/// `usize::MAX` for more than a `usize` counts, which is never let decode.
pub(crate) fn cost(bytes: &[u8], request: Request) -> usize {
    let Some((w, h)) = dimensions(bytes) else {
        return 0;
    };
    let area = |(w, h): (u32, u32)| (w as usize).saturating_mul(h as usize);
    let asks_nothing = request.width == 0 && request.height == 0 && request.clip.is_none();
    let kept = if asks_nothing {
        kept_size((w, h), request.limit)
    } else {
        (w, h)
    };
    let held = area(kept).saturating_mul(4);
    let k = halvings((w, h), kept).min(3);
    let from = match jpeg_frame(bytes) {
        // Made smaller row by row: a row, and that row widened.
        None if asks_nothing
            && kept != (w, h)
            && bytes.starts_with(PNG)
            && !png_interlaced(bytes) =>
        {
            (w as usize).saturating_mul(8)
        }
        Some(frame) if asks_nothing && k > 0 => {
            let side = |n: u32| n.div_ceil(1 << k);
            let smaller = area((side(w), side(h))).saturating_mul(3);
            smaller.saturating_add(if frame.progressive {
                frame.coefficients
            } else {
                0
            })
        }
        _ => area((w, h)).saturating_mul(4),
    };
    held.saturating_add(from)
}

/// What a JPEG's frame header says.
struct JpegFrame {
    width: u32,
    height: u32,
    progressive: bool,
    /// Bytes of every component's coefficients for the whole picture,
    /// two a sample, in whole blocks of 8 by 8.
    coefficients: usize,
}

/// The frame header of a JPEG, found segment after segment.
fn jpeg_frame(bytes: &[u8]) -> Option<JpegFrame> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
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
                let (height, width) = (be16(at + 3)?, be16(at + 5)?);
                let n = usize::from(*bytes.get(at + 7)?);
                let factors: Vec<(u32, u32)> = (0..n)
                    .map(|c| {
                        let f = *bytes.get(at + 9 + 3 * c)?;
                        Some((u32::from(f >> 4).max(1), u32::from(f & 15).max(1)))
                    })
                    .collect::<Option<_>>()?;
                let most =
                    |pick: fn(&(u32, u32)) -> u32| factors.iter().map(pick).max().unwrap_or(1);
                let (hmax, vmax) = (most(|f| f.0), most(|f| f.1));
                let blocks =
                    |n: u32, f: u32, max: u32| ((n * f).div_ceil(max)).div_ceil(8) as usize * 8;
                // A count that wrapped would let a huge picture through
                // as a small one.
                let coefficients = factors
                    .iter()
                    .map(|&(hf, vf)| {
                        blocks(width, hf, hmax)
                            .checked_mul(blocks(height, vf, vmax))
                            .and_then(|n| n.checked_mul(2))
                            .unwrap_or(usize::MAX)
                    })
                    .fold(0usize, usize::saturating_add);
                return Some(JpegFrame {
                    width,
                    height,
                    progressive: matches!(marker, 0xc2 | 0xc6 | 0xca | 0xce),
                    coefficients,
                });
            }
            0xd9 | 0xda => return None,
            _ => at += be16(at)? as usize,
        }
    }
}

/// The size a PNG, JPEG or GIF says it has, read from its header alone.
pub(crate) fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(PNG) {
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
    jpeg_frame(bytes).map(|f| (f.width, f.height))
}

/// Decodes `bytes` of `url`, then scales and clips them to `request`.
pub(crate) fn load_bytes(url: &str, bytes: &[u8], request: Request) -> Result<Loaded, LoadError> {
    // Without the `pdf` feature a document is a format there is no reader
    // for, as in a Qt without its PDF plugin.
    #[cfg(feature = "pdf")]
    if is_pdf(bytes) {
        return draw_pdf(url, bytes, request);
    }
    if is_svg(bytes) {
        let image = draw_svg(url, bytes, request)?;
        return Ok(Loaded {
            natural: (image.width(), image.height()),
            image,
            vector: true,
            frames: None,
            pages: 1,
        });
    }
    within_budget(url, bytes, request)?;
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
            pages: 1,
        });
    }
    let asks_nothing = request.width == 0 && request.height == 0 && request.clip.is_none();
    if asks_nothing
        && let Some(natural) = dimensions(bytes)
        && let kept = kept_size(natural, request.limit)
        && let k @ 1.. = halvings(natural, kept)
        && let Some((raw, _)) = decode_jpeg_smaller(bytes, natural, k)
    {
        let image = raw.shrunk_to(kept, url)?;
        return Ok(Loaded {
            image,
            natural,
            vector: false,
            frames: None,
            pages: 1,
        });
    }
    if asks_nothing && let Some(loaded) = png_shrunk(url, bytes, request.limit) {
        return loaded;
    }
    let raw = decode_raw(url, bytes)?;
    if asks_nothing {
        // Straight from the decoder's pixels: neither a copy at full size
        // with four bytes a pixel nor one premultiplied comes in between.
        let natural = (raw.width, raw.height);
        let image = raw.shrunk_to(kept_size(natural, request.limit), url)?;
        return Ok(Loaded {
            image,
            natural,
            vector: false,
            frames: None,
            pages: 1,
        });
    }
    derive(url, raw.reduced(0, url)?, request)
}

/// What decoding a raster picture for `request` gives, made from all its
/// pixels (`whole`, premultiplied) instead.
pub(crate) fn derive(url: &str, whole: ImageHandle, request: Request) -> Result<Loaded, LoadError> {
    let image = fitted(url, whole, request)?;
    let natural = (image.width(), image.height());
    let kept = kept_size(natural, request.limit);
    let image = if kept == natural {
        image
    } else {
        Raw::of(&image, &image.data()).shrunk_to(kept, url)?
    };
    Ok(Loaded {
        image,
        natural,
        vector: false,
        frames: None,
        pages: 1,
    })
}

/// The pixels a picture of `natural` size keeps for `limit`: its own
/// shape with the longest side `limit` long, or all of it when that is no
/// longer than `limit` (or `limit` is 0).
pub(crate) fn kept_size(natural: (u32, u32), limit: u32) -> (u32, u32) {
    let longest = natural.0.max(natural.1);
    if limit == 0 || longest <= limit {
        return natural;
    }
    let side = |n: u32| {
        ((u64::from(n) * u64::from(limit) + u64::from(longest) / 2) / u64::from(longest)).max(1)
            as u32
    };
    (side(natural.0), side(natural.1))
}

/// How many times a picture of `natural` size can be halved and still
/// cover `kept` on both sides.
pub(crate) fn halvings(natural: (u32, u32), kept: (u32, u32)) -> u32 {
    let mut k = 0;
    while k < 16
        && natural.0.div_ceil(1 << (k + 1)) >= kept.0
        && natural.1.div_ceil(1 << (k + 1)) >= kept.1
        && (1 << k) < natural.0.max(natural.1)
    {
        k += 1;
    }
    k
}

/// `need` pixels rounded up to a quarter octave (…, 512, 640, 768, 896,
/// 1024, 1280, …): the limits pictures are kept at, so that a window
/// resized a little shares pictures with what it was, and a picture is
/// kept at most a quarter larger than it is shown.
pub(crate) fn step_up(need: f64) -> u32 {
    if !(need.is_finite() && need >= 1.0) {
        return 1;
    }
    let n = need.ceil().min(f64::from(1u32 << 30)) as u32;
    if n <= 4 {
        return n.next_power_of_two();
    }
    let quarter = (1u32 << (31 - n.leading_zeros())) / 4;
    n.div_ceil(quarter) * quarter
}

/// Which pixels of a side `from` long each pixel of the same side made
/// `to` long covers, and by how much (a share of the larger pixel, so the
/// shares of one sum to 1). For `to <= from`.
struct Spans {
    from: u64,
    to: u64,
}

// On a grid `from * to` long, source pixel `i` is `[i * to, (i + 1) * to)`
// and smaller pixel `o` is `[o * from, (o + 1) * from)`: whole numbers, so
// which pixels meet is never a rounding question.
impl Spans {
    fn new(from: usize, to: usize) -> Self {
        Spans {
            from: from as u64,
            to: to as u64,
        }
    }

    /// The smaller pixels source pixel `i` falls in, with its share of
    /// each: one, or two when it straddles their border.
    fn of(&self, i: usize) -> impl Iterator<Item = (usize, f32)> {
        let (start, end) = (i as u64 * self.to, (i as u64 + 1) * self.to);
        let (first, last) = (start / self.from, (end - 1) / self.from);
        (first..=last).map(move |o| {
            let covered = end.min((o + 1) * self.from) - start.max(o * self.from);
            (o as usize, covered as f32 / self.from as f32)
        })
    }

    /// The last source pixel smaller pixel `o` takes from.
    fn ends_at(&self, o: usize) -> usize {
        (((o as u64 + 1) * self.from).div_ceil(self.to) - 1) as usize
    }
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
    /// The pixels made `kept` large, every pixel the average of the area
    /// it covers, partly covered pixels by how much of them it covers. An
    /// even halving goes through [`Raw::reduced`]. Like it, it keeps only
    /// the result and a couple of rows besides.
    pub(crate) fn shrunk_to(self, kept: (u32, u32), url: &str) -> Result<ImageHandle, LoadError> {
        if kept.0 >= self.width && kept.1 >= self.height {
            return self.reduced(0, url);
        }
        let mut shrink = Shrink::new(self.layout(), kept, url)?;
        self.feed(&mut shrink);
        shrink.finish()
    }

    /// The pixels of `image`, which are `data` (its [`ImageHandle::data`]).
    pub(crate) fn of(image: &ImageHandle, data: &'a [u8]) -> Self {
        Raw {
            width: image.width(),
            height: image.height(),
            channels: 4,
            premultiplied: true,
            data: std::borrow::Cow::Borrowed(data),
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
        let mut shrink = Shrink::halving(self.layout(), k, url)?;
        self.feed(&mut shrink);
        shrink.finish()
    }

    fn layout(&self) -> Layout {
        Layout {
            width: self.width,
            height: self.height,
            channels: self.channels,
            premultiplied: self.premultiplied,
        }
    }

    fn feed(&self, shrink: &mut Shrink<'_>) {
        let line = self.width as usize * self.channels;
        for row in self.data.chunks_exact(line) {
            shrink.row(row);
        }
    }
}

/// The shape of pixels as a decoder gives them, row after row.
#[derive(Clone, Copy)]
struct Layout {
    width: u32,
    height: u32,
    /// 3 (RGB) or 4 (RGBA).
    channels: usize,
    premultiplied: bool,
}

impl Layout {
    /// A pixel premultiplied, as a decoder's row holds it.
    fn pixel(&self, px: &[u8]) -> [u8; 4] {
        let (r, g, b, a) = match self.channels {
            3 => (px[0], px[1], px[2], 255),
            _ => (px[0], px[1], px[2], px[3]),
        };
        if self.premultiplied || a == 255 {
            [r, g, b, a]
        } else {
            let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
            [pre(r), pre(g), pre(b), a]
        }
    }

    /// A pixel premultiplied, unrounded, for averages over areas.
    fn pixel_f32(&self, px: &[u8]) -> [f32; 4] {
        let (r, g, b, a) = match self.channels {
            3 => (px[0], px[1], px[2], 255),
            _ => (px[0], px[1], px[2], px[3]),
        };
        if self.premultiplied || a == 255 {
            [r, g, b, a].map(f32::from)
        } else {
            let a = f32::from(a);
            [
                f32::from(r) * a / 255.0,
                f32::from(g) * a / 255.0,
                f32::from(b) * a / 255.0,
                a,
            ]
        }
    }
}

/// Makes pixels smaller as their rows come, top to bottom, so that what is
/// kept is the result and a few rows of sums: a decoder that gives rows
/// one at a time never holds the picture whole.
struct Shrink<'u> {
    from: Layout,
    to: (usize, usize),
    url: &'u str,
    /// Source rows taken so far.
    y: usize,
    out: Vec<u8>,
    how: How,
}

enum How {
    /// Blocks `step` pixels on a side, averaged: the sums of the row of
    /// blocks under way.
    Halving { step: usize, sums: Vec<u32> },
    /// Any smaller size, every pixel the average of the area it covers.
    Area {
        xs: Spans,
        ys: Spans,
        /// Output rows being summed: the one a source row ends in and the
        /// next, which a row straddling the border starts.
        rows: [Vec<f32>; 2],
        across: Vec<f32>,
        /// The output row `rows[0]` is.
        oy: usize,
    },
}

impl<'u> Shrink<'u> {
    /// To `kept`, through a halving when that is exactly it.
    fn new(from: Layout, kept: (u32, u32), url: &'u str) -> Result<Self, LoadError> {
        let k = halvings((from.width, from.height), kept);
        let halved = (from.width.div_ceil(1 << k), from.height.div_ceil(1 << k));
        if halved == kept {
            return Self::halving(from, k, url);
        }
        let (ow, oh) = (kept.0 as usize, kept.1 as usize);
        Self::with(
            from,
            (ow, oh),
            url,
            How::Area {
                xs: Spans::new(from.width as usize, ow),
                ys: Spans::new(from.height as usize, oh),
                rows: [vec![0f32; ow * 4], vec![0f32; ow * 4]],
                across: vec![0f32; ow * 4],
                oy: 0,
            },
        )
    }

    /// Smaller `2^k` times on each side (`k` > 0).
    fn halving(from: Layout, k: u32, url: &'u str) -> Result<Self, LoadError> {
        let step = 1usize << k;
        let to = (
            (from.width as usize).div_ceil(step),
            (from.height as usize).div_ceil(step),
        );
        Self::with(
            from,
            to,
            url,
            How::Halving {
                step,
                sums: vec![0u32; to.0 * 4],
            },
        )
    }

    fn with(from: Layout, to: (usize, usize), url: &'u str, how: How) -> Result<Self, LoadError> {
        let bytes =
            to.0.checked_mul(to.1)
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| too_large(url))?;
        let mut out = Vec::new();
        out.try_reserve_exact(bytes).map_err(|_| too_large(url))?;
        Ok(Shrink {
            from,
            to,
            url,
            y: 0,
            out,
            how,
        })
    }

    /// Takes the next source row, `width * channels` bytes.
    fn row(&mut self, line: &[u8]) {
        let (w, h, n) = (
            self.from.width as usize,
            self.from.height as usize,
            self.from.channels,
        );
        let (ow, oh) = self.to;
        let y = self.y;
        if y >= h {
            return;
        }
        self.y += 1;
        let from = self.from;
        match &mut self.how {
            How::Halving { step, sums } => {
                let step = *step;
                for (x, px) in line.chunks_exact(n).take(w).enumerate() {
                    let s = &mut sums[(x / step) * 4..(x / step) * 4 + 4];
                    for (sum, v) in s.iter_mut().zip(from.pixel(px)) {
                        *sum += u32::from(v);
                    }
                }
                // The last row of a row of blocks: it is complete.
                if (y + 1).is_multiple_of(step) || y + 1 == h {
                    let tall = y % step + 1;
                    for bx in 0..ow {
                        let wide = (w - bx * step).min(step);
                        let count = (wide * tall) as u32;
                        let s = &sums[bx * 4..bx * 4 + 4];
                        self.out
                            .extend(s.iter().map(|v| ((v + count / 2) / count) as u8));
                    }
                    sums.fill(0);
                }
            }
            How::Area {
                xs,
                ys,
                rows,
                across,
                oy,
            } => {
                across.fill(0.0);
                for (x, px) in line.chunks_exact(n).take(w).enumerate() {
                    let px = from.pixel_f32(px);
                    for (o, share) in xs.of(x) {
                        for (sum, v) in across[o * 4..o * 4 + 4].iter_mut().zip(px) {
                            *sum += v * share;
                        }
                    }
                }
                for (o, share) in ys.of(y) {
                    let row = &mut rows[o - *oy];
                    for (sum, v) in row.iter_mut().zip(across.iter()) {
                        *sum += v * share;
                    }
                }
                // The last source row of output row `oy`: it is complete.
                while *oy < oh && ys.ends_at(*oy) == y {
                    self.out
                        .extend(rows[0].as_chunks::<4>().0.iter().flat_map(|px| {
                            let a = px[3].round().clamp(0.0, 255.0);
                            // Colour never above alpha: the pixels stay
                            // premultiplied.
                            [px[0], px[1], px[2], px[3]].map(|v| v.round().clamp(0.0, a) as u8)
                        }));
                    rows.swap(0, 1);
                    rows[1].fill(0.0);
                    *oy += 1;
                }
            }
        }
    }

    fn finish(self) -> Result<ImageHandle, LoadError> {
        let url = self.url;
        if self.y < self.from.height as usize {
            return Err(LoadError::decode(url, "Unable to read image data"));
        }
        ImageHandle::from_premultiplied(self.to.0 as u32, self.to.1 as u32, self.out)
            .map_err(|err| LoadError::decode(url, err.to_string()))
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
    use std::io::Read;
    let mut bytes = Vec::new();
    open_file(path)?.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The file [`read_file`] reads.
fn open_file(path: &std::path::Path) -> Option<std::fs::File> {
    if path.is_file() {
        return std::fs::File::open(path).ok();
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
            .then(|| std::fs::File::open(&candidate).ok())
            .flatten()
    })
}

/// The size the header of what `origin` holds or names says, reading no
/// more of a file than its first bytes: a JPEG's size can come after a
/// thumbnail in its metadata, so up to [`HEADER_READ`] of them.
pub(crate) fn header_size(origin: &Origin) -> Option<(u32, u32)> {
    use std::io::Read;
    match origin {
        Origin::Bytes(bytes) if !is_svg(bytes) => dimensions(bytes),
        Origin::File(path) => {
            let mut file = open_file(path)?;
            let mut bytes = Vec::new();
            for want in [4096, HEADER_READ] {
                let have = bytes.len() as u64;
                (&mut file).take(want - have).read_to_end(&mut bytes).ok()?;
                if let Some(size) = dimensions(&bytes) {
                    return Some(size);
                }
                if (bytes.len() as u64) < want {
                    return None;
                }
            }
            None
        }
        Origin::Bytes(_) | Origin::Remote => None,
    }
}

/// How much of a file [`header_size`] reads at most.
const HEADER_READ: u64 = 128 * 1024;

pub(crate) fn too_large(url: &str) -> LoadError {
    LoadError::decode(url, TOO_LARGE)
}

const TOO_LARGE: &str = "the picture is too large";

/// The most bytes the pixels of one picture may take at four bytes a
/// pixel, whole as decoded or as made for a `sourceSize`: 256 MiB, as Qt
/// 6's image reader allows by default, and the image cache's whole budget
/// on a desktop. A header that says more is refused before any decoder
/// runs, since a decoder that cannot have its memory aborts the process
/// rather than fails.
pub(crate) const MOST_PIXEL_BYTES: usize = 256 << 20;

/// The most a decode may hold at its height ([`cost`]): the largest
/// picture whole, and as much again that it is made from.
pub(crate) const MOST_DECODE_COST: usize = 2 * MOST_PIXEL_BYTES;

/// Whether pixels `size` large, at four bytes a pixel, are within
/// [`MOST_PIXEL_BYTES`]; a count that overflows is not.
fn fits(size: (u32, u32)) -> bool {
    (size.0 as usize)
        .checked_mul(size.1 as usize)
        .and_then(|n| n.checked_mul(4))
        .is_some_and(|n| n <= MOST_PIXEL_BYTES)
}

/// Refuses, from its header alone, a picture whose decode for `request`
/// would hold more than [`MOST_DECODE_COST`], or that a `sourceSize` would
/// make larger than [`MOST_PIXEL_BYTES`]. What has no header read here is
/// checked by each decoder once it has read its own.
fn within_budget(url: &str, bytes: &[u8], request: Request) -> Result<(), LoadError> {
    let Some(natural) = dimensions(bytes) else {
        return Ok(());
    };
    let asks_size = request.width > 0 || request.height > 0;
    if cost(bytes, request) > MOST_DECODE_COST
        || (asks_size && !fits(requested_size(natural, request)))
    {
        return Err(too_large(url));
    }
    Ok(())
}

/// `n` bytes to fill, or none when they cannot be had.
fn buffer(n: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(n).map_err(|_| TOO_LARGE.to_owned())?;
    Ok(out)
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

/// Whether `bytes` are a PDF document, by its header, which readers look
/// for in the first kilobyte.
#[cfg(feature = "pdf")]
fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|w| w == b"%PDF-")
}

/// Draws page `request.page` of a PDF document, or its first page when it
/// has no such page, as a drawing: a page is as large as its crop box in
/// points (Qt 6.11.2, measured on qml6), made at the size `request` asks
/// for in device pixels, over nothing.
#[cfg(feature = "pdf")]
fn draw_pdf(url: &str, bytes: &[u8], request: Request) -> Result<Loaded, LoadError> {
    use hayro::hayro_interpret::util::TransformExt as _;
    use hayro::vello_cpu;
    // Qt's words for a document it cannot read, whatever the reason.
    let unreadable = || LoadError::decode(url, "Unable to read image data");
    let pdf = hayro::hayro_syntax::Pdf::new(bytes.to_vec()).map_err(|_| unreadable())?;
    let pages = pdf.pages();
    let page = usize::try_from(request.page)
        .ok()
        .and_then(|i| pages.get(i))
        .or_else(|| pages.first())
        .ok_or_else(unreadable)?;
    let (w, h) = page.render_dimensions();
    let (w, h) = (f64::from(w), f64::from(h));
    let (lw, lh) = drawing_size((w, h), request);
    let scale = request.scale_factor();
    let device = |n: u32| {
        let n = (f64::from(n) * scale).round().max(1.0);
        // As many pixels a side as the rasteriser takes.
        (n <= f64::from(u16::MAX))
            .then_some(n as u16)
            .ok_or_else(|| too_large(url))
    };
    let (pw, ph) = (device(lw)?, device(lh)?);
    let mut ctx = vello_cpu::RenderContext::new(pw, ph);
    let transform = hayro::kurbo::Affine::scale_non_uniform(f64::from(pw) / w, f64::from(ph) / h)
        * page.initial_transform(true).to_kurbo();
    hayro::render_into(
        page,
        &hayro::RenderCache::new(),
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &hayro::RenderSettings::default(),
        &mut ctx,
        transform,
    );
    ctx.flush();
    let mut pixmap = vello_cpu::Pixmap::new(pw, ph);
    ctx.render_with(
        &mut pixmap,
        &mut vello_cpu::Resources::default(),
        vello_cpu::RasterizerSettings {
            target_init: vello_cpu::TargetInit::Clear(vello_cpu::color::palette::css::TRANSPARENT),
            ..Default::default()
        },
    );
    // vello_cpu keeps premultiplied RGBA, as an `ImageHandle` does.
    let image = ImageHandle::from_premultiplied(
        u32::from(pw),
        u32::from(ph),
        pixmap.data_as_u8_slice().to_vec(),
    )
    .map_err(|err| LoadError::decode(url, err.to_string()))?;
    let image = clip(url, image, request.clip, scale)?;
    Ok(Loaded {
        natural: (image.width(), image.height()),
        image,
        vector: true,
        frames: None,
        pages: u32::try_from(pages.len()).unwrap_or(u32::MAX),
    })
}

/// The first bytes of a GIF, either version.
const GIF: &[u8] = b"GIF8";

/// Decodes a PNG, JPEG or GIF (its first frame), told apart by their first
/// bytes as Qt does (the file name's ending plays no part), with the
/// pixels as the decoder gave them.
fn decode_raw<'a>(url: &str, bytes: &[u8]) -> Result<Raw<'a>, LoadError> {
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

/// A PNG kept smaller than it is, made smaller row by row as it is
/// decoded: the picture is never whole in memory. None for what this does
/// not take (not a PNG, interlaced, kept whole), which is decoded whole.
fn png_shrunk(url: &str, bytes: &[u8], limit: u32) -> Option<Result<Loaded, LoadError>> {
    if !bytes.starts_with(PNG) || png_interlaced(bytes) {
        return None;
    }
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    // What cannot be read is said by the whole decode, in its words.
    let mut reader = dec.read_info().ok()?;
    let natural = (reader.info().width, reader.info().height);
    let kept = kept_size(natural, limit);
    if kept == natural {
        return None;
    }
    let (colour, _) = reader.output_color_type();
    let channels = match colour {
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha => 4,
        png::ColorType::Rgb | png::ColorType::Grayscale => 3,
        _ => return None,
    };
    let from = Layout {
        width: natural.0,
        height: natural.1,
        channels,
        premultiplied: false,
    };
    let mut shrink = match Shrink::new(from, kept, url) {
        Ok(shrink) => shrink,
        Err(err) => return Some(Err(err)),
    };
    let mut wide = Vec::new();
    loop {
        let row = match reader.next_row() {
            Ok(Some(row)) => row.data(),
            Ok(None) => break,
            Err(err) => return Some(Err(LoadError::decode(url, err.to_string()))),
        };
        match colour {
            png::ColorType::Grayscale => {
                wide.clear();
                wide.extend(row.iter().flat_map(|g| [*g, *g, *g]));
                shrink.row(&wide);
            }
            png::ColorType::GrayscaleAlpha => {
                wide.clear();
                wide.extend(
                    row.as_chunks::<2>()
                        .0
                        .iter()
                        .flat_map(|c| [c[0], c[0], c[0], c[1]]),
                );
                shrink.row(&wide);
            }
            _ => shrink.row(row),
        }
    }
    Some(shrink.finish().map(|image| Loaded {
        image,
        natural,
        vector: false,
        frames: None,
        pages: 1,
    }))
}

/// The first bytes of a PNG.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Whether a PNG's header says its rows come in passes (Adam7), which
/// cannot be made smaller one row at a time.
fn png_interlaced(bytes: &[u8]) -> bool {
    bytes.get(28).is_some_and(|&i| i != 0)
}

fn decode_png(bytes: &[u8]) -> Result<Decoded, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palettes become RGB, 16 bits become 8, and a transparent colour key
    // becomes an alpha channel: four layouts are left to convert.
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    if !fits((reader.info().width, reader.info().height)) {
        return Err(TOO_LARGE.to_owned());
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| TOO_LARGE.to_owned())?;
    let mut buf = buffer(size)?;
    buf.resize(size, 0);
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let (channels, pixels): (usize, Vec<u8>) = match info.color_type {
        png::ColorType::Rgba => (4, buf),
        png::ColorType::Rgb => (3, buf),
        png::ColorType::Grayscale => {
            let mut rgb = buffer(buf.len().saturating_mul(3))?;
            rgb.extend(buf.iter().flat_map(|g| [*g, *g, *g]));
            (3, rgb)
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = buffer(buf.len().saturating_mul(2))?;
            rgba.extend(
                buf.as_chunks::<2>()
                    .0
                    .iter()
                    .flat_map(|c| [c[0], c[0], c[0], c[1]]),
            );
            (4, rgba)
        }
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
    if !fits((w as u32, h as u32)) {
        return Err(TOO_LARGE.to_owned());
    }
    let mut canvas = buffer(w * h * 4)?;
    canvas.resize(w * h * 4, 0);
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
    dec.decode_headers().map_err(|e| e.to_string())?;
    let info = dec
        .info()
        .ok_or_else(|| "the JPEG has no header".to_owned())?;
    if !fits((u32::from(info.width), u32::from(info.height))) {
        return Err(TOO_LARGE.to_owned());
    }
    let rgb = dec.decode().map_err(|e| e.to_string())?;
    Ok((u32::from(info.width), u32::from(info.height), 3, rgb))
}

/// A JPEG of `natural` size decoded halved up to `k` times (at most three:
/// an eighth) by its IDCT, so that the picture is never held whole, with
/// how many times it was halved. None for what is no JPEG or has no RGB
/// or grey 8-bit pixels; the whole picture is decoded then.
fn decode_jpeg_smaller<'a>(bytes: &[u8], natural: (u32, u32), k: u32) -> Option<(Raw<'a>, u32)> {
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return None;
    }
    let mut dec = jpeg_decoder::Decoder::new(bytes);
    dec.set_max_decoding_buffer_size(MOST_DECODE_COST);
    let side = |n: u32, s: u32| u16::try_from(n.div_ceil(1 << s)).ok();
    let want = k.min(3);
    let (w, h) = dec
        .scale(side(natural.0, want)?, side(natural.1, want)?)
        .ok()?;
    // The halvings the decoder chose, if they are halvings at all.
    let done =
        (0..=want).find(|&s| side(natural.0, s) == Some(w) && side(natural.1, s) == Some(h))?;
    let format = dec.info()?.pixel_format;
    if !matches!(
        format,
        jpeg_decoder::PixelFormat::RGB24 | jpeg_decoder::PixelFormat::L8
    ) {
        return None;
    }
    let pixels = dec.decode().ok()?;
    let channels = if format == jpeg_decoder::PixelFormat::L8 {
        1
    } else {
        3
    };
    if pixels.len() != usize::from(w) * usize::from(h) * channels {
        return None;
    }
    let data = if channels == 1 {
        let mut rgb = buffer(pixels.len() * 3).ok()?;
        rgb.extend(pixels.iter().flat_map(|&g| [g, g, g]));
        rgb
    } else {
        pixels
    };
    Some((
        Raw {
            width: u32::from(w),
            height: u32::from(h),
            channels: 3,
            premultiplied: false,
            data: std::borrow::Cow::Owned(data),
        },
        done,
    ))
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
            page: 0,
        }
    }

    /// A PDF document with a page of each size in points, each filled red
    /// over its left half.
    fn pdf(pages: &[(u32, u32)]) -> Vec<u8> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            String::new(),
        ];
        let mut kids = Vec::new();
        for (i, (w, h)) in pages.iter().enumerate() {
            kids.push(format!("{} 0 R", 3 + 2 * i));
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Contents {} 0 R >>",
                4 + 2 * i
            ));
            let paint = format!("1 0 0 rg 0 0 {} {h} re f", w / 2);
            objects.push(format!(
                "<< /Length {} >>\nstream\n{paint}\nendstream",
                paint.len()
            ));
        }
        objects[1] = format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        );
        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out += &format!("{} 0 obj\n{body}\nendobj\n", i + 1);
        }
        let xref = out.len();
        out += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for at in offsets {
            out += &format!("{at:010} 00000 n \n");
        }
        out += &format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        );
        out.into_bytes()
    }

    #[test]
    #[cfg(feature = "pdf")]
    fn a_pdf_page_is_drawn_at_its_size_in_points_over_nothing() {
        let bytes = pdf(&[(40, 20), (10, 30)]);
        let first = load_bytes("a.pdf", &bytes, Request::default()).unwrap();
        assert_eq!((first.pages, first.natural), (2, (40, 20)));
        assert!(first.vector);
        let px = first.image.data();
        let at = |x: usize, y: usize| &px[(y * 40 + x) * 4..][..4];
        assert_eq!(at(5, 10), [255, 0, 0, 255]);
        assert_eq!(at(35, 10), [0, 0, 0, 0]);
        let second = Request {
            page: 1,
            ..Request::default()
        };
        let second = load_bytes("a.pdf", &bytes, second).unwrap();
        assert_eq!(second.natural, (10, 30));
    }

    #[test]
    #[cfg(feature = "pdf")]
    fn a_pdf_page_there_is_not_is_the_first_and_a_size_asked_keeps_its_shape() {
        let bytes = pdf(&[(40, 20)]);
        let missing = Request {
            page: 5,
            ..Request::default()
        };
        assert_eq!(
            load_bytes("a.pdf", &bytes, missing).unwrap().natural,
            (40, 20)
        );
        let wider = load_bytes("a.pdf", &bytes, ask(100, 0, false)).unwrap();
        assert_eq!(wider.natural, (100, 50));
        let sharp = load_bytes("a.pdf", &bytes, Request::default().at_scale(2.0)).unwrap();
        assert_eq!((sharp.image.width(), sharp.image.height()), (80, 40));
    }

    #[test]
    #[cfg(feature = "pdf")]
    fn a_broken_pdf_is_an_error() {
        let err = load_bytes("a.pdf", b"%PDF-1.4\nnothing", Request::default()).unwrap_err();
        assert!(matches!(err, LoadError::Decode { .. }), "{err:?}");
    }

    #[test]
    #[cfg(not(feature = "pdf"))]
    fn without_the_pdf_feature_a_document_is_an_unsupported_format() {
        let err = load_bytes("file:///a.pdf", &pdf(&[(40, 20)]), Request::default()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Error decoding: file:///a.pdf: Unsupported image format"
        );
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
        let firsts: Vec<Vec<u8>> = frames
            .images
            .iter()
            .map(|i| i.data()[..4].to_vec())
            .collect();
        assert_eq!(
            firsts,
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
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
    fn a_progressive_jpeg_costs_its_coefficients_too() {
        let encode = |progressive: bool| {
            let mut jpeg = Vec::new();
            let mut enc = jpeg_encoder::Encoder::new(&mut jpeg, 90);
            enc.set_progressive(progressive);
            enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
            enc.encode(&[0; 64 * 32 * 3], 64, 32, jpeg_encoder::ColorType::Rgb)
                .unwrap();
            jpeg
        };
        let small = Request {
            limit: 16,
            ..Request::default()
        };
        // Kept 16 by 8 at four bytes a pixel, decoded 16 by 8 at three.
        assert_eq!(cost(&encode(false), small), 16 * 8 * 4 + 16 * 8 * 3);
        // And three components of 64 by 32 at two bytes a sample.
        assert_eq!(
            cost(&encode(true), small),
            16 * 8 * 4 + 16 * 8 * 3 + 3 * 64 * 32 * 2
        );
    }

    #[test]
    fn a_header_says_the_size_without_decoding() {
        let png = png_of(5, 3, png::ColorType::Rgb, &[0; 45]);
        assert_eq!(dimensions(&png), Some((5, 3)));
        assert_eq!(cost(&png, Request::default()), 120, "decoded and kept");
        assert_eq!(dimensions(&gif(&RGB, 10)), Some((3, 2)));
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 90)
            .encode(&[0; 7 * 4 * 3], 7, 4, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        assert_eq!(dimensions(&jpeg), Some((7, 4)));
        assert_eq!(dimensions(WIDE), None);
        assert_eq!(cost(WIDE, Request::default()), 0);
        assert_eq!(dimensions(&[0xff, 0xd8, 0xff]), None, "cut short");
    }

    #[test]
    fn a_picture_is_kept_as_long_as_the_limit() {
        assert_eq!(kept_size((3000, 2000), 0), (3000, 2000));
        assert_eq!(kept_size((3000, 2000), 4096), (3000, 2000));
        assert_eq!(kept_size((3000, 2000), 3000), (3000, 2000));
        assert_eq!(kept_size((3000, 2000), 1024), (1024, 683));
        assert_eq!(kept_size((2000, 3000), 896), (597, 896));
        assert_eq!(kept_size((4000, 1), 512), (512, 1), "never empty");
    }

    #[test]
    fn a_picture_is_halved_while_it_covers_what_is_kept() {
        assert_eq!(halvings((3000, 2000), (3000, 2000)), 0);
        assert_eq!(halvings((3000, 2000), (1500, 1000)), 1);
        assert_eq!(halvings((3000, 2000), (1024, 683)), 1);
        assert_eq!(halvings((3000, 2000), (375, 250)), 3);
        assert_eq!(halvings((1023, 10), (512, 5)), 1);
        assert_eq!(halvings((1021, 10), (512, 5)), 0);
        assert_eq!(halvings((1, 1), (1, 1)), 0);
    }

    #[test]
    fn limits_go_up_by_quarter_octaves() {
        let steps: Vec<u32> = [0.5, 1.0, 3.0, 4.0, 5.0, 6.5, 800.0, 1024.0, 1024.5, 1500.0]
            .into_iter()
            .map(step_up)
            .collect();
        assert_eq!(steps, [1, 1, 4, 4, 5, 7, 896, 1024, 1280, 1536]);
    }

    #[test]
    fn shrinking_to_a_size_averages_the_area_each_pixel_covers() {
        let raw = Raw {
            width: 30,
            height: 20,
            channels: 3,
            premultiplied: false,
            data: std::borrow::Cow::Owned((0..30 * 20).flat_map(|_| [40, 80, 120]).collect()),
        };
        let image = raw.shrunk_to((12, 8), "file:///a.png").unwrap();
        assert_eq!((image.width(), image.height()), (12, 8));
        assert!(image.data().chunks(4).all(|px| px == [40, 80, 120, 255]));

        // Four pixels made three: each takes three quarters of one and a
        // quarter of its neighbour, the transparent one adding no colour.
        let raw = Raw {
            width: 4,
            height: 1,
            channels: 4,
            premultiplied: false,
            data: std::borrow::Cow::Owned(
                [0, 60, 120]
                    .iter()
                    .flat_map(|&v| [v, v, v, 255])
                    .chain([180, 180, 180, 0])
                    .collect(),
            ),
        };
        let image = raw.shrunk_to((3, 1), "file:///a.png").unwrap();
        assert_eq!(
            image.data(),
            &[15, 15, 15, 255, 90, 90, 90, 255, 30, 30, 30, 64]
        );
    }

    #[test]
    fn every_pixel_goes_whole_into_pixels_not_yet_finished() {
        for from in 1..200 {
            for to in 1..=from {
                let spans = Spans::new(from, to);
                let mut shares = vec![0f32; to];
                for i in 0..from {
                    let mut total = 0.0;
                    for (o, share) in spans.of(i) {
                        assert!(spans.ends_at(o) >= i, "{from} to {to}: {i} after {o} ended");
                        shares[o] += share;
                        total += share;
                    }
                    assert!((total * from as f32 / to as f32 - 1.0).abs() < 1e-4);
                }
                assert!(
                    shares.iter().all(|s| (s - 1.0).abs() < 1e-4),
                    "{from} to {to}"
                );
                assert_eq!(spans.ends_at(to - 1), from - 1);
            }
        }
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

    /// A picture `w` by `h` of `colour` whose every pixel differs.
    fn patterned(w: u32, h: u32, colour: png::ColorType) -> Vec<u8> {
        let n = colour.samples();
        let data: Vec<u8> = (0..w * h * n as u32)
            .map(|i| (i.wrapping_mul(37) ^ (i >> 3)) as u8)
            .collect();
        png_of(w, h, colour, &data)
    }

    #[test]
    fn a_png_kept_smaller_is_made_smaller_as_its_rows_come() {
        use png::ColorType::*;
        for colour in [Rgb, Rgba, Grayscale, GrayscaleAlpha] {
            let bytes = patterned(67, 41, colour);
            // An even halving and any other size.
            for limit in [34, 20] {
                let streamed = png_shrunk("file:///a.png", &bytes, limit)
                    .expect("taken row by row")
                    .unwrap();
                let whole = decode_raw("file:///a.png", &bytes)
                    .unwrap()
                    .shrunk_to(kept_size((67, 41), limit), "file:///a.png")
                    .unwrap();
                assert_eq!(streamed.natural, (67, 41));
                assert_eq!(
                    (streamed.image.width(), streamed.image.height()),
                    (whole.width(), whole.height())
                );
                assert_eq!(streamed.image.data(), whole.data(), "{colour:?} at {limit}");
            }
            // Kept whole, it is decoded whole.
            assert!(png_shrunk("file:///a.png", &bytes, 0).is_none());
            assert!(png_shrunk("file:///a.png", &bytes, 67).is_none());
        }
    }

    #[test]
    fn an_interlaced_png_is_decoded_whole() {
        let mut bytes = patterned(40, 40, png::ColorType::Rgb);
        // The header's interlace method; its checksum is not checked here.
        bytes[28] = 1;
        assert!(png_interlaced(&bytes));
        assert!(png_shrunk("file:///a.png", &bytes, 10).is_none());
    }

    #[test]
    fn a_png_cut_short_cannot_be_read_row_by_row_either() {
        let bytes = patterned(64, 64, png::ColorType::Rgba);
        let cut = &bytes[..bytes.len() / 2];
        let err = png_shrunk("file:///cut.png", cut, 16)
            .expect("taken row by row")
            .unwrap_err();
        assert!(matches!(err, LoadError::Decode { .. }), "{err:?}");
    }

    #[test]
    fn a_png_made_smaller_as_it_is_decoded_costs_a_row() {
        let bytes = patterned(400, 300, png::ColorType::Rgba);
        let limit = |limit| Request {
            limit,
            ..Request::default()
        };
        assert_eq!(cost(&bytes, limit(100)), 100 * 75 * 4 + 400 * 8);
        // Whole, it is the picture and what it is made from.
        assert_eq!(cost(&bytes, limit(0)), 400 * 300 * 4 * 2);
    }

    #[test]
    fn a_jpeg_kept_smaller_is_decoded_smaller() {
        let data: Vec<u8> = (0..64 * 32).flat_map(|_| [32u8, 96, 160]).collect();
        let mut bytes = Vec::new();
        jpeg_encoder::Encoder::new(&mut bytes, 95)
            .encode(&data, 64, 32, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let (raw, done) = decode_jpeg_smaller(&bytes, (64, 32), 5).unwrap();
        assert_eq!(
            (raw.width, raw.height, done),
            (8, 4, 3),
            "an eighth at most"
        );
        for (limit, size) in [(32, (32, 16)), (24, (24, 12)), (8, (8, 4)), (4, (4, 2))] {
            let request = Request {
                limit,
                ..Request::default()
            };
            let loaded = load_bytes("file:///a.jpg", &bytes, request).unwrap();
            assert_eq!(loaded.natural, (64, 32));
            assert_eq!((loaded.image.width(), loaded.image.height()), size);
            let px = &loaded.image.data()[..4];
            for (got, want) in px.iter().zip([32u8, 96, 160, 255]) {
                assert!(got.abs_diff(want) <= 4, "{px:?}");
            }
        }
    }

    /// `bytes` with the size their header says made `width` by `height`,
    /// the rest as it was: a picture far larger than its data, which no
    /// decoder may be let near.
    fn claiming(bytes: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut out = bytes.to_vec();
        if out.starts_with(PNG) {
            out[16..20].copy_from_slice(&width.to_be_bytes());
            out[20..24].copy_from_slice(&height.to_be_bytes());
            let crc = crc32(&out[12..29]);
            out[29..33].copy_from_slice(&crc.to_be_bytes());
        } else if out.starts_with(GIF) {
            out[6..8].copy_from_slice(&(width as u16).to_le_bytes());
            out[8..10].copy_from_slice(&(height as u16).to_le_bytes());
        } else {
            let sof = out.windows(2).position(|m| m == [0xff, 0xc0]).unwrap();
            out[sof + 5..sof + 7].copy_from_slice(&(height as u16).to_be_bytes());
            out[sof + 7..sof + 9].copy_from_slice(&(width as u16).to_be_bytes());
        }
        out
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
            }
        }
        !crc
    }

    fn too_large_for(url: &str) -> Result<Loaded, LoadError> {
        Err(LoadError::decode(url, TOO_LARGE))
    }

    fn sized(width: u32, height: u32) -> Request {
        Request {
            width,
            height,
            ..Request::default()
        }
    }

    #[test]
    fn a_png_saying_it_is_huge_is_refused_before_it_is_decoded() {
        let png = claiming(
            &png_of(1, 1, png::ColorType::Rgba, &[0; 4]),
            100_000,
            100_000,
        );
        assert_eq!(dimensions(&png), Some((100_000, 100_000)));
        let url = "file:///huge.png";
        let load = |request| load_bytes(url, &png, request).map(|_| ());
        assert_eq!(load(Request::default()), too_large_for(url).map(|_| ()));
        // A sourceSize decodes it whole, so it is refused as well.
        assert_eq!(load(sized(100, 100)), too_large_for(url).map(|_| ()));
        // The decoder itself refuses it before it makes room for it.
        assert_eq!(decode_png(&png).err().as_deref(), Some(TOO_LARGE));
    }

    #[test]
    fn a_jpeg_saying_it_is_huge_is_refused_before_it_is_decoded() {
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 90)
            .encode(&[0; 16 * 16 * 3], 16, 16, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let jpeg = claiming(&jpeg, 16_000, 16_000);
        assert_eq!(dimensions(&jpeg), Some((16_000, 16_000)));
        let url = "file:///huge.jpg";
        let load = |request| load_bytes(url, &jpeg, request).map(|_| ());
        assert_eq!(load(Request::default()), too_large_for(url).map(|_| ()));
        assert_eq!(load(sized(100, 0)), too_large_for(url).map(|_| ()));
        assert_eq!(decode_jpeg(&jpeg).err().as_deref(), Some(TOO_LARGE));
    }

    #[test]
    fn a_gif_saying_it_is_huge_is_refused_before_it_is_decoded() {
        let gif = claiming(&gif(&RGB, 10), 65_535, 65_535);
        let url = "file:///huge.gif";
        assert_eq!(
            load_bytes(url, &gif, Request::default()).map(|_| ()),
            too_large_for(url).map(|_| ())
        );
        assert_eq!(decode_gif(&gif, false).err().as_deref(), Some(TOO_LARGE));
    }

    #[test]
    fn a_size_asked_for_cannot_make_a_picture_huge() {
        let png = png_of(2, 1, png::ColorType::Rgba, &[0; 8]);
        let url = "file:///wide.png";
        let cover = Request {
            width: 60_000,
            height: 60_000,
            cover: true,
            ..Request::default()
        };
        assert_eq!(
            load_bytes(url, &png, cover).map(|_| ()),
            too_large_for(url).map(|_| ())
        );
        // What is within the budget is made as before.
        let loaded = load_bytes(url, &png, ask(40, 20, true)).unwrap();
        assert_eq!((loaded.image.width(), loaded.image.height()), (40, 20));
    }

    #[test]
    fn the_largest_picture_let_decode_is_within_the_decode_budget() {
        assert!(fits((8192, 8192)));
        assert!(!fits((8192, 8193)));
        assert!(!fits((u32::MAX, u32::MAX)), "a count that overflows");
        let png = claiming(&png_of(1, 1, png::ColorType::Rgba, &[0; 4]), 8192, 8192);
        assert_eq!(cost(&png, Request::default()), MOST_DECODE_COST);
    }
}
