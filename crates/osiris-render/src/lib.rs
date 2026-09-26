//! Sprite renderer. Callers queue sprites for a frame in painter's order; the renderer
//! turns them into instanced quads, split into batches only where the atlas page changes.

mod atlas;

use atlas::{Atlas, AtlasEntry, PAGE_SIZE};
use bytemuck::{Pod, Zeroable};
use osiris_formats::{ImageLibrary, ImageRecord};

pub use atlas::AtlasEntry as Entry;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    screen: [f32; 2],
    camera: [f32; 2],
    zoom: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    pos: [f32; 2],
    size: [f32; 2],
    uv0: [f32; 2],
    uv1: [f32; 2],
    color: [f32; 4],
    flags: u32,
}

/// Which texture a [`Batch`] samples from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageSource {
    Atlas(u16),
    Dynamic(u16),
}

struct Batch {
    source: PageSource,
    /// Drawn with the multiplying pipeline ([`Paint::Filter`]).
    multiply: bool,
    clip: Option<[f32; 4]>,
    start: u32,
    end: u32,
}

/// A caller-uploaded RGBA image drawn as a single quad, outside the atlas (e.g. a
/// minimap that's rebuilt in a CPU buffer whenever the map changes). See
/// [`Renderer::upload_dynamic`].
struct DynamicTex {
    key: u32,
    w: u32,
    h: u32,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// Handle to a texture uploaded with [`Renderer::upload_dynamic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicHandle(u16);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// World pixel shown at the top-left corner of the screen.
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    World,
    Screen,
}

/// How an image is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paint {
    /// Texture colours multiplied by the tint.
    #[default]
    Normal,
    /// Only the texture's shape is kept, filled with the tint colour.
    Silhouette,
    /// Texture colours with each 5-6-5 channel ANDed with the bits of a 16-bit
    /// colour, as the original tints a building's placement ghost (0x1fe3 keeps
    /// only green).
    Masked(u16),
    /// Only the texture's shape is kept, and what is already drawn under it keeps
    /// only the bits of a 16-bit 5-6-5 colour, as the original marks placement tiles.
    /// A channel the mask cuts to its low bits is darkened to what those bits hold
    /// on average (a multiply; the original ANDs the screen's pixels).
    Filter(u16),
    /// Texture colours, with the shape grown by a texel or two into its transparent
    /// edge. The city view draws the ground this way under the ground at zooms that
    /// aren't a whole number of device pixels to a texel: there each tile's diamond
    /// edge is sampled on its own, and the texels either side of the seam can both
    /// come out transparent, leaving dots of the clear colour along the tile edges.
    Spread,
}

impl Paint {
    /// The multiply factor per channel that stands in for ANDing with `mask`.
    fn filter_tint(mask: u16) -> [f32; 4] {
        let keep = |bits: u16, max: u16| if bits == max { 1.0 } else { bits as f32 / max as f32 / 2.0 };
        [keep(mask >> 11, 31), keep(mask >> 5 & 63, 63), keep(mask & 31, 31), 1.0]
    }
}

pub const WHITE: [f32; 4] = [1.0; 4];

/// How many texels past its edge an image drawn with [`Paint::Spread`] reaches (the
/// shader's `SPREAD`).
const SPREAD: f32 = 2.0;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    /// Multiplies what is drawn by the fragment colour.
    multiply_pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    page_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas: Atlas,
    white: AtlasEntry,
    dynamic: Vec<DynamicTex>,
    instances: Vec<Instance>,
    batches: Vec<Batch>,
    instance_buffer: wgpu::Buffer,
    pub camera: Camera,
    pub screen: [f32; 2],
    /// Physical pixels per logical pixel of `screen`.
    pub scale: f32,
    /// Screen-space rectangle (x, y, w, h) that subsequent draws are clipped to.
    clip: Option<[f32; 4]>,
    /// While set, screen-space draws and clips are given in a frame of their own that
    /// lands at this offset and scale (a screen laid out at 1024x768, stretched).
    pub screen_frame: Option<([f32; 2], f32)>,
    /// While set, images are filtered smoothly (bilinear) rather than by nearest
    /// pixel: for art stretched by a fraction, where nearest pixels double some rows
    /// and columns and not others.
    pub smooth: bool,
    pub library: ImageLibrary,
    /// The pack image of every core image id (below `EXTRA_BASE`), worked out once:
    /// `pack << 16 | index`, or `NO_IMAGE`.
    resolved: Vec<u32>,
    /// Atlas entries by core image id, once looked up (the outer `None`: not yet).
    by_id: Vec<Option<Option<AtlasEntry>>>,
}

