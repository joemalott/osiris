//! Script steps for the Mission Editor in the headless harness (`--script`), so its
//! screens can be screenshotted:
//!
//! - `editor [MAP]` opens a map (a name in the game's Maps folder, or a path;
//!   Default.map when none is given), `editopen MAP` likewise;
//! - `editnew N` makes a new map, N a size (0-5) or a width (56 ... 226);
//! - `edittool NAME [N]`, `editbrush N`, `editpaint NAME x,y [BRUSH]` pick a tool and
//!   use it (grass, trees, water, floodplain, marsh, meadow, rock, ore, cliff, dunes,
//!   and the points entry, exit, invasion N, riverin, riverout, fishing N, killer N,
//!   prey N, disembark N);
//! - `editroad x,y x,y` lays a road; `editrefresh` is Refresh Map;
//! - `editclimate N` sets the terrain set (0 humid, 1 normal, 2 arid);
//! - `editmenu N` opens tool button N's submenu, `edittop N` a menu of the bar,
//!   `editpopup sizes|save`, `editoptions [PAGE]` the Options screen (main,
//!   starting, date, win, monuments, allowed, gods, flood), `editchooser PAGE WHAT`
//!   with a value's list or keypad open over it;
//! - `editview x,y` centres the view on a tile, `editzoom Z` zooms, `edithover x,y`
//!   puts the mouse there;
//! - `editsave PATH` saves the map; `editplay` saves it and screenshots the city
//!   started from it instead.

use super::options::{Options, Page};
use super::terrain::Paint;
use super::view::Popup;
use super::{Editor, MAP_SIZES, Point, Tool};
use anyhow::{Context, Result, bail};
use osiris_sim::map::terrain as bits;
use std::path::{Path, PathBuf};

/// What the script leaves for the screenshot.
#[derive(Default)]
pub struct Outcome {
    /// A tile to centre the view on.
    pub view: Option<(i32, i32)>,
    /// A tile the mouse rests on.
    pub hover: Option<(i32, i32)>,
    /// The map saved to play: the screenshot shows its city.
    pub play: Option<PathBuf>,
    /// The camera's zoom.
    pub zoom: Option<f32>,
}

fn point(s: &str) -> Result<(i32, i32)> {
    let (x, y) = s.split_once(',').context("expected x,y")?;
    Ok((x.trim().parse()?, y.trim().parse()?))
}

/// The map named `name`: a path, or a name in the game's Maps folder.
pub fn map_path(data: &Path, name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.exists() {
        return p;
    }
    let file = if name.to_ascii_lowercase().ends_with(".map") { name.to_owned() } else { format!("{name}.map") };
    data.join("Maps").join(file)
}

fn tool(name: &str, n: u8) -> Result<Tool> {
    Ok(match name {
        "grass" | "land" => Tool::Paint(Paint::Grass),
        "trees" => Tool::Paint(Paint::Trees),
        "water" => Tool::Paint(Paint::Water),
        "floodplain" => Tool::Paint(Paint::Floodplain),
        "marsh" | "marshland" => Tool::Paint(Paint::Marshland),
        "meadow" => Tool::Paint(Paint::Meadow),
        "rock" => Tool::Paint(Paint::Rock(bits::ROCK)),
        "ore" => Tool::Paint(Paint::Rock(bits::ROCK | bits::ORE)),
        "cliff" => Tool::Paint(Paint::Rock(bits::ROCK | bits::CLIFF)),
        "dunes" => Tool::Paint(Paint::Dunes),
        "road" => Tool::Road,
        "entry" => Tool::Point(Point::Entry),
        "exit" => Tool::Point(Point::Exit),
        "invasion" => Tool::Point(Point::Invasion(n.min(15))),
        "riverin" => Tool::Point(Point::RiverIn),
        "riverout" => Tool::Point(Point::RiverOut),
        "fishing" => Tool::Point(Point::Fishing(n.min(7))),
        "killer" | "predator" => Tool::Point(Point::Predator(n.min(3))),
        "prey" => Tool::Point(Point::Prey(n.min(3))),
        "disembark" => Tool::Point(Point::Disembark(n.min(2))),
        _ => bail!("unknown editor tool {name}"),
    })
}

/// Whether `script` is for the editor (its first step starts with `edit`).
pub fn is_editor_script(script: &str) -> bool {
    script.split(';').map(str::trim).find(|s| !s.is_empty()).is_some_and(|s| s.starts_with("edit"))
}

