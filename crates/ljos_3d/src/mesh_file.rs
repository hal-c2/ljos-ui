//! The `.mesh` files Qt's `balsam` writes, which a `Model`'s `source` names.
//!
//! A file ends in a table of its meshes; a `Model` draws the first. A mesh is
//! one buffer of corners, one of indices, and subsets of those indices, each
//! with a material of its own: [`MeshFile::parts`] holds a geometry per
//! subset, with only that subset's corners, so a renderer draws, culls and
//! sorts each by itself. Files come from anywhere: every count and offset in
//! one is checked before it is used, and what does not add up is an error.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use crate::geometry::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
use crate::math::Vec3;

const FILE_MAGIC: u32 = 555_777_497;
const MESH_MAGIC: u32 = 3_365_961_549;
const VERSIONS: std::ops::RangeInclusive<u16> = 3..=7;
const FOOTER: usize = 16;
const TABLE_ROW: usize = 16;
/// The bytes from a mesh's magic number to the end of its fixed header.
const HEADER: usize = 68;
/// Where a mesh's numbers are counted from, past its magic number and version.
const BODY: usize = 12;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MeshFileError {
    #[error("it is not a mesh file")]
    NotAMeshFile,
    #[error("it has no mesh in it")]
    NoMesh,
    #[error("it is a mesh file of a version, {0}, that is not known")]
    UnknownVersion(u16),
    #[error("it ends before its {0} do")]
    Truncated(&'static str),
    #[error("its {name} is kept as a number of a kind, {kind}, that is not known")]
    UnknownNumber { name: String, kind: u32 },
    #[error("its {0} does not fit in a corner")]
    OutsideCorner(String),
    #[error("it has corners but no attr_pos to place them")]
    NoPosition,
    #[error("it draws in a way, {0}, that is not known")]
    UnknownDrawMode(u32),
    #[error("subset {subset} reaches past the end of the mesh")]
    SubsetOutside { subset: usize },
    #[error("index {index} is past its {corners} corners")]
    IndexOutside { index: u32, corners: usize },
}

/// One subset, ready to draw.
#[derive(Debug, Clone)]
pub struct Part {
    pub geometry: Arc<CustomGeometry>,
    /// Nothing to draw: a subset of no shapes still takes up a place in the
    /// list of materials.
    pub empty: bool,
    /// Which corner of the file each of its vertices is: what the file's
    /// morph targets are numbered by.
    pub corners: Vec<u32>,
}

/// A morph target layer per attribute and target, as the file keeps them from
/// version 7: `width` by `width` texels of four floats each, attribute by
/// attribute within a target.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphTargets {
    pub count: u32,
    pub width: u32,
    /// What each layer of a target holds: `attr_pos`, `attr_norm` ...
    pub attributes: Vec<String>,
    pub texels: Vec<f32>,
}

/// A mesh file, read.
#[derive(Debug, Clone)]
pub struct MeshFile {
    pub version: u16,
    /// A geometry per subset, in the order of the materials that draw them.
    pub parts: Vec<Part>,
    /// How many joints the corners may name.
    pub joints: u32,
    pub morph_targets: Option<MorphTargets>,
    morphed: Arc<Mutex<Morphed>>,
}

/// Geometries as morphed by a part and its targets' weights and attributes,
/// so that a model whose weights hold still keeps one upload. Weak: what no
/// scene holds any more goes.
type Morphed = HashMap<(usize, Vec<(u32, i32)>), Weak<CustomGeometry>>;

impl MeshFile {
    /// Part `which`'s geometry with `targets` weighed in: a weight and the
    /// `MorphTarget.attributes` it moves, per target of the file.
    pub fn morphed(&self, which: usize, targets: &[(f32, i32)]) -> Arc<CustomGeometry> {
        let part = &self.parts[which];
        let Some(morphs) = &self.morph_targets else {
            return part.geometry.clone();
        };
        if targets.iter().all(|(weight, _)| *weight == 0.0) {
            return part.geometry.clone();
        }
        let key = (
            which,
            targets.iter().map(|(w, a)| (w.to_bits(), *a)).collect(),
        );
        let mut made = self.morphed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(geometry) = made.get(&key).and_then(Weak::upgrade) {
            return geometry;
        }
        made.retain(|_, geometry| geometry.strong_count() > 0);
        let geometry = Arc::new(morph_part(part, morphs, targets));
        made.insert(key, Arc::downgrade(&geometry));
        geometry
    }
}

