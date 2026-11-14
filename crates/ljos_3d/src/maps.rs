//! A material's maps on the device: each image uploaded once, shared by
//! every material and view that samples it, and let go when no scene holds
//! the image any more.
//!
//! Pixels are uploaded straight, not premultiplied, as a material reads a
//! map's channels one by one. Colour maps (base, diffuse, emissive) are
//! sRGB textures, so the sampler hands the shader linear light; the maps of
//! single values and the normal map are read as they are stored. Mipmaps
//! are made on the CPU, once, while the pixels are in memory anyway: no
//! pipeline is kept for it, and WebGL2 makes them the same way.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use crate::scene::Material;
use crate::texture::{Filter, PixelEncoding, Sampling, Texture, TexturePixels, Tiling};

/// The maps a material binds, in the order of their bindings: base colour
/// (or diffuse), metalness (or a `DefaultMaterial`'s specular), roughness,
/// normal, emissive, occlusion, opacity, transmission, height, clearcoat, its roughness and its normals. Each
/// sampler's binding is its texture's plus [`MAPS`].
pub(crate) const MAPS: usize = 12;
pub(crate) const BASE: usize = 0;
pub(crate) const METALNESS: usize = 1;
pub(crate) const ROUGHNESS: usize = 2;
pub(crate) const NORMAL: usize = 3;
pub(crate) const EMISSIVE: usize = 4;
pub(crate) const OCCLUSION: usize = 5;
pub(crate) const OPACITY: usize = 6;
pub(crate) const TRANSMISSION: usize = 7;
pub(crate) const HEIGHT: usize = 8;
pub(crate) const CLEARCOAT: usize = 9;
pub(crate) const CLEARCOAT_ROUGHNESS: usize = 10;
pub(crate) const CLEARCOAT_NORMAL: usize = 11;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TextureKey {
    image: u64,
    srgb: bool,
    mipmapped: bool,
}

struct Uploaded {
    image: Weak<dyn TexturePixels>,
    view: wgpu::TextureView,
    bytes: u64,
}

/// What one material binds: per map, the texture and how it is read.
pub(crate) type GroupKey = [Option<(TextureKey, Sampling)>; MAPS];

/// The maps of `material` in binding order, each with whether it is a
/// colour, and so sRGB.
pub(crate) fn slots(material: &Material) -> [Option<(&Texture, bool)>; MAPS] {
    let mut out = [None; MAPS];
    match material {
        Material::Default(m) => {
            out[BASE] = m.diffuse_map.as_ref().map(|t| (t, true));
            out[METALNESS] = m.maps.specular.as_ref().map(|t| (t, true));
            out[ROUGHNESS] = m.maps.roughness.as_ref().map(|t| (t, false));
            out[NORMAL] = m.maps.normal.as_ref().map(|t| (t, false));
            out[EMISSIVE] = m.maps.emissive.as_ref().map(|t| (t, true));
            out[OPACITY] = m.maps.opacity.as_ref().map(|t| (t, false));
        }
        Material::Principled(m) => {
            let maps = &m.maps;
            // Fractions are light already, to Qt only as a PrincipledMaterial's
            // colour: everywhere else they are read as a screen shows them.
            out[BASE] = maps
                .base_color
                .as_ref()
                .map(|t| (t, t.image.encoding() != PixelEncoding::Rgbe));
            out[METALNESS] = maps.metalness.as_ref().map(|t| (t, false));
            out[ROUGHNESS] = maps.roughness.as_ref().map(|t| (t, false));
            out[NORMAL] = maps.normal.as_ref().map(|t| (t, false));
            out[EMISSIVE] = maps.emissive.as_ref().map(|t| (t, true));
            out[OCCLUSION] = maps.occlusion.as_ref().map(|t| (t, false));
            out[OPACITY] = maps.opacity.as_ref().map(|t| (t, false));
            out[TRANSMISSION] = maps.transmission.as_ref().map(|t| (t, false));
            out[HEIGHT] = maps.height.as_ref().map(|t| (t, false));
            out[CLEARCOAT] = maps.clearcoat.as_ref().map(|t| (t, false));
            out[CLEARCOAT_ROUGHNESS] = maps.clearcoat_roughness.as_ref().map(|t| (t, false));
            out[CLEARCOAT_NORMAL] = maps.clearcoat_normal.as_ref().map(|t| (t, false));
        }
        // A custom material's textures are its own samplers, bound apart.
        Material::Custom(_) => {}
    }
    out
}

