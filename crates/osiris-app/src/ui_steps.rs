//! Scripted player input (`--script "...; ui ..."`): clicks and keys given to the city
//! once it is drawn, through the same handlers the mouse and keyboard reach, so a
//! headless run (or a test) drives the windows as a player would.

use crate::game::Game;
use anyhow::{Context, Result, bail};
use osiris_render::Renderer;

/// One step of player input.
#[derive(Debug, Clone, PartialEq)]
pub enum UiStep {
    /// A left click on a map tile (the view scrolled to it first).
    Tile(i32, i32),
    /// A right click on a map tile: its information window.
    Inspect(i32, i32),
    /// A left click on a ship's tile, which takes command of it.
    Ship(ShipRef),
    /// A right click on a ship's tile: its window.
    ShipInfo(ShipRef),
    /// Order button 0-4 of the open company or ship window.
    Button(usize),
    /// A right click with nothing under it in particular: closes or cancels.
    Back,
    /// A key, by name (`h`, `n`, `a`, `e`, `k`, `r`, `w`, `escape`).
    Key(String),
    /// Time passes, and the messages it brings are put away.
    Ticks(u32),
    /// Opens an overseer by name, as the top menu does.
    Overseer(String),
    /// The military overseer's army/navy switch.
    ArmyNavy,
    /// A navy page row's button: 0 go to the ship, 1 back to its wharf, 2 Kingdom
    /// service.
    NavyRow(usize, usize),
    /// The military pages' "Dispatch now" button.
    Dispatch,
    /// Reports the city's ships and what the game has in hand.
    Report,
}

/// A ship, by figure id or as the city's first warship or transport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShipRef {
    Id(u32),
    Kind(u16),
}

impl ShipRef {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "warship" => Self::Kind(osiris_sim::navy::WARSHIP),
            "transport" => Self::Kind(osiris_sim::navy::TRANSPORT),
            id => Self::Id(id.parse()?),
        })
    }

    pub fn find(self, world: &osiris_sim::World) -> Option<osiris_sim::figures::FigureId> {
        match self {
            Self::Id(id) => world.figures.get(id).map(|f| f.id),
            Self::Kind(k) => world.figures.iter().find(|f| f.kind == k && !f.dead && f.ship.is_some()).map(|f| f.id),
        }
    }
}

/// Parses the words after `ui`.
pub fn parse(words: &[&str]) -> Result<UiStep> {
    let point = |s: &str| -> Result<(i32, i32)> {
        let (x, y) = s.split_once(',').context("expected x,y")?;
        Ok((x.trim().parse()?, y.trim().parse()?))
    };
    Ok(match words {
        ["tile", p] => {
            let (x, y) = point(p)?;
            UiStep::Tile(x, y)
        }
        ["inspect", p] => {
            let (x, y) = point(p)?;
            UiStep::Inspect(x, y)
        }
        ["ship", s] => UiStep::Ship(ShipRef::parse(s)?),
        ["shipinfo", s] => UiStep::ShipInfo(ShipRef::parse(s)?),
        ["button", n] => UiStep::Button(n.parse()?),
        ["back"] => UiStep::Back,
        ["key", k] => UiStep::Key(k.to_string()),
        ["ticks", n] => UiStep::Ticks(n.parse()?),
        ["overseer", name] => UiStep::Overseer(name.to_string()),
        ["armynavy"] => UiStep::ArmyNavy,
        ["navyrow", row, what] => UiStep::NavyRow(
            row.parse()?,
            match *what {
                "go" => 0,
                "back" => 1,
                "service" => 2,
                _ => bail!("navyrow: go, back or service"),
            },
        ),
        ["dispatch"] => UiStep::Dispatch,
        ["report"] => UiStep::Report,
        _ => bail!("unknown ui step: {}", words.join(" ")),
    })
}

/// The screen point of tile `(x, y)`'s middle, the view first scrolled to it.
fn tile_point(game: &mut Game, r: &mut Renderer, x: i32, y: i32) -> [f32; 2] {
    game.view.center_on(r, &game.world.map, x, y);
    let w = crate::city_view::tile_to_world(&game.world.map, x, y);
    let w = [w[0] + crate::city_view::TILE_W / 2.0, w[1] + crate::city_view::TILE_H / 2.0];
    [(w[0] - r.camera.x) * r.camera.zoom, (w[1] - r.camera.y) * r.camera.zoom]
}

