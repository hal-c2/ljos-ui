//! The video of an MP4 or QuickTime file, read in order for playback.
//!
//! FilmCraft's sources keep what they decode for an editor's timeline: a
//! cache of up to 384 MiB a source, and at least 64 frames whatever their
//! size, given back only once the process holds more than 1 GiB. A player
//! asks for each frame once, in order, so that cache only grew: a 1080p
//! clip held 382 MiB of frames it had shown, and the pool their planes
//! went back to another 40 MiB. Here a player's decode thread runs the
//! decoder itself and keeps the pictures it put out ahead of the one
//! asked for and the last one handed out; the rest are dropped as the
//! clock passes them. Ahead are the codec's reordering and, as a
//! frame-threaded decoder hands over every picture that has finished, up
//! to its threads and two more bumped while the one asked for was in
//! flight; how many of those come follows the threads' timing.
//!
//! Which picture a time shows follows FilmCraft's MP4 source, so a frame
//! is the one the source would have given: the sample presented at the
//! time rounded to the track's units, after the source's two corrections
//! of sample times ([`snap_to_frame_grid`], [`rebase_unedited_video`]).
//! A file this does not read (no video, a clean aperture, a codec without
//! a decoder) plays through the source as before.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use filmcraft_codecs::video::VideoDecoder;
use filmcraft_color::ColorInfo;
use filmcraft_frame::VideoFrame;
use filmcraft_isobmff::{ByteSource, CodecConfig, Mp4File, TrackKind};
use filmcraft_media::SharedReader;
use filmcraft_time::{FrameRate, Tick};

/// Bytes and pictures every player's reader holds, for the memory tool.
static HELD_BYTES: AtomicUsize = AtomicUsize::new(0);
static HELD_FRAMES: AtomicUsize = AtomicUsize::new(0);

/// A running decoder this many samples short of the wanted sample's sync
/// sample decodes on to it rather than starting again there: playback
/// wants those pictures anyway (FilmCraft's `CONTINUE_THROUGH`).
const CONTINUE_THROUGH: usize = 48;

/// The share of a frame's time decoding may take on average before the
/// reader gives its decoder every thread FilmCraft would.
const BUSY: f64 = 0.5;

/// What the readers of every player hold: (bytes, pictures).
pub(crate) fn held() -> (usize, usize) {
    (
        HELD_BYTES.load(Ordering::Relaxed),
        HELD_FRAMES.load(Ordering::Relaxed),
    )
}

struct Src(SharedReader);

impl ByteSource for Src {
    fn len(&self) -> u64 {
        self.0.len()
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.0.read_at(offset, buf)
    }
}

pub(crate) struct Mp4Video {
    bytes: Src,
    file: Mp4File,
    track: usize,
    rotation: u8,
    /// The container's colour, which wins over the stream's.
    color: Option<ColorInfo>,
    /// Made with FilmCraft's few threads for background work until it is
    /// too slow for the file's rate (`full`): a frame-threaded decoder
    /// holds two pictures more than its threads in flight, and each
    /// thread an allocator arena; 16 of them held 250 MB more for a
    /// 1080p clip than 2, which decoded it at 4 ms a frame.
    decoder: Box<dyn VideoDecoder>,
    full: bool,
    /// How long a frame lasts, and about how long a sample took to decode
    /// of late, in seconds.
    period: f64,
    busy: f64,
    /// The next sample (decode order) to feed, `None` when the decoder
    /// has to start again at a sync sample; the sample the run started at.
    next: Option<usize>,
    start: usize,
    /// The latest time the run has put out.
    out_max: i64,
    /// Pictures put out and not yet asked for, by time.
    ahead: BTreeMap<i64, VideoFrame>,
    /// The last picture handed out, by time: asked again while paused or
    /// at a rate below the file's.
    last: Option<(i64, VideoFrame)>,
    /// What this reader last added to [`HELD_BYTES`] and [`HELD_FRAMES`].
    counted: (usize, usize),
}