/// `part`'s geometry with each target's layers added in, times its weight.
fn morph_part(part: &Part, morphs: &MorphTargets, targets: &[(f32, i32)]) -> CustomGeometry {
    let source = &part.geometry;
    let stride = source.stride().max(1);
    let mut data = source.vertex_data().to_vec();
    let per_layer = morphs.width as usize * morphs.width as usize;
    let layers = morphs.attributes.len();
    let offset_of = |semantic| {
        source
            .attributes()
            .iter()
            .find(|a| a.semantic == semantic)
            .map(|a| a.offset)
    };
    for (t, (weight, attributes)) in targets.iter().enumerate().take(morphs.count as usize) {
        for (a, name) in morphs.attributes.iter().enumerate() {
            let Some((flag, semantic, numbers)) = morph_layer(name) else {
                continue;
            };
            if attributes & flag == 0 {
                continue;
            }
            let Some(at) = offset_of(semantic) else {
                continue;
            };
            let layer = (t * layers + a) * per_layer;
            for (vertex, corner) in part.corners.iter().enumerate() {
                let texel = (layer + *corner as usize) * 4;
                let Some(delta) = morphs.texels.get(texel..texel + 4) else {
                    continue;
                };
                for (n, d) in delta.iter().enumerate().take(numbers) {
                    // A change of v, in a file measured from the bottom.
                    let d = *d;
                    let i = vertex * stride + at + n * 4;
                    let Some(bytes) = data.get_mut(i..i + 4) else {
                        continue;
                    };
                    let old = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    bytes.copy_from_slice(&(old + d * weight).to_le_bytes());
                }
            }
        }
    }
    let mut geometry = CustomGeometry::new();
    for a in source.attributes() {
        geometry.add_attribute(a.semantic, a.offset, a.component);
    }
    geometry.set_stride(stride);
    geometry.set_primitive_type(source.primitive_type());
    // The base shape's box would cull what has moved out of it.
    if let Some(at) = offset_of(Semantic::Position) {
        let mut least = [f32::MAX; 3];
        let mut most = [f32::MIN; 3];
        for row in data.chunks_exact(stride) {
            for (n, bytes) in row[at..].as_chunks::<4>().0.iter().take(3).enumerate() {
                let v = f32::from_le_bytes(*bytes);
                least[n] = least[n].min(v);
                most[n] = most[n].max(v);
            }
        }
        geometry.set_bounds(
            Vec3::new(least[0], least[1], least[2]),
            Vec3::new(most[0], most[1], most[2]),
        );
    }
    geometry.set_vertex_data(data);
    geometry.set_index_data(source.index_data().to_vec());
    geometry.update();
    geometry
}

/// A layer's `MorphTarget.attributes` flag, what it changes in the vertices
/// and how many numbers. Tangents are not kept, so they are not moved.
fn morph_layer(name: &str) -> Option<(i32, Semantic, usize)> {
    use crate::qml::enums::morph;
    Some(match name {
        "attr_pos" => (morph::POSITION, Semantic::Position, 3),
        "attr_norm" => (morph::NORMAL, Semantic::Normal, 3),
        "attr_uv0" => (morph::TEX_COORD_0, Semantic::TexCoord0, 2),
        "attr_uv1" => (morph::TEX_COORD_1, Semantic::TexCoord1, 2),
        "attr_color" => (morph::COLOR, Semantic::Color, 4),
        _ => return None,
    })
}

/// Bounds-checked reading. Nothing here adds without checking, as every number
/// comes from the file.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], MeshFileError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(MeshFileError::Truncated(self.what))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, MeshFileError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, MeshFileError> {
        self.u32().map(f32::from_bits)
    }

    fn vec3(&mut self) -> Result<Vec3, MeshFileError> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    /// On to the next multiple of four past the body's start: by four when
    /// it is at one already, as the format has it.
    fn align(&mut self, from: usize) -> Result<(), MeshFileError> {
        self.take(4 - (self.at - from) % 4).map(drop)
    }

    /// A name: its length, then its bytes up to a zero.
    fn name(&mut self, from: usize) -> Result<String, MeshFileError> {
        let len = self.u32()? as usize;
        let raw = self.take(len)?;
        self.align(from)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn number_size(kind: u32) -> Option<usize> {
    Some(match kind {
        1 | 2 => 1,
        3 | 4 | 9 => 2,
        5 | 6 | 10 => 4,
        11 => 8,
        _ => return None,
    })
}

/// One number of `kind` at the start of `b`, which holds it: its size was
/// checked against the corner when the attribute was read.
fn number(kind: u32, b: &[u8]) -> f32 {
    match kind {
        1 => f32::from(b[0]),
        2 => f32::from(b[0] as i8),
        3 => f32::from(u16::from_le_bytes([b[0], b[1]])),
        4 => f32::from(i16::from_le_bytes([b[0], b[1]])),
        5 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
        6 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
        9 => half(u16::from_le_bytes([b[0], b[1]])),
        10 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        _ => f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
    }
}

/// A 16 bit float.
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff) / 1024.0;
    sign * match exponent {
        0 => fraction * 2f32.powi(-14),
        31 if fraction == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + fraction) * 2f32.powi(exponent - 15),
    }
}

/// An attribute of every corner.
#[derive(Debug, Clone, Copy)]
struct Column {
    kind: u32,
    count: usize,
    offset: usize,
}

impl Column {
    /// The first `N` numbers of `corner`; what the file does not have is 0.
    fn read<const N: usize>(self, corner: &[u8]) -> [f32; N] {
        let size = number_size(self.kind).unwrap_or(1);
        std::array::from_fn(|i| {
            if i < self.count {
                number(self.kind, &corner[self.offset + i * size..])
            } else {
                0.0
            }
        })
    }

    /// Weights kept as whole numbers are fractions of the largest one.
    fn weights(self, corner: &[u8]) -> [f32; 4] {
        let scale = match self.kind {
            1 => 1.0 / 255.0,
            3 => 1.0 / 65535.0,
            _ => 1.0,
        };
        self.read::<4>(corner).map(|w| w * scale)
    }
}

