// Release builds on Windows run without a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod advisors;
mod anims;
mod popup;
mod army_view;
mod city_view;
mod data_dir;
mod disaster_view;
mod editor;
mod empire_window;
mod game;
mod gfx;
mod info;
mod menu;
mod message_list;
mod rules_panel;
mod sound_options;
mod minimap;
mod overlay;
mod mission_brief;
mod progress;
mod script;
mod sidebar;
mod tomb_view;
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
    /// The game data from `--data`; otherwise found or asked for at startup.
    data: Option<PathBuf>,
    map: Option<PathBuf>,
    mission: Option<usize>,
    screenshot: Option<PathBuf>,
    script: Option<String>,
    size: (u32, u32),
    /// Whether `--size` was actually typed: an explicit size (as every screenshot
    /// passes) overrides the player's saved window, which otherwise wins.
    size_given: bool,
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        data: None,
        map: None,
        mission: None,
        screenshot: None,
        script: None,
        size: (1280, 800),
        size_given: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().with_context(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--data" => args.data = Some(val()?.into()),
            "--map" => args.map = Some(val()?.into()),
            "--mission" => args.mission = Some(val()?.parse()?),
            "--screenshot" => args.screenshot = Some(val()?.into()),
            "--script" => args.script = Some(val()?),
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').context("--size WxH")?;
                args.size = (w.parse()?, h.parse()?);
                args.size_given = true;
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

/// Everything loaded once at startup and shared by every game.
struct Assets {
    data: PathBuf,
    defs: Arc<Defs>,
    balance: Arc<Balance>,
    balances: Arc<[Arc<Balance>; 5]>,
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

/// A new game of `source` at `difficulty`.
fn new_world(assets: &Assets, source: &Source, difficulty: u8) -> Result<World> {
    let (scenario, mission) = match source {
        Source::Map(path) => (Scenario::load_map(path)?, None),
        Source::Mission(n) => (MissionPak::open(&assets.data.join("mission1.pak"))?.scenario(*n)?, Some(*n)),
    };
    let mut world = World::new(&scenario, assets.defs.clone(), assets.balance.clone());
    world.attach_balances(assets.balances.clone());
    world.begin_at(difficulty);
    world.start(&scenario);
    if let Some(n) = mission {
        world.load_mission(n as i32);
    }
    Ok(world)
}

/// Saved games, the last written first, so Continue picks up the latest.
fn newest_first(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
    let written = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    v.sort_by_key(|p| std::cmp::Reverse(written(p)));
    v
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

/// Where the player's own maps go (the Mission Editor saves there).
fn maps_dir() -> PathBuf {
    user_dir().join("maps")
}

fn rules_path() -> PathBuf {
    user_dir().join("rules.toml")
}

/// The player's game rules, applied to every game.
fn load_rules() -> osiris_sim::Rules {
    std::fs::read_to_string(rules_path()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

/// The difficulty new games start at, which the player last chose (the original
/// keeps it in Pharaoh.inf).
fn load_difficulty() -> u8 {
    std::fs::read_to_string(user_dir().join("difficulty.txt")).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(osiris_sim::difficulty::NORMAL).min(osiris_sim::difficulty::IMPOSSIBLE)
}

/// Whether the city is saved automatically each month (the original's Autosave
/// option, on unless the player turned it off).
fn load_autosave() -> bool {
    std::fs::read_to_string(user_dir().join("autosave.txt")).map_or(true, |s| s.trim() != "off")
}

fn save_autosave(on: bool) {
    let _ = std::fs::write(user_dir().join("autosave.txt"), if on { "on\n" } else { "off\n" });
}

fn save_difficulty(d: u8) {
    let _ = std::fs::write(user_dir().join("difficulty.txt"), format!("{d}\n"));
}

/// The interface's own design size (see sidebar.rs, top_menu.rs, menu.rs): below
/// this the sidebar and top menu bar start folding up, so it is the smallest
/// sensible window.
const MIN_WINDOW: (u32, u32) = (1024, 768);

fn window_state_path() -> PathBuf {
    user_dir().join("window.txt")
}

/// The window as the player last left it: its windowed logical size (the size to
/// come back to once it isn't maximized or full screen), whether it was maximized,
/// and whether it was full screen. `None` on a fresh install, or if the file is
/// unreadable.
fn load_window_state() -> Option<((u32, u32), bool, bool)> {
    let s = std::fs::read_to_string(window_state_path()).ok()?;
    let mut it = s.split_whitespace();
    let w: u32 = it.next()?.parse().ok()?;
    let h: u32 = it.next()?.parse().ok()?;
    let maximized = it.next()? != "0";
    let fullscreen = it.next()? != "0";
    (w > 0 && h > 0).then_some(((w, h), maximized, fullscreen))
}

fn save_window_state(size: (u32, u32), maximized: bool, fullscreen: bool) {
    let _ = std::fs::write(window_state_path(), format!("{} {} {} {}\n", size.0, size.1, maximized as u8, fullscreen as u8));
}

/// `size` clamped to fit the event loop's primary monitor (its own logical size),
/// so a window saved on a bigger screen doesn't open off the edge of a smaller one.
fn clamp_to_monitor(size: (u32, u32), event_loop: &ActiveEventLoop) -> (u32, u32) {
    let Some(monitor) = event_loop.primary_monitor() else { return size };
    let logical: winit::dpi::LogicalSize<f64> = monitor.size().to_logical(monitor.scale_factor());
    (size.0.min(logical.width as u32).max(1), size.1.min(logical.height as u32).max(1))
}

/// A sensible window size when nothing is saved yet: the largest 4:3-or-wider size
/// that fits in about 85% of the monitor, never smaller than the interface's own
/// minimum. Falls back to that minimum if the monitor can't be found.
fn default_window_size(event_loop: &ActiveEventLoop) -> (u32, u32) {
    let Some(monitor) = event_loop.primary_monitor() else { return MIN_WINDOW };
    let logical: winit::dpi::LogicalSize<f64> = monitor.size().to_logical(monitor.scale_factor());
    let w = (logical.width * 0.85) as u32;
    let h = ((logical.height * 0.85) as u32).min(w * 3 / 4);
    (w.max(MIN_WINDOW.0), h.max(MIN_WINDOW.1))
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

/// The "Unlock All Missions" cheat (not in the original): plays the whole campaign
/// through on paper, taking the first choice at every fork, so every mission on that
/// route is marked won and can be started from the main menu.
fn unlock_all_missions(p: &mut progress::Progress, c: &Campaign) {
    loop {
        match p.next.clone() {
            progress::Next::Mission(m) => p.won(c, m),
            progress::Next::Choice(_) => {
                let Some(path) = p.choice(c).and_then(|(_, choices)| choices.first().map(|ch| ch.path_id)) else { break };
                p.choose(c, path);
            }
            progress::Next::End => break,
        }
    }
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
        delete_title: t(5, 90),
        delete_body: t(5, 91),
        exists_title: t(5, 92),
        exists_body: t(5, 93),
        none_title: t(5, 94),
        none_body: t(5, 95),
    }
}

/// What the menu shows of the family's campaign: the missions won and their results,
/// the period reached, and whether a city of it waits to be resumed. The choice of
/// city waiting is that of the campaign being played, `at`.
fn campaign_view(assets: &Assets, p: &progress::Progress, at: &progress::Progress) -> menu::CampaignView {
    let text = |id: u32| assets.text.get(144, id as usize).unwrap_or("").trim().to_string();
    let choice = at.choice(&assets.campaign).map(|(screen, choices)| menu::ChoiceView {
        map: menu::CHOICE_MAPS + screen.graphic_id,
        title: text(screen.title_text_id),
        prompt: text(0),
        points: choices.iter().map(|c| menu::ChoicePoint { x: c.x as f32, y: c.y as f32, label: text(c.text_id), path: c.path_id }).collect(),
    });
    menu::CampaignView {
        done: p.done.clone(),
        choice,
        results: p.results.clone(),
        period: p.period(&assets.campaign),
        resume: p.save.as_ref().is_some_and(|s| s.exists()),
    }
}

/// The first five missions teach the game, and their briefings name the tutorial's
/// first goal (text 62, FUN_004e2530 with nothing yet done).
const TUTORIAL_GOALS: [usize; 5] = [21, 24, 28, 33, 31];

/// Mission `m`'s briefing: its campaign.txt intro message and its goals.
fn briefing_view(assets: &Assets, m: usize, back: bool) -> menu::BriefingView {
    let intro = progress::mission_entry(&assets.campaign, m).map_or(200 + m, |e| e.intro_mm as usize);
    let msg = assets.messages.get(intro).cloned().unwrap_or_default();
    let name = assets.mission_names.get(m).cloned().unwrap_or_default();
    let brief = MissionPak::open(&assets.data.join("mission1.pak")).ok().and_then(|pak| pak.scenario(m).ok()).map(|s| mission_brief::Brief::new(name, &s)).unwrap_or_default();
    menu::BriefingView {
        mission: m,
        title: msg.title,
        subtitle: msg.subtitle,
        content: msg.content,
        brief,
        tutorial: TUTORIAL_GOALS.get(m).and_then(|&i| assets.text.get(62, i)).map(|s| s.trim().to_owned()),
        back,
    }
}

/// How the mission in play was started, which decides what follows a win.
#[derive(Debug, Clone)]
enum Run {
    /// The family history: a win moves the family's campaign on.
    History,
    /// A period played from Explore History: wins walk its missions and choices, the
    /// family's own campaign stays where it is.
    Replay(progress::Progress),
    /// A single mission from Explore History's list, a custom map or another save.
    Single,
}

enum Screen {
    Menu(Box<menu::Menu>),
    Playing(Box<game::Game>, Option<usize>),
    Editor(Box<editor::Editor>),
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
    /// Whether the cursor is over the window, which is focused: only then does the
    /// screen's edge scroll the city.
    cursor_in: bool,
    focused: bool,
    /// The player's sound settings, and the Sound options window open over the menu
    /// with a click on it waiting to be handled.
    sound: sound_options::SoundPrefs,
    sound_window: Option<sound_options::SoundWindow>,
    sound_click: Option<[f32; 2]>,
    keys: std::collections::HashSet<KeyCode>,
    last_frame: std::time::Instant,
    status: Option<(String, f32)>,
    /// A File-menu request the city made while drawing (a popup answered), carried
    /// out once the frame is done.
    drawn_request: Option<top_menu::MenuAction>,
    /// When the city was last saved automatically: its month, and the moment.
    autosaved: Option<(i32, std::time::Instant)>,
    /// The active family; empty until one is chosen (gating the menu on startup).
    family: String,
    run: Run,
    /// The city picked on the choice screen, taken once its briefing is read.
    pending_path: Option<u32>,
    /// The editor waiting while its map is played: leaving the game goes back to it.
    editor_waiting: Option<Box<editor::Editor>>,
    /// The window's logical size when neither maximized nor full screen, kept so
    /// leaving either state (and the next launch) has a size to come back to.
    windowed_size: (u32, u32),
}

impl App {
    /// Saves directory for the active family, or the pre-family default when none is
    /// chosen yet (used by `--map`/`--mission` direct launches, which skip the menu).
    fn saves_dir(&self) -> PathBuf {
        if self.family.is_empty() { legacy_saves_dir() } else { family_saves_dir(&self.family) }
    }

    fn menu(&self) -> Box<menu::Menu> {
        let campaign = if self.family.is_empty() {
            menu::CampaignView::default()
        } else {
            let p = load_progress(&self.assets.campaign, &self.family);
            match &self.run {
                Run::Replay(q) => campaign_view(&self.assets, &p, q),
                _ => campaign_view(&self.assets, &p, &p),
            }
        };
        let mut menu = Box::new(menu::Menu::new(
            self.assets.mission_names.clone(),
            campaign,
            list_files(&self.assets.data.join("Maps"), "map"),
            newest_first(list_files(&self.saves_dir(), "osiris")),
            load_rules(),
            self.family.clone(),
            family_text(&self.assets),
            self.assets.text.clone(),
            self.assets.data.clone(),
        ));
        menu.difficulty = load_difficulty();
        menu.editor_maps = editor::list_maps(&self.assets.data, &maps_dir());
        menu
    }

    /// Back to the main menu, or to the Mission Editor when its map was being played.
    fn back_to_menu(&mut self) {
        self.screen = Some(match self.editor_waiting.take() {
            Some(e) => Screen::Editor(e),
            None => Screen::Menu(self.menu()),
        });
    }

    /// Starts a custom map, which Replay can start again from its file.
    fn start_map(&mut self, path: PathBuf) {
        match new_world(&self.assets, &Source::Map(path.clone()), load_difficulty()) {
            Ok(world) => {
                self.start(world, None);
                if let Some(Screen::Playing(g, _)) = &mut self.screen {
                    g.replay_map = Some(path);
                }
            }
            Err(e) => self.status = Some((format!("Could not start: {e}"), 5.0)),
        }
    }

    /// Opens a map in the Mission Editor.
    fn edit(&mut self, path: &Path) {
        match editor::Editor::open(path, self.assets.defs.clone(), self.assets.text.clone(), maps_dir()) {
            Ok(e) => self.screen = Some(Screen::Editor(Box::new(e))),
            Err(e) => self.status = Some((format!("Could not open the map: {e}"), 5.0)),
        }
    }

    /// Carries out what the editor asked for.
    fn editor_request(&mut self) {
        let Some(Screen::Editor(e)) = &mut self.screen else { return };
        let Some(request) = e.request.take() else { return };
        match request {
            editor::Request::Exit => self.screen = Some(Screen::Menu(self.menu())),
            editor::Request::Open => {
                let mut menu = self.menu();
                menu.open_page("editor");
                self.screen = Some(Screen::Menu(menu));
            }
            editor::Request::Play(path) => {
                let Some(Screen::Editor(e)) = self.screen.take() else { return };
                self.editor_waiting = Some(e);
                self.run = Run::Single;
                self.start_map(path);
                if !matches!(self.screen, Some(Screen::Playing(..))) {
                    self.back_to_menu();
                }
            }
        }
    }

    fn start(&mut self, mut world: World, mission: Option<usize>) {
        let (Some(gfx), Some(images)) = (self.gfx.as_mut(), self.images.clone()) else { return };
        world.rules = load_rules();
        let mut game = game::Game::new(world, images, self.assets.text.clone(), self.assets.messages.clone(), self.audio.clone());
        game.phrases = self.assets.phrases.clone();
        game.player_name = player_name();
        game.victory_text = mission.and_then(|m| progress::mission_entry(&self.assets.campaign, m)).map_or(37, |e| e.victory_text as usize);
        game.set_autosave(load_autosave());
        game.sound_prefs = self.sound;
        self.autosaved = None;
        start_camera(&mut gfx.renderer, &mut game);
        self.screen = Some(Screen::Playing(Box::new(game), mission));
    }

    fn choose(&mut self, choice: menu::Choice, event_loop: &ActiveEventLoop) {
        let c = self.assets.campaign.clone();
        let result = match &choice {
            menu::Choice::Sound => {
                self.sound_window = Some(sound_options::SoundWindow);
                return;
            }
            menu::Choice::Quit => {
                event_loop.exit();
                return;
            }
            // Explore History's list: the mission on its own, after its briefing.
            menu::Choice::Mission(n) => {
                self.run = Run::Single;
                self.pending_path = None;
                self.brief(*n, false);
                return;
            }
            menu::Choice::Resume => {
                let p = load_progress(&c, &self.family);
                match p.save.as_deref().map(|s| load_game(&self.assets, s)) {
                    Some(Ok(w)) => {
                        self.run = Run::History;
                        let m = w.mission.as_ref().map(|m| m.id as usize);
                        Ok((w, m))
                    }
                    Some(Err(e)) => Err(e),
                    None => return,
                }
            }
            menu::Choice::Begin => {
                self.run = Run::History;
                self.step();
                return;
            }
            menu::Choice::Period(k) => {
                self.run = Run::Replay(progress::Progress::at_period(&c, *k));
                self.step();
                return;
            }
            menu::Choice::ToCity(m) => {
                // The city picked on the choice screen is only taken now.
                if let Some(path) = self.pending_path.take() {
                    match &mut self.run {
                        Run::History => {
                            let mut p = load_progress(&c, &self.family);
                            p.choose(&c, path);
                            save_progress(&p, &self.family);
                        }
                        Run::Replay(q) => q.choose(&c, path),
                        Run::Single => {}
                    }
                }
                new_world(&self.assets, &Source::Mission(*m), load_difficulty()).map(|w| (w, Some(*m)))
            }
            menu::Choice::Family(name) => {
                self.family = name.clone();
                choose_family(&self.family);
                self.screen = Some(Screen::Menu(self.menu()));
                return;
            }
            // A city is picked: its first mission's briefing, from which Cancel goes
            // back to the choice.
            menu::Choice::Path(path) => {
                let mut at = match &self.run {
                    Run::Replay(q) => q.clone(),
                    _ => load_progress(&c, &self.family),
                };
                at.choose(&c, *path);
                if let progress::Next::Mission(m) = at.next {
                    self.pending_path = Some(*path);
                    self.brief(m, true);
                } else {
                    self.screen = Some(Screen::Menu(self.menu()));
                }
                return;
            }
            menu::Choice::Map(p) => {
                self.run = Run::Single;
                self.start_map(p.clone());
                return;
            }
            menu::Choice::Edit(p) => {
                self.edit(&p.clone());
                return;
            }
            menu::Choice::Save(p) => load_game(&self.assets, p).map(|w| {
                let m = w.mission.as_ref().map(|m| m.id as usize);
                // A save of the family's next mission goes on with its history.
                let next = load_progress(&c, &self.family).next;
                self.run = if m.is_some_and(|m| next == progress::Next::Mission(m)) { Run::History } else { Run::Single };
                (w, m)
            }),
        };
        match result {
            Ok((world, mission)) => self.start(world, mission),
            Err(e) => self.status = Some((format!("Could not start: {e}"), 5.0)),
        }
    }

    /// Mission `m`'s briefing, before its city.
    fn brief(&mut self, m: usize, back: bool) {
        let mut menu = self.menu();
        menu.show_briefing(briefing_view(&self.assets, m, back));
        self.screen = Some(Screen::Menu(menu));
    }

    /// What comes next in the campaign being played: a mission's briefing, the choice
    /// of a city, or, at the end, the family's menu.
    fn step(&mut self) {
        let next = match &self.run {
            Run::Replay(q) => q.next.clone(),
            Run::History => load_progress(&self.assets.campaign, &self.family).next,
            Run::Single => progress::Next::End,
        };
        match next {
            progress::Next::Mission(m) => self.brief(m, false),
            progress::Next::Choice(_) => {
                let mut menu = self.menu();
                menu.show_choice();
                self.screen = Some(Screen::Menu(menu));
            }
            progress::Next::End => self.screen = Some(Screen::Menu(self.menu())),
        }
    }

    /// The mission is played again from its start, as the same kind of play.
    fn restart(&mut self, m: usize) {
        match new_world(&self.assets, &Source::Mission(m), load_difficulty()) {
            Ok(world) => self.start(world, Some(m)),
            Err(e) => self.status = Some((format!("Could not start: {e}"), 5.0)),
        }
    }

    /// The victory has been read. As in the original (FUN_00418640), the family keeps
    /// the best result of any campaign mission won. The family history moves on to the
    /// next mission or choice of the period; when the period is won, the campaign
    /// window offers the next one, except after the New Kingdom (where the original
    /// plays its closing film) and at the very end, which go back to the family's
    /// menu. A period played from Explore History walks its missions the same way and
    /// ends at the menu; a single mission goes straight back.
    fn mission_won(&mut self, mission: Option<(usize, osiris_sim::ratings::MissionResult)>) {
        let c = self.assets.campaign.clone();
        let Some((m, result)) = mission else {
            self.back_to_menu();
            return;
        };
        let mut p = load_progress(&c, &self.family);
        p.record(m, result);
        if !self.family.is_empty() {
            if matches!(self.run, Run::History) {
                let before = p.period(&c);
                p.won(&c, m);
                p.save = None;
                save_progress(&p, &self.family);
                let now = p.period(&c);
                if now != before {
                    let mut menu = self.menu();
                    if now != 5 && now < c.sections.len() {
                        menu.show_periods(now, true);
                    }
                    self.screen = Some(Screen::Menu(menu));
                    return;
                }
            } else {
                save_progress(&p, &self.family);
            }
        }
        match &mut self.run {
            Run::History => self.step(),
            Run::Replay(q) => {
                let before = q.period(&c);
                q.won(&c, m);
                if q.period(&c) == before {
                    self.step();
                } else {
                    self.run = Run::Single;
                    self.screen = Some(Screen::Menu(self.menu()));
                }
            }
            Run::Single => self.screen = Some(Screen::Menu(self.menu())),
        }
    }

    /// Carries out a File-menu choice made in a running game.
    fn menu_request(&mut self, request: top_menu::MenuAction, event_loop: &ActiveEventLoop) {
        use top_menu::MenuAction;
        let (mission, map) = match &self.screen {
            Some(Screen::Playing(g, m)) => (*m, g.replay_map.clone()),
            _ => (None, None),
        };
        match request {
            MenuAction::Save => self.quicksave(),
            MenuAction::Quit => event_loop.exit(),
            MenuAction::Load => {
                let mut menu = self.menu();
                menu.open_page("load");
                self.screen = Some(Screen::Menu(menu));
            }
            MenuAction::Replay => match (mission, map) {
                (Some(n), _) => self.restart(n),
                (None, Some(p)) if p.exists() => self.start_map(p),
                _ => self.status = Some(("Only campaign missions and maps can be replayed".into(), 3.0)),
            },
            _ => self.back_to_menu(),
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
        self.saved_history(path);
    }

    /// A city of the family history was saved: "Resume Family History" loads it, as
    /// the original keeps the history's save in the player file.
    fn saved_history(&self, path: PathBuf) {
        if !matches!(self.run, Run::History) || self.family.is_empty() {
            return;
        }
        let mut p = load_progress(&self.assets.campaign, &self.family);
        if p.save.as_ref() != Some(&path) {
            p.save = Some(path);
            save_progress(&p, &self.family);
        }
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

    /// Alt+Enter, F11, or Ctrl+Cmd+F on macOS: borderless full screen on whichever
    /// monitor the window is already on, from any screen. The Options menu's own
    /// Fullscreen entry (only reachable in a running game) goes through
    /// `game::Game::fullscreen_changed` instead, since the window lives here, not
    /// with the game.
    fn toggle_fullscreen(&mut self) {
        gfx::set_fullscreen(!gfx::fullscreen());
        let Some(gfx) = &self.gfx else { return };
        let on = gfx::fullscreen();
        gfx.window.set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
        if !on {
            // A window that opened straight into full screen (a fresh install's
            // first launch) has no windowed frame of its own to fall back to.
            let _ = gfx.window.request_inner_size(winit::dpi::LogicalSize::new(self.windowed_size.0, self.windowed_size.1));
        }
        let maximized = gfx.window.is_maximized();
        save_window_state(self.windowed_size, maximized, on);
        if let Some(Screen::Playing(g, _)) = &mut self.screen {
            g.sync_fullscreen_label();
        }
    }
}

/// The original saves the city to last.sav as each month begins while Autosave is
/// on. Osiris keeps one autosave per city, beside its saved game; at high speeds
/// it saves at most every few seconds, and writes the file off the main thread.
fn autosave(game: &game::Game, saves: &Path, last: &mut Option<(i32, std::time::Instant)>) -> Option<PathBuf> {
    let w = &game.world;
    let month = w.time.year * 12 + w.time.month as i32;
    let now = std::time::Instant::now();
    match *last {
        None => {
            *last = Some((month, now));
            None
        }
        Some((m, t)) if m != month && now - t >= std::time::Duration::from_secs(3) => {
            *last = Some((month, now));
            if !game.autosave || w.won || w.lost {
                return None;
            }
            let bytes = w.save().ok()?;
            let path = autosave_path(saves, &w.scenario_name);
            let written = path.clone();
            std::thread::spawn(move || {
                let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
                let tmp = path.with_extension("tmp");
                if std::fs::write(&tmp, bytes).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            });
            Some(written)
        }
        Some(_) => None,
    }
}

fn autosave_path(saves: &Path, scenario: &str) -> PathBuf {
    saves.join(format!("{} autosave.osiris", sanitize(scenario)))
}

pub fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    s.trim().to_owned()
}

fn load_game(assets: &Assets, path: &Path) -> Result<World> {
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    let mut world = World::load(&bytes, assets.defs.clone(), assets.balance.clone()).map_err(anyhow::Error::msg)?;
    world.attach_balances(assets.balances.clone());
    // As in the original, a saved game brings back its difficulty, and the player
    // goes on at it.
    save_difficulty(world.difficulty);
    Ok(world)
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        // An explicit --size (as every screenshot passes) wins outright; otherwise
        // the player's saved window comes back, clamped to the monitor it's on now,
        // or else a size that fits this monitor comfortably.
        let (size, maximized, fullscreen) = if self.args.size_given {
            (self.args.size, false, false)
        } else if let Some((size, maximized, fullscreen)) = load_window_state() {
            (clamp_to_monitor(size, _event_loop), maximized, fullscreen)
        } else {
            // A fresh install opens full screen; the size that fits the monitor
            // becomes the windowed size to come back to once the player turns full
            // screen off (there's no windowed session of its own to remember one).
            (default_window_size(_event_loop), false, true)
        };
        self.windowed_size = size;
        gfx::set_fullscreen(fullscreen);
        let mut attrs = Window::default_attributes()
            .with_title("Osiris")
            .with_inner_size(winit::dpi::LogicalSize::new(size.0, size.1))
            .with_min_inner_size(winit::dpi::LogicalSize::new(MIN_WINDOW.0, MIN_WINDOW.1))
            .with_maximized(maximized && !fullscreen);
        if fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        let started = (|| -> Result<_> {
            let window = _event_loop.create_window(attrs).context("could not open a window")?;
            let library = self.library.take().context("the game art was already taken")?;
            let images = sidebar::SidebarImages::load(&library)?;
            let gfx = pollster::block_on(gfx::Gfx::new(window, library))?;
            Ok((images, gfx))
        })();
        let (images, gfx) = match started {
            Ok(v) => v,
            Err(e) => {
                log::error!("{e:#}");
                data_dir::show_error(&format!("Osiris could not start its graphics.\n\n{e:#}"));
                _event_loop.exit();
                return;
            }
        };
        self.images = Some(images);
        self.gfx = Some(gfx);
        let direct = if let Some(map) = &self.args.map {
            Some((Source::Map(map.clone()), None))
        } else {
            self.args.mission.map(|n| (Source::Mission(n), Some(n)))
        };
        match direct {
            Some((Source::Map(p), _)) => {
                self.start_map(p);
                if self.screen.is_none() {
                    self.screen = Some(Screen::Menu(self.menu()));
                }
            }
            Some((source, mission)) => match new_world(&self.assets, &source, load_difficulty()) {
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
        let Some(scale) = self.gfx.as_ref().map(|g| g.scale()) else { return };
        let at = [(self.cursor.0 / scale) as f32, (self.cursor.1 / scale) as f32];
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.gfx {
                    gfx.resize(size.width, size.height);
                    let maximized = gfx.window.is_maximized();
                    let fullscreen = gfx.window.fullscreen().is_some();
                    // Only a plain resize (not a maximize or a fullscreen switch)
                    // changes the size to come back to.
                    if !maximized && !fullscreen {
                        let logical: winit::dpi::LogicalSize<u32> = size.to_logical(gfx.window.scale_factor());
                        self.windowed_size = (logical.width.max(1), logical.height.max(1));
                    }
                    save_window_state(self.windowed_size, maximized, fullscreen);
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
                // The fullscreen toggle works everywhere (menus, the city, the
                // editor), so it is checked before anything screen-specific.
                let alt = [KeyCode::AltLeft, KeyCode::AltRight].iter().any(|k| self.keys.contains(k));
                let ctrl = [KeyCode::ControlLeft, KeyCode::ControlRight].iter().any(|k| self.keys.contains(k));
                let cmd = [KeyCode::SuperLeft, KeyCode::SuperRight].iter().any(|k| self.keys.contains(k));
                match code {
                    KeyCode::F5 => self.quicksave(),
                    KeyCode::F9 => self.quickload(),
                    KeyCode::F11 => self.toggle_fullscreen(),
                    KeyCode::Enter | KeyCode::NumpadEnter if alt => self.toggle_fullscreen(),
                    KeyCode::KeyF if cfg!(target_os = "macos") && ctrl && cmd => self.toggle_fullscreen(),
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
                        Some(Screen::Editor(e)) if e.wants_text() => {
                            let text = match code {
                                KeyCode::Backspace => Some("\u{8}".to_owned()),
                                KeyCode::Enter | KeyCode::NumpadEnter => Some("\n".to_owned()),
                                KeyCode::Escape => {
                                    e.cancel();
                                    None
                                }
                                _ => event.text.as_ref().map(|t| t.to_string()),
                            };
                            if let Some(t) = text {
                                e.type_text(&t);
                            }
                        }
                        Some(Screen::Editor(e)) => {
                            let alt = [KeyCode::AltLeft, KeyCode::AltRight].iter().any(|k| self.keys.contains(k));
                            let key = match code {
                                KeyCode::KeyH => "h",
                                KeyCode::KeyZ => "z",
                                KeyCode::KeyD => "d",
                                KeyCode::Escape => "escape",
                                _ => "",
                            };
                            e.key(key, alt);
                        }
                        // The original's cheat box (Ctrl+Alt+C): typed keys go to it
                        // while it's open, as the family name box does in the menu.
                        Some(Screen::Playing(g, _)) if g.wants_cheat_text() => {
                            let text = match code {
                                KeyCode::Backspace => Some("\u{8}".to_owned()),
                                KeyCode::Enter | KeyCode::NumpadEnter => Some("\n".to_owned()),
                                KeyCode::Escape => {
                                    g.cancel_cheat_entry();
                                    None
                                }
                                _ => event.text.as_ref().map(|t| t.to_string()),
                            };
                            if let Some(t) = text {
                                g.type_cheat_text(&t);
                            }
                        }
                        Some(Screen::Playing(g, _)) if code == KeyCode::Escape && g.idle() => g.ask_to_leave(top_menu::MenuAction::MainMenu),
                        Some(Screen::Playing(g, _)) => {
                            let ctrl = [KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]
                                .iter()
                                .any(|k| self.keys.contains(k));
                            let alt = [KeyCode::AltLeft, KeyCode::AltRight].iter().any(|k| self.keys.contains(k));
                            if ctrl && alt && code == KeyCode::KeyC {
                                g.toggle_cheat_entry();
                            } else {
                                key_pressed(g, code, ctrl);
                            }
                        }
                        _ => {}
                    },
                }
                // "Unlock All Missions" (not in the original): touches the saved
                // campaign progress, which lives with the app, not the world.
                if let Some(Screen::Playing(g, _)) = &mut self.screen
                    && std::mem::take(&mut g.cheat_unlock_missions)
                {
                    let mut p = load_progress(&self.assets.campaign, &self.family);
                    unlock_all_missions(&mut p, &self.assets.campaign);
                    save_progress(&p, &self.family);
                    self.status = Some(("All missions unlocked".to_owned(), 3.0));
                }
            }
            WindowEvent::CursorEntered { .. } => self.cursor_in = true,
            WindowEvent::CursorLeft { .. } => self.cursor_in = false,
            WindowEvent::Focused(f) => {
                self.focused = f;
                if !f {
                    self.keys.clear();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_in = true;
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
                    Some(Screen::Editor(e)) => e.set_cursor(&gfx.renderer, at),
                    None => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(gfx) = &mut self.gfx else { return };
                let screen = gfx.renderer.screen;
                let mut chosen = None;
                let mut request = None;
                match &mut self.screen {
                    Some(Screen::Menu(_)) if self.sound_window.is_some() => match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => self.sound_click = Some(at),
                        (MouseButton::Right, ElementState::Pressed) => self.sound_window = None,
                        _ => {}
                    },
                    Some(Screen::Menu(m)) => match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            chosen = m.click(screen, at);
                            if std::mem::take(&mut m.rules_changed) {
                                save_rules(&m.rules);
                            }
                            if std::mem::take(&mut m.difficulty_changed) {
                                save_difficulty(m.difficulty);
                            }
                        }
                        (MouseButton::Left, ElementState::Released) => m.release(),
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
                    Some(Screen::Editor(e)) => match (button, state) {
                        (MouseButton::Left, ElementState::Pressed) => {
                            if let Some((x, y)) = e.press(&gfx.renderer, at) {
                                e.centre_on(&mut gfx.renderer, x, y);
                            }
                        }
                        (MouseButton::Left, ElementState::Released) => e.release(),
                        (MouseButton::Right, ElementState::Pressed) => {
                            e.cancel();
                            self.drag = Some(self.cursor);
                        }
                        (MouseButton::Middle, ElementState::Pressed) => self.drag = Some(self.cursor),
                        (_, ElementState::Released) => self.drag = None,
                        _ => {}
                    },
                    None => {}
                }
                self.editor_request();
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
                    Some(Screen::Editor(_)) => city_view::zoom_at(&mut gfx.renderer, at, 1.1f32.powf(steps)),
                    None => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                if let Some(r) = self.drawn_request.take() {
                    self.menu_request(r, event_loop);
                }
            }
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
        let saves = self.saves_dir();
        let Some(gfx) = &mut self.gfx else { return };
        let mut finished: Option<Option<(usize, osiris_sim::ratings::MissionResult)>> = None;
        let mut autosaved = None;
        let mut lost_choice: Option<(top_menu::MenuAction, Option<usize>, Option<PathBuf>)> = None;
        match &mut self.screen {
            Some(Screen::Menu(m)) => {
                let panels = self.images.as_ref().map(|i| &i.panels);
                let (sound, window, click, audio) = (&mut self.sound, &mut self.sound_window, self.sound_click.take(), self.audio.as_deref());
                let cursor = [(self.cursor.0 / gfx.scale()) as f32, (self.cursor.1 / gfx.scale()) as f32];
                let text = self.assets.text.clone();
                gfx.frame(|r| {
                    if let Some(p) = panels {
                        m.draw(r, p);
                        if let Some(w) = window {
                            let img = crate::widgets::UiImages::load(&r.library).expect("ui images");
                            match w.draw(r, p, img, &text, cursor, click, sound, audio) {
                                sound_options::Outcome::Open => {}
                                sound_options::Outcome::Changed => sound.save(&user_dir()),
                                sound_options::Outcome::Closed => *window = None,
                            }
                        }
                    }
                    if let Some(s) = &status {
                        osiris_ui::draw_text(r, osiris_ui::Font::NormalYellow, s, 20.0, 20.0, osiris_ui::font::WHITE);
                    }
                });
            }
            Some(Screen::Playing(game, mission)) => {
                let pan = 900.0 * dt / gfx.renderer.camera.zoom;
                let screen = gfx.renderer.screen;
                let scale = gfx.scale();
                let cam = &mut gfx.renderer.camera;
                // The cursor at the screen's edge scrolls as the arrow keys do. A window
                // that isn't full screen gets a wider band, since the cursor can slip past it.
                let (cx, cy) = (self.cursor.0 / scale, self.cursor.1 / scale);
                let band = if gfx.window.fullscreen().is_some() { 2.0 } else { 8.0 };
                let edges = self.cursor_in && self.focused && self.drag.is_none();
                let edge = |k: KeyCode| {
                    edges && match k {
                        KeyCode::ArrowLeft => cx < band,
                        KeyCode::ArrowRight => cx >= screen[0] as f64 - band,
                        KeyCode::ArrowUp => cy < band,
                        _ => cy >= screen[1] as f64 - band,
                    }
                };
                for (k, dx, dy) in [
                    (KeyCode::ArrowLeft, -1.0, 0.0),
                    (KeyCode::ArrowRight, 1.0, 0.0),
                    (KeyCode::ArrowUp, 0.0, -1.0),
                    (KeyCode::ArrowDown, 0.0, 1.0),
                ] {
                    if self.keys.contains(&k) || edge(k) {
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
                if std::mem::take(&mut game.difficulty_changed) {
                    save_difficulty(game.world.difficulty);
                }
                if std::mem::take(&mut game.sound_changed) {
                    self.sound = game.sound_prefs;
                    self.sound.save(&user_dir());
                }
                if std::mem::take(&mut game.autosave_changed) {
                    save_autosave(game.autosave);
                }
                if std::mem::take(&mut game.fullscreen_changed) {
                    let on = gfx::fullscreen();
                    gfx.window.set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
                    if !on {
                        let _ = gfx.window.request_inner_size(winit::dpi::LogicalSize::new(self.windowed_size.0, self.windowed_size.1));
                    }
                    save_window_state(self.windowed_size, gfx.window.is_maximized(), on);
                }
                autosaved = autosave(game, &saves, &mut self.autosaved);
                // Once the victory message has been read, go on to the next mission.
                if game.world.won && game.idle() {
                    finished = Some(mission.map(|m| (m, game.world.mission_result())));
                }
                // A lost mission waits on its screen's New Game or Replay mission.
                if game.world.lost
                    && let Some(a) = game.request.take()
                {
                    lost_choice = Some((a, *mission, game.replay_map.clone()));
                }
                if let Some(a) = game.request.take() {
                    self.drawn_request = Some(a);
                }
            }
            Some(Screen::Editor(e)) => {
                // The arrow keys and the screen's edge scroll the map, as in the city.
                let pan = 900.0 * dt / gfx.renderer.camera.zoom;
                let screen = gfx.renderer.screen;
                let scale = gfx.scale();
                let (cx, cy) = (self.cursor.0 / scale, self.cursor.1 / scale);
                let edges = self.cursor_in && self.focused && self.drag.is_none();
                for (k, dx, dy, at_edge) in [
                    (KeyCode::ArrowLeft, -1.0, 0.0, cx < 8.0),
                    (KeyCode::ArrowRight, 1.0, 0.0, cx >= screen[0] as f64 - 8.0),
                    (KeyCode::ArrowUp, 0.0, -1.0, cy < 8.0),
                    (KeyCode::ArrowDown, 0.0, 1.0, cy >= screen[1] as f64 - 8.0),
                ] {
                    if self.keys.contains(&k) || (edges && at_edge) {
                        gfx.renderer.camera.x += dx * pan;
                        gfx.renderer.camera.y += dy * pan;
                    }
                }
                e.update(dt);
                let panels = self.images.as_ref().map(|i| i.panels.clone());
                gfx.frame(|r| {
                    if let Some(p) = &panels {
                        e.draw(r, p);
                    }
                    if let Some(s) = &status {
                        osiris_ui::draw_text(r, osiris_ui::Font::NormalYellow, s, 20.0, 50.0, osiris_ui::font::WHITE);
                    }
                });
            }
            None => {}
        }
        self.editor_request();
        match lost_choice {
            Some((top_menu::MenuAction::Replay, Some(n), _)) => match new_world(&self.assets, &Source::Mission(n), load_difficulty()) {
                Ok(world) => self.start(world, Some(n)),
                Err(e) => self.status = Some((format!("Could not start: {e}"), 5.0)),
            },
            Some((top_menu::MenuAction::Replay, None, Some(p))) => self.start_map(p),
            Some(_) => self.back_to_menu(),
            None => {}
        }
        if let Some(path) = autosaved {
            self.saved_history(path);
        }
        if let Some(mission) = finished {
            self.mission_won(mission);
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
        (KeyCode::KeyR, false) => g.rotate_statue(),
        (KeyCode::KeyR, true) => g.turn_statue(),
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

/// The model files of difficulty `d` (as spelled in their names).
fn load_balance(data: &Path, d: &str) -> Result<Balance> {
    let model_path = data.join(format!("Pharaoh_Model_{d}.txt"));
    let model_text = std::fs::read(&model_path).with_context(|| model_path.display().to_string())?;
    let mut balance = Balance::from_model(&Model::parse(&String::from_utf8_lossy(&model_text))?);
    if let Ok(t) = std::fs::read(data.join(format!("Tax_Sentiment_Model_{d}.txt"))) {
        balance.tax_sentiment = osiris_formats::model::parse_tax_sentiment(&String::from_utf8_lossy(&t));
    }
    let figures = data.join(format!("Figure_model_{}.txt", d.to_lowercase()));
    if let Ok(t) = std::fs::read(figures).or_else(|_| std::fs::read(data.join("Figure_model.txt"))) {
        balance.set_units(&osiris_formats::model::parse_figures(&String::from_utf8_lossy(&t))?);
    }
    Ok(balance)
}

fn load_assets(data: &Path, library: &ImageLibrary) -> Result<Assets> {
    let defs = Arc::new(Defs::load(library).map_err(anyhow::Error::msg)?);
    let balances: Vec<Arc<Balance>> = osiris_sim::difficulty::FILE_NAMES.iter().map(|d| load_balance(data, d).map(Arc::new)).collect::<Result<_>>()?;
    let balances: Arc<[Arc<Balance>; 5]> = Arc::new(balances.try_into().expect("five difficulties"));
    let balance = balances[osiris_sim::difficulty::NORMAL as usize].clone();
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
    Ok(Assets { data: data.to_owned(), defs, balance, balances, text, messages, phrases, mission_names, campaign: Arc::new(campaign) })
}

/// The log of the last windowed run, in the user folder: a Windows build has no
/// console, so warnings and a crash's report go here for the player to send in.
fn log_path() -> PathBuf {
    user_dir().join("osiris.log")
}

/// Sends the log to [`log_path`] when there is no console to read it in, and turns a
/// panic into a message box plus a line in that log instead of a silent exit.
fn init_logging(headless: bool) {
    let to_file = cfg!(windows) && !headless;
    let mut logger = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(if to_file { "warn,osiris=info" } else { "warn" }));
    if to_file && let Ok(f) = std::fs::File::create(log_path()) {
        logger.target(env_logger::Target::Pipe(Box::new(f)));
    }
    logger.init();
    if headless {
        return;
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        let what = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        log::error!("crashed{at}: {what}");
        default(info);
        data_dir::show_error(&format!("Osiris crashed{at}:\n\n{what}\n\nThe details are in {}.", log_path().display()));
    }));
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let headless = args.screenshot.is_some();
    init_logging(headless);
    log::info!("Osiris {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    // Screenshots keep the automatic size so they don't depend on the player's choice.
    if !headless {
        gfx::load_ui_size();
    }
    let result = run(args);
    // Launched by a double-click there is no console to read an error in.
    if let Err(e) = &result
        && !headless
    {
        log::error!("{e:#}");
        data_dir::show_error(&format!("{e:#}"));
    }
    result
}

fn run(mut args: Args) -> Result<()> {
    let data = match args.data.take() {
        Some(d) => d,
        None => match data_dir::find(&user_dir()) {
            Some(d) => d,
            None if args.screenshot.is_some() => PathBuf::from("PharaohData"),
            None => match data_dir::pick(&user_dir()) {
                Some(d) => d,
                None => return Ok(()),
            },
        },
    };
    let args = Args { data: Some(data.clone()), ..args };
    let library = ImageLibrary::open(&data.join("Data")).with_context(|| {
        format!(
            "Could not read the Pharaoh game data at {}. Choose the folder Pharaoh is installed in \
             (the GOG or Steam Pharaoh + Cleopatra), or pass --data <dir>.",
            data.display()
        )
    })?;
    let assets = load_assets(&data, &library)?;
    migrate_legacy_family();

    if let Some(out) = &args.screenshot {
        // The Mission Editor's own steps (editor/script.rs): its screen, or the city
        // started from the map it saves to play.
        let mut play_map = None;
        if let Some(steps) = args.script.as_deref().filter(|s| editor::script::is_editor_script(s)) {
            let start = editor::script::map_path(&data, editor::DEFAULT_MAP);
            let mut e = editor::Editor::open(&start, assets.defs.clone(), assets.text.clone(), maps_dir())?;
            let done = e.run_script(&data, steps)?;
            match done.play {
                Some(p) => play_map = Some(p),
                None => {
                    let images = sidebar::SidebarImages::load(&library)?;
                    return gfx::screenshot(library, args.size, out, |r| {
                        if let Some(z) = done.zoom {
                            r.camera.zoom = z;
                        }
                        if let Some((x, y)) = done.view {
                            e.centre_on(r, x, y);
                        }
                        if done.hover.is_some() {
                            e.set_hover(done.hover);
                        }
                        e.draw(r, &images.panels);
                    });
                }
            }
        }
        let source = match (&play_map, &args.map) {
            (Some(p), _) => Source::Map(p.clone()),
            (None, Some(m)) => Source::Map(m.clone()),
            (None, None) => Source::Mission(args.mission.unwrap_or(0)),
        };
        let mut world = new_world(&assets, &source, osiris_sim::difficulty::NORMAL)?;
        let view = match &args.script {
            Some(s) if play_map.is_none() => script::run_script(&mut world, s)?,
            _ => script::ScriptView { keep_dialogs: true, ..Default::default() },
        };
        let images = sidebar::SidebarImages::load(&library)?;
        if view.menu {
            // A campaign part-way through: `menuwins` steps into it (each a mission won
            // or the first city chosen), or else up to its first choice of city.
            let c = &assets.campaign;
            let mut p = progress::Progress::new(c);
            match view.menu_wins {
                Some(n) => {
                    for _ in 0..n {
                        match p.next {
                            progress::Next::Mission(m) => p.won(c, m),
                            progress::Next::Choice(_) => {
                                let path = p.choice(c).and_then(|(_, ch)| ch.first()).map_or(0, |x| x.path_id);
                                p.choose(c, path);
                            }
                            progress::Next::End => break,
                        }
                    }
                }
                None => {
                    while let progress::Next::Mission(m) = p.next {
                        p.won(c, m);
                    }
                }
            }
            let family = load_current_family();
            let saves = if family.is_empty() { vec![] } else { list_files(&family_saves_dir(&family), "osiris") };
            let mut menu = menu::Menu::new(
                assets.mission_names.clone(),
                campaign_view(&assets, &p, &p),
                list_files(&assets.data.join("Maps"), "map"),
                saves,
                Default::default(),
                family,
                family_text(&assets),
                assets.text.clone(),
                assets.data.clone(),
            );
            menu.difficulty = load_difficulty();
            menu.editor_maps = editor::list_maps(&assets.data, &maps_dir());
            if let Some(page) = &view.menu_page {
                menu.open_page(page);
            }
            if let Some(k) = view.menu_period {
                menu.show_periods(k, false);
                if view.menu_page.as_deref() == Some("history") {
                    menu.open_page("history");
                    menu.pick_period(k);
                }
            }
            if let Some(m) = view.menu_brief {
                menu.show_briefing(briefing_view(&assets, m, view.menu_page.as_deref() == Some("choice")));
            }
            if let Some(i) = view.menu_pick {
                menu.pick_row(i);
            }
            if view.menu_results {
                menu.show_prior_results(true);
            }
            return gfx::screenshot(library, args.size, out, |r| menu.draw(r, &images.panels));
        }
        let mut game = game::Game::new(world, images, assets.text.clone(), assets.messages.clone(), None);
        game.replay_map = match source {
            Source::Map(p) => Some(p),
            Source::Mission(_) => None,
        };
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
            e.show(match (view.empire_popup.as_deref(), city) {
                (Some("confirm"), Some(c)) => Some(empire_window::EmpirePopup::Confirm(c)),
                (Some("nomoney"), _) => Some(empire_window::EmpirePopup::NoMoney),
                (Some("opened"), Some(c)) => Some(empire_window::EmpirePopup::Opened(c)),
                _ => None,
            });
            game.empire = Some(e);
        }
        if view.sound {
            game.sound_window = Some(sound_options::SoundWindow);
        }
        if view.difficulty {
            game.difficulty_panel = true;
        }
        if view.leave {
            game.ask_to_leave(top_menu::MenuAction::MainMenu);
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
        if let Some((k, tile)) = view.hold {
            game.hold_tool(k, tile);
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

    let audio = osiris_audio::Audio::new(args.data.as_deref().unwrap_or(Path::new("PharaohData"))).ok().map(Arc::new);
    let sound = sound_options::SoundPrefs::load(&user_dir());
    if let Some(a) = &audio {
        sound.apply(a);
    }
    let event_loop = EventLoop::new()?;
    let initial_size = args.size;
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
        cursor_in: false,
        focused: true,
        sound,
        sound_window: None,
        sound_click: None,
        keys: Default::default(),
        last_frame: std::time::Instant::now(),
        status: None,
        drawn_request: None,
        autosaved: None,
        family: load_current_family(),
        run: Run::Single,
        pending_path: None,
        editor_waiting: None,
        // Replaced in `resumed`, once the window (and its monitor) exists.
        windowed_size: initial_size,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
