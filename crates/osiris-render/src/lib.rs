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
    space: f32,
}

struct Batch {
    page: u16,
    start: u32,
    end: u32,
}

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

pub const WHITE: [f32; 4] = [1.0; 4];

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    page_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas: Atlas,
    white: AtlasEntry,
    instances: Vec<Instance>,
    batches: Vec<Batch>,
    instance_buffer: wgpu::Buffer,
    pub camera: Camera,
    pub screen: [f32; 2],
    pub library: ImageLibrary,
}

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
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2, 4 => Float32x4, 5 => Float32
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let instance_buffer = Self::make_instance_buffer(&device, 1 << 16);
        let mut atlas = Atlas::new();
        let white = atlas.reserve_white(&device, &queue, &page_layout, &sampler);
        Self {
            device,
            queue,
            pipeline,
            globals,
            globals_bind,
            page_layout,
            sampler,
            atlas,
            white,
            instances: Vec::new(),
            batches: Vec::new(),
            instance_buffer,
            camera: Camera::default(),
            screen: [1.0, 1.0],
            library,
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

    /// Atlas entry for global image `id`, uploading it on first use.
    pub fn entry(&mut self, id: u32) -> Option<AtlasEntry> {
        let img = self.library.resolve(id)?;
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
        Some(self.library.record(self.library.resolve(id)?))
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
        let s = 1.0 / PAGE_SIZE as f32;
        let (mut u0, mut u1) = (e.x as f32 * s, (e.x + e.w) as f32 * s);
        if e.flip != flip {
            std::mem::swap(&mut u0, &mut u1);
        }
        let inst = Instance {
            pos,
            size,
            uv0: [u0, e.y as f32 * s],
            uv1: [u1, (e.y + e.h) as f32 * s],
            color,
            space: if space == Space::Screen { 1.0 } else { 0.0 },
        };
        let idx = self.instances.len() as u32;
        match self.batches.last_mut() {
            Some(b) if b.page == e.page => b.end = idx + 1,
            _ => self.batches.push(Batch {
                page: e.page,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            for b in &self.batches {
                pass.set_bind_group(1, &self.atlas.pages[b.page as usize].bind_group, &[]);
                pass.draw(0..4, b.start..b.end);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.instances.clear();
        self.batches.clear();
    }
}