/// The attributes the renderer draws with, where the file has them.
#[derive(Debug, Default, Clone, Copy)]
struct Columns {
    position: Option<Column>,
    normal: Option<Column>,
    uv0: Option<Column>,
    uv1: Option<Column>,
    color: Option<Column>,
    joints: Option<Column>,
    weights: Option<Column>,
}

impl Columns {
    /// Lays one corner's attributes out as [`Columns::describe`] says.
    fn write(&self, corner: &[u8], out: &mut Vec<u8>) {
        let mut floats = |values: &[f32]| {
            for v in values {
                out.extend_from_slice(&v.to_le_bytes());
            }
        };
        if let Some(c) = self.position {
            floats(&c.read::<3>(corner));
        }
        if let Some(c) = self.normal {
            floats(&c.read::<3>(corner));
        }
        if let Some(c) = self.uv0 {
            floats(&c.read::<2>(corner));
        }
        if let Some(c) = self.uv1 {
            floats(&c.read::<2>(corner));
        }
        if let Some(c) = self.color {
            let mut rgba = c.weights(corner);
            // Three numbers are an opaque colour.
            if c.count < 4 {
                rgba[3] = 1.0;
            }
            floats(&rgba);
        }
        if let (Some(j), Some(w)) = (self.joints, self.weights) {
            floats(&w.weights(corner));
            for joint in j.read::<4>(corner) {
                out.extend_from_slice(&(joint.clamp(0.0, 65535.0) as u16).to_le_bytes());
            }
        }
    }

    /// Says how [`Columns::write`] lays a corner out; returns its stride.
    fn describe(&self, g: &mut CustomGeometry) -> usize {
        let mut at = 0;
        let mut add = |semantic, floats: usize| {
            g.add_attribute(semantic, at, ComponentType::F32);
            at += floats * 4;
        };
        add(Semantic::Position, 3);
        if self.normal.is_some() {
            add(Semantic::Normal, 3);
        }
        if self.uv0.is_some() {
            add(Semantic::TexCoord0, 2);
        }
        if self.uv1.is_some() {
            add(Semantic::TexCoord1, 2);
        }
        if self.color.is_some() {
            add(Semantic::Color, 4);
        }
        if self.joints.is_some() && self.weights.is_some() {
            add(Semantic::Weight, 4);
            g.add_attribute(Semantic::Joint, at, ComponentType::U16);
            at += 8;
        }
        at
    }
}

/// What the file's indices are kept as.
#[derive(Debug, Clone, Copy)]
struct Indices<'a> {
    bytes: &'a [u8],
    size: usize,
}

impl Indices<'_> {
    fn count(&self) -> usize {
        self.bytes.len() / self.size
    }

    fn get(&self, i: usize) -> u32 {
        let b = &self.bytes[i * self.size..];
        match self.size {
            2 => u32::from(u16::from_le_bytes([b[0], b[1]])),
            4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => u32::from(b[0]),
        }
    }
}

/// How the file's draw mode makes shapes of a run of indices.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DrawMode {
    Points,
    LineStrip,
    LineLoop,
    Lines,
    TriangleStrip,
    TriangleFan,
    Triangles,
}

impl DrawMode {
    fn of(mode: u32) -> Result<Self, MeshFileError> {
        Ok(match mode {
            1 => DrawMode::Points,
            2 => DrawMode::LineStrip,
            3 => DrawMode::LineLoop,
            4 => DrawMode::Lines,
            5 => DrawMode::TriangleStrip,
            6 => DrawMode::TriangleFan,
            7 => DrawMode::Triangles,
            other => return Err(MeshFileError::UnknownDrawMode(other)),
        })
    }

    /// `corners` as a list of shapes the geometry draws: strips and fans
    /// become triangles here so that a clockwise file's are all turned the
    /// same way, by one rule.
    fn shapes(self, corners: Vec<u32>, clockwise: bool) -> (PrimitiveType, Vec<u32>) {
        let triangles = |mut list: Vec<u32>| {
            if clockwise {
                for t in list.as_chunks_mut::<3>().0 {
                    t.swap(1, 2);
                }
            }
            (PrimitiveType::Triangles, list)
        };
        match self {
            DrawMode::Points => (PrimitiveType::Points, corners),
            DrawMode::Lines => (PrimitiveType::Lines, corners),
            DrawMode::LineStrip => (PrimitiveType::LineStrip, corners),
            DrawMode::LineLoop => {
                let mut closed = corners;
                if let Some(&first) = closed.first() {
                    closed.push(first);
                }
                (PrimitiveType::LineStrip, closed)
            }
            DrawMode::Triangles => {
                let mut list = corners;
                list.truncate(list.len() / 3 * 3);
                triangles(list)
            }
            DrawMode::TriangleStrip => triangles(
                corners
                    .windows(3)
                    .enumerate()
                    .flat_map(|(k, w)| {
                        if k % 2 == 0 {
                            [w[0], w[1], w[2]]
                        } else {
                            [w[1], w[0], w[2]]
                        }
                    })
                    .collect(),
            ),
            DrawMode::TriangleFan => triangles(
                corners
                    .get(1..)
                    .unwrap_or_default()
                    .windows(2)
                    .flat_map(|w| [corners[0], w[0], w[1]])
                    .collect(),
            ),
        }
    }
}

