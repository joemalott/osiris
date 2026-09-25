//! The army. Each fort holds a company of up to sixteen soldiers of its kind
//! (infantry, archers or charioteers) with a standard bearer, mustered on the parade
//! ground beside it. A recruiter enlists men for the nearest fort short of them,
//! outfitting each infantryman with a load of weapons and each charioteer with a load
//! of chariots; archers bring their own bows. The player sends a company to a spot
//! (it forms up around its standard) or back to its fort.
//!
//! Fighting, as the original does it: a soldier or invader who takes on an enemy
//! next to him strikes first after 12 ticks and then every 24 (every 12 against a
//! citizen or criminal); the man he goes for, if not already fighting, turns on him
//! and strikes after 24. No more than two set on a man who is already fighting.
//! A blow does the striker's attack × (20 − the target's armour) / 20, the armour
//! counted from 0 to 20 and adjusted as below. The city's missiles (javelins) do
//! their thrower's missile attack × (20 − the target's armour against missiles) / 20;
//! the invaders' (spears in the original, whatever the archer) do the spear's missile
//! attack, 10, or nothing at all to a man whose armour against missiles comes to more
//! than 10. A figure dies when its damage passes its hit points, and lies on the
//! field a while before it is gone.
//!
//! Orders: a company holds its ground in tight or loose formation, fighting only what
//! comes at it; or it engages enemies that come near, in formation; or breaks ranks
//! to mop up every enemy it can find; or, charioteers, charges. A company standing
//! halted in the field fights by its orders, and its men fight worse when they have
//! turned from the way it faces (a flank or rear attack):
//!
//! - infantry in tight formation strike at +4 and take blows at +4 armour;
//! - in loose formation, infantry and archers have +4 armour against missiles;
//! - mopping up, men strike at +2 but have −2 armour, against missiles too;
//! - a man turned from his company's front has −4 armour, against missiles too.
//!
//! Infantry and charioteers who strike a man busy with someone else from behind
//! strike at +4. Charioteers charging do not stop to fight but ride the enemy down:
//! each they run into takes their attack, four times over once they have run six
//! tiles straight (until twenty), and his armour does him no good. The armour a
//! company's experience was meant to add, (experience + 10) / 20, counts against
//! missiles only: in melee the original takes the striker's company's, which an
//! invader hasn't.
//!
//! Morale: every man lost shakes his side, the more so the bigger the share of it
//! that fell; a company rests its spirits at the fort month by month and loses heart
//! when kept out long. A company whose morale breaks runs home, and a broken army
//! runs for the edge of the map.
//!
//! Experience: a company's experience (0 to 100) is the average of its men's. A raw
//! recruit brings none, one who passed through a military academy 25, and a temple
//! complex to Seth adds 10 either way; every enemy a company's man kills adds one.
//! Experienced companies count for more in distant battles, and Seth, when upset,
//! takes the most experienced. The ball on a company's standard climbs with it.

use crate::balance::UnitStats;
use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

pub const ARCHER: u16 = 11;
pub const CHARIOTEER: u16 = 12;
pub const INFANTRY: u16 = 13;
pub const STANDARD_BEARER: u16 = 14;
pub const ARROW: u16 = 59;
pub const JAVELIN: u16 = 60;

pub const FORT_CHARIOTEERS: u16 = 40;
pub const FORT_ARCHERS: u16 = 44;
pub const FORT_INFANTRY: u16 = 45;
pub const FORT_GROUND: u16 = 54;
pub const RECRUITER: u16 = 95;
const ACADEMIES: [u16; 3] = [94, 185, 186];
pub const WEAPONS: u16 = 10;
pub const CHARIOTS: u16 = 28;

/// What a recruiter must hand a new soldier of `kind`, if anything.
fn outfit(kind: u16) -> Option<u16> {
    match kind {
        INFANTRY => Some(WEAPONS),
        CHARIOTEER => Some(CHARIOTS),
        _ => None,
    }
}

/// The invaders' missile in the original, whatever the archer: its row of the
/// figure model says what a hit does.
const SPEAR: u16 = 71;

/// The row of the figure model a figure type fights by. The original's infantry are
/// type 12 and its charioteers type 13 (a charioteers' fort raises a company of type
/// 13, and the company windows and the Military overseer call them so), where Osiris
/// numbers them the other way round.
fn model_row(kind: u16) -> u16 {
    match kind {
        INFANTRY => 12,
        CHARIOTEER => 13,
        k => k,
    }
}

/// Soldiers a company holds.
pub const COMPANY_SIZE: usize = 16;
/// Ticks a blow takes to land.
const BLOW_TICKS: u16 = 24;
/// Where a blow's count starts when it strikes the first blow of a fight, or a
/// citizen or criminal.
pub(crate) const QUICK_BLOW: u16 = 12;
/// The most armour counts for.
const MAX_ARMOR: i32 = 20;
/// The straight run (tiles) over which a charging chariot tramples at four times
/// its attack.
const CHARGE_RUN: std::ops::Range<i32> = 6..20;
/// Ticks a fallen figure lies on the field.
const CORPSE_TICKS: i32 = 200;
/// Where the parade ground sits beside its fort.
const GROUND_OFFSET: (i32, i32) = (3, -1);
/// The 4x4 tiles of the parade ground beside a fort placed at `(x, y)`.
pub fn fort_ground(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    let (gx, gy) = (x + GROUND_OFFSET.0, y + GROUND_OFFSET.1);
    (gy..gy + 4).flat_map(move |yy| (gx..gx + 4).map(move |xx| (xx, yy)))
}

/// Morale at or below which a side breaks and runs.
pub const BROKEN_MORALE: i32 = 20;

/// Morale lost for a death, by the share of the side it was (percent).
pub fn morale_loss(share_pct: i32) -> i32 {
    match share_pct {
        p if p < 8 => 5,
        p if p < 10 => 7,
        p if p < 14 => 10,
        p if p < 20 => 12,
        p if p < 30 => 15,
        _ => 20,
    }
}

/// The highest morale a company of `kind` reaches: charioteers 80, infantry and
/// archers 60. (The original would raise it by 20 for a company marked as trained,
/// but nothing ever sets that mark: the academy gives experience instead.)
fn morale_cap(kind: u16) -> i32 {
    if kind == CHARIOTEER { 80 } else { 60 }
}

/// The most experience a company can have.
pub const MAX_EXPERIENCE: i32 = 100;
/// What a recruit trained at a military academy brings to his company.
const ACADEMY_EXPERIENCE: i32 = 25;
/// What a temple complex to Seth adds to every recruit's.
const SETH_EXPERIENCE: i32 = 10;

/// A company's experience after a recruit bringing `brings` joins it, `n` men with
/// him. The original counts him as half a man, except that an academy recruit
/// joining a company greener than himself counts fully (a complex to Seth makes
/// him bring 35, so he counts as half again). Rounded up.
fn with_recruit(experience: i32, n: i32, brings: i32) -> i32 {
    let n = n.max(1);
    if experience < ACADEMY_EXPERIENCE && brings == ACADEMY_EXPERIENCE {
        return ceil_div((n - 1) * experience + brings, n);
    }
    ceil_div((2 * n - 1) * experience + brings, 2 * n).min(MAX_EXPERIENCE)
}

