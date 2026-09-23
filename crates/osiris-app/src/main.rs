mod advisors;
mod anims;
mod army_view;
mod city_view;
mod empire_window;
mod game;
mod gfx;
mod info;
mod menu;
mod message_list;
mod rules_panel;
mod minimap;
mod overlay;
mod script;
mod sidebar;
mod top_menu;
mod water_view;
mod widgets;

use anyhow::{Context, Result, bail};
use osiris_formats::{Campaign, ImageLibrary, MessageTable, MissionPak, Model, Scenario, TextTable};
use osiris_sim::{Balance, Defs, World};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const USAGE: &str = "usage: osiris [--data DIR] [--map FILE | --mission N] [--screenshot OUT.png] [--size WxH] [--script STEPS]";

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
        data: default_data_dir(),
        map: None,
        mission: None,
        screenshot: None,
        script: None,
        size: (1280, 800),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().with_context(|| format!("{a} needs a value\n{USAGE}"));
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
            _ if a.starts_with("-psn_") => {} // macOS Finder process serial number
            _ => bail!("unknown argument {a}\n{USAGE}"),
        }
    }
    Ok(args)
}

/// Where Osiris keeps saves and campaign progress.
fn user_dir() -> PathBuf {
    let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("Library/Application Support/Osiris");
    let _ = std::fs::create_dir_all(dir.join("saves"));
    dir
}

/// The game data: `PharaohData` next to the executable or app bundle, in the user
/// directory, or in the current directory.
fn default_data_dir() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1).take(5) {
            candidates.push(dir.join("PharaohData"));
        }
    }
    candidates.push(user_dir().join("PharaohData"));
    candidates.push(PathBuf::from("PharaohData"));
    candidates
        .into_iter()
        .find(|p| p.join("Data").is_dir())
        .unwrap_or_else(|| PathBuf::from("PharaohData"))
}

/// Everything loaded once at startup and shared by every game.
struct Assets {
    data: PathBuf,
    defs: Arc<Defs>,
    balance: Arc<Balance>,
    text: Arc<TextTable>,
    messages: Arc<MessageTable>,
    phrases: Arc<osiris_formats::Phrases>,
    mission_names: Vec<String>,
}

enum Source {
    Mission(usize),
    Map(PathBuf),
}

fn new_world(assets: &Assets, source: &Source) -> Result<World> {
    let (scenario, mission) = match source {
        Source::Map(path) => (Scenario::load_map(path)?, None),
        Source::Mission(n) => (MissionPak::open(&assets.data.join("mission1.pak"))?.scenario(*n)?, Some(*n)),
    };
    let mut world = World::new(&scenario, assets.defs.clone(), assets.balance.clone());
    world.start(&scenario);
    if let Some(n) = mission {
        world.load_mission(n as i32);
    }
    Ok(world)
}

fn list_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
                .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('_')))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn rules_path() -> PathBuf {
    user_dir().join("rules.toml")
}

