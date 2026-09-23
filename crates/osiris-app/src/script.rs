//! Headless test scripts (`--script`): build, advance time and report, for screenshots
//! and debugging without a window.

use anyhow::{Context, Result, bail};
use osiris_sim::{Command, World};

fn parse_point(s: &str) -> Result<(i32, i32)> {
    let (x, y) = s.split_once(',').context("expected x,y")?;
    Ok((x.trim().parse()?, y.trim().parse()?))
}

/// What a script asks the screenshot to show.
#[derive(Default)]
pub struct ScriptView {
    pub centre: Option<(i32, i32)>,
    pub info: Option<(i32, i32)>,
    pub keep_dialogs: bool,
    pub menu: bool,
    pub messages: bool,
    pub menu_page: Option<String>,
    pub rules: bool,
    pub overlay: Option<String>,
    pub top_menu: Option<usize>,
    pub build_menu: Option<String>,
    pub empire: Option<Option<usize>>,
    pub advisor: Option<String>,
    pub orders: bool,
}

/// Runs `--script` steps against the world.
pub fn run_script(world: &mut World, script: &str) -> Result<ScriptView> {
    let mut view = ScriptView::default();
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
            ["view", p] => view.centre = Some(parse_point(p)?),
            ["info", p] => view.info = Some(parse_point(p)?),
            ["dialogs"] => view.keep_dialogs = true,
            ["menu"] => view.menu = true,
            ["menu", page] => {
                view.menu = true;
                view.menu_page = Some(page.to_string());
            }
            ["rules"] => view.rules = true,
            ["overlay", name] => view.overlay = Some(name.to_string()),
            ["topmenu", n] => view.top_menu = Some(n.parse()?),
            ["messages"] => view.messages = true,
            ["buildmenu", name] => view.build_menu = Some(name.to_string()),
            ["advisor", name] => view.advisor = Some(name.to_string()),
            ["orders"] => view.orders = true,
            ["empire"] => view.empire = Some(None),
            ["empire", c] => view.empire = Some(Some(c.parse()?)),
            ["burn", p] => {
                let (x, y) = parse_point(p)?;
                let id = world.map.building.at_or(x, y, 0);
                world.destroy(id, true);
            }
            ["stock", p, r, n] => {
                let (x, y) = parse_point(p)?;
                let id = world.map.building.at_or(x, y, 0);
                world.buildings.get(id).context("no building there")?;
                let (r, n): (u16, i32) = (r.parse()?, n.parse()?);
                let have = world.stored(id, r);
                world.take_stored(id, r, have);
                world.add_stored(id, r, n);
            }
            ["opentrade", c] => {
                let c: usize = c.parse()?;
                world.trade.cities[c].open = true;
            }
            ["trade", r, st, n] => {
                let st = match *st {
                    "import" => osiris_sim::trade::status::IMPORT,
                    "export" => osiris_sim::trade::status::EXPORT,
                    _ => osiris_sim::trade::status::NONE,
                };
                world.set_trade(r.parse()?, st, n.parse()?);
            }
            ["tradereport"] => {
                for (i, c) in world.trade.cities.iter().enumerate() {
                    let route = &world.trade.routes[c.route as usize];
                    let traded: Vec<(usize, i32)> = route.traded.iter().copied().enumerate().filter(|t| t.1 > 0).collect();
                    eprintln!("  city {i} name {} type {} open {} sea {} traded {:?}", c.name_id, c.city_type, c.open, c.sea, traded);
                }
                for t in &world.trade.traders {
                    eprintln!("  trader {:?} at {:?}", t, world.trader_position(t));
                }
                eprintln!("  finance {:?} entry {:?} exit {:?}", world.finance.this_year, world.entry_point, world.exit_point);
            }
            ["globallabor"] => world.rules.global_labor_pool = true,
            ["safe"] => {
                world.rules.fire = false;
                world.rules.collapse = false;
            }
            ["saveload"] => {
                let bytes = world.save().map_err(anyhow::Error::msg)?;
                let loaded = World::load(&bytes, world.defs.clone(), world.balance.clone()).map_err(anyhow::Error::msg)?;
                let same = loaded.save().map_err(anyhow::Error::msg)? == bytes;
                eprintln!("saveload: {} bytes, identical after reload: {same}", bytes.len());
                *world = loaded;
            }
            ["grid", p] => {
                // Prints the terrain around a tile: # road, B building, ~ water, . open, x blocked.
                let (cx, cy) = parse_point(p)?;
                for y in cy - 6..=cy + 6 {
                    let row: String = (cx - 8..=cx + 8)
                        .map(|x| {
                            let t = world.map.terrain.at_or(x, y, 0);
                            use osiris_sim::map::{mask, terrain};
                            if t & terrain::BUILDING != 0 { 'B' } else if t & terrain::ROAD != 0 { '#' } else if t & terrain::WATER != 0 { '~' } else if t & mask::NOT_CLEAR & !terrain::FLOODPLAIN != 0 { 'x' } else { '.' }
                        })
                        .collect();
                    eprintln!("{y:4} {row}");
                }
                eprintln!("     x from {}", cx - 8);
            }
            ["report"] => {
                let houses: Vec<String> = world
                    .buildings
                    .iter()
                    .filter_map(|b| b.house.as_ref().map(|h| format!("({},{})L{}p{}d{}{}f{:?}{:?}", b.x, b.y, h.level, h.population, b.desirability, if h.well_access { "w" } else { "" }, h.foods, h.blocked_by)))
                    .collect();
                eprintln!(
                    "{:?} pop {} treasury {} figures {} houses {:?}",
                    world.time, world.population, world.treasury, world.figures.len(), houses
                );
                eprintln!("  labor {:?} unemployment {}%", world.labor, world.unemployment);
                for b in world.buildings.iter().filter(|b| !b.is_house()) {
                    let mut stock: Vec<(usize, i32)> = b.stock.iter().copied().enumerate().filter(|&(_, v)| v > 0).collect();
                    stock.extend(b.spaces.iter().filter(|s| s.1 > 0).map(|s| (s.0 as usize, s.1)));
                    eprintln!("  bld {} kind {} at ({},{}) workers {} covered {} road {:?} walkers {:?} progress {} stock {:?} shows {:?}", b.id, b.kind, b.x, b.y, b.workers, b.houses_covered, b.road, b.walkers, b.progress, stock, b.shows);
                }
                for f in world.figures.iter().take(40) {
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

