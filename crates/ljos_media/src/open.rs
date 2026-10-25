//! Opening a source off the Engine thread: a resource the host
//! registered, a local file or a fetched one, demuxed far enough to know
//! its streams, duration and metadata. The answer is a platform reply,
//! so whoever asked sees `LoadingMedia` first, as in Qt.

use std::io::Read;

use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_platform::{Platform, is_remote, url_path};

#[cfg(feature = "decode")]
use std::sync::Arc;

use crate::meta::Entry;
use crate::sink::{Format, Soon};
use crate::stream::{MediaStream, StreamFormat, StreamOptions};

/// What opening found.
pub(crate) struct Opened {
    pub duration_ms: f64,
    pub has_audio: bool,
    pub video: Option<VideoInfo>,
    /// The whole file's metadata, in the order `keys()` lists it.
    pub meta: Vec<Entry>,
    pub audio_tracks: Vec<Vec<Entry>>,
    pub video_tracks: Vec<Vec<Entry>>,
    /// The sound decoded whole for a device of this format, when asked.
    pub samples: Option<std::sync::Arc<[f32]>>,
    #[cfg(feature = "decode")]
    pub source: filmcraft_media::SharedSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VideoInfo {
    pub fps: f64,
    pub width: u32,
    pub height: u32,
}

/// Why a source did not open, in `MediaPlayer.Error`'s terms.
#[derive(Clone, Debug)]
pub(crate) enum OpenError {
    /// There is nothing to read there.
    Resource,
    /// Something is there that cannot be played.
    Format(String),
    /// The network said no.
    Network(String),
}

impl OpenError {
    /// `MediaPlayer.Error`.
    pub(crate) fn code(&self) -> i32 {
        match self {
            OpenError::Resource => 1,
            OpenError::Format(_) => 2,
            OpenError::Network(_) => 3,
        }
    }