impl Mp4Video {
    /// A reader for the first video track of the file `reader` holds;
    /// `None` when the file is not one this reads. `color` is the colour
    /// FilmCraft's source resolved for the track.
    pub(crate) fn open(reader: &SharedReader, color: ColorInfo) -> Option<Mp4Video> {
        let mut head = [0u8; 12];
        reader.read_at(0, &mut head).ok()?;
        if !matches!(
            &head[4..8],
            b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip"
        ) {
            return None;
        }
        let bytes = Src(reader.clone());
        let mut file = filmcraft_isobmff::open(&bytes).ok()?;
        let playable = |t: &filmcraft_isobmff::Track, kind| {
            t.kind == kind && !t.samples.is_empty() && !t.entries.is_empty() && t.timescale > 0
        };
        let track = file
            .tracks
            .iter()
            .position(|t| playable(t, TrackKind::Video))?;
        let t = &file.tracks[track];
        let entry = &t.entries[0];
        // A cut picture is the source's to make.
        if entry
            .video
            .as_ref()
            .is_some_and(|v| v.clean_aperture.is_some())
        {
            return None;
        }
        let rotation = t.display_rotation().unwrap_or(0);
        let explicit = entry.video.as_ref().is_some_and(|v| {
            matches!(
                v.color,
                Some(
                    filmcraft_isobmff::ColorInfo::Nclx { .. }
                        | filmcraft_isobmff::ColorInfo::Nclc { .. }
                )
            )
        });
        let decoder = make_decoder(entry, false)?;
        let period = {
            let mut durs: Vec<u32> = t.samples.iter().take(240).map(|s| s.duration).collect();
            durs.sort_unstable();
            f64::from(durs[durs.len() / 2]) / f64::from(t.timescale)
        };
        let audio_offsets: Vec<(i64, u32)> = file
            .tracks
            .iter()
            .filter(|t| playable(t, TrackKind::Audio))
            .take(filmcraft_media::MAX_AUDIO_STREAMS)
            .map(|t| (audio_offset(t), t.timescale.max(1)))
            .collect();
        snap_to_frame_grid(&mut file.tracks[track]);
        rebase_unedited_video(&mut file.tracks[track], &audio_offsets);
        Some(Mp4Video {
            bytes,
            file,
            track,
            rotation,
            color: explicit.then_some(color),
            decoder,
            full: false,
            period,
            busy: 0.0,
            next: None,
            start: 0,
            out_max: i64::MIN,
            ahead: BTreeMap::new(),
            last: None,
            counted: (0, 0),
        })
    }

    /// The picture shown at `at`, decoded on from where the last one was
    /// when it is near, else from the sync sample before it.
    pub(crate) fn frame(&mut self, at: Tick) -> Option<VideoFrame> {
        let t = &self.file.tracks[self.track];
        let n = t.samples.len();
        let target = at
            .max(Tick::ZERO)
            .to_rational_round(1, i64::from(t.timescale));
        let i = t.sample_at_presentation_time(target).unwrap_or(n - 1);
        let want = t.samples[i].pts;
        if let Some((pts, f)) = &self.last
            && *pts == want
        {
            return Some(f.clone());
        }
        if !self.ahead.contains_key(&want) {
            let began = std::time::Instant::now();
            let fed = self.decode_to(i, want)?;
            // A sample's share: a seek feeds a GOP at once, and only the
            // rate the decoder keeps matters.
            if fed > 0 {
                let each = began.elapsed().as_secs_f64() / f64::from(fed);
                let old = 0.9f64.powi(fed.min(64) as i32);
                self.busy = old * self.busy + (1.0 - old) * each;
            }
        }
        // The nearest at or before the time, as the source gives it: a
        // decoder may stamp a picture a unit off.
        let mut later = self.ahead.split_off(&(want + 1));
        std::mem::swap(&mut later, &mut self.ahead);
        if let Some((pts, frame)) = later.pop_last() {
            self.last = Some((pts, self.finish(frame)));
        }
        self.count();
        self.last.as_ref().map(|(_, f)| f.clone())
    }