struct Subset {
    count: usize,
    offset: usize,
    min: Vec3,
    max: Vec3,
}

impl MeshFile {
    /// Every corner and triangle of the mesh: what a collision shape is made
    /// from.
    pub fn triangles(&self) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for part in self.parts.iter().filter(|p| !p.empty) {
            let Ok(mesh) = part.geometry.to_mesh() else {
                continue;
            };
            let first = positions.len() as u32;
            let (corners, _) = mesh.indices.as_chunks::<3>();
            triangles.extend(corners.iter().map(|c| c.map(|i| first + i)));
            positions.extend(mesh.positions);
        }
        (positions, triangles)
    }

    /// Reads the file's first mesh.
    pub fn read(bytes: &[u8]) -> Result<MeshFile, MeshFileError> {
        let size = bytes.len();
        let footer = size
            .checked_sub(FOOTER)
            .ok_or(MeshFileError::NotAMeshFile)?;
        if u32_at(bytes, footer) != Some(FILE_MAGIC) {
            return Err(MeshFileError::NotAMeshFile);
        }
        let meshes = u32_at(bytes, size - 4).ok_or(MeshFileError::NotAMeshFile)? as usize;
        if meshes == 0 {
            return Err(MeshFileError::NoMesh);
        }
        let row = meshes
            .checked_mul(TABLE_ROW)
            .and_then(|table| footer.checked_sub(table))
            .ok_or(MeshFileError::Truncated("mesh table"))?;
        let start = bytes
            .get(row..row + 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .and_then(|s| usize::try_from(s).ok())
            .ok_or(MeshFileError::Truncated("mesh table"))?;
        if start.checked_add(HEADER).is_none_or(|end| end > size)
            || u32_at(bytes, start) != Some(MESH_MAGIC)
        {
            return Err(MeshFileError::NotAMeshFile);
        }
        let version = u16::from_le_bytes([bytes[start + 4], bytes[start + 5]]);
        if !VERSIONS.contains(&version) {
            return Err(MeshFileError::UnknownVersion(version));
        }
        read_mesh(bytes, start + BODY, version)
    }
}

fn read_mesh(bytes: &[u8], base: usize, version: u16) -> Result<MeshFile, MeshFileError> {
    let mut r = Reader {
        bytes,
        at: base,
        what: "header",
    };
    let target_attributes = r.u32()? as usize;
    let entries = r.u32()? as usize;
    let stride = (r.u32()? as usize).max(1);
    let target_bytes = r.u32()? as usize;
    let vertex_bytes = r.u32()? as usize;
    let index_size = match r.u32()? {
        3 => 2,
        5 => 4,
        _ => 1,
    };
    r.u32()?; // where the indices are, which the format has anyway
    let index_bytes = r.u32()? as usize;
    let targets = r.u32()?;
    let subsets = r.u32()? as usize;
    let joints = r.u32()?;
    r.u32()?;
    let mode = r.u32()?;
    let winding = r.u32()?;

    r.what = "attributes";
    let mut listed = Vec::new();
    for _ in 0..entries {
        r.u32()?; // where its name is, which comes next anyway
        listed.push((r.u32()?, r.u32()? as usize, r.u32()? as usize));
    }
    r.align(base)?;
    let mut columns = Columns::default();
    for (kind, count, offset) in listed {
        let name = r.name(base)?;
        let Some(size) = number_size(kind) else {
            return Err(MeshFileError::UnknownNumber { name, kind });
        };
        count
            .checked_mul(size)
            .and_then(|bytes| bytes.checked_add(offset))
            .filter(|&end| end <= stride)
            .ok_or_else(|| MeshFileError::OutsideCorner(name.clone()))?;
        let column = Some(Column {
            kind,
            count,
            offset,
        });
        match name.as_str() {
            "attr_pos" => columns.position = column,
            "attr_norm" => columns.normal = column,
            "attr_uv0" => columns.uv0 = column,
            "attr_uv1" => columns.uv1 = column,
            "attr_color" => columns.color = column,
            "attr_joints" => columns.joints = column,
            "attr_weights" => columns.weights = column,
            _ => {}
        }
    }

    r.what = "corners";
    let vertices = r.take(vertex_bytes)?;
    r.align(base)?;
    r.what = "triangles";
    let indices = Indices {
        bytes: r.take(index_bytes)?,
        size: index_size,
    };
    r.align(base)?;
    r.what = "subsets";
    let mut list = Vec::new();
    let mut name_lengths = Vec::new();
    let mut simpler = 0usize;
    for _ in 0..subsets {
        let count = r.u32()? as usize;
        let offset = r.u32()? as usize;
        let min = r.vec3()?;
        let max = r.vec3()?;
        r.u32()?; // where its name is
        name_lengths.push(r.u32()? as usize);
        if version >= 5 {
            r.take(8)?; // the size of its light map
        }
        if version >= 6 {
            simpler = simpler
                .checked_add(r.u32()? as usize)
                .ok_or(MeshFileError::Truncated(r.what))?;
        }
        list.push(Subset {
            count,
            offset,
            min,
            max,
        });
    }

    r.what = "targets";
    // Older files keep their targets' numbers among the corners' or, as they
    // say nothing of them in these fields, not at all.
    let morph_targets = if version >= 7 && target_attributes > 0 && targets > 0 && target_bytes > 0
    {
        Some(read_targets(
            &mut r,
            base,
            &name_lengths,
            simpler,
            target_attributes,
            targets,
            target_bytes,
        )?)
    } else {
        None
    };

    if vertex_bytes > 0 && columns.position.is_none() {
        return Err(MeshFileError::NoPosition);
    }
    let mode = DrawMode::of(mode)?;
    let corners = vertices.len() / stride;
    let clockwise = winding == 1;
    let parts = list
        .iter()
        .enumerate()
        .map(|(i, subset)| {
            part(
                i, subset, vertices, stride, corners, indices, &columns, mode, clockwise,
            )
        })
        .collect::<Result<_, _>>()?;
    Ok(MeshFile {
        version,
        parts,
        joints,
        morph_targets,
        morphed: Arc::default(),
    })
}

