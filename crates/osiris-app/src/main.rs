// Release builds on Windows run without a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
mod progress;
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

/// Where Osiris keeps saves and campaign progress: the platform's usual place for
/// application data.
fn user_dir() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = || env("HOME").unwrap_or_else(|| PathBuf::from("."));
    let dir = if cfg!(target_os = "macos") {
        home().join("Library/Application Support/Osiris")
    } else if cfg!(windows) {
        env("APPDATA").unwrap_or_else(|| PathBuf::from(".")).join("Osiris")
    } else {
        env("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("osiris")
    };
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
    campaign: Arc<Campaign>,
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

/// Legacy single-player files, from before families: migrated into a family folder
/// the first time Osiris runs with the new layout (see [`migrate_legacy_family`]).
fn legacy_name_path() -> PathBuf {
    user_dir().join("name.txt")
}
fn legacy_progress_path() -> PathBuf {
    user_dir().join("progress.txt")
}
fn legacy_saves_dir() -> PathBuf {
    user_dir().join("saves")
}

/// Where every family's folder lives: `families/<sanitized name>/`.
fn families_dir() -> PathBuf {
    user_dir().join("families")
}

/// A family's own folder: its exact name (`name.txt`, since the folder name is
/// sanitized and may have lost punctuation), campaign progress and saves.
fn family_dir(name: &str) -> PathBuf {
    families_dir().join(sanitize(name))
}

fn family_progress_path(name: &str) -> PathBuf {
    family_dir(name).join("progress.txt")
}

fn family_saves_dir(name: &str) -> PathBuf {
    family_dir(name).join("saves")
}

/// A family folder's exact display name, read back from its `name.txt`, falling
/// back to the folder name itself if that's missing.
fn family_display_name(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("name.txt"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
}

/// Every family in the registry, alphabetically.
pub fn list_families() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(families_dir())
        .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).map(|e| family_display_name(&e.path())).collect())
        .unwrap_or_default();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

/// Creates a family's folder and makes it the current one. Assumes the caller has
/// already checked the name is non-empty and not already taken.
pub fn create_family(name: &str) {
    let dir = family_dir(name);
    let _ = std::fs::create_dir_all(dir.join("saves"));
    let _ = std::fs::write(dir.join("name.txt"), name.trim());
    choose_family(name);
}

/// Removes a family and all of its saved games. If it was the current family,
/// [`load_current_family`] naturally stops finding it, so no pointer needs clearing.
pub fn delete_family(name: &str) {
    let _ = std::fs::remove_dir_all(family_dir(name));
}

fn current_family_path() -> PathBuf {
    user_dir().join("family.txt")
}

/// Remembers the last chosen family, for next launch.
pub fn choose_family(name: &str) {
    let _ = std::fs::create_dir_all(user_dir());
    let _ = std::fs::write(current_family_path(), name.trim());
}

/// The last chosen family, if it still exists.
fn load_current_family() -> String {
    let name = std::fs::read_to_string(current_family_path()).unwrap_or_default().trim().to_owned();
    if !name.is_empty() && family_dir(&name).is_dir() { name } else { String::new() }
}

/// Moves the pre-family `name.txt`/`progress.txt`/`saves/` into a family folder, the
/// first time Osiris runs with the family registry. A no-op once any family exists.
fn migrate_legacy_family() {
    if !list_families().is_empty() {
        return;
    }
    let legacy_progress = legacy_progress_path();
    let legacy_saves = legacy_saves_dir();
    let has_saves = std::fs::read_dir(&legacy_saves).is_ok_and(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("osiris"))));
    if !legacy_progress.exists() && !has_saves {
        return;
    }
    let name = std::fs::read_to_string(legacy_name_path())
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_family_name);
    let dir = family_dir(&name);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("name.txt"), &name);
    if legacy_progress.exists() {
        let _ = std::fs::rename(&legacy_progress, dir.join("progress.txt"));
    }
    let saves_dir = dir.join("saves");
    let _ = std::fs::create_dir_all(&saves_dir);
    if let Ok(rd) = std::fs::read_dir(&legacy_saves) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("osiris")) {
                let _ = std::fs::rename(&p, saves_dir.join(p.file_name().unwrap()));
            }
        }
    }
    choose_family(&name);
}

fn default_family_name() -> String {
    std::env::var("USER")
        .ok()
        .and_then(|u| {
            let mut c = u.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect())
        })
        .unwrap_or_else(|| "Governor".to_owned())
}

/// The governor's name for the messages: the active family, else the account's.
pub fn player_name() -> String {
    let name = load_current_family();
    if !name.is_empty() { name } else { default_family_name() }
}

/// Where the player is in `family`'s campaign.
fn load_progress(c: &Campaign, family: &str) -> progress::Progress {
    match std::fs::read_to_string(family_progress_path(family)) {
        Ok(text) => progress::Progress::parse(c, &text),
        Err(_) => progress::Progress::new(c),
    }
}

fn save_progress(p: &progress::Progress, family: &str) {
    let _ = std::fs::create_dir_all(family_dir(family));
    let _ = std::fs::write(family_progress_path(family), p.to_text());
}

