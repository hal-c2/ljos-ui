//! Wavefront OBJ files read into the [`Asset`] a glTF file makes, so
//! whatever shows, bounds, picks or copies one takes the other.
//!
//! Like [`gltf`](crate::gltf), reading is plain work on bytes in steps a
//! host can fetch between: [`parse`] reads the `.obj` and says which `.mtl`
//! files it names, [`Document::read_libraries`] reads those and says which
//! pictures they name, and [`Document::build`] makes the asset. A hostile
//! file is an error or a warning, never a panic: every index is checked.
//!
//! The format is older than physically based materials and leaves much
//! open, so each choice here is one the renderer's material can show:
//! - `o` and `g` each start a model named after them (a name that comes
//!   again goes on the same model), one primitive for each `usemtl`;
//! - `v` runs up an image in OBJ and down it here, as glTF's does;
//! - a face without normals takes its own, or the mean of its smoothing
//!   group's about each corner where `s` names one;
//! - `Kd` and `Ke` are linear light, as exporters that have a scene's
//!   colours write them and as glTF's factors are;
//! - `Ns` is taken to a roughness by [`roughness_of`], an approximation;
//! - `illum 0`, a constant colour, is unlit; other `illum`s are ignored;
//! - `map_Bump` is read as a normal map, not a height map, which is what
//!   exporters of real-time assets mean by it.

use std::collections::HashMap;
use std::sync::Arc;

use crate::fetch::{Fetch, Fetched, fetch_all, join};
use crate::geometry::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
use crate::gltf::{
    self, Asset, AssetMaterial, AssetMesh, AssetNode, AssetPrimitive, LoadError, MaterialTextures,
    Sampler, TextureImage, TextureRef, Wrap,
};
use crate::scene::{Color, Lighting, PrincipledMaterial, Transform};
use crate::texture::{Channel, TexturePixels};

/// Which reader a model file's name asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reader {
    Gltf,
    Obj,
}

/// The reader for the file at `url`, by its extension in any case; `None`
/// for a file neither reads.
pub fn reader_for(url: &str) -> Option<Reader> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, extension) = name.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "obj" => Some(Reader::Obj),
        "gltf" | "glb" => Some(Reader::Gltf),
        _ => None,
    }
}

/// Reads the model at `url`, whichever kind its extension says, with the
/// files it names. See [`gltf::load`] for who this is for.
pub fn load_any<F>(
    fetch: &F,
    cx: &F::Cx,
    url: &str,
    done: impl FnOnce(&F::Cx, Result<Asset, LoadError>) + 'static,
) where
    F: Fetch + Clone + 'static,
{
    match reader_for(url) {
        Some(Reader::Gltf) => gltf::load(fetch, cx, url, done),
        Some(Reader::Obj) => load(fetch, cx, url, done),
        None => done(
            cx,
            Err(LoadError::File(format!(
                "{url}: only .obj, .gltf and .glb files are read"
            ))),
        ),
    }
}

/// Reads the `.obj` at `url`, its `.mtl` files and their pictures through
/// `fetch`, and calls `done` with the asset.
pub fn load<F>(
    fetch: &F,
    cx: &F::Cx,
    url: &str,
    done: impl FnOnce(&F::Cx, Result<Asset, LoadError>) + 'static,
) where
    F: Fetch + Clone + 'static,
{
    let (others, base) = (fetch.clone(), url.to_owned());
    fetch.fetch(
        cx,
        url,
        Box::new(move |cx, got| {
            let mut doc = match got.map_err(LoadError::File).and_then(|b| parse(&b)) {
                Ok(doc) => doc,
                Err(err) => return done(cx, Err(err)),
            };
            let pictures = others.clone();
            let libraries = doc.libraries();
            fetch_all(
                &others,
                cx,
                &base.clone(),
                libraries,
                move |cx, mut files| {
                    doc.read_libraries(&base, &mut |uri| files.take(uri));
                    // Already whole URLs: each resolved against its `.mtl`.
                    let urls = doc.image_urls();
                    fetch_all(&pictures, cx, "", urls, move |cx, mut files| {
                        done(cx, doc.build(&mut |uri| files.take(uri)));
                    });
                },
            );
        }),
    );
}

