mod city_view;
mod gfx;

use anyhow::{Context, Result, bail};
use osiris_formats::{ImageLibrary, MissionPak, Scenario};
use std::path::PathBuf;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const USAGE: &str =
    "usage: osiris [--data DIR] [--map FILE | --mission N] [--screenshot OUT.png] [--size WxH]";

struct Args {
    data: PathBuf,
    map: Option<PathBuf>,
    mission: Option<usize>,
    screenshot: Option<PathBuf>,
    size: (u32, u32),
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        data: PathBuf::from("PharaohData"),
        map: None,
        mission: None,
        screenshot: None,
        size: (1280, 800),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .with_context(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--data" => args.data = val()?.into(),
            "--map" => args.map = Some(val()?.into()),
            "--mission" => args.mission = Some(val()?.parse()?),
            "--screenshot" => args.screenshot = Some(val()?.into()),
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').context("--size WxH")?;
                args.size = (w.parse()?, h.parse()?);
            }
            _ => bail!("unknown argument {a}\n{USAGE}"),
        }
    }
    Ok(args)
}

fn load_scenario(args: &Args) -> Result<Scenario> {
    if let Some(map) = &args.map {
        return Ok(Scenario::load_map(map)?);
    }
    let pak = MissionPak::open(&args.data.join("mission1.pak"))?;
    Ok(pak.scenario(args.mission.unwrap_or(0))?)
}

struct App {
    args: Args,
    library: Option<ImageLibrary>,
    scenario: Scenario,
    gfx: Option<gfx::Gfx>,
    view: city_view::CityView,
    drag: Option<(f64, f64)>,
    cursor: (f64, f64),
    keys: std::collections::HashSet<KeyCode>,
    last_frame: std::time::Instant,
    frames: u32,
    fps_timer: std::time::Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Osiris")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.args.size.0,
                self.args.size.1,
            ));
        let window = event_loop.create_window(attrs).expect("create window");
        let library = self.library.take().expect("library");
        let mut gfx = pollster::block_on(gfx::Gfx::new(window, library)).expect("init graphics");
        self.view.center_camera(&mut gfx.renderer, &self.scenario);
        self.gfx = Some(gfx);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if code == KeyCode::Escape {
                                event_loop.exit();
                            }
                            self.keys.insert(code);
                        }
                        ElementState::Released => {
                            self.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = gfx.window.scale_factor();
                let p = (position.x, position.y);
                if let Some(d) = self.drag {
                    let cam = &mut gfx.renderer.camera;
                    cam.x -= ((p.0 - d.0) / scale) as f32 / cam.zoom;
                    cam.y -= ((p.1 - d.1) / scale) as f32 / cam.zoom;
                    self.drag = Some(p);
                }
                self.cursor = p;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Right || button == MouseButton::Middle {
                    self.drag = (state == ElementState::Pressed).then_some(self.cursor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                let scale = gfx.window.scale_factor();
                let at = [
                    (self.cursor.0 / scale) as f32,
                    (self.cursor.1 / scale) as f32,
                ];
                city_view::zoom_at(&mut gfx.renderer, at, 1.1f32.powf(steps));
            }
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;
                let pan = 900.0 * dt / gfx.renderer.camera.zoom;
                let cam = &mut gfx.renderer.camera;
                for (k, dx, dy) in [
                    (KeyCode::ArrowLeft, -1.0, 0.0),
                    (KeyCode::KeyA, -1.0, 0.0),
                    (KeyCode::ArrowRight, 1.0, 0.0),
                    (KeyCode::KeyD, 1.0, 0.0),
                    (KeyCode::ArrowUp, 0.0, -1.0),
                    (KeyCode::KeyW, 0.0, -1.0),
                    (KeyCode::ArrowDown, 0.0, 1.0),
                    (KeyCode::KeyS, 0.0, 1.0),
                ] {
                    if self.keys.contains(&k) {
                        cam.x += dx * pan;
                        cam.y += dy * pan;
                    }
                }
                gfx.frame(|r| self.view.draw(r, &self.scenario));
                self.frames += 1;
                if self.fps_timer.elapsed().as_secs_f32() >= 1.0 {
                    let t = self.fps_timer.elapsed().as_secs_f32();
                    gfx.window.set_title(&format!(
                        "Osiris - {} ({:.0} fps, {} sprites)",
                        self.scenario.info.subtitle,
                        self.frames as f32 / t,
                        self.view.last_sprites
                    ));
                    self.frames = 0;
                    self.fps_timer = std::time::Instant::now();
                }
                gfx.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = parse_args()?;
    let library = ImageLibrary::open(&args.data.join("Data"))
        .with_context(|| format!("loading sprites from {}", args.data.display()))?;
    let scenario = load_scenario(&args)?;

    if let Some(out) = &args.screenshot {
        let mut view = city_view::CityView::default();
        return gfx::screenshot(library, args.size, out, |r| {
            view.center_camera(r, &scenario);
            view.draw(r, &scenario);
        });
    }

    let event_loop = EventLoop::new()?;
    let now = std::time::Instant::now();
    let mut app = App {
        args,
        library: Some(library),
        scenario,
        gfx: None,
        view: city_view::CityView::default(),
        drag: None,
        cursor: (0.0, 0.0),
        keys: Default::default(),
        last_frame: now,
        frames: 0,
        fps_timer: now,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
