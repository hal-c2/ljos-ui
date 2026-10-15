//! `MediaMetaData`: what a file says about itself, keyed by
//! `MediaMetaData.Key`. Qt hands it out as a value type; here it is an
//! uncreatable object a player owns, with the same methods.

use std::sync::Arc;

use ljos_engine::{Atom, QmlObject, QtObject, Size, Value, qml_impl};

/// One key's value, made on the thread that opened the file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetaValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Size(u32, u32),
    /// A `QMediaFormat` or rotation enum value, and how Qt says it.
    Enum(i32, String),
    /// Milliseconds.
    Duration(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub key: i32,
    pub value: MetaValue,
}

pub(crate) mod key {
    pub const TITLE: i32 = 0;
    pub const LANGUAGE: i32 = 6;
    pub const DURATION: i32 = 10;
    pub const FILE_FORMAT: i32 = 12;
    pub const AUDIO_BIT_RATE: i32 = 13;
    pub const AUDIO_CODEC: i32 = 14;
    pub const VIDEO_BIT_RATE: i32 = 15;
    pub const VIDEO_CODEC: i32 = 16;
    pub const VIDEO_FRAME_RATE: i32 = 17;
    pub const ORIENTATION: i32 = 26;
    pub const RESOLUTION: i32 = 27;
    pub const HAS_HDR_CONTENT: i32 = 28;
}

/// The keys in declaration order, with what `metaDataKeyToString` says.
const KEYS: [(&str, &str); 29] = [
    ("Title", "Title"),
    ("Author", "Author"),
    ("Comment", "Comment"),
    ("Description", "Description"),
    ("Genre", "Genre"),
    ("Date", "Date"),
    ("Language", "Language"),
    ("Publisher", "Publisher"),
    ("Copyright", "Copyright"),
    ("Url", "Url"),
    ("Duration", "Duration"),
    ("MediaType", "Media type"),
    ("FileFormat", "Container Format"),
    ("AudioBitRate", "Audio bit rate"),
    ("AudioCodec", "Audio codec"),
    ("VideoBitRate", "Video bit rate"),
    ("VideoCodec", "Video codec"),
    ("VideoFrameRate", "Video frame rate"),
    ("AlbumTitle", "Album title"),
    ("AlbumArtist", "Album artist"),
    ("ContributingArtist", "Contributing artist"),
    ("TrackNumber", "Track number"),
    ("Composer", "Composer"),
    ("LeadPerformer", "Lead performer"),
    ("ThumbnailImage", "Thumbnail image"),
    ("CoverArtImage", "Cover art image"),
    ("Orientation", "Orientation"),
    ("Resolution", "Resolution"),
    ("HasHdrContent", "Has HDR content"),
];

fn meta_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Key",
        KEYS.iter()
            .zip(0i64..)
            .map(|((name, _), i)| (*name, i))
            .collect(),
    )]
}

/// `MediaMetaData` (QtMultimedia): a set of key/value pairs.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "MediaMetaData",
    extends = QtObject,
    enums = meta_enums,
    uncreatable
)]
pub struct MediaMetaData {
    #[qml(base)]
    pub object: QtObject,
    #[qml(skip)]
    entries: Vec<Entry>,
}

impl MediaMetaData {
    pub(crate) fn with(entries: Vec<Entry>) -> MediaMetaData {
        MediaMetaData {
            object: QtObject::default(),
            entries,
        }
    }

    pub(crate) fn replace(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
    }

    fn find(&self, key: i32) -> Option<&MetaValue> {
        self.entries.iter().find(|e| e.key == key).map(|e| &e.value)
    }
}