    /// Decodes on until the picture shown at `want` (sample `i`) is out;
    /// how many samples it fed.
    fn decode_to(&mut self, i: usize, want: i64) -> Option<u32> {
        let t = &self.file.tracks[self.track];
        let n = t.samples.len();
        let mut key = decode_start(t, i);
        let continuing = self.next.is_some_and(|next| {
            let near = next <= key && key - next <= CONTINUE_THROUGH;
            self.start <= key
                && (next > key || near)
                && (i >= next || (self.start <= i && want > self.out_max))
        });
        if !continuing {
            // An MP4 without `stss` flags every sample as sync: step back
            // to one the codec can start from.
            while key > 0 {
                let data = self.file.read_sample(&self.bytes, self.track, key).ok()?;
                if self.decoder.is_random_access(&data) != Some(false) {
                    break;
                }
                key = t.sync_sample_before(key - 1);
            }
            if self.wants_full() {
                self.decoder = make_decoder(&t.entries[0], true)?;
                self.full = true;
            }
            self.decoder.reset();
            self.ahead.clear();
            self.next = Some(key);
            self.start = key;
            self.out_max = i64::MIN;
        }
        let mut next = self.next.unwrap_or(key);
        let mut fed = 0;
        let limit = (i.max(next) + 64).min(n);
        while next < limit {
            let pts = self.file.tracks[self.track].samples[next].pts;
            let data = self.file.read_sample(&self.bytes, self.track, next).ok()?;
            if next > self.start && self.wants_full() && self.starts_here(next, &data) {
                // A run that starts here needs nothing of the old one but
                // the pictures it still holds.
                let out = self.decoder.flush();
                self.keep(out, want);
                self.decoder = make_decoder(&self.file.tracks[self.track].entries[0], true)?;
                self.full = true;
                self.start = next;
            }
            next += 1;
            // A picture nothing refers to and that would be dropped
            // unseen is not decoded.
            if next - 1 != i && pts < want && self.decoder.is_disposable(&data) {
                continue;
            }
            fed += 1;
            let Ok(out) = self.decoder.decode(&data, pts) else {
                self.next = None;
                return None;
            };
            self.keep(out, want);
            if self.ahead.contains_key(&want) {
                self.next = Some(next);
                return Some(fed);
            }
        }
        let out = self.decoder.flush();
        self.keep(out, want);
        self.next = None;
        Some(fed)
    }

    /// Keeps what the decoder put out: the latest picture before `want`
    /// (in case `want` never comes) and those from it on.
    fn keep(&mut self, out: Vec<filmcraft_codecs::DecodedFrame>, want: i64) {
        for d in out {
            self.out_max = self.out_max.max(d.pts);
            self.ahead.insert(d.pts, d.frame);
        }
        if let Some((&before, _)) = self.ahead.range(..want).next_back() {
            self.ahead = self.ahead.split_off(&before);
        }
        self.count();
    }

    fn wants_full(&self) -> bool {
        !self.full && self.busy > BUSY * self.period
    }

    /// Whether a new decoder can start at sample `k`, `data`, and decode
    /// every picture from it on: an instantaneous refresh, which nothing
    /// after it in either order refers past. An open GOP's random-access
    /// picture is not: the leading pictures after it refer to the GOP
    /// before.
    fn starts_here(&self, k: usize, data: &[u8]) -> bool {
        let t = &self.file.tracks[self.track];
        if !t.samples[k].is_sync {
            return false;
        }
        let nal_types = |size: u8, first_byte_type: fn(u8) -> u8| -> Vec<u8> {
            let size = usize::from(size);
            let mut at = 0;
            let mut types = Vec::new();
            while (1..=4).contains(&size) && at + size < data.len() {
                let len = data[at..at + size]
                    .iter()
                    .fold(0usize, |a, &b| (a << 8) | usize::from(b));
                at += size;
                if len == 0 || at + len > data.len() {
                    break;
                }
                types.push(first_byte_type(data[at]));
                at += len;
            }
            types
        };
        match t.codec() {
            Some(CodecConfig::Avc(c)) => nal_types(c.length_size, |h| h & 0x1f).contains(&5),
            // IDR_W_RADL, IDR_N_LP
            Some(CodecConfig::Hevc(c)) => nal_types(c.length_size, |h| (h >> 1) & 0x3f)
                .iter()
                .any(|t| matches!(t, 19 | 20)),
            _ => self.decoder.intra_only() || self.decoder.is_random_access(data) == Some(true),
        }
    }