fn ceil_div(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// The experience rank of a company (0 "Green" to 5 "The best", text 138:60-65),
/// which also picks its icon in the fort window and the Military overseer's report.
pub fn experience_rank(experience: i32) -> usize {
    ((experience + 10) / 20).clamp(0, 5) as usize
}

/// The frame of the experience ball on a company's standard: 0 at the top of the
/// staff, 20 at the bottom.
pub fn experience_ball(experience: i32) -> u32 {
    (20 - (experience + 3) / 5).clamp(0, 20) as u32
}

/// Whether killing a figure of `kind` teaches a soldier anything: not animals or
/// robbers.
fn teaches(kind: u16) -> bool {
    !matches!(kind, 68..=70 | 82..=84 | 102..=104 | crate::crime::ROBBER)
}

/// Action states of fighters.
pub mod action {
    pub const AT_REST: u16 = 80;
    pub const GOING_TO_FORT: u16 = 81;
    pub const GOING_TO_STANDARD: u16 = 83;
    pub const GOING_TO_ACADEMY: u16 = 85;
    pub const GOING_ABROAD: u16 = 87;
    pub const CHASING: u16 = 86;
    pub const AT_STANDARD: u16 = 84;
    pub const ATTACK: u16 = 90;
    pub const FLEEING: u16 = 148;
    pub const CORPSE: u16 = 149;
}

/// A fort's company.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Company {
    pub fort: BuildingId,
    pub ground: BuildingId,
    /// Soldier figure type.
    pub kind: u16,
    pub soldiers: Vec<FigureId>,
    /// Recruits on their way from the recruiter.
    pub recruits: Vec<FigureId>,
    pub standard: FigureId,
    /// Where the company forms up, when it is out of its fort.
    pub standard_tile: (i32, i32),
    pub at_fort: bool,
    pub morale: i32,
    /// Months out of the fort.
    #[serde(default)]
    pub months_away: i32,
    /// The company's experience, 0 to 100: the average of its men's.
    #[serde(default)]
    pub experience: i32,
    /// Old saves' mark of academy training, turned into experience on loading.
    #[serde(default, skip_serializing)]
    trained: bool,
    /// Marked for Kingdom service: it answers Pharaoh's calls for troops.
    #[serde(default)]
    pub kingdom_service: bool,
    /// Soldiers away fighting for the Kingdom.
    #[serde(default)]
    pub abroad: i32,
    #[serde(default)]
    pub order: Order,
    /// Charioteers' horses: the ticks of charging they have left in them.
    #[serde(default = "full_wind")]
    pub wind: i32,
    /// The tick the horses last ran or rested, so a company tires once a tick.
    #[serde(default)]
    pub wind_tick: u64,
    /// The way the company faces in the field (a direction): the way it last
    /// marched.
    #[serde(default)]
    pub facing: u8,
    /// The company window's "rotate the line" switch: the next tight or loose
    /// formation ordered is turned.
    #[serde(default)]
    pub rotate: bool,
    /// Whether the tight or loose line the company holds was turned when ordered.
    #[serde(default)]
    pub turned: bool,
    /// Charioteers who have charged can't charge again until their horses have had
    /// [`CHARGE_REST`] soldier-turns to recover.
    #[serde(default)]
    pub charged: bool,
    #[serde(default)]
    pub charge_rest: i32,
}

impl Company {
    /// An old save's mark of academy training becomes an academy recruit's
    /// experience.
    fn upgrade(&mut self) {
        if std::mem::take(&mut self.trained) {
            self.experience = self.experience.max(ACADEMY_EXPERIENCE);
        }
    }
}

/// Ticks a company of charioteers can charge at top speed before its horses tire,
/// and how many ticks of rest win back one. (The manual only says the horses tire
/// after a great distance and must rest; these amounts are placeholders.)
const WIND: i32 = 300;
const REST_PER_WIND: u64 = 2;

fn full_wind() -> i32 {
    WIND
}

/// After a charge, the turns its men must take (each soldier's turn counts) before
/// the company can charge again, as the original counts them.
const CHARGE_REST: i32 = 2000;

/// A company's standing orders. (The original keeps them as a number: tight 1 or 2
/// and loose 3 or 4, by which way the line is turned; engage 15, mop up 6, charge 0.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Order {
    #[default]
    HoldTight,
    HoldLoose,
    Engage,
    MopUp,
    Charge,
}

impl Order {
    /// How far the company goes after enemies, in tiles (0: only those at hand).
    fn reach(self) -> i32 {
        match self {
            Order::HoldTight | Order::HoldLoose => 0,
            Order::Engage => 6,
            Order::MopUp | Order::Charge => 20,
        }
    }
}

/// Troops sent to fight for the Kingdom, and the request they answer.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DistantBattle {
    pub request: usize,
    pub enemy: i32,
    pub strength: i32,
    pub companies: Vec<usize>,
    /// Months until the battle, or, once fought, until the survivors are home.
    pub months: i32,
    pub fought: bool,
}

/// Who is on the field this tick, gathered once so fighters needn't search every
/// figure: the city's fighters (soldiers, constables, sentries) and the invaders.
#[derive(Debug, Clone, Default)]
pub struct Combatants {
    pub defenders: Vec<(FigureId, i32, i32)>,
    pub invaders: Vec<(FigureId, i32, i32)>,
    /// The wild beasts about, whom the city's men fight like invaders.
    pub predators: Vec<(FigureId, i32, i32)>,
}

/// Whether direction `a` is `b` or one step either side of it: a man fighting that
/// way faces the way his company does, or strikes at another's back.
fn toward(a: u8, b: u8) -> bool {
    (a as i32 - b as i32).rem_euclid(8) <= 1 || (b as i32 - a as i32).rem_euclid(8) <= 1
}

/// The direction from `from` to `to`, to the nearest eighth of the compass.
fn general_direction(from: (i32, i32), to: (i32, i32)) -> Option<u8> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let sx = if 2 * dx.abs() < dy.abs() { 0 } else { dx.signum() };
    let sy = if 2 * dy.abs() < dx.abs() { 0 } else { dy.signum() };
    crate::figures::direction_to((0, 0), (sx, sy))
}

/// A company standing halted in the field, as a man of it fights: its arm, its
/// orders, and whether he faces the way it does.
#[derive(Debug, Clone, Copy)]
struct Line {
    kind: u16,
    order: Order,
    facing: bool,
}

impl Line {
    /// What the line adds to its man's attack.
    fn attack(self) -> i32 {
        match (self.kind, self.order) {
            (_, _) if !self.facing => 0,
            (INFANTRY, Order::HoldTight) => 4,
            (_, Order::MopUp) => 2,
            _ => 0,
        }
    }

    /// What the line adds to its man's armour against blows.
    fn armor(self) -> i32 {
        match (self.kind, self.order) {
            (_, _) if !self.facing => -4,
            (INFANTRY, Order::HoldTight) => 4,
            (_, Order::MopUp) => -2,
            _ => 0,
        }
    }

    /// What the line adds to its man's armour against missiles.
    fn missile_armor(self) -> i32 {
        match (self.kind, self.order) {
            (_, _) if !self.facing => -4,
            (INFANTRY | ARCHER, Order::HoldLoose) => 4,
            (_, Order::MopUp) => -2,
            _ => 0,
        }
    }
}

/// The damage a blow or missile of `attack` does against `armor`, the armour
/// counted from 0 to 20.
fn blow(attack: i32, armor: i32) -> i32 {
    if attack == 0 {
        return 0;
    }
    ((MAX_ARMOR - armor.clamp(0, MAX_ARMOR)) * attack / MAX_ARMOR).max(0)
}

