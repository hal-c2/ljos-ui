//! glTF 2.0 files read into what a [`Scene`] draws: QtQuick3D's
//! `RuntimeLoader`.
//!
//! Reading is two steps so that a host can fetch a file's other files its
//! own way between them: [`parse`] reads the JSON (or the GLB container)
//! and says which external files the document names; [`Document::build`]
//! is given those files' bytes and makes an [`Asset`]: the file's nodes,
//! their meshes as [`CustomGeometry`] and their materials. Both are plain
//! work on bytes, meant for a background thread, and neither touches a
//! file, the network or a GPU.
//!
//! Every range a file gives (a buffer view in a buffer, an accessor in a
//! view, an index into the vertices) is checked here before it is read, so
//! a broken or hostile file is an error or a warning, never a panic.

use std::collections::HashMap;
use std::sync::Arc;

use crate::geometry::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
use crate::math::{Mat4, Quat, Vec3};
use crate::scene::{
    AlphaMode, Color, CullMode, Lighting, Material, MeshSource, Model, Node, NodeId, NodeKind,
    PrincipledMaps, PrincipledMaterial, Scene, Transform,
};
use crate::texture::{self, Channel, PixelEncoding, Sampling, Texture, TexturePixels, Tiling};

/// The widest or tallest picture a file's texture may be: a 8192 by 8192
/// RGBA texture is 256 MiB, which is already more than a phone gives one.
pub const MAX_IMAGE_SIDE: u32 = 8192;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoadError {
    /// The bytes are not glTF: JSON that does not parse, a GLB container
    /// that is cut short, or a document that refers to what it lacks.
    #[error("not a glTF 2.0 file: {0}")]
    Parse(String),
    /// A buffer's bytes could not be had or are fewer than it says.
    #[error("buffer {index} ({uri}): {reason}")]
    Buffer {
        index: usize,
        uri: String,
        reason: String,
    },
    /// The file names no node that can be drawn.
    #[error("the file has no nodes")]
    Empty,
}

