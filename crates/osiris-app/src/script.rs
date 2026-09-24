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
    /// Camera zoom for the screenshot.
    pub zoom: Option<f32>,
    pub empire: Option<Option<usize>>,
    pub advisor: Option<String>,
    /// A popup to open over the overseer: salary, gift or donate.
    pub advisor_popup: Option<String>,
    pub orders: bool,
    /// Screen point the mouse rests on, for tooltips.
    pub hover: Option<[f32; 2]>,
    /// A sidebar slide frozen part-way: collapsing or not, and the step (0-47).
    pub slide: Option<(bool, f32)>,
    /// A building tool held with the cursor on a tile, to show its placement preview.
    pub hold: Option<(u16, (i32, i32))>,
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
            // Places N random buildings from the build menus at valid spots, with roads
            // and some clearing, from seed S.
            ["fuzz", n, s] => fuzz(world, n.parse()?, s.parse()?, None),
            // A town near the entry: a road grid, then N random buildings (half houses) on it.
            ["town", n, s] => {
                let c = town_roads(world);
                fuzz(world, n.parse()?, s.parse()?, Some(c));
            }
            // Lets every building type be built.
            ["allowall"] => {
                let all: Vec<u16> = (0..world.defs.buildings.len() as u16).filter(|&k| world.defs.building(k).is_some()).collect();
                if let Some(m) = world.mission.as_mut() {
                    m.allowed.extend(all);
                }
            }
            // Runs N ticks, reporting the time taken, the slowest tick and figures that never moved.
            ["timed", n] => {
                let before: std::collections::HashMap<_, _> = world.figures.iter().map(|f| (f.id, (f.x, f.y, f.kind, f.action))).collect();
                let start = std::time::Instant::now();
                let mut worst = std::time::Duration::ZERO;
                for _ in 0..n.parse::<u32>()? {
                    let t = std::time::Instant::now();
                    world.tick();
                    worst = worst.max(t.elapsed());
                }
                let mut still: std::collections::BTreeMap<(u16, u16), u32> = Default::default();
                for f in world.figures.iter() {
                    if before.get(&f.id).is_some_and(|b| (b.0, b.1) == (f.x, f.y)) {
                        *still.entry((f.kind, f.action)).or_default() += 1;
                    }
                }
                let idle = world.buildings.iter().filter(|b| !b.is_house() && b.workers == 0 && world.defs.building(b.kind).is_some_and(|d| d.labor.is_some())).count();
                eprintln!(
                    "timed {n}: {:?} total, worst tick {:?}; pop {} treasury {} figures {} buildings {} unstaffed {idle}; unmoved (kind,action) {still:?}",
                    start.elapsed(), worst, world.population, world.treasury, world.figures.len(), world.buildings.iter().count()
                );
            }
            // Every figure of kind K: where it is, what it is doing and where it is going.
            ["figs", k] => {
                let k: u16 = k.parse()?;
                for f in world.figures.iter().filter(|f| f.kind == k) {
                    eprintln!("  fig {} at ({},{}) action {} dest {:?} route {} moving {} counter {} stuck {} foe {} target {}", f.id, f.x, f.y, f.action, f.destination, f.route.len(), f.moving, f.counter, f.stuck, f.foe, f.target);
                }
            }
            // A tile's terrain bits and building.
            ["tile", p] => {
                let (x, y) = parse_point(p)?;
                let id = world.map.building.at_or(x, y, 0);
                eprintln!("  tile {x},{y}: terrain {:#x} building {id} kind {:?} image {} edges {:#x}", world.map.terrain.at_or(x, y, 0), world.buildings.get(id).map(|b| b.kind), world.map.images.at_or(x, y, 0), world.map.edges.at_or(x, y, 0));
            }
            ["view", p] => view.centre = Some(parse_point(p)?),
            ["info", p] => view.info = Some(parse_point(p)?),
            ["dialogs"] => view.keep_dialogs = true,
            ["zoom", z] => view.zoom = Some(z.parse()?),
            ["sidebar", "collapse"] => crate::sidebar::set_collapsed(true),
            ["sidebar", "expand"] => crate::sidebar::set_collapsed(false),
            ["sidebar", "slide", dir, step] => view.slide = Some((*dir == "collapse", step.parse()?)),
            ["hover", p] => {
                let (x, y) = parse_point(p)?;
                view.hover = Some([x as f32, y as f32]);
            }
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
            ["burial", r, n] => world.burial.get_mut(r.parse::<usize>()?).context("no such burial rank")?.0 = n.parse()?,
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
            // Holds building tool K with the cursor on tile x,y for the screenshot.
            ["hold", k, p] => view.hold = Some((k.parse()?, parse_point(p)?)),
            // The statue tool after N presses of R (look N mod 4, turned N / 4 times).
            ["statue", n] => {
                let n: u8 = n.parse()?;
                world.statue_variant = n % 4;
                world.statue_facing = (n / 4 % 4 + 1) % 4;
            }
            ["gateface", f] => world.gatehouse_facing = f.parse::<u8>()? & 1,
            // The temple complex facing for the next build (0 along x, 1 along y).
            ["complexface", f] => world.complex_facing = f.parse::<u8>()? & 1,
            ["treasury", n] => world.treasury = n.parse()?,
            ["gatefacing", n] => world.gatehouse_facing = n.parse::<u8>()?.min(1),
            // The placement preview of building K with the cursor on x,y: each tile's verdict.
            ["preview", k, p] => {
                let k: u16 = k.parse()?;
                let (cx, cy) = parse_point(p)?;
                let (ax, ay) = world.cursor_tile(k);
                let pv = world.placement_preview(k, cx - ax, cy - ay);
                let blocked: Vec<String> = pv.tiles.iter().filter_map(|t| t.blocked.map(|why| format!("{},{} {why}", t.x, t.y))).collect();
                let red = pv.tiles.iter().filter(|t| t.red).count();
                eprintln!("preview {k} at {},{}: {:?}, {} tiles, {red} red, blocking {blocked:?}", cx - ax, cy - ay, pv.result, pv.tiles.len());
            }
            // Where building K could first be placed with its cursor tile, scanning rows, or "none".
            ["site", k] => {
                let k: u16 = k.parse()?;
                let (ax, ay) = world.cursor_tile(k);
                let site = (0..world.map.height).flat_map(|y| (0..world.map.width).map(move |x| (x, y))).find(|&(x, y)| world.can_place(k, x, y).is_ok());
                eprintln!("site {k}: {:?}", site.map(|(x, y)| (x + ax, y + ay)));
            }
            // Lets building type K be built, as a tutorial unlock would.
            ["allow", k] => {
                if let Some(m) = world.mission.as_mut() {
                    m.allowed.insert(k.parse()?);
                }
            }
            // Where royal tomb type K could first be placed (scanning rows), or "none".
            ["tombsite", k] => {
                let k: u16 = k.parse()?;
                let site = (0..world.map.height).flat_map(|y| (0..world.map.width).map(move |x| (x, y))).find(|&(x, y)| world.can_place(k, x, y).is_ok());
                let mut why = std::collections::BTreeMap::new();
                for y in 0..world.map.height {
                    for x in 0..world.map.width {
                        if let Err(e) = world.can_place(k, x, y) {
                            *why.entry(e).or_insert(0) += 1;
                        }
                    }
                }
                eprintln!("tombsite {k}: {site:?} {why:?}");
            }
            // Every royal tomb's state: percent, lamps, and each chamber's stage/work/man.
            ["tombs"] => {
                for b in world.buildings.iter().filter(|b| osiris_sim::royal_tombs::is_royal_tomb(b.kind)) {
                    let Some(m) = &b.monument else { continue };
                    let chambers: Vec<String> = m.chambers.iter().map(|c| format!("{}/{}{}", c.progress, c.left, if c.worker != 0 { "*" } else { "" })).collect();
                    eprintln!("tomb {} at {},{}: {}% lamps {} run {} sealed {} chambers {}", b.kind, b.x, b.y, world.monument_percent(b.id), m.lamps, m.lamp_run, m.finished, chambers.join(" "));
                }
                eprintln!("burial provisions {:?}", world.burial_needs());
                for b in world.buildings.iter().filter(|b| matches!(b.kind, 72 | 231 | 179 | 199)) {
                    eprintln!("  building {} at {},{} workers {} road {:?} lamps {} clay {} paint {}", b.kind, b.x, b.y, b.workers, b.road, world.stored(b.id, 34), b.stock[11], b.stock[33]);
                }
                for f in world.figures.iter().filter(|f| matches!(f.kind, 81 | 108) || f.kind == osiris_sim::farms::PEASANT) {
                    eprintln!("  figure {} kind {} action {} at {},{} job {}", f.id, f.kind, f.action, f.x, f.y, f.amount);
                }
            }
            // Sets every royal tomb chamber to stage N (0-4) and the lamps to L.
            ["tombstage", n, lamps] => {
                let (n, lamps): (u8, i32) = (n.parse()?, lamps.parse()?);
                let ids: Vec<_> = world.buildings.iter().filter(|b| osiris_sim::royal_tombs::is_royal_tomb(b.kind)).map(|b| b.id).collect();
                for id in ids {
                    let Some(b) = world.buildings.get_mut(id) else { continue };
                    let l = osiris_sim::royal_tombs::layout(b.kind).context("tomb")?;
                    if let Some(m) = b.monument.as_mut() {
                        m.lamps = lamps;
                        for (c, s) in l.chambers.iter().zip(m.chambers.iter_mut()) {
                            s.progress = n;
                            s.left = if (1..4).contains(&n) { c.work } else { 0 };
                        }
                    }
                    world.refresh_monument_images(id);
                }
            }
            // Sets every monument's phase, and its blocks' work: "n" for all, or
            // "a:b" to ramp from a on the first block to b on the last.
            ["monphase", phase, work] => {
                let (a, b) = work.split_once(':').unwrap_or((work, work));
                let (a, b): (i32, i32) = (a.parse()?, b.parse()?);
                let ids: Vec<_> = world.buildings.iter().filter(|b| b.monument.is_some()).map(|b| b.id).collect();
                for id in ids {
                    let def = world.buildings.get(id).and_then(|b| osiris_sim::monuments::monument_def(b.kind));
                    // Pyramids and mastabas go block by block: their stage's start.
                    if def.is_some_and(|d| osiris_sim::pyramids::blockwise(d.style)) {
                        world.set_tomb_stage(id, phase.parse()?);
                        continue;
                    }
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
                world.trade.cities.get_mut(c).context("no such trade city")?.open = true;
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
                    eprintln!("  city {i} name {} type {} open {} sea {} traded {:?} next {} route {} points {} limits {:?} traders {:?}", c.name_id, c.city_type, c.open, c.sea, traded, c.entry_delay, c.route, route.points.len(), route.limit.iter().enumerate().filter(|l| *l.1 > 0).collect::<Vec<_>>(), c.traders);
                }
                eprintln!("  finance {:?} entry {:?} exit {:?}", world.finance.this_year, world.entry_point, world.exit_point);
            }
            ["events"] => {
                for (i, e) in world.scenario_events.list.iter().enumerate() {
                    eprintln!(
                        "  event {i} kind {} trigger {} at y{} m{} res {} amount {} city {:?} state {} left {} active {} wait {} next {}/{}/{}",
                        e.kind, e.trigger, e.year, e.month, e.resource, e.amount, e.city, e.state, e.months_left, e.active, e.wait, e.on_completed, e.on_refusal, e.on_too_late
                    );
                }
                for n in world.notices.log.iter().filter(|n| n.text.is_some()) {
                    eprintln!("  posted {}/{} {:?}", n.month, n.year, n.text);
                }
                eprintln!("  kingdom {} wages {}", world.ratings.kingdom, world.finance.kingdom_wages);
            }
            ["health"] => {
                let houses: Vec<(i32, i32, i32, i32)> = world.buildings.iter().filter_map(|b| b.house.as_ref().filter(|h| h.population > 0).map(|h| (h.population, h.disease_risk, h.quarantine, h.crime))).collect();
                let plagued = houses.iter().filter(|h| h.2 > 0).count();
                let crime: i32 = houses.iter().map(|h| h.3).max().unwrap_or(0);
                let wanderers: Vec<u16> = world.figures.iter().filter(|f| matches!(f.kind, 22 | 23 | 98)).map(|f| f.kind).collect();
                eprintln!("  health {} target {} houses {} plagued {plagued} max crime {crime} sentiment {} wanderers {:?} treasury {} migration {:?}", world.ratings.health, world.ratings.health_target, houses.len(), world.sentiment, wanderers, world.treasury, world.migration);
                let log: Vec<&str> = world.notices.log.iter().map(|n| n.key.as_str()).filter(|k| k.contains("plague") || k.contains("disease") || k.contains("malaria") || k.contains("crime")).collect();
                eprintln!("  log {log:?}");
            }
            ["invade", invader, n, point] => world.invade_now(invader.parse()?, n.parse()?, point.parse()?),
            ["company", c, p] => {
                let p = parse_point(p)?;
                world.move_company(c.parse()?, p);
            }
            ["fortreturn", c] => world.return_company(c.parse()?),
            // The company window's switch that turns the next held line.
            ["rotate", c] => world.rotate_line(c.parse()?),
            ["service", c] => world.toggle_kingdom_service(c.parse()?),
            // Lets the city raise a temple complex to god `g`.
            ["allowcomplex", g] => world.complex_gods[g.parse::<usize>()?.min(4)] = true,
            // A god blesses or curses the city now: god index, bless|curse, major|minor.
            ["god", g, what, size] => {
                let before = world.messages.len();
                world.god_acts_now(g.parse()?, *what == "bless", *size == "major");
                let keys: Vec<&str> = world.messages.iter().skip(before).map(String::as_str).collect();
                eprintln!("{step}: posted {keys:?}");
            }
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
                    eprintln!("  company {i} fort {} kind {} at_fort {} morale {} wind {} experience {} order {:?} turned {} charged {} wounds {}% soldiers {} recruits {} {:?}", c.fort, c.kind, c.at_fort, c.morale, c.wind, c.experience, c.order, c.turned, c.charged, world.company_wounds(i), c.soldiers.len(), c.recruits.len(), alive);
                }
                for b in world.buildings.iter().filter(|b| b.kind == osiris_sim::military::RECRUITER) {
                    eprintln!("  recruiter {} workers {} days {} weapons {} chariots {} wanted {}", b.id, b.workers, b.spawn_delay, b.stock[osiris_sim::military::WEAPONS as usize], b.stock[osiris_sim::military::CHARIOTS as usize], world.recruits_wanted(b.id));
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
            // Ends the game lost, to show the lost-mission screen.
            ["lose"] => world.lost = true,
            // The build tool's gatehouse facing (0 or 1) for later builds.
            ["gatehouse", f] => world.gatehouse_facing = f.parse()?,
            // The build tool's statue look (0..) and facing (0-3) for later builds.
            ["statue", v, f] => {
                world.statue_variant = v.parse()?;
                world.statue_facing = f.parse()?;
            }
            ["roadimages"] => {
                // Road tiles by image id: count, terrain bits and one example tile.
                use osiris_sim::map::terrain;
                let mut counts: std::collections::BTreeMap<u32, (u32, u32, (i32, i32))> = Default::default();
                for y in 0..world.map.height {
                    for x in 0..world.map.width {
                        let t = world.map.terrain.at_or(x, y, 0);
                        if t & terrain::ROAD != 0 {
                            let e = counts.entry(world.map.images.at_or(x, y, 0)).or_insert((0, t, (x, y)));
                            e.0 += 1;
                        }
                    }
                }
                for (image, (n, t, p)) in counts {
                    eprintln!("road image {image}: {n} tiles, terrain {t:#x}, e.g. {p:?}");
                }
            }
            ["asciimap"] => {
                // The whole map: ~ water, p floodplain, C cliff, B building, x blocked, . clear land.
                use osiris_sim::map::{mask, terrain};
                for y in 0..world.map.height {
                    let row: String = (0..world.map.width)
                        .map(|x| {
                            let t = world.map.terrain.at_or(x, y, 0);
                            let cliff = terrain::CLIFF | terrain::ROCK;
                            if t & terrain::WATER != 0 { '~' } else if t & terrain::FLOODPLAIN != 0 { 'p' } else if t & terrain::BUILDING != 0 { 'B' } else if t & cliff == cliff { 'C' } else if t & mask::NOT_CLEAR != 0 { 'x' } else { '.' }
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
            // Lets the city raise a temple complex to god g (0 Osiris .. 4 Bast).
            ["complexgod", g] => world.complex_gods[g.parse::<usize>()?.min(4)] = true,
            // Every pyramid's and mastaba's state in a line, and the craftsmen at it.
            ["tomb"] => {
                for b in world.buildings.iter().filter(|b| b.monument.is_some()) {
                    if let Some(s) = world.tomb_summary(b.id) {
                        let crew: Vec<(u16, i32)> = b.monument.as_ref().map_or_else(Vec::new, |m| m.craftsmen.iter().filter_map(|&(k, c)| world.figures.get(c).map(|f| (k, f.amount))).collect());
                        eprintln!("{:?} tomb {} kind {}: {s} crew {crew:?}", world.time, b.id, b.kind);
                    }
                }
            }
            // Water lifts (facing, water, staff), ditches (wet/all) and each farm's
            // irrigation and fertility.
            ["irrigation"] => {
                use osiris_sim::map::terrain;
                for b in world.buildings.iter().filter(|b| b.kind == osiris_sim::irrigation::WATER_LIFT) {
                    eprintln!("lift {} at ({},{}) facing {} water {} workers {}", b.id, b.x, b.y, b.orientation, b.water, b.workers);
                }
                let ditches: Vec<(i32, i32)> = (0..world.map.height).flat_map(|y| (0..world.map.width).map(move |x| (x, y))).filter(|&(x, y)| world.map.terrain_is(x, y, terrain::CANAL)).collect();
                let wet = ditches.iter().filter(|&&(x, y)| world.ditch_wet(x, y)).count();
                eprintln!("ditches {} wet {wet}", ditches.len());
                for b in world.buildings.iter().filter(|b| world.is_farm(b.kind)) {
                    eprintln!("farm {} kind {} at ({},{}) irrigated {} fertility {}", b.id, b.kind, b.x, b.y, world.is_irrigated(b.id), world.fertility(b.id));
                }
            }
            // The land round p for irrigation: ~ water, p floodplain, m meadow, = dry and
            // w wet ditch, L lift, B building, # road, x blocked, . clear; irrigated land
            // is upper case (P, M, :).
            ["irrmap", p] => {
                use osiris_sim::map::{mask, terrain};
                let (cx, cy) = parse_point(p)?;
                for y in cy - 8..=cy + 8 {
                    let row: String = (cx - 12..=cx + 12)
                        .map(|x| {
                            let t = world.map.terrain.at_or(x, y, 0);
                            let lit = t & terrain::IRRIGATION_RANGE != 0;
                            let id = world.map.building.at_or(x, y, 0);
                            if world.buildings.get(id).is_some_and(|b| b.kind == osiris_sim::irrigation::WATER_LIFT) {
                                'L'
                            } else if t & terrain::BUILDING != 0 {
                                'B'
                            } else if t & terrain::CANAL != 0 {
                                if world.ditch_wet(x, y) { 'w' } else { '=' }
                            } else if t & terrain::WATER != 0 {
                                '~'
                            } else if t & terrain::ROAD != 0 {
                                '#'
                            } else if t & terrain::FLOODPLAIN != 0 {
                                if lit { 'P' } else { 'p' }
                            } else if t & terrain::MEADOW != 0 {
                                if lit { 'M' } else { 'm' }
                            } else if t & mask::NOT_CLEAR != 0 {
                                'x'
                            } else if lit {
                                ':'
                            } else {
                                '.'
                            }
                        })
                        .collect();
                    eprintln!("{y:4} {row}");
                }
                eprintln!("     x from {}", cx - 12);
            }
            // Up to n spots a water lift can go, with its facing and whether it would
            // stand on the floodplain's bank.
            ["liftsites", n] => {
                let mut found = 0;
                'scan: for y in 0..world.map.height {
                    for x in 0..world.map.width {
                        if world.can_place(osiris_sim::irrigation::WATER_LIFT, x, y).is_ok() {
                            let (facing, _) = osiris_sim::irrigation::lift_site(&world.map, x, y).unwrap_or_default();
                            let bank = (0..2).any(|d| (0..2).any(|e| osiris_sim::irrigation::floodplain_bank(&world.map, x + d, y + e)));
                            eprintln!("lift site ({x},{y}) facing {facing} bank {bank}");
                            found += 1;
                            if found >= n.parse::<i32>()? {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            ["monlist"] => eprintln!("monuments {:?} complex gods {:?} debt rate {} burial {:?}", world.scenario_monuments, world.complex_gods, world.debt_rate, world.burial_needs()),
            // Puts groundwater under a rectangle, for testing wells anywhere.
            ["groundwater", a, b] => {
                let (a, b) = (parse_point(a)?, parse_point(b)?);
                for y in a.1..=b.1 {
                    for x in a.0..=b.0 {
                        world.map.terrain.update(x, y, |t| t | osiris_sim::map::terrain::GROUNDWATER);
                    }
                }
            }
            // Sets the house at p to level L and has it grow to an n x n block.
            ["expand", p, l, n] => {
                let (x, y) = parse_point(p)?;
                let id = world.map.building.at_or(x, y, 0);
                world.buildings.get(id).and_then(|b| b.house.as_ref()).context("no house there")?;
                world.set_house_level(id, l.parse()?);
                world.expand_house(id, n.parse()?);
            }
            // Houses per level ("level:count/size" with m for merged), and migration.
            ["houses"] => {
                let mut levels: std::collections::BTreeMap<String, i32> = Default::default();
                for b in world.buildings.iter() {
                    if let Some(h) = &b.house {
                        let key = format!("L{}{}{}", h.level, if h.merged { "m" } else { "" }, if b.size > 1 { format!("x{}", b.size) } else { String::new() });
                        *levels.entry(key).or_default() += 1;
                    }
                }
                eprintln!("{:?} pop {} sentiment {} migration {:?} houses {levels:?}", world.time, world.population, world.sentiment, world.migration);
            }
            ["taxrate", n] => world.finance.tax_rate = n.parse::<i32>()?.clamp(0, 25),
            ["ratings"] => {
                let (r, f, l) = (&world.ratings, &world.finance, &world.labor);
                eprintln!(
                    "{} {:?} pop {} treasury {} culture {} prosperity {}/{} monument {} kingdom {} | work {}/{} needed {} short {} unemployed {}% | last year {:?} tribute unpaid {}",
                    world.time.year, world.time.month, world.population, world.treasury, r.culture, r.prosperity, r.prosperity_max, r.monument, r.kingdom, l.employed, l.available, l.needed, l.shortage, world.unemployment, f.last_year, r.tribute_unpaid_years
                );
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

/// Every building type reachable from the build menus, in menu order.
fn menu_kinds(world: &World) -> Vec<u16> {
    fn walk(world: &World, key: &str, out: &mut Vec<u16>, depth: u32) {
        let Some(menu) = world.defs.menu(key) else { return };
        for item in &menu.items {
            if let Some(sub) = item.strip_prefix("menu_") {
                if depth < 4 {
                    walk(world, sub, out, depth + 1);
                }
            } else if let Some(d) = world.defs.building_by_key(item)
                && !out.contains(&d.id)
            {
                out.push(d.id);
            }
        }
    }
    let mut out = Vec::new();
    for m in &world.defs.menus {
        walk(world, &m.key, &mut out, 0);
    }
    out
}

/// The `fuzz` step: random buildings at valid spots, roads beside them, some clearing.
/// Lays a road grid over the most open ground near the entry, joined to the entry,
/// and returns its centre.
fn town_roads(world: &mut World) -> (i32, i32) {
    use osiris_sim::map::mask;
    let (ex, ey) = world.entry_point;
    let open = |w: &World, x: i32, y: i32| w.map.contains(x, y) && !w.map.terrain_is(x, y, mask::NOT_CLEAR) && w.map.building.at_or(x, y, 0) == 0;
    let mut best = ((ex, ey), -1);
    for cy in (ey - 40..=ey + 40).step_by(4) {
        for cx in (ex - 40..=ex + 40).step_by(4) {
            let n = (-15..=15).step_by(3).flat_map(|dy| (-15..=15).step_by(3).map(move |dx| (dx, dy))).filter(|&(dx, dy)| open(world, cx + dx, cy + dy)).count() as i32;
            if n > best.1 {
                best = ((cx, cy), n);
            }
        }
    }
    let (cx, cy) = best.0;
    for d in (-15..=15).step_by(5) {
        world.apply(&Command::Road { start: (cx - 15, cy + d), end: (cx + 15, cy + d) });
        world.apply(&Command::Road { start: (cx + d, cy - 15), end: (cx + d, cy + 15) });
    }
    world.apply(&Command::Road { start: (ex, ey), end: (cx, cy) });
    eprintln!("town at {cx},{cy} (open {})", best.1);
    (cx, cy)
}

fn fuzz(world: &mut World, n: u32, seed: u64, town: Option<(i32, i32)>) {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = move |m: i32| -> i32 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % m.max(1) as u64) as i32
    };
    let kinds = menu_kinds(world);
    let (w, h) = (world.map.width, world.map.height);
    let (mut built, mut failed) = (0, 0);
    let mut placed: std::collections::BTreeMap<u16, u32> = Default::default();
    for _ in 0..n {
        if next(100) < 5 {
            let (x, y) = (next(w), next(h));
            world.apply(&Command::Clear { x0: x, y0: y, x1: x + 2, y1: y + 2 });
            continue;
        }
        let k = if town.is_some() && next(2) == 0 { osiris_sim::buildings::kind::VACANT_LOT } else { kinds[next(kinds.len() as i32) as usize] };
        if !world.is_allowed(k) {
            continue;
        }
        let mut site = None;
        for _ in 0..400 {
            let (x, y) = match town {
                Some((cx, cy)) => (cx - 17 + next(35), cy - 17 + next(35)),
                None => (next(w), next(h)),
            };
            if world.can_place(k, x, y).is_ok() {
                site = Some((x, y));
                break;
            }
        }
        let Some((x, y)) = site else {
            failed += 1;
            continue;
        };
        let (fw, fh) = world.footprint_of(k);
        let (x1, y1) = if osiris_sim::buildings::kind::is_house(k) { (x + next(3), y + next(3)) } else { (x, y) };
        let out = world.apply(&Command::Build { kind: k, x, y, x1, y1 });
        if matches!(out, osiris_sim::Outcome::Done { .. }) {
            built += 1;
            *placed.entry(k).or_default() += 1;
            if town.is_some() {
                continue;
            }
            // A road along the front, running off towards a random point.
            let (rx, ry) = (x - 1, y + fh.max(y1 - y + 1));
            let far = (rx + next(30) - 15, ry + next(30) - 15);
            world.apply(&Command::Road { start: (rx, ry), end: (x + fw, ry) });
            world.apply(&Command::Road { start: (rx, ry), end: far });
        }
    }
    eprintln!("fuzz {n} {seed}: built {built}, no site {failed}, treasury {}, kinds {placed:?}", world.treasury);
}