pub(crate) struct Maps {
    pub(crate) layout: wgpu::BindGroupLayout,
    /// Where a material has no map it reads white, which multiplies
    /// nothing away; the normal map is skipped by the shader instead. Made
    /// with the first frame, as the renderer is made without a queue.
    white: Option<wgpu::TextureView>,
    no_sampler: wgpu::Sampler,
    textures: HashMap<TextureKey, Uploaded>,
    samplers: HashMap<Sampling, wgpu::Sampler>,
    groups: HashMap<GroupKey, wgpu::BindGroup>,
    max_side: u32,
}

impl Maps {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let mut entries = Vec::with_capacity(MAPS * 2);
        for i in 0..MAPS as u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        for i in 0..MAPS as u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: MAPS as u32 + i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ljos_3d maps"),
            entries: &entries,
        });
        let no_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ljos_3d no map"),
            ..Default::default()
        });
        Maps {
            layout,
            white: None,
            no_sampler,
            textures: HashMap::new(),
            samplers: HashMap::new(),
            groups: HashMap::new(),
            max_side: device.limits().max_texture_dimension_2d,
        }
    }

    /// Uploads what `material` samples that is not on the device yet, and
    /// makes its bind group; the key [`Maps::group`] takes.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        material: &Material,
    ) -> GroupKey {
        self.white(device, queue);
        let mut key: GroupKey = [None; MAPS];
        for (slot, map) in key.iter_mut().zip(slots(material)) {
            let Some((texture, srgb)) = map else {
                continue;
            };
            let k = TextureKey {
                image: texture.image.id(),
                srgb,
                mipmapped: texture.sampling.mipmapped(),
            };
            if !self.textures.contains_key(&k) {
                let Some(uploaded) = self.upload(device, queue, texture, k) else {
                    continue;
                };
                self.textures.insert(k, uploaded);
            }
            self.samplers
                .entry(texture.sampling)
                .or_insert_with(|| sampler(device, texture.sampling));
            *slot = Some((k, texture.sampling));
        }
        if !self.groups.contains_key(&key) {
            let group = self.make_group(device, &key);
            self.groups.insert(key, group);
        }
        key
    }

    /// One texture a custom material or an effect samples by its own
    /// sampler: uploaded unless it is there, and read as it is stored, as
    /// a snippet decides itself what its channels hold. `None` for an
    /// image with no pixels.
    pub(crate) fn single(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &Texture,
    ) -> Option<(wgpu::TextureView, wgpu::Sampler)> {
        let k = TextureKey {
            image: texture.image.id(),
            srgb: false,
            mipmapped: texture.sampling.mipmapped(),
        };
        if !self.textures.contains_key(&k) {
            let uploaded = self.upload(device, queue, texture, k)?;
            self.textures.insert(k, uploaded);
        }
        let sampler = self
            .samplers
            .entry(texture.sampling)
            .or_insert_with(|| sampler(device, texture.sampling));
        Some((self.textures[&k].view.clone(), sampler.clone()))
    }

    /// How a Buffer of an effect is read.
    pub(crate) fn effect_sampler(
        &mut self,
        device: &wgpu::Device,
        filter: Filter,
        tiling: Tiling,
    ) -> wgpu::Sampler {
        let sampling = Sampling {
            min: filter,
            mag: filter,
            mip: Filter::None,
            tiling_u: tiling,
            tiling_v: tiling,
            generate_mipmaps: false,
        };
        self.samplers
            .entry(sampling)
            .or_insert_with(|| sampler(device, sampling))
            .clone()
    }

    pub(crate) fn group(&self, key: &GroupKey) -> &wgpu::BindGroup {
        &self.groups[key]
    }

    fn white(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.white.is_none() {
            let level = (1, 1, vec![255; 4]);
            self.white = Some(upload(device, queue, "ljos_3d no map", false, &[level]));
        }
    }

    /// The group of a material with no maps.
    pub(crate) fn plain(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> GroupKey {
        self.white(device, queue);
        let key = [None; MAPS];
        if !self.groups.contains_key(&key) {
            let group = self.make_group(device, &key);
            self.groups.insert(key, group);
        }
        key
    }

    /// Lets go of the textures whose image no scene holds any more, and of
    /// the groups that bound them.
    pub(crate) fn sweep(&mut self) {
        let before = self.textures.len();
        self.textures.retain(|_, t| t.image.strong_count() > 0);
        if self.textures.len() != before {
            let textures = &self.textures;
            self.groups
                .retain(|k, _| k.iter().flatten().all(|(t, _)| textures.contains_key(t)));
        }
    }

    /// Bytes the uploaded maps take on the device.
    pub(crate) fn bytes(&self) -> u64 {
        self.textures.values().map(|t| t.bytes).sum()
    }

    fn make_group(&self, device: &wgpu::Device, key: &GroupKey) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(MAPS * 2);
        for (i, slot) in key.iter().enumerate() {
            let white = self.white.as_ref().expect("made before any group");
            let view = slot.map_or(white, |(t, _)| &self.textures[&t].view);
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        for (i, slot) in key.iter().enumerate() {
            let s = slot.map_or(&self.no_sampler, |(_, s)| &self.samplers[&s]);
            entries.push(wgpu::BindGroupEntry {
                binding: (MAPS + i) as u32,
                resource: wgpu::BindingResource::Sampler(s),
            });
        }
        crate::counted::create_bind_group(
            device,
            &wgpu::BindGroupDescriptor {
                label: Some("ljos_3d maps"),
                layout: &self.layout,
                entries: &entries,
            },
        )
    }

    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &Texture,
        key: TextureKey,
    ) -> Option<Uploaded> {
        let image = &texture.image;
        let mut level = straight_rgba8(&**image)?;
        // Larger than the device takes (WebGL2 promises 2048): halved
        // until it fits, as a mipmap would be.
        while level.0.max(level.1) > self.max_side {
            level = halve(&level, key.srgb);
        }
        let mut levels = vec![level];
        if key.mipmapped {
            while let Some(last) = levels.last()
                && last.0.max(last.1) > 1
            {
                let next = halve(last, key.srgb);
                levels.push(next);
            }
        }
        let bytes = levels.iter().map(|l| l.2.len() as u64).sum();
        let view = upload(device, queue, "ljos_3d map", key.srgb, &levels);
        image.uploaded();
        Some(Uploaded {
            image: Arc::downgrade(&image.0),
            view,
            bytes,
        })
    }
}