/// A left click at screen point `p`, between two drawn frames.
fn left_click(game: &mut Game, r: &mut Renderer, p: [f32; 2]) {
    game.set_cursor(r, p);
    if let Some((x, y)) = game.press_at(r) {
        game.view.center_on(r, &game.world.map, x, y);
    }
    game.release();
    game.draw(r);
}

/// A right click at screen point `p`, as the window's handler gives it: anything
/// open is cancelled, and with nothing open the tile is inspected.
fn right_click(game: &mut Game, r: &mut Renderer, p: [f32; 2]) {
    game.set_cursor(r, p);
    let inspect = game.idle();
    game.cancel();
    if inspect {
        game.inspect();
    }
    game.draw(r);
}

/// Runs one step.
pub fn run(game: &mut Game, r: &mut Renderer, step: &UiStep) -> Result<()> {
    game.draw(r);
    match step {
        UiStep::Tile(x, y) => {
            let p = tile_point(game, r, *x, *y);
            left_click(game, r, p);
        }
        UiStep::Inspect(x, y) => {
            let p = tile_point(game, r, *x, *y);
            right_click(game, r, p);
        }
        UiStep::Ship(s) | UiStep::ShipInfo(s) => {
            let f = s.find(&game.world).and_then(|id| game.world.figures.get(id)).with_context(|| format!("no ship {s:?}"))?;
            let p = tile_point(game, r, f.x, f.y);
            if matches!(step, UiStep::Ship(_)) {
                left_click(game, r, p);
            } else {
                right_click(game, r, p);
            }
        }
        UiStep::Button(i) => {
            let p = game.info.as_ref().context("no window open")?.order_button_centre(*i);
            left_click(game, r, p);
        }
        UiStep::Back => {
            let p = game.cursor_point();
            right_click(game, r, p);
        }
        UiStep::Key(k) => {
            use winit::keyboard::KeyCode;
            let code = match k.as_str() {
                "h" => KeyCode::KeyH,
                "n" => KeyCode::KeyN,
                "a" => KeyCode::KeyA,
                "e" => KeyCode::KeyE,
                "k" => KeyCode::KeyK,
                "r" => KeyCode::KeyR,
                "w" => KeyCode::KeyW,
                "escape" => KeyCode::Escape,
                _ => bail!("ui key: no key {k}"),
            };
            crate::key_pressed(game, code, false);
            game.draw(r);
        }
        UiStep::Ticks(n) => {
            for _ in 0..*n {
                game.world.tick();
            }
            // The messages of the while are read and put away.
            game.close_dialog();
        }
        UiStep::Overseer(name) => {
            let a = crate::advisors::ALL.iter().find(|a| format!("{a:?}").eq_ignore_ascii_case(name)).with_context(|| format!("no overseer {name}"))?;
            game.open_advisor(*a);
            game.draw(r);
        }
        UiStep::ArmyNavy | UiStep::NavyRow(..) | UiStep::Dispatch => {
            let [px, py] = crate::advisors::panel_origin(r.screen);
            let rect = match step {
                UiStep::ArmyNavy => crate::advisors::ARMY_NAVY,
                UiStep::Dispatch => crate::advisors::DISPATCH,
                UiStep::NavyRow(row, col) => {
                    let b = crate::advisors::navy_button([0.0, 0.0], *row, *col);
                    [b[0], b[1], b[2], b[3]]
                }
                _ => unreachable!(),
            };
            left_click(game, r, [px + rect[0] + rect[2] / 2.0, py + rect[1] + rect[3] / 2.0]);
        }
        UiStep::Report => report(game),
    }
    Ok(())
}