fn read_targets(
    r: &mut Reader<'_>,
    base: usize,
    name_lengths: &[usize],
    simpler: usize,
    attributes: usize,
    count: u32,
    size: usize,
) -> Result<MorphTargets, MeshFileError> {
    let truncated = MeshFileError::Truncated("targets");
    r.align(base)?;
    // After the subsets come their names, two bytes to a letter, and the
    // simpler versions of each, three numbers to one.
    for &length in name_lengths {
        r.take(
            length
                .checked_mul(2)
                .ok_or(MeshFileError::Truncated("targets"))?,
        )?;
        r.align(base)?;
    }
    r.take(
        simpler
            .checked_mul(12)
            .ok_or(MeshFileError::Truncated("targets"))?,
    )?;
    r.align(base)?;
    r.take(
        attributes
            .checked_mul(16)
            .ok_or(MeshFileError::Truncated("targets"))?,
    )?;
    r.align(base)?;
    let mut names = Vec::new();
    for _ in 0..attributes {
        names.push(r.name(base)?);
    }
    let layers = attributes
        .checked_mul(count as usize)
        .filter(|&l| l > 0)
        .ok_or(MeshFileError::Truncated("targets"))?;
    // Square layers of four floats a texel, as many as fill the bytes.
    let texels_a_layer = size / layers / 16;
    let width = (texels_a_layer as f64).sqrt().ceil() as usize;
    let used = width
        .checked_mul(width)
        .and_then(|t| t.checked_mul(16))
        .and_then(|b| b.checked_mul(layers))
        .filter(|&b| b <= size)
        .ok_or(truncated)?;
    let raw = r.take(size)?;
    Ok(MorphTargets {
        count,
        width: width as u32,
        attributes: names,
        texels: raw[..used]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
    })
}

/// The geometry of one subset, with only the corners it uses.
#[allow(clippy::too_many_arguments)]
fn part(
    which: usize,
    subset: &Subset,
    vertices: &[u8],
    stride: usize,
    corners: usize,
    indices: Indices<'_>,
    columns: &Columns,
    mode: DrawMode,
    clockwise: bool,
) -> Result<Part, MeshFileError> {
    let end = subset
        .offset
        .checked_add(subset.count)
        .ok_or(MeshFileError::SubsetOutside { subset: which })?;
    // Without indices a subset is a run of the corners themselves.
    let run: Vec<u32> = if indices.bytes.is_empty() {
        if end > corners {
            return Err(MeshFileError::SubsetOutside { subset: which });
        }
        (subset.offset as u32..end as u32).collect()
    } else {
        if end > indices.count() {
            return Err(MeshFileError::SubsetOutside { subset: which });
        }
        (subset.offset..end).map(|i| indices.get(i)).collect()
    };
    let (primitive, run) = mode.shapes(run, clockwise);

    let mut geometry = CustomGeometry::new();
    let new_stride = columns.describe(&mut geometry);
    geometry.set_stride(new_stride);
    geometry.set_primitive_type(primitive);

    // Each corner keeps its number in the order it is first used.
    let mut placed = vec![u32::MAX; corners];
    let mut kept = Vec::new();
    let mut data = Vec::new();
    let mut compact = Vec::with_capacity(run.len());
    for index in run {
        let slot = placed
            .get_mut(index as usize)
            .ok_or(MeshFileError::IndexOutside { index, corners })?;
        if *slot == u32::MAX {
            *slot = (data.len() / new_stride.max(1)) as u32;
            kept.push(index);
            columns.write(&vertices[index as usize * stride..], &mut data);
        }
        compact.push(*slot);
    }
    let empty = compact.is_empty();
    let wide = data.len() / new_stride.max(1) > usize::from(u16::MAX);
    let index_data = if wide {
        compact.iter().flat_map(|i| i.to_le_bytes()).collect()
    } else {
        compact
            .iter()
            .flat_map(|&i| (i as u16).to_le_bytes())
            .collect()
    };
    geometry.add_attribute(
        Semantic::Index,
        0,
        if wide {
            ComponentType::U32
        } else {
            ComponentType::U16
        },
    );
    geometry.set_vertex_data(data);
    geometry.set_index_data(index_data);
    if sane_box(subset.min, subset.max) {
        geometry.set_bounds(subset.min, subset.max);
    }
    geometry.update();
    Ok(Part {
        geometry: Arc::new(geometry),
        empty,
        corners: kept,
    })
}