/// A parsed glTF file whose external files have not been read yet.
pub struct Document {
    document: ::gltf::Document,
    blob: Option<Vec<u8>>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("nodes", &self.document.nodes().len())
            .field("blob", &self.blob.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// Reads a `.gltf` (JSON) or `.glb` (binary) file.
pub fn parse(bytes: &[u8]) -> Result<Document, LoadError> {
    let gltf = ::gltf::Gltf::from_slice_without_validation(bytes)
        .map_err(|e| LoadError::Parse(describe(&e)))?;
    let mut json = gltf.document.into_json();
    check_what_validation_skips(&mut json)?;
    let document = ::gltf::Document::from_json(json).map_err(|e| LoadError::Parse(describe(&e)))?;
    Ok(Document {
        document,
        blob: gltf.blob,
    })
}

// gltf-json's validation leaves some of what the gltf crate then unwraps
// unchecked, so a file that breaks those rules would panic the reader:
// - a primitive's POSITION accessor is looked up before its index is
//   checked;
// - an image needs a URI or a buffer view, and one in a buffer view a MIME
//   type. Images are told apart by their bytes here, so a missing MIME
//   type is filled in rather than refused.
fn check_what_validation_skips(json: &mut ::gltf::json::Root) -> Result<(), LoadError> {
    use ::gltf::json::validation::Checked;
    for (m, mesh) in json.meshes.iter().enumerate() {
        for (p, primitive) in mesh.primitives.iter().enumerate() {
            let position = primitive
                .attributes
                .get(&Checked::Valid(::gltf::json::mesh::Semantic::Positions));
            if position.is_some_and(|a| a.value() >= json.accessors.len()) {
                return Err(LoadError::Parse(format!(
                    "meshes[{m}].primitives[{p}].attributes.POSITION: Index out of range"
                )));
            }
        }
    }
    for (i, image) in json.images.iter_mut().enumerate() {
        match (&image.buffer_view, &image.uri) {
            (None, None) => {
                return Err(LoadError::Parse(format!(
                    "images[{i}]: neither a uri nor a bufferView"
                )));
            }
            (Some(_), _) if image.mime_type.is_none() => {
                image.mime_type = Some(::gltf::json::image::MimeType("image/png".to_owned()));
            }
            _ => {}
        }
    }
    Ok(())
}

// The crate's validation error is a list of JSON paths; the first one is
// what a person fixing the file needs to look at.
fn describe(e: &::gltf::Error) -> String {
    match e {
        ::gltf::Error::Validation(errors) => match errors.first() {
            Some((path, error)) if errors.len() == 1 => format!("{path}: {error}"),
            Some((path, error)) => {
                format!("{path}: {error} (and {} more)", errors.len() - 1)
            }
            None => e.to_string(),
        },
        _ => e.to_string(),
    }
}

impl Document {
    /// The files the document names beside itself, buffers and images, as
    /// they are written in it (relative URIs, still percent-encoded). Data
    /// URIs and the GLB's own chunk are not among them.
    pub fn external_uris(&self) -> Vec<String> {
        let buffers = self.document.buffers().filter_map(|b| match b.source() {
            ::gltf::buffer::Source::Uri(uri) => Some(uri),
            ::gltf::buffer::Source::Bin => None,
        });
        let images = self.document.images().filter_map(|i| match i.source() {
            ::gltf::image::Source::Uri { uri, .. } => Some(uri),
            ::gltf::image::Source::View { .. } => None,
        });
        let mut out: Vec<String> = Vec::new();
        for uri in buffers.chain(images) {
            if !uri.starts_with("data:") && !out.iter().any(|u| u == uri) {
                out.push(uri.to_owned());
            }
        }
        out
    }

    /// Makes the asset, asking `fetch` for each of
    /// [`external_uris`](Self::external_uris). A buffer that cannot be had
    /// is an error; an image that cannot be had or decoded is a warning
    /// and its material keeps its factors.
    pub fn build(
        &self,
        fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Asset, LoadError> {
        let doc = &self.document;
        let mut buffers = Vec::with_capacity(doc.buffers().len());
        for b in doc.buffers() {
            let (uri, bytes) = match b.source() {
                ::gltf::buffer::Source::Bin => ("the GLB chunk".to_owned(), self.blob.clone()),
                ::gltf::buffer::Source::Uri(uri) => {
                    let got = if uri.starts_with("data:") {
                        data_uri(uri)
                    } else {
                        fetch(uri)
                    };
                    let bytes = got.map_err(|reason| LoadError::Buffer {
                        index: b.index(),
                        uri: shorten(uri),
                        reason,
                    })?;
                    (shorten(uri), Some(bytes))
                }
            };
            let Some(bytes) = bytes else {
                return Err(LoadError::Buffer {
                    index: b.index(),
                    uri,
                    reason: "the file has no binary chunk".to_owned(),
                });
            };
            if bytes.len() < b.length() {
                return Err(LoadError::Buffer {
                    index: b.index(),
                    uri,
                    reason: format!("{} bytes, but it says {}", bytes.len(), b.length()),
                });
            }
            buffers.push(bytes);
        }
        let mut builder = Builder {
            doc,
            buffers: &buffers,
            fetch,
            images: HashMap::new(),
            warnings: Vec::new(),
        };
        builder.asset()
    }
}

// A data URI can be megabytes; an error message needs only its start.
fn shorten(uri: &str) -> String {
    if uri.starts_with("data:") {
        let head: String = uri.chars().take(40).collect();
        format!("{head}…")
    } else {
        uri.to_owned()
    }
}

/// The bytes of a `data:` URI.
fn data_uri(uri: &str) -> Result<Vec<u8>, String> {
    let (header, payload) = uri
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(','))
        .ok_or_else(|| "a data URI without a comma".to_owned())?;
    if !header.ends_with(";base64") {
        return Err("a data URI that is not base64".to_owned());
    }
    base64(payload).ok_or_else(|| "a data URI whose base64 is broken".to_owned())
}

/// Standard base64, padding optional. glTF writers use nothing else, and
/// the decoder is twenty lines against a dependency.
fn base64(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }
    let text = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    for chunk in text.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

/// A glTF file made into scene data: what a `RuntimeLoader` shows.
#[derive(Debug, Default)]
pub struct Asset {
    pub nodes: Vec<AssetNode>,
    /// The nodes the file's scene starts from.
    pub roots: Vec<usize>,
    pub meshes: Vec<AssetMesh>,
    pub materials: Vec<AssetMaterial>,
    /// The box the drawn meshes fill, in the space the roots are placed
    /// in; None when nothing is drawn.
    pub bounds: Option<(Vec3, Vec3)>,
    /// What was left out and why: a primitive of points, an image that
    /// would not decode.
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct AssetNode {
    pub name: Option<String>,
    pub transform: Transform,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Default)]
pub struct AssetMesh {
    pub primitives: Vec<AssetPrimitive>,
}

#[derive(Debug)]
pub struct AssetPrimitive {
    pub geometry: Arc<CustomGeometry>,
    /// Into [`Asset::materials`].
    pub material: usize,
}

/// A material as the file gives it: the factors, drawn today, and the
/// textures, decoded for a renderer that binds them.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetMaterial {
    pub name: Option<String>,
    pub material: PrincipledMaterial,
    pub textures: MaterialTextures,
}

/// The textures of a glTF metal-roughness material. Each is the image
/// decoded to RGBA with the UV set and the sampler the file asks for.
/// Whether a texture's numbers are sRGB (base colour, emissive) or linear
/// (the rest) is the glTF rule the renderer applies when it uploads them.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialTextures {
    pub base_color: Option<TextureRef>,
    /// Roughness in green, metalness in blue.
    pub metallic_roughness: Option<TextureRef>,
    /// A tangent-space normal map.
    pub normal: Option<TextureRef>,
    pub normal_scale: f32,
    /// Ambient occlusion in red.
    pub occlusion: Option<TextureRef>,
    pub occlusion_strength: f32,
    pub emissive: Option<TextureRef>,
}

