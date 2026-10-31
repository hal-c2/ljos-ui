//! What the renderer draws beyond one copy of a plain mesh: the copies of
//! an instanced model, a second UV set and the joints of a skin.
//!
//! Every draw reads a range of the target's instance rows; a model drawn
//! once reads row 0, the identity, so one pipeline draws both. A mesh with
//! a second UV set or joints has a second vertex buffer and is drawn by
//! the shaders' `vs_extra`. A skinned model's joints are a slot of the
//! target's joint buffer, bound beside the model's uniforms.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::instancing::MAX_JOINTS;
use crate::math::Mat4;
use crate::mesh::Mesh;

/// One copy: the top three rows of the matrix put in front of the model's
/// world transform, its linear colour and its custom data. Only a
/// CustomMaterial reads the data; it rides in the same row, 16 bytes a
/// copy, so that one buffer and one binding serve every pipeline.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq, Debug)]
pub(crate) struct InstanceRow {
    rows: [[f32; 4]; 3],
    color: [f32; 4],
    data: [f32; 4],
}

impl InstanceRow {
    pub(crate) const IDENTITY: InstanceRow = InstanceRow {
        rows: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
        color: [1.0; 4],
        data: [0.0; 4],
    };

    /// The copy's place as a matrix.
    pub(crate) fn matrix(&self) -> Mat4 {
        let r = &self.rows;
        Mat4([
            [r[0][0], r[1][0], r[2][0], 0.0],
            [r[0][1], r[1][1], r[2][1], 0.0],
            [r[0][2], r[1][2], r[2][2], 0.0],
            [r[0][3], r[1][3], r[2][3], 1.0],
        ])
    }

    pub(crate) fn new(m: &Mat4, color: [f32; 4], data: [f32; 4]) -> Self {
        let c = &m.0;
        let row = |r: usize| [c[0][r], c[1][r], c[2][r], c[3][r]];
        InstanceRow {
            rows: [row(0), row(1), row(2)],
            color,
            data,
        }
    }
}

/// The custom data is at location 11, after the second vertex buffer's,
/// so that the built-in shaders, which do not read it, keep their own.
pub(crate) const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    11 => Float32x4,
];

pub(crate) fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<InstanceRow>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRIBUTES,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ExtraVertex {
    uv1: [f32; 2],
    joints: [u16; 4],
    weights: [f32; 4],
}

pub(crate) const EXTRA_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    8 => Float32x2,
    9 => Uint16x4,
    10 => Float32x4,
];

pub(crate) fn extra_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ExtraVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &EXTRA_ATTRIBUTES,
    }
}

/// The second vertex buffer of a mesh that has a second UV set or joints;
/// `None` for one that has neither, which needs none.
pub(crate) fn upload_extra(device: &wgpu::Device, mesh: &Mesh) -> Option<wgpu::Buffer> {
    if mesh.uv1.is_empty() && mesh.joints.is_empty() {
        return None;
    }
    let vertices: Vec<ExtraVertex> = (0..mesh.positions.len())
        .map(|i| ExtraVertex {
            uv1: mesh
                .uv1
                .get(i)
                .or(mesh.uvs.get(i))
                .copied()
                .unwrap_or_default(),
            joints: mesh.joints.get(i).copied().unwrap_or_default(),
            weights: mesh.weights.get(i).copied().unwrap_or_default(),
        })
        .collect();
    Some(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ljos_3d second vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

/// Bytes one skin's joints take: a slot of the joint buffer.
pub(crate) const JOINT_SLOT: u64 = (MAX_JOINTS * std::mem::size_of::<Mat4>()) as u64;

/// A model's uniforms and its joints, each at a dynamic offset.
pub(crate) fn model_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: None,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ljos_3d model"),
        entries: &[entry(0), entry(1)],
    })
}

/// A buffer that holds rows of `T`, grown as more are needed, written only
/// where they changed since the last frame.
pub(crate) struct Rows<T> {
    pub(crate) buffer: wgpu::Buffer,
    capacity: u64,
    written: Vec<T>,
    label: &'static str,
    usage: wgpu::BufferUsages,
}

impl<T: Pod + PartialEq> Rows<T> {
    pub(crate) fn new(
        device: &wgpu::Device,
        label: &'static str,
        usage: wgpu::BufferUsages,
        capacity: u64,
    ) -> Self {
        Rows {
            buffer: Self::buffer(device, label, usage, capacity),
            capacity,
            written: Vec::new(),
            label,
            usage,
        }
    }

    fn buffer(
        device: &wgpu::Device,
        label: &str,
        usage: wgpu::BufferUsages,
        capacity: u64,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity * std::mem::size_of::<T>() as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Writes `rows`. Whether the buffer was made again, which a bind
    /// group holding it must be too.
    pub(crate) fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, rows: &[T]) -> bool {
        let grown = rows.len() as u64 > self.capacity;
        if grown {
            self.capacity = (rows.len() as u64).next_power_of_two();
            self.buffer = Self::buffer(device, self.label, self.usage, self.capacity);
            self.written.clear();
        }
        // From the first row that differs: copies move together, so a
        // changed table rarely leaves a long unchanged tail worth skipping.
        let first = rows
            .iter()
            .zip(&self.written)
            .position(|(a, b)| a != b)
            .unwrap_or(self.written.len().min(rows.len()));
        if first < rows.len() {
            let offset = first as u64 * std::mem::size_of::<T>() as u64;
            queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(&rows[first..]));
        }
        self.written.clear();
        self.written.extend_from_slice(rows);
        grown
    }
}

/// The joint buffer's slots as one value each: what a skinned draw binds
/// at `slot * JOINT_SLOT`. Slot 0 is bound by every unskinned draw, and
/// never read.
pub(crate) type JointSlot = [[[f32; 4]; 4]; MAX_JOINTS];

/// A skin's palette as a slot.
pub(crate) fn joint_slot(palette: &[Mat4]) -> JointSlot {
    let mut slot = [[[0.0; 4]; 4]; MAX_JOINTS];
    for (s, m) in slot.iter_mut().zip(palette) {
        *s = m.0;
    }
    slot
}

/// The sphere around spheres: their box's.
pub(crate) fn union(
    spheres: impl Iterator<Item = (crate::math::Vec3, f32)>,
) -> Option<(crate::math::Vec3, f32)> {
    use crate::math::Vec3;
    let mut lo = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut hi = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for (c, r) in spheres {
        any = true;
        lo = Vec3::new(lo.x.min(c.x - r), lo.y.min(c.y - r), lo.z.min(c.z - r));
        hi = Vec3::new(hi.x.max(c.x + r), hi.y.max(c.y + r), hi.z.max(c.z + r));
    }
    any.then(|| {
        let centre = (lo + hi) * 0.5;
        (centre, (hi - centre).length())
    })
}
