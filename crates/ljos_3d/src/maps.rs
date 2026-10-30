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
/// (or diffuse), metalness, roughness, normal, emissive, occlusion and
/// opacity. Each sampler's binding is its texture's plus [`MAPS`].
pub(crate) const MAPS: usize = 7;
pub(crate) const BASE: usize = 0;
pub(crate) const METALNESS: usize = 1;
pub(crate) const ROUGHNESS: usize = 2;
pub(crate) const NORMAL: usize = 3;
pub(crate) const EMISSIVE: usize = 4;
pub(crate) const OCCLUSION: usize = 5;
pub(crate) const OPACITY: usize = 6;

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
        Material::Default(m) => out[BASE] = m.diffuse_map.as_ref().map(|t| (t, true)),
        Material::Principled(m) => {
            let maps = &m.maps;
            out[BASE] = maps.base_color.as_ref().map(|t| (t, true));
            out[METALNESS] = maps.metalness.as_ref().map(|t| (t, false));
            out[ROUGHNESS] = maps.roughness.as_ref().map(|t| (t, false));
            out[NORMAL] = maps.normal.as_ref().map(|t| (t, false));
            out[EMISSIVE] = maps.emissive.as_ref().map(|t| (t, true));
            out[OCCLUSION] = maps.occlusion.as_ref().map(|t| (t, false));
            out[OPACITY] = maps.opacity.as_ref().map(|t| (t, false));
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
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ljos_3d maps"),
            layout: &self.layout,
            entries: &entries,
        })
    }

    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &Texture,
        key: TextureKey,
    ) -> Option<Uploaded> {
        let image = &texture.image;
        let mut level = straight_rgba8(&**image, key.srgb)?;
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
type Level = (u32, u32, Vec<u8>);

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    srgb: bool,
    levels: &[Level],
) -> wgpu::TextureView {
    let (w, h, _) = levels[0];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
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
    });
    for (mip, (w, h, data)) in levels.iter().enumerate() {
        queue.write_texture(
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

/// The pixels of `image` as straight RGBA8: sRGB for a colour map, linear
/// values for the others. `None` when it holds none.
fn straight_rgba8(image: &dyn TexturePixels, srgb: bool) -> Option<Level> {
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
        // Light beyond 1 is clamped: a material's map holds a colour.
        PixelEncoding::Rgbe => {
            for p in data.as_chunks_mut::<4>().0 {
                let rgb = rgbe_to_linear(*p);
                for (c, v) in p[..3].iter_mut().zip(rgb) {
                    let v = v.clamp(0.0, 1.0);
                    *c = (if srgb { linear_to_srgb(v) } else { v } * 255.0).round() as u8;
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
fn halve((w, h, data): &Level, srgb: bool) -> Level {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let lut: Vec<f32> = (0..=255u8)
        .map(|b| {
            let v = f32::from(b) / 255.0;
            if srgb { srgb_to_linear(v) } else { v }
        })
        .collect();
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0.0f32; 3];
            let (mut alpha, mut n) = (0.0f32, 0.0f32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                let i = (sy as usize * *w as usize + sx as usize) * 4;
                let a = f32::from(data[i + 3]) / 255.0;
                for c in 0..3 {
                    sum[c] += lut[usize::from(data[i + c])] * a;
                }
                alpha += a;
                n += 1.0;
            }
            let o = (y as usize * nw as usize + x as usize) * 4;
            for c in 0..3 {
                let v = if alpha > 0.0 { sum[c] / alpha } else { 0.0 };
                let v = if srgb { linear_to_srgb(v) } else { v };
                out[o + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out[o + 3] = (alpha / n * 255.0).round() as u8;
        }
    }
    (nw, nh, out)
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

    #[test]
    fn rgbe_reads_as_radiance_writes_it() {
        assert_eq!(rgbe_to_linear([128, 128, 128, 128]), [0.5; 3]);
        assert_eq!(rgbe_to_linear([1, 2, 3, 0]), [0.0; 3]);
    }
}