impl Default for MaterialTextures {
    fn default() -> Self {
        MaterialTextures {
            base_color: None,
            metallic_roughness: None,
            normal: None,
            normal_scale: 1.0,
            occlusion: None,
            occlusion_strength: 1.0,
            emissive: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextureRef {
    /// Shared by every material that uses the same image.
    pub image: Arc<TextureImage>,
    /// Which `TEXCOORD_n` the texture is looked up with.
    pub uv_set: u32,
    pub sampler: Sampler,
}

/// A decoded picture: `width` × `height` RGBA, 8 bits a channel, rows top
/// first, not premultiplied.
#[derive(Clone, PartialEq, Eq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// What the renderer shares its upload by, for as long as the model
    /// holds the picture.
    id: u64,
}

impl TextureImage {
    fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        TextureImage {
            width,
            height,
            rgba,
            id: texture::fresh_pixels_id(),
        }
    }
}

/// The pixels stay after they are uploaded: the model is drawn again from
/// them by any view or renderer that shows it later, and reading the file
/// again is what the loader is there to spare.
impl TexturePixels for TextureImage {
    fn id(&self) -> u64 {
        self.id
    }

    fn size(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    fn encoding(&self) -> PixelEncoding {
        PixelEncoding::Srgb8Straight
    }

    fn opaque(&self) -> bool {
        self.rgba.chunks_exact(4).all(|p| p[3] == 255)
    }

    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.rgba);
    }
}

impl std::fmt::Debug for TextureImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextureImage({}×{})", self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    ClampToEdge,
    MirroredRepeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    pub mag: Filter,
    pub min: Filter,
    /// None when the texture has no mipmaps to blend.
    pub mipmap: Option<Filter>,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}

impl Default for Sampler {
    /// glTF leaves an unset filter to the viewer: trilinear, as most do.
    fn default() -> Self {
        Sampler {
            mag: Filter::Linear,
            min: Filter::Linear,
            mipmap: Some(Filter::Linear),
            wrap_s: Wrap::Repeat,
            wrap_t: Wrap::Repeat,
        }
    }
}

impl Asset {
    /// Adds the file's nodes under `parent`, each mesh primitive a model
    /// of its own beneath its node.
    pub fn add_to(&self, scene: &mut Scene, parent: NodeId) {
        let mut seen = vec![false; self.nodes.len()];
        for &root in &self.roots {
            self.add_node(scene, parent, root, &mut seen);
        }
    }

    fn add_node(&self, scene: &mut Scene, parent: NodeId, index: usize, seen: &mut [bool]) {
        // A node is in the tree once; `build` has already refused cycles,
        // and this keeps a hand-made Asset from looping too.
        let Some(node) = self.nodes.get(index) else {
            return;
        };
        if std::mem::replace(&mut seen[index], true) {
            return;
        }
        let id = scene.add(
            Some(parent),
            Node::new(NodeKind::Group).with_transform(node.transform),
        );
        if let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)) {
            for p in &mesh.primitives {
                let material = self
                    .materials
                    .get(p.material)
                    .map_or_else(|| Material::Principled(gltf_default()), scene_material);
                scene.add(
                    Some(id),
                    Node::new(NodeKind::Model(Model {
                        mesh: MeshSource::Custom(p.geometry.clone()),
                        material,
                    })),
                );
            }
        }
        for &child in &node.children {
            self.add_node(scene, id, child, seen);
        }
    }
}

