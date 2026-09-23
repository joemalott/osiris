//! Lazily filled texture atlas. Images are decoded and packed the first time they are
//! drawn. Mirrored records share their source's pixels and flip their UVs.

use osiris_formats::{ImageLibrary, PackImage};

pub const PAGE_SIZE: u32 = 4096;
const PAD: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub flip: bool,
}

struct Shelf {
    y: u32,
    h: u32,
    x: u32,
}

pub(crate) struct Page {
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl Page {
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (pw, ph) = (w + PAD, h + PAD);
        // Best-fitting existing shelf: tall enough, not wastefully tall.
        let best = self
            .shelves
            .iter_mut()
            .filter(|s| s.h >= ph && s.h <= ph + ph / 2 + 4 && s.x + pw <= PAGE_SIZE)
            .min_by_key(|s| s.h);
        if let Some(s) = best {
            let pos = (s.x, s.y);
            s.x += pw;
            return Some(pos);
        }
        if self.next_y + ph > PAGE_SIZE || pw > PAGE_SIZE {
            return None;
        }
        let y = self.next_y;
        self.next_y += ph;
        self.shelves.push(Shelf { y, h: ph, x: pw });
        Some((0, y))
    }
}

pub(crate) struct Atlas {
    pub pages: Vec<Page>,
    entries: std::collections::HashMap<PackImage, Option<AtlasEntry>>,
}

impl Atlas {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            entries: Default::default(),
        }
    }

    /// Reserves a 4x4 white block on the first page, used to draw solid rectangles.
    pub fn reserve_white(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> AtlasEntry {
        let mut page = Self::new_page(device, layout, sampler);
        let (x, y) = page.alloc(4, 4).expect("empty page");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &page.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8; 4 * 16],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(4),
            },
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );
        self.pages.push(page);
        // Sample the centre texels only, so filtering never reaches the neighbours.
        AtlasEntry {
            page: 0,
            x: x as u16 + 1,
            y: y as u16 + 1,
            w: 2,
            h: 2,
            flip: false,
        }
    }

    fn new_page(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Page {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas page"),
            size: wgpu::Extent3d {
                width: PAGE_SIZE,
                height: PAGE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas page"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Page {
            texture,
            bind_group,
            shelves: Vec::new(),
            next_y: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get(
        &mut self,
        lib: &ImageLibrary,
        img: PackImage,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Option<AtlasEntry> {
        if let Some(e) = self.entries.get(&img) {
            return *e;
        }
        let rec = lib.record(img);
        let entry = if rec.mirror_offset != 0 {
            let src = PackImage {
                pack: img.pack,
                index: (img.index as i32 + rec.mirror_offset) as u16,
            };
            self.get(lib, src, device, queue, layout, sampler)
                .map(|e| AtlasEntry { flip: !e.flip, ..e })
        } else {
            self.upload(lib, img, device, queue, layout, sampler)
        };
        self.entries.insert(img, entry);
        entry
    }

    fn upload(
        &mut self,
        lib: &ImageLibrary,
        img: PackImage,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Option<AtlasEntry> {
        let sprite = match lib.sg3(img.pack).ok_or_else(|| osiris_formats::Error::Invalid("pack missing".into())).and_then(|s| s.decode(img.index as usize)) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("image {img:?}: {e}");
                return None;
            }
        };
        if sprite.width == 0 || sprite.height == 0 {
            return None;
        }
        let (w, h) = (sprite.width, sprite.height);
        let mut slot = self
            .pages
            .iter_mut()
            .enumerate()
            .find_map(|(i, p)| p.alloc(w, h).map(|(x, y)| (i, x, y)));
        if slot.is_none() {
            let mut page = Self::new_page(device, layout, sampler);
            let (x, y) = page.alloc(w, h)?;
            self.pages.push(page);
            slot = Some((self.pages.len() - 1, x, y));
        }
        let (page, x, y) = slot?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.pages[page].texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            sprite.pixels.as_flattened(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        Some(AtlasEntry {
            page: page as u16,
            x: x as u16,
            y: y as u16,
            w: w as u16,
            h: h as u16,
            flip: false,
        })
    }
}