/// The city's ships, and what the game has in hand.
fn report(game: &Game) {
    let w = &game.world;
    for f in w.figures.iter().filter(|f| osiris_sim::navy::is_city_ship(f.kind)) {
        let s = f.ship.as_deref();
        eprintln!(
            "  ui ship {} kind {} at {},{} action {} hull {}% docked {} anchor {:?} aboard {:?} service {}",
            f.id,
            f.kind,
            f.x,
            f.y,
            f.action,
            w.hull_percent(f.id),
            w.ship_docked(f.id),
            s.map(|s| s.anchor),
            s.and_then(|s| s.aboard),
            s.is_some_and(|s| s.service)
        );
    }
    eprintln!(
        "  ui selected ship {:?} window {} overseer {:?}",
        game.selected_ship,
        game.info.is_some(),
        game.advisors.as_ref().map(|a| a.current)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ShipClick;
    use crate::info::Target;
    use osiris_sim::Command;
    use osiris_sim::figures::FigureId;
    use osiris_sim::navy::{ShipOrder, TRANSPORT, WARSHIP, ship};
    use osiris_sim::replay::Entry;

    /// The Sandbox map's river with a shipwright stocked with timber, a warship wharf
    /// and a transport wharf on its north bank, a small town on the south bank, and
    /// everything fully staffed. A warship and a transport are moored by the time
    /// 4000 ticks have run.
    const TOWN: &str = "safe; fullstaff; allowall; globallabor; noinvasions; road 150,126 176,126; road 176,126 176,128; road 176,128 190,128; road 170,126 170,131; build 10 160,127 168,130; road 168,110 176,110; road 176,110 176,109; road 176,109 196,109; build 74 180,110; build 182 186,110; build 181 172,111; stock 180,110 20 200";
    /// An archers' fort and a recruiter with weapons north of the wharves: sixteen
    /// archers stand at the fort by 9000 ticks.
    const FORT: &str = "road 168,109 168,95; build 44 170,100; build 95 165,104; stock 165,104 10 800";

    /// Builds the town after `script`, then gives the game and a renderer to `f`,
    /// recording every command from there. Skips (passes) without the game data, or
    /// without a graphics adapter to draw with.
    fn play(script: &str, f: impl FnOnce(&mut Game, &mut Renderer)) {
        let Some(data) = crate::editor::test_data() else { return };
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).expect("image library");
        let assets = crate::load_assets(&data, &library).expect("assets");
        let mut world = crate::new_world(&assets, &crate::Source::Map(data.join("Maps/Sandbox.map")), osiris_sim::difficulty::NORMAL).expect("Sandbox");
        crate::script::run_script(&mut world, script).expect("script");
        world.start_recording().expect("recording");
        let images = crate::sidebar::SidebarImages::load(&library).expect("sidebar images");
        let mut game = Game::new(world, images, assets.text.clone(), assets.messages.clone(), None);
        game.close_dialog();
        let out = std::env::temp_dir().join(format!("osiris-ui-steps-{}-{:?}.png", std::process::id(), std::thread::current().id()));
        let mut f = Some(f);
        let shot = crate::gfx::screenshot(library, (640, 480), &out, |r| {
            if let Some(f) = f.take() {
                f(&mut game, r);
            }
        });
        let _ = std::fs::remove_file(&out);
        if let Err(e) = shot {
            assert!(f.is_some(), "{e}");
            eprintln!("skipped: no renderer ({e})");
        }
    }

    fn step(game: &mut Game, r: &mut Renderer, s: &str) {
        let words: Vec<&str> = s.split_whitespace().collect();
        run(game, r, &parse(&words).expect("step")).expect(s);
    }

    /// The commands applied since the last call.
    fn commands(game: &mut Game, seen: &mut usize) -> Vec<Command> {
        let events = game.world.replay().expect("recording").events;
        let new = events[*seen..]
            .iter()
            .filter_map(|e| match &e.entry {
                Entry::Command(c) => Some(c.clone()),
                Entry::Undo => None,
            })
            .collect();
        *seen = events.len();
        new
    }

    fn ship_of(game: &Game, kind: u16) -> FigureId {
        game.world.figures.iter().find(|f| f.kind == kind && f.ship.is_some()).map(|f| f.id).expect("a ship of that kind")
    }

    fn action(game: &Game, s: FigureId) -> u16 {
        game.world.figures.get(s).map_or(0, |f| f.action)
    }

    fn anchor(game: &Game, s: FigureId) -> (i32, i32) {
        game.world.figures.get(s).and_then(|f| f.ship.as_ref()).map(|s| s.anchor).unwrap_or_default()
    }

    fn marked(game: &Game, s: FigureId) -> bool {
        game.world.figures.get(s).and_then(|f| f.ship.as_ref()).is_some_and(|s| s.service)
    }

    #[test]
    fn a_warship_takes_each_order_from_its_window() {
        play(&format!("{TOWN}; ticks 4000"), |game, r| {
            let w = ship_of(game, WARSHIP);
            let mut seen = 0;
            assert_eq!(action(game, w), ship::ENGAGE, "a new warship guards its wharf");

            // A right click on it opens its window.
            step(game, r, "shipinfo warship");
            assert!(matches!(game.info.as_ref().map(|i| i.target), Some(Target::Ship(s)) if s == w));

            // Hold: the order, and the ship in hand for a click on the water. It stays
            // in hand for the next click, until a right click lets it go.
            step(game, r, "button 0");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Hold }]);
            assert_eq!((action(game, w), game.selected_ship, game.info.is_some()), (ship::HOLD, Some((w, ShipClick::Move)), false));
            step(game, r, "tile 186,118");
            assert_eq!(commands(game, &mut seen), [Command::MoveShip { ship: w, x: 186, y: 118 }]);
            assert_eq!((action(game, w), anchor(game, w)), (ship::HOLD, (186, 118)));
            assert!(game.selected_ship.is_some(), "a ship holding stays in hand");
            step(game, r, "back");
            assert_eq!(game.selected_ship, None);
            step(game, r, "ticks 400");
            let f = game.world.figures.get(w).expect("warship");
            assert_eq!((f.x, f.y), (186, 118), "it sailed there");

            // Engage: in hand as well.
            step(game, r, "shipinfo warship");
            step(game, r, "button 1");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Engage }]);
            assert_eq!((action(game, w), game.selected_ship), (ship::ENGAGE, Some((w, ShipClick::Move))));
            step(game, r, "back");

            // Seek and destroy: the window closes and nothing is in hand.
            step(game, r, "shipinfo warship");
            step(game, r, "button 2");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Seek }]);
            assert_eq!((action(game, w), game.selected_ship, game.info.is_some()), (ship::SEEK, None, false));

            // Repair is greyed while the hull is whole: the click does nothing.
            step(game, r, "shipinfo warship");
            step(game, r, "button 3");
            assert_eq!(commands(game, &mut seen), []);
            assert!(game.info.is_some());

            // Return to Wharf, away from it.
            step(game, r, "button 4");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Return }]);
            assert_eq!((action(game, w), game.selected_ship, game.info.is_some()), (ship::WARSHIP_HOME, None, false));

            // Holed, it can be sent for repairs; bound for the yard, Repair greys.
            game.world.figures.get_mut(w).expect("warship").damage = 100;
            step(game, r, "shipinfo warship");
            step(game, r, "button 3");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Repair }]);
            assert_eq!((action(game, w), game.info.is_some()), (ship::WARSHIP_TO_YARD, false));
            step(game, r, "shipinfo warship");
            step(game, r, "button 3");
            assert_eq!(commands(game, &mut seen), []);

            // A left click takes command of it, and a key gives it an order.
            step(game, r, "back");
            step(game, r, "ship warship");
            assert_eq!(game.selected_ship, Some((w, ShipClick::Move)));
            step(game, r, "key h");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: w, order: ShipOrder::Hold }]);
        });
    }

    #[test]
    fn a_transport_carries_a_company_across_by_its_window() {
        play(&format!("{TOWN}; {FORT}; ticks 9000"), |game, r| {
            let t = ship_of(game, TRANSPORT);
            let mut seen = 0;
            assert_eq!(game.world.company_men(0), 16);
            assert_eq!(action(game, t), ship::EVADE, "a new transport evades by its wharf");

            // Embark, then a click on one of the company's archers.
            step(game, r, "shipinfo transport");
            step(game, r, "button 2");
            assert_eq!(game.selected_ship, Some((t, ShipClick::Embark)));
            step(game, r, "tile 174,100");
            assert_eq!(commands(game, &mut seen), [Command::Embark { ship: t, company: 0 }]);
            assert_eq!(game.selected_ship, None);
            step(game, r, "ticks 3000");
            assert_eq!(game.world.company_ship(0), Some(t));

            // Evade: the window closes, nothing in hand.
            step(game, r, "shipinfo transport");
            step(game, r, "button 1");
            assert_eq!(commands(game, &mut seen), [Command::ShipOrder { ship: t, order: ShipOrder::Evade }]);
            assert_eq!((game.selected_ship, game.info.is_some()), (None, false));

            // Disembark on the south bank.
            step(game, r, "shipinfo transport");
            step(game, r, "button 2");
            assert_eq!(game.selected_ship, Some((t, ShipClick::Disembark)));
            step(game, r, "tile 170,127");
            assert_eq!(commands(game, &mut seen), [Command::Disembark { ship: t, x: 170, y: 127 }]);
            step(game, r, "ticks 3000");
            assert_eq!(game.world.company_ship(0), None);
            let soldiers = game.world.military.companies[0].soldiers.clone();
            assert!(soldiers.iter().all(|&s| game.world.figures.get(s).is_some_and(|f| f.y > 120)), "the archers are ashore on the south bank");

            // Hold, and two clicks on the water while it stays in hand.
            step(game, r, "shipinfo transport");
            step(game, r, "button 0");
            step(game, r, "tile 176,118");
            step(game, r, "tile 178,118");
            assert_eq!(
                commands(game, &mut seen),
                [Command::ShipOrder { ship: t, order: ShipOrder::Hold }, Command::MoveShip { ship: t, x: 176, y: 118 }, Command::MoveShip { ship: t, x: 178, y: 118 }]
            );
            assert_eq!((action(game, t), anchor(game, t), game.selected_ship.map(|s| s.0)), (ship::HOLD, (178, 118), Some(t)));
        });
    }

    /// A call for troops from a city reached by sea, due in three months.
    fn sea_call(world: &mut osiris_sim::World) -> usize {
        let route = world.trade.routes.len();
        world.trade.routes.push(osiris_sim::trade::TradeRoute { points: vec![(0, 0), (800, 0)], step: 5, sea: true, ..Default::default() });
        world.trade.cities.push(osiris_sim::trade::TradeCity { sea: true, route: route as u8, ..Default::default() });
        let i = world.scenario_events.request_troops_now(50);
        let due = world.month_count() + 3;
        let e = &mut world.scenario_events.list[i];
        e.city = Some(world.trade.cities.len() - 1);
        e.route = route as i32;
        e.due = Some(due);
        e.months_left = 3;
        i
    }

    #[test]
    fn the_navy_page_marks_a_warship_and_sends_it_to_the_kingdom() {
        play(&format!("{TOWN}; ticks 4000"), |game, r| {
            let w = ship_of(game, WARSHIP);
            let mut seen = 0;
            step(game, r, "overseer military");
            step(game, r, "armynavy");
            assert!(game.advisors.as_ref().is_some_and(|a| a.navy_page()));

            // With no call by sea the mark is refused, and a popup says why.
            step(game, r, "navyrow 0 service");
            assert_eq!(commands(game, &mut seen), [Command::ShipService(w)]);
            assert!(!marked(game, w));
            assert!(game.advisors.as_ref().and_then(|a| a.notice()).is_some());
            step(game, r, "back");

            // A city by sea calls: marked, the heading turns to Dispatch now, which
            // sends it off.
            let call = sea_call(&mut game.world);
            step(game, r, "navyrow 0 service");
            assert_eq!(commands(game, &mut seen), [Command::ShipService(w)]);
            assert!(marked(game, w));
            step(game, r, "dispatch");
            assert_eq!(commands(game, &mut seen), [Command::DispatchRequest(call)]);
            assert_eq!(action(game, w), ship::TO_BATTLE);
            assert_eq!(game.advisors.as_ref().map(|a| a.current), Some(crate::advisors::Advisor::Military));
        });
    }

    #[test]
    fn a_mark_left_from_a_closed_call_comes_off_on_the_navy_page() {
        play(&format!("{TOWN}; ticks 4000"), |game, r| {
            let w = ship_of(game, WARSHIP);
            let call = sea_call(&mut game.world);
            step(game, r, "overseer military");
            step(game, r, "armynavy");
            step(game, r, "navyrow 0 service");
            assert!(marked(game, w));
            // The call is withdrawn: the page shows the mark greyed and takes it off.
            game.world.scenario_events.list[call].active = false;
            assert!(!game.world.sea_troops_wanted());
            game.draw(r);
            assert!(!marked(game, w));
        });
    }

    #[test]
    fn dispatch_with_two_calls_by_sea_sends_the_player_to_choose() {
        play(&format!("{TOWN}; ticks 4000"), |game, r| {
            let mut seen = 0;
            sea_call(&mut game.world);
            sea_call(&mut game.world);
            // Going from the military overseer to the political one by the strip says
            // nothing.
            step(game, r, "overseer military");
            let political = crate::advisors::strip_button_centre(r.screen, crate::advisors::Advisor::Political);
            left_click(game, r, political);
            let adv = game.advisors.as_ref().expect("overseers");
            assert_eq!((adv.current, adv.notice()), (crate::advisors::Advisor::Political, None));
            step(game, r, "overseer military");
            step(game, r, "armynavy");
            step(game, r, "navyrow 0 service");
            commands(game, &mut seen);
            // Dispatch now with two calls open sends nothing, and asks the player to
            // choose in the Political Overseer (text group 5, 102 and 103).
            step(game, r, "dispatch");
            assert_eq!(commands(game, &mut seen), []);
            let adv = game.advisors.as_ref().expect("overseers");
            assert_eq!((adv.current, adv.notice()), (crate::advisors::Advisor::Political, Some((102, 103))));
        });
    }
}