#[qml_impl]
impl MediaMetaData {
    /// The value of `key`, `undefined` when there is none.
    #[qml(invokable)]
    fn value(&self, key: i32) -> Value {
        match self.find(key) {
            None => Value::Undefined,
            Some(MetaValue::Text(s)) => Value::from(s.as_str()),
            Some(MetaValue::Number(n) | MetaValue::Duration(n)) => Value::Real(*n),
            Some(MetaValue::Bool(b)) => Value::Bool(*b),
            Some(MetaValue::Size(w, h)) => Value::Size(Arc::new(Size {
                width: f64::from(*w),
                height: f64::from(*h),
            })),
            Some(MetaValue::Enum(v, _)) => Value::Int(*v),
        }
    }

    /// Sets `key` to `value`.
    #[qml(invokable)]
    fn insert(&mut self, key: i32, value: Value) {
        let value = match &value {
            Value::Bool(b) => MetaValue::Bool(*b),
            Value::Int(i) => MetaValue::Number(f64::from(*i)),
            Value::Real(r) => MetaValue::Number(*r),
            Value::Size(s) => MetaValue::Size(s.width as u32, s.height as u32),
            other => MetaValue::Text(other.to_js_string().to_string()),
        };
        match self.entries.iter_mut().find(|e| e.key == key) {
            Some(e) => e.value = value,
            None => self.entries.push(Entry { key, value }),
        }
    }

    /// Takes `key` out.
    #[qml(invokable)]
    fn remove(&mut self, key: i32) {
        self.entries.retain(|e| e.key != key);
    }

    /// The keys that have a value.
    #[qml(invokable)]
    fn keys(&self) -> Value {
        Value::List(Arc::new(
            self.entries.iter().map(|e| Value::Int(e.key)).collect(),
        ))
    }

    /// Takes every key out.
    #[qml(invokable)]
    fn clear(&mut self) {
        self.entries.clear();
    }

    #[qml(invokable, name = "isEmpty")]
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value of `key` as text: `""` when there is none.
    #[qml(invokable, name = "stringValue")]
    fn string_value(&self, key: i32) -> Atom {
        Atom::from(self.find(key).map(text_of).unwrap_or_default().as_str())
    }

    /// What `key` is called, for showing it.
    #[qml(invokable, name = "metaDataKeyToString")]
    fn key_to_string(&self, key: i32) -> Atom {
        usize::try_from(key)
            .ok()
            .and_then(|k| KEYS.get(k))
            .map_or(Atom::EMPTY, |(_, text)| Atom::from(*text))
    }
}