const NONE: u32 = u32::MAX;

/// One corner of a face, 0-based; [`NONE`] where the file gives none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Corner {
    position: u32,
    uv: u32,
    normal: u32,
}

/// The triangles of one material in one group.
#[derive(Debug, Default)]
struct Run {
    material: Option<String>,
    /// Three corners a triangle.
    corners: Vec<Corner>,
    /// Each triangle's smoothing group; 0 is none.
    smoothing: Vec<u32>,
}

#[derive(Debug, Default)]
struct Group {
    name: Option<String>,
    runs: Vec<Run>,
}

/// A map as a `.mtl` names it.
#[derive(Clone, Debug)]
struct MapFile {
    url: String,
    clamp: bool,
    /// `-bm`: how strongly a normal map counts.
    strength: f32,
}

#[derive(Debug)]
struct Mtl {
    material: PrincipledMaterial,
    base_color: Option<MapFile>,
    emissive: Option<MapFile>,
    normal: Option<MapFile>,
    opacity: Option<MapFile>,
}

/// A parsed `.obj` whose other files have not been read yet.
#[derive(Debug, Default)]
pub struct Document {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
    groups: Vec<Group>,
    libraries: Vec<String>,
    materials: HashMap<String, Mtl>,
    warnings: Vec<String>,
}

fn not_obj(line: usize, what: &str) -> LoadError {
    LoadError::Obj(format!("line {line}: {what}"))
}

/// The first `N` of `parts` as finite numbers.
fn numbers<'a, const N: usize>(parts: impl Iterator<Item = &'a str>) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    let mut parts = parts;
    for o in &mut out {
        *o = parts.next()?.parse().ok().filter(|f: &f32| f.is_finite())?;
    }
    Some(out)
}

/// A 1-based index, or one counted back from the end of what is listed so
/// far, as 0-based. Past the end is checked once the whole file is read: a
/// face may name a vertex that comes later.
fn index(text: &str, listed: usize) -> Option<u32> {
    let i: i64 = text.parse().ok()?;
    let at = match i {
        0 => return None,
        i if i > 0 => i - 1,
        i => i64::try_from(listed).ok()? + i,
    };
    u32::try_from(at).ok().filter(|a| *a != NONE)
}