/// The scene material a loaded material draws with: its factors, and its
/// textures as the material's maps.
///
/// glTF multiplies each factor by its texture, as QtQuick3D's maps do,
/// keeps roughness in green and metalness in blue, which are the maps'
/// default channels, and lays UVs out with the image's top at v = 0, as
/// the built-in meshes do. A texture read through a second UV set is left
/// out (its factor still draws): the renderer reads one.
pub fn scene_material(m: &AssetMaterial) -> Material {
    let mut material = m.material.clone();
    let t = &m.textures;
    let map = |r: &Option<TextureRef>| {
        r.as_ref()
            .filter(|r| r.uv_set == 0)
            .map(TextureRef::texture)
    };
    let metal_rough = map(&t.metallic_roughness);
    material.maps = Box::new(PrincipledMaps {
        base_color: map(&t.base_color),
        metalness: metal_rough.clone(),
        metalness_channel: Channel::B,
        roughness: metal_rough,
        roughness_channel: Channel::G,
        normal: map(&t.normal),
        normal_strength: t.normal_scale,
        emissive: map(&t.emissive),
        occlusion: map(&t.occlusion),
        occlusion_channel: Channel::R,
        occlusion_amount: t.occlusion_strength,
        ..PrincipledMaps::default()
    });
    Material::Principled(material)
}

impl TextureRef {
    /// The texture a material's map reads: the same picture, so one upload
    /// serves every material that uses it.
    fn texture(&self) -> Texture {
        let tiling = |w| match w {
            Wrap::Repeat => Tiling::Repeat,
            Wrap::ClampToEdge => Tiling::ClampToEdge,
            Wrap::MirroredRepeat => Tiling::MirroredRepeat,
        };
        let filter = |f| match f {
            Filter::Nearest => texture::Filter::Nearest,
            Filter::Linear => texture::Filter::Linear,
        };
        let s = &self.sampler;
        Texture {
            image: texture::TextureImage(self.image.clone()),
            sampling: Sampling {
                min: filter(s.min),
                mag: filter(s.mag),
                mip: s.mipmap.map_or(texture::Filter::None, filter),
                tiling_u: tiling(s.wrap_s),
                tiling_v: tiling(s.wrap_t),
                generate_mipmaps: s.mipmap.is_some(),
            },
            uv: texture::UvTransform::default(),
        }
    }
}

/// The material glTF gives a primitive that names none.
fn gltf_default() -> PrincipledMaterial {
    PrincipledMaterial {
        metalness: 1.0,
        roughness: 1.0,
        ..PrincipledMaterial::default()
    }
}

/// glTF's factors are linear; a scene's colour is sRGB, as QML's is.
fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    // The curve's float arithmetic gives 0.99999994 for 1, which would
    // make a pure colour from a file one step short of pure.
    if c == 1.0 {
        1.0
    } else if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

struct Builder<'a> {
    doc: &'a ::gltf::Document,
    buffers: &'a [Vec<u8>],
    fetch: &'a mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    images: HashMap<usize, Option<Arc<TextureImage>>>,
    warnings: Vec<String>,
}