    fn finish(&self, mut f: VideoFrame) -> VideoFrame {
        if let Some(c) = self.color
            && !matches!(
                f.data,
                filmcraft_frame::PixelData::Rgba8(_) | filmcraft_frame::PixelData::RgbaF32(_)
            )
        {
            f.color = c;
        }
        if self.rotation != 0 {
            f.rotated(self.rotation)
        } else {
            f
        }
    }

    fn bytes_held(&self) -> (usize, usize) {
        let ahead: usize = self.ahead.values().map(VideoFrame::byte_size).sum();
        let last = self.last.as_ref().map_or(0, |(_, f)| f.byte_size());
        (
            ahead + last,
            self.ahead.len() + usize::from(self.last.is_some()),
        )
    }

    /// Brings the counts every reader adds to up to date: what this one
    /// held is taken out, what it holds now put in.
    fn count(&mut self) {
        let (bytes, frames) = self.bytes_held();
        let (was_bytes, was_frames) = std::mem::replace(&mut self.counted, (bytes, frames));
        HELD_BYTES.fetch_add(bytes, Ordering::Relaxed);
        HELD_BYTES.fetch_sub(was_bytes, Ordering::Relaxed);
        HELD_FRAMES.fetch_add(frames, Ordering::Relaxed);
        HELD_FRAMES.fetch_sub(was_frames, Ordering::Relaxed);
    }
}

impl Drop for Mp4Video {
    fn drop(&mut self) {
        self.ahead.clear();
        self.last = None;
        self.count();
    }
}

/// A decoder for `entry`: with every thread FilmCraft gives playback when
/// `full`, else with the few it gives background work.
fn make_decoder(
    entry: &filmcraft_isobmff::SampleEntry,
    full: bool,
) -> Option<Box<dyn VideoDecoder>> {
    filmcraft_media::cancel::with_background(!full, || filmcraft_codecs::make_video_decoder(entry))
        .ok()
}

/// The sync sample to start at for sample `i`: its own, or an earlier one
/// when `i` is a leading picture of an open GOP, shown before its
/// random-access picture and predicted from the GOP before.
fn decode_start(t: &filmcraft_isobmff::Track, i: usize) -> usize {
    let mut key = t.sync_sample_before(i);
    while key > 0 && t.samples[i].pts < t.samples[key].pts {
        key = t.sync_sample_before(key - 1).min(key - 1);
    }
    key
}

/// Where an audio track starts on the presentation, as FilmCraft's MP4
/// source reads it: Opus without an edit list starts its pre-skip early.
fn audio_offset(t: &filmcraft_isobmff::Track) -> i64 {
    match t.codec() {
        Some(CodecConfig::Opus(o)) if t.edit_offset == 0 => {
            -i64::from(o.pre_skip) * i64::from(t.timescale.max(1)) / 48_000
        }
        _ => t.edit_offset,
    }
}