/// Reads a `.obj` file.
pub fn parse(bytes: &[u8]) -> Result<Document, LoadError> {
    if bytes.contains(&0) {
        return Err(LoadError::Obj("it is not text".to_owned()));
    }
    let text = String::from_utf8_lossy(bytes);
    let mut doc = Document::default();
    let mut names: HashMap<Option<String>, usize> = HashMap::new();
    let mut group = None;
    let mut material: Option<String> = None;
    let mut smoothing = 0;
    let mut dropped = 0usize;
    let mut unread = (0usize, 0usize);
    let mut joined = String::new();
    for (number, raw) in text.lines().enumerate() {
        let number = number + 1;
        // A line that ends in a backslash goes on in the next.
        if let Some(start) = raw.trim_end().strip_suffix('\\') {
            joined.push_str(start);
            joined.push(' ');
            continue;
        }
        let whole;
        let line = if joined.is_empty() {
            raw
        } else {
            joined.push_str(raw);
            whole = std::mem::take(&mut joined);
            &whole
        };
        let line = line.split('#').next().unwrap_or_default().trim();
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        match keyword {
            "v" => doc
                .positions
                .push(numbers::<3>(parts).ok_or_else(|| not_obj(number, "a broken vertex"))?),
            "vn" => doc
                .normals
                .push(numbers::<3>(parts).ok_or_else(|| not_obj(number, "a broken normal"))?),
            // Its third number, for a 3D texture, is not read.
            "vt" => {
                let mut parts = parts;
                let u = parts.next();
                let v = parts.next().unwrap_or("0");
                let [u, v] = numbers::<2>([u.unwrap_or(""), v].into_iter())
                    .ok_or_else(|| not_obj(number, "a broken texture coordinate"))?;
                doc.uvs.push([u, 1.0 - v]);
            }
            "f" => {
                let corners: Option<Vec<Corner>> = parts.map(|token| corner(token, &doc)).collect();
                match corners {
                    Some(corners) if corners.len() >= 3 => {
                        let at = *group.get_or_insert_with(|| {
                            *names.entry(None).or_insert_with(|| {
                                doc.groups.push(Group::default());
                                doc.groups.len() - 1
                            })
                        });
                        let runs = &mut doc.groups[at].runs;
                        let run = match runs.iter().position(|r| r.material == material) {
                            Some(r) => &mut runs[r],
                            None => {
                                runs.push(Run {
                                    material: material.clone(),
                                    ..Run::default()
                                });
                                runs.last_mut().expect("just pushed")
                            }
                        };
                        // A polygon is a fan from its first corner.
                        for pair in corners[1..].windows(2) {
                            run.corners.extend([corners[0], pair[0], pair[1]]);
                            run.smoothing.push(smoothing);
                        }
                    }
                    _ => dropped += 1,
                }
            }
            "o" | "g" => {
                let name = line[keyword.len()..].trim();
                let name = (!name.is_empty()).then(|| name.to_owned());
                let at = *names.entry(name.clone()).or_insert_with(|| {
                    doc.groups.push(Group {
                        name,
                        runs: Vec::new(),
                    });
                    doc.groups.len() - 1
                });
                group = Some(at);
            }
            "usemtl" => {
                let name = line[keyword.len()..].trim();
                material = (!name.is_empty()).then(|| name.to_owned());
            }
            "s" => {
                smoothing = match parts.next() {
                    Some("off") | None => 0,
                    Some(n) => n.parse().unwrap_or(1),
                };
            }
            "mtllib" => {
                let names: Vec<&str> = parts.collect();
                let several = names
                    .iter()
                    .all(|n| n.to_ascii_lowercase().ends_with(".mtl"));
                if several {
                    doc.libraries.extend(names.iter().map(|n| file_uri(n)));
                } else {
                    // One name with spaces in it.
                    doc.libraries.push(file_uri(line[keyword.len()..].trim()));
                }
            }
            "l" => unread.0 += 1,
            "p" => unread.1 += 1,
            // Curves and surfaces, and what no reader agrees on.
            _ => {}
        }
    }
    doc.drop_past_the_end(&mut dropped);
    if dropped > 0 {
        doc.warnings.push(format!(
            "{dropped} faces are left out: too few corners or a broken index"
        ));
    }
    if unread != (0, 0) {
        doc.warnings.push(format!(
            "{} lines and {} points are not drawn",
            unread.0, unread.1
        ));
    }
    doc.groups.iter_mut().for_each(|g| {
        g.runs.retain(|r| !r.corners.is_empty());
    });
    doc.groups.retain(|g| !g.runs.is_empty());
    if doc.groups.is_empty() {
        return Err(LoadError::Obj("it has no faces".to_owned()));
    }
    Ok(doc)
}

/// `v`, `v/vt`, `v//vn` or `v/vt/vn`.
fn corner(token: &str, doc: &Document) -> Option<Corner> {
    let mut parts = token.split('/');
    let position = index(parts.next()?, doc.positions.len())?;
    let optional = |part: Option<&str>, listed| match part {
        None | Some("") => Some(NONE),
        Some(text) => index(text, listed),
    };
    let uv = optional(parts.next(), doc.uvs.len())?;
    let normal = optional(parts.next(), doc.normals.len())?;
    Some(Corner {
        position,
        uv,
        normal,
    })
}

/// A name as a `.obj` or `.mtl` writes it, a path and not a URL, as a URI
/// reference [`join`] can resolve. A Windows path from the machine that
/// wrote the file means nothing here: its last part is looked for beside
/// the file.
fn file_uri(name: &str) -> String {
    if name.contains("://") {
        return name.to_owned();
    }
    let name = name.replace('\\', "/");
    let drive = name.as_bytes().get(1) == Some(&b':');
    let name = if drive {
        name.rsplit('/').next().unwrap_or_default()
    } else {
        &name
    };
    name.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
}

