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
//! to mop up every enemy it can find; or, charioteers, charges. The standard goes
//! straight to the spot the company is sent to, and the men march to their places
//! round it; a company that has halted, every man in his place, fights by its
//! orders, and its men fight worse when they have turned from the way it faces (a
//! flank or rear attack):
//!
//! - infantry in tight formation strike at +4 and take blows at +4 armour;
//! - in loose formation, infantry and archers have +4 armour against missiles;
//! - mopping up, men strike at +2 but have −2 armour, against missiles too;
//! - a man turned from his company's front has −4 armour, against missiles too.
//!
//! Infantry and charioteers who strike a man busy with someone else from behind
//! strike at +4. Charioteers sent somewhere at the charge ride for their places in
//! a close block round the spot, at twice their pace from the sixth tile to the
//! twentieth, and do not stop to fight but ride the enemy down: each they run into
//! takes their attack, four times over over those same tiles, and his armour does
//! him no good. Once all have arrived, the company engages. The armour a
//! company's experience was meant to add, (experience + 10) / 20, counts against
//! missiles only: in melee the original takes the striker's company's, which an
//! invader hasn't.
//!
//! Morale: every man lost shakes his company or formation, the more so the bigger
//! the share of it that fell; a company rests its spirits at the fort month by
//! month, up to 60 (charioteers 80, and 20 more once its recruits train at a military
//! academy), and loses heart when kept out long. Invaders' formations hold at most 70
//! (Bedouin 80). Once a day, a company out of its fort or a formation of invaders at
//! 20 or less breaks: the company runs home and won't take orders for a while, the
//! formation leaves the map, and the first break shakes the rest of that side and
//! heartens the other.
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
pub(crate) const SPEAR: u16 = 71;

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
/// The tiles from his place an engaging soldier goes after enemies.
const ENGAGE_LEASH: i32 = 6;
/// The most forts a city may have (0x471114).
const MAX_FORTS: usize = 6;
/// Why a seventh fort can't be built: the app shows the game's own 19:50.
pub const FORTS_FULL: &str = "Your army has its full complement of Forts";
/// Ticks a blow takes to land.
const BLOW_TICKS: u16 = 24;
/// Where a blow's count starts when it strikes the first blow of a fight, or a
/// citizen or criminal.
pub(crate) const QUICK_BLOW: u16 = 12;
/// The most armour counts for.
const MAX_ARMOR: i32 = 20;
/// The tiles into its charge over which a chariot rides at twice its pace and
/// tramples at four times its attack (the soldier's +0x134 at 0x48e9e0).
const CHARGE_RUN: std::ops::Range<i32> = 6..20;
/// Ticks a fallen figure lies on the field: the original's dying count runs to 127
/// and the figure is gone on the 128th (FUN_004b1d50).
pub(crate) const CORPSE_TICKS: i32 = 128;
/// Where the parade ground sits beside its fort.
const GROUND_OFFSET: (i32, i32) = (3, -1);
/// The 4x4 tiles of the parade ground beside a fort placed at `(x, y)`.
pub fn fort_ground(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    let (gx, gy) = (x + GROUND_OFFSET.0, y + GROUND_OFFSET.1);
    (gy..gy + 4).flat_map(move |yy| (gx..gx + 4).map(move |xx| (xx, yy)))
}

/// Morale at or below which a side breaks and runs.
pub const BROKEN_MORALE: i32 = 20;
/// Morale at or below which a broken company is badly broken.
const SHATTERED_MORALE: i32 = 10;
/// What the first break of a side's company or formation does to the morale of the
/// rest of that side, and of the other (FUN_004b79f0).
const BREAK_SHOCK: i32 = 10;

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
/// archers 60, each 20 more once a recruit of the company has set out for a
/// military academy (FUN_004b7b50; the mark is set by the soldier's
/// going-to-the-academy state at 0x490357).
fn morale_cap(kind: u16, trained: bool) -> i32 {
    (if kind == CHARIOTEER { 80 } else { 60 }) + if trained { 20 } else { 0 }
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
    /// Charioteers at the charge (the original's 0x13).
    pub const CHARGING: u16 = 82;
    pub const FLEEING: u16 = 148;
    /// Aboard a transport, out of sight (the original's 0x12).
    pub const ABOARD: u16 = 150;
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
    /// A recruit of the company has set out for a military academy: its morale can
    /// climb 20 higher. The original never takes the mark away.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub schooled: bool,
    /// Marked for Kingdom service: it answers Pharaoh's calls for troops.
    #[serde(default)]
    pub kingdom_service: bool,
    /// Soldiers away fighting for the Kingdom.
    #[serde(default)]
    pub abroad: i32,
    #[serde(default)]
    pub order: Order,
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
    /// Days its morale has been found broken while out (the original's +0x66, days
    /// at 11 to 20, and +0x73, days at 10 or less): the company runs home, and takes
    /// no orders once it has been so two days, or one day badly. Cleared by a month
    /// spent at the fort.
    /// The orders it had before it was sent to mop up, which it takes again once
    /// no enemy is left.
    #[serde(default, skip_serializing_if = "is_hold_tight")]
    pub before_mop_up: Order,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub routed: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub broken: u8,
    /// Not halted: when the company was last mustered (twice a day, and at each
    /// loss), some man of it had not yet arrived at his place, or had turned to take
    /// aim since he did (the original's +0x5a is set only when every man's direction
    /// reads "at destination", FUN_004b7c80). Its flag waves, and it fights without
    /// its line's advantages.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub marching: bool,
    /// Half-days until its men re-form after fighting (the original's +0x50), after
    /// loosing missiles (+0x5c) and after being struck by them (+0x5e): while any is
    /// running, a man out of his place stays where he is (FUN_004b8b20 counts them
    /// down; state 12 of 0x48e9e0 waits on them).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fought: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub shot: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub struck: u8,
}

/// Half-days a fight, a missile loosed or a missile struck holds a company's men
/// where they stand.
const REFORM_WAIT: u8 = 6;

fn is_zero(n: &u8) -> bool {
    *n == 0
}

fn is_hold_tight(o: &Order) -> bool {
    *o == Order::HoldTight
}

impl Company {
    /// An old save's mark of academy training becomes an academy recruit's
    /// experience.
    fn upgrade(&mut self) {
        if std::mem::take(&mut self.trained) {
            self.experience = self.experience.max(ACADEMY_EXPERIENCE);
            self.schooled = true;
        }
    }
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
            Order::MopUp => 20,
            // At the charge they ride for their places, not after the enemy.
            Order::Charge => 0,
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
    /// On the way: the months still to go (never below 1 while they wait for the
    /// request's due month). Coming home: the months until they are back.
    pub months: i32,
    /// The request's due month has come: they fought, or came too late, and are on
    /// the way home.
    pub fought: bool,
    /// They reached the battle too late to fight it (request state 7).
    #[serde(default)]
    pub late: bool,
    /// For a city reached by sea: the troops went by ship.
    #[serde(default)]
    pub sea: bool,
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
    /// The troops away for the Kingdom, one entry per request they answer: the
    /// original can have a land call and a sea call answered at once.
    #[serde(default)]
    pub battles: Vec<DistantBattle>,
    /// Older saved games kept a single battle here; it moves to `battles` on load.
    #[serde(default, skip_serializing)]
    pub battle: Option<DistantBattle>,
    /// Veterans of the last campaign mission waiting for a fort of their kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carried: Vec<CarriedCompany>,
}

/// The best company of a kind a won campaign mission leaves for the next
/// (FUN_004188f0): the one with the most experience, then the most men.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CarriedCompany {
    pub kind: u16,
    pub men: u8,
    pub experience: i32,
}

/// The missions whose veterans of the mission before rejoin them (text group 307:
/// the second field is 1, FUN_00418bf0).
const KEEPS_ARMY: [i32; 6] = [42, 43, 44, 49, 51, 52];

impl Military {
    /// The troops away for request `request`, if any.
    pub fn battle_for(&self, request: usize) -> Option<&DistantBattle> {
        self.battles.iter().find(|b| b.request == request)
    }