/// The family pages' text, resolved from `Pharaoh_Text.eng` (group numbers are the
/// original's own).
fn family_text(assets: &Assets) -> menu::FamilyText {
    let t = |group: u32, id: u32| assets.text.get(group as usize, id as usize).unwrap_or("").trim().to_string();
    menu::FamilyText {
        registry_title: t(292, 3),
        new_button: t(292, 0),
        delete_button: t(292, 1),
        proceed_button: t(292, 2),
        back_button: t(292, 4),
        enter_name: t(31, 0),
        continue_button: t(13, 5),
        cancel_button: t(12, 0),
        delete_title: t(5, 90),
        delete_body: t(5, 91),
        exists_title: t(5, 92),
        exists_body: t(5, 93),
        none_title: t(5, 94),
        none_body: t(5, 95),
        yes: t(18, 1),
        no: t(18, 0),
    }
}

/// What the menu shows of the campaign: the missions the player may start, which are
/// won, and the choice of city waiting to be made, if any.
fn campaign_view(assets: &Assets, p: &progress::Progress) -> menu::CampaignView {
    let text = |id: u32| assets.text.get(144, id as usize).unwrap_or("").trim().to_string();
    let choice = p.choice(&assets.campaign).map(|(screen, choices)| menu::ChoiceView {
        map: menu::CHOICE_MAPS + screen.graphic_id,
        title: text(screen.title_text_id),
        prompt: text(0),
        points: choices.iter().map(|c| menu::ChoicePoint { x: c.x as f32, y: c.y as f32, label: text(c.text_id), path: c.path_id }).collect(),
    });
    let count = assets.mission_names.len();
    menu::CampaignView {
        playable: p.playable().into_iter().filter(|&m| m < count).collect(),
        done: p.done.clone(),
        choice,
    }
}

enum Screen {
    Menu(Box<menu::Menu>),
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
    /// The active family; empty until one is chosen (gating the menu on startup).
    family: String,
}

impl App {
    /// Saves directory for the active family, or the pre-family default when none is
    /// chosen yet (used by `--map`/`--mission` direct launches, which skip the menu).
    fn saves_dir(&self) -> PathBuf {
        if self.family.is_empty() { legacy_saves_dir() } else { family_saves_dir(&self.family) }
    }

    fn menu(&self) -> Box<menu::Menu> {
        let campaign = if self.family.is_empty() { menu::CampaignView::default() } else { campaign_view(&self.assets, &load_progress(&self.assets.campaign, &self.family)) };
        Box::new(menu::Menu::new(
            self.assets.mission_names.clone(),
            campaign,
            list_files(&self.assets.data.join("Maps"), "map"),
            list_files(&self.saves_dir(), "osiris"),
            load_rules(),
            self.family.clone(),
            family_text(&self.assets),
        ))
    }

    fn start(&mut self, mut world: World, mission: Option<usize>) {
        let (Some(gfx), Some(images)) = (self.gfx.as_mut(), self.images.clone()) else { return };
        world.rules = load_rules();
        let mut game = game::Game::new(world, images, self.assets.text.clone(), self.assets.messages.clone(), self.audio.clone());
        game.phrases = self.assets.phrases.clone();
        game.player_name = player_name();
        start_camera(&mut gfx.renderer, &mut game);
        self.screen = Some(Screen::Playing(Box::new(game), mission));
    }

