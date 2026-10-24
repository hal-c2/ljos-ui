//! `VideoSink`, what takes a player's frames: the one a `VideoOutput`
//! has shows them, and one of the program's own only tells it of them.
//!
//! Every frame, whoever made it (a player's decode thread, or a
//! [`crate::VideoFrameInput`] fed by the program), goes through a sink:
//! the sink keeps the newest, says its size, fires `videoFrameChanged`,
//! and hands it to its output, if it has one.

use std::fmt;

use ljos_engine::{Atom, Engine, ObjectId, QmlObject, QtObject, Size, Value, qml_impl};
use ljos_render::ImageHandle;

use crate::video::{self, VideoOutput};

/// The size a sink with no video reads, as in Qt.
pub(crate) const NO_SIZE: Size = Size {
    width: -1.0,
    height: -1.0,
};

/// `VideoSink` (QtMultimedia).
#[derive(QmlObject)]
#[qml(module = "QtMultimedia", name = "VideoSink", extends = QtObject)]
pub struct VideoSink {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub subtitle_text: Atom,
    /// The frame's size; (-1, -1) with no video.
    #[qml(notify, readonly)]
    pub video_size: Size,
    #[qml(skip)]
    frame: Option<ImageHandle>,
    /// The output whose sink this is.
    #[qml(skip)]
    output: Option<ObjectId>,
}

impl fmt::Debug for VideoSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VideoSink")
            .field("video_size", &self.video_size)
            .field("frame", &self.frame.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for VideoSink {
    fn default() -> Self {
        VideoSink {
            object: QtObject::default(),
            subtitle_text: Atom::EMPTY,
            video_size: NO_SIZE,
            frame: None,
            output: None,
        }
    }
}

#[qml_impl]
impl VideoSink {
    /// QML sees no value in the frame: a `QVideoFrame` is not a QML type.
    /// Qt's VideoSink declares the signal again without the parameter, so
    /// a handler is not given a `frame` by name.
    #[qml(signal, name = "videoFrameChanged")]
    fn video_frame_changed(_: Value);
}

/// Makes a sink for `output`, which shows what it is given.
pub(crate) fn make_for(e: &Engine, output: ObjectId) -> Option<ObjectId> {
    let sink = e
        .create_native_incomplete(
            VideoSink {
                output: Some(output),
                ..VideoSink::default()
            },
            Some(output),
        )
        .ok()?;
    e.complete(sink).ok()?;
    Some(sink)
}

/// The sink frames for `target` go through: `target` itself, or the
/// one of the `VideoOutput` it is.
pub(crate) fn of(e: &Engine, target: Option<ObjectId>) -> Option<ObjectId> {
    let t = target.filter(|t| e.is_alive(*t))?;
    if e.with::<VideoSink, _>(t, |_| ()).is_ok() {
        return Some(t);
    }
    e.untrack(|| e.get(t, VideoOutput::VIDEO_SINK))
        .ok()
        .flatten()
        .filter(|s| e.is_alive(*s))
}

/// Whether `target` is a sink of the program's, which is always seen.
pub(crate) fn is_sink(e: &Engine, target: ObjectId) -> bool {
    e.with::<VideoSink, _>(target, |_| ()).is_ok()
}

/// Gives `sink` a frame of `size` that was not shown yet: says its size
/// before the frame, as Qt does.
pub(crate) fn sized(e: &Engine, sink: ObjectId, size: Size) {
    if e.untrack(|| e.get(sink, VideoSink::VIDEO_SIZE)).ok() != Some(size) {
        let _ = e.set_internal(sink, VideoSink::VIDEO_SIZE, size);
    }
}

/// Gives `sink` `image`, which its output shows.
pub(crate) fn show(e: &Engine, sink: ObjectId, image: ImageHandle) {
    let size = Size {
        width: f64::from(image.width()),
        height: f64::from(image.height()),
    };
    let Ok(output) = e.with_mut::<VideoSink, _>(sink, |s| {
        s.frame = Some(image.clone());
        s.output
    }) else {
        return;
    };
    sized(e, sink, size);
    if let Some(o) = output {
        video::paint(e, o, image);
    }
    let _ = e.emit_by_name(sink, "videoFrameChanged", &[Value::Undefined]);
}

/// Takes the video out of `sink`; the sink hears of it when it had any.
pub(crate) fn clear(e: &Engine, sink: ObjectId) {
    let Ok((had, output)) = e.with_mut::<VideoSink, _>(sink, |s| (s.frame.take(), s.output)) else {
        return;
    };
    let had_size = e.untrack(|| e.get(sink, VideoSink::VIDEO_SIZE)).ok() != Some(NO_SIZE);
    if had_size {
        let _ = e.set_internal(sink, VideoSink::VIDEO_SIZE, NO_SIZE);
    }
    if let Some(o) = output {
        video::unpaint(e, o);
    }
    if had.is_some() {
        let _ = e.emit_by_name(sink, "videoFrameChanged", &[Value::Undefined]);
    }
}