    /// Whether company `c` is one of those sent away for the Kingdom.
    pub fn sent_away(&self, c: usize) -> bool {
        self.battles.iter().any(|b| b.companies.contains(&c))
    }
}

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
/// engaging and at rest on the parade ground, each a four-by-four block; and the
/// charioteers' tight line, across or turned, a tile between its ranks (the
/// original's layouts 0, 1-4, 6, 15, 7, 13 and 14).
const LAYOUTS: [[(i8, i8); COMPANY_SIZE]; 10] = [
    [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (-1, 1), (0, -1), (1, -1), (-1, -1), (2, -1), (2, 0), (2, 1), (0, 2), (1, 2), (-1, 2), (2, 2)],
    [(0, 0), (0, 1), (-1, 0), (1, 0), (-1, 1), (1, 1), (-2, 0), (-2, 1), (2, 0), (2, 1), (-3, 0), (-3, 1), (3, 0), (3, 1), (-4, 0), (-4, 1)],
    [(0, 0), (0, -1), (0, 1), (1, 0), (1, -1), (1, 1), (0, -2), (1, -2), (0, 2), (1, 2), (0, -3), (1, -3), (0, 3), (1, 3), (0, -4), (1, -4)],
    [(0, 0), (2, 0), (-2, 0), (1, 1), (-1, 1), (3, 1), (-3, 1), (4, 0), (-4, 0), (5, 1), (6, 0), (-5, 1), (-6, 0), (7, 1), (8, 0), (-7, 1)],
    [(0, 0), (0, -2), (0, 2), (1, -1), (1, 1), (1, -3), (1, 3), (0, -4), (0, 4), (1, -5), (0, -6), (1, 5), (0, 6), (1, -7), (0, -8), (1, 7)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
    [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (2, 1), (1, 2), (0, 2), (2, 2), (3, 0), (3, 1), (3, 2), (1, 3), (2, 3), (0, 3), (3, 3)],
    [(0, 0), (0, 2), (-1, 0), (1, 0), (-1, 2), (1, 2), (-2, 0), (-2, 2), (2, 0), (2, 2), (-3, 0), (-3, 2), (3, 0), (3, 2), (-4, 0), (-4, 2)],
    [(0, 0), (0, -1), (0, 1), (2, 0), (2, -1), (2, 1), (0, -2), (2, -2), (0, 2), (2, 2), (0, -3), (2, -3), (0, 3), (2, 3), (0, -4), (2, -4)],
];

/// The row of [`LAYOUTS`] a company in the field stands in: by its orders, and for
/// the held lines by which way the line was turned when they were given.
fn layout(order: Order, turned: bool, kind: u16) -> usize {
    match order {
        Order::Charge => 0,
        // Chariots need room: theirs is the original's 13 and 14.
        Order::HoldTight if kind == CHARIOTEER => 8 + turned as usize,
        Order::HoldTight => 1 + turned as usize,
        Order::HoldLoose => 3 + turned as usize,
        Order::MopUp => 5,
        Order::Engage => 6,
    }
}

/// The tiles round a soldier's place he tries when he can't stand on it, ring by
/// ring out to six (168 tiles), each ring from due north clockwise: the original's
/// table at 0x5ca506.
fn alternative_tiles() -> impl Iterator<Item = (i32, i32)> {
    (1..=6).flat_map(|k: i32| {
        let top = (0..=k).map(move |x| (x, -k));
        let right = (-k + 1..=k).map(move |y| (k, y));
        let bottom = (-k..k).rev().map(move |x| (x, k));
        let left = (-k..k).rev().map(move |y| (-k, y));
        let back = (-k + 1..0).map(move |x| (x, -k));
        top.chain(right).chain(bottom).chain(left).chain(back)
    })
}

/// Where a soldier stands around his company's standard.
fn slot_offset(slot: u8, order: Order, turned: bool, kind: u16) -> (i32, i32) {
    let (dx, dy) = LAYOUTS[layout(order, turned, kind)][slot as usize % COMPANY_SIZE];
    (dx as i32, dy as i32)
}

/// Whether a missile loosed from tile `from` gets to tile `to` (FUN_004b4420): the
/// line between the tiles' corners is walked in fifteenths of a tile, eight steps at
/// a time, and the tile reached after each eight must not be a tree, a wall, a
/// gatehouse or a building bigger than one tile. From a wall or gatehouse the first
/// six looks (about three tiles) don't count, so men on the walls shoot over them.
pub(crate) fn line_of_sight(map: &crate::map::Map, from: (i32, i32), to: (i32, i32)) -> bool {
    use crate::map::terrain::{BUILDING, GATEHOUSE, TREE, WALL};
    let mut over = if map.terrain_is(from.0, from.1, WALL | GATEHOUSE) { 6 } else { 0 };
    let (mut x, mut y) = (from.0 * 15, from.1 * 15);
    let (tx, ty) = (to.0 * 15, to.1 * 15);
    let (mut dx, mut dy) = ((tx - x).abs(), (ty - y).abs());
    // The long way is walked a step at a time, the other when the error says.
    let along_y = dx < dy;
    let mut err = if dx < dy {
        2 * dx - dy
    } else if dy < dx {
        2 * dy - dx
    } else {
        0
    };
    for _ in 0..1000 {
        for _ in 0..8 {
            if dx + dy < 1 {
                return true;
            }
            if along_y {
                err += if err < 0 { 2 * dx } else { 2 * (dx - dy) };
                dy -= 1;
                y += (ty - y).signum();
                if err >= 0 {
                    dx -= 1;
                    x += (tx - x).signum();
                }
            } else {
                err += if err < 0 { 2 * dy } else { 2 * (dy - dx) };
                dx -= 1;
                x += (tx - x).signum();
                if err >= 0 {
                    dy -= 1;
                    y += (ty - y).signum();
                }
            }
        }
        let (cx, cy) = (x / 15, y / 15);
        if over > 0 {
            over -= 1;
            continue;
        }
        let t = map.terrain.at_or(cx, cy, 0);
        if t & (TREE | WALL | GATEHOUSE) != 0 || t & BUILDING != 0 && map.bitfields.at_or(cx, cy, 0) & 0x0f != 0 {
            return false;
        }
    }
    false
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
        if let Some(s) = self.invader_stats(f).or_else(|| self.enemy_ship_stats(f)) {
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
                self.map.terrain.update(xx, yy, |t| t | crate::map::terrain::BUILDING | crate::map::terrain::WALKABLE_BUILDING);
            }
        }
        let standard = self.figures.spawn(STANDARD_BEARER, gx, gy, Travel::Land);
        let c = self.free_company().unwrap_or(self.military.companies.len());
        if let Some(f) = self.figures.get_mut(standard) {
            f.formation = c as u16 + 1;
            f.home = id;
            f.action = action::AT_REST;
        }
        let company = Company { fort: id, ground, kind, standard, standard_tile: (gx, gy), at_fort: true, morale: 50, ..Default::default() };
        match self.military.companies.get_mut(c) {
            Some(old) => *old = company,
            None => self.military.companies.push(company),
        }
        self.rejoin(c);
    }

    /// The companies this mission, just won, leaves for the next: of each kind
    /// (infantry, archers, charioteers), the one with the most experience, then the
    /// most men, the first of equals (FUN_004188f0). Those with no men are left out.
    pub fn companies_to_carry(&self) -> Vec<CarriedCompany> {
        let men = |co: &Company| co.soldiers.iter().chain(&co.recruits).filter(|&&s| self.figures.get(s).is_some_and(|f| !f.dead && f.action != action::CORPSE)).count().min(COMPANY_SIZE) as u8;
        [INFANTRY, ARCHER, CHARIOTEER]
            .into_iter()
            .filter_map(|kind| {
                let mut best: Option<CarriedCompany> = None;
                for co in self.military.companies.iter().filter(|co| co.fort != 0 && co.kind == kind) {
                    let c = CarriedCompany { kind, men: men(co), experience: co.experience };
                    if best.is_none_or(|b| (c.experience, c.men) > (b.experience, b.men)) {
                        best = Some(c);
                    }
                }
                best.filter(|b| b.men > 0)
            })
            .collect()
    }

    /// At a campaign mission's start, the companies the mission before left: kept
    /// for the missions that take them (FUN_00418bf0), with the "Loyal Soldiers
    /// Rejoin You" message naming the forts to build for those the mission allows
    /// (FUN_00417b40). Returns how many are kept.
    pub fn carry_in_companies(&mut self, carried: &[CarriedCompany]) -> usize {
        self.military.carried.clear();
        if !self.mission.as_ref().is_some_and(|m| KEEPS_ARMY.contains(&m.id)) {
            return 0;
        }
        self.military.carried = carried.iter().filter(|c| c.men > 0).copied().collect();
        let has = |w: &World, kind: u16, fort: u16| w.military.carried.iter().any(|c| c.kind == kind) && w.is_allowed(fort);
        let reason = match (has(self, INFANTRY, FORT_INFANTRY), has(self, ARCHER, FORT_ARCHERS), has(self, CHARIOTEER, FORT_CHARIOTEERS)) {
            (true, true, true) => "all_three",
            (true, true, false) => "inf_arch",
            (true, false, true) => "inf_char",
            (false, true, true) => "arch_char",
            (true, false, false) => "inf_only",
            (false, true, false) => "arch_only",
            (false, false, true) => "char_only",
            (false, false, false) => return self.military.carried.len(),
        };
        self.post_event_text(crate::scenario_events::EventText {
            title: "troopcarryover_title".into(),
            body: "troopcarryover_initial_announcement".into(),
            reason: format!("troopcarryover_{reason}"),
            template: 131,
            ..Default::default()
        });
        self.military.carried.len()
    }

    /// The first fort of a kind built in a mission that kept the last mission's
    /// company of that kind takes its experience, and its men walk in from the
    /// map's entry point (else its exit, else beside the fort) to the parade ground
    /// (0x4709b0, FUN_00472790).
    fn rejoin(&mut self, c: usize) {
        let co = &self.military.companies[c];
        let Some(i) = self.military.carried.iter().position(|v| v.kind == co.kind) else { return };
        let v = self.military.carried.remove(i);
        let (kind, fort, ground) = (co.kind, co.fort, co.standard_tile);
        self.military.companies[c].experience = v.experience;
        let beside = self.buildings.get(fort).map_or(ground, |b| (b.x + 4, b.y + 1));
        let from = [self.entry_point, self.exit_point]
            .into_iter()
            .find(|&p| crate::figures::find_route(&self.map, Travel::Land, p, ground).is_some())
            .unwrap_or(beside);
        for slot in 0..v.men.min(COMPANY_SIZE as u8) {
            let fid = self.figures.spawn(kind, from.0, from.1, Travel::Land);
            if let Some(f) = self.figures.get_mut(fid) {
                f.formation = c as u16 + 1;
                f.slot = slot;
                f.home = fort;
            }
            self.military.companies[c].recruits.push(fid);
            self.send_to_post(fid);
        }
    }

    /// The first company record a lost fort has freed, which a new fort takes, as the
    /// original takes the first record not in use (FUN_004b6e10; FUN_004b82e0 clears
    /// a lost fort's). Osiris passes over a record its men abroad or aboard a
    /// transport still answer to.
    fn free_company(&self) -> Option<usize> {
        (0..self.military.companies.len()).find(|&c| {
            let co = &self.military.companies[c];
            co.fort == 0 && co.abroad == 0 && !self.military.sent_away(c) && self.company_ship(c).is_none()
        })
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
                    self.map.terrain.update(xx, yy, |t| (t & !crate::map::terrain::ROAD) | crate::map::terrain::BUILDING | crate::map::terrain::WALKABLE_BUILDING);
                    self.map.building.set(xx, yy, ground);
                }
            }
            self.set_building_image(ground, image);
        }
    }

    /// Whether the city has as many forts as it may: six (the original refuses a
    /// seventh, 19:50 "Your army has its full complement of Forts").
    pub fn forts_full(&self) -> bool {
        self.military.companies.iter().filter(|c| c.fort != 0).count() >= MAX_FORTS
    }

    /// The company whose parade ground is `ground`, if its fort still stands.
    pub(crate) fn fort_of_ground(&self, ground: BuildingId) -> Option<BuildingId> {
        self.military.companies.iter().find(|c| c.ground == ground && c.fort != 0).map(|c| c.fort)
    }

    /// When a fort goes, its ground goes with it and its company disbands
    /// (FUN_004b82e0): the standard is gone at once, and its men, at hand or on
    /// their way, fall where they stand (0x48eacb), to lie as the dead do.
    pub(crate) fn remove_fort(&mut self, id: BuildingId) {
        let Some(c) = self.military.companies.iter().position(|c| c.fort == id) else { return };
        let company = self.military.companies[c].clone();
        if let Some(f) = self.figures.get_mut(company.standard) {
            f.dead = true;
        }
        for fid in company.soldiers.iter().chain(&company.recruits) {
            if let Some(f) = self.figures.get_mut(*fid)
                && f.action != action::CORPSE
                && f.action != action::ABOARD
            {
                f.action = action::CORPSE;
                f.counter = 0;
                f.foe = 0;
                f.route.clear();
                f.moving = false;
            } else if let Some(f) = self.figures.get_mut(*fid) {
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

    /// Whether two road tiles are on one road network, as the original's buildings
    /// keep a network number (+0x14) that its recruiter compares before sending a
    /// sentry or taking a delivery.
    pub(crate) fn same_road_network(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        a == b || crate::figures::find_route(&self.map, Travel::Roads, a, b).is_some()
    }

    /// Whether a cart with weapons or chariots on the road at `from` may take them
    /// to recruiter `b` (FUN_0045f7b0 for workshops' carts, FUN_0045bf00 for storage
    /// yards'): the recruiter is staffed, the city has a fort, and the recruiter's
    /// road is on the cart's road network.
    pub(crate) fn recruiter_takes_from(&self, b: &crate::buildings::Building, from: (i32, i32)) -> bool {
        b.workers > 0 && self.military.companies.iter().any(|c| c.fort != 0) && b.road.is_some_and(|r| self.same_road_network(from, r))
    }

    /// Whether this recruiter would enlist anyone: some fort is short of men it
    /// can outfit, or a tower lacks a sentry.
    pub fn recruits_wanted(&self, recruiter: BuildingId) -> bool {
        self.company_to_recruit(recruiter).is_some() || self.tower_wanting_sentry(recruiter).is_some()
    }

    /// The company a recruiter enlists for next: of the companies at their forts
    /// (not in the field, nor away fighting for the Kingdom) and short of men whom
    /// it can outfit, charioteers first, then infantry, then archers, and of those
    /// the one whose fort is nearest.
    fn company_to_recruit(&self, recruiter: BuildingId) -> Option<usize> {
        let b = self.buildings.get(recruiter)?;
        let has = |res: u16| b.stock.get(res as usize).copied().unwrap_or(0) > 0;
        self.military
            .companies
            .iter()
            .enumerate()
            .filter(|&(i, c)| c.fort != 0 && c.at_fort && c.abroad == 0 && !self.military.sent_away(i))
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

    /// A recruiter enlists a soldier, if some fort wants one (FUN_004b70c0): he
    /// takes a load of weapons (infantry) or chariots (charioteers) from its store,
    /// walks out of its door to the road beside the fully staffed military academy
    /// nearest the fort (FUN_004b75c0), if there is one with a road, and on to his
    /// place on the fort's parade ground. His company takes his experience at once:
    /// an academy's even if it has no road for him to call at.
    fn recruit(&mut self, recruiter: BuildingId) {
        let Some(c) = self.company_to_recruit(recruiter) else { return };
        let Some(road) = self.buildings.get(recruiter).and_then(|b| b.road) else { return };
        if let Some(res) = outfit(self.military.companies[c].kind) {
            // A whole load, without a floor at nothing (FUN_004b70c0 only checks there
            // was some): its stock comes and goes a load at a time, so it never dips
            // below.
            self.buildings.get_mut(recruiter).expect("present").stock[res as usize] -= crate::economy::LOAD;
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
        match academy.flatten() {
            Some(to) => self.go_to_academy(fid, to),
            None => self.send_to_post(fid),
        }
    }

    /// A recruit sets out for the academy's road: his company is marked as trained
    /// the moment he does (0x490357), and if no way leads there he is lost, as the
    /// original's soldier is when his route can't reach (state 0xd, direction 10).
    fn go_to_academy(&mut self, fid: FigureId, to: (i32, i32)) {
        if let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) {
            c.schooled = true;
        }
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = action::GOING_TO_ACADEMY;
        if !f.go_to(map, to) {
            self.lose_recruit(fid);
        }
    }

    /// A recruit who can't reach where he is going is gone.
    fn lose_recruit(&mut self, fid: FigureId) {
        if let Some(f) = self.figures.get_mut(fid) {
            f.dead = true;
        }
        for c in &mut self.military.companies {
            c.recruits.retain(|&s| s != fid);
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
            let (dx, dy) = slot_offset(f.slot, c.order, c.turned, c.kind);
            Some((c.standard_tile.0 + dx, c.standard_tile.1 + dy))
        }
    }

    /// Sends a soldier to his place, or, when he can't stand there or get there
    /// (a building, water, a wall in the way), to the nearest tile round it he can,
    /// trying them ring by ring out to six tiles as the original does (its
    /// alternative tiles at 0x5ca506, taken in turn each time the route fails).
    fn send_to_post(&mut self, fid: FigureId) {
        let Some(post) = self.post_of(fid) else { return };
        let at_fort = self.figures.get(fid).and_then(|f| self.military.companies.get(f.formation as usize - 1)).is_some_and(|c| c.at_fort);
        // A tile another man of the company is bound for, or has as his place, is
        // taken: unlike the original's men, Osiris's don't stack on one another.
        let others: Vec<FigureId> = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map(|c| c.soldiers.iter().chain(&c.recruits).copied().filter(|&s| s != fid).collect()).unwrap_or_default();
        let taken: Vec<(i32, i32)> = others.iter().flat_map(|&s| [self.figures.get(s).and_then(|o| o.destination), self.post_of(s)]).flatten().collect();
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = if at_fort { action::GOING_TO_FORT } else { action::GOING_TO_STANDARD };
        f.post = Some(post);
        let tiles = std::iter::once((0, 0)).chain(alternative_tiles()).map(|(dx, dy)| (post.0 + dx, post.1 + dy));
        for (i, tile) in tiles.enumerate() {
            if !crate::figures::passable(map, f.travel, tile.0, tile.1) || i > 0 && taken.contains(&tile) {
                continue;
            }
            // Routed from where his step ends, or a man turned mid-step stops a
            // tile off.
            if (f.x, f.y) == tile && !f.moving {
                f.destination = Some(tile);
                f.route.clear();
                f.aimed = false;
                f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
                return;
            }
            if f.reroute(map, tile) {
                return;
            }
        }
        // Nowhere near to stand: he stays put. (The original's man, out of tiles to
        // try, is struck off.)
        f.destination = Some((f.x, f.y));
        f.route.clear();
        f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
    }

    /// Whether a soldier stands in his place in the ranks, or where he went for want
    /// of it.
    fn in_place(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return true };
        !f.moving && f.destination == Some((f.x, f.y)) && f.post.is_some() && f.post == self.post_of(fid)
    }

    /// Whether a company's men must stay where they are rather than re-form: it has
    /// fought, loosed missiles or been struck by them lately.
    fn holding_where_they_are(&self, company: usize) -> bool {
        self.military.companies.get(company).is_some_and(|c| c.fought > 0 || c.shot > 0 || c.struck > 0)
    }

    /// Closes up a company's ranks (FUN_004b7c80): its living men, recruits on their
    /// way included, take places 0, 1, 2, ... in the order of their figure numbers,
    /// so a man lost leaves no gap.
    fn close_ranks(&mut self, company: usize) {
        let Some(c) = self.military.companies.get(company) else { return };
        let mut men: Vec<FigureId> = c.soldiers.iter().chain(&c.recruits).copied().filter(|&s| self.figures.get(s).is_some_and(|f| !f.dead && f.action != action::CORPSE)).collect();
        men.sort_unstable();
        for (slot, s) in men.into_iter().enumerate() {
            if let Some(f) = self.figures.get_mut(s) {
                f.slot = slot as u8;
            }
        }
    }

    /// Orders a company out to form up around `tile`. False, and nothing done, when
    /// its standard can't get there ("This company cannot reach its intended
    /// destination"). The standard is planted there at once, as the original's is
    /// (FUN_004b6400 only sets the company's spot, and the bearer stands on it,
    /// FUN_00490b50); the men march to it. Charioteers under orders to charge set
    /// off at the charge, and must rest their horses before charging again; sent on
    /// while still charging, they engage instead.
    pub fn move_company(&mut self, company: usize, tile: (i32, i32)) -> bool {
        // Aboard a transport, it goes where the ship takes it.
        if self.company_ship(company).is_some() || self.refuses_orders(company) {
            return false;
        }
        let Some(standard) = self.military.companies.get(company).map(|c| c.standard) else { return false };
        let Some(from) = self.figures.get(standard).map(|f| (f.x, f.y)) else { return false };
        if crate::figures::find_route(&self.map, Travel::Land, from, tile).is_none() {
            return false;
        }
        let c = &mut self.military.companies[company];
        // It faces the way it marches.
        if let Some(d) = general_direction(from, tile) {
            c.facing = d;
        }
        // Sent on during a charge, it engages instead (FUN_004b6400).
        let charge = c.order == Order::Charge && !c.charged;
        if c.order == Order::Charge && c.charged {
            c.order = Order::Engage;
        }
        if charge {
            c.charged = true;
            c.charge_rest = 0;
        }
        c.at_fort = false;
        c.standard_tile = tile;
        let soldiers = c.soldiers.clone();
        self.plant_standard(company);
        for s in soldiers {
            if self.figures.get(s).is_some_and(|f| f.action != action::ATTACK) {
                self.send_to_post(s);
                // At the charge (FUN_004b6a40: state 0x13, its run begun afresh).
                if let Some(f) = self.figures.get_mut(s)
                    && charge
                    && f.action == action::GOING_TO_STANDARD
                {
                    f.action = action::CHARGING;
                    f.counter = 0;
                }
            }
        }
        if charge && !self.military.companies[company].soldiers.iter().any(|&s| self.riding(s)) {
            self.military.companies[company].order = Order::Engage;
        }
        true
    }

    /// The standard stands where its company forms up: on its spot in the field, or
    /// on the parade ground at the fort (FUN_00490b50). Aboard a transport, it is
    /// with the ship.
    fn plant_standard(&mut self, company: usize) {
        let Some(c) = self.military.companies.get(company) else { return };
        let spot = if c.at_fort { self.buildings.get(c.ground).map(|g| (g.x, g.y)) } else { Some(c.standard_tile) };
        let at_fort = c.at_fort;
        let (Some(spot), Some(f)) = (spot, self.figures.get_mut(c.standard)) else { return };
        if f.action == action::ABOARD {
            return;
        }
        (f.x, f.y) = spot;
        f.destination = Some(spot);
        f.route.clear();
        f.moving = false;
        f.progress = 0;
        f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
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

    /// Whether a company stands halted, every man in his place (its flag hangs
    /// still), as it was found when last mustered.
    pub fn company_halted(&self, company: usize) -> bool {
        self.military.companies.get(company).is_some_and(|c| !c.marching)
    }

    /// Whether a soldier has arrived where he was going and stands there, not
    /// having turned to take aim since: the original's direction byte reads "at
    /// destination". A man drawn into a fight keeps what he was doing before.
    fn arrived(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return true };
        let doing = if f.action == action::ATTACK { f.resume } else { f.action };
        f.action == action::ABOARD || !f.moving && f.route.is_empty() && !f.aimed && matches!(doing, action::AT_STANDARD | action::AT_REST)
    }

    /// Musters a company (FUN_004b7c80): it is halted only if every man with it,
    /// recruits on their way included, has arrived.
    fn muster_company(&mut self, company: usize) {
        let Some(c) = self.military.companies.get(company) else { return };
        let men: Vec<FigureId> = c.soldiers.iter().chain(&c.recruits).copied().filter(|&s| self.figures.get(s).is_some_and(|f| !f.dead && f.action != action::CORPSE)).collect();
        let marching = !men.iter().all(|&s| self.arrived(s));
        self.military.companies[company].marching = marching;
    }

    /// Orders a company back to its fort. False, and nothing done, when its standard
    /// has no way to the parade ground ("This company cannot reach its intended
    /// destination", FUN_004b6c20). Men in a fight finish it first.
    pub fn return_company(&mut self, company: usize) -> bool {
        if self.company_ship(company).is_some() {
            return false;
        }
        let Some(c) = self.military.companies.get(company) else { return false };
        let home = self.buildings.get(c.ground).map(|g| (g.x, g.y));
        let from = self.figures.get(c.standard).map(|f| (f.x, f.y));
        let (Some(home), Some(from)) = (home, from) else { return false };
        if crate::figures::find_route(&self.map, Travel::Land, from, home).is_none() {
            return false;
        }
        let c = &mut self.military.companies[company];
        c.at_fort = true;
        let soldiers = c.soldiers.clone();
        self.plant_standard(company);
        for s in soldiers {
            if self.figures.get(s).is_some_and(|f| !matches!(f.action, action::ATTACK | action::CORPSE)) {
                self.send_to_post(s);
            }
        }
        true
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
        if act == action::ABOARD {
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
                    Step::Arrived => {
                        f.action = if act == action::GOING_TO_FORT { action::AT_REST } else { action::AT_STANDARD };
                        f.aimed = false;
                    }
                    // Something built across his way: he finds another (route
                    // result 9, 0x48e9e0).
                    Step::Blocked | Step::Lost => {
                        f.route.clear();
                        self.send_to_post(fid);
                    }
                }
                if act == action::GOING_TO_STANDARD {
                    self.engage(fid, 1);
                }
            }
            action::AT_STANDARD => {
                // Archers and charioteers throw at enemies in range (0x48fb05: types
                // 11 and 13 by their figure model's missile attack, range and rate: a
                // charioteer's 10, 8 tiles, every 120 ticks).
                if kind == ARCHER || kind == CHARIOTEER {
                    self.shoot_at_foes(fid);
                }
                self.engage(fid, 1);
                if self.figures.get(fid).is_none_or(|f| f.action != action::AT_STANDARD) {
                    return;
                }
                let Some(c) = self.company_of(fid) else { return };
                let (at_fort, order) = self.military.companies.get(c).map_or((true, Order::HoldTight), |c| (c.at_fort, c.order));
                let placed = self.in_place(fid);
                // Called home, he goes; out of his place, he takes it again once
                // the company has stopped fighting and shooting a while (state 12
                // of 0x48e9e0). Engaging, he goes after enemies only from his place
                // (state 0x14 is entered on arriving there); mopping up, from
                // wherever he is.
                if at_fort || !placed && !self.holding_where_they_are(c) {
                    self.send_to_post(fid);
                } else if order.reach() > 0 && (placed || order == Order::MopUp) {
                    self.chase(fid, order.reach());
                }
            }
            action::CHARGING => self.ride(fid),
            action::CHASING => {
                self.engage(fid, 1);
                if self.figures.get(fid).is_some_and(|f| f.action == action::CHASING) {
                    let reach = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.order.reach());
                    self.chase(fid, reach);
                }
            }
            action::AT_REST => {
                // At the fort a man takes his place on the parade ground at once
                // (state 8, FUN_004b53b0).
                if !self.in_place(fid) {
                    self.send_to_post(fid);
                }
            }
            action::ATTACK => {
                self.fight(fid);
                // The fight over, he goes back to what he was doing (FUN_004b1da0).
                let at_fort = self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_none_or(|c| c.at_fort);
                if let Some(f) = self.figures.get_mut(fid)
                    && f.action == 0
                {
                    // Called home meanwhile, he goes from where he stands.
                    let r = f.resume;
                    f.action = match r {
                        action::AT_REST | action::GOING_TO_FORT if at_fort => r,
                        action::AT_STANDARD | action::GOING_TO_STANDARD | action::CHASING if !at_fort => r,
                        _ => action::AT_STANDARD,
                    };
                }
            }
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
                // He calls at the academy's road and goes straight on to his fort
                // (0x490345: state 0xd, then 0x17). A building in his way sends
                // him round it; the academy going meanwhile doesn't stop him.
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => self.send_to_post(fid),
                    Step::Blocked | Step::Lost => {
                        let to = f.destination.unwrap_or((f.x, f.y));
                        self.go_to_academy(fid, to);
                    }
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
            // The distance the longer way, and of those as near the first by figure
            // number (FUN_004999e0).
            .min_by_key(|&&(id, ox, oy)| ((ox - x).abs().max((oy - y).abs()), id))
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
            if f.action != action::ATTACK {
                f.resume = f.action;
            }
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
            // The distance the longer way, and of those as near the first by figure
            // number (FUN_004999e0).
            .min_by_key(|&&(id, ox, oy)| ((ox - x).abs().max((oy - y).abs()), id))
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

    /// Whether a charioteer is at the charge, riding for his place round the spot
    /// the company was sent to.
    fn riding(&self, fid: FigureId) -> bool {
        self.figures.get(fid).is_some_and(|f| f.action == action::CHARGING)
    }

    /// A charioteer's turn at the charge (state 0x13 of 0x48e9e0): he rides for his
    /// place in the charge's close block round the company's spot, at twice his pace
    /// once he has ridden six tiles and until twenty, riding down whoever he comes
    /// upon. Once every man of it has arrived, the company engages (layout 15).
    fn ride(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let (x, y) = (f.x, f.y);
        f.speed = if CHARGE_RUN.contains(&f.counter) { 2 } else { 1 };
        let step = f.walk(map);
        f.speed = 1;
        let (nx, ny) = (f.x, f.y);
        match step {
            Step::Moving => {}
            Step::Arrived => {
                f.action = action::AT_STANDARD;
                f.aimed = false;
                f.counter = 0;
            }
            Step::Blocked | Step::Lost => {
                f.route.clear();
                self.send_to_post(fid);
                if let Some(f) = self.figures.get_mut(fid)
                    && f.action == action::GOING_TO_STANDARD
                {
                    f.action = action::CHARGING;
                }
            }
        }
        if (nx, ny) != (x, y) {
            // The run counts the tiles ridden since the charge began (+0x134).
            if let Some(f) = self.figures.get_mut(fid) {
                f.counter += 1;
            }
            self.trample(fid, (nx, ny));
        }
        let Some(c) = self.company_of(fid) else { return };
        let co = &self.military.companies[c];
        if co.order == Order::Charge && !co.soldiers.iter().any(|&s| self.riding(s)) {
            self.military.companies[c].order = Order::Engage;
        }
    }

    /// A soldier under orders to go after enemies heads for the nearest within `reach`
    /// tiles; when none is left, he stands where he is.
    fn chase(&mut self, fid: FigureId, reach: i32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y, kind, foe) = (f.x, f.y, f.kind, f.foe);
        // He keeps after the one he picked while it stands.
        let kept = (f.action == action::CHASING).then_some(foe).filter(|&o| self.figures.get(o).is_some_and(|o| !o.dead && o.action != action::CORPSE && self.foe_of_the_city(o)));
        let mut target = kept.or_else(|| self.seek_foe(fid, reach)).and_then(|o| self.figures.get(o).map(|o| (o.id, (o.x, o.y))));
        // Engaging, a man goes no further than six tiles from his place before he
        // turns back to it (state 0x14 at 0x48e9e0).
        let engaging = self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_some_and(|c| c.order == Order::Engage);
        if engaging && self.post_of(fid).is_some_and(|p| (p.0 - x).abs().max((p.1 - y).abs()) >= ENGAGE_LEASH) {
            target = None;
        }
        // An archer or charioteer near enough to loose at it stands and does.
        let range = self.fighter_stats(fid).missile_range;
        if let Some((id, to)) = target
            && matches!(kind, ARCHER | CHARIOTEER)
            && (to.0 - x).abs().max((to.1 - y).abs()) < range
            && self.figures.get(fid).is_some_and(|f| !f.moving)
        {
            let f = self.figures.get_mut(fid).expect("present");
            f.route.clear();
            f.destination = Some((x, y));
            f.post = None;
            f.foe = id;
            f.action = action::CHASING;
            self.shoot_at_foes(fid);
            return;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        // One he can't get at (a beast across the water, say) he leaves be, and goes
        // back to his place, as the original's man whose route fails does.
        if let Some((_, to)) = target
            && !f.moving
            && f.destination != Some(to)
        {
            let before = f.destination;
            if f.go_to(map, to) {
                // He has left his place.
                f.post = None;
            } else {
                f.destination = before;
                target = None;
            }
        }
        match target {
            Some((id, _)) => {
                f.action = action::CHASING;
                f.foe = id;
                f.walk(map);
            }
            // Nothing left to go after: he finishes his step and stands, to take his
            // place again when the company re-forms (state 0x14 to 12).
            None if f.action == action::CHASING => {
                f.route.clear();
                f.destination = Some(f.step_end());
                f.foe = 0;
                f.action = action::GOING_TO_STANDARD;
            }
            None => {}
        }
    }

    /// Whether the city's men go after a figure: an invader, or a wild beast about.
    fn foe_of_the_city(&self, o: &crate::figures::Figure) -> bool {
        self.is_invader(o) || crate::predators::is_predator(o.kind) && o.action != crate::predators::action::HIDDEN
    }

    /// The enemy a soldier sent after them picks within `reach` tiles (states 0x14
    /// and 0xe of 0x48e9e0). An archer or charioteer takes the mark he would loose
    /// at, looking `reach` tiles beyond his missile range (FUN_00499b50). An
    /// infantryman takes the nearest invader or beast, the distance taken the
    /// longer way, one whom another of the city's men is already after counting as
    /// twice as far, and of those as near the first by figure number (FUN_004999e0).
    fn seek_foe(&self, fid: FigureId, reach: i32) -> Option<FigureId> {
        let f = self.figures.get(fid)?;
        let (x, y) = (f.x, f.y);
        if matches!(f.kind, ARCHER | CHARIOTEER) {
            let range = self.fighter_stats(fid).missile_range;
            return self.missile_target((x, y), range + reach, false).map(|t| t.0);
        }
        let mut foes: Vec<(FigureId, i32, i32)> = self.combatants.invaders.iter().chain(&self.combatants.predators).copied().collect();
        foes.sort_unstable_by_key(|o| o.0);
        let wanted = |id: FigureId| self.combatants.defenders.iter().any(|&(d, _, _)| d != fid && self.figures.get(d).is_some_and(|a| a.foe == id && matches!(a.action, action::CHASING | action::ATTACK)));
        let mut best = None;
        let mut near = i32::MAX;
        for (id, ox, oy) in foes {
            if self.figures.get(id).is_none_or(|o| o.dead || o.action == action::CORPSE) {
                continue;
            }
            let d = (ox - x).abs().max((oy - y).abs());
            if d > reach {
                continue;
            }
            let d = if wanted(id) { 2 * d } else { d };
            if d < near {
                (best, near) = (Some(id), d);
            }
        }
        best
    }

    /// A charging chariot comes onto `tile`: the first enemy there takes its attack,
    /// four times over if it has ridden six to nineteen tiles of its charge, his armour
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

    /// The line a soldier fights in: his company, when it was found halted, every
    /// man in his place, at its last muster (the original's +0x5a, wherever the
    /// company stands). He faces its way when standing in it, or when fighting or
    /// walking that way.
    fn line(&self, fid: FigureId) -> Option<Line> {
        let f = self.figures.get(fid)?;
        if !is_soldier(f.kind) {
            return None;
        }
        let c = self.military.companies.get(self.company_of(fid)?)?;
        if c.marching {
            return None;
        }
        let facing = f.action == action::AT_STANDARD || toward(f.direction, c.facing);
        Some(Line { kind: c.kind, order: c.order, facing })
    }

    /// Changes a company's orders; its men re-form at their new places. A tight or
    /// loose line is turned if the company's rotate switch is on. Charioteers can't
    /// be ordered to charge while their horses recover from the last.
    pub fn set_order(&mut self, company: usize, order: Order) {
        if self.refuses_orders(company) {
            return;
        }
        let Some(c) = self.military.companies.get_mut(company) else { return };
        if order == Order::Charge && (c.kind != CHARIOTEER || c.charged) {
            return;
        }
        // Mopping up is for while there are enemies about: the orders it replaces
        // come back once they are gone (company +0x62).
        if order == Order::MopUp && c.order != Order::MopUp {
            c.before_mop_up = c.order;
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
        // A blow that fells its man is heard as his cry, one that doesn't as the blow.
        if self.hurt(foe, blow(attack, armor)) {
            self.learn(company, victim);
        } else {
            self.figure_sound(fid, 2);
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
        let (formation, band) = if crate::navy::is_enemy_ship(f.kind) { (0, 0) } else { (f.formation, f.band) };
        self.figure_sound(fid, 3);
        // The share of the company still standing that he was.
        let standing = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| {
            c.soldiers.iter().chain(&c.recruits).filter(|&&s| self.figures.get(s).is_some_and(|f| !f.dead && f.action != action::CORPSE)).count() as i32
        });
        // Whether it breaks is seen to once a day (`check_morale`). His company closes
        // its ranks at once (FUN_004b7ab0).
        if let Some(c) = self.company_of(fid) {
            self.close_ranks(c);
            self.muster_company(c);
            self.change_morale(c, -morale_loss(if standing > 0 { 100 / standing } else { 0 }));
        } else if formation >= 1000 {
            self.army_loses(formation as usize - 1000, band);
        }
        true
    }

    /// What a figure is worth as a mark for one of the city's archers or charioteers
    /// (FUN_00499b50): invaders 50, an enemy transport with troops aboard 40 and an
    /// empty one 20, an enemy warship 30, a wild beast 10. None for anyone else.
    fn city_mark(&self, o: &crate::figures::Figure) -> Option<i32> {
        use crate::navy::{EGYPT_TRANSPORT, EGYPT_WARSHIP, ENEMY_TRANSPORT, ENEMY_WARSHIP};
        if o.ship.as_ref().is_some_and(|s| s.hidden) {
            return None;
        }
        match o.kind {
            k if crate::invasions::is_invader_kind(k) => Some(50),
            ENEMY_TRANSPORT | EGYPT_TRANSPORT => Some(if o.amount > 0 { 40 } else { 20 }),
            ENEMY_WARSHIP | EGYPT_WARSHIP => Some(30),
            k if crate::predators::is_predator(k) && o.action != crate::predators::action::HIDDEN => Some(10),
            _ => None,
        }
    }

    /// What a figure is worth as a mark for an invading archer (FUN_00499e50), and
    /// how much farther off it counts: the city's soldiers and tower sentries 50, a
    /// transport with a company aboard 40 and an empty one 20, a warship 30; and
    /// while the city has fewer than four soldiers, anyone else of the city's at 10,
    /// counted five tiles farther. Never invaders, beasts, game, missiles,
    /// standards and the like.
    fn invader_mark(&self, o: &crate::figures::Figure, few_soldiers: bool) -> Option<(i32, i32)> {
        use crate::navy::{TRANSPORT, WARSHIP};
        if o.ship.as_ref().is_some_and(|s| s.hidden) || o.action == action::ABOARD {
            return None;
        }
        match o.kind {
            // (The original's figure 62 too, which Osiris has none of.)
            ARCHER | CHARIOTEER | INFANTRY | crate::defenses::TOWER_SENTRY | 62 => Some((50, 0)),
            TRANSPORT => Some((if o.ship.as_ref().is_some_and(|s| s.aboard.is_some()) { 40 } else { 20 }, 0)),
            WARSHIP => Some((30, 0)),
            0 | 6 | 14 | 36 | 37 | 41 | 43..=45 | 54..=56 | 58..=61 | 63 | 65 | 67 | 68 | 70 | 71 | 82..=84 | 86 | 92 | 93 | 95 | 97 | 99..=104 | 109 => None,
            _ => few_soldiers.then_some((10, 5)),
        }
    }

    /// The mark an archer at `at` looses at, the city's (`invader` false) or an
    /// invader's: going through the figures in order, one worth more than the mark
    /// so far and nearer than it, within `range` and in sight, takes its place
    /// (FUN_00499b50, FUN_00499e50). So the first of the best kind found is kept even
    /// if another of that kind is nearer, and a better mark farther off than a
    /// lesser one is passed over.
    pub(crate) fn missile_target(&self, at: (i32, i32), range: i32, invader: bool) -> Option<(FigureId, i32, i32)> {
        let few_soldiers = invader && self.figures.iter().filter(|o| is_soldier(o.kind) && !o.dead).count() < 4;
        let (mut best, mut worth, mut near) = (None, 0, range);
        for o in self.figures.iter().filter(|o| !o.dead && o.action != action::CORPSE) {
            let mark = if invader { self.invader_mark(o, few_soldiers) } else { self.city_mark(o).map(|w| (w, 0)) };
            let Some((w, farther)) = mark else { continue };
            let d = (o.x - at.0).abs().max((o.y - at.1).abs()) + farther;
            if w > worth && d < near && line_of_sight(&self.map, at, (o.x, o.y)) {
                (best, worth, near) = (Some((o.id, o.x, o.y)), w, d);
            }
        }
        best
    }

    /// Marks or unmarks company `company` for Kingdom service (0x40a390). A company
    /// away takes no change. A company aboard a transport can be marked only while
    /// a city by sea calls for troops, one ashore only while a city by land does;
    /// otherwise its mark is cleared.
    pub fn toggle_kingdom_service(&mut self, company: usize) {
        if self.company_away(company) || self.military.sent_away(company) {
            return;
        }
        let refused = self.service_refusal(self.company_ship(company).is_some()).is_some();
        if let Some(c) = self.military.companies.get_mut(company) {
            c.kingdom_service = !refused && !c.kingdom_service;
        }
    }

    /// The companies and ships marked for Kingdom service go off to fight `enemy` for
    /// the request `request` (FUN_004b84b0). For a city reached by land the marked
    /// companies ashore march out of the city; for one reached by sea only the
    /// marked companies aboard a transport go, their transports with them, and the
    /// marked warships. Companies already away for another call stay with it. They
    /// set out with the months the journey takes (FUN_0044c1a0: FUN_0044c110, or 12
    /// when that is nothing).
    pub(crate) fn send_to_battle(&mut self, request: usize, enemy: i32) {
        let sea = self.request_by_sea(request);
        let marked: Vec<usize> = (0..self.military.companies.len())
            .filter(|&c| self.military.companies[c].kingdom_service && !self.military.companies[c].soldiers.is_empty() && !self.military.sent_away(c) && self.company_ship(c).is_some() == sea)
            .collect();
        let exit = self.exit_point;
        for &c in &marked {
            self.military.companies[c].at_fort = false;
            if sea {
                if let Some(t) = self.company_ship(c) {
                    self.ship_to_battle(t, request);
                }
                continue;
            }
            let soldiers = self.military.companies[c].soldiers.clone();
            self.military.companies[c].abroad = soldiers.len() as i32;
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
        let warships = if sea { self.service_warships() } else { Vec::new() };
        for &s in &warships {
            self.ship_to_battle(s, request);
        }
        let strength = self.battle_strength(request, &marked, sea);
        let months = match self.request_travel_months(request) {
            m if m < 1 => 12,
            m => m,
        };
        self.military.battles.retain(|b| b.request != request);
        self.military.battles.push(DistantBattle { request, enemy, strength, companies: marked, months, fought: false, late: false, sea });
    }

    /// The strength the troops away for `request` bring to its battle, reckoned on
    /// the day (FUN_004b8360): each company (its men, and one more for each hundred
    /// points of its experience; an infantryman one more), and each warship a tenth
    /// of its hull.
    fn battle_strength(&self, request: usize, companies: &[usize], sea: bool) -> i32 {
        let men = companies
            .iter()
            .filter_map(|&c| self.military.companies.get(c))
            .map(|c| {
                let n = if sea { c.soldiers.len() as i32 } else { c.abroad };
                (c.experience + if c.kind == INFANTRY { 200 } else { 100 }) * n / 100
            })
            .sum::<i32>();
        men + if sea { self.warship_battle_strength(request) } else { 0 }
    }

    /// Older saved games kept one battle apart from the list.
    pub(crate) fn adopt_old_battle(&mut self) {
        if let Some(b) = self.military.battle.take() {
            self.military.battles.push(b);
        }
    }

    /// A month passes for the troops away for `request` when it is not the request's
    /// due month (FUN_0044c9e0, states 5 to 7). On the way they draw nearer, but go
    /// no nearer than a month off until the due month decides it. On the way home
    /// they come a month closer, and when they arrive the Overseer of the Military
    /// says so: message 288 after a victory, 287 when they got there too late.
    pub(crate) fn distant_battle_month(&mut self, request: usize) {
        let Some(k) = self.military.battles.iter().position(|b| b.request == request) else { return };
        let b = &mut self.military.battles[k];
        b.months -= 1;
        if !b.fought {
            b.months = b.months.max(1);
            return;
        }
        if b.months >= 1 {
            return;
        }
        let late = b.late;
        self.post(if late { "message_troops_return_failed" } else { "message_troops_return_victorious" }, None, true);
        self.troops_come_home(k);
    }

    /// The request's due month has come with its troops on the way (FUN_0044c9e0):
    /// if they are within two months of the battle they fight it (FUN_00446260), and
    /// the winners set out on the whole way home; otherwise they arrived too late and
    /// turn back, as far from home as they had come. Returns the outcome: `None`
    /// when they were too late, else whether they won. A lost battle leaves no one
    /// to come home.
    pub(crate) fn distant_battle_due(&mut self, request: usize) -> Option<bool> {
        let k = self.military.battles.iter().position(|b| b.request == request)?;
        let travel = self.request_travel_months(request);
        let b = self.military.battles[k].clone();
        if b.months >= 3 {
            let bb = &mut self.military.battles[k];
            bb.fought = true;
            bb.late = true;
            bb.months = (travel - b.months).max(1);
            return None;
        }
        let strength = self.battle_strength(b.request, &b.companies, b.sea);
        // Seth, if he has promised, sees the troops through without loss.
        let protected = std::mem::take(&mut self.religion.seth_protects);
        let won = protected || strength >= b.enemy;
        let losses = if protected {
            0
        } else if won {
            battle_losses((strength - b.enemy) * 100 / strength.max(1))
        } else {
            100
        };
        for &c in &b.companies {
            if b.sea {
                self.company_loses_aboard(c, losses);
            } else {
                let co = &mut self.military.companies[c];
                co.abroad -= co.abroad * losses / 100;
            }
        }
        if b.sea {
            self.warships_take_losses(b.request, losses);
        }
        if won {
            let bb = &mut self.military.battles[k];
            bb.fought = true;
            bb.months = travel.max(1);
            bb.strength = strength;
        } else {
            // No one comes home; the emptied companies wait at their forts for
            // recruits.
            self.military.battles.remove(k);
            for &c in &b.companies {
                if let Some(co) = self.military.companies.get_mut(c) {
                    co.abroad = 0;
                    co.at_fort = co.soldiers.is_empty() || co.at_fort;
                }
            }
        }
        Some(won)
    }

    /// The troops of battle `k` are home (FUN_004b8690): the survivors walk back to
    /// their forts from the city's exit, and the ships sail back.
    fn troops_come_home(&mut self, k: usize) {
        let b = self.military.battles.remove(k);
        let exit = self.exit_point;
        for &c in &b.companies {
            if b.sea {
                continue;
            }
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
        self.ships_come_home(b.request);
    }

    /// A company that fought from a transport loses `pct` of its men; lost to a man,
    /// its transport is lost with it (FUN_004b8890, FUN_004b8460).
    fn company_loses_aboard(&mut self, c: usize, pct: i32) {
        let Some(co) = self.military.companies.get_mut(c) else { return };
        let alive = co.soldiers.len() as i32;
        let kills = (alive * pct / 100) as usize;
        let lost: Vec<FigureId> = co.soldiers.split_off(co.soldiers.len() - kills);
        for s in lost {
            if let Some(f) = self.figures.get_mut(s) {
                f.dead = true;
            }
        }
        if kills as i32 == alive
            && alive > 0
            && let Some(t) = self.company_ship(c)
        {
            self.sink_abroad(t);
        }
    }
    /// Twice a day (FUN_004b7790): every company closes its ranks and is mustered,
    /// its re-forming waits run down (FUN_004b8b20), and a company mopping up with no enemy left on the field (no invaders, no wild
    /// beasts) goes back to its orders before, and re-forms.
    pub(crate) fn update_companies(&mut self) {
        let invaders = self.figures.iter().any(|f| !f.dead && f.action != action::CORPSE && self.is_invader(f));
        for c in 0..self.military.companies.len() {
            self.close_ranks(c);
            self.muster_company(c);
            // The re-forming waits run down, and are over once no invader is left
            // on the map; a man still fighting holds his company where it is.
            let fighting = self.military.companies[c].soldiers.iter().any(|&s| self.figures.get(s).is_some_and(|f| f.action == action::ATTACK));
            let co = &mut self.military.companies[c];
            for wait in [&mut co.fought, &mut co.shot, &mut co.struck] {
                *wait = if invaders { wait.saturating_sub(1) } else { 0 };
            }
            if fighting {
                co.fought = REFORM_WAIT;
            }
        }
        let enemies = self.figures.iter().any(|f| !f.dead && f.action != action::CORPSE && (self.is_invader(f) || crate::predators::is_predator(f.kind) && f.action != crate::predators::action::HIDDEN));
        if enemies {
            return;
        }
        for c in 0..self.military.companies.len() {
            let co = &mut self.military.companies[c];
            if co.order != Order::MopUp || co.fort == 0 {
                continue;
            }
            // Its line is turned as it was: mopping up doesn't change that.
            co.order = co.before_mop_up;
            if co.at_fort {
                continue;
            }
            for s in co.soldiers.clone() {
                if self.figures.get(s).is_some_and(|f| matches!(f.action, action::AT_STANDARD | action::GOING_TO_STANDARD | action::CHASING)) {
                    self.send_to_post(s);
                }
            }
        }
    }

    /// A company's morale changes by `delta`, from 0 up to its kind's highest
    /// (FUN_004b7b50).
    pub(crate) fn change_morale(&mut self, company: usize, delta: i32) {
        if let Some(c) = self.military.companies.get_mut(company) {
            c.morale = (c.morale + delta).min(morale_cap(c.kind, c.schooled)).max(0);
        }
    }

    /// Whether a company refuses orders: it has run from the field two days, or one
    /// day badly broken. One that broke only a day ago rallies to an order, gaining
    /// 10 morale (FUN_004b6400 and the other orders).
    fn refuses_orders(&mut self, company: usize) -> bool {
        if self.company_too_shaken(company) {
            return true;
        }
        let Some(c) = self.military.companies.get(company) else { return true };
        if c.routed == 1 {
            self.change_morale(company, BREAK_SHOCK);
        }
        false
    }

    /// Whether a company's morale is too low for it to take orders ("Company's
    /// morale is too low to respond!").
    pub fn company_too_shaken(&self, company: usize) -> bool {
        self.military.companies.get(company).is_none_or(|c| c.broken > 0 || c.routed >= 2)
    }

    /// Daily, at the original's formation tick (FUN_004b79f0): a company out of its
    /// fort, or an invaders' formation, whose morale has fallen to 20 breaks. The
    /// first time it does, the rest of its side lose 10 morale and the other side
    /// gain 10. A broken company runs home, its men in a fight finishing it first;
    /// a broken formation of invaders leaves the map the same way.
    pub(crate) fn check_morale(&mut self) {
        for c in 0..self.military.companies.len() {
            let co = &self.military.companies[c];
            if co.fort == 0 || co.at_fort || self.military.sent_away(c) || self.company_ship(c).is_some() {
                continue;
            }
            let fresh = co.routed == 0 && co.broken == 0;
            if co.morale > BROKEN_MORALE {
                continue;
            }
            // The shock falls on the company itself too, before it is judged.
            if fresh {
                self.side_shaken(false);
            }
            let co = &mut self.military.companies[c];
            if co.morale <= SHATTERED_MORALE {
                co.broken = co.broken.saturating_add(1);
            } else {
                co.routed = co.routed.saturating_add(1);
            }
            self.run_home(c);
        }
        self.check_enemy_morale();
    }

    /// The first break of a company (`invaders` false) or of a formation of invaders
    /// shakes its side and heartens the other.
    pub(crate) fn side_shaken(&mut self, invaders: bool) {
        let (ours, theirs) = if invaders { (BREAK_SHOCK, -BREAK_SHOCK) } else { (-BREAK_SHOCK, BREAK_SHOCK) };
        for c in 0..self.military.companies.len() {
            if self.military.companies[c].fort != 0 {
                self.change_morale(c, ours);
            }
        }
        self.enemy_morale_all(theirs);
    }

    /// A broken company runs for its fort: the men not fighting at once, those in a
    /// fight once it is over.
    fn run_home(&mut self, company: usize) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        c.at_fort = true;
        let soldiers = c.soldiers.clone();
        self.plant_standard(company);
        for s in soldiers {
            if self.figures.get(s).is_some_and(|f| !matches!(f.action, action::ATTACK | action::CORPSE)) {
                self.send_to_post(s);
            }
        }
    }

    pub(crate) fn update_morale_month(&mut self) {
        self.enemy_morale_month();
        for c in &mut self.military.companies {
            if c.at_fort {
                c.months_away = 0;
                (c.routed, c.broken) = (0, 0);
                c.morale = (c.morale + 5).min(morale_cap(c.kind, c.schooled));
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

    /// An archer looses a missile each time he has reloaded (his rate of fire, in
    /// ticks), at the mark the original would pick (`missile_target`), if any. The city's javelins
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
        let Some((target, tx, ty)) = self.missile_target((x, y), stats.missile_range, mine) else { return };
        if let Some(f) = self.figures.get_mut(fid) {
            f.direction = crate::figures::direction_to((x, y), (tx, ty)).unwrap_or(f.direction);
            f.aimed = is_soldier(f.kind);
        }
        if let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) {
            c.shot = REFORM_WAIT;
        }
        self.figure_sound(fid, 2);
        let missile = self.figures.spawn(if mine { ARROW } else { JAVELIN }, x, y, Travel::Land);
        let company = if mine { 0 } else { self.company_of(fid).map_or(0, |c| c as u16 + 1) };
        let attack = if mine { self.balance.unit(SPEAR).missile_attack } else { stats.missile_attack };
        if let Some(m) = self.figures.get_mut(missile) {
            m.foe = target;
            // The company whose man loosed it, which learns from a kill.
            m.formation = company;
            m.amount = attack;
            // A ship the invader's missile strikes takes the archer's own missile
            // attack.
            if mine {
                m.cargo = stats.missile_attack.max(0) as u16;
            }
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
        let (target, attack, company, theirs, shooter) = (m.foe, m.amount, m.formation.checked_sub(1).map(|c| c as usize), m.kind == ARROW, m.cargo as i32);
        let hit = self.figures.get(target).is_some_and(|t| !t.dead && t.action != action::CORPSE && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1);
        if !hit {
            return;
        }
        let armor = self.fighter_stats(target).missile_armor;
        let (kind, holding) = self.figures.get(target).map_or((0, false), |t| (t.kind, t.action == crate::navy::ship::HOLD));
        if theirs && (crate::navy::is_city_ship(kind) || kind == crate::fishing::FISHING_BOAT) {
            // A ship struck takes the shooter's missile attack, all or nothing; a
            // warship holding position has 2 more armour (the missile's action at
            // 0x4a1ef0).
            let armor = armor + if kind == crate::navy::WARSHIP && holding { 2 } else { 0 };
            self.hurt(target, spear(shooter, armor));
        } else if theirs {
            // A company struck by a missile holds where it stands a while (the arrow's
            // hit, FUN_0049a5a0).
            if let Some(c) = self.company_of(target).and_then(|c| self.military.companies.get_mut(c)) {
                c.struck = REFORM_WAIT;
            }
            let seasoned = if is_soldier(kind) { (self.company_of(target).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.experience) + 10) / 20 } else { 0 };
            let armor = (armor + seasoned).min(MAX_ARMOR) + self.line(target).map_or(0, |l| l.missile_armor());
            self.hurt(target, spear(attack, armor));
        } else if self.hurt(target, blow(attack, armor.min(MAX_ARMOR))) {
            self.learn(company, kind);
        }
    }

    /// The standard bearer stands with the company's flag where it forms up.
    pub(crate) fn update_standard_bearer(&mut self, fid: FigureId) {
        if let Some(c) = self.company_of(fid) {
            self.plant_standard(c);
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
    fn alternative_tiles_spiral_out_as_the_originals() {
        let tiles: Vec<(i32, i32)> = alternative_tiles().collect();
        assert_eq!(tiles.len(), 168);
        assert_eq!(&tiles[..10], &[(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -2), (1, -2)]);
        assert_eq!(&tiles[22..25], &[(-2, -2), (-1, -2), (0, -3)]);
        assert_eq!(tiles[167], (-1, -6));
        let mut seen = tiles.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 168);
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
        assert_eq!(slot_offset(15, Order::HoldTight, true, INFANTRY), (1, -4));
        assert_eq!(slot_offset(14, Order::HoldLoose, false, INFANTRY), (8, 0));
        assert_eq!(slot_offset(1, Order::HoldTight, false, CHARIOTEER), (0, 2));
        assert_eq!(slot_offset(15, Order::HoldTight, true, CHARIOTEER), (2, -4));
        assert_eq!(slot_offset(14, Order::HoldLoose, false, CHARIOTEER), (8, 0));
    }

    #[test]
    fn ranks_and_ball() {
        assert_eq!((experience_rank(0), experience_rank(9), experience_rank(10), experience_rank(89), experience_rank(90), experience_rank(100)), (0, 0, 1, 4, 5, 5));
        assert_eq!((experience_ball(0), experience_ball(2), experience_ball(50), experience_ball(97), experience_ball(100)), (20, 19, 10, 0, 0));
    }

    /// Mission 2's open ground south of the river: a road with a recruiter at its
    /// west end, an archers' fort below it and, if `academy`, a military academy on
    /// the road 18 tiles east, everything staffed.
    fn barracks(academy: bool) -> Option<World> {
        barracks_of(FORT_ARCHERS, academy)
    }

    fn barracks_of(fort: u16, academy: bool) -> Option<World> {
        use crate::world::{Command, Outcome};
        let mut w = crate::irrigation::tests::mission_world(2)?;
        w.invasions.planned.clear();
        // The fighters' stats come from the difficulty's figure model.
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        w.balance = crate::balance::Balance::load_all(&data).ok()?[crate::difficulty::NORMAL as usize].clone();
        let mut steps = vec![Command::Road { start: (40, 70), end: (66, 70) }, Command::Build { kind: RECRUITER, x: 42, y: 71, x1: 42, y1: 71 }, Command::Build { kind: fort, x: 44, y: 75, x1: 44, y1: 75 }];
        if academy {
            steps.push(Command::Build { kind: ACADEMIES[0], x: 60, y: 71, x1: 60, y1: 71 });
        }
        for cmd in steps {
            assert!(matches!(w.apply(&cmd), Outcome::Done { items: 1.., .. }), "{cmd:?}");
        }
        Some(w)
    }

    /// Ticks until the first recruit has joined his company, and returns the tiles
    /// he walked.
    fn first_recruit(w: &mut World) -> Vec<(i32, i32)> {
        let mut walked = Vec::new();
        let mut recruit = None;
        for _ in 0..40 * crate::time::TICKS_PER_DAY {
            w.tick();
            let c = &w.military.companies[0];
            recruit = recruit.or(c.recruits.first().copied());
            if let Some(f) = recruit.and_then(|s| w.figures.get(s))
                && walked.last() != Some(&(f.x, f.y))
            {
                walked.push((f.x, f.y));
            }
            if recruit.is_some_and(|s| c.soldiers.contains(&s)) {
                return walked;
            }
        }
        panic!("no recruit joined: walked {walked:?}");
    }

    #[test]
    fn recruits_call_at_the_academy_and_bring_its_training() {
        let Some(mut w) = barracks(true) else { return };
        let walked = first_recruit(&mut w);
        // He sets out from the recruiter's road, calls at the academy's and goes back
        // to the parade ground.
        let academy_road = w.buildings.iter().find(|b| b.kind == ACADEMIES[0]).and_then(|b| b.road).expect("academy road");
        assert_eq!(walked.first(), Some(&(42, 70)), "{walked:?}");
        assert!(walked.contains(&academy_road), "never reached the academy at {academy_road:?}: {walked:?}");
        let c = &w.military.companies[0];
        assert!(fort_ground(44, 75).any(|t| Some(&t) == walked.last()), "{walked:?}");
        // His company has the academy's experience, and its morale can climb to 80.
        assert_eq!((c.experience, c.schooled), (ACADEMY_EXPERIENCE, true));
        for _ in 0..12 {
            w.update_morale_month();
        }
        assert_eq!(w.military.companies[0].morale, 80);

        // Without an academy he goes straight to the fort, raw, and the company's
        // morale stops at 60.
        let Some(mut w) = barracks(false) else { return };
        let walked = first_recruit(&mut w);
        assert!(walked.iter().all(|&(x, _)| x < 50), "{walked:?}");
        for _ in 0..12 {
            w.update_morale_month();
        }
        let c = &w.military.companies[0];
        assert_eq!((c.experience, c.schooled, c.morale), (0, false, 60));
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
        assert_eq!((c.experience, c.schooled), (ACADEMY_EXPERIENCE, true));
        assert!(!rmp_serde::to_vec_named(&c).unwrap().windows(7).any(|w| w == b"trained"));
    }

    /// Ticks until company 0 has a full company of men with it, or panics.
    fn muster(w: &mut World) {
        for _ in 0..400 * crate::time::TICKS_PER_DAY {
            w.tick();
            if w.military.companies[0].soldiers.len() == COMPANY_SIZE {
                return;
            }
        }
        panic!("company never filled: {:?}", w.military.companies[0]);
    }

    /// Every man of company 0 stands at his own place: on the field around the
    /// standard, or on the parade ground at the fort.
    fn assert_in_formation(w: &World, state: u16) {
        let c = &w.military.companies[0];
        let mut tiles = Vec::new();
        for &s in &c.soldiers {
            let f = w.figures.get(s).expect("soldier");
            assert_eq!(f.action, state, "soldier {s} at {:?}", (f.x, f.y));
            assert_eq!(Some((f.x, f.y)), w.post_of(s), "soldier {s} in slot {} stands off his place: {:?} {:?} {:?} {}", f.slot, f.destination, f.post, f.route, crate::figures::passable(&w.map, Travel::Land, f.x, f.y));
            tiles.push((f.x, f.y));
        }
        tiles.sort();
        tiles.dedup();
        assert_eq!(tiles.len(), c.soldiers.len(), "men share a tile");
    }

    #[test]
    fn a_company_musters_beats_an_army_and_goes_home() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 16 * crate::economy::LOAD);
        muster(&mut w);
        // A load of weapons went with each man.
        assert_eq!(w.stored(recruiter, WEAPONS), 0);
        assert_in_formation(&w, action::AT_REST);

        // Twenty raiders come; the company is sent to mop them up.
        w.invade_now(crate::invasions::invader::BEDOUIN, 20, 1);
        for _ in 0..100 {
            w.tick();
        }
        w.set_order(0, Order::MopUp);
        // They wait where they came in, the ground round the fort being held, so the
        // company goes out to meet them.
        let e = w.invasions.armies[0].entry;
        assert!(w.move_company(0, (e.0 - 12, e.1 - 4)));
        let mut over = false;
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
            let fighting = w.invasions.armies.iter().any(|a| a.figures.iter().any(|&f| w.figures.get(f).is_some_and(|f| !f.dead && !matches!(f.action, action::CORPSE | action::FLEEING))));
            // The enemy gone, the men re-form round the standard.
            if !fighting && w.military.companies[0].soldiers.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action == action::AT_STANDARD) && w.in_place(s)) {
                over = true;
                break;
            }
        }
        assert!(over, "the battle never ended: {:?} men {:?} armies {:?}", w.military.companies[0], w.military.companies[0].soldiers.iter().filter_map(|&s| w.figures.get(s)).map(|f| (f.x, f.y, f.action, f.slot, f.destination, f.route.len())).collect::<Vec<_>>(), w.invasions.armies.iter().map(|a| (a.band_morale.clone(), a.figures.iter().filter_map(|&f| w.figures.get(f)).map(|f| (f.x, f.y, f.action)).collect::<Vec<_>>())).collect::<Vec<_>>());
        let c = &w.military.companies[0];
        assert!(c.experience > 0 && !c.soldiers.is_empty(), "{c:?}");
        // Back in its lines, each man in his own place.
        assert_in_formation(&w, action::AT_STANDARD);

        w.return_company(0);
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_in_formation(&w, action::AT_REST);
    }

    #[test]
    fn a_company_marches_off_to_a_distant_battle_and_comes_home() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 16 * crate::economy::LOAD);
        muster(&mut w);
        // A city two months away by land calls for troops against 20, due in three.
        let route = w.trade.routes.len();
        w.trade.routes.push(crate::trade::TradeRoute { points: vec![(0, 0), (800, 0)], step: 5, ..Default::default() });
        w.trade.cities.push(crate::trade::TradeCity { route: route as u8, ..Default::default() });
        let i = w.scenario_events.request_troops_now(20);
        let due = w.month_count() + 3;
        let e = &mut w.scenario_events.list[i];
        e.city = Some(w.trade.cities.len() - 1);
        e.route = route as i32;
        e.due = Some(due);
        assert!(!w.request_by_sea(i));
        assert!(!w.can_send_request(i), "no company is marked");
        w.toggle_kingdom_service(0);
        assert!(w.can_send_request(i));
        assert!(w.dispatch_request(i));
        // They march out of the city and are gone; the fort sends for no recruits
        // while they are away.
        for _ in 0..30 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert!(w.military.companies[0].soldiers.is_empty(), "{:?}", w.military.companies[0]);
        assert_eq!(w.company_men(0), 16);
        assert!(!w.recruits_wanted(recruiter));
        // Infantry count two each: 32 against 20, 37% ahead, a quarter lost.
        assert_eq!(w.military.battle_for(i).map(|b| b.strength), Some(32));
        for _ in 0..8 * 31 * crate::time::TICKS_PER_DAY {
            w.tick();
            if w.military.battles.is_empty() {
                break;
            }
        }
        assert!(w.military.battles.is_empty());
        assert_eq!(w.scenario_events.list[i].state, crate::scenario_events::state::RECEIVED);
        assert!(w.notices.log.iter().any(|n| n.key == "message_troops_return_victorious"));
        // The twelve left walk back to the fort and take their places.
        for _ in 0..30 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 12, "{:?}", w.military.companies[0]);
        assert_in_formation(&w, action::AT_REST);
    }

    #[test]
    fn the_standard_goes_first_and_the_company_halts_when_all_are_in_place() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 4 * crate::economy::LOAD);
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 4);
        assert!(w.company_halted(0));
        // Sent out, the standard stands on its spot at once; the men march to it.
        assert!(w.move_company(0, (40, 60)));
        let standard = w.military.companies[0].standard;
        assert_eq!(w.figures.get(standard).map(|f| (f.x, f.y, f.action)), Some((40, 60, action::AT_STANDARD)));
        w.tick();
        assert_eq!(w.figures.get(standard).map(|f| (f.x, f.y, f.moving)), Some((40, 60, false)));
        // Mustered while they are on the way, it is not halted, and stays so until
        // the muster after the last man is in his place.
        w.update_companies();
        assert!(!w.company_halted(0));
        let mut arrived_at = None;
        for t in 0..20 * crate::time::TICKS_PER_DAY {
            w.tick();
            let men = &w.military.companies[0].soldiers;
            if arrived_at.is_none() && men.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action == action::AT_STANDARD && !f.moving)) {
                arrived_at = Some(t);
                assert!(!w.company_halted(0), "halted before a muster");
            }
            if w.company_halted(0) {
                let t0 = arrived_at.expect("halted before every man arrived");
                assert!(t - t0 <= crate::time::TICKS_PER_DAY, "{t0} {t}");
                assert_in_formation(&w, action::AT_STANDARD);
                // Back to the fort: the standard is on the parade ground at once.
                w.return_company(0);
                let g = w.buildings.get(w.military.companies[0].ground).map(|g| (g.x, g.y));
                assert_eq!(w.figures.get(standard).map(|f| (f.x, f.y)), g);
                return;
            }
        }
        panic!("never halted");
    }

    #[test]
    fn ranks_close_after_a_loss_once_the_fighting_has_died_down() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 4 * crate::economy::LOAD);
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert!(w.move_company(0, (40, 60)));
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_in_formation(&w, action::AT_STANDARD);
        let mut men = w.military.companies[0].soldiers.clone();
        men.sort_unstable();
        assert_eq!(men.len(), 4);
        // The second man falls: the others close up in the order of their numbers.
        w.military.companies[0].fought = REFORM_WAIT;
        assert!(w.hurt(men[1], 1000));
        let survivors = [men[0], men[2], men[3]];
        assert_eq!(survivors.map(|s| w.figures.get(s).map(|f| f.slot)), [Some(0), Some(1), Some(2)]);
        // While the company has lately fought, the men stay where they stand.
        let stood: Vec<(i32, i32)> = survivors.iter().filter_map(|&s| w.figures.get(s).map(|f| (f.x, f.y))).collect();
        assert!(survivors.iter().any(|&s| !w.in_place(s)));
        for _ in 0..100 {
            w.gather_combatants();
            for s in survivors {
                w.update_soldier(s);
            }
        }
        let now: Vec<(i32, i32)> = survivors.iter().filter_map(|&s| w.figures.get(s).map(|f| (f.x, f.y))).collect();
        assert_eq!(now, stood);
        // With no invader about the wait is over at the next muster, and they re-form.
        w.update_companies();
        assert_eq!(w.military.companies[0].fought, 0);
        for _ in 0..5 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 3);
        assert_in_formation(&w, action::AT_STANDARD);
    }

    #[test]
    fn men_whose_places_are_built_on_stand_beside_them() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 16 * crate::economy::LOAD);
        muster(&mut w);
        // A tight line formed at the recruiter's west edge would put half the men on
        // it: they stand round it instead, each on his own tile.
        assert!(w.move_company(0, (41, 72)));
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        let c = &w.military.companies[0];
        let mut tiles = Vec::new();
        for &s in &c.soldiers {
            let f = w.figures.get(s).expect("soldier");
            assert_eq!(f.action, action::AT_STANDARD, "{:?}", (f.x, f.y));
            assert!(crate::figures::passable(&w.map, Travel::Land, f.x, f.y), "soldier on {:?}", (f.x, f.y));
            assert!(w.map.building.at_or(f.x, f.y, 0) != recruiter);
            tiles.push((f.x, f.y));
        }
        tiles.sort();
        tiles.dedup();
        assert_eq!(tiles.len(), COMPANY_SIZE, "men share a tile");
    }

    #[test]
    fn charioteers_throw_javelins() {
        let Some(mut w) = barracks_of(FORT_CHARIOTEERS, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, CHARIOTS, crate::economy::LOAD);
        for _ in 0..30 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 1);
        assert_eq!(w.stored(recruiter, CHARIOTS), 0);
        assert!(w.move_company(0, (40, 60)));
        for _ in 0..5 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        let man = w.military.companies[0].soldiers[0];
        let (x, y) = w.figures.get(man).map(|f| (f.x, f.y)).expect("charioteer");
        // An invader six tiles off, inside his range of 8.
        w.invade_now(crate::invasions::invader::BEDOUIN, 1, 1);
        let them = w.invasions.armies[0].figures[0];
        let mut thrown = false;
        for _ in 0..200 {
            if let Some(f) = w.figures.get_mut(them) {
                (f.x, f.y, f.action) = (x - 6, y, 0);
                f.route.clear();
                f.moving = false;
            }
            w.gather_combatants();
            w.update_soldier(man);
            thrown |= w.figures.iter().any(|f| f.kind == JAVELIN && f.formation == 1);
        }
        assert!(thrown, "{:?} {:?}", w.figures.get(man).map(|f| (f.action, f.attack_tick, f.x, f.y)), w.fighter_stats(man));
    }

    #[test]
    fn a_charge_rides_out_once_and_then_engages() {
        let Some(mut w) = barracks_of(FORT_CHARIOTEERS, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, CHARIOTS, 2 * crate::economy::LOAD);
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 2);
        w.set_order(0, Order::Charge);
        assert!(w.move_company(0, (40, 50)));
        let men = w.military.companies[0].soldiers.clone();
        assert!(men.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action == action::CHARGING)));
        // A man's first six tiles take twice as long as the next six.
        let man = men[0];
        let mut reached = Vec::new();
        let mut last = w.figures.get(man).map(|f| (f.x, f.y));
        for t in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
            let now = w.figures.get(man).map(|f| (f.x, f.y));
            if now != last {
                reached.push(t);
                last = now;
            }
            if w.military.companies[0].order != Order::Charge {
                break;
            }
        }
        assert!(reached.len() >= 13, "{reached:?}");
        let (slow, fast) = (reached[6] - reached[0], reached[12] - reached[6]);
        assert!(fast * 3 <= slow * 2, "{slow} {fast}");
        // All in, the company engages; the charge is spent until the horses rest.
        let c = &w.military.companies[0];
        assert_eq!((c.order, c.charged), (Order::Engage, true));
        assert!(men.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action != action::CHARGING)));
        w.set_order(0, Order::Charge);
        assert_eq!(w.military.companies[0].order, Order::Engage);
    }

    #[test]
    fn called_home_fighters_finish_and_the_cut_off_stay() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 3 * crate::economy::LOAD);
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert!(w.move_company(0, (40, 60)));
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_in_formation(&w, action::AT_STANDARD);
        // A raider comes at one of the men; while they fight the company is called
        // home. The others go; he fights on, and goes once the raider is down.
        w.invade_now(crate::invasions::invader::BEDOUIN, 1, 1);
        let them = w.invasions.armies[0].figures[0];
        let man = w.military.companies[0].soldiers[0];
        let (x, y) = w.figures.get(man).map(|f| (f.x, f.y)).expect("man");
        if let Some(f) = w.figures.get_mut(them) {
            (f.x, f.y) = (x - 1, y);
            f.route.clear();
            f.moving = false;
        }
        w.gather_combatants();
        w.engage(man, 1);
        assert_eq!(w.figures.get(man).map(|f| (f.action, f.foe)), Some((action::ATTACK, them)));
        assert!(w.return_company(0));
        assert_eq!(w.figures.get(man).map(|f| (f.action, f.foe)), Some((action::ATTACK, them)));
        let others: Vec<FigureId> = w.military.companies[0].soldiers.iter().copied().filter(|&s| s != man).collect();
        assert!(others.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action == action::GOING_TO_FORT)));
        assert!(w.hurt(them, 1000));
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_in_formation(&w, action::AT_REST);
        // Walled in out in the field, the company can't be called home.
        assert!(w.move_company(0, (40, 60)));
        for _ in 0..10 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        for d in -6..=6 {
            for (wx, wy) in [(40 + d, 54), (40 + d, 66), (34, 60 + d), (46, 60 + d)] {
                w.map.terrain.set(wx, wy, crate::map::terrain::WALL);
            }
        }
        assert!(!w.return_company(0));
        assert!(!w.military.companies[0].at_fort);
    }

    #[test]
    fn archers_shoot_at_what_they_can_see_and_the_original_would_pick() {
        use crate::map::terrain::{TREE, WALL};
        let Some(mut w) = crate::irrigation::tests::mission_world(2) else { return };
        for y in 50..70 {
            for x in 30..50 {
                w.map.terrain.set(x, y, 0);
            }
        }
        assert!(line_of_sight(&w.map, (35, 60), (45, 60)));
        assert!(line_of_sight(&w.map, (35, 60), (42, 55)));
        w.map.terrain.set(40, 60, TREE);
        assert!(!line_of_sight(&w.map, (35, 60), (45, 60)));
        // From the walls, the first tiles out (six looks, about three tiles) don't
        // block.
        w.map.terrain.set(37, 60, TREE);
        w.map.terrain.set(40, 60, 0);
        w.map.terrain.set(35, 60, WALL);
        assert!(line_of_sight(&w.map, (35, 60), (45, 60)));
        w.map.terrain.set(35, 60, 0);
        assert!(!line_of_sight(&w.map, (35, 60), (45, 60)));
        w.map.terrain.set(37, 60, 0);

        // The first mark found that is worth more and nearer is taken: a beast first
        // and nearer keeps an invader farther off from being picked.
        assert!(crate::predators::is_predator(82));
        let beast = w.figures.spawn(82, 37, 60, Travel::Land);
        let raider = w.figures.spawn(43, 40, 60, Travel::Land);
        assert_eq!(w.missile_target((35, 60), 10, false).map(|t| t.0), Some(beast));
        w.figures.get_mut(beast).expect("beast").dead = true;
        assert_eq!(w.missile_target((35, 60), 10, false).map(|t| t.0), Some(raider));
        // Out of range, or out of sight, it is passed over.
        assert_eq!(w.missile_target((35, 60), 5, false), None);
        w.map.terrain.set(38, 60, TREE);
        assert_eq!(w.missile_target((35, 60), 10, false), None);
        w.map.terrain.set(38, 60, 0);

        // Invading archers go for soldiers, and for anyone else only while the city
        // has fewer than four, five tiles farther than he stands.
        let townsman = w.figures.spawn(1, 37, 62, Travel::Land);
        assert_eq!(w.missile_target((40, 60), 10, true).map(|t| t.0), Some(townsman));
        assert_eq!(w.missile_target((40, 60), 7, true), None);
        let soldier = w.figures.spawn(INFANTRY, 46, 60, Travel::Land);
        assert_eq!(w.missile_target((40, 60), 10, true).map(|t| t.0), Some(soldier));
        for i in 0..4 {
            w.figures.spawn(ARCHER, 70 + i, 90, Travel::Land);
        }
        w.figures.get_mut(soldier).expect("soldier").dead = true;
        assert_eq!(w.missile_target((40, 60), 10, true), None);
    }

    #[test]
    fn mopping_up_ends_when_the_enemy_is_gone() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        w.set_order(0, Order::HoldLoose);
        w.set_order(0, Order::MopUp);
        assert!(w.move_company(0, (40, 60)));
        w.invade_now(crate::invasions::invader::BEDOUIN, 4, 1);
        w.update_companies();
        assert_eq!(w.military.companies[0].order, Order::MopUp, "invaders about");
        // The invaders and beasts gone, it takes its loose line again.
        let ids: Vec<FigureId> = w.figures.iter().filter(|f| w.is_invader(f) || crate::predators::is_predator(f.kind)).map(|f| f.id).collect();
        for id in ids {
            w.figures.get_mut(id).expect("figure").dead = true;
        }
        w.update_companies();
        assert_eq!(w.military.companies[0].order, Order::HoldLoose);
    }

    #[test]
    fn a_fort_goes_with_its_ground_and_its_men_fall() {
        use crate::world::{Command, Outcome};
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 3 * crate::economy::LOAD);
        for _ in 0..40 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        let men = w.military.companies[0].soldiers.clone();
        assert!(!men.is_empty());
        // Clearing the parade ground takes the fort; its men fall and lie a while.
        let (gx, gy) = (44 + GROUND_OFFSET.0, 75 + GROUND_OFFSET.1);
        assert!(matches!(w.apply(&Command::Clear { x0: gx + 1, y0: gy + 1, x1: gx + 1, y1: gy + 1 }), Outcome::Done { .. }));
        assert!(w.buildings.iter().all(|b| !matches!(b.kind, FORT_INFANTRY | FORT_GROUND)));
        assert_eq!(w.military.companies[0].fort, 0);
        assert!(men.iter().all(|&s| w.figures.get(s).is_some_and(|f| f.action == action::CORPSE)));
        for _ in 0..CORPSE_TICKS {
            w.tick();
        }
        assert!(men.iter().all(|&s| w.figures.get(s).is_none_or(|f| f.dead)));
        // Six forts at most: once there are six, a seventh is refused anywhere.
        let spots: Vec<(i32, i32)> = (40..140).step_by(8).flat_map(|y| (10..140).step_by(9).map(move |x| (x, y))).collect();
        for &(x, y) in &spots {
            if !w.forts_full() {
                w.apply(&Command::Build { kind: FORT_ARCHERS, x, y, x1: x, y1: y });
            }
        }
        assert!(w.forts_full());
        assert_eq!(w.buildings.iter().filter(|b| fort_soldier(b.kind).is_some()).count(), 6);
        assert!(spots.iter().all(|&(x, y)| w.can_place(FORT_ARCHERS, x, y) != Ok(())));
        assert!(spots.iter().any(|&(x, y)| w.can_place(FORT_ARCHERS, x, y) == Err(FORTS_FULL)));
    }

    #[test]
    fn broken_sides_run_once_a_day_and_shake_the_rest() {
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 4 * crate::economy::LOAD);
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.military.companies[0].soldiers.len(), 4);
        assert!(w.move_company(0, (40, 60)));
        w.invade_now(crate::invasions::invader::BEDOUIN, 8, 1);
        // An invaders' formation starts at 100; its first loss holds it to the
        // Bedouin's 80.
        assert_eq!(w.band_morale(0, 0), 100);
        let invader = w.invasions.armies[0].figures[0];
        w.army_loses(0, 0);
        assert_eq!(w.band_morale(0, 0), 80);
        // Broken: it leaves once the day's check comes, and the city's men take heart.
        w.military.companies[0].morale = 40;
        w.invasions.armies[0].band_morale[0] = 20;
        w.check_morale();
        assert!(w.invasions.armies[0].withdrawn.contains(&0));
        assert_eq!(w.figures.get(invader).map(|f| f.action), Some(action::FLEEING));
        assert_eq!(w.military.companies[0].morale, 50);
        // A company broken in the field runs home, the shock of it bringing it to 5,
        // badly broken, and its enemies take heart. It takes no orders.
        w.military.companies[0].morale = 15;
        let before = w.band_morale(0, 0);
        w.check_morale();
        let c = &w.military.companies[0];
        assert!(c.at_fort && c.routed == 0 && c.broken == 1 && c.morale == 5, "{c:?}");
        assert_eq!(w.band_morale(0, 0), (before + 10).min(80));
        assert!(!w.move_company(0, (41, 60)));
        // Broken a day, but not badly, an order rallies it (+10); two days, not.
        (w.military.companies[0].routed, w.military.companies[0].broken) = (1, 0);
        assert!(w.move_company(0, (40, 60)));
        assert_eq!(w.military.companies[0].morale, 15);
        assert!(!w.military.companies[0].at_fort);
        w.military.companies[0].routed = 2;
        assert!(!w.move_company(0, (41, 60)));
        // A month at the fort sets it right.
        w.return_company(0);
        w.update_morale_month();
        let c = &w.military.companies[0];
        assert_eq!((c.routed, c.broken), (0, 0));
    }

    /// A recruiter serves only its own road network (FUN_004b7400, FUN_0045f7b0):
    /// a tower on a road of its own gets no sentry from it, and weapons made on
    /// another network go to a storage yard there instead of waiting on the
    /// recruiter. Joining the roads puts both within its reach.
    #[test]
    fn a_recruiter_serves_only_its_road_network() {
        use crate::world::{Command, Outcome};
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        let needed = w.workers_needed(RECRUITER);
        w.buildings.get_mut(recruiter).expect("recruiter").workers = needed;
        let o = w.apply(&Command::Road { start: (44, 62), end: (64, 62) });
        assert!(matches!(o, Outcome::Done { items: 1.., .. }), "{o:?}");
        let tower = w.create_building(crate::defenses::TOWER, 47, 63);
        let yard = w.create_building(crate::buildings::kind::STORAGE_YARD, 54, 63);
        for b in [tower, yard] {
            let needed = w.workers_needed(w.buildings.get(b).expect("built").kind);
            let b = w.buildings.get_mut(b).expect("built");
            assert!(b.road.is_some_and(|r| r.1 == 62), "{:?}", b.road);
            b.workers = needed;
        }
        assert_eq!(w.tower_wanting_sentry(recruiter), None);
        assert_eq!(w.cart_destination(WEAPONS, (62, 62)), Some(yard));
        assert!(matches!(w.apply(&Command::Road { start: (44, 63), end: (44, 69) }), Outcome::Done { items: 1.., .. }));
        assert_eq!(w.tower_wanting_sentry(recruiter), Some(tower));
        assert_eq!(w.cart_destination(WEAPONS, (62, 62)), Some(recruiter));
        // With no fort the recruiter takes none.
        let fort = w.military.companies[0].fort;
        w.demolish(fort);
        assert_eq!(w.cart_destination(WEAPONS, (62, 62)), Some(yard));
    }

    /// A fort built after one is lost takes the first company record free
    /// (FUN_004b6e10), and its standard answers to it.
    #[test]
    fn a_new_fort_takes_a_lost_forts_company() {
        use crate::world::{Command, Outcome};
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        assert!(matches!(w.apply(&Command::Build { kind: FORT_ARCHERS, x: 54, y: 75, x1: 54, y1: 75 }), Outcome::Done { items: 1.., .. }));
        let first = w.military.companies[0].fort;
        w.demolish(first);
        assert_eq!(w.military.companies[0].fort, 0);
        assert!(matches!(w.apply(&Command::Build { kind: FORT_CHARIOTEERS, x: 44, y: 75, x1: 44, y1: 75 }), Outcome::Done { items: 1.., .. }));
        assert_eq!(w.military.companies.len(), 2);
        let c = &w.military.companies[0];
        assert_eq!((c.kind, c.morale, c.soldiers.len()), (CHARIOTEER, 50, 0));
        assert_eq!(w.company_of(c.standard), Some(0));
        assert_eq!(w.military.companies[1].kind, ARCHER);
    }

    /// A won mission's best company of each kind rejoins the next mission that
    /// keeps them (FUN_004188f0, FUN_00418bf0): its first fort of that kind takes
    /// the company's experience, and the men march in to the parade ground.
    #[test]
    fn veterans_rejoin_the_next_mission() {
        use crate::world::{Command, Outcome};
        let Some(mut w) = barracks_of(FORT_INFANTRY, false) else { return };
        let recruiter = w.buildings.iter().find(|b| b.kind == RECRUITER).map(|b| b.id).expect("recruiter");
        w.add_stored(recruiter, WEAPONS, 16 * crate::economy::LOAD);
        muster(&mut w);
        w.military.companies[0].experience = 30;
        let carried = w.companies_to_carry();
        assert_eq!(carried, vec![CarriedCompany { kind: INFANTRY, men: 16, experience: 30 }]);

        // Mission 2 doesn't keep them; Qadesh (42) does.
        let Some(mut next) = crate::economy::tests::mission(2) else { return };
        next.invasions.planned.clear();
        assert_eq!(next.carry_in_companies(&carried), 0);
        next.mission.as_mut().expect("mission").id = 42;
        assert_eq!(next.carry_in_companies(&carried), 1);
        assert!(next.message_texts.back().is_some_and(|t| t.reason == "troopcarryover_inf_only"));
        // An archers' fort leaves them waiting; the infantry fort takes them.
        let fort = |kind| Command::Build { kind, x: 44, y: 75, x1: 44, y1: 75 };
        assert!(matches!(next.apply(&Command::Road { start: (40, 70), end: (66, 70) }), Outcome::Done { items: 1.., .. }));
        assert!(matches!(next.apply(&fort(FORT_ARCHERS)), Outcome::Done { items: 1.., .. }));
        assert_eq!(next.military.carried.len(), 1);
        let archers = next.military.companies[0].fort;
        next.demolish(archers);
        assert!(matches!(next.apply(&fort(FORT_INFANTRY)), Outcome::Done { items: 1.., .. }));
        assert!(next.military.carried.is_empty());
        let c = &next.military.companies[0];
        assert_eq!((c.kind, c.experience, c.recruits.len()), (INFANTRY, 30, 16));
        for _ in 0..60 * crate::time::TICKS_PER_DAY {
            next.tick();
            if next.military.companies[0].soldiers.len() == COMPANY_SIZE {
                break;
            }
        }
        assert_eq!(next.military.companies[0].soldiers.len(), COMPANY_SIZE, "{:?}", next.military.companies[0]);
    }
}
