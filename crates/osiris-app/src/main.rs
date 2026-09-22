mod city_view;
mod game;
mod gfx;
mod sidebar;

use anyhow::{Context, Result, bail};
use osiris_formats::{ImageLibrary, MissionPak, Model, Scenario, TextTable};
use osiris_sim::{Balance, Command, Defs, World};
use std::sync::Arc;
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
    script: Option<String>,
    size: (u32, u32),
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        data: PathBuf::from("PharaohData"),
        map: None,
        mission: None,
        screenshot: None,
        script: None,
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
            "--script" => args.script = Some(val()?),
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
    game: Option<game::Game>,
    world: Option<World>,
    text: Arc<TextTable>,
    gfx: Option<gfx::Gfx>,
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
        let images = sidebar::SidebarImages::load(&library).expect("sidebar images");
        let mut gfx = pollster::block_on(gfx::Gfx::new(window, library)).expect("init graphics");
        let mut game = game::Game::new(self.world.take().expect("world"), images, self.text.clone());
        let (cx, cy) = start_view(&game.world);
        game.view.center_on(&mut gfx.renderer, &game.world.map, cx, cy);
        self.game = Some(game);
        self.gfx = Some(gfx);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(gfx), Some(game)) = (self.gfx.as_mut(), self.game.as_mut()) else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                key_pressed(game, code);
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
                let at = [(p.0 / scale) as f32, (p.1 / scale) as f32];
                game.set_cursor(&gfx.renderer, at);
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => game.press(gfx.renderer.screen[0]),
                (MouseButton::Left, ElementState::Released) => game.release(),
                (MouseButton::Right, ElementState::Pressed) => {
                    game.cancel();
                    self.drag = Some(self.cursor);
                }
                (MouseButton::Middle, ElementState::Pressed) => self.drag = Some(self.cursor),
                (_, ElementState::Released) => self.drag = None,
                _ => {}
            },
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
                game.update(dt);
                gfx.frame(|r| game.draw(r));
                self.frames += 1;
                if self.fps_timer.elapsed().as_secs_f32() >= 1.0 {
                    let t = self.fps_timer.elapsed().as_secs_f32();
                    gfx.window.set_title(&format!(
                        "Osiris - {} ({:.0} fps, {} sprites)",
                        game.world.scenario_name,
                        self.frames as f32 / t,
                        game.view.last_sprites
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

fn key_pressed(g: &mut game::Game, code: KeyCode) {
        match code {
            KeyCode::Escape => g.cancel(),
            KeyCode::KeyR => g.tool = game::Tool::Road,
            KeyCode::KeyC | KeyCode::Delete | KeyCode::Backspace => g.tool = game::Tool::Clear,
            KeyCode::KeyP | KeyCode::Space => g.paused = !g.paused,
            KeyCode::BracketRight | KeyCode::Equal => g.speed = (g.speed + 10).min(100),
            KeyCode::BracketLeft | KeyCode::Minus => g.speed = g.speed.saturating_sub(10).max(10),
            _ => {}
        }
}

/// Where the camera starts: the scenario's entry point, or the map centre.
fn start_view(w: &World) -> (i32, i32) {
    let e = w.entry_point;
    if w.map.contains(e.0, e.1) { e } else { (w.map.width / 2, w.map.height / 2) }
}

fn parse_point(s: &str) -> Result<(i32, i32)> {
    let (x, y) = s.split_once(',').context("expected x,y")?;
    Ok((x.trim().parse()?, y.trim().parse()?))
}

/// Runs `--script` steps against the world; returns the tile to centre the view on.
fn run_script(world: &mut World, script: &str) -> Result<Option<(i32, i32)>> {
    let mut view = None;
    for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = step.split_whitespace().collect();
        match parts.as_slice() {
            ["road", a, b] => {
                let out = world.apply(&Command::Road { start: parse_point(a)?, end: parse_point(b)? });
                eprintln!("{step}: {out:?}");
            }
            ["clear", a, b] => {
                let (a, b) = (parse_point(a)?, parse_point(b)?);
                let out = world.apply(&Command::Clear { x0: a.0, y0: a.1, x1: b.0, y1: b.1 });
                eprintln!("{step}: {out:?}");
            }
            ["build", k, a] | ["build", k, a, _] => {
                let a = parse_point(a)?;
                let b = match parts.get(3) { Some(p) => parse_point(p)?, None => a };
                let out = world.apply(&Command::Build { kind: k.parse()?, x: a.0, y: a.1, x1: b.0, y1: b.1 });
                eprintln!("{step}: {out:?}");
            }
            ["ticks", n] => {
                for _ in 0..n.parse::<u32>()? {
                    world.tick();
                }
            }
            ["view", p] => view = Some(parse_point(p)?),
            ["report"] => {
                let houses: Vec<String> = world
                    .buildings
                    .iter()
                    .filter_map(|b| b.house.as_ref().map(|h| format!("({},{})L{}p{}i{}d{}{}", b.x, b.y, h.level, h.population, h.incoming, b.desirability, if h.well_access { "w" } else { "" })))
                    .collect();
                eprintln!(
                    "{:?} pop {} treasury {} figures {} houses {:?}",
                    world.time, world.population, world.treasury, world.figures.len(), houses
                );
                eprintln!("  labor {:?} unemployment {}%", world.labor, world.unemployment);
                for b in world.buildings.iter().filter(|b| !b.is_house()) {
                    eprintln!("  bld {} kind {} at ({},{}) workers {} covered {} road {:?} walkers {:?}", b.id, b.kind, b.x, b.y, b.workers, b.houses_covered, b.road, b.walkers);
                }
                for f in world.figures.iter().take(5) {
                    eprintln!(
                        "  fig {} kind {} at ({},{}) dest {:?} route {} moving {} counter {} progress {} dir {}",
                        f.id, f.kind, f.x, f.y, f.destination, f.route.len(), f.moving, f.counter, f.progress, f.direction
                    );
                }
            }
            _ => bail!("bad script step: {step}"),
        }
    }
    Ok(view)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = parse_args()?;
    let library = ImageLibrary::open(&args.data.join("Data"))
        .with_context(|| format!("loading sprites from {}", args.data.display()))?;
    let scenario = load_scenario(&args)?;
    let defs = Arc::new(Defs::load(&library).map_err(anyhow::Error::msg)?);
    let model_path = args.data.join("Pharaoh_Model_Normal.txt");
    let model_text = std::fs::read(&model_path).with_context(|| model_path.display().to_string())?;
    let model = Model::parse(&String::from_utf8_lossy(&model_text))?;
    let balance = Arc::new(Balance::from_model(&model));
    let text = Arc::new(TextTable::parse(&std::fs::read(args.data.join("Pharaoh_Text.eng"))?)?);
    let mut world = World::new(&scenario, defs, balance);
    let script_view = match &args.script {
        Some(s) => run_script(&mut world, s)?,
        None => None,
    };

    if let Some(out) = &args.screenshot {
        let images = sidebar::SidebarImages::load(&library)?;
        let mut game = game::Game::new(world, images, text);
        let (cx, cy) = script_view.unwrap_or_else(|| start_view(&game.world));
        return gfx::screenshot(library, args.size, out, |r| {
            game.view.center_on(r, &game.world.map, cx, cy);
            game.draw(r);
        });
    }

    let event_loop = EventLoop::new()?;
    let now = std::time::Instant::now();
    let mut app = App {
        args,
        library: Some(library),
        game: None,
        world: Some(world),
        text,
        gfx: None,
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
