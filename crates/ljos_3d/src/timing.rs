//! How long the GPU took to draw a target's picture: timestamps written at
//! the start and end of its shadow passes and of its scene pass (which
//! shades, tonemaps and resolves the samples), read back without waiting.
//!
//! A frame's queries are resolved and copied into a buffer of their own in
//! the frame's commands. The next time the target is drawn, that buffer
//! is asked to be mapped; wgpu maps it once the GPU is done with it, at a
//! later submit or poll, and the drawing after that reads it. Nothing
//! waits for the GPU and nothing polls for it: a target drawn once keeps
//! its last reading until it is drawn again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// The features GPU timing needs, where the adapter has them: ask for
/// these with the device and the renderer times its targets.
///
/// None where reading the queries back would wait for the GPU: wgpu's GL
/// backend without query buffer objects copies each result through the
/// processor, which stalls until the frame is done. None on WebGL2,
/// which has no timer queries a page may use.
pub fn timing_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let webgl = cfg!(target_arch = "wasm32") && adapter.get_info().backend == wgpu::Backend::Gl;
    let resolves = adapter
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::NONBLOCKING_QUERY_RESOLVE);
    if webgl || !resolves {
        return wgpu::Features::empty();
    }
    adapter.features() & wgpu::Features::TIMESTAMP_QUERY
}

/// The queries: the first shadow pass's start and the last's end, then
/// the scene pass's start and end.
const QUERIES: u32 = 4;
const SHADOW: u32 = 0;
const SCENE: u32 = 2;
const BYTES: u64 = QUERIES as u64 * wgpu::QUERY_SIZE as u64;

/// Frames whose readings may be on their way at once: one recorded, one
/// being mapped, one mapped and not yet read. A fourth frame drawn before
/// the first is read goes untimed rather than waiting.
const READS: usize = 3;

/// Where a frame's readings are, as the map callback and the renderer see
/// them.
const FREE: u8 = 0;
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const MAPPED: u8 = 3;

/// A target's timestamp queries and the buffers they are read through.
pub(crate) struct Timer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    reads: [Read; READS],
    /// The read the frame being recorded copies into; `None` between
    /// frames, and for a frame that found every read in use.
    this: Option<usize>,
    /// Nanoseconds a timestamp tick is on this queue.
    period: f32,
    last: Option<Duration>,
    /// The last reading has not been taken yet.
    fresh: bool,
}

struct Read {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    /// Whether the frame drew shadows: its shadow queries were written.
    shadows: bool,
}

impl Timer {
    /// Queries for `device`; `None` without [`timing_features`].
    pub(crate) fn new(device: &wgpu::Device) -> Option<Timer> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let read = |_| Read {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ljos_3d timing read"),
                size: BYTES,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            state: Arc::new(AtomicU8::new(FREE)),
            shadows: false,
        };
        Some(Timer {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("ljos_3d timing"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ljos_3d timing resolve"),
                size: BYTES,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            reads: std::array::from_fn(read),
            this: None,
            period: 1.0,
            last: None,
            fresh: false,
        })
    }

    /// A frame is about to be recorded, the last one's commands submitted:
    /// reads what has been mapped, asks for the last frame's readings to
    /// be mapped, and picks where this frame's go.
    pub(crate) fn begin(&mut self, queue: &wgpu::Queue) {
        self.period = queue.get_timestamp_period();
        for read in &mut self.reads {
            match read.state.load(Ordering::Acquire) {
                MAPPED => {
                    // A range that cannot be viewed is a reading lost, not
                    // a frame lost.
                    let took = read
                        .buffer
                        .slice(..)
                        .get_mapped_range()
                        .ok()
                        .and_then(|bytes| {
                            let mut ticks = [0u64; QUERIES as usize];
                            for (tick, b) in ticks.iter_mut().zip(bytes.as_chunks::<8>().0) {
                                *tick = u64::from_le_bytes(*b);
                            }
                            reading(&ticks, read.shadows, self.period)
                        });
                    read.buffer.unmap();
                    read.state.store(FREE, Ordering::Release);
                    if took.is_some() {
                        self.last = took;
                        self.fresh = true;
                    }
                }
                COPIED => {
                    read.state.store(MAPPING, Ordering::Release);
                    let state = Arc::clone(&read.state);
                    read.buffer
                        .slice(..)
                        .map_async(wgpu::MapMode::Read, move |mapped| {
                            let to = if mapped.is_ok() { MAPPED } else { FREE };
                            state.store(to, Ordering::Release);
                        });
                }
                _ => {}
            }
        }
        self.this = self
            .reads
            .iter()
            .position(|r| r.state.load(Ordering::Acquire) == FREE);
    }

