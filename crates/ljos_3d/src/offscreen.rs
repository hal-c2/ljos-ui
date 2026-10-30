//! A scene drawn on a device of its own and read back: for whatever needs
//! the pixels on the CPU (tests, the reference renderer, a snapshot), never
//! for a window, which draws on the device its painter has.

use std::time::Duration;

use crate::gpu::{COLOR_FORMAT, Renderer, Target};
use crate::scene::Scene;

#[derive(Debug, thiserror::Error)]
pub enum OffscreenError {
    #[error("no graphics adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("no graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the picture could not be read back: {0}")]
    Readback(String),
}

/// A device and a renderer for drawing off-screen.
pub struct Offscreen {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::AdapterInfo,
    pub renderer: Renderer,
}

impl std::fmt::Debug for Offscreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Offscreen")
            .field("adapter", &self.adapter.name)
            .field("backend", &self.adapter.backend)
            .finish_non_exhaustive()
    }
}

/// A picture read back: rows top to bottom, premultiplied sRGB RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().expect("four channels")
    }
}

/// How long a read back may wait for the device. A software rasteriser
/// takes its time; a device that never answers is lost, and saying so
/// beats waiting for ever.
const WAIT: Duration = Duration::from_secs(10);

impl Offscreen {
    /// The first adapter wgpu ranks, with no surface to draw into.
    pub fn new() -> Result<Self, OffscreenError> {
        pollster::block_on(async {
            // WGPU_BACKEND and the other variables wgpu reads choose, so a
            // run can be held to one driver.
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await?;
            // Asked for where the adapter has it, so multisampling can use
            // the sample counts this adapter has beyond WebGPU's 1 and 4;
            // and the timestamps a target's GPU time is read from.
            let required_features = (adapter.features()
                & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
                | crate::timing_features(&adapter);
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("ljos_3d offscreen"),
                    required_features,
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    ..Default::default()
                })
                .await?;
            let mut renderer = Renderer::new(&device);
            renderer.use_adapter_sample_counts(&adapter, &device);
            renderer.use_adapter_formats(&adapter);
            Ok(Offscreen {
                device,
                queue,
                adapter: adapter.get_info(),
                renderer,
            })
        })
    }

    /// Draws `scene` at `width` by `height` pixels and reads it back.
    pub fn render(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
    ) -> Result<Rgba, OffscreenError> {
        let mut target = Target::new(&self.device, &self.renderer, [width, height]);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer
            .render(&self.device, &self.queue, &mut encoder, scene, &mut target);
        self.queue.submit([encoder.finish()]);
        read_back(&self.device, &self.queue, target.texture())
    }
}

/// Copies `texture` (of [`COLOR_FORMAT`]) to the CPU.
pub fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Rgba, OffscreenError> {
    debug_assert_eq!(texture.format(), COLOR_FORMAT);
    let (width, height) = (texture.width(), texture.height());
    let row = width * 4;
    let padded =
        row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ljos_3d read back"),
        size: u64::from(padded * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(WAIT),
        })
        .map_err(|e| OffscreenError::Readback(e.to_string()))?;
    rx.recv()
        .map_err(|e| OffscreenError::Readback(e.to_string()))?
        .map_err(|e| OffscreenError::Readback(e.to_string()))?;
    let mapped = slice
        .get_mapped_range()
        .map_err(|e| OffscreenError::Readback(e.to_string()))?;
    let mut pixels = Vec::with_capacity((row * height) as usize);
    for r in 0..height {
        let start = (r * padded) as usize;
        pixels.extend_from_slice(&mapped[start..start + row as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}
