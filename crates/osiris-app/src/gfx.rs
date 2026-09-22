//! Window surface and offscreen targets around the renderer.

use anyhow::{Context, Result};
use osiris_formats::ImageLibrary;
use osiris_render::Renderer;
use std::sync::Arc;
use winit::window::Window;

const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

async fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter> {
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .context("no GPU adapter")
}

async fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    Ok(adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?)
}

/// Prefer a non-sRGB format so sprite colours reach the screen unchanged.
fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .unwrap_or(formats[0])
}

pub struct Gfx {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
}

impl Gfx {
    pub async fn new(window: Window, library: ImageLibrary) -> Result<Self> {
        let window = Arc::new(window);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone())?;
        let adapter = adapter(&instance, Some(&surface)).await?;
        let (device, queue) = device(&adapter).await?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("surface unsupported")?;
        config.format = pick_format(&surface.get_capabilities(&adapter).formats);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(device, queue, config.format, library);
        let scale = window.scale_factor() as f32;
        renderer.screen = [size.width as f32 / scale, size.height as f32 / scale];
        renderer.scale = scale;
        Ok(Self {
            window,
            surface,
            config,
            renderer,
        })
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(self.renderer.device(), &self.config);
        let scale = self.window.scale_factor() as f32;
        self.renderer.screen = [w as f32 / scale, h as f32 / scale];
        self.renderer.scale = scale;
    }

    pub fn frame(&mut self, draw: impl FnOnce(&mut Renderer)) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.surface.configure(self.renderer.device(), &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        draw(&mut self.renderer);
        self.renderer.flush(&view, Some(CLEAR));
        self.window.pre_present_notify();
        self.renderer.queue().present(frame);
    }
}

/// Renders one frame offscreen and writes it to `out` as PNG.
pub fn screenshot(
    library: ImageLibrary,
    size: (u32, u32),
    out: &std::path::Path,
    draw: impl FnOnce(&mut Renderer),
) -> Result<()> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(adapter(&instance, None))?;
    let (device, queue) = pollster::block_on(device(&adapter))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (w, h) = size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer = Renderer::new(device, queue, format, library);
    renderer.screen = [w as f32, h as f32];
    draw(&mut renderer);
    let sprites = renderer.instance_count();
    let t0 = std::time::Instant::now();
    renderer.flush(&texture.create_view(&Default::default()), Some(CLEAR));

    let row = (w * 4).div_ceil(256) * 256;
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = renderer
        .device()
        .create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        texture.size(),
    );
    renderer.queue().submit([enc.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
    renderer
        .device()
        .poll(wgpu::PollType::wait_indefinitely())?;
    let data = slice.get_mapped_range()?;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = (y * row) as usize;
        pixels.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    drop(data);
    eprintln!("rendered {sprites} sprites in {:?}", t0.elapsed());
    let file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