/// Moves the times of a constant-rate track onto its frame grid, as
/// FilmCraft's MP4 source does: rounded to a coarse timescale they fall a
/// unit off it, and a lookup at a frame's time found the frame before.
/// A track whose times are not a grid is left as it is.
fn snap_to_frame_grid(t: &mut filmcraft_isobmff::Track) {
    let Some(first) = t.samples.iter().map(|s| s.pts).min() else {
        return;
    };
    let mut durs: Vec<u32> = t.samples.iter().map(|s| s.duration).collect();
    durs.sort_unstable();
    let Some(&median) = durs.get(durs.len() / 2) else {
        return;
    };
    let near: Vec<f64> = durs
        .iter()
        .filter(|&&d| d.abs_diff(median) <= 1)
        .map(|&d| f64::from(d))
        .collect();
    let mean = near.iter().sum::<f64>() / near.len().max(1) as f64;
    if mean <= 0.0 {
        return;
    }
    let rate = FrameRate::from_f64(f64::from(t.timescale) / mean);
    let (step_num, step_den) = (
        i128::from(t.timescale) * i128::from(rate.den),
        i128::from(rate.num),
    );
    if step_num <= 0 || step_den <= 0 {
        return;
    }
    let snap = |pts: i64| -> Option<(i128, i64)> {
        let off = i128::from(pts) - i128::from(first);
        let n = (2 * off * step_den + step_num).div_euclid(2 * step_num);
        let at = i128::from(first) + (2 * n * step_num + step_den).div_euclid(2 * step_den);
        let off_grid = (i128::from(pts) - at).abs();
        if off_grid * 4 * step_den >= step_num {
            return None;
        }
        Some((
            n,
            if off_grid <= 1 {
                pts
            } else {
                i64::try_from(at).ok()?
            },
        ))
    };
    let Some(snapped) = t
        .samples
        .iter()
        .map(|s| snap(s.pts))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let mut points: Vec<i128> = snapped.iter().map(|&(n, _)| n).collect();
    points.sort_unstable();
    points.dedup();
    if points.len() != snapped.len() {
        return;
    }
    for (s, (_, p)) in t.samples.iter_mut().zip(snapped) {
        s.pts = p;
    }
}