impl Builder<'_> {
    fn asset(&mut self) -> Result<Asset, LoadError> {
        let doc = self.doc;
        let materials = doc.materials().map(|m| self.material(&m)).collect();
        let mut meshes = Vec::with_capacity(doc.meshes().len());
        for mesh in doc.meshes() {
            let mut out = AssetMesh::default();
            for p in mesh.primitives() {
                let what = format!("mesh {} primitive {}", mesh.index(), p.index());
                match self.primitive(&p) {
                    Ok(geometry) => out.primitives.push(AssetPrimitive {
                        geometry: Arc::new(geometry),
                        // The last slot is the default material, added below.
                        material: p.material().index().unwrap_or(doc.materials().len()),
                    }),
                    Err(why) => self.warnings.push(format!("{what} is left out: {why}")),
                }
            }
            meshes.push(out);
        }
        let mut asset = Asset {
            nodes: doc
                .nodes()
                .map(|n| AssetNode {
                    name: n.name().map(str::to_owned),
                    transform: transform(n.transform()),
                    mesh: n.mesh().map(|m| m.index()),
                    children: n.children().map(|c| c.index()).collect(),
                })
                .collect(),
            roots: self.roots(),
            meshes,
            materials,
            bounds: None,
            warnings: Vec::new(),
        };
        asset.materials.push(AssetMaterial {
            name: None,
            material: gltf_default(),
            textures: MaterialTextures::default(),
        });
        if asset.nodes.is_empty() {
            return Err(LoadError::Empty);
        }
        self.refuse_cycles(&asset)?;
        asset.bounds = bounds(&asset);
        asset.warnings = std::mem::take(&mut self.warnings);
        Ok(asset)
    }

    /// The default scene's nodes, else the first scene's, else every node
    /// no other node holds: a file may name no scene at all.
    fn roots(&self) -> Vec<usize> {
        let doc = self.doc;
        if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
            return scene.nodes().map(|n| n.index()).collect();
        }
        let mut held = vec![false; doc.nodes().len()];
        for n in doc.nodes() {
            for c in n.children() {
                held[c.index()] = true;
            }
        }
        (0..held.len()).filter(|i| !held[*i]).collect()
    }

    // glTF requires the nodes to be a forest; a file that loops would make
    // the scene recurse forever.
    fn refuse_cycles(&self, asset: &Asset) -> Result<(), LoadError> {
        let mut parent = vec![None; asset.nodes.len()];
        for (i, n) in asset.nodes.iter().enumerate() {
            for &c in &n.children {
                if parent[c].replace(i).is_some() {
                    return Err(LoadError::Parse(format!(
                        "node {c} is the child of more than one node"
                    )));
                }
            }
        }
        for start in 0..asset.nodes.len() {
            let mut at = start;
            for _ in 0..=asset.nodes.len() {
                match parent[at] {
                    Some(p) if p == start => {
                        return Err(LoadError::Parse(format!(
                            "node {start} is its own ancestor"
                        )));
                    }
                    Some(p) => at = p,
                    None => break,
                }
            }
        }
        Ok(())
    }

    fn material(&mut self, m: &::gltf::Material) -> AssetMaterial {
        let pbr = m.pbr_metallic_roughness();
        let [r, g, b, a] = pbr.base_color_factor();
        let [er, eg, eb] = m.emissive_factor();
        let material = PrincipledMaterial {
            base_color: Color {
                r: to_srgb(r),
                g: to_srgb(g),
                b: to_srgb(b),
                a: a.clamp(0.0, 1.0),
            },
            metalness: pbr.metallic_factor().clamp(0.0, 1.0),
            roughness: pbr.roughness_factor().clamp(0.0, 1.0),
            emissive: [er, eg, eb].map(|c| c.max(0.0)),
            opacity: 1.0,
            alpha_mode: match m.alpha_mode() {
                ::gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                ::gltf::material::AlphaMode::Mask => AlphaMode::Mask,
                ::gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            alpha_cutoff: m.alpha_cutoff().unwrap_or(0.5),
            cull_mode: if m.double_sided() {
                CullMode::None
            } else {
                CullMode::Back
            },
            lighting: if m.unlit() {
                Lighting::None
            } else {
                Lighting::Fragment
            },
            // The textures are kept beside the material, and made its maps
            // by `scene_material`.
            maps: Box::default(),
        };
        let normal = m.normal_texture();
        let occlusion = m.occlusion_texture();
        let textures = MaterialTextures {
            base_color: pbr
                .base_color_texture()
                .and_then(|t| self.texture(&t.texture(), t.tex_coord())),
            metallic_roughness: pbr
                .metallic_roughness_texture()
                .and_then(|t| self.texture(&t.texture(), t.tex_coord())),
            normal_scale: normal.as_ref().map_or(1.0, |n| n.scale()),
            normal: normal.and_then(|t| self.texture(&t.texture(), t.tex_coord())),
            occlusion_strength: occlusion.as_ref().map_or(1.0, |o| o.strength()),
            occlusion: occlusion.and_then(|t| self.texture(&t.texture(), t.tex_coord())),
            emissive: m
                .emissive_texture()
                .and_then(|t| self.texture(&t.texture(), t.tex_coord())),
        };
        AssetMaterial {
            name: m.name().map(str::to_owned),
            material,
            textures,
        }
    }

    fn texture(&mut self, t: &::gltf::Texture, uv_set: u32) -> Option<TextureRef> {
        let image = self.image(&t.source())?;
        if uv_set > 0 {
            self.warnings.push(format!(
                "texture {} reads TEXCOORD_{uv_set}, which is not drawn: its material keeps its factor",
                t.index()
            ));
        }
        let s = t.sampler();
        use ::gltf::texture::{MagFilter, MinFilter, WrappingMode};
        let wrap = |w| match w {
            WrappingMode::ClampToEdge => Wrap::ClampToEdge,
            WrappingMode::MirroredRepeat => Wrap::MirroredRepeat,
            WrappingMode::Repeat => Wrap::Repeat,
        };
        let (min, mipmap) = match s.min_filter() {
            None => (Filter::Linear, Some(Filter::Linear)),
            Some(MinFilter::Nearest) => (Filter::Nearest, None),
            Some(MinFilter::Linear) => (Filter::Linear, None),
            Some(MinFilter::NearestMipmapNearest) => (Filter::Nearest, Some(Filter::Nearest)),
            Some(MinFilter::LinearMipmapNearest) => (Filter::Linear, Some(Filter::Nearest)),
            Some(MinFilter::NearestMipmapLinear) => (Filter::Nearest, Some(Filter::Linear)),
            Some(MinFilter::LinearMipmapLinear) => (Filter::Linear, Some(Filter::Linear)),
        };
        Some(TextureRef {
            image,
            uv_set,
            sampler: Sampler {
                mag: match s.mag_filter() {
                    Some(MagFilter::Nearest) => Filter::Nearest,
                    Some(MagFilter::Linear) | None => Filter::Linear,
                },
                min,
                mipmap,
                wrap_s: wrap(s.wrap_s()),
                wrap_t: wrap(s.wrap_t()),
            },
        })
    }

    /// The image decoded, once however many textures use it.
    fn image(&mut self, image: &::gltf::Image) -> Option<Arc<TextureImage>> {
        if let Some(done) = self.images.get(&image.index()) {
            return done.clone();
        }
        let decoded = self.image_bytes(image).and_then(|b| decode_image(&b));
        let out = match decoded {
            Ok(i) => Some(Arc::new(i)),
            Err(why) => {
                self.warnings
                    .push(format!("image {} is left out: {why}", image.index()));
                None
            }
        };
        self.images.insert(image.index(), out.clone());
        out
    }

    fn image_bytes(&mut self, image: &::gltf::Image) -> Result<Vec<u8>, String> {
        match image.source() {
            ::gltf::image::Source::View { view, .. } => self.view(&view).map(<[u8]>::to_vec),
            ::gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => data_uri(uri),
            ::gltf::image::Source::Uri { uri, .. } => (self.fetch)(uri),
        }
    }

    /// A buffer view's bytes, checked to lie inside its buffer.
    fn view(&self, view: &::gltf::buffer::View) -> Result<&[u8], String> {
        let buffer = &self.buffers[view.buffer().index()];
        view.offset()
            .checked_add(view.length())
            .and_then(|end| buffer.get(view.offset()..end))
            .ok_or_else(|| format!("buffer view {} lies outside its buffer", view.index()))
    }

    fn primitive(&self, p: &::gltf::Primitive) -> Result<CustomGeometry, String> {
        use ::gltf::mesh::{Mode, Semantic as S};
        let primitive = match p.mode() {
            Mode::Triangles => PrimitiveType::Triangles,
            Mode::TriangleStrip => PrimitiveType::TriangleStrip,
            Mode::TriangleFan => PrimitiveType::TriangleFan,
            other => return Err(format!("{other:?} are not drawn")),
        };
        let positions = p.get(&S::Positions).ok_or("it has no positions")?;
        let count = positions.count();
        let mut columns: Vec<(Semantic, usize, Vec<f32>)> =
            vec![(Semantic::Position, 3, self.floats(&positions, 3)?)];
        for (semantic, glt, width) in [
            (Semantic::Normal, S::Normals, 3),
            (Semantic::TexCoord0, S::TexCoords(0), 2),
            (Semantic::TexCoord1, S::TexCoords(1), 2),
            (Semantic::Tangent, S::Tangents, 4),
        ] {
            if let Some(a) = p.get(&glt) {
                if a.count() != count {
                    return Err(format!("its {glt:?} and positions differ in number"));
                }
                columns.push((semantic, width, self.floats(&a, width)?));
            }
        }
        let stride_floats: usize = columns.iter().map(|c| c.1).sum();
        let mut vertices = Vec::with_capacity(count * stride_floats * 4);
        for v in 0..count {
            for (_, width, data) in &columns {
                for f in &data[v * width..(v + 1) * width] {
                    vertices.extend_from_slice(&f.to_le_bytes());
                }
            }
        }
        let mut g = CustomGeometry::new();
        g.set_vertex_data(vertices);
        g.set_stride(stride_floats * 4);
        let mut offset = 0;
        for (semantic, width, _) in &columns {
            g.add_attribute(*semantic, offset, ComponentType::F32);
            offset += width * 4;
        }
        if let Some(a) = p.indices() {
            let indices = self.indices(&a)?;
            if let Some(bad) = indices.iter().find(|i| **i as usize >= count) {
                return Err(format!("index {bad} is past its {count} vertices"));
            }
            g.set_index_data(indices.iter().flat_map(|i| i.to_le_bytes()).collect());
            g.add_attribute(Semantic::Index, 0, ComponentType::U32);
        }
        g.set_primitive_type(primitive);
        if let Some((min, max)) = accessor_box(&positions) {
            g.set_bounds(min, max);
        }
        g.update();
        Ok(g)
    }

    /// An accessor's elements as `width` floats each: 32-bit floats, or
    /// normalised unsigned integers (quantised texture coordinates).
    fn floats(&self, a: &::gltf::Accessor, width: usize) -> Result<Vec<f32>, String> {
        use ::gltf::accessor::DataType;
        if a.dimensions().multiplicity() != width {
            return Err(format!(
                "accessor {} has {} components, not {width}",
                a.index(),
                a.dimensions().multiplicity()
            ));
        }
        let (size, read): (usize, fn(&[u8]) -> f32) = match (a.data_type(), a.normalized()) {
            (DataType::F32, _) => (4, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            (DataType::U8, true) => (1, |b| f32::from(b[0]) / 255.0),
            (DataType::U16, true) => (2, |b| f32::from(u16::from_le_bytes([b[0], b[1]])) / 65535.0),
            (DataType::I8, true) => (1, |b| (f32::from(b[0] as i8) / 127.0).max(-1.0)),
            (DataType::I16, true) => (2, |b| {
                (f32::from(i16::from_le_bytes([b[0], b[1]])) / 32767.0).max(-1.0)
            }),
            (t, n) => {
                return Err(format!(
                    "accessor {} is {t:?}{}, which is not read yet",
                    a.index(),
                    if n { " normalised" } else { "" }
                ));
            }
        };
        let elements = self.elements(a, size * width)?;
        let mut out = Vec::with_capacity(a.count() * width);
        for e in elements {
            for c in 0..width {
                out.push(read(&e[c * size..]));
            }
        }
        Ok(out)
    }

    fn indices(&self, a: &::gltf::Accessor) -> Result<Vec<u32>, String> {
        use ::gltf::accessor::DataType;
        if a.dimensions().multiplicity() != 1 {
            return Err(format!("index accessor {} is not scalar", a.index()));
        }
        let (size, read): (usize, fn(&[u8]) -> u32) = match a.data_type() {
            DataType::U8 => (1, |b| u32::from(b[0])),
            DataType::U16 => (2, |b| u32::from(u16::from_le_bytes([b[0], b[1]]))),
            DataType::U32 => (4, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            t => return Err(format!("index accessor {} is {t:?}", a.index())),
        };
        Ok(self.elements(a, size)?.map(read).collect())
    }

    /// Each element's bytes, `size` of them, wherever the view's stride
    /// puts them; every element checked to lie inside the view first.
    fn elements<'b>(
        &'b self,
        a: &::gltf::Accessor,
        size: usize,
    ) -> Result<impl Iterator<Item = &'b [u8]> + 'b, String> {
        if a.sparse().is_some() {
            return Err(format!(
                "accessor {} is sparse, which is not read yet",
                a.index()
            ));
        }
        let view = a
            .view()
            .ok_or_else(|| format!("accessor {} has no buffer view", a.index()))?;
        let bytes = self.view(&view)?;
        let stride = view.stride().unwrap_or(size);
        if stride < size {
            return Err(format!(
                "buffer view {} has a stride under its element",
                view.index()
            ));
        }
        let count = a.count();
        let offset = a.offset();
        let end = count
            .checked_sub(1)
            .map_or(Some(offset), |last| {
                last.checked_mul(stride)
                    .and_then(|s| s.checked_add(offset))
                    .and_then(|s| s.checked_add(size))
            })
            .ok_or_else(|| format!("accessor {} is too large", a.index()))?;
        if end > bytes.len() {
            return Err(format!("accessor {} runs past its buffer view", a.index()));
        }
        Ok((0..count).map(move |i| &bytes[offset + i * stride..offset + i * stride + size]))
    }
}

/// The node's own transform. A matrix becomes translation, rotation and
/// scale, as a Node keeps them; a shear in it is lost.
fn transform(t: ::gltf::scene::Transform) -> Transform {
    let ([tx, ty, tz], [x, y, z, w], [sx, sy, sz]) = t.decomposed();
    let rotation = Quat { x, y, z, w };
    let ok = |v: f32| v.is_finite();
    Transform {
        position: Vec3::new(tx, ty, tz),
        rotation: if [x, y, z, w].into_iter().all(ok) {
            rotation
        } else {
            Quat::IDENTITY
        },
        scale: if [sx, sy, sz].into_iter().all(ok) {
            Vec3::new(sx, sy, sz)
        } else {
            Vec3::ZERO
        },
    }
}

/// The box glTF requires a position accessor to state.
fn accessor_box(a: &::gltf::Accessor) -> Option<(Vec3, Vec3)> {
    let v = |j: ::gltf::json::Value| -> Option<Vec3> {
        let n = j.as_array()?;
        let f = |i: usize| n.get(i)?.as_f64().map(|f| f as f32);
        Some(Vec3::new(f(0)?, f(1)?, f(2)?))
    };
    Some((v(a.min()?)?, v(a.max()?)?))
}

/// The box every drawn primitive fills, through the node transforms from
/// the roots down.
fn bounds(asset: &Asset) -> Option<(Vec3, Vec3)> {
    let mut out: Option<(Vec3, Vec3)> = None;
    let mut stack: Vec<(usize, Mat4)> = asset.roots.iter().map(|r| (*r, Mat4::IDENTITY)).collect();
    let mut seen = vec![false; asset.nodes.len()];
    while let Some((i, parent)) = stack.pop() {
        let Some(node) = asset.nodes.get(i) else {
            continue;
        };
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        let world = parent * node.transform.matrix();
        for p in node
            .mesh
            .and_then(|m| asset.meshes.get(m))
            .map_or(&[][..], |m| &m.primitives)
        {
            let Some((min, max)) = p.geometry.bounds() else {
                continue;
            };
            for k in 0..8 {
                let corner = Vec3::new(
                    if k & 1 == 0 { min.x } else { max.x },
                    if k & 2 == 0 { min.y } else { max.y },
                    if k & 4 == 0 { min.z } else { max.z },
                );
                let c = world.transform_point(corner);
                let (lo, hi) = out.unwrap_or((c, c));
                out = Some((
                    Vec3::new(lo.x.min(c.x), lo.y.min(c.y), lo.z.min(c.z)),
                    Vec3::new(hi.x.max(c.x), hi.y.max(c.y), hi.z.max(c.z)),
                ));
            }
        }
        stack.extend(node.children.iter().map(|c| (*c, world)));
    }
    out
}

/// A PNG or a JPEG, told apart by their first bytes rather than by a MIME
/// type files often get wrong.
fn decode_image(bytes: &[u8]) -> Result<TextureImage, String> {
    if bytes.starts_with(b"\x89PNG") {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)
    } else {
        Err("it is neither a PNG nor a JPEG".to_owned())
    }
}