/// The player's game rules, applied to every game.
fn load_rules() -> osiris_sim::Rules {
    std::fs::read_to_string(rules_path()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn save_rules(rules: &osiris_sim::Rules) {
    if let Ok(s) = toml::to_string(rules) {
        let _ = std::fs::write(rules_path(), s);
    }
}

fn progress_path() -> PathBuf {
    user_dir().join("progress.txt")
}

/// Highest campaign mission index the player may start.
fn unlocked_missions() -> usize {
    std::fs::read_to_string(progress_path()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

fn unlock_mission(n: usize) {
    if n > unlocked_missions() {
        let _ = std::fs::write(progress_path(), n.to_string());
    }
}

enum Screen {
    Menu(menu::Menu),
    Playing(Box<game::Game>, Option<usize>),
}

struct App {
    args: Args,
    assets: Assets,
    library: Option<ImageLibrary>,
    images: Option<sidebar::SidebarImages>,
    screen: Option<Screen>,
    audio: Option<Arc<osiris_audio::Audio>>,
    gfx: Option<gfx::Gfx>,
    drag: Option<(f64, f64)>,
    /// Where a right-click began with nothing open, to inspect the tile if it isn't a drag.
    inspect: Option<(f64, f64)>,
    cursor: (f64, f64),
    keys: std::collections::HashSet<KeyCode>,
    last_frame: std::time::Instant,
    status: Option<(String, f32)>,
}

impl App {
    fn menu(&self) -> menu::Menu {
        menu::Menu::new(
            self.assets.mission_names.clone(),
            unlocked_missions(),
            list_files(&self.assets.data.join("Maps"), "map"),
            list_files(&user_dir().join("saves"), "osiris"),
            load_rules(),
        )
    }

    fn start(&mut self, mut world: World, mission: Option<usize>) {
        let (Some(gfx), Some(images)) = (self.gfx.as_mut(), self.images.clone()) else { return };
        world.rules = load_rules();
        let mut game = game::Game::new(world, images, self.assets.text.clone(), self.assets.messages.clone(), self.audio.clone());
        game.phrases = self.assets.phrases.clone();
        let (cx, cy) = start_view(&game.world);
        game.view.center_on(&mut gfx.renderer, &game.world.map, cx, cy);
        self.screen = Some(Screen::Playing(Box::new(game), mission));
    }

    fn choose(&mut self, choice: menu::Choice, event_loop: &ActiveEventLoop) {
        let result = match &choice {
            menu::Choice::Quit => {
                event_loop.exit();
                return;
            }
            menu::Choice::Mission(n) => new_world(&self.assets, &Source::Mission(*n)).map(|w| (w, Some(*n))),
            menu::Choice::Map(p) => new_world(&self.assets, &Source::Map(p.clone())).map(|w| (w, None)),
            menu::Choice::Save(p) => load_game(&self.assets, p).map(|w| {
                let m = w.mission.as_ref().map(|m| m.id as usize);
                (w, m)
            }),
        };
        match result {
            Ok((world, mission)) => self.start(world, mission),
            Err(e) => self.status = Some((format!("Could not start: {e}"), 5.0)),
        }
    }

    /// Carries out a File-menu choice made in a running game.
    fn menu_request(&mut self, request: top_menu::MenuAction, event_loop: &ActiveEventLoop) {
        use top_menu::MenuAction;
        let mission = match &self.screen {
            Some(Screen::Playing(_, m)) => *m,
            _ => None,
        };
        match request {
            MenuAction::Save => self.quicksave(),
            MenuAction::Quit => event_loop.exit(),
            MenuAction::Load => {
                let mut menu = self.menu();
                menu.open_page("load");
                self.screen = Some(Screen::Menu(menu));
            }
            MenuAction::Replay => match mission {
                Some(n) => self.choose(menu::Choice::Mission(n), event_loop),
                None => self.status = Some(("Only campaign missions can be replayed".into(), 3.0)),
            },
            _ => self.screen = Some(Screen::Menu(self.menu())),
        }
    }

    fn save_path(game: &game::Game) -> PathBuf {
        user_dir().join("saves").join(format!("{}.osiris", sanitize(&game.world.scenario_name)))
    }

    fn quicksave(&mut self) {
        let Some(Screen::Playing(game, _)) = &self.screen else { return };
        let path = Self::save_path(game);
        let result = game.world.save().map_err(anyhow::Error::msg).and_then(|b| Ok(std::fs::write(&path, b)?));
        let msg = match result {
            Ok(()) => format!("Saved {}", path.file_stem().unwrap_or_default().to_string_lossy()),
            Err(e) => format!("Save failed: {e}"),
        };
        self.status = Some((msg, 3.0));
    }

    fn quickload(&mut self) {
        let Some(Screen::Playing(game, _)) = &self.screen else { return };
        let path = Self::save_path(game);
        match load_game(&self.assets, &path) {
            Ok(world) => {
                let m = world.mission.as_ref().map(|m| m.id as usize);
                self.start(world, m);
                self.status = Some(("Loaded".into(), 2.0));
            }
            Err(e) => self.status = Some((format!("Load failed: {e}"), 3.0)),
        }
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    s.trim().to_owned()
}

fn load_game(assets: &Assets, path: &Path) -> Result<World> {
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    World::load(&bytes, assets.defs.clone(), assets.balance.clone()).map_err(anyhow::Error::msg)
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Osiris")
            .with_inner_size(winit::dpi::LogicalSize::new(self.args.size.0, self.args.size.1));
        let window = _event_loop.create_window(attrs).expect("create window");
        let library = self.library.take().expect("library");
        self.images = Some(sidebar::SidebarImages::load(&library).expect("sidebar images"));
        self.gfx = Some(pollster::block_on(gfx::Gfx::new(window, library)).expect("init graphics"));
        let direct = if let Some(map) = &self.args.map {
            Some((Source::Map(map.clone()), None))
        } else {
            self.args.mission.map(|n| (Source::Mission(n), Some(n)))
        };
        match direct {
            Some((source, mission)) => match new_world(&self.assets, &source) {
                Ok(world) => self.start(world, mission),
                Err(e) => {
                    self.status = Some((format!("{e}"), 5.0));
                    self.screen = Some(Screen::Menu(self.menu()));
                }
            },
            None => self.screen = Some(Screen::Menu(self.menu())),
        }
        if let Some(a) = &self.audio {
            a.update_music(0);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(scale) = self.gfx.as_ref().map(|g| g.window.scale_factor()) else { return };
        let at = [(self.cursor.0 / scale) as f32, (self.cursor.1 / scale) as f32];
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.gfx {
                    gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                if event.state == ElementState::Released {
                    self.keys.remove(&code);
                    return;
                }
                self.keys.insert(code);
                if event.repeat {
                    return;
                }
                match code {
                    KeyCode::F5 => self.quicksave(),
                    KeyCode::F9 => self.quickload(),
                    _ => match &mut self.screen {
                        Some(Screen::Menu(m)) if code == KeyCode::Escape => m.back(),
                        Some(Screen::Playing(g, _)) if code == KeyCode::Escape && g.idle() => {
                            self.screen = Some(Screen::Menu(self.menu()));
                        }
                        Some(Screen::Playing(g, _)) => {
                            let ctrl = [KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]
                                .iter()
                                .any(|k| self.keys.contains(k));
                            key_pressed(g, code, ctrl);
                        }
                        _ => {}
                    },
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(gfx) = &mut self.gfx else { return };
                let p = (position.x, position.y);
                if let Some(d) = self.drag {
                    let cam = &mut gfx.renderer.camera;
                    cam.x -= ((p.0 - d.0) / scale) as f32 / cam.zoom;
                    cam.y -= ((p.1 - d.1) / scale) as f32 / cam.zoom;
                    self.drag = Some(p);
                }
                self.cursor = p;
                let at = [(p.0 / scale) as f32, (p.1 / scale) as f32];
                match &mut self.screen {
                    Some(Screen::Menu(m)) => m.hover(gfx.renderer.screen, at),
                    Some(Screen::Playing(g, _)) => g.set_cursor(&gfx.renderer, at),
                    None => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(gfx) = &mut self.gfx else { return };
                let screen = gfx.renderer.screen;
                let mut chosen = None;
                let mut request = None;
                match &mut self.screen {
                    Some(Screen::Menu(m)) => match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            chosen = m.click(screen, at);
                            if std::mem::take(&mut m.rules_changed) {
                                save_rules(&m.rules);
                            }
                        }
                        (MouseButton::Right, ElementState::Pressed) => m.back(),
                        _ => {}
                    },
                    Some(Screen::Playing(game, _)) => match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            if let Some((x, y)) = game.press_at(&gfx.renderer) {
                                game.view.center_on(&mut gfx.renderer, &game.world.map, x, y);
                            }
                            if std::mem::take(&mut game.rules_changed) {
                                save_rules(&game.world.rules);
                            }
                            request = game.request.take();
                        }
                        (MouseButton::Left, ElementState::Released) => game.release(),
                        (MouseButton::Right, ElementState::Pressed) => {
                            // With nothing open, a right-click (not a drag) inspects the tile.
                            self.inspect = game.idle().then_some(self.cursor);
                            game.cancel();
                            self.drag = Some(self.cursor);
                        }
                        (MouseButton::Right, ElementState::Released) => {
                            if let Some(p) = self.inspect.take()
                                && (p.0 - self.cursor.0).abs() + (p.1 - self.cursor.1).abs() < 6.0
                            {
                                game.inspect();
                            }
                            self.drag = None;
                        }
                        (MouseButton::Middle, ElementState::Pressed) => self.drag = Some(self.cursor),
                        (_, ElementState::Released) => self.drag = None,
                        _ => {}
                    },
                    None => {}
                }
                if let Some(c) = chosen {
                    self.choose(c, event_loop);
                }
                if let Some(r) = request {
                    self.menu_request(r, event_loop);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(gfx) = &mut self.gfx else { return };
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                match &mut self.screen {
                    Some(Screen::Menu(m)) => m.scroll(-steps.signum() as i32),
                    Some(Screen::Playing(game, _)) => {
                        if !game.scroll_dialog(-steps * 20.0, gfx.renderer.screen) {
                            city_view::zoom_at(&mut gfx.renderer, at, 1.1f32.powf(steps));
                        }
                    }
                    None => {}
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

impl App {
    fn redraw(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        if let Some((_, t)) = &mut self.status {
            *t -= dt;
            if *t <= 0.0 {
                self.status = None;
            }
        }
        let status = self.status.as_ref().map(|(s, _)| s.clone());
        let Some(gfx) = &mut self.gfx else { return };
        let mut finished: Option<Option<usize>> = None;
        match &mut self.screen {
            Some(Screen::Menu(m)) => {
                let panels = self.images.as_ref().map(|i| &i.panels);
                gfx.frame(|r| {
                    if let Some(p) = panels {
                        m.draw(r, p);
                    }
                    if let Some(s) = &status {
                        osiris_ui::draw_text(r, osiris_ui::Font::NormalYellow, s, 20.0, 20.0, osiris_ui::font::WHITE);
                    }
                });
            }
            Some(Screen::Playing(game, mission)) => {
                let pan = 900.0 * dt / gfx.renderer.camera.zoom;
                let screen = gfx.renderer.screen;
                let cam = &mut gfx.renderer.camera;
                for (k, dx, dy) in [
                    (KeyCode::ArrowLeft, -1.0, 0.0),
                    (KeyCode::ArrowRight, 1.0, 0.0),
                    (KeyCode::ArrowUp, 0.0, -1.0),
                    (KeyCode::ArrowDown, 0.0, 1.0),
                ] {
                    if self.keys.contains(&k) {
                        match &mut game.empire {
                            Some(e) => e.scroll_by(screen, dx * 900.0 * dt, dy * 900.0 * dt),
                            None => {
                                cam.x += dx * pan;
                                cam.y += dy * pan;
                            }
                        }
                    }
                }
                game.update(dt);
                gfx.frame(|r| {
                    game.draw(r);
                    if let Some(s) = &status {
                        osiris_ui::draw_text(r, osiris_ui::Font::NormalYellow, s, 20.0, 50.0, osiris_ui::font::WHITE);
                    }
                });
                // Once the victory message has been read, go on to the next mission.
                if game.world.won && game.idle() {
                    finished = Some(*mission);
                }
            }
            None => {}
        }
        if let Some(mission) = finished {
            let next = mission.map(|m| m + 1);
            if let Some(n) = next {
                unlock_mission(n);
            }
            let mut menu = self.menu();
            if next.is_some() {
                menu.show_campaign();
            }
            self.screen = Some(Screen::Menu(menu));
        }
        if let Some(gfx) = &self.gfx {
            gfx.window.request_redraw();
        }
    }
}

/// The original's single-key commands. `ctrl` is Control or Command.
fn key_pressed(g: &mut game::Game, code: KeyCode, ctrl: bool) {
    use game::Tool;
    use osiris_sim::buildings::kind;
    match (code, ctrl) {
        (KeyCode::Escape, _) => g.cancel(),
        (KeyCode::F2, _) => g.open_rules(),
        (KeyCode::KeyP, _) => g.paused = !g.paused,
        (KeyCode::BracketRight | KeyCode::PageDown, false) => g.faster(),
        (KeyCode::BracketLeft | KeyCode::PageUp, false) => g.slower(),
        (KeyCode::Minus, false) => g.open_advisor(advisors::Advisor::Financial),
        (KeyCode::Equal, false) => g.open_advisor(advisors::Advisor::Chief),
        (KeyCode::Space, _) => g.toggle_overlay(),
        (KeyCode::KeyW, false) => g.show_overlay(overlay::Overlay::Water),
        (KeyCode::KeyF, false) => g.show_overlay(overlay::Overlay::Fire),
        (KeyCode::KeyD, false) => g.show_overlay(overlay::Overlay::Damage),
        (KeyCode::KeyB, false) => g.tool = Tool::Road,
        (KeyCode::KeyX, false) | (KeyCode::Delete | KeyCode::Backspace, _) => g.tool = Tool::Clear,
        (KeyCode::KeyH, true) => g.try_tool(kind::VACANT_LOT),
        (KeyCode::KeyG, false) => g.try_tool(kind::GARDENS),
        (KeyCode::KeyF, true) => g.try_tool(kind::FIREHOUSE),
        (KeyCode::KeyA, true) => g.try_tool(kind::ARCHITECT_POST),
        (KeyCode::KeyO, false) => g.try_tool(kind::APOTHECARY),
        (KeyCode::KeyN, false) => g.try_tool(kind::GRANARY),
        (KeyCode::KeyU, false) => g.try_tool(kind::STORAGE_YARD),
        (KeyCode::KeyM, false) => g.try_tool(kind::BAZAAR),
        (KeyCode::KeyT, false) => g.try_tool(kind::WATER_SUPPLY),
        _ => {}
    }
}

/// Where the camera starts: the scenario's entry point, or the map centre.
fn start_view(w: &World) -> (i32, i32) {
    let e = w.entry_point;
    if w.map.contains(e.0, e.1) { e } else { (w.map.width / 2, w.map.height / 2) }
}

fn load_assets(data: &Path, library: &ImageLibrary) -> Result<Assets> {
    let defs = Arc::new(Defs::load(library).map_err(anyhow::Error::msg)?);
    let model_path = data.join("Pharaoh_Model_Normal.txt");
    let model_text = std::fs::read(&model_path).with_context(|| model_path.display().to_string())?;
    let mut balance = Balance::from_model(&Model::parse(&String::from_utf8_lossy(&model_text))?);
    if let Ok(t) = std::fs::read(data.join("Tax_Sentiment_Model_Normal.txt")) {
        balance.tax_sentiment = osiris_formats::model::parse_tax_sentiment(&String::from_utf8_lossy(&t));
    }
    if let Ok(t) = std::fs::read(data.join("Figure_model_normal.txt")).or_else(|_| std::fs::read(data.join("Figure_model.txt"))) {
        balance.set_units(&osiris_formats::model::parse_figures(&String::from_utf8_lossy(&t))?);
    }
    let balance = Arc::new(balance);
    let text = Arc::new(TextTable::parse(&std::fs::read(data.join("Pharaoh_Text.eng"))?)?);
    let messages = Arc::new(MessageTable::parse(&std::fs::read(data.join("Pharaoh_MM.eng"))?)?);
    let campaign = std::fs::read(data.join("campaign.txt"))
        .ok()
        .and_then(|b| Campaign::parse(&String::from_utf8_lossy(&b)).ok());
    let pak = MissionPak::open(&data.join("mission1.pak"))?;
    let count = (0..pak.slots()).filter(|&i| pak.entry(i).is_some()).count();
    let names = campaign.map(|c| c.mission_names).unwrap_or_default();
    let mission_names = (0..count)
        .map(|i| names.get(i).cloned().unwrap_or_else(|| format!("Mission {}", i + 1)))
        .collect();
    let phrases = Arc::new(osiris_formats::Phrases::parse(&String::from_utf8_lossy(&std::fs::read(data.join("eventmsg.txt")).unwrap_or_default())));
    Ok(Assets { data: data.to_owned(), defs, balance, text, messages, phrases, mission_names })
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = parse_args()?;
    let library = ImageLibrary::open(&args.data.join("Data")).with_context(|| {
        format!(
            "Could not find the Pharaoh game data at {}. Put your PharaohData folder (from the GOG \
             Pharaoh Gold install) next to Osiris, or pass --data <dir>.",
            args.data.display()
        )
    })?;
    let assets = load_assets(&args.data, &library)?;

    if let Some(out) = &args.screenshot {
        let source = match &args.map {
            Some(m) => Source::Map(m.clone()),
            None => Source::Mission(args.mission.unwrap_or(0)),
        };
        let mut world = new_world(&assets, &source)?;
        let view = match &args.script {
            Some(s) => script::run_script(&mut world, s)?,
            None => script::ScriptView { keep_dialogs: true, ..Default::default() },
        };
        let images = sidebar::SidebarImages::load(&library)?;
        if view.menu {
            let mut menu = menu::Menu::new(assets.mission_names.clone(), 2, list_files(&assets.data.join("Maps"), "map"), vec![], Default::default());
            if let Some(page) = &view.menu_page {
                menu.open_page(page);
            }
            return gfx::screenshot(library, args.size, out, |r| menu.draw(r, &images.panels));
        }
        let mut game = game::Game::new(world, images, assets.text.clone(), assets.messages.clone(), None);
        game.phrases = assets.phrases.clone();
        let (cx, cy) = view.centre.or(view.info).unwrap_or_else(|| start_view(&game.world));
        if !view.keep_dialogs {
            game.close_dialog();
        }
        if let Some(name) = &view.overlay {
            game.view_overlay = overlay::MENU.iter().map(|(o, _)| *o).find(|o| format!("{o:?}").eq_ignore_ascii_case(name));
        }
        if let Some(n) = view.top_menu {
            game.open_top_menu(n);
        }
        if let Some(name) = &view.build_menu
            && let Some(c) = sidebar::Category::ALL.iter().find(|c| format!("{c:?}").eq_ignore_ascii_case(name))
        {
            game.choose_category(*c);
        }
        if let Some(name) = &view.advisor
            && let Some(a) = advisors::ALL.iter().find(|a| format!("{a:?}").eq_ignore_ascii_case(name))
        {
            game.open_advisor(*a);
            if let (Some(p), Some(adv)) = (&view.advisor_popup, game.advisors.as_mut()) {
                adv.open_popup(p);
            }
        }
        if let Some(city) = view.empire {
            let mut e = empire_window::EmpireWindow::default();
            e.select(city);
            game.empire = Some(e);
        }
        if view.rules {
            game.rules_panel = Some(rules_panel::RulesPanel::default());
        }
        if view.messages {
            game.message_list = Some(message_list::MessageList::default());
        }
        if let Some((x, y)) = view.info {
            let mut panel = info::InfoPanel::new(game.info_target(x, y));
            if view.orders {
                panel.open_orders();
            }
            game.info = Some(panel);
        }
        return gfx::screenshot(library, args.size, out, |r| {
            game.view.center_on(r, &game.world.map, cx, cy);
            game.draw(r);
        });
    }

    let audio = osiris_audio::Audio::new(&args.data).ok().map(Arc::new);
    let event_loop = EventLoop::new()?;
    let mut app = App {
        args,
        assets,
        library: Some(library),
        images: None,
        screen: None,
        audio,
        gfx: None,
        drag: None,
        inspect: None,
        cursor: (0.0, 0.0),
        keys: Default::default(),
        last_frame: std::time::Instant::now(),
        status: None,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