fn text_of(v: &MetaValue) -> String {
    match v {
        MetaValue::Text(s) | MetaValue::Enum(_, s) => s.clone(),
        MetaValue::Number(n) => number(*n),
        MetaValue::Bool(b) => b.to_string(),
        MetaValue::Size(w, h) => format!("{w} x {h}"),
        MetaValue::Duration(ms) => {
            let s = (*ms / 1000.0).floor() as u64;
            format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        }
    }
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// `QMediaFormat::VideoCodec` and its name.
fn video_codec(codec: &str) -> (i32, String) {
    let c = codec.to_ascii_lowercase();
    let known = [
        (&["264", "avc"][..], 3, "H264"),
        (&["265", "hevc", "hvc", "hev"][..], 4, "H265"),
        (&["vp8"][..], 5, "VP8"),
        (&["vp9", "vp09"][..], 6, "VP9"),
        (&["av1", "av01"][..], 7, "AV1"),
        (&["theora"][..], 8, "Theora"),
        (&["mjpeg", "jpeg"][..], 10, "MotionJPEG"),
        (&["mpeg4", "mp4v"][..], 2, "MPEG4"),
        (&["mpeg2", "mp2v"][..], 1, "MPEG2"),
    ];
    known
        .iter()
        .find(|(names, _, _)| names.iter().any(|n| c.contains(n)))
        .map_or((-1, codec.to_owned()), |(_, v, s)| (*v, (*s).to_owned()))
}

/// `QMediaFormat::AudioCodec` and its name.
fn audio_codec(codec: &str) -> (i32, String) {
    let c = codec.to_ascii_lowercase();
    let known = [
        (&["aac", "mp4a"][..], 1, "AAC"),
        (&["eac3", "ec-3"][..], 3, "EAC3"),
        (&["ac3", "ac-3"][..], 2, "AC3"),
        (&["mp3", "mpeg audio"][..], 0, "MP3"),
        (&["flac"][..], 4, "FLAC"),
        (&["opus"][..], 6, "Opus"),
        (&["vorbis"][..], 7, "Vorbis"),
        (&["alac"][..], 10, "ALAC"),
        (&["pcm", "wav", "lpcm", "s16", "f32", "s24"][..], 8, "Wave"),
    ];
    known
        .iter()
        .find(|(names, _, _)| names.iter().any(|n| c.contains(n)))
        .map_or((-1, codec.to_owned()), |(_, v, s)| (*v, (*s).to_owned()))
}

/// `QMediaFormat::FileFormat` and its name.
fn file_format(container: &str) -> (i32, String) {
    let c = container.to_ascii_lowercase();
    let known = [
        (&["webm"][..], 6, "WebM"),
        (&["matroska", "mkv"][..], 2, "Matroska"),
        (&["quicktime", "mov"][..], 5, "QuickTime"),
        (&["m4a"][..], 7, "MPEG-4 Audio"),
        (&["mp4", "m4v", "isobmff", "iso"][..], 3, "MPEG-4"),
        (&["ogg"][..], 4, "Ogg"),
        (&["avi"][..], 1, "AVI"),
        (&["wav", "wave"][..], 12, "Wave"),
        (&["mp3"][..], 10, "MP3"),
        (&["flac"][..], 11, "FLAC"),
    ];
    known
        .iter()
        .find(|(names, _, _)| names.iter().any(|n| c.contains(n)))
        .map_or((-1, container.to_owned()), |(_, v, s)| {
            (*v, (*s).to_owned())
        })
}

/// The file's metadata, and each audio and video track's.
#[cfg(feature = "decode")]
pub(crate) fn describe(
    info: &filmcraft_media::MediaInfo,
    duration_ms: f64,
    title: Option<String>,
) -> (Vec<Entry>, Vec<Vec<Entry>>, Vec<Vec<Entry>>) {
    let e = |key, value| Entry { key, value };
    let mut file = vec![e(key::DURATION, MetaValue::Duration(duration_ms))];
    let mut audio_tracks = Vec::new();
    let mut video_tracks = Vec::new();
    if let Some(rate) = info.video.as_ref().and_then(|v| v.bitrate) {
        file.push(e(key::VIDEO_BIT_RATE, MetaValue::Number(rate as f64)));
    }
    let (fmt, fmt_name) = file_format(&info.container);
    file.push(e(key::FILE_FORMAT, MetaValue::Enum(fmt, fmt_name)));
    if let Some(a) = info.audio() {
        let (c, name) = audio_codec(&a.codec);
        file.push(e(key::AUDIO_CODEC, MetaValue::Enum(c, name)));
    }
    if let Some(title) = title {
        file.push(e(key::TITLE, MetaValue::Text(title)));
    }
    if let Some(v) = &info.video {
        let fps = if v.frame_rate.den != 0 {
            v.frame_rate.num as f64 / v.frame_rate.den as f64
        } else {
            0.0
        };
        let (c, name) = video_codec(&v.codec);
        let video = [
            e(key::ORIENTATION, MetaValue::Enum(0, "None".to_owned())),
            e(key::HAS_HDR_CONTENT, MetaValue::Bool(v.hdr.is_some())),
            e(key::VIDEO_CODEC, MetaValue::Enum(c, name)),
            e(key::VIDEO_FRAME_RATE, MetaValue::Number(fps)),
            e(key::RESOLUTION, MetaValue::Size(v.width, v.height)),
        ];
        file.extend(video.iter().cloned());
        let mut track = vec![e(key::DURATION, MetaValue::Duration(duration_ms))];
        if let Some(rate) = v.bitrate {
            track.push(e(key::VIDEO_BIT_RATE, MetaValue::Number(rate as f64)));
        }
        track.push(e(key::LANGUAGE, MetaValue::Text("Default".to_owned())));
        track.extend(video);
        video_tracks.push(track);
    }
    for a in &info.audio_streams {
        let (c, name) = audio_codec(&a.codec);
        audio_tracks.push(vec![
            e(key::DURATION, MetaValue::Duration(duration_ms)),
            e(key::LANGUAGE, MetaValue::Text("Default".to_owned())),
            e(key::AUDIO_CODEC, MetaValue::Enum(c, name)),
        ]);
    }
    let _ = key::AUDIO_BIT_RATE;
    (file, audio_tracks, video_tracks)
}

/// The title an MP4 or QuickTime file carries: `moov/udta/meta/ilst/©nam`
/// as an iTunes-style tag, or `moov/udta/©nam` as QuickTime writes it.
#[cfg(feature = "decode")]
pub(crate) fn mp4_title(r: &dyn filmcraft_media::ByteReader) -> Option<String> {
    let moov = child(r, 0, r.len(), *b"moov")?;
    let udta = child(r, moov.0, moov.1, *b"udta")?;
    let nam = [0xA9, b'n', b'a', b'm'];
    if let Some(meta) = child(r, udta.0, udta.1, *b"meta") {
        // `meta` is a full box in MP4 (4 bytes of version and flags) and
        // a plain one in QuickTime: its first child is `hdlr` either way.
        let mut start = meta.0;
        let mut peek = [0u8; 8];
        if r.read_at(start, &mut peek).is_ok() && &peek[4..8] != b"hdlr" {
            start += 4;
        }
        if let Some(ilst) = child(r, start, meta.1, *b"ilst")
            && let Some(item) = child(r, ilst.0, ilst.1, nam)
            && let Some(data) = child(r, item.0, item.1, *b"data")
        {
            // Type and locale, then the text.
            return text(r, data.0 + 8, data.1);
        }
    }
    let item = child(r, udta.0, udta.1, nam)?;
    // A 16-bit length and a language code, then the text.
    text(r, item.0 + 4, item.1)
}

#[cfg(feature = "decode")]
fn text(r: &dyn filmcraft_media::ByteReader, from: u64, to: u64) -> Option<String> {
    let len = usize::try_from(to.checked_sub(from)?).ok()?;
    if len > 4096 {
        return None;
    }
    let mut buf = vec![0u8; len];
    r.read_at(from, &mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf)
        .trim_end_matches('\0')
        .to_owned();
    (!s.is_empty()).then_some(s)
}

/// The payload of the first box of `kind` among those in `[from, to)`.
#[cfg(feature = "decode")]
fn child(
    r: &dyn filmcraft_media::ByteReader,
    from: u64,
    to: u64,
    kind: [u8; 4],
) -> Option<(u64, u64)> {
    let mut at = from;
    while at + 8 <= to {
        let mut head = [0u8; 8];
        r.read_at(at, &mut head).ok()?;
        let size32 = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        let (size, header) = match size32 {
            0 => (to - at, 8),
            1 => {
                let mut large = [0u8; 8];
                r.read_at(at + 8, &mut large).ok()?;
                (u64::from_be_bytes(large), 16)
            }
            n => (u64::from(n), 8),
        };
        if size < header || at + size > to {
            return None;
        }
        if head[4..8] == kind {
            return Some((at + header, at + size));
        }
        at += size;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_reads_as_a_clock() {
        assert_eq!(text_of(&MetaValue::Duration(1000.0)), "00:00:01");
        assert_eq!(text_of(&MetaValue::Duration(3_725_000.0)), "01:02:05");
    }

    #[test]
    fn codecs_have_qt_names() {
        assert_eq!(video_codec("avc1"), (3, "H264".to_owned()));
        assert_eq!(audio_codec("mp4a.40.2"), (1, "AAC".to_owned()));
        assert_eq!(file_format("mp4"), (3, "MPEG-4".to_owned()));
    }
}