/// A shininess to a roughness: Walter et al.'s match of a Blinn-Phong
/// exponent to a microfacet width `sqrt(2 / (Ns + 2))`, and the width is
/// the square of a roughness. It is an approximation: `Ns` belongs to a
/// different model of light, and only the highlight's width carries over.
fn roughness_of(ns: f32) -> f32 {
    (2.0 / (ns.max(0.0) + 2.0)).powf(0.25).clamp(0.0, 1.0)
}

/// What a material or a face with no material is drawn with: dull white.
fn default_material() -> PrincipledMaterial {
    PrincipledMaterial {
        roughness: 1.0,
        ..PrincipledMaterial::default()
    }
}

/// A map statement: its options, then the file's name, which may have
/// spaces in it.
fn map_file(rest: &str, library: &str, warnings: &mut Vec<String>) -> Option<MapFile> {
    let mut tokens = rest.split_whitespace().peekable();
    let (mut clamp, mut strength) = (false, 1.0);
    let mut placed = false;
    while let Some(option) = tokens.peek().copied().filter(|t| t.starts_with('-')) {
        tokens.next();
        match option {
            "-clamp" => clamp = tokens.next() == Some("on"),
            "-bm" => strength = tokens.next().and_then(|t| t.parse().ok()).unwrap_or(1.0),
            "-blendu" | "-blendv" | "-boost" | "-texres" | "-imfchan" | "-cc" | "-type" => {
                tokens.next();
            }
            "-mm" => {
                tokens.next();
                tokens.next();
            }
            // One to three numbers.
            "-o" | "-s" | "-t" => {
                placed = true;
                for _ in 0..3 {
                    if tokens.peek().is_none_or(|t| t.parse::<f32>().is_err()) {
                        break;
                    }
                    tokens.next();
                }
            }
            _ => {}
        }
    }
    let name = tokens.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    if placed {
        warnings.push(format!("{name} is drawn without its -o, -s or -t"));
    }
    Some(MapFile {
        url: join(library, &file_uri(&name)),
        clamp,
        strength,
    })
}

/// The materials of one `.mtl`, whose URL its maps are relative to.
fn read_library(text: &str, library: &str, doc: &mut Document) {
    let mut current: Option<(String, Mtl)> = None;
    let mut warnings = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let Some((keyword, rest)) = line
            .split_once(char::is_whitespace)
            .map(|(k, r)| (k, r.trim()))
        else {
            continue;
        };
        let keyword = keyword.to_ascii_lowercase();
        if keyword == "newmtl" {
            if let Some((name, mtl)) = current.take() {
                doc.materials.insert(name, mtl);
            }
            current = Some((
                rest.to_owned(),
                Mtl {
                    material: default_material(),
                    base_color: None,
                    emissive: None,
                    normal: None,
                    opacity: None,
                },
            ));
            continue;
        }
        let Some((_, mtl)) = &mut current else {
            continue;
        };
        let m = &mut mtl.material;
        // `Kd r` alone is a grey; `Kd spectral ...` is not read.
        let rgb = || {
            let mut v = rest.split_whitespace().map(|t| t.parse::<f32>());
            let r = v.next()?.ok()?;
            match (v.next(), v.next()) {
                (Some(Ok(g)), Some(Ok(b))) => Some([r, g, b]),
                (None, _) => Some([r; 3]),
                _ => None,
            }
            .filter(|c| c.iter().all(|f| f.is_finite()))
        };
        // `d -halo 0.5` keeps its number last.
        let last = rest
            .split_whitespace()
            .last()
            .and_then(|t| t.parse::<f32>().ok())
            .filter(|f| f.is_finite());
        match keyword.as_str() {
            "kd" => {
                if let Some([r, g, b]) = rgb() {
                    m.base_color = Color {
                        r: gltf::to_srgb(r),
                        g: gltf::to_srgb(g),
                        b: gltf::to_srgb(b),
                        a: 1.0,
                    };
                }
            }
            "ke" => {
                if let Some(c) = rgb() {
                    m.emissive = c.map(|v| v.max(0.0));
                }
            }
            "d" => {
                if let Some(d) = last {
                    m.opacity = d.clamp(0.0, 1.0);
                }
            }
            "tr" => {
                if let Some(t) = last {
                    m.opacity = 1.0 - t.clamp(0.0, 1.0);
                }
            }
            "ns" => {
                if let Some(ns) = last {
                    m.roughness = roughness_of(ns);
                }
            }
            "illum" => {
                if rest == "0" {
                    m.lighting = Lighting::None;
                }
            }
            "map_kd" => mtl.base_color = map_file(rest, library, &mut warnings),
            "map_ke" => mtl.emissive = map_file(rest, library, &mut warnings),
            "map_d" => mtl.opacity = map_file(rest, library, &mut warnings),
            "map_bump" | "bump" | "norm" => mtl.normal = map_file(rest, library, &mut warnings),
            _ => {}
        }
    }
    if let Some((name, mtl)) = current {
        doc.materials.insert(name, mtl);
    }
    doc.warnings.append(&mut warnings);
}