    fn choose(&mut self, choice: menu::Choice, event_loop: &ActiveEventLoop) {
        let result = match &choice {
            menu::Choice::Quit => {
                event_loop.exit();
                return;
            }
            menu::Choice::Mission(n) => new_world(&self.assets, &Source::Mission(*n)).map(|w| (w, Some(*n))),
            menu::Choice::Family(name) => {
                self.family = name.clone();
                choose_family(&self.family);
                self.screen = Some(Screen::Menu(self.menu()));
                return;
            }
            menu::Choice::Path(path) => {
                // The city is chosen: on to its first mission.
                let mut p = load_progress(&self.assets.campaign, &self.family);
                p.choose(&self.assets.campaign, *path);
                save_progress(&p, &self.family);
                match p.next {
                    progress::Next::Mission(m) => {
                        self.choose(menu::Choice::Mission(m), event_loop);
                    }
                    _ => {
                        let mut menu = self.menu();
                        menu.show_campaign();
                        self.screen = Some(Screen::Menu(menu));
                    }
                }
                return;
            }
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

    fn save_path(&self, game: &game::Game) -> PathBuf {
        self.saves_dir().join(format!("{}.osiris", sanitize(&game.world.scenario_name)))
    }

    fn quicksave(&mut self) {
        let Some(Screen::Playing(game, _)) = &self.screen else { return };
        let path = self.save_path(game);
        let result = game.world.save().map_err(anyhow::Error::msg).and_then(|b| Ok(std::fs::write(&path, b)?));
        let msg = match result {
            Ok(()) => format!("Saved {}", path.file_stem().unwrap_or_default().to_string_lossy()),
            Err(e) => format!("Save failed: {e}"),
        };
        self.status = Some((msg, 3.0));
    }

    fn quickload(&mut self) {
        let Some(Screen::Playing(game, _)) = &self.screen else { return };
        let path = self.save_path(game);
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

pub fn sanitize(name: &str) -> String {
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
            None => {
                let mut menu = self.menu();
                if self.family.is_empty() {
                    // No family chosen yet (or this is a fresh install): gate on the
                    // registry before the rest of the menu is reachable.
                    menu.open_page("family");
                }
                self.screen = Some(Screen::Menu(menu));
            }
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
                        Some(Screen::Menu(m)) if m.wants_text() => {
                            let text = match code {
                                KeyCode::Backspace => Some("\u{8}".to_owned()),
                                KeyCode::Enter | KeyCode::NumpadEnter => Some("\n".to_owned()),
                                _ => event.text.as_ref().map(|t| t.to_string()),
                            };
                            if let Some(t) = text {
                                m.type_family_name(&t);
                            }
                        }
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
        let mut failed = false;
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
                // Once the victory message has been read, go on to the next mission;
                // once the defeat has been read, back to try again.
                if game.world.won && game.idle() {
                    finished = Some(*mission);
                }
                if game.world.lost && game.idle() {
                    failed = true;
                }
            }
            None => {}
        }
        if failed {
            let mut menu = self.menu();
            menu.show_campaign();
            self.screen = Some(Screen::Menu(menu));
        }
        if let Some(mission) = finished {
            if let Some(m) = mission {
                let mut p = load_progress(&self.assets.campaign, &self.family);
                p.won(&self.assets.campaign, m);
                save_progress(&p, &self.family);
            }
            let mut menu = self.menu();
            if mission.is_some() {
                // The next mission, or the choice of the next city.
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

/// Puts the camera where the map's editor left it, or else over the entry point.
fn start_camera(r: &mut osiris_render::Renderer, game: &mut game::Game) {
    let map = &game.world.map;
    if let Some((a, b)) = game.world.start_corner {
        let origin = city_view::tile_to_world(map, 0, 0);
        r.camera.x = origin[0] + a as f32 * city_view::TILE_W / 2.0;
        r.camera.y = origin[1] + b as f32 * city_view::TILE_H / 2.0 - sidebar::TOP / r.camera.zoom;
    } else {
        let (cx, cy) = start_view(&game.world);
        game.view.center_on(r, map, cx, cy);
    }
}

/// Where the camera starts without a stored view: the scenario's entry point, or
/// the map centre.
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
        .and_then(|b| Campaign::parse(&String::from_utf8_lossy(&b)).ok())
        .unwrap_or_default();
    let pak = MissionPak::open(&data.join("mission1.pak"))?;
    let count = (0..pak.slots()).filter(|&i| pak.entry(i).is_some()).count();
    let names = campaign.mission_names.clone();
    let mission_names = (0..count)
        .map(|i| names.get(i).cloned().unwrap_or_else(|| format!("Mission {}", i + 1)))
        .collect();
    let phrases = Arc::new(osiris_formats::Phrases::parse(&String::from_utf8_lossy(&std::fs::read(data.join("eventmsg.txt")).unwrap_or_default())));
    Ok(Assets { data: data.to_owned(), defs, balance, text, messages, phrases, mission_names, campaign: Arc::new(campaign) })
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
    migrate_legacy_family();

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
            // A campaign part-way through, at its first choice of city.
            let c = &assets.campaign;
            let mut p = progress::Progress::new(c);
            while let progress::Next::Mission(m) = p.next {
                p.won(c, m);
            }
            let family = load_current_family();
            let mut menu = menu::Menu::new(
                assets.mission_names.clone(),
                campaign_view(&assets, &p),
                list_files(&assets.data.join("Maps"), "map"),
                vec![],
                Default::default(),
                family,
                family_text(&assets),
            );
            if let Some(page) = &view.menu_page {
                menu.open_page(page);
            }
            return gfx::screenshot(library, args.size, out, |r| menu.draw(r, &images.panels));
        }
        let mut game = game::Game::new(world, images, assets.text.clone(), assets.messages.clone(), None);
        game.phrases = assets.phrases.clone();
        game.player_name = player_name();
        let at = view.centre.or(view.info);
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
        if let Some((collapsing, step)) = view.slide {
            game.sidebar.show_slide(collapsing, step);
        }
        let zoom = view.zoom;
        let hover = view.hover;
        return gfx::screenshot(library, args.size, out, |r| {
            if let Some(z) = zoom {
                r.camera.zoom = z;
            }
            match at {
                Some((cx, cy)) => game.view.center_on(r, &game.world.map, cx, cy),
                None => start_camera(r, &mut game),
            }
            if let Some(p) = hover {
                // Negative coordinates count from the right or bottom edge.
                let p = [if p[0] < 0.0 { r.screen[0] + p[0] } else { p[0] }, if p[1] < 0.0 { r.screen[1] + p[1] } else { p[1] }];
                game.set_cursor(r, p);
            }
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
        family: load_current_family(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