impl Editor {
    pub fn run_script(&mut self, data: &Path, script: &str) -> Result<Outcome> {
        let mut out = Outcome::default();
        for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let parts: Vec<&str> = step.split_whitespace().collect();
            match parts.as_slice() {
                ["editor"] => {}
                ["editor" | "editopen", _, ..] => {
                    // The name may have spaces: the rest of the step.
                    let name = step.split_once(' ').map_or("", |(_, n)| n.trim());
                    let path = map_path(data, name);
                    *self = Editor::open(&path, self.defs.clone(), self.text.clone(), self.maps_dir.clone())?;
                }
                ["editnew", n] => {
                    let n: i32 = n.parse()?;
                    let size = MAP_SIZES.iter().position(|&w| w == n).unwrap_or(n.clamp(0, 5) as usize);
                    self.new_map(size);
                }
                ["edittool", name] => self.tool = tool(name, 0)?,
                ["edittool", name, n] => self.tool = tool(name, n.parse()?)?,
                ["editbrush", n] => self.brush = n.parse::<u8>()?.min(4),
                ["editpaint", name, p] | ["editpaint", name, p, _] => {
                    if let Some(b) = parts.get(3) {
                        self.brush = b.parse::<u8>()?.min(4);
                    }
                    let (x, y) = point(p)?;
                    self.tool = tool(name, 0)?;
                    let changed = self.apply(x, y);
                    eprintln!("{step}: changed {changed}");
                }
                ["editpoint", name, p] | ["editpoint", name, _, p] => {
                    let n = if parts.len() == 4 { parts[2].parse()? } else { 0 };
                    self.tool = tool(name, n)?;
                    let (x, y) = point(p)?;
                    let placed = self.apply(x, y);
                    eprintln!("{step}: placed {placed}");
                }
                ["editroad", a, b] => {
                    self.tool = Tool::Road;
                    let changed = self.road(point(a)?, point(b)?);
                    eprintln!("{step}: changed {changed}");
                }
                ["editrefresh"] => self.refresh_map(),
                ["editclimate", n] => {
                    self.scenario.info.climate = n.parse::<u8>()?.min(2);
                    self.refresh_map();
                }
                ["editmenu", n] => self.open_submenu(n.parse()?),
                ["edittop", n] => self.open_top_menu(n.parse()?),
                ["editpopup", "sizes"] => self.view.popup = Some(Popup::Sizes),
                ["editpopup", "save"] => self.view.popup = Some(Popup::SaveName(self.name.clone())),
                ["editoptions"] => self.view.options = Some(Options::default()),
                ["editoptions", page] => {
                    let page = match *page {
                        "starting" => Page::Starting,
                        "date" => Page::StartDate,
                        "win" => Page::Win,
                        "monuments" => Page::Monuments,
                        "allowed" => Page::Allowed,
                        "gods" => Page::Gods,
                        "flood" => Page::Flood,
                        _ => Page::Main,
                    };
                    self.view.options = Some(Options::at(page));
                }
                ["editchooser", page, what] => {
                    let page = match *page {
                        "starting" => Page::Starting,
                        "win" => Page::Win,
                        "monuments" => Page::Monuments,
                        _ => Page::Main,
                    };
                    self.view.options = Some(Options::with_chooser(page, what, &self.scenario));
                }
                ["editview", p] => out.view = Some(point(p)?),
                ["editzoom", z] => out.zoom = Some(z.parse()?),
                ["edithover", p] => out.hover = Some(point(p)?),
                ["editsave", p] => {
                    self.save_as(Path::new(p))?;
                    eprintln!("{step}: saved");
                }
                ["editplay"] => {
                    self.play()?;
                    if let Some(super::Request::Play(p)) = self.request.take() {
                        out.play = Some(p);
                    }
                }
                ["editinfo", p] => {
                    let (x, y) = point(p)?;
                    let m = &self.map;
                    eprintln!(
                        "  tile {x},{y}: terrain {:#x} image {} moisture {} fertility {}",
                        m.terrain.at_or(x, y, 0),
                        m.images.at_or(x, y, 0),
                        m.moisture.at_or(x, y, 0),
                        m.fertility.at_or(x, y, 0)
                    );
                }
                _ => bail!("unknown editor step: {step}"),
            }
        }
        Ok(out)
    }
}