impl Document {
    /// The `.mtl` files the document names, as URI references relative to
    /// it.
    pub fn libraries(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for library in &self.libraries {
            if !out.contains(library) {
                out.push(library.clone());
            }
        }
        out
    }

    /// Reads each of [`libraries`](Self::libraries) from `fetch`, given the
    /// document's own URL `base`. One that cannot be had is a warning, and
    /// the faces that name its materials keep the default.
    pub fn read_libraries(&mut self, base: &str, fetch: &mut dyn FnMut(&str) -> Fetched) {
        for uri in self.libraries() {
            match fetch(&uri) {
                Ok(bytes) => {
                    let library = join(base, &uri);
                    read_library(&String::from_utf8_lossy(&bytes), &library, self);
                }
                Err(why) => self
                    .warnings
                    .push(format!("material library {uri} is left out: {why}")),
            }
        }
    }

    fn used_materials(&self) -> impl Iterator<Item = &Mtl> {
        self.groups
            .iter()
            .flat_map(|g| &g.runs)
            .filter_map(|r| self.materials.get(r.material.as_ref()?))
    }

    /// The whole URLs of the pictures the materials in use name, for
    /// [`build`](Self::build). A map is relative to its `.mtl`, which is
    /// beside the `.obj` in all but a few files.
    pub fn image_urls(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for mtl in self.used_materials() {
            for map in [&mtl.base_color, &mtl.emissive, &mtl.normal, &mtl.opacity]
                .into_iter()
                .flatten()
            {
                if !out.contains(&map.url) {
                    out.push(map.url.clone());
                }
            }
        }
        out
    }

    /// Makes the asset, asking `fetch` for each of
    /// [`image_urls`](Self::image_urls). A picture that cannot be had or
    /// decoded is a warning, and its material keeps its factors.
    pub fn build(&self, fetch: &mut dyn FnMut(&str) -> Fetched) -> Result<Asset, LoadError> {
        let mut warnings = self.warnings.clone();
        let mut images = HashMap::new();
        let mut materials = Vec::new();
        let mut slots: HashMap<&str, usize> = HashMap::new();
        for run in self.groups.iter().flat_map(|g| &g.runs) {
            let Some(name) = run.material.as_deref() else {
                continue;
            };
            if slots.contains_key(name) {
                continue;
            }
            match self.materials.get(name) {
                Some(mtl) => {
                    slots.insert(name, materials.len());
                    materials.push(asset_material(name, mtl, fetch, &mut images, &mut warnings));
                }
                None => warnings.push(format!("material {name} is not in the file's libraries")),
            }
        }
        // The last slot is what a face with no material takes.
        let fallback = materials.len();
        materials.push(AssetMaterial {
            name: None,
            material: default_material(),
            textures: MaterialTextures::default(),
        });
        let smooth = self.smooth_normals();
        let mut nodes = Vec::with_capacity(self.groups.len());
        let mut meshes = Vec::with_capacity(self.groups.len());
        let mut roots = Vec::with_capacity(self.groups.len());
        for group in &self.groups {
            let primitives = group
                .runs
                .iter()
                .map(|run| AssetPrimitive {
                    geometry: Arc::new(self.geometry(run, &smooth)),
                    material: run
                        .material
                        .as_deref()
                        .and_then(|m| slots.get(m).copied())
                        .unwrap_or(fallback),
                })
                .collect();
            roots.push(nodes.len());
            nodes.push(AssetNode {
                name: group.name.clone(),
                transform: Transform::default(),
                mesh: Some(meshes.len()),
                skin: None,
                children: Vec::new(),
            });
            meshes.push(AssetMesh { primitives });
        }
        let mut asset = Asset {
            nodes,
            roots,
            meshes,
            materials,
            skins: Vec::new(),
            bounds: None,
            warnings,
        };
        asset.bounds = gltf::bounds(&asset);
        Ok(asset)
    }