const NO_IMAGE: u32 = u32::MAX;

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        library: ImageLibrary,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let page_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas page"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite"),
            bind_group_layouts: &[Some(&globals_layout), Some(&page_layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2, 4 => Float32x4, 5 => Uint32
        ];
        let make_pipeline = |blend: wgpu::BlendState| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let pipeline = make_pipeline(wgpu::BlendState::ALPHA_BLENDING);
        let multiply_pipeline = make_pipeline(wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Dst, dst_factor: wgpu::BlendFactor::Zero, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Nearest,
            // Nearest when shrunk too: blending would mix tiles' transparent edges in
            // and show a seam between every tile.
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let instance_buffer = Self::make_instance_buffer(&device, 1 << 16);
        let mut atlas = Atlas::new();
        let white = atlas.reserve_white(&device, &queue, &page_layout, &sampler);
        let resolved = (0..osiris_formats::images::EXTRA_BASE).map(|id| library.resolve(id).map_or(NO_IMAGE, |p| (p.pack as u32) << 16 | p.index as u32)).collect();
        Self {
            device,
            queue,
            pipeline,
            multiply_pipeline,
            globals,
            globals_bind,
            page_layout,
            sampler,
            atlas,
            white,
            dynamic: Vec::new(),
            instances: Vec::new(),
            batches: Vec::new(),
            instance_buffer,
            camera: Camera::default(),
            screen: [1.0, 1.0],
            scale: 1.0,
            clip: None,
            screen_frame: None,
            smooth: false,
            library,
            resolved,
            by_id: Vec::new(),
        }
    }

    fn make_instance_buffer(device: &wgpu::Device, count: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (count * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The pack image global image `id` names.
    fn resolve(&self, id: u32) -> Option<osiris_formats::PackImage> {
        match self.resolved.get(id as usize) {
            Some(&NO_IMAGE) => None,
            Some(&v) => Some(osiris_formats::PackImage { pack: (v >> 16) as u16, index: v as u16 }),
            None => self.library.resolve(id),
        }
    }

    /// Atlas entry for global image `id`, uploading it on first use.
    pub fn entry(&mut self, id: u32) -> Option<AtlasEntry> {
        let i = id as usize;
        if let Some(Some(e)) = self.by_id.get(i) {
            return *e;
        }
        let e = self.lookup(id);
        if i < self.resolved.len() {
            if self.by_id.len() <= i {
                self.by_id.resize(self.resolved.len(), None);
            }
            self.by_id[i] = Some(e);
        }
        e
    }

    fn lookup(&mut self, id: u32) -> Option<AtlasEntry> {
        let img = self.resolve(id)?;
        self.atlas.get(
            &self.library,
            img,
            &self.device,
            &self.queue,
            &self.page_layout,
            &self.sampler,
        )
    }

    pub fn record(&self, id: u32) -> Option<&ImageRecord> {
        Some(self.library.record(self.resolve(id)?))
    }

    fn push(
        &mut self,
        e: AtlasEntry,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        space: Space,
        flip: bool,
    ) {
        self.push_painted(e, pos, size, color, space, flip, Paint::Normal);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_painted(
        &mut self,
        e: AtlasEntry,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        space: Space,
        flip: bool,
        paint: Paint,
    ) {
        let s = 1.0 / PAGE_SIZE as f32;
        let (mut u0, mut u1) = (e.x as f32 * s, (e.x + e.w) as f32 * s);
        if e.flip != flip {
            std::mem::swap(&mut u0, &mut u1);
        }
        let mut uv0 = [u0, e.y as f32 * s];
        let mut uv1 = [u1, (e.y + e.h) as f32 * s];
        let (mut pos, mut size) = (pos, size);
        if paint == Paint::Spread {
            // The quad reaches SPREAD texels past the image on every side, so the
            // spread can cover pixels just outside it; the shader keeps its sampling
            // to the image itself.
            for i in 0..2 {
                let du = (uv1[i] - uv0[i]) / size[i];
                uv0[i] -= du * SPREAD;
                uv1[i] += du * SPREAD;
                pos[i] -= SPREAD;
                size[i] += 2.0 * SPREAD;
            }
        }
        self.push_raw(PageSource::Atlas(e.page), uv0, uv1, pos, size, color, space, paint);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_raw(
        &mut self,
        source: PageSource,
        uv0: [f32; 2],
        uv1: [f32; 2],
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        space: Space,
        paint: Paint,
    ) {
        // bit 0 screen space, 1 silhouette, 2 masked, 3 filter, 4 smooth, 5 spread;
        // bits 16-31 the mask.
        let (mode, mask, color) = match paint {
            Paint::Normal => (0, 0, color),
            Paint::Silhouette => (2, 0, color),
            Paint::Masked(m) => (4, m, color),
            Paint::Filter(m) => (8, m, Paint::filter_tint(m)),
            Paint::Spread => (32, 0, color),
        };
        let (pos, size) = match self.screen_frame {
            Some((o, k)) if space == Space::Screen => {
                // Both edges land on whole device pixels, so images that meet in the
                // frame meet on screen, with no pixel between them left uncovered.
                // Stretched 2.5 times an edge falls half-way between pixels, where two
                // neighbours' rounding errors could round it apart; the bias rounds
                // both the same way.
                let snap = |v: f32| (v * self.scale - 0.01).round() / self.scale;
                let x0 = snap(o[0] + pos[0] * k);
                let y0 = snap(o[1] + pos[1] * k);
                let x1 = snap(o[0] + (pos[0] + size[0]) * k);
                let y1 = snap(o[1] + (pos[1] + size[1]) * k);
                ([x0, y0], [x1 - x0, y1 - y0])
            }
            _ => (pos, size),
        };
        let inst = Instance {
            pos,
            size,
            uv0,
            uv1,
            color,
            flags: (space == Space::Screen) as u32 | mode | (self.smooth as u32) << 4 | (mask as u32) << 16,
        };
        let multiply = matches!(paint, Paint::Filter(_));
        let idx = self.instances.len() as u32;
        let clip = self.clip;
        match self.batches.last_mut() {
            Some(b) if b.source == source && b.clip == clip && b.multiply == multiply => b.end = idx + 1,
            _ => self.batches.push(Batch {
                source,
                multiply,
                clip,
                start: idx,
                end: idx + 1,
            }),
        }
        self.instances.push(inst);
    }

    /// Queues image `id` with its top-left corner at `pos`.
    pub fn image(
        &mut self,
        id: u32,
        pos: [f32; 2],
        color: [f32; 4],
        space: Space,
    ) -> Option<[f32; 2]> {
        let e = self.entry(id)?;
        let size = [e.w as f32, e.h as f32];
        self.push(e, pos, size, color, space, false);
        Some(size)
    }

    pub fn image_painted(&mut self, id: u32, pos: [f32; 2], color: [f32; 4], space: Space, paint: Paint) -> Option<[f32; 2]> {
        let e = self.entry(id)?;
        let size = [e.w as f32, e.h as f32];
        self.push_painted(e, pos, size, color, space, false, paint);
        Some(size)
    }

    pub fn image_flipped(
        &mut self,
        id: u32,
        pos: [f32; 2],
        color: [f32; 4],
        space: Space,
        flip: bool,
    ) {
        if let Some(e) = self.entry(id) {
            self.push(e, pos, [e.w as f32, e.h as f32], color, space, flip);
        }
    }

    pub fn image_scaled(
        &mut self,
        id: u32,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        space: Space,
    ) {
        if let Some(e) = self.entry(id) {
            self.push(e, pos, size, color, space, false);
        }
    }

    pub fn rect(&mut self, pos: [f32; 2], size: [f32; 2], color: [f32; 4], space: Space) {
        let e = self.white;
        self.push(e, pos, size, color, space, false);
    }

    fn make_dynamic_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dynamic image"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self::write_dynamic_texture(queue, &texture, w, h, rgba);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dynamic image"),
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
        (texture, bind_group)
    }

    fn write_dynamic_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
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
    }

    /// Uploads (or updates) a caller-managed RGBA8 image addressed by `key`, for content
    /// that isn't part of the sprite library (e.g. a minimap rebuilt in a CPU buffer). A
    /// second call with the same `key` reuses the existing texture, recreating it only if
    /// `w`/`h` changed. `rgba` must be `w * h * 4` bytes, row-major, straight alpha.
    pub fn upload_dynamic(&mut self, key: u32, w: u32, h: u32, rgba: &[u8]) -> DynamicHandle {
        if let Some(i) = self.dynamic.iter().position(|d| d.key == key) {
            if self.dynamic[i].w == w && self.dynamic[i].h == h {
                Self::write_dynamic_texture(&self.queue, &self.dynamic[i].texture, w, h, rgba);
            } else {
                let (texture, bind_group) = Self::make_dynamic_texture(
                    &self.device,
                    &self.queue,
                    &self.page_layout,
                    &self.sampler,
                    w,
                    h,
                    rgba,
                );
                self.dynamic[i] = DynamicTex { key, w, h, texture, bind_group };
            }
            return DynamicHandle(i as u16);
        }
        let (texture, bind_group) = Self::make_dynamic_texture(
            &self.device,
            &self.queue,
            &self.page_layout,
            &self.sampler,
            w,
            h,
            rgba,
        );
        self.dynamic.push(DynamicTex { key, w, h, texture, bind_group });
        DynamicHandle((self.dynamic.len() - 1) as u16)
    }

    /// Queues a dynamic image (from [`Renderer::upload_dynamic`]) stretched to `size`
    /// with its top-left corner at `pos`.
    pub fn dynamic_image(&mut self, handle: DynamicHandle, pos: [f32; 2], size: [f32; 2], space: Space) {
        if self.dynamic.get(handle.0 as usize).is_none() {
            return;
        }
        self.push_raw(
            PageSource::Dynamic(handle.0),
            [0.0, 0.0],
            [1.0, 1.0],
            pos,
            size,
            WHITE,
            space,
            Paint::Normal,
        );
    }

    /// Clips following draws to a screen rectangle, or stops clipping with `None`.
    pub fn set_clip(&mut self, clip: Option<[f32; 4]>) {
        self.clip = match (clip, self.screen_frame) {
            (Some(c), Some((o, k))) => Some([o[0] + c[0] * k, o[1] + c[1] * k, c[2] * k, c[3] * k]),
            _ => clip,
        };
    }

    /// Visible world rectangle as `(x0, y0, x1, y1)`.
    pub fn world_view(&self) -> [f32; 4] {
        let c = self.camera;
        [
            c.x,
            c.y,
            c.x + self.screen[0] / c.zoom,
            c.y + self.screen[1] / c.zoom,
        ]
    }

    pub fn screen_to_world(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] / self.camera.zoom + self.camera.x,
            p[1] / self.camera.zoom + self.camera.y,
        ]
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Draws everything queued since the last call into `target`, then clears the queue.
    pub fn flush(&mut self, target: &wgpu::TextureView, clear: Option<wgpu::Color>) {
        let g = Globals {
            screen: self.screen,
            camera: [self.camera.x, self.camera.y],
            zoom: self.camera.zoom,
            _pad: [0.0; 3],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        let needed = (self.instances.len() * std::mem::size_of::<Instance>()) as u64;
        if needed > self.instance_buffer.size() {
            self.instance_buffer =
                Self::make_instance_buffer(&self.device, self.instances.len().next_power_of_two());
        }
        if !self.instances.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sprites"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            let mut multiply = None;
            for b in &self.batches {
                if multiply != Some(b.multiply) {
                    pass.set_pipeline(if b.multiply { &self.multiply_pipeline } else { &self.pipeline });
                    multiply = Some(b.multiply);
                }
                let bind_group = match b.source {
                    PageSource::Atlas(p) => &self.atlas.pages[p as usize].bind_group,
                    PageSource::Dynamic(p) => &self.dynamic[p as usize].bind_group,
                };
                pass.set_bind_group(1, bind_group, &[]);
                let (tw, th) = (self.screen[0] * self.scale, self.screen[1] * self.scale);
                let [x, y, w, h] = b.clip.unwrap_or([0.0, 0.0, self.screen[0], self.screen[1]]);
                let x0 = (x * self.scale).clamp(0.0, tw) as u32;
                let y0 = (y * self.scale).clamp(0.0, th) as u32;
                let x1 = ((x + w) * self.scale).clamp(0.0, tw) as u32;
                let y1 = ((y + h) * self.scale).clamp(0.0, th) as u32;
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.draw(0..4, b.start..b.end);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.instances.clear();
        self.batches.clear();
    }
}