/// One level of a texture: width, height and its RGBA8 pixels.
pub(crate) type Level = (u32, u32, Vec<u8>);

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    srgb: bool,
    levels: &[Level],
) -> wgpu::TextureView {
    let (w, h, _) = levels[0];
    let texture = crate::counted::create_texture(
        device,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
    );
    for (mip, (w, h, data)) in levels.iter().enumerate() {
        crate::counted::write_texture(
            queue,
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(*h),
            },
            wgpu::Extent3d {
                width: *w,
                height: *h,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn sampler(device: &wgpu::Device, s: Sampling) -> wgpu::Sampler {
    let address = |t: Tiling| match t {
        Tiling::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        Tiling::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        Tiling::Repeat => wgpu::AddressMode::Repeat,
    };
    // None is no filter for minification or magnification: the nearest
    // pixel, which is what reading without one gives.
    let filter = |f: Filter| match f {
        Filter::Linear => wgpu::FilterMode::Linear,
        Filter::None | Filter::Nearest => wgpu::FilterMode::Nearest,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("ljos_3d map"),
        address_mode_u: address(s.tiling_u),
        address_mode_v: address(s.tiling_v),
        mag_filter: filter(s.mag),
        min_filter: filter(s.min),
        mipmap_filter: match s.mip {
            Filter::Linear => wgpu::MipmapFilterMode::Linear,
            Filter::None | Filter::Nearest => wgpu::MipmapFilterMode::Nearest,
        },
        ..Default::default()
    })
}

/// The pixels of `image` as straight RGBA8. `None` when it holds none.
fn straight_rgba8(image: &dyn TexturePixels) -> Option<Level> {
    let [w, h] = image.size();
    let len = w as usize * h as usize * 4;
    let mut data = Vec::new();
    image.with_pixels(&mut |p| {
        if p.len() == len {
            data = p.to_vec();
        }
    });
    if data.is_empty() || len == 0 {
        return None;
    }
    match image.encoding() {
        PixelEncoding::Srgb8Straight => {}
        PixelEncoding::Srgb8Premultiplied => {
            for p in data.as_chunks_mut::<4>().0 {
                let a = u32::from(p[3]);
                if a > 0 && a < 255 {
                    for c in &mut p[..3] {
                        *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
        }
        // Light beyond 1 is clamped: a material's map holds a colour. The
        // numbers go as they are; which materials read them as a screen
        // shows them is `slots`' to say.
        PixelEncoding::Rgbe => {
            for p in data.as_chunks_mut::<4>().0 {
                let rgb = rgbe_to_linear(*p);
                for (c, v) in p[..3].iter_mut().zip(rgb) {
                    *c = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                p[3] = 255;
            }
        }
    }
    Some((w, h, data))
}

/// Radiance's shared exponent to linear light.
pub(crate) fn rgbe_to_linear(p: [u8; 4]) -> [f32; 3] {
    if p[3] == 0 {
        return [0.0; 3];
    }
    let f = 2f32.powi(i32::from(p[3]) - 136);
    [
        f32::from(p[0]) * f,
        f32::from(p[1]) * f,
        f32::from(p[2]) * f,
    ]
}

pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

// Only the tests' reference mean goes back from light to bytes this way;
// `halve` uses the edges in `SrgbTables` instead.
#[cfg(test)]
pub(crate) fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The next mipmap level: each pixel the mean of up to four. Colours are
/// averaged in linear light and by their alpha, so that neither a dark
/// fringe nor a clear pixel's colour bleeds in.
///
/// It runs on the thread that draws, for every level of every mipmapped
/// map (two 2048-pixel maps make 2.8 million pixels), so it is written to
/// be cheap: whole numbers where the colours are numbers, and tables where
/// they are sRGB, never a power or a rounding call per pixel.
fn halve(level: &Level, srgb: bool) -> Level {
    if srgb {
        let tables = srgb_tables();
        halve_with(level, |four| srgb_mean(tables, four))
    } else {
        halve_with(level, number_mean)
    }
}

/// `level` halved, each new pixel what `mean` makes of the four beneath it
/// (the last row and column repeated where the size is odd).
fn halve_with(&(w, h, ref data): &Level, mean: impl Fn([[u8; 4]; 4]) -> [u8; 4]) -> Level {
    let (w, h) = (w as usize, h as usize);
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let (pixels, _) = data.as_chunks::<4>();
    let row = |y: usize| &pixels[y.min(h - 1) * w..][..w];
    let mut out = vec![0u8; nw * nh * 4];
    let (written, _) = out.as_chunks_mut::<4>();
    for (y, line) in written.chunks_exact_mut(nw).enumerate() {
        let (top, bottom) = (row(2 * y), row(2 * y + 1));
        for (x, pixel) in line.iter_mut().enumerate() {
            let (left, right) = (2 * x, (2 * x + 1).min(w - 1));
            *pixel = mean([top[left], top[right], bottom[left], bottom[right]]);
        }
    }
    (nw as u32, nh as u32, out)
}

/// The mean of four pixels whose colours are numbers, weighted by alpha
/// and rounded half up, exactly: Σ(c·a) / Σa in whole numbers.
fn number_mean(four: [[u8; 4]; 4]) -> [u8; 4] {
    let alpha: u32 = four.iter().map(|p| u32::from(p[3])).sum();
    let mut out = [0, 0, 0, ((alpha + 2) / 4) as u8];
    if alpha == 4 * 255 {
        // Opaque, as most maps are: the weights are all the same.
        for (c, o) in out[..3].iter_mut().enumerate() {
            let sum: u32 = four.iter().map(|p| u32::from(p[c])).sum();
            *o = ((sum + 2) / 4) as u8;
        }
    } else if alpha > 0 {
        for (c, o) in out[..3].iter_mut().enumerate() {
            let sum: u32 = four.iter().map(|p| u32::from(p[c]) * u32::from(p[3])).sum();
            // round(sum / alpha), at most 255 since every c is.
            *o = ((2 * sum + alpha) / (2 * alpha)) as u8;
        }
    }
    out
}

/// The mean of four sRGB pixels, in linear light and weighted by alpha.
fn srgb_mean(SrgbTables { linear, edges }: &SrgbTables, four: [[u8; 4]; 4]) -> [u8; 4] {
    let alpha: u32 = four.iter().map(|p| u32::from(p[3])).sum();
    let mut out = [0, 0, 0, ((alpha + 2) / 4) as u8];
    if alpha > 0 {
        let weight = 1.0 / alpha as f32;
        for (c, o) in out[..3].iter_mut().enumerate() {
            let sum: f32 = four
                .iter()
                .map(|p| linear[usize::from(p[c])] * f32::from(p[3]))
                .sum();
            // The sRGB byte whose span of light holds the mean: as many
            // as the edges at or below it.
            *o = edges.partition_point(|&e| e <= sum * weight) as u8;
        }
    }
    out
}

struct SrgbTables {
    /// The light each sRGB byte stands for.
    linear: [f32; 256],
    /// The light half way between byte k and byte k + 1, where rounding
    /// to the nearer byte changes from one to the other.
    edges: [f32; 255],
}

fn srgb_tables() -> &'static SrgbTables {
    static TABLES: std::sync::OnceLock<SrgbTables> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| SrgbTables {
        linear: std::array::from_fn(|b| srgb_to_linear(b as f32 / 255.0)),
        edges: std::array::from_fn(|k| srgb_to_linear((k as f32 + 0.5) / 255.0)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mipmap_of_red_beside_clear_stays_red() {
        let level = (2, 1, vec![255, 0, 0, 255, 0, 0, 255, 0]);
        let (w, h, px) = halve(&level, true);
        assert_eq!((w, h), (1, 1));
        assert_eq!(px, [255, 0, 0, 128]);
    }

    #[test]
    fn colours_average_in_linear_light() {
        let level = (2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]);
        let (_, _, px) = halve(&level, true);
        // Half of white's light is sRGB 188, not 128.
        assert_eq!(px[0], 188);
        let (_, _, data) = halve(&level, false);
        assert_eq!(data[0], 128);
    }

    /// What `halve` made before it was made cheap, in floating point with
    /// a power and a rounding per channel: the measure it is held to.
    fn halve_reference((w, h, data): &Level, srgb: bool) -> Level {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut out = vec![0u8; nw as usize * nh as usize * 4];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0.0f32; 3];
                let mut alpha = 0.0f32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                    let i = (sy as usize * *w as usize + sx as usize) * 4;
                    let a = f32::from(data[i + 3]) / 255.0;
                    for c in 0..3 {
                        let v = f32::from(data[i + c]) / 255.0;
                        sum[c] += if srgb { srgb_to_linear(v) } else { v } * a;
                    }
                    alpha += a;
                }
                let o = (y as usize * nw as usize + x as usize) * 4;
                for c in 0..3 {
                    let v = if alpha > 0.0 { sum[c] / alpha } else { 0.0 };
                    let v = if srgb { linear_to_srgb(v) } else { v };
                    out[o + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                out[o + 3] = (alpha / 4.0 * 255.0).round() as u8;
            }
        }
        (nw, nh, out)
    }

    /// Pixels of every kind: noise, opaque and clear runs, and every byte
    /// beside every other, at sizes even and odd.
    fn levels() -> Vec<Level> {
        let mut seed = 0x9e37_79b9u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut levels = Vec::new();
        for (w, h) in [(64, 64), (33, 17), (1, 9), (7, 1), (2, 2)] {
            let noise = (0..w * h * 4).map(|_| next() as u8).collect();
            levels.push((w, h, noise));
            let opaque = (0..w * h * 4)
                .map(|i| if i % 4 == 3 { 255 } else { next() as u8 })
                .collect();
            levels.push((w, h, opaque));
        }
        let ramp = (0..256 * 256)
            .flat_map(|i| {
                let (a, b) = ((i % 256) as u8, (i / 256) as u8);
                [a, b, a ^ b, if i % 3 == 0 { 255 } else { b }]
            })
            .collect();
        levels.push((256, 256, ramp));
        levels
    }

    /// The exact mean of each four number pixels: Σ(c·a) / Σa and Σa / 4,
    /// rounded half up. A double holds them well enough for that: a mean
    /// that is not a half lies at least 1/2040 from one.
    fn halve_exact((w, h, data): &Level) -> Level {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut out = Vec::with_capacity(nw as usize * nh as usize * 4);
        let half_up = |v: f64| (v + 0.5).floor() as u8;
        for y in 0..nh {
            for x in 0..nw {
                let four = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| {
                    let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                    let i = (sy as usize * *w as usize + sx as usize) * 4;
                    [0, 1, 2, 3].map(|c| f64::from(data[i + c]))
                });
                let alpha: f64 = four.iter().map(|p| p[3]).sum();
                for c in 0..3 {
                    let sum: f64 = four.iter().map(|p| p[c] * p[3]).sum();
                    out.push(if alpha > 0.0 { half_up(sum / alpha) } else { 0 });
                }
                out.push(half_up(alpha / 4.0));
            }
        }
        (nw, nh, out)
    }

    #[test]
    fn whole_number_means_are_the_exact_ones() {
        for level in levels() {
            let fast = halve(&level, false);
            let exact = halve_exact(&level);
            assert_eq!((fast.0, fast.1), (exact.0, exact.1));
            for (i, (f, e)) in fast.2.iter().zip(&exact.2).enumerate() {
                assert_eq!(f, e, "{}x{} byte {i}", fast.0, fast.1);
            }
        }
    }

    #[test]
    fn whole_number_means_are_the_floating_point_ones() {
        for level in levels() {
            let (w, h, fast) = halve(&level, false);
            let (rw, rh, slow) = halve_reference(&level, false);
            assert_eq!((w, h), (rw, rh));
            // The floating point mean can fall a hair short of a half and
            // round down where the exact one rounds up; nothing else differs.
            for (i, (f, s)) in fast.iter().zip(&slow).enumerate() {
                assert!(f.abs_diff(*s) <= 1, "{w}x{h} byte {i}: {f} against {s}");
            }
        }
    }

    #[test]
    fn table_means_in_linear_light_are_the_powered_ones() {
        for level in levels() {
            let (w, h, fast) = halve(&level, true);
            let (_, _, slow) = halve_reference(&level, true);
            for (i, (f, s)) in fast.iter().zip(&slow).enumerate() {
                assert!(f.abs_diff(*s) <= 1, "{w}x{h} byte {i}: {f} against {s}");
            }
        }
    }

    #[test]
    fn rgbe_reads_as_radiance_writes_it() {
        assert_eq!(rgbe_to_linear([128, 128, 128, 128]), [0.5; 3]);
        assert_eq!(rgbe_to_linear([1, 2, 3, 0]), [0.0; 3]);
    }
}