    /// `MediaPlayer.errorString`, as Qt's FFmpeg backend words it where
    /// it was seen.
    pub(crate) fn message(&self) -> String {
        match self {
            OpenError::Resource => "Could not open file".to_owned(),
            OpenError::Format(why) | OpenError::Network(why) => why.clone(),
        }
    }
}

/// Where the answer goes: `(engine, object, generation, outcome)`. A plain
/// function, so the thread that opens carries nothing of the Engine's.
pub(crate) type Done = fn(&Engine, ObjectId, u64, Result<Opened, OpenError>);

/// The source being opened is a stream of pictures, with no container:
/// `(engine, object, generation, format, options)`, answered by the
/// stream to write it into, or nothing when it is no longer wanted.
pub(crate) type Streams =
    fn(&Engine, ObjectId, u64, StreamFormat, StreamOptions) -> Option<MediaStream>;

/// Opens `url` (already resolved) for `me`, answering through `done` on
/// the Engine thread. `samples` asks for the sound decoded whole, for a
/// device of that format, which the thread opening it waits for. A remote source that `streams` can take is
/// read into it as it arrives, instead of whole before it plays.
pub(crate) fn start(
    e: &Engine,
    me: ObjectId,
    generation: u64,
    url: String,
    samples: Option<Soon>,
    done: Done,
    streams: Option<Streams>,
) {
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(&url) {
        let bytes: Vec<u8> = bytes.to_vec();
        spawn(e, me, generation, done, move || {
            from_bytes(&name_of(&url), bytes, samples.and_then(|s| s.wait()))
        });
        return;
    }
    if is_remote(&url) {
        let name = name_of(&url);
        let Some(streams) = streams else {
            platform.fetch_for(Some(me), &url, move |e, got| match got {
                Ok(bytes) => spawn(e, me, generation, done, move || {
                    from_bytes(&name, bytes, samples.and_then(|s| s.wait()))
                }),
                Err(err) => done(e, me, generation, Err(OpenError::Network(err.to_string()))),
            });
            return;
        };
        let fetcher = platform.fetcher();
        let reply = platform.expect_for(format!("open {url}"), Some(me));
        let spawned = std::thread::Builder::new()
            .name("ljos-media-fetch".to_owned())
            .spawn(move || {
                let mut incoming = match fetcher.stream(&url) {
                    Ok(incoming) => incoming,
                    Err(err) => {
                        let err = OpenError::Network(err.to_string());
                        reply.send(move |e| done(e, me, generation, Err(err)));
                        return;
                    }
                };
                let mut head = Vec::new();
                let read = fill(&mut *incoming.body, &mut head, SNIFF);
                let Some((format, options)) = read
                    .is_ok()
                    .then(|| stream_kind(incoming.content_type.as_deref(), &head))
                    .flatten()
                else {
                    let outcome = read
                        .and_then(|()| incoming.body.read_to_end(&mut head).map(drop))
                        .map_err(|err| OpenError::Network(err.to_string()))
                        .and_then(|()| from_bytes(&name, head, samples.and_then(|s| s.wait())));
                    reply.send(move |e| done(e, me, generation, outcome));
                    return;
                };
                let (tx, rx) = std::sync::mpsc::channel();
                reply.send(move |e| {
                    let _ = tx.send(streams(e, me, generation, format, options));
                });
                if let Ok(Some(stream)) = rx.recv() {
                    pass_on(&mut *incoming.body, head, &stream);
                }
            });
        if let Err(err) = spawned {
            e.log(
                LogLevel::Warning,
                crate::CATEGORY,
                format!("no thread to fetch media with: {err}"),
            );
            done(e, me, generation, Err(OpenError::Format(err.to_string())));
        }
        return;
    }
    let real = url_path(&url).map(|p| platform.env().real(&p));
    spawn(e, me, generation, done, move || match real {
        Some(path) => from_path(&path, samples.and_then(|s| s.wait())),
        None => Err(OpenError::Resource),
    });
}

/// Bytes read before a remote source is known by them.
const SNIFF: usize = 8;

/// What is read from a remote stream at a time.
const PIECE: usize = 64 * 1024;

/// Reads into `into` until it has `want` bytes or the body ends.
fn fill(body: &mut dyn Read, into: &mut Vec<u8>, want: usize) -> std::io::Result<()> {
    let mut piece = [0; SNIFF];
    while into.len() < want {
        match body.read(&mut piece[..want - into.len()]) {
            Ok(0) => break,
            Ok(n) => into.extend_from_slice(&piece[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Whether a remote body is a stream of pictures with no container, and
/// how it is played: by what the server calls it, else by how it
/// begins. A server that pushes picture after picture
/// (`multipart/x-mixed-replace`, a camera's) is played live; a stream
/// served as a file is paced, every picture in its turn. Anything else
/// is a container, read whole.
fn stream_kind(content_type: Option<&str>, head: &[u8]) -> Option<(StreamFormat, StreamOptions)> {
    let paced = StreamOptions::default();
    match content_type {
        Some("multipart/x-mixed-replace") => {
            return Some((
                StreamFormat::Mjpeg,
                StreamOptions {
                    live: true,
                    frame_rate: None,
                },
            ));
        }
        Some("video/h264") => return Some((StreamFormat::H264AnnexB, paced)),
        _ => {}
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some((StreamFormat::Mjpeg, paced));
    }
    // A start code, and not an MP4 box with a 64-bit size.
    let start_code = head.starts_with(&[0, 0, 1]) || head.starts_with(&[0, 0, 0, 1]);
    (start_code && head.get(4..8) != Some(b"ftyp")).then_some((StreamFormat::H264AnnexB, paced))
}

/// Writes what was read and the rest of the body into `stream`, until
/// either ends. A body cut short ends the stream where it was cut.
fn pass_on(body: &mut dyn Read, head: Vec<u8>, stream: &MediaStream) {
    if stream.write(&head).is_err() {
        return;
    }
    let mut piece = vec![0; PIECE];
    loop {
        match body.read(&mut piece) {
            Ok(0) => break,
            Ok(n) => {
                if stream.write(&piece[..n]).is_err() {
                    return;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    stream.finish();
}

fn name_of(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_owned()
}

fn spawn(
    e: &Engine,
    me: ObjectId,
    generation: u64,
    done: Done,
    work: impl FnOnce() -> Result<Opened, OpenError> + Send + 'static,
) {
    let reply = Platform::of(e).expect_for("media decode", Some(me));
    let spawned = std::thread::Builder::new()
        .name("ljos-media-open".to_owned())
        .spawn(move || {
            let outcome = work();
            reply.send(move |e| done(e, me, generation, outcome));
        });
    if let Err(err) = spawned {
        e.log(
            LogLevel::Warning,
            crate::CATEGORY,
            format!("no thread to open media with: {err}"),
        );
        done(e, me, generation, Err(OpenError::Format(err.to_string())));
    }
}

#[cfg(not(feature = "decode"))]
pub(crate) fn from_bytes(_: &str, _: Vec<u8>, _: Option<Format>) -> Result<Opened, OpenError> {
    Err(no_decoder())
}

#[cfg(not(feature = "decode"))]
fn from_path(path: &std::path::Path, _: Option<Format>) -> Result<Opened, OpenError> {
    if path.is_file() {
        Err(no_decoder())
    } else {
        Err(OpenError::Resource)
    }
}

#[cfg(not(feature = "decode"))]
fn no_decoder() -> OpenError {
    OpenError::Format("This build of Ljós has no media decoder".to_owned())
}

#[cfg(feature = "decode")]
pub(crate) fn from_bytes(
    name: &str,
    bytes: Vec<u8>,
    samples: Option<Format>,
) -> Result<Opened, OpenError> {
    let reader: filmcraft_media::SharedReader =
        Arc::new(filmcraft_media::reader::MemReader(bytes.into()));
    from_reader(name, reader, samples)
}

#[cfg(feature = "decode")]
fn from_path(path: &std::path::Path, samples: Option<Format>) -> Result<Opened, OpenError> {
    let reader =
        filmcraft_media::reader::FileReader::open(path).map_err(|_| OpenError::Resource)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    from_reader(&name, Arc::new(reader), samples)
}

#[cfg(feature = "decode")]
fn from_reader(
    name: &str,
    reader: filmcraft_media::SharedReader,
    samples: Option<Format>,
) -> Result<Opened, OpenError> {
    use filmcraft_media::MediaKind;

    let source = filmcraft_media::reader::open_reader(
        name,
        Arc::clone(&reader),
        &filmcraft_codecs::reader_openers(),
        &filmcraft_codecs::openers(),
    )
    .map_err(|e| OpenError::Format(e.to_string()))?;
    let info = source.info().clone();
    if matches!(info.kind, MediaKind::Synthetic) {
        return Err(OpenError::Format(format!("{name}: not a media file")));
    }
    let duration_ms = (info.duration.seconds() * 1000.0).round().max(0.0);
    let video = info.video.as_ref().map(|v| VideoInfo {
        width: v.width,
        height: v.height,
        fps: if v.frame_rate.den != 0 && v.frame_rate.num > 0 {
            v.frame_rate.num as f64 / v.frame_rate.den as f64
        } else {
            25.0
        },
    });
    let title = crate::meta::mp4_title(&*reader);
    let (meta, audio_tracks, video_tracks) = crate::meta::describe(&info, duration_ms, title);
    let samples = match (samples, info.has_audio()) {
        (Some(format), true) => {
            let frames = (info.duration.seconds() * f64::from(format.rate)).ceil() as usize;
            let buffer = source
                .audio(0, frames, format.rate)
                .map_err(|e| OpenError::Format(e.to_string()))?;
            Some(crate::sink::interleave(&buffer.channels, format.channels).into())
        }
        _ => None,
    };
    Ok(Opened {
        duration_ms,
        has_audio: info.has_audio(),
        video,
        meta,
        audio_tracks,
        video_tracks,
        samples,
        source,
    })
}