/// Starts a video track without an edit list at its first picture, as
/// FilmCraft's MP4 source does, moving it back by the earliest start of
/// it and the audio tracks so they stay in step.
fn rebase_unedited_video(t: &mut filmcraft_isobmff::Track, audio: &[(i64, u32)]) {
    if !t.edits.is_empty() || t.timescale == 0 {
        return;
    }
    let Some(first) = t
        .samples
        .iter()
        .map(|s| s.pts)
        .min()
        .map(|p| p.saturating_add(t.edit_offset))
        .filter(|&p| p > 0)
    else {
        return;
    };
    let vts = i64::from(t.timescale);
    let video_start = Tick::from_rational(first, 1, vts);
    let shift = audio.iter().fold(video_start, |m, &(offset, ts)| {
        m.min(Tick::from_rational(offset.max(0), 1, i64::from(ts)))
    });
    if shift <= Tick::ZERO {
        return;
    }
    let v = if shift == video_start {
        first
    } else {
        shift.to_rational_round(1, vts)
    };
    t.edit_offset = t.edit_offset.saturating_sub(v);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use filmcraft_media::FrameRequest;
    use filmcraft_time::Tick;

    use crate::open::{Opened, from_path};

    fn media(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../features/multimedia/media")
            .join(name)
    }

    fn open(name: &str) -> Opened {
        from_path(&media(name), None).expect("the clip opens")
    }

    /// The times a player asks at: forward at twice the frame rate, then,
    /// with `seeks`, a jump back, one far ahead, and the same time twice.
    fn times(o: &Opened, seeks: bool) -> Vec<f64> {
        let fps = o.video.expect("a video").fps;
        let mut at: Vec<f64> = (0..)
            .map(|k| f64::from(k) * 500.0 / fps)
            .take_while(|&ms| ms <= o.duration_ms + 100.0)
            .collect();
        if seeks {
            at.extend([
                o.duration_ms * 0.25,
                o.duration_ms * 0.75,
                0.0,
                0.0,
                o.duration_ms * 0.5,
            ]);
        }
        at
    }

    #[test]
    fn a_reader_gives_the_frames_the_source_gives() {
        for name in ["clip.mp4", "bframes.mp4", "rotated.mp4"] {
            same_frames(name, true, |_| {});
        }
    }

    #[test]
    fn a_reader_too_slow_for_its_clip_takes_a_full_decoder_at_a_refresh() {
        // Too slow from the third frame on: the switch comes at the next
        // refresh of the run, not at a seek.
        let reader = same_frames("bframes.mp4", false, |r| {
            if r.counted.1 > 0 {
                r.busy = f64::INFINITY;
            }
        });
        assert!(reader.full);
    }

    /// Plays `name` through a reader and the source alike, `each` given
    /// the reader before every frame.
    fn same_frames(
        name: &str,
        seeks: bool,
        each: impl Fn(&mut super::Mp4Video),
    ) -> super::Mp4Video {
        {
            let mut o = open(name);
            let mut reader = o.video_reader.take().expect("an MP4 is read in order");
            for ms in times(&o, seeks) {
                each(&mut reader);
                let at = Tick::from_seconds_f64(ms / 1000.0);
                let ours = reader.frame(at).expect("a frame");
                let theirs = o
                    .source
                    .video_frame(FrameRequest::full(at))
                    .expect("a frame");
                assert_eq!(
                    (ours.width, ours.height, ours.to_rgba8()),
                    (theirs.width, theirs.height, theirs.to_rgba8()),
                    "{name} at {ms} ms"
                );
            }
            reader
        }
    }

    /// The pictures an H.264 decoder may keep waiting for output before it
    /// bumps one: the stream's `max_num_reorder_frames`, at most its DPB.
    fn reordering(reader: &super::Mp4Video) -> usize {
        let t = &reader.file.tracks[reader.track];
        let Some(super::CodecConfig::Avc(c)) = t.codec() else {
            panic!("bframes.mp4 is H.264");
        };
        let nal = c.sps.first().expect("an SPS");
        let sps =
            filmcraft_h264::params::Sps::parse(&filmcraft_bitstream::unescape_rbsp(&nal[1..]))
                .expect("the SPS reads");
        sps.max_num_reorder_frames().min(sps.max_dpb_frames())
    }

    /// The worker threads of the reader's decoder: playback's (one a core,
    /// at most 16) once it is `full`, else the two FilmCraft gives a
    /// decoder for background work (`BACKGROUND_H264_THREADS`, which it
    /// keeps to itself), no more than playback's.
    fn decoder_threads(reader: &super::Mp4Video) -> usize {
        const BACKGROUND: usize = 2;
        let playback = filmcraft_codecs::video::h264_threads();
        let threads = if reader.full {
            playback
        } else {
            BACKGROUND.min(playback)
        };
        assert_eq!(
            reader.decoder.thread_limited(),
            threads < playback,
            "FilmCraft's decoder for background work no longer has {BACKGROUND} threads"
        );
        threads
    }

    #[test]
    fn a_reader_playing_holds_a_few_pictures() {
        let mut o = open("bframes.mp4");
        let mut reader = o.video_reader.take().expect("an MP4 is read in order");
        let fps = o.video.expect("a video").fps;
        let reorder = reordering(&reader);
        for k in 0..(o.duration_ms * fps / 1000.0) as u32 {
            reader
                .frame(Tick::from_seconds_f64(f64::from(k) / fps))
                .expect("a frame");
            // A frame-threaded decoder hands a picture over once it is
            // decoded, and those after it in output order that are too, so
            // how many come at once follows its threads' timing. What it
            // promises: the one shown, the `reorder` waiting before it was
            // bumped, and the pictures bumped while it was still in flight,
            // which is at most `threads + 2` feeds.
            let threads = decoder_threads(&reader);
            let bound = 1 + reorder + threads + 2;
            let held = reader.bytes_held().1;
            assert!(
                held <= bound,
                "at frame {k} held {held} pictures (ahead {:?}, last {:?}), more than \
                 1 shown + {reorder} reordered + {threads} threads + 2 in flight",
                reader.ahead.keys().collect::<Vec<_>>(),
                reader.last.as_ref().map(|(pts, _)| pts),
            );
        }
        drop(reader);
    }

    #[test]
    fn a_file_without_a_video_track_has_no_reader() {
        let o = from_path(&media("beep.wav"), None).expect("the sound opens");
        assert!(o.video_reader.is_none());
    }
}