/// The damage an invader's missile of `attack` does against `armor`: the original
/// works it out as (30 − armour) / 20 × attack, so all of it up to 10 armour and
/// none past.
fn spear(attack: i32, armor: i32) -> i32 {
    if attack == 0 {
        return 0;
    }
    ((30 - armor.clamp(0, MAX_ARMOR)) / MAX_ARMOR * attack).max(0)
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Military {
    pub companies: Vec<Company>,
    #[serde(default)]
    pub battle: Option<DistantBattle>,
}

/// Months troops take to reach a distant battle, and to come home.
const TRAVEL_MONTHS: i32 = 2;

/// Share of the troops lost in a won battle, by how far they outnumbered the enemy
/// (their surplus as a percentage of their strength).
fn battle_losses(advantage_pct: i32) -> i32 {
    match advantage_pct {
        a if a < 10 => 70,
        a if a < 25 => 50,
        a if a < 50 => 25,
        a if a < 75 => 15,
        a if a < 100 => 10,
        a if a < 150 => 5,
        _ => 0,
    }
}

/// Days a recruiter waits between recruits, by how well it is staffed (the
/// original's table); none without staff.
fn recruit_delay(workers: i32, needed: i32) -> Option<i32> {
    if workers <= 0 {
        return None;
    }
    Some(match workers * 100 / needed.max(1) {
        p if p >= 100 => 8,
        p if p >= 75 => 12,
        p if p >= 50 => 16,
        p if p >= 25 => 32,
        _ => 48,
    })
}

/// The fort kinds and the soldiers they hold.
pub fn fort_soldier(k: u16) -> Option<u16> {
    match k {
        FORT_CHARIOTEERS => Some(CHARIOTEER),
        FORT_ARCHERS => Some(ARCHER),
        FORT_INFANTRY => Some(INFANTRY),
        _ => None,
    }
}

pub fn is_soldier(k: u16) -> bool {
    matches!(k, ARCHER | CHARIOTEER | INFANTRY)
}

/// Where each of a company's sixteen men stands relative to its standard, by the
/// original's formation layouts: the charge's close block; the tight double line,
/// across or turned; the loose staggered line, across or turned; and mopping up,
/// engaging and at rest on the parade ground, each a four-by-four block (the
/// original's layouts 0, 1-4, 6, 15 and 7).
const LAYOUTS: [[(i8, i8); COMPANY_SIZE]; 8] = [
    [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (-1, 1), (0, -1), (1, -1), (-1, -1), (2, -1), (2, 0), (2, 1), (0, 2), (1, 2), (-1, 2), (2, 2)],
    [(0, 0), (0, 1), (-1, 0), (1, 0), (-1, 1), (1, 1), (-2, 0), (-2, 1), (2, 0), (2, 1), (-3, 0), (-3, 1), (3, 0), (3, 1), (-4, 0), (-4, 1)],
    [(0, 0), (0, -1), (0, 1), (1, 0), (1, -1), (1, 1), (0, -2), (1, -2), (0, 2), (1, 2), (0, -3), (1, -3), (0, 3), (1, 3), (0, -4), (1, -4)],
    [(0, 0), (2, 0), (-2, 0), (1, 1), (-1, 1), (3, 1), (-3, 1), (4, 0), (-4, 0), (5, 1), (6, 0), (-5, 1), (-6, 0), (7, 1), (8, 0), (-7, 1)],
    [(0, 0), (0, -2), (0, 2), (1, -1), (1, 1), (1, -3), (1, 3), (0, -4), (0, 4), (1, -5), (0, -6), (1, 5), (0, 6), (1, -7), (0, -8), (1, 7)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
];

/// The row of [`LAYOUTS`] a company in the field stands in: by its orders, and for
/// the held lines by which way the line was turned when they were given.
fn layout(order: Order, turned: bool) -> usize {
    match order {
        Order::Charge => 0,
        Order::HoldTight => 1 + turned as usize,
        Order::HoldLoose => 3 + turned as usize,
        Order::MopUp => 5,
        Order::Engage => 6,
    }
}

/// Where a soldier stands around his company's standard.
fn slot_offset(slot: u8, order: Order, turned: bool) -> (i32, i32) {
    let (dx, dy) = LAYOUTS[layout(order, turned)][slot as usize % COMPANY_SIZE];
    (dx as i32, dy as i32)
}

/// A soldier's place on the parade ground at rest.
fn ground_slot(ground: (i32, i32), slot: u8) -> (i32, i32) {
    let (dx, dy) = LAYOUTS[7][slot as usize % COMPANY_SIZE];
    (ground.0 + dx as i32, ground.1 + dy as i32)
}

impl World {
    /// A fighter's stats: soldiers and the city's own from the figure model, invaders
    /// from their nation's rows.
    pub fn fighter_stats(&self, fid: FigureId) -> UnitStats {
        let Some(f) = self.figures.get(fid) else { return UnitStats::default() };
        if let Some(s) = self.invader_stats(f) {
            return s;
        }
        self.balance.unit(model_row(f.kind))
    }

    /// Saves from before experience marked companies whose men had trained at an
    /// academy; they get an academy recruit's experience.
    pub(crate) fn upgrade_companies(&mut self) {
        for c in &mut self.military.companies {
            c.upgrade();
        }
    }

    /// Places a fort's parade ground and musters its company.
    pub(crate) fn place_fort(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(kind) = fort_soldier(b.kind) else { return };
        let (gx, gy) = (b.x + GROUND_OFFSET.0, b.y + GROUND_OFFSET.1);
        let ground_image = self.defs.building(b.kind).and_then(|d| d.anims.get("ground")).map(|a| a.image);
        let ground = self.create_building(FORT_GROUND, gx, gy);
        if let Some(image) = ground_image {
            self.set_building_image(ground, image);
        }
        // Soldiers drill on the parade ground: it stays a building (so nothing is built
        // or redrawn over it) that can be walked on.
        for yy in gy..gy + 4 {
            for xx in gx..gx + 4 {
                self.map.terrain.update(xx, yy, |t| t | crate::map::terrain::BUILDING | crate::map::terrain::PARADE_GROUND);
            }
        }
        let standard = self.figures.spawn(STANDARD_BEARER, gx, gy, Travel::Land);
        let company = self.military.companies.len() as u16 + 1;
        if let Some(f) = self.figures.get_mut(standard) {
            f.formation = company;
            f.home = id;
            f.action = action::AT_REST;
        }
        self.military.companies.push(Company { fort: id, ground, kind, standard, standard_tile: (gx, gy), at_fort: true, morale: 50, wind: WIND, ..Default::default() });
    }

    /// Whether a tile of a fort's parade ground is free.
    pub(crate) fn fort_ground_tile_clear(&self, xx: i32, yy: i32) -> bool {
        self.map.contains(xx, yy) && !self.map.terrain_is(xx, yy, crate::map::mask::NOT_CLEAR) && self.map.building.at_or(xx, yy, 0) == 0
    }

    /// Saves from before parade grounds kept their building bit: the ground's tiles
    /// lost it, so roads, grass and cleared land could be drawn over them (a black
    /// yard). Gives them back the bit, the building and the ground image.
    pub(crate) fn upgrade_fort_grounds(&mut self) {
        let grounds: Vec<(BuildingId, u32)> = self
            .military
            .companies
            .iter()
            .filter_map(|c| {
                let fort = self.buildings.get(c.fort)?;
                let image = self.defs.building(fort.kind).and_then(|d| d.anims.get("ground")).map(|a| a.image)?;
                self.buildings.get(c.ground).filter(|g| g.kind == FORT_GROUND).map(|g| (g.id, image))
            })
            .collect();
        for (ground, image) in grounds {
            let Some(g) = self.buildings.get(ground) else { continue };
            let (gx, gy, size) = (g.x, g.y, g.size);
            for yy in gy..gy + size {
                for xx in gx..gx + size {
                    self.map.terrain.update(xx, yy, |t| (t & !crate::map::terrain::ROAD) | crate::map::terrain::BUILDING | crate::map::terrain::PARADE_GROUND);
                    self.map.building.set(xx, yy, ground);
                }
            }
            self.set_building_image(ground, image);
        }
    }

    /// When a fort goes, its ground goes with it and its company disbands.
    pub(crate) fn remove_fort(&mut self, id: BuildingId) {
        let Some(c) = self.military.companies.iter().position(|c| c.fort == id) else { return };
        let company = self.military.companies[c].clone();
        for fid in company.soldiers.iter().chain(&company.recruits).chain(std::iter::once(&company.standard)) {
            if let Some(f) = self.figures.get_mut(*fid) {
                f.dead = true;
            }
        }
        self.military.companies[c].fort = 0;
        self.military.companies[c].soldiers.clear();
        self.military.companies[c].recruits.clear();
        if self.buildings.get(company.ground).is_some() {
            self.demolish(company.ground);
        }
    }

    /// Daily: each recruiter with road access and staff counts the days to its next
    /// recruit, fewer the better staffed it is (the original's table: 8 days at full
    /// strength up to 48 with a skeleton staff), and then sends a sentry to a tower
    /// that lacks one or, failing that, a soldier to a fort.
    pub(crate) fn update_recruiters(&mut self) {
        let recruiters: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == RECRUITER && b.road.is_some()).map(|b| b.id).collect();
        for r in recruiters {
            let Some(b) = self.buildings.get(r) else { continue };
            let Some(delay) = recruit_delay(b.workers, self.workers_needed(b.kind)) else { continue };
            let b = self.buildings.get_mut(r).expect("present");
            b.spawn_delay += 1;
            if b.spawn_delay <= delay {
                continue;
            }
            b.spawn_delay = 0;
            if !self.man_a_tower(r) {
                self.recruit(r);
            }
        }
    }

    /// Whether this recruiter would enlist anyone: some fort is short of men it
    /// can outfit, or a tower lacks a sentry.
    pub fn recruits_wanted(&self, recruiter: BuildingId) -> bool {
        self.company_to_recruit(recruiter).is_some() || self.tower_wanting_sentry().is_some()
    }

    /// The company a recruiter enlists for next: of the companies at their forts
    /// (not in the field, nor away fighting for the Kingdom) and short of men whom
    /// it can outfit, charioteers first, then infantry, then archers, and of those
    /// the one whose fort is nearest.
    fn company_to_recruit(&self, recruiter: BuildingId) -> Option<usize> {
        let b = self.buildings.get(recruiter)?;
        let has = |res: u16| b.stock.get(res as usize).copied().unwrap_or(0) > 0;
        let away = self.military.battle.as_ref().map_or(&[][..], |battle| &battle.companies[..]);
        self.military
            .companies
            .iter()
            .enumerate()
            .filter(|&(i, c)| c.fort != 0 && c.at_fort && c.abroad == 0 && !away.contains(&i))
            .filter(|(_, c)| c.soldiers.len() + c.recruits.len() < COMPANY_SIZE)
            .filter(|(_, c)| outfit(c.kind).is_none_or(has))
            .filter_map(|(i, c)| {
                let f = self.buildings.get(c.fort)?;
                let priority = match c.kind {
                    CHARIOTEER => 3,
                    INFANTRY => 2,
                    _ => 1,
                };
                Some((i, priority, (f.x - b.x).abs().max((f.y - b.y).abs())))
            })
            .max_by_key(|&(i, priority, distance)| (priority, std::cmp::Reverse(distance), std::cmp::Reverse(i)))
            .map(|(i, _, _)| i)
    }

    /// A recruiter enlists a soldier, if some fort wants one: he takes a load of
    /// weapons (infantry) or chariots (charioteers) from its store, walks out of
    /// its door to the nearest fully staffed military academy to the fort if there
    /// is one, and on to his place on the fort's parade ground.
    fn recruit(&mut self, recruiter: BuildingId) {
        let Some(c) = self.company_to_recruit(recruiter) else { return };
        let Some(road) = self.buildings.get(recruiter).and_then(|b| b.road) else { return };
        if let Some(res) = outfit(self.military.companies[c].kind) {
            let stock = &mut self.buildings.get_mut(recruiter).expect("present").stock[res as usize];
            *stock = (*stock - crate::economy::LOAD).max(0);
        }
        let company = &self.military.companies[c];
        let kind = company.kind;
        let used: Vec<u8> = company.soldiers.iter().chain(&company.recruits).filter_map(|&s| self.figures.get(s).map(|f| f.slot)).collect();
        let slot = (0..COMPANY_SIZE as u8).find(|s| !used.contains(s)).unwrap_or(0);
        let fid = self.figures.spawn(kind, road.0, road.1, Travel::Land);
        if let Some(f) = self.figures.get_mut(fid) {
            f.formation = c as u16 + 1;
            f.slot = slot;
            f.home = self.military.companies[c].fort;
        }
        self.military.companies[c].recruits.push(fid);
        let fort = self.buildings.get(self.military.companies[c].fort).map_or((road.0, road.1), |f| (f.x, f.y));
        let academy = self
            .buildings
            .iter()
            .filter(|a| ACADEMIES.contains(&a.kind) && a.workers >= self.workers_needed(a.kind))
            .min_by_key(|a| ((a.x - fort.0).abs().max((a.y - fort.1).abs()), a.id))
            .map(|a| a.road);
        // He brings his training, and Seth's favour, to the company's experience,
        // even if the academy has no road to call at.
        let brings = if academy.is_some() { ACADEMY_EXPERIENCE } else { 0 } + if self.complex_blessing(crate::temple_complex::SETH, 0) { SETH_EXPERIENCE } else { 0 };
        let co = &mut self.military.companies[c];
        let n = (co.soldiers.len() + co.recruits.len()) as i32 + co.abroad;
        co.experience = with_recruit(co.experience, n, brings);
        let map = &self.map;
        let training = match (academy.flatten(), self.figures.get_mut(fid)) {
            (Some(to), Some(f)) => {
                let ok = f.go_to(map, to);
                if ok {
                    f.action = action::GOING_TO_ACADEMY;
                }
                ok
            }
            _ => false,
        };
        if !training {
            self.send_to_post(fid);
        }
    }

    /// Where a soldier belongs now: his place on the parade ground, or around the
    /// standard when the company is out.
    fn post_of(&self, fid: FigureId) -> Option<(i32, i32)> {
        let f = self.figures.get(fid)?;
        let c = self.military.companies.get(f.formation.checked_sub(1)? as usize)?;
        if c.at_fort {
            let g = self.buildings.get(c.ground)?;
            Some(ground_slot((g.x, g.y), f.slot))
        } else {
            let (dx, dy) = slot_offset(f.slot, c.order, c.turned);
            Some((c.standard_tile.0 + dx, c.standard_tile.1 + dy))
        }
    }

    fn send_to_post(&mut self, fid: FigureId) {
        let Some(post) = self.post_of(fid) else { return };
        let at_fort = self.figures.get(fid).and_then(|f| self.military.companies.get(f.formation as usize - 1)).is_some_and(|c| c.at_fort);
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = if at_fort { action::GOING_TO_FORT } else { action::GOING_TO_STANDARD };
        if (f.x, f.y) == post {
            f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
        } else if !f.go_to(map, post) {
            // Nowhere to stand there: stay put.
            f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
        }
    }

    /// Orders a company out to form up around `tile`. False, and nothing done, when
    /// its standard can't get there ("This company cannot reach its intended
    /// destination"). Charioteers under orders to charge set off at the charge,
    /// and must rest their horses before charging again.
    pub fn move_company(&mut self, company: usize, tile: (i32, i32)) -> bool {
        let Some(standard) = self.military.companies.get(company).map(|c| c.standard) else { return false };
        let from = self.figures.get(standard).map(|f| (f.x, f.y));
        let map = &self.map;
        let reached = match self.figures.get_mut(standard) {
            Some(f) if (f.x, f.y) == tile => true,
            Some(f) => f.go_to(map, tile),
            None => false,
        };
        if !reached {
            return false;
        }
        if let Some(f) = self.figures.get_mut(standard) {
            f.action = action::GOING_TO_STANDARD;
        }
        let c = &mut self.military.companies[company];
        // It faces the way it marches.
        if let Some(d) = from.and_then(|from| general_direction(from, tile)) {
            c.facing = d;
        }
        if c.order == Order::Charge && !c.charged {
            c.charged = true;
            c.charge_rest = 0;
        }
        c.at_fort = false;
        c.standard_tile = tile;
        let soldiers = c.soldiers.clone();
        for s in soldiers {
            if self.figures.get(s).is_some_and(|f| f.action != action::ATTACK) {
                self.send_to_post(s);
            }
        }
        true
    }

    /// The company window's switch that turns the line of the next tight or loose
    /// formation ordered.
    pub fn rotate_line(&mut self, company: usize) {
        if let Some(c) = self.military.companies.get_mut(company) {
            c.rotate = !c.rotate;
        }
    }

    /// The men a company counts: those with it, those on their way from the
    /// recruiter, and those away fighting for the Kingdom.
    pub fn company_men(&self, company: usize) -> usize {
        self.military.companies.get(company).map_or(0, |c| c.soldiers.len() + c.recruits.len() + c.abroad.max(0) as usize)
    }

    /// The wounds a company's men carry, as a percentage of all their hit points
    /// (0 when unhurt).
    pub fn company_wounds(&self, company: usize) -> i32 {
        let Some(c) = self.military.companies.get(company) else { return 0 };
        let (mut damage, mut hp) = (0, 0);
        for &s in c.soldiers.iter().chain(&c.recruits) {
            if let Some(f) = self.figures.get(s) {
                damage += f.damage.max(0);
                hp += self.fighter_stats(s).hp.max(0);
            }
        }
        if hp > 0 { damage * 100 / hp } else { 0 }
    }

    /// Whether a company's standard is planted (its flag hangs still), rather than
    /// on the move.
    pub fn company_halted(&self, company: usize) -> bool {
        self.military.companies.get(company).and_then(|c| self.figures.get(c.standard)).is_some_and(|f| !f.moving)
    }

    /// Orders a company back to its fort.
    pub fn return_company(&mut self, company: usize) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        c.at_fort = true;
        let (standard, soldiers, ground) = (c.standard, c.soldiers.clone(), c.ground);
        let home = self.buildings.get(ground).map(|g| (g.x, g.y));
        let map = &self.map;
        if let (Some(f), Some(home)) = (self.figures.get_mut(standard), home) {
            f.action = action::GOING_TO_FORT;
            f.go_to(map, home);
        }
        for s in soldiers {
            if let Some(f) = self.figures.get_mut(s) {
                f.foe = 0;
            }
            self.send_to_post(s);
        }
    }

    /// The company a figure belongs to, if it is one of the city's soldiers.
    pub fn company_of(&self, fid: FigureId) -> Option<usize> {
        let f = self.figures.get(fid)?;
        (is_soldier(f.kind) || f.kind == STANDARD_BEARER).then(|| f.formation.checked_sub(1).map(|c| c as usize)).flatten()
    }

    /// A soldier's turn: march to his post, stand, and fight what comes near.
    pub(crate) fn update_soldier(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, kind) = (f.action, f.kind);
        if act == action::CORPSE {
            self.update_corpse(fid);
            return;
        }
        // Charged horses recover a little with each man's turn.
        if let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c))
            && c.charged
        {
            c.charge_rest += 1;
            if c.charge_rest > CHARGE_REST {
                c.charged = false;
                c.charge_rest = 0;
            }
        }
        // Recruits join the company when they reach it.
        if let Some(c) = self.company_of(fid)
            && let Some(i) = self.military.companies[c].recruits.iter().position(|&r| r == fid)
            && matches!(act, action::AT_REST | action::AT_STANDARD)
        {
            self.military.companies[c].recruits.remove(i);
            self.military.companies[c].soldiers.push(fid);
        }
        match act {
            action::GOING_TO_FORT | action::GOING_TO_STANDARD => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    _ => f.action = if act == action::GOING_TO_FORT { action::AT_REST } else { action::AT_STANDARD },
                }
                if act == action::GOING_TO_STANDARD {
                    self.engage(fid, 1);
                }
            }
            action::AT_STANDARD => {
                self.breathe(fid, false);
                if kind == ARCHER {
                    self.shoot_at_foes(fid);
                }
                // Charging charioteers ride at the enemy rather than stand and fight.
                if !self.charging(fid) {
                    self.engage(fid, 1);
                }
                let reach = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.order.reach());
                if reach > 0 && self.figures.get(fid).is_some_and(|f| f.action == action::AT_STANDARD) {
                    self.chase(fid, reach);
                }
            }
            action::CHASING => {
                if !self.charging(fid) {
                    self.engage(fid, 1);
                }
                if self.figures.get(fid).is_some_and(|f| f.action == action::CHASING) {
                    let reach = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.order.reach());
                    self.chase(fid, reach);
                }
            }
            action::AT_REST => self.breathe(fid, false),
            action::ATTACK => self.fight(fid),
            action::GOING_ABROAD => {
                // Marching out of the city; gone once past its edge.
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                    for c in &mut self.military.companies {
                        c.soldiers.retain(|&s| s != fid);
                    }
                }
            }
            action::GOING_TO_ACADEMY => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    self.send_to_post(fid);
                }
            }
            _ => self.send_to_post(fid),
        }
    }

    /// Gathers this tick's combatants (at the start of the figures' turn).
    pub(crate) fn gather_combatants(&mut self) {
        let mut c = Combatants::default();
        for f in self.figures.iter().filter(|f| !f.dead && f.action != action::CORPSE) {
            if self.is_invader(f) {
                c.invaders.push((f.id, f.x, f.y));
            } else if crate::predators::is_predator(f.kind) {
                if f.action != crate::predators::action::HIDDEN {
                    c.predators.push((f.id, f.x, f.y));
                }
            } else if is_soldier(f.kind) || matches!(f.kind, crate::crime::CONSTABLE | crate::defenses::TOWER_SENTRY) {
                c.defenders.push((f.id, f.x, f.y));
            }
        }
        self.combatants = c;
    }

    /// The figures the city's men (or, with `invader`, the invaders) fight: the
    /// invaders and the wild beasts, or the city's men.
    fn foes(&self, invader: bool) -> impl Iterator<Item = &(FigureId, i32, i32)> {
        let c = &self.combatants;
        let (list, beasts) = if invader { (&c.defenders, &c.predators[..0]) } else { (&c.invaders, &c.predators[..]) };
        list.iter().chain(beasts)
    }

    /// The nearest living enemy of a figure on the invaders' side (`invader`) or the
    /// city's, within `range` tiles: its id and tile.
    pub(crate) fn nearest_foe(&self, invader: bool, (x, y): (i32, i32), range: i32) -> Option<(FigureId, i32, i32)> {
        self.foes(invader)
            .filter(|&&(_, ox, oy)| (ox - x).abs() <= range && (oy - y).abs() <= range)
            .filter(|&&(id, _, _)| self.figures.get(id).is_some_and(|o| !o.dead && o.action != action::CORPSE))
            .min_by_key(|&&(_, ox, oy)| (ox - x).abs() + (oy - y).abs())
            .copied()
    }

    /// Takes on an enemy within `range` tiles, if there is one: the nearest who is not
    /// already fighting two. He strikes first after 12 ticks; the enemy, if not
    /// already fighting, turns on him and strikes after 24.
    pub(crate) fn engage(&mut self, fid: FigureId, range: i32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y) = (f.x, f.y);
        let Some((foe, ox, oy)) = self.nearest_open_foe(self.is_invader(f), (x, y), range) else { return };
        let facing = crate::figures::direction_to((x, y), (ox, oy));
        if let Some(f) = self.figures.get_mut(fid) {
            f.foe = foe;
            f.action = action::ATTACK;
            f.attack_tick = QUICK_BLOW;
            f.route.clear();
            f.moving = false;
            f.direction = facing.unwrap_or(f.direction);
        }
        // Sentries keep to their towers, and the fleeing keep running.
        if let Some(o) = self.figures.get_mut(foe)
            && !matches!(o.action, action::ATTACK | action::CORPSE | action::FLEEING | action::GOING_ABROAD)
            && o.kind != crate::defenses::TOWER_SENTRY
        {
            o.resume = o.action;
            o.foe = fid;
            o.action = action::ATTACK;
            o.attack_tick = 0;
            o.route.clear();
            o.moving = false;
            o.direction = facing.map_or(o.direction, |d| (d + 4) % 8);
        }
    }

    /// The nearest enemy of a figure on the invaders' side (`invader`) or the city's,
    /// within `range` tiles, whom it can take on: one not already fighting two, nor a
    /// chariot at the charge.
    pub(crate) fn nearest_open_foe(&self, invader: bool, (x, y): (i32, i32), range: i32) -> Option<(FigureId, i32, i32)> {
        self.foes(invader)
            .filter(|&&(_, ox, oy)| (ox - x).abs() <= range && (oy - y).abs() <= range)
            .filter(|&&(id, _, _)| self.figures.get(id).is_some_and(|o| !o.dead && o.action != action::CORPSE))
            .filter(|&&(id, _, _)| self.attackers(id) < 2 && !self.riding(id))
            .min_by_key(|&&(_, ox, oy)| (ox - x).abs() + (oy - y).abs())
            .copied()
    }

    /// How many are fighting a figure, if it is fighting itself.
    fn attackers(&self, fid: FigureId) -> usize {
        let Some(f) = self.figures.get(fid).filter(|f| f.action == action::ATTACK) else { return 0 };
        let c = &self.combatants;
        let list: Vec<&(FigureId, i32, i32)> = if self.is_invader(f) {
            c.defenders.iter().chain(&c.predators).collect()
        } else if crate::predators::is_predator(f.kind) {
            c.defenders.iter().chain(&c.invaders).collect()
        } else {
            c.invaders.iter().chain(&c.predators).collect()
        };
        list.into_iter().filter(|&&(id, _, _)| self.figures.get(id).is_some_and(|a| a.action == action::ATTACK && a.foe == fid)).count()
    }

    /// Whether a soldier is a charioteer charging with horses still fresh.
    fn charging(&self, fid: FigureId) -> bool {
        self.figures.get(fid).is_some_and(|f| f.kind == CHARIOTEER)
            && self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_some_and(|c| c.order == Order::Charge && c.wind > 0)
    }

    /// Whether a charioteer is at the charge: galloping after the enemy.
    fn riding(&self, fid: FigureId) -> bool {
        self.figures.get(fid).is_some_and(|f| f.action == action::CHASING) && self.charging(fid)
    }

    /// Once a tick, a company's horses tire while it charges and recover while it
    /// doesn't.
    fn breathe(&mut self, fid: FigureId, running: bool) {
        let now = self.time.total_ticks;
        let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) else { return };
        if c.wind_tick == now {
            return;
        }
        c.wind_tick = now;
        if running {
            c.wind = (c.wind - 1).max(0);
        } else if now.is_multiple_of(REST_PER_WIND) {
            c.wind = (c.wind + 1).min(WIND);
        }
    }

    /// A soldier under orders to go after enemies heads for the nearest within `reach`
    /// tiles, or back to his place when none is left. Charging charioteers go at the
    /// gallop until their horses tire, and then at half pace; galloping, they ride
    /// down whoever they run into, and fight once they can ride no further.
    fn chase(&mut self, fid: FigureId, reach: i32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y, heading) = (f.x, f.y, f.direction);
        let target = self.nearest_foe(false, (x, y), reach).map(|o| (o.1, o.2));
        let charioteer = f.kind == CHARIOTEER && self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_some_and(|c| c.order == Order::Charge);
        let fresh = self.charging(fid);
        if charioteer && target.is_some() {
            self.breathe(fid, fresh);
        }
        let spent = charioteer && !fresh && self.time.total_ticks % 2 == 1;
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match target {
            Some(to) => {
                f.action = action::CHASING;
                if !f.moving && f.destination != Some(to) {
                    f.go_to(map, to);
                }
                f.speed = if fresh { 2 } else { 1 };
                if !spent {
                    f.walk(map);
                }
                f.speed = 1;
                if !fresh {
                    f.counter = 0;
                    return;
                }
                let (nx, ny, moving) = (f.x, f.y, f.moving);
                if (nx, ny) == (x, y) {
                    // Nowhere further to ride: fight.
                    if !moving && (to.0 - x).abs() <= 1 && (to.1 - y).abs() <= 1 {
                        self.engage(fid, 1);
                    }
                    return;
                }
                // The run counts the tiles gone straight.
                let steps = (nx - x).abs().max((ny - y).abs());
                f.counter = if f.direction == heading { f.counter + steps } else { steps };
                self.trample(fid, (nx, ny));
            }
            None if f.action == action::CHASING => self.send_to_post(fid),
            None => {}
        }
    }

    /// A charging chariot comes onto `tile`: the first enemy there takes its attack,
    /// four times over if it has run six to nineteen tiles straight, his armour
    /// counting for nothing. (The original weighs it by the chariot's own armour
    /// instead, (30 - armour) / 20, which is one for any chariot.)
    fn trample(&mut self, fid: FigureId, tile: (i32, i32)) {
        let victim = self.combatants.invaders.iter().map(|o| o.0).find(|&id| self.figures.get(id).is_some_and(|o| (o.x, o.y) == tile && !o.dead && o.action != action::CORPSE));
        let Some(victim) = victim else { return };
        let stats = self.fighter_stats(fid);
        let run = self.figures.get(fid).map_or(0, |f| f.counter);
        let attack = if CHARGE_RUN.contains(&run) { 4 * stats.attack } else { stats.attack };
        let kind = self.figures.get(victim).map_or(0, |o| o.kind);
        if self.hurt(victim, (30 - stats.armor) / MAX_ARMOR * attack) {
            self.learn(self.company_of(fid), kind);
        }
    }

    /// The line a soldier fights in: his company, when it stands halted in the field
    /// with its standard planted. He faces its way when standing in it, or when
    /// fighting or walking that way.
    fn line(&self, fid: FigureId) -> Option<Line> {
        let f = self.figures.get(fid)?;
        if !is_soldier(f.kind) {
            return None;
        }
        let c = self.military.companies.get(self.company_of(fid)?)?;
        let halted = !c.at_fort && self.figures.get(c.standard).is_some_and(|s| s.action == action::AT_STANDARD);
        if !halted {
            return None;
        }
        let facing = f.action == action::AT_STANDARD || toward(f.direction, c.facing);
        Some(Line { kind: c.kind, order: c.order, facing })
    }

    /// Changes a company's orders; its men re-form at their new places. A tight or
    /// loose line is turned if the company's rotate switch is on. Charioteers can't
    /// be ordered to charge while their horses recover from the last.
    pub fn set_order(&mut self, company: usize, order: Order) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        if order == Order::Charge && (c.kind != CHARIOTEER || c.charged) {
            return;
        }
        c.order = order;
        if matches!(order, Order::HoldTight | Order::HoldLoose) {
            c.turned = c.rotate;
        }
        if c.at_fort {
            return;
        }
        for s in c.soldiers.clone() {
            if self.figures.get(s).is_some_and(|f| matches!(f.action, action::AT_STANDARD | action::GOING_TO_STANDARD)) {
                self.send_to_post(s);
            }
        }
    }

    /// A blow comes round: the foe takes the striker's attack against his armour,
    /// each as their lines make it, the striker's the better for coming at his back.
    pub(crate) fn fight(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (foe, x, y) = (f.foe, f.x, f.y);
        let alive = self.figures.get(foe).filter(|o| !o.dead && o.action != action::CORPSE);
        let Some(o) = alive.filter(|o| (o.x - x).abs() <= 1 && (o.y - y).abs() <= 1) else {
            let f = self.figures.get_mut(fid).expect("present");
            f.foe = 0;
            f.action = 0;
            return;
        };
        let (ox, oy, their_foe, their_way, victim) = (o.x, o.y, o.foe, o.direction, o.kind);
        let f = self.figures.get_mut(fid).expect("present");
        f.direction = crate::figures::direction_to((x, y), (ox, oy)).unwrap_or(f.direction);
        let (way, kind) = (f.direction, f.kind);
        f.attack_tick += 1;
        if f.attack_tick < BLOW_TICKS {
            return;
        }
        let (mine, theirs) = (self.fighter_stats(fid), self.fighter_stats(foe));
        // Citizens and criminals are struck down twice as fast.
        self.figures.get_mut(fid).expect("present").attack_tick = if matches!(theirs.class, 1 | 4) { QUICK_BLOW } else { 0 };
        let company = self.company_of(fid);
        // The original means to add (experience + 10) / 20 to the armour of the
        // city's men, but takes the striker's company's experience: an invader's,
        // which is none.
        let seasoned = if !self.figures.get(foe).is_some_and(|o| self.is_invader(o) || crate::predators::is_predator(o.kind) || crate::animals::is_animal(o.kind)) { (company.and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.experience) + 10) / 20 } else { 0 };
        let mut armor = (theirs.armor + seasoned).min(MAX_ARMOR);
        let mut attack = mine.attack;
        // Infantry and charioteers strike at the back of a man busy with another.
        if their_foe != fid && company.is_some() && matches!(kind, INFANTRY | CHARIOTEER) && toward(way, their_way) {
            attack += 4;
        }
        if let Some(l) = self.line(fid) {
            attack += l.attack();
        }
        if let Some(l) = self.line(foe) {
            armor += l.armor();
        }
        if self.hurt(foe, blow(attack, armor)) {
            self.learn(company, victim);
        }
    }

    /// A company's man has killed a figure of `kind`: the company gains a point of
    /// experience.
    fn learn(&mut self, company: Option<usize>, kind: u16) {
        if let Some(c) = company.and_then(|c| self.military.companies.get_mut(c))
            && teaches(kind)
        {
            c.experience = (c.experience + 1).min(MAX_EXPERIENCE);
        }
    }

    /// Adds damage to a figure; it falls when the damage passes its hit points, and
    /// its side's morale suffers. True if this killed it.
    pub(crate) fn hurt(&mut self, fid: FigureId, damage: i32) -> bool {
        let hp = self.fighter_stats(fid).hp.max(1);
        let Some(f) = self.figures.get_mut(fid) else { return false };
        f.damage += damage;
        if f.damage <= hp || f.action == action::CORPSE {
            return false;
        }
        f.action = action::CORPSE;
        f.counter = 0;
        f.foe = 0;
        f.route.clear();
        f.moving = false;
        let formation = if f.kind == crate::navy::ENEMY_TRANSPORT { 0 } else { f.formation };
        // The share of the company still standing that he was.
        let standing = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| {
            c.soldiers.iter().chain(&c.recruits).filter(|&&s| self.figures.get(s).is_some_and(|f| !f.dead && f.action != action::CORPSE)).count() as i32
        });
        if let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) {
            c.morale = (c.morale - morale_loss(if standing > 0 { 100 / standing } else { 0 })).max(0);
            if c.morale <= BROKEN_MORALE && !c.at_fort {
                let company = self.company_of(fid).expect("checked");
                self.return_company(company);
            }
        } else if formation >= 1000 {
            self.army_loses(formation as usize - 1000);
        }
        true
    }

    /// The strength the companies marked for Kingdom service would bring to a distant
    /// battle: each soldier one, and one more for each hundred points of his company's
    /// experience; an infantryman one more.
    pub fn kingdom_service_strength(&self) -> i32 {
        self.military
            .companies
            .iter()
            .filter(|c| c.kingdom_service && c.fort != 0)
            .map(|c| (c.experience + if c.kind == INFANTRY { 200 } else { 100 }) * c.soldiers.len() as i32 / 100)
            .sum()
    }

    pub fn toggle_kingdom_service(&mut self, company: usize) {
        if let Some(c) = self.military.companies.get_mut(company) {
            c.kingdom_service = !c.kingdom_service;
        }
    }

    /// The companies marked for Kingdom service march off to fight `enemy` for the
    /// request `request`.
    pub(crate) fn send_to_battle(&mut self, request: usize, enemy: i32) {
        let strength = self.kingdom_service_strength();
        let marked: Vec<usize> = (0..self.military.companies.len()).filter(|&c| self.military.companies[c].kingdom_service && !self.military.companies[c].soldiers.is_empty()).collect();
        let exit = self.exit_point;
        for &c in &marked {
            let soldiers = self.military.companies[c].soldiers.clone();
            self.military.companies[c].abroad = soldiers.len() as i32;
            self.military.companies[c].at_fort = false;
            let map = &self.map;
            for s in soldiers {
                if let Some(f) = self.figures.get_mut(s) {
                    f.action = action::GOING_ABROAD;
                    f.foe = 0;
                    if !f.go_to(map, exit) {
                        f.dead = true;
                    }
                }
            }
        }
        self.military.battle = Some(DistantBattle { request, enemy, strength, companies: marked, months: TRAVEL_MONTHS, fought: false });
    }

    /// Monthly: the troops abroad reach their battle and fight it; the survivors come
    /// home. A won battle meets the request; a lost one fails it.
    pub(crate) fn update_distant_battle(&mut self) {
        let Some(b) = self.military.battle.as_mut() else { return };
        b.months -= 1;
        if b.months > 0 {
            return;
        }
        let b = b.clone();
        if b.fought {
            // Home again: the survivors walk back to their forts.
            self.military.battle = None;
            let exit = self.exit_point;
            for &c in &b.companies {
                let n = std::mem::take(&mut self.military.companies[c].abroad);
                self.military.companies[c].at_fort = true;
                for _ in 0..n {
                    let kind = self.military.companies[c].kind;
                    let used: Vec<u8> = self.military.companies[c].soldiers.iter().chain(&self.military.companies[c].recruits).filter_map(|&s| self.figures.get(s).map(|f| f.slot)).collect();
                    let slot = (0..COMPANY_SIZE as u8).find(|s| !used.contains(s)).unwrap_or(0);
                    let fid = self.figures.spawn(kind, exit.0, exit.1, Travel::Land);
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.formation = c as u16 + 1;
                        f.slot = slot;
                        f.home = self.military.companies[c].fort;
                    }
                    self.military.companies[c].recruits.push(fid);
                    self.send_to_post(fid);
                }
            }
            return;
        }
        // Seth, if he has promised, sees the troops through without loss.
        let protected = std::mem::take(&mut self.religion.seth_protects);
        let won = protected || b.strength >= b.enemy && b.strength > 0;
        let losses = if protected {
            0
        } else if won {
            battle_losses((b.strength - b.enemy) * 100 / b.strength.max(1))
        } else {
            100
        };
        for &c in &b.companies {
            let co = &mut self.military.companies[c];
            co.abroad -= co.abroad * losses / 100;
        }
        let returning = b.companies.iter().any(|&c| self.military.companies[c].abroad > 0);
        if let Some(bb) = self.military.battle.as_mut() {
            bb.fought = true;
            bb.months = TRAVEL_MONTHS;
        }
        if !returning {
            self.military.battle = None;
        }
        self.settle_troop_request(b.request, won);
    }
    pub(crate) fn update_morale_month(&mut self) {
        for c in &mut self.military.companies {
            if c.at_fort {
                c.months_away = 0;
                c.morale = (c.morale + 5).min(morale_cap(c.kind));
            } else {
                c.months_away += 1;
                if c.months_away > 3 {
                    c.morale = (c.morale - 5).max(0);
                }
            }
        }
    }

    /// The fallen lie a while, then are gone, and leave their company.
    fn update_corpse(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter < CORPSE_TICKS {
            return;
        }
        f.dead = true;
        for c in &mut self.military.companies {
            c.soldiers.retain(|&s| s != fid);
            c.recruits.retain(|&s| s != fid);
        }
    }

    /// An archer looses a missile at the nearest enemy in range each time he has
    /// reloaded (his rate of fire, in ticks), if there is one. The city's javelins
    /// carry their thrower's missile attack; the invaders' do the spear's.
    pub(crate) fn shoot_at_foes(&mut self, fid: FigureId) {
        let stats = self.fighter_stats(fid);
        if stats.missile_range <= 0 {
            return;
        }
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.attack_tick += 1;
        if (f.attack_tick as i32) <= stats.missile_delay {
            return;
        }
        f.attack_tick = 0;
        let (x, y) = (f.x, f.y);
        let mine = self.figures.get(fid).is_some_and(|f| self.is_invader(f));
        let range = stats.missile_range;
        // The city's men shoot at invaders before beasts.
        let target = if mine {
            self.nearest_foe(true, (x, y), range)
        } else {
            let near = |list: &[(FigureId, i32, i32)]| list.iter().filter(|o| (o.1 - x).abs() <= range && (o.2 - y).abs() <= range).min_by_key(|o| (o.1 - x).abs() + (o.2 - y).abs()).copied();
            near(&self.combatants.invaders).or_else(|| near(&self.combatants.predators))
        };
        let Some((target, tx, ty)) = target else { return };
        if let Some(f) = self.figures.get_mut(fid) {
            f.direction = crate::figures::direction_to((x, y), (tx, ty)).unwrap_or(f.direction);
        }
        let missile = self.figures.spawn(if mine { ARROW } else { JAVELIN }, x, y, Travel::Land);
        let company = if mine { 0 } else { self.company_of(fid).map_or(0, |c| c as u16 + 1) };
        let attack = if mine { self.balance.unit(SPEAR).missile_attack } else { stats.missile_attack };
        if let Some(m) = self.figures.get_mut(missile) {
            m.foe = target;
            // The company whose man loosed it, which learns from a kill.
            m.formation = company;
            m.amount = attack;
            m.destination = Some((tx, ty));
            m.direction = crate::figures::direction_to((x, y), (tx, ty)).unwrap_or(0);
        }
    }

    /// A missile flies a tile every few ticks toward where its target stood, and
    /// wounds it if it is still there: a javelin by its attack against the target's
    /// armour against missiles; an invader's missile all or nothing, its armour
    /// counting the experience of a soldier's company and his line.
    pub(crate) fn update_missile(&mut self, fid: FigureId) {
        let Some(m) = self.figures.get_mut(fid) else { return };
        m.counter += 1;
        if m.counter % 3 != 0 {
            return;
        }
        let Some((tx, ty)) = m.destination else {
            m.dead = true;
            return;
        };
        if (m.x, m.y) != (tx, ty) {
            m.x += (tx - m.x).signum();
            m.y += (ty - m.y).signum();
            return;
        }
        m.dead = true;
        let (target, attack, company, theirs) = (m.foe, m.amount, m.formation.checked_sub(1).map(|c| c as usize), m.kind == ARROW);
        let hit = self.figures.get(target).is_some_and(|t| !t.dead && t.action != action::CORPSE && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1);
        if !hit {
            return;
        }
        let armor = self.fighter_stats(target).missile_armor;
        let kind = self.figures.get(target).map_or(0, |t| t.kind);
        if theirs {
            let seasoned = if is_soldier(kind) { (self.company_of(target).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.experience) + 10) / 20 } else { 0 };
            let armor = (armor + seasoned).min(MAX_ARMOR) + self.line(target).map_or(0, |l| l.missile_armor());
            self.hurt(target, spear(attack, armor));
        } else if self.hurt(target, blow(attack, armor.min(MAX_ARMOR))) {
            self.learn(company, kind);
        }
    }

    /// The standard bearer carries the company's flag to where it forms up.
    pub(crate) fn update_standard_bearer(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        if f.walk(map) != Step::Moving {
            f.action = if f.action == action::GOING_TO_FORT { action::AT_REST } else { action::AT_STANDARD };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recruits_average_in() {
        // A raw company's first academy recruit counts fully; later ones keep it at 25.
        assert_eq!(with_recruit(0, 1, ACADEMY_EXPERIENCE), 25);
        assert_eq!(with_recruit(25, 2, ACADEMY_EXPERIENCE), 25);
        // A raw recruit counts as half a man, rounded up.
        assert_eq!(with_recruit(50, 16, 0), 49);
        // Seth's recruits (10, or 35 with an academy) count as half a man too.
        assert_eq!(with_recruit(0, 1, 10), 5);
        assert_eq!(with_recruit(0, 1, 35), 18);
        assert_eq!(with_recruit(100, 16, 35), 98);
    }

    #[test]
    fn blows_and_lines() {
        // Bedouin (attack 12) on infantry (armour 2) in a tight line, facing and not.
        let tight = |facing| Line { kind: INFANTRY, order: Order::HoldTight, facing };
        assert_eq!(blow(12, 2 + tight(true).armor()), 8);
        assert_eq!(blow(12, 2 + tight(false).armor()), 12);
        assert_eq!(blow(12 + tight(true).attack(), 0), 16);
        assert_eq!((blow(0, 5), blow(10, 25), blow(5, -3)), (0, 0, 5));
        // An invader's missile does all or nothing.
        assert_eq!((spear(10, 10), spear(10, 11), spear(10, -4)), (10, 0, 10));
        let loose = Line { kind: ARCHER, order: Order::HoldLoose, facing: true };
        assert_eq!((loose.missile_armor(), loose.armor(), loose.attack()), (4, 0, 0));
        assert!(toward(0, 7) && toward(7, 0) && toward(3, 3) && !toward(0, 2) && !toward(6, 0));
        assert_eq!((general_direction((0, 0), (1, -9)), general_direction((0, 0), (5, -4)), general_direction((0, 0), (-9, 2))), (Some(0), Some(1), Some(6)));
    }

    #[test]
    fn recruiters_wait_by_staffing() {
        // The recruiter (10 workers) waits 8 days full, up to 48 with one man.
        assert_eq!([10, 8, 5, 3, 1, 0].map(|w| recruit_delay(w, 10)), [Some(8), Some(12), Some(16), Some(32), Some(48), None]);
    }

    #[test]
    fn formations_place_every_man_once() {
        for (i, row) in LAYOUTS.iter().enumerate() {
            let mut seen = row.to_vec();
            seen.sort();
            seen.dedup();
            // Every layout gives each man his own tile.
            assert_eq!(seen.len(), COMPANY_SIZE, "layout {i}");
        }
        assert_eq!(slot_offset(15, Order::HoldTight, true), (1, -4));
        assert_eq!(slot_offset(14, Order::HoldLoose, false), (8, 0));
    }

    #[test]
    fn ranks_and_ball() {
        assert_eq!((experience_rank(0), experience_rank(9), experience_rank(10), experience_rank(89), experience_rank(90), experience_rank(100)), (0, 0, 1, 4, 5, 5));
        assert_eq!((experience_ball(0), experience_ball(2), experience_ball(50), experience_ball(97), experience_ball(100)), (20, 19, 10, 0, 0));
    }

    #[test]
    fn old_saves_keep_their_training() {
        #[derive(serde::Serialize)]
        struct Old {
            fort: BuildingId,
            ground: BuildingId,
            kind: u16,
            soldiers: Vec<FigureId>,
            recruits: Vec<FigureId>,
            standard: FigureId,
            standard_tile: (i32, i32),
            at_fort: bool,
            morale: i32,
            trained: bool,
        }
        let old = Old { fort: 1, ground: 2, kind: INFANTRY, soldiers: vec![], recruits: vec![], standard: 3, standard_tile: (0, 0), at_fort: true, morale: 50, trained: true };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let mut c: Company = rmp_serde::from_slice(&bytes).unwrap();
        c.upgrade();
        assert_eq!(c.experience, ACADEMY_EXPERIENCE);
        assert!(!rmp_serde::to_vec_named(&c).unwrap().windows(7).any(|w| w == b"trained"));
    }
}