    // Every index is in range from here on.
    fn drop_past_the_end(&mut self, dropped: &mut usize) {
        let (p, t, n) = (self.positions.len(), self.uvs.len(), self.normals.len());
        let good = |c: &Corner| {
            (c.position as usize) < p
                && (c.uv == NONE || (c.uv as usize) < t)
                && (c.normal == NONE || (c.normal as usize) < n)
        };
        for run in self.groups.iter_mut().flat_map(|g| &mut g.runs) {
            if run.corners.iter().all(good) {
                continue;
            }
            let mut smoothing = Vec::with_capacity(run.smoothing.len());
            let mut corners = Vec::with_capacity(run.corners.len());
            for (triangle, s) in run.corners.as_chunks::<3>().0.iter().zip(&run.smoothing) {
                if triangle.iter().all(good) {
                    corners.extend_from_slice(triangle);
                    smoothing.push(*s);
                } else {
                    *dropped += 1;
                }
            }
            run.corners = corners;
            run.smoothing = smoothing;
        }
    }

    /// For each position in each smoothing group, the normals of its faces
    /// that have none of their own added up, weighed by the faces' areas.
    fn smooth_normals(&self) -> HashMap<(u32, u32), [f32; 3]> {
        let mut out = HashMap::new();
        for run in self.groups.iter().flat_map(|g| &g.runs) {
            for (triangle, &s) in run.corners.as_chunks::<3>().0.iter().zip(&run.smoothing) {
                if s == 0 {
                    continue;
                }
                let face = self.face_normal(triangle);
                for c in triangle.iter().filter(|c| c.normal == NONE) {
                    let sum: &mut [f32; 3] = out.entry((c.position, s)).or_default();
                    for (to, f) in sum.iter_mut().zip(face) {
                        *to += f;
                    }
                }
            }
        }
        out
    }

    /// As long as twice the triangle's area.
    fn face_normal(&self, t: &[Corner; 3]) -> [f32; 3] {
        let [a, b, c] = t.map(|c| self.positions[c.position as usize]);
        let (u, v) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    }