    /// Where shadow pass `i` of `n` writes its timestamps: the first its
    /// start, the last its end.
    pub(crate) fn shadow_pass(
        &self,
        i: usize,
        n: usize,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.pass(i == 0, i + 1 == n, SHADOW)
    }

    /// Where the scene pass writes its timestamps.
    pub(crate) fn scene_pass(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.pass(true, true, SCENE)
    }

    fn pass(&self, begin: bool, end: bool, at: u32) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        // A pass between the first and the last writes nothing, and wgpu
        // refuses writes that name no index.
        if self.this.is_none() || !(begin || end) {
            return None;
        }
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: begin.then_some(at),
            end_of_pass_write_index: end.then_some(at + 1),
        })
    }

    /// The frame's passes are recorded: its timestamps go to its read.
    /// Only the queries the frame wrote are resolved; a query never
    /// written may hold anything, or make the resolve wait for it.
    pub(crate) fn end(&mut self, encoder: &mut wgpu::CommandEncoder, shadows: bool) {
        let Some(i) = self.this.take() else {
            return;
        };
        let first = if shadows { SHADOW } else { SCENE };
        let bytes = u64::from(QUERIES - first) * u64::from(wgpu::QUERY_SIZE);
        encoder.resolve_query_set(&self.queries, first..QUERIES, &self.resolve, 0);
        let read = &mut self.reads[i];
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &read.buffer, 0, bytes);
        read.shadows = shadows;
        read.state.store(COPIED, Ordering::Release);
    }

    /// The GPU time of the last frame read.
    pub(crate) fn last(&self) -> Option<Duration> {
        self.last
    }

    /// The GPU time read since the last time this was asked, if any.
    pub(crate) fn take(&mut self) -> Option<Duration> {
        std::mem::take(&mut self.fresh)
            .then_some(self.last)
            .flatten()
    }
}

/// The GPU time `ticks` (the shadow passes' start and end when `shadows`,
/// then the scene pass's) come to, `period` nanoseconds a tick; `None`
/// for readings that make no sense, as a query a driver did not write.
fn reading(ticks: &[u64], shadows: bool, period: f32) -> Option<Duration> {
    let ticks = if shadows { &ticks[..4] } else { &ticks[..2] };
    let mut sum = 0u64;
    for &[start, end] in ticks.as_chunks::<2>().0 {
        if start == 0 || end < start {
            return None;
        }
        sum += end - start;
    }
    Some(Duration::from_nanos(
        (sum as f64 * f64::from(period)) as u64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_adds_the_passes_and_refuses_what_makes_no_sense() {
        assert_eq!(
            reading(&[100, 300, 400, 1000, 0, 0], true, 1.0),
            Some(Duration::from_nanos(800))
        );
        // Without shadows the scene's pair comes first.
        assert_eq!(
            reading(&[400, 1000, 7, 7], false, 2.0),
            Some(Duration::from_nanos(1200))
        );
        assert_eq!(
            reading(&[500, 400], false, 1.0),
            None,
            "ends before it starts"
        );
        assert_eq!(reading(&[0, 400], false, 1.0), None, "never written");
    }
}
