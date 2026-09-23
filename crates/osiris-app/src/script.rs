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
    /// A popup to open over the overseer: salary, gift or donate.
    pub advisor_popup: Option<String>,
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
            ["advisor", name, popup] => {
                view.advisor = Some(name.to_string());
                view.advisor_popup = Some(popup.to_string());
            }
            ["savings", n] => world.governor.savings = n.parse()?,
            ["burial", r, n] => world.burial[r.parse::<usize>()?].0 = n.parse()?,
            ["sendburial", r, n] => {
                let sent = world.dispatch_burial(r.parse()?, n.parse()?);
                eprintln!("{step}: sent {sent}");
            }
            ["monuments", list] => {
                for (slot, m) in world.scenario_monuments.iter_mut().zip(list.split(',')) {
                    *slot = m.parse()?;
                }
            }
            ["orders"] => view.orders = true,
            // Sets every monument's phase, and its blocks' work: "n" for all, or
            // "a:b" to ramp from a on the first block to b on the last.
            ["monphase", phase, work] => {
                let (a, b) = work.split_once(':').unwrap_or((work, work));
                let (a, b): (i32, i32) = (a.parse()?, b.parse()?);
                let ids: Vec<_> = world.buildings.iter().filter(|b| b.monument.is_some()).map(|b| b.id).collect();
                for id in ids {
                    let def = world.buildings.get(id).and_then(|b| osiris_sim::monuments::monument_def(b.kind));
                    if let (Some(def), Some(m)) = (def, world.buildings.get_mut(id).and_then(|b| b.monument.as_mut())) {
                        m.phase = phase.parse()?;
                        m.finished = m.phase + 1 >= def.phase_count;
                        m.progress = vec![0; def.units(m.phase)];
                        let n = m.progress.len().max(2) as i32;
                        for (i, p) in m.progress.iter_mut().enumerate() {
                            *p = (a + (b - a) * i as i32 / (n - 1)) as u16;
                        }
                    }
                    world.refresh_monument_images(id);
                }
            }
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
                    eprintln!("  city {i} name {} type {} open {} sea {} traded {:?} next {} route {} points {} limits {:?}", c.name_id, c.city_type, c.open, c.sea, traded, c.entry_delay, c.route, route.points.len(), route.limit.iter().enumerate().filter(|l| *l.1 > 0).collect::<Vec<_>>());
                }
                for t in &world.trade.traders {
                    eprintln!("  trader {:?} at {:?}", t, world.trader_position(t));
                }
                eprintln!("  finance {:?} entry {:?} exit {:?}", world.finance.this_year, world.entry_point, world.exit_point);
            }
            ["events"] => {
                for (i, e) in world.scenario_events.list.iter().enumerate() {
                    eprintln!(
                        "  event {i} kind {} trigger {} at y{} m{} res {} amount {} city {:?} state {} left {} active {} wait {}",
                        e.kind, e.trigger, e.year, e.month, e.resource, e.amount, e.city, e.state, e.months_left, e.active, e.wait
                    );
                }
                for n in world.notices.log.iter().filter(|n| n.text.is_some()) {
                    eprintln!("  posted {}/{} {:?}", n.month, n.year, n.text);
                }
                eprintln!("  kingdom {} wages {}", world.ratings.kingdom, world.finance.kingdom_wages);
            }
            ["health"] => {
                let houses: Vec<(i32, i32, i32, i32)> = world.buildings.iter().filter_map(|b| b.house.as_ref().filter(|h| h.population > 0).map(|h| (h.population, h.common_health, h.plague_days, h.criminal_active))).collect();
                let plagued = houses.iter().filter(|h| h.2 > 0).count();
                let crime: i32 = houses.iter().map(|h| h.3).max().unwrap_or(0);
                let wanderers: Vec<u16> = world.figures.iter().filter(|f| matches!(f.kind, 22 | 23 | 98)).map(|f| f.kind).collect();
                eprintln!("  health {} target {} houses {} plagued {plagued} max crime {crime} sentiment {} wanderers {:?} treasury {}", world.ratings.health, world.ratings.health_target, houses.len(), world.sentiment, wanderers, world.treasury);
                let log: Vec<&str> = world.notices.log.iter().map(|n| n.key.as_str()).filter(|k| k.contains("plague") || k.contains("disease") || k.contains("malaria") || k.contains("crime")).collect();
                eprintln!("  log {log:?}");
            }
            ["invade", invader, n, point] => world.invade_now(invader.parse()?, n.parse()?, point.parse()?),
            ["company", c, p] => {
                let p = parse_point(p)?;
                world.move_company(c.parse()?, p);
            }
            ["fortreturn", c] => world.return_company(c.parse()?),
            ["service", c] => world.toggle_kingdom_service(c.parse()?),
            // Starts the scenario's earthquake now, or one at a tile with a severity.
            ["quake"] => eprintln!("{step}: started {}", world.quake_now(None, None)),
            ["quake", p, n] => eprintln!("{step}: started {}", world.quake_now(Some(parse_point(p)?), Some(n.parse()?))),
            ["quakes"] => {
                for q in &world.earthquakes.list {
                    eprintln!("  quake y{} m{} severity {} state {} steps {} fronts {:?}", q.year, q.month, q.severity, q.state, q.steps, q.fronts);
                }
                eprintln!("  epicentre {:?}", world.earthquakes.epicentre);
            }
            ["noinvasions"] => world.invasions.planned.clear(),
            ["order", c, o] => {
                use osiris_sim::military::Order;
                let order = match *o {
                    "tight" => Order::HoldTight,
                    "loose" => Order::HoldLoose,
                    "engage" => Order::Engage,
                    "charge" => Order::Charge,
                    _ => Order::MopUp,
                };
                world.set_order(c.parse()?, order);
            }
            ["seapoint", p] => world.invasions.sea_points.push(parse_point(p)?),
            ["landpoint", p] => world.invasions.land_points.push(parse_point(p)?),
            ["troops", n] => {
                let i = world.scenario_events.request_troops_now(n.parse()?);
                let ok = world.dispatch_request(i);
                eprintln!("{step}: request {i} dispatched {ok}");
            }
            ["army"] => {
                for (i, c) in world.military.companies.iter().enumerate() {
                    let alive: Vec<(i32, i32, u16, i32)> = c.soldiers.iter().filter_map(|&s| world.figures.get(s)).map(|f| (f.x, f.y, f.action, f.damage)).collect();
                    eprintln!("  company {i} fort {} kind {} at_fort {} morale {} wind {} trained {} soldiers {} recruits {} {:?}", c.fort, c.kind, c.at_fort, c.morale, c.wind, c.trained, c.soldiers.len(), c.recruits.len(), alive);
                }
                for (i, a) in world.invasions.armies.iter().enumerate() {
                    let alive: Vec<(i32, i32, u16, u16, i32)> = a.figures.iter().filter_map(|&s| world.figures.get(s)).map(|f| (f.x, f.y, f.kind, f.action, f.damage)).collect();
                    let t = world.buildings.get(a.target).map(|b| (b.kind, b.x, b.y, b.enemy_damage));
                    eprintln!("  army {i} invader {} nation {} target {} {:?} morale {} fleeing {} {:?}", a.invader, a.nation, a.target, t, a.morale, a.fleeing, alive);
                }
                eprintln!("  battle {:?} kingdom {}", world.military.battle, world.ratings.kingdom);
                eprintln!("  points land {:?} sea {:?} landings {:?}", world.invasions.land_points, world.invasions.sea_points, world.invasions.landings);
                eprintln!("  won {} lost {} limit {:?} survival {:?}", world.won, world.lost, world.time_limit, world.survival);
                eprintln!("  planned {:?} lost {}", world.invasions.planned.iter().map(|p| (p.invader, p.year, p.month, p.warning, p.done)).collect::<Vec<_>>(), world.invasions.lost);
            }
            ["clearmessages"] => {
                world.messages.clear();
                world.message_texts.clear();
            }
            ["dispatch", i] => {
                let ok = world.dispatch_request(i.parse()?);
                eprintln!("{step}: {ok}");
            }
            ["globallabor"] => world.rules.global_labor_pool = true,
            ["fullstaff"] => world.test_full_staff = true,
            ["nodisease"] => world.rules.disease = false,
            ["nodisasters"] => world.rules.disasters = false,
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
            ["imagestats"] => {
                // Image ids on water, tree, rock and plain tiles: the most common of each.
                use osiris_sim::map::terrain;
                for (name, bit) in [("water", terrain::WATER), ("tree", terrain::TREE), ("rock", terrain::ROCK), ("marsh", terrain::MARSHLAND), ("flood", terrain::FLOODPLAIN)] {
                    let mut counts: std::collections::BTreeMap<u32, u32> = Default::default();
                    for y in 0..world.map.height {
                        for x in 0..world.map.width {
                            if world.map.terrain.at_or(x, y, 0) & bit != 0 {
                                *counts.entry(world.map.images.at_or(x, y, 0)).or_default() += 1;
                            }
                        }
                    }
                    let mut v: Vec<(u32, u32)> = counts.into_iter().collect();
                    v.sort_by_key(|e| std::cmp::Reverse(e.1));
                    eprintln!("{name}: {:?}", &v[..v.len().min(6)]);
                }
            }
            ["asciimap"] => {
                // The whole map: ~ water, p floodplain, x blocked, . clear land.
                use osiris_sim::map::{mask, terrain};
                for y in 0..world.map.height {
                    let row: String = (0..world.map.width)
                        .map(|x| {
                            let t = world.map.terrain.at_or(x, y, 0);
                            if t & terrain::WATER != 0 { '~' } else if t & terrain::FLOODPLAIN != 0 { 'p' } else if t & mask::NOT_CLEAR != 0 { 'x' } else { '.' }
                        })
                        .collect();
                    eprintln!("{y:4} {row}");
                }
            }
            ["grid", p] => {
                // Prints the terrain around a tile: # road, B building, f ferry crossing, ~ water, . open, x blocked.
                let (cx, cy) = parse_point(p)?;
                for y in cy - 6..=cy + 6 {
                    let row: String = (cx - 8..=cx + 8)
                        .map(|x| {
                            let t = world.map.terrain.at_or(x, y, 0);
                            use osiris_sim::map::{mask, terrain};
                            if t & terrain::FERRY_ROUTE != 0 && t & terrain::WATER != 0 { 'f' } else if t & terrain::BUILDING != 0 { 'B' } else if t & terrain::ROAD != 0 { '#' } else if t & terrain::WATER != 0 { '~' } else if t & mask::NOT_CLEAR & !terrain::FLOODPLAIN != 0 { 'x' } else { '.' }
                        })
                        .collect();
                    eprintln!("{y:4} {row}");
                }
                eprintln!("     x from {}", cx - 8);
            }
            ["monlist"] => eprintln!("monuments {:?} complex gods {:?} debt rate {}", world.scenario_monuments, world.complex_gods, world.debt_rate),
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
                let r = &world.ratings;
                eprintln!("  sentiment {} culture {} prosperity {}/{} monument {} kingdom {} coverage {:?}", world.sentiment, r.culture, r.prosperity, r.prosperity_max, r.monument, r.kingdom, r.coverage);
                let gods: Vec<(u8, i32, i32, i32, i32)> = world.religion.gods.iter().map(|g| (g.status, g.mood, g.target, g.wrath, g.coverage)).collect();
                eprintln!("  gods {:?} common {}", gods, world.religion.coverage_common);
                for b in world.buildings.iter().filter(|b| !b.is_house()) {
                    let mut stock: Vec<(usize, i32)> = b.stock.iter().copied().enumerate().filter(|&(_, v)| v > 0).collect();
                    stock.extend(b.spaces.iter().filter(|s| s.1 > 0).map(|s| (s.0 as usize, s.1)));
                    eprintln!("  bld {} kind {} at ({},{}) workers {} covered {} road {:?} walkers {:?} progress {} stock {:?} shows {:?}", b.id, b.kind, b.x, b.y, b.workers, b.houses_covered, b.road, b.walkers, b.progress, stock, b.shows);
                }
                for b in world.buildings.iter().filter(|b| b.monument.is_some()) {
                    let m = b.monument.as_ref().unwrap();
                    eprintln!("  monument {} kind {} phase {} finished {} funeral {} delivered {:?} in flight {:?} progress {:?} craftsmen {:?}", b.id, b.kind, m.phase, m.finished, m.funeral_done, m.delivered, m.in_flight, m.progress, m.craftsmen);
                }
                for f in world.figures.iter().take(40) {
                    eprintln!(
                        "  fig {} kind {} at ({},{}) dest {:?} route {} moving {} counter {} progress {} dir {} action {} home {} amount {}",
                        f.id, f.kind, f.x, f.y, f.destination, f.route.len(), f.moving, f.counter, f.progress, f.direction, f.action, f.home, f.amount
                    );
                }
            }
            _ => bail!("bad script step: {step}"),
        }
    }
    Ok(view)
}

