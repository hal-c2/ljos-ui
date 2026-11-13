//! The wgpu calls the renderer makes, counted into the frame's tally
//! ([`crate::stats::FrameCounters`]) as they are made.
//!
//! A pass is a [`Pass`], which counts what is set and drawn on it and is a
//! `wgpu::RenderPass` for everything else; the device and queue calls that
//! make or upload something go through the functions here. Counting is an
//! add into a thread's tally, so it is on in every build: a number that
//! appears only when asked for would measure a different program.

use std::ops::{Deref, DerefMut, Range};

use wgpu::util::DeviceExt as _;

use crate::stats::count;

/// A render pass whose draws, pipelines and bind groups are counted.
pub(crate) struct Pass<'e>(wgpu::RenderPass<'e>);

impl<'e> Pass<'e> {
    pub(crate) fn begin(
        encoder: &'e mut wgpu::CommandEncoder,
        desc: &wgpu::RenderPassDescriptor<'_>,
    ) -> Self {
        count(|c| c.render_passes += 1);
        Self(encoder.begin_render_pass(desc))
    }

    pub(crate) fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        count(|c| c.pipeline_switches += 1);
        self.0.set_pipeline(pipeline);
    }

    pub(crate) fn set_bind_group<'a, G>(&mut self, index: u32, group: G, offsets: &[u32])
    where
        Option<&'a wgpu::BindGroup>: From<G>,
    {
        count(|c| c.bind_group_sets += 1);
        self.0.set_bind_group(index, group, offsets);
    }

    pub(crate) fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
        drawn(vertices.len(), instances.len());
        self.0.draw(vertices, instances);
    }

    pub(crate) fn draw_indexed(&mut self, indices: Range<u32>, base: i32, instances: Range<u32>) {
        drawn(indices.len(), instances.len());
        self.0.draw_indexed(indices, base, instances);
    }
}

impl<'e> Deref for Pass<'e> {
    type Target = wgpu::RenderPass<'e>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Pass<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// A draw on a pass the host owns, as the blit into its picture is.
pub(crate) fn drawn(vertices: usize, instances: usize) {
    count(|c| {
        c.draw_calls += 1;
        c.vertices += (vertices * instances) as u64;
    });
}

/// A pipeline or bind group set on a pass the host owns.
pub(crate) fn set_on_host_pass(pipelines: u64, bind_groups: u64) {
    count(|c| {
        c.pipeline_switches += pipelines;
        c.bind_group_sets += bind_groups;
    });
}

pub(crate) fn create_render_pipeline(
    device: &wgpu::Device,
    desc: &wgpu::RenderPipelineDescriptor<'_>,
) -> wgpu::RenderPipeline {
    count(|c| c.pipelines_created += 1);
    device.create_render_pipeline(desc)
}

pub(crate) fn create_shader_module(
    device: &wgpu::Device,
    desc: wgpu::ShaderModuleDescriptor<'_>,
) -> wgpu::ShaderModule {
    count(|c| c.shader_modules_created += 1);
    device.create_shader_module(desc)
}

pub(crate) fn create_buffer(
    device: &wgpu::Device,
    desc: &wgpu::BufferDescriptor<'_>,
) -> wgpu::Buffer {
    count(|c| c.buffers_created += 1);
    device.create_buffer(desc)
}

/// A buffer made with its contents, which is a write of them too.
pub(crate) fn create_buffer_init(
    device: &wgpu::Device,
    desc: &wgpu::util::BufferInitDescriptor<'_>,
) -> wgpu::Buffer {
    count(|c| {
        c.buffers_created += 1;
        c.buffer_writes += 1;
        c.buffer_write_bytes += desc.contents.len() as u64;
    });
    device.create_buffer_init(desc)
}

pub(crate) fn create_texture(
    device: &wgpu::Device,
    desc: &wgpu::TextureDescriptor<'_>,
) -> wgpu::Texture {
    count(|c| c.textures_created += 1);
    device.create_texture(desc)
}

pub(crate) fn create_bind_group(
    device: &wgpu::Device,
    desc: &wgpu::BindGroupDescriptor<'_>,
) -> wgpu::BindGroup {
    count(|c| c.bind_groups_created += 1);
    device.create_bind_group(desc)
}

pub(crate) fn write_buffer(queue: &wgpu::Queue, buffer: &wgpu::Buffer, offset: u64, data: &[u8]) {
    count(|c| {
        c.buffer_writes += 1;
        c.buffer_write_bytes += data.len() as u64;
    });
    queue.write_buffer(buffer, offset, data);
}

pub(crate) fn write_texture(
    queue: &wgpu::Queue,
    texture: wgpu::TexelCopyTextureInfo<'_>,
    data: &[u8],
    layout: wgpu::TexelCopyBufferLayout,
    size: wgpu::Extent3d,
) {
    count(|c| {
        c.texture_uploads += 1;
        c.texture_upload_bytes += data.len() as u64;
    });
    queue.write_texture(texture, data, layout, size);
}
