//! The `QtMultimedia` module for Ljós: `MediaPlayer`, `AudioOutput`,
//! `VideoOutput`, `Video`, `SoundEffect`, `MediaDevices` and the
//! `MediaMetaData` and `AudioDevice` types they hand out.
//!
//! A host registers it beside the standard modules:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_media::register(&engine);
//! ```
//!
//! ARCHITECTURE.md §7.6 is the design. In short:
//!
//! - **Opening** happens on a thread of its own, reading a local file,
//!   a resource the host registered or what the platform fetched, and
//!   the answer comes back as a platform reply. A source that cannot be
//!   opened is `MediaPlayer.ResourceError` and `InvalidMedia`, as in Qt.
//! - **Decoding** is FilmCraft's pure-Rust demuxers and codecs (the
//!   `decode` feature, on by default). Without it every source is a
//!   `FormatError` and every `SoundEffect` an `Error`, and nothing else
//!   changes: the types still load.
//! - **Sound** goes to the system's default output through cpal (the
//!   `audio` feature). With no device, without the feature, or when the
//!   host asks for it ([`use_null_audio`]), it goes to a null sink and the
//!   engine's clock is the media clock, so a test's virtual clock plays
//!   a clip in exactly its duration.
//! - **Pacing** is deadlines on the engine's timer service
//!   ([`ljos_engine::Engine::call_at`]): the next position report (every
//!   50 ms, as Qt's FFmpeg backend reports), the next video frame, the
//!   end of the stream. A paused or stopped player, a `SoundEffect` that
//!   is not playing and a `VideoOutput` that cannot be seen arm nothing.
//! - **Frames** are decoded and converted to RGBA on the player's decode
//!   thread, one request at a time (a newer request replaces one not yet
//!   started, so a frame the clock has passed is never decoded), and
//!   reach the screen through the render tree's image path
//!   ([`ljos_quick::register_paint_item`]).
//!
//! - **Sinks**: every frame goes through a `VideoSink`, a `VideoOutput`'s
//!   own or one of the program's. A program hands in frames of its own,
//!   from any thread, through a [`VideoFrameInput`].
//! - **Streams**: a player plays bytes a program writes as they come
//!   ([`MediaStream`]), paced or live, and MJPEG or H.264 from a web
//!   server as it arrives. A program that wants the pictures itself
//!   decodes with the same [`Decoder`] on a thread of its own, and
//!   converts the pictures it keeps with [`VideoFrame::into_rgba`].
//!
//! Camera, CaptureSession, MediaRecorder, ImageCapture, ScreenCapture,
//! WindowCapture, AudioInput and subtitles are not built.

// Without a decoder no frame, sample or metadata is ever made, so the
// paths that carry them are unreachable but kept, to keep one shape.
#![cfg_attr(not(feature = "decode"), allow(dead_code))]

mod devices;
mod frame_input;
mod meta;
mod open;
mod player;
mod sink;
mod sound;
mod stream;
mod stream_decoder;
mod units;
mod video;
mod video_sink;

#[cfg(feature = "decode")]
mod decoder;

use ljos_engine::Engine;

pub use devices::{AudioDevice, AudioOutput, MediaDevices};
pub use frame_input::{FrameError, VideoFrame, VideoFrameInput, YuvColour, YuvMatrix};
pub use meta::MediaMetaData;
pub use player::MediaPlayer;
pub use sound::SoundEffect;
pub use stream::{MediaStream, StreamError, StreamFormat, StreamOptions};
pub use stream_decoder::{DecodeError, Decoder};
pub use video::{Video, VideoOutput};
pub use video_sink::VideoSink;

/// The logging category of every warning this module gives.
const CATEGORY: &str = "qt.multimedia";

/// Registers `QtMultimedia` on `engine`: the one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtMultimedia" 6.0 => [
        MediaPlayer,
        AudioOutput,
        AudioDevice,
        MediaDevices,
        MediaMetaData,
        VideoOutput,
        Video,
        VideoSink,
        SoundEffect,
    ]));
    video::register_paint(engine);
}

/// Sends all sound to the null sink, whatever devices the system has: for
/// a host whose clock is not the wall clock (a test, a renderer), where
/// sound would play at the wrong pace or not at all. The engine's clock is
/// then the media clock. Call it before anything plays.
pub fn use_null_audio(engine: &Engine) {
    sink::Audio::of(engine).force_null();
}