/// Whether a box can be trusted for culling; otherwise the corners say.
fn sane_box(min: Vec3, max: Vec3) -> bool {
    let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    finite(min) && finite(max) && min.x <= max.x && min.y <= max.y && min.z <= max.z
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file by the bytes, as the format lays them out.
    struct Build {
        version: u16,
        stride: u32,
        attributes: Vec<(&'static str, u32, u32, u32)>,
        vertices: Vec<u8>,
        index_kind: u32,
        indices: Vec<u8>,
        subsets: Vec<(u32, u32)>,
        mode: u32,
        winding: u32,
        targets: Option<(Vec<&'static str>, u32, Vec<f32>)>,
    }

    impl Default for Build {
        fn default() -> Self {
            // One triangle, positions only.
            let vertices = [
                [0.0f32, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [5.0, 5.0, 5.0],
            ]
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect();
            Build {
                version: 5,
                stride: 12,
                attributes: vec![("attr_pos", 10, 3, 0)],
                vertices,
                index_kind: 3,
                indices: [0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect(),
                subsets: vec![(0, 3)],
                mode: 7,
                winding: 2,
                targets: None,
            }
        }
    }

    impl Build {
        fn bytes(&self) -> Vec<u8> {
            let mut b: Vec<u8> = Vec::new();
            let put = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
            let align = |b: &mut Vec<u8>| b.resize(b.len() + 4 - b.len() % 4, 0);
            let (target_attrs, target_count, target_data) = match &self.targets {
                Some((names, count, data)) => (names.len() as u32, *count, data.len() as u32 * 4),
                None => (0, 0, 0),
            };
            for v in [
                target_attrs,
                self.attributes.len() as u32,
                self.stride,
                target_data,
                self.vertices.len() as u32,
                self.index_kind,
                0,
                self.indices.len() as u32,
                target_count,
                self.subsets.len() as u32,
                3,
                0,
                self.mode,
                self.winding,
            ] {
                put(&mut b, v);
            }
            for (_, kind, count, offset) in &self.attributes {
                for v in [0, *kind, *count, *offset] {
                    put(&mut b, v);
                }
            }
            align(&mut b);
            let name = |b: &mut Vec<u8>, n: &str| {
                b.extend_from_slice(&(n.len() as u32 + 1).to_le_bytes());
                b.extend_from_slice(n.as_bytes());
                b.push(0);
                b.resize(b.len() + 4 - b.len() % 4, 0);
            };
            for (n, ..) in &self.attributes {
                name(&mut b, n);
            }
            b.extend_from_slice(&self.vertices);
            align(&mut b);
            b.extend_from_slice(&self.indices);
            align(&mut b);
            for (offset, count) in &self.subsets {
                put(&mut b, *count);
                put(&mut b, *offset);
                for f in [0.0f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
                    b.extend_from_slice(&f.to_le_bytes());
                }
                put(&mut b, 0);
                put(&mut b, 0);
                if self.version >= 5 {
                    put(&mut b, 0);
                    put(&mut b, 0);
                }
                if self.version >= 6 {
                    put(&mut b, 0);
                }
            }
            if let Some((names, _, data)) = &self.targets {
                align(&mut b);
                for _ in &self.subsets {
                    align(&mut b);
                }
                align(&mut b);
                b.resize(b.len() + names.len() * 16, 0);
                align(&mut b);
                for n in names {
                    name(&mut b, n);
                }
                for f in data {
                    b.extend_from_slice(&f.to_le_bytes());
                }
            }
            let mut out = Vec::new();
            out.extend_from_slice(&MESH_MAGIC.to_le_bytes());
            out.extend_from_slice(&self.version.to_le_bytes());
            out.extend_from_slice(&[0; 6]);
            out.extend_from_slice(&b);
            out.extend_from_slice(&0u64.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&FILE_MAGIC.to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&1u32.to_le_bytes());
            out
        }

        fn read(&self) -> Result<MeshFile, MeshFileError> {
            MeshFile::read(&self.bytes())
        }
    }

    fn positions(part: &Part) -> Vec<[f32; 3]> {
        part.geometry.to_mesh().unwrap().positions
    }

    #[test]
    fn every_version_reads() {
        for version in 3..=7 {
            let file = Build {
                version,
                ..Build::default()
            }
            .read()
            .unwrap_or_else(|e| panic!("version {version}: {e}"));
            assert_eq!(file.version, version);
            assert_eq!(file.parts.len(), 1);
        }
    }

    #[test]
    fn a_subset_holds_only_the_corners_it_uses() {
        let file = Build::default().read().unwrap();
        assert_eq!(file.parts[0].geometry.vertex_data().len(), 3 * 12);
        assert_eq!(file.joints, 3);
    }

    #[test]
    fn indices_of_every_width_read() {
        let bytes: [(u32, Vec<u8>); 3] = [
            (1, vec![0u8, 1, 2]),
            (
                3,
                [0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect(),
            ),
            (
                5,
                [0u32, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect(),
            ),
        ];
        for (index_kind, indices) in bytes {
            let file = Build {
                index_kind,
                indices,
                ..Build::default()
            }
            .read()
            .unwrap();
            let mesh = file.parts[0].geometry.to_mesh().unwrap();
            assert_eq!(mesh.positions[1], [1.0, 0.0, 0.0], "kind {index_kind}");
            assert_eq!(mesh.indices.len(), 3);
        }
    }

    #[test]
    fn no_indices_is_a_run_of_corners() {
        let file = Build {
            indices: Vec::new(),
            subsets: vec![(1, 3)],
            ..Build::default()
        }
        .read()
        .unwrap();
        assert_eq!(positions(&file.parts[0])[2], [5.0, 5.0, 5.0]);
    }

    #[test]
    fn a_clockwise_file_is_turned_to_face_the_same_way() {
        let file = Build {
            winding: 1,
            ..Build::default()
        }
        .read()
        .unwrap();
        // Corners are numbered as used, so the swap shows in their order.
        let mesh = file.parts[0].geometry.to_mesh().unwrap();
        let at = |n: usize| mesh.positions[mesh.indices[n] as usize];
        // Flat faces come out unindexed; order them through the indices.
        assert_eq!(at(1), [0.0, 1.0, 0.0]);
        assert_eq!(at(2), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn strips_and_fans_become_triangles() {
        for (mode, want) in [(5, 6), (6, 6)] {
            let indices = [0u16, 1, 2, 3];
            let file = Build {
                mode,
                indices: indices.iter().flat_map(|i| i.to_le_bytes()).collect(),
                subsets: vec![(0, 4)],
                ..Build::default()
            }
            .read()
            .unwrap();
            assert_eq!(
                file.parts[0].geometry.to_mesh().unwrap().indices.len(),
                want
            );
        }
    }

    #[test]
    fn a_line_loop_closes() {
        let file = Build {
            mode: 3,
            ..Build::default()
        }
        .read()
        .unwrap();
        assert_eq!(
            file.parts[0].geometry.primitive_type(),
            PrimitiveType::LineStrip
        );
        assert_eq!(file.parts[0].geometry.index_data().len(), 4 * 2);
    }

    #[test]
    fn joints_and_weights_come_out_whole_and_as_fractions() {
        let mut vertices = Vec::new();
        for corner in 0..3u8 {
            for f in [f32::from(corner), 0.0, 0.0] {
                vertices.extend_from_slice(&f.to_le_bytes());
            }
            vertices.extend_from_slice(&[corner, 1, 2, 3]);
            vertices.extend_from_slice(&[255, 0, 0, 0]);
        }
        let file = Build {
            stride: 20,
            attributes: vec![
                ("attr_pos", 10, 3, 0),
                ("attr_joints", 1, 4, 12),
                ("attr_weights", 1, 4, 16),
            ],
            vertices,
            ..Build::default()
        }
        .read()
        .unwrap();
        let mesh = file.parts[0].geometry.to_mesh().unwrap();
        assert_eq!(mesh.joints[1], [1, 1, 2, 3]);
        assert_eq!(mesh.weights[1], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn numbers_of_every_kind_read() {
        assert_eq!(number(1, &[200]), 200.0);
        assert_eq!(number(2, &[0xff]), -1.0);
        assert_eq!(number(3, &40_000u16.to_le_bytes()), 40_000.0);
        assert_eq!(number(4, &(-2i16).to_le_bytes()), -2.0);
        assert_eq!(number(5, &7u32.to_le_bytes()), 7.0);
        assert_eq!(number(6, &(-7i32).to_le_bytes()), -7.0);
        assert_eq!(number(9, &0x3c00u16.to_le_bytes()), 1.0);
        assert_eq!(number(9, &0xc000u16.to_le_bytes()), -2.0);
        assert_eq!(number(10, &1.5f32.to_le_bytes()), 1.5);
        assert_eq!(number(11, &2.5f64.to_le_bytes()), 2.5);
    }

    #[test]
    fn morph_targets_of_version_7_read() {
        // Two attributes, one target, four texels a layer.
        let data: Vec<f32> = (0..2 * 4 * 4).map(|i| i as f32).collect();
        let file = Build {
            version: 7,
            targets: Some((vec!["attr_pos", "attr_norm"], 1, data)),
            ..Build::default()
        }
        .read()
        .unwrap();
        let targets = file.morph_targets.unwrap();
        assert_eq!(targets.count, 1);
        assert_eq!(targets.width, 2);
        assert_eq!(targets.attributes, ["attr_pos", "attr_norm"]);
        assert_eq!(targets.texels.len(), 32);
        assert_eq!(targets.texels[31], 31.0);
    }

    #[test]
    fn a_subset_of_nothing_keeps_its_place() {
        let file = Build {
            subsets: vec![(0, 0), (0, 3)],
            ..Build::default()
        }
        .read()
        .unwrap();
        assert!(file.parts[0].empty);
        assert!(!file.parts[1].empty);
    }

    #[test]
    fn what_is_not_a_mesh_file_says_so() {
        assert_eq!(
            MeshFile::read(b"").unwrap_err(),
            MeshFileError::NotAMeshFile
        );
        assert_eq!(
            MeshFile::read(&[0; 200]).unwrap_err(),
            MeshFileError::NotAMeshFile
        );
    }

    #[test]
    fn a_file_with_no_mesh_says_so() {
        let mut bytes = Build::default().bytes();
        let count = bytes.len() - 4;
        bytes[count..].fill(0);
        assert_eq!(MeshFile::read(&bytes).unwrap_err(), MeshFileError::NoMesh);
    }

    #[test]
    fn a_table_past_the_file_says_so() {
        let mut bytes = Build::default().bytes();
        let count = bytes.len() - 4;
        bytes[count..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            MeshFile::read(&bytes).unwrap_err(),
            MeshFileError::Truncated(_)
        ));
    }

    #[test]
    fn an_unknown_version_says_which() {
        let file = Build {
            version: 9,
            ..Build::default()
        }
        .read();
        assert_eq!(file.unwrap_err(), MeshFileError::UnknownVersion(9));
    }

    #[test]
    fn an_unknown_number_kind_names_the_attribute() {
        let file = Build {
            attributes: vec![("attr_pos", 42, 3, 0)],
            ..Build::default()
        }
        .read();
        assert_eq!(
            file.unwrap_err(),
            MeshFileError::UnknownNumber {
                name: "attr_pos".to_owned(),
                kind: 42
            }
        );
    }

    #[test]
    fn an_attribute_past_the_corner_says_so() {
        let file = Build {
            attributes: vec![("attr_pos", 10, 3, 8)],
            ..Build::default()
        }
        .read();
        assert_eq!(
            file.unwrap_err(),
            MeshFileError::OutsideCorner("attr_pos".to_owned())
        );
    }

    #[test]
    fn corners_with_nothing_to_place_them_say_so() {
        let file = Build {
            attributes: vec![("attr_norm", 10, 3, 0)],
            ..Build::default()
        }
        .read();
        assert_eq!(file.unwrap_err(), MeshFileError::NoPosition);
    }

    #[test]
    fn an_unknown_draw_mode_says_which() {
        let file = Build {
            mode: 99,
            ..Build::default()
        }
        .read();
        assert_eq!(file.unwrap_err(), MeshFileError::UnknownDrawMode(99));
    }

    #[test]
    fn a_subset_past_the_indices_says_which() {
        let file = Build {
            subsets: vec![(0, 3), (2, 2)],
            ..Build::default()
        }
        .read();
        assert_eq!(
            file.unwrap_err(),
            MeshFileError::SubsetOutside { subset: 1 }
        );
    }

    #[test]
    fn an_index_past_the_corners_says_which() {
        let file = Build {
            indices: [0u16, 1, 9].iter().flat_map(|i| i.to_le_bytes()).collect(),
            ..Build::default()
        }
        .read();
        assert_eq!(
            file.unwrap_err(),
            MeshFileError::IndexOutside {
                index: 9,
                corners: 4
            }
        );
    }

    // Whatever a cut or a flipped byte does to the numbers, the answer is an
    // error or a mesh, never a panic.
    #[test]
    fn a_damaged_file_never_panics() {
        for version in [3, 5, 7] {
            let build = Build {
                version,
                targets: (version == 7).then(|| (vec!["attr_pos"], 1, vec![0.0; 16])),
                ..Build::default()
            };
            let good = build.bytes();
            for cut in 0..good.len() {
                let _ = MeshFile::read(&good[..cut]);
            }
            for at in 0..good.len() {
                for value in [0x00, 0x7f, 0xff] {
                    let mut bad = good.clone();
                    bad[at] = value;
                    let _ = MeshFile::read(&bad);
                }
            }
        }
    }

    /// The corpus is a submodule that CI's `check` job doesn't fetch, so these
    /// tests say they were skipped rather than fail on its absence. A checkout
    /// that has it gets the whole check: a missing file there is still a panic.
    fn demos(test: &str) -> Option<std::path::PathBuf> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/qtdoc/examples/demos");
        if root.is_dir() {
            return Some(root);
        }
        use std::io::Write as _;
        // To the stream itself: the harness swallows what a passing test
        // prints.
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPED {test}: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
            root.display()
        );
        None
    }

    // The corpus is real output of Qt's tool, at versions 3, 5 and 7, each
    // read back and built for the renderer.
    #[test]
    fn the_corpus_files_read() {
        let Some(root) = demos("the_corpus_files_read") else {
            return;
        };
        let mut stack = vec![root];
        let mut read = 0;
        let mut versions = std::collections::BTreeSet::new();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "mesh") {
                    let bytes = std::fs::read(&path).unwrap();
                    let file = MeshFile::read(&bytes)
                        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    assert!(!file.parts.is_empty(), "{}", path.display());
                    for part in &file.parts {
                        part.geometry
                            .validate()
                            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    }
                    versions.insert(file.version);
                    read += 1;
                }
            }
        }
        assert!(read >= 52, "only {read} files read");
        assert_eq!(versions.into_iter().collect::<Vec<_>>(), [3, 5, 7]);
    }

    #[test]
    fn the_robot_arm_has_its_subsets_and_real_bounds() {
        let Some(root) = demos("the_robot_arm_has_its_subsets_and_real_bounds") else {
            return;
        };
        let bytes = std::fs::read(root.join("robotarm/content/meshes/base.mesh")).unwrap();
        let file = MeshFile::read(&bytes).unwrap();
        assert_eq!(file.version, 5);
        assert!(file.parts.len() > 1);
        for part in &file.parts {
            let (centre, radius) = part.geometry.bounding_sphere();
            assert!(radius > 0.0 && centre.x.is_finite());
            assert!(
                part.geometry
                    .to_mesh()
                    .unwrap()
                    .normals
                    .iter()
                    .any(|n| n != &[0.0; 3])
            );
        }
    }
}