fn too_large(w: u32, h: u32) -> Option<String> {
    (w == 0 || h == 0 || w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE)
        .then(|| format!("{w}×{h} is outside 1 to {MAX_IMAGE_SIDE} a side"))
}

fn decode_png(bytes: &[u8]) -> Result<TextureImage, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palettes become RGB and 16 bits become 8: four layouts are left.
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (reader.info().width, reader.info().height);
    if let Some(why) = too_large(w, h) {
        return Err(why);
    }
    let size = reader.output_buffer_size().ok_or("the PNG is too large")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        other => return Err(format!("unexpected PNG colour type {other:?}")),
    };
    Ok(TextureImage::new(info.width, info.height, rgba))
}

fn decode_jpeg(bytes: &[u8]) -> Result<TextureImage, String> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    dec.decode_headers().map_err(|e| e.to_string())?;
    let info = dec.info().ok_or("the JPEG has no header")?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    if let Some(why) = too_large(w, h) {
        return Err(why);
    }
    let rgba = dec.decode().map_err(|e| e.to_string())?;
    if rgba.len() != w as usize * h as usize * 4 {
        return Err("the JPEG decoded to the wrong size".to_owned());
    }
    Ok(TextureImage::new(w, h, rgba))
}

#[cfg(test)]
mod tests;