    /// One run's triangles as a mesh, interleaved as a glTF mesh is. A
    /// corner is shared where every number it has is the same.
    fn geometry(&self, run: &Run, smooth: &HashMap<(u32, u32), [f32; 3]>) -> CustomGeometry {
        let has_uvs = run.corners.iter().any(|c| c.uv != NONE);
        let floats = if has_uvs { 8 } else { 6 };
        // Which group a normal is worked out in: a flat face's own triangle
        // is its group, so that no other face shares its corners.
        let mut seen: HashMap<(Corner, u64), u32> = HashMap::new();
        let mut vertices = Vec::new();
        let mut positions = Vec::new();
        let mut indices = Vec::with_capacity(run.corners.len() * 4);
        for (i, (triangle, &s)) in run
            .corners
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&run.smoothing)
            .enumerate()
        {
            let flat = if s == 0 {
                unit(self.face_normal(triangle))
            } else {
                [0.0; 3]
            };
            for c in triangle {
                let made = c.normal == NONE;
                let tag = match (made, s) {
                    (false, _) => 0,
                    (true, 0) => (1 << 32) | i as u64,
                    (true, s) => u64::from(s),
                };
                let next = (vertices.len() / (floats * 4)) as u32;
                let at = *seen.entry((*c, tag)).or_insert(next);
                indices.extend_from_slice(&at.to_le_bytes());
                if at != next {
                    continue;
                }
                let position = self.positions[c.position as usize];
                let normal = if !made {
                    unit(self.normals[c.normal as usize])
                } else if s == 0 {
                    flat
                } else {
                    unit(smooth.get(&(c.position, s)).copied().unwrap_or_default())
                };
                positions.extend_from_slice(&position);
                let uv = if c.uv == NONE {
                    [0.0; 2]
                } else {
                    self.uvs[c.uv as usize]
                };
                let columns = position.iter().chain(&normal).chain(&uv);
                for f in columns.take(floats) {
                    vertices.extend_from_slice(&f.to_le_bytes());
                }
            }
        }
        let mut g = CustomGeometry::new();
        g.set_vertex_data(vertices);
        g.set_stride(floats * 4);
        g.add_attribute(Semantic::Position, 0, ComponentType::F32);
        g.add_attribute(Semantic::Normal, 12, ComponentType::F32);
        if has_uvs {
            g.add_attribute(Semantic::TexCoord0, 24, ComponentType::F32);
        }
        g.set_index_data(indices);
        g.add_attribute(Semantic::Index, 0, ComponentType::U32);
        g.set_primitive_type(PrimitiveType::Triangles);
        if let Some((min, max)) = gltf::position_box(&positions) {
            g.set_bounds(min, max);
        }
        g.update();
        g
    }
}

/// `v` of length one; a face with no area faces the default way.
fn unit(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 && length.is_finite() {
        v.map(|c| c / length)
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn asset_material(
    name: &str,
    mtl: &Mtl,
    fetch: &mut dyn FnMut(&str) -> Fetched,
    images: &mut HashMap<String, Option<Arc<TextureImage>>>,
    warnings: &mut Vec<String>,
) -> AssetMaterial {
    let mut texture = |map: &Option<MapFile>| -> Option<TextureRef> {
        let map = map.as_ref()?;
        let image = images
            .entry(map.url.clone())
            .or_insert_with(|| {
                let decoded = fetch(&map.url).and_then(|b| gltf::decode_image(&b));
                match decoded {
                    Ok(image) => Some(Arc::new(image)),
                    Err(why) => {
                        warnings.push(format!("image {} is left out: {why}", map.url));
                        None
                    }
                }
            })
            .clone()?;
        let wrap = if map.clamp {
            Wrap::ClampToEdge
        } else {
            Wrap::Repeat
        };
        Some(TextureRef {
            image,
            uv_set: 0,
            sampler: Sampler {
                wrap_s: wrap,
                wrap_t: wrap,
                ..Sampler::default()
            },
        })
    };
    let base_color = texture(&mtl.base_color);
    let emissive = texture(&mtl.emissive);
    let normal = texture(&mtl.normal);
    let opacity = texture(&mtl.opacity);
    let mut material = mtl.material.clone();
    // A map multiplies its factor, and a material that gives a glow only by
    // its picture has no factor to multiply: all of the picture shows.
    if emissive.is_some() && material.emissive == [0.0; 3] {
        material.emissive = [1.0; 3];
    }
    // `map_d` is one value a pixel: a grey picture's, or the alpha of one
    // that has any, as a colour map used again for its cut-outs has.
    let opacity_channel = match &opacity {
        Some(o) if o.image.opaque() => Channel::R,
        _ => Channel::A,
    };
    AssetMaterial {
        name: Some(name.to_owned()),
        material,
        textures: MaterialTextures {
            base_color,
            emissive,
            normal_scale: mtl.normal.as_ref().map_or(1.0, |n| n.strength),
            normal,
            opacity,
            opacity_channel,
            ..MaterialTextures::default()
        },
    }
}

#[cfg(test)]
mod tests;
