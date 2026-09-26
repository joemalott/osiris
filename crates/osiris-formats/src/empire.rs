//! The empire map stored in scenarios: its objects (cities, labels, ornaments, route
//! markers), the trade route paths, and the trade price table.
//!
//! Each of the 200 object records is 62 bytes followed by the city's trade amounts:
//! three 32-bit masks (with two bytes of padding) in files before version 160, one tier
//! byte per resource afterwards. Every file of the original is older than 160; the
//! game turns the masks into tier bytes as it loads them (FUN_004d19e0), giving
//! resources 32-35, which no mask can hold, the middle tier.
//!
//! Writing puts the objects, routes and prices back into the chunks they came from
//! (`Empire::write_chunks`). Each record starts from the bytes the file held, so the
//! fields Osiris doesn't read stay as they were, and a field whose value hasn't changed
//! keeps its stored bytes exactly (a list of goods with a gap in it, a resource marked
//! in two masks), which is what lets an untouched map come back byte for byte.

use crate::Result;
use crate::bytes::Reader;
use crate::chunks::ChunkFile;

pub const MAX_OBJECTS: usize = 200;
pub const MAX_ROUTES: usize = 50;
/// Resource slots in per-resource tables (index 0 unused).
pub const RESOURCES: usize = 36;
/// Goods a city's record can list as sold and as bought.
pub const MAX_SELLS: usize = 14;
pub const MAX_BUYS: usize = 8;
/// Waypoints a route can have.
pub const MAX_POINTS: usize = 50;
const ROUTE_RECORD: usize = 324;

/// The price table the game starts from and the editor's Reset prices button puts
/// back (the exe's table at 0x5d2d68): (buy, sell) per load.
pub const DEFAULT_PRICES: [(i32, i32); RESOURCES] = [
    (0, 0),
    (28, 21),
    (47, 35),
    (33, 25),
    (33, 25),
    (33, 25),
    (33, 25),
    (42, 33),
    (44, 34),
    (21, 16),
    (325, 275),
    (38, 29),
    (150, 120),
    (140, 105),
    (48, 37),
    (185, 140),
    (54, 42),
    (210, 160),
    (120, 92),
    (310, 150),
    (225, 170),
    (0, 0),
    (31, 23),
    (200, 165),
    (38, 29),
    (46, 35),
    (60, 45),
    (0, 0),
    (375, 315),
    (240, 185),
    (40, 32),
    (110, 85),
    (40, 30),
    (155, 116),
    (250, 190),
    (72, 54),
];

/// Object kinds on the empire map.
pub mod object {
    /// A picture ("simple graphic" in the editor).
    pub const ORNAMENT: u8 = 0;
    pub const CITY: u8 = 1;
    /// A region's name (text group 196).
    pub const REGION: u8 = 2;
    pub const BATTLE_ICON: u8 = 3;
    pub const LAND_TRADE_ROUTE: u8 = 4;
    pub const SEA_TRADE_ROUTE: u8 = 5;
}

/// City kinds: ours, the Pharaoh's, other Egyptian and foreign cities, each in a
/// trading and a non-trading form.
pub mod city {
    pub const OURS: u8 = 0;
    pub const PHARAOH_TRADING: u8 = 1;
    pub const PHARAOH: u8 = 2;
    pub const EGYPTIAN_TRADING: u8 = 3;
    pub const EGYPTIAN: u8 = 4;
    pub const FOREIGN_TRADING: u8 = 5;
    pub const FOREIGN: u8 = 6;

    pub fn trades(t: u8) -> bool {
        matches!(t, PHARAOH_TRADING | EGYPTIAN_TRADING | FOREIGN_TRADING)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmpireObject {
    pub kind: u8,
    pub in_use: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub image_id: u16,
    pub expanded_image_id: u16,
    /// Where the city's name sits on the empire map: 0 left of the icon, 1 above,
    /// 2 right, 3 below.
    pub text_align: u8,
    pub city_type: u8,
    /// The city's name (text group 195), or a region's (group 196).
    pub city_name_id: u8,
    pub trade_route_id: u8,
    pub trade_route_open: bool,
    pub trade_route_cost: u16,
    /// Resources the city sells (up to 14 ids, 0 = none).
    pub sells: Vec<u8>,
    /// Resources the city buys (up to 8 ids).
    pub buys: Vec<u8>,
    /// A battle marker's path and order (the editor's "Path" and "Order").
    pub invasion_path: u8,
    pub invasion_years: u8,
    /// Trade amount tier per resource: 0 none, 1 = 1500, 2 = 2500, 3 = 4000 a year.
    pub demand: Vec<u8>,
    /// The record as the file held it (empty for an object made since), which
    /// writing starts from.
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct EmpireRoute {
    pub in_use: bool,
    /// 1 land, 2 sea (anything else is the editor's "General route").
    pub route_type: u8,
    /// Pixels between the dots drawn along the route (the original takes 5 when the
    /// file's value is 0 or over 50).
    pub step: u8,
    pub points: Vec<(i32, i32)>,
    /// The objects (cities) the route starts and ends in, -1 for none: worked out by
    /// the editor from where its first and last points lie.
    pub from_object: i16,
    pub to_object: i16,
    /// The record as the file held it (empty for a route made since).
    pub raw: Vec<u8>,
}

impl EmpireRoute {
    /// The step the original uses for a stored step: 5 for 0 or over 50.
    pub fn spacing(&self) -> u8 {
        if (1..=50).contains(&self.step) { self.step } else { 5 }
    }

    /// The dots along the route (FUN_00445d10, as the editor builds them): from each
    /// waypoint to the next, one every `step` pixels, the first at the waypoint.
    pub fn dots(&self) -> Vec<(i32, i32)> {
        let step = self.spacing() as f32;
        let mut dots = Vec::new();
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (dx, dy) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
            let len = (dx * dx + dy * dy).sqrt();
            if len == 0.0 {
                continue;
            }
            let mut d = 0.0;
            while d <= len {
                dots.push(((a.0 as f32 + dx * d / len) as i32, (a.1 as f32 + dy * d / len) as i32));
                d += step;
            }
        }
        dots
    }

    /// The route's length as the editor shows it ("Route length"): its dots times
    /// the step between them.
    pub fn length(&self) -> i32 {
        self.dots().len() as i32 * self.spacing() as i32
    }
}

#[derive(Debug, Clone, Default)]
pub struct Empire {
    pub objects: Vec<EmpireObject>,
    pub routes: Vec<EmpireRoute>,
    /// (buy, sell) price of a load of each resource; all zero when the file has none.
    pub prices: Vec<(i32, i32)>,
}

impl Empire {
    pub fn from_chunks(file: &ChunkFile) -> Result<Self> {
        let mut e = Empire::default();
        if let Some(data) = file.get("empire_map_objects") {
            let record = data.len() / MAX_OBJECTS;
            for i in 0..MAX_OBJECTS {
                e.objects.push(parse_object(&data[i * record..(i + 1) * record], file.version)?);
            }
        }
        if let Some(data) = file.get("empire_map_routes") {
            let record = data.len() / MAX_ROUTES;
            for i in 0..MAX_ROUTES {
                e.routes.push(parse_route(&data[i * record..(i + 1) * record])?);
            }
        }
        if let Some(data) = file.get("trade_prices") {
            let mut r = Reader::new(data, "trade_prices");
            for _ in 0..RESOURCES {
                e.prices.push((r.i32()?, r.i32()?));
            }
        }
        Ok(e)
    }

    /// The objects of the empire the editor's Reset button brings back: the 200
    /// records at 0x500 of `Pharaoh2.emp`, in the original's own format (FUN_00444270).
    /// The routes stored in that file are not valid ones, so a reset clears them.
    pub fn default_objects(emp: &[u8]) -> Result<Vec<EmpireObject>> {
        const START: usize = 0x500;
        const RECORD: usize = 76;
        let data = emp.get(START..START + RECORD * MAX_OBJECTS).ok_or_else(|| crate::Error::Invalid("Pharaoh2.emp is too short".into()))?;
        (0..MAX_OBJECTS).map(|i| parse_object(&data[i * RECORD..(i + 1) * RECORD], 149)).collect()
    }

    /// The route id's path, if it is in use.
    pub fn route(&self, id: u8) -> Option<&EmpireRoute> {
        self.routes.get(id as usize).filter(|r| r.in_use)
    }

    /// Writes the objects, routes and prices into `file`'s empire chunks, the mirror
    /// of `from_chunks` (only chunks the file has are written).
    pub fn write_chunks(&self, file: &mut ChunkFile) -> Result<()> {
        let version = file.version;
        if let Some(data) = file.get_mut("empire_map_objects") {
            let record = data.len() / MAX_OBJECTS;
            for i in 0..MAX_OBJECTS {
                let blank = EmpireObject::default();
                let o = self.objects.get(i).unwrap_or(&blank);
                write_object(o, &mut data[i * record..(i + 1) * record], version);
            }
        }
        if let Some(data) = file.get_mut("empire_map_routes") {
            let record = data.len() / MAX_ROUTES;
            for i in 0..MAX_ROUTES {
                let blank = EmpireRoute::default();
                let r = self.routes.get(i).unwrap_or(&blank);
                write_route(r, &mut data[i * record..(i + 1) * record]);
            }
        }
        if !self.prices.is_empty()
            && let Some(data) = file.get_mut("trade_prices")
        {
            for (r, chunk) in data.chunks_exact_mut(8).enumerate() {
                let (buy, sell) = self.prices.get(r).copied().unwrap_or((0, 0));
                chunk[..4].copy_from_slice(&buy.to_le_bytes());
                chunk[4..].copy_from_slice(&sell.to_le_bytes());
            }
        }
        Ok(())
    }
}

/// A list of up to `n` resource ids as stored: the ids, then zeros.
fn list_bytes(v: &[u8], n: usize) -> Vec<u8> {
    let mut out: Vec<u8> = v.iter().copied().filter(|&r| r != 0).take(n).collect();
    out.resize(n, 0);
    out
}

fn nonzero(b: &[u8]) -> Vec<u8> {
    b.iter().copied().filter(|&v| v != 0).collect()
}

/// Tiers from the three masks of files before version 160 (FUN_004d19e0): a
/// resource in the 40-load mask is tier 3, else in the 25-load one tier 2, else in
/// the 15-load one tier 1; resources 32-35 are past the masks and get tier 2.
fn tiers_from_masks(t40: u32, t25: u32, t15: u32) -> Vec<u8> {
    let mut d = vec![0u8; RESOURCES];
    for (res, t) in d.iter_mut().enumerate() {
        if res >= 32 {
            *t = 2;
            continue;
        }
        let bit = 1u32 << res;
        *t = if t40 & bit != 0 {
            3
        } else if t25 & bit != 0 {
            2
        } else if t15 & bit != 0 {
            1
        } else {
            0
        };
    }
    d
}

fn parse_object(data: &[u8], version: i32) -> Result<EmpireObject> {
    let mut r = Reader::new(data, "empire object");
    let mut o = EmpireObject { kind: r.u8()?, in_use: r.u8()? != 0, demand: vec![0; RESOURCES], raw: data.to_vec(), ..Default::default() };
    r.skip(2)?;
    o.x = r.u16()? as i32;
    o.y = r.u16()? as i32;
    o.width = r.u16()? as i32;
    o.height = r.u16()? as i32;
    o.image_id = r.u16()?;
    o.expanded_image_id = r.u16()?;
    r.skip(3)?;
    o.text_align = r.u8()?;
    r.skip(4)?;
    o.city_type = r.u8()?;
    o.city_name_id = r.u8()?;
    o.trade_route_id = r.u8()?;
    o.trade_route_open = r.u8()? != 0;
    o.trade_route_cost = r.u16()?;
    o.sells = nonzero(r.bytes(MAX_SELLS)?);
    r.skip(8)?;
    o.buys = nonzero(r.bytes(MAX_BUYS)?);
    o.invasion_path = r.u8()?;
    o.invasion_years = r.u8()?;
    if version < 160 {
        r.skip(2)?;
        let (t40, t25, t15) = (r.u32()?, r.u32()?, r.u32()?);
        o.demand = tiers_from_masks(t40, t25, t15);
    } else {
        o.demand = r.bytes(RESOURCES)?.to_vec();
    }
    Ok(o)
}

/// Writes `o` over `rec`, one record of the objects chunk. Fields are written only
/// where the stored bytes don't already read as the object's value.
fn write_object(o: &EmpireObject, rec: &mut [u8], version: i32) {
    let old = if o.raw.len() == rec.len() { o.raw.clone() } else { vec![0; rec.len()] };
    rec.copy_from_slice(&old);
    let put16 = |rec: &mut [u8], at: usize, v: u16| rec[at..at + 2].copy_from_slice(&v.to_le_bytes());
    rec[0] = o.kind;
    // Placed objects are stored as 2 (1 is the editor's object still following the
    // mouse, FUN_00407620); a stored non-zero value is kept.
    rec[1] = match (o.in_use, old[1]) {
        (false, _) => 0,
        (true, 0) => 2,
        (true, v) => v,
    };
    put16(rec, 4, o.x as u16);
    put16(rec, 6, o.y as u16);
    put16(rec, 8, o.width as u16);
    put16(rec, 10, o.height as u16);
    put16(rec, 12, o.image_id);
    put16(rec, 14, o.expanded_image_id);
    rec[19] = o.text_align;
    rec[24] = o.city_type;
    rec[25] = o.city_name_id;
    rec[26] = o.trade_route_id;
    if (old[27] != 0) != o.trade_route_open {
        rec[27] = o.trade_route_open as u8;
    }
    put16(rec, 28, o.trade_route_cost);
    if nonzero(&old[30..44]) != nonzero(&list_bytes(&o.sells, MAX_SELLS)) {
        rec[30..44].copy_from_slice(&list_bytes(&o.sells, MAX_SELLS));
    }
    if nonzero(&old[52..60]) != nonzero(&list_bytes(&o.buys, MAX_BUYS)) {
        rec[52..60].copy_from_slice(&list_bytes(&o.buys, MAX_BUYS));
    }
    rec[60] = o.invasion_path;
    rec[61] = o.invasion_years;
    let mut demand = o.demand.clone();
    demand.resize(RESOURCES, 0);
    if version < 160 {
        let mask = |at: usize| u32::from_le_bytes(old[at..at + 4].try_into().expect("4 bytes"));
        let stored = tiers_from_masks(mask(64), mask(68), mask(72));
        if stored[..32] != demand[..32] {
            let (mut t40, mut t25, mut t15) = (0u32, 0u32, 0u32);
            for (res, &t) in demand.iter().enumerate().take(32) {
                let bit = 1u32 << res;
                match t {
                    3 => t40 |= bit,
                    2 => t25 |= bit,
                    1 => t15 |= bit,
                    _ => {}
                }
            }
            rec[64..68].copy_from_slice(&t40.to_le_bytes());
            rec[68..72].copy_from_slice(&t25.to_le_bytes());
            rec[72..76].copy_from_slice(&t15.to_le_bytes());
        }
    } else {
        rec[62..62 + RESOURCES].copy_from_slice(&demand);
    }
}

fn parse_route(data: &[u8]) -> Result<EmpireRoute> {
    let mut r = Reader::new(data, "empire route");
    let step = r.u8()?;
    r.skip(7)?;
    let mut points = Vec::with_capacity(MAX_POINTS);
    for _ in 0..MAX_POINTS {
        let (x, y) = (r.u16()? as i32, r.u16()? as i32);
        let used = r.u8()? != 0;
        r.skip(1)?;
        points.push((x, y, used));
    }
    r.skip(8)?;
    let from_object = r.i16()?;
    let to_object = r.i16()?;
    let route_type = r.u8()?;
    let num_points = r.u8()? as usize;
    let in_use = r.u8()? != 0;
    Ok(EmpireRoute {
        in_use,
        route_type,
        step,
        points: points.into_iter().take(num_points).map(|(x, y, _)| (x, y)).collect(),
        from_object,
        to_object,
        raw: data.to_vec(),
    })
}

/// Writes `route` over `rec`, one record of the routes chunk (324 bytes): the step,
/// the waypoints, the count of dots the game builds along them, the objects at its
/// ends, the type, the waypoint count and whether it is in use. The four bytes after
/// the step and the four after the dot count are where the game keeps the dots while
/// it runs, and must be 0 in a file for the game to take the route (FUN_00444270).
fn write_route(route: &EmpireRoute, rec: &mut [u8]) {
    let old = if route.raw.len() == rec.len() { route.raw.clone() } else { vec![0; rec.len()] };
    rec.copy_from_slice(&old);
    let fresh = route.raw.len() != ROUTE_RECORD;
    let stored = parse_route(&old).ok();
    let same_path = stored.as_ref().is_some_and(|s| s.points == route.points && s.step == route.step);
    rec[0] = route.step;
    if fresh {
        rec[1..8].fill(0);
    }
    let n = route.points.len().min(MAX_POINTS);
    for i in 0..MAX_POINTS {
        let at = 8 + 6 * i;
        match route.points.get(i).filter(|_| i < n) {
            Some(&(x, y)) => {
                rec[at..at + 2].copy_from_slice(&(x as u16).to_le_bytes());
                rec[at + 2..at + 4].copy_from_slice(&(y as u16).to_le_bytes());
                if !same_path {
                    rec[at + 4] = 1;
                }
            }
            None if !same_path => rec[at..at + 6].fill(0),
            None => {}
        }
    }
    if !same_path {
        let dots = if route.points.len() >= 2 { route.dots().len() as u32 } else { 0 };
        rec[308..312].copy_from_slice(&dots.to_le_bytes());
        rec[312..316].fill(0);
    }
    rec[316..318].copy_from_slice(&route.from_object.to_le_bytes());
    rec[318..320].copy_from_slice(&route.to_object.to_le_bytes());
    rec[320] = route.route_type;
    rec[321] = n as u8;
    if (old[322] != 0) != route.in_use {
        rec[322] = route.in_use as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_record_sizes() {
        // 62 fixed bytes plus the trade amounts.
        assert_eq!(15200 / MAX_OBJECTS, 76);
        assert_eq!(19600 / MAX_OBJECTS, 98);
        assert_eq!(16200 / MAX_ROUTES, ROUTE_RECORD);
    }

    /// A record rewritten unchanged is the same bytes, and changed fields read back.
    #[test]
    fn object_writes_back() {
        let mut raw = vec![0u8; 76];
        raw[0] = object::CITY;
        raw[1] = 2;
        raw[24] = city::FOREIGN;
        raw[30] = 5;
        raw[32] = 9; // a gap at 31, kept while the list is unchanged
        raw[64..68].copy_from_slice(&(1u32 << 5).to_le_bytes());
        raw[68..72].copy_from_slice(&((1u32 << 5) | (1 << 9)).to_le_bytes());
        let o = parse_object(&raw, 149).unwrap();
        assert_eq!(o.sells, vec![5, 9]);
        assert_eq!((o.demand[5], o.demand[9], o.demand[33]), (3, 2, 2));
        let mut rec = vec![0u8; 76];
        write_object(&o, &mut rec, 149);
        assert_eq!(rec, raw);

        let mut o2 = o.clone();
        o2.sells = vec![5, 9, 20];
        o2.buys = vec![1];
        o2.demand[20] = 1;
        o2.city_type = city::FOREIGN_TRADING;
        o2.trade_route_id = 3;
        o2.trade_route_cost = 1200;
        write_object(&o2, &mut rec, 149);
        let back = parse_object(&rec, 149).unwrap();
        assert_eq!((back.sells, back.buys, back.city_type, back.trade_route_id, back.trade_route_cost), (vec![5, 9, 20], vec![1], city::FOREIGN_TRADING, 3, 1200));
        assert_eq!((back.demand[5], back.demand[9], back.demand[20]), (3, 2, 1));
    }

    #[test]
    fn route_writes_back() {
        let r = EmpireRoute { in_use: true, route_type: 2, step: 5, points: vec![(10, 10), (40, 50), (100, 50)], from_object: 3, to_object: -1, raw: Vec::new() };
        let mut rec = vec![0xaa; ROUTE_RECORD];
        write_route(&r, &mut rec);
        let back = parse_route(&rec).unwrap();
        assert_eq!((back.in_use, back.route_type, back.step, back.points.clone(), back.from_object, back.to_object), (true, 2, 5, r.points.clone(), 3, -1));
        assert_eq!(&rec[4..8], &[0, 0, 0, 0], "the dots' pointer must be 0 in a file");
        assert_eq!(u32::from_le_bytes(rec[308..312].try_into().unwrap()) as usize, r.dots().len());
        // 50 px (11 dots from 0 to 50) then 60 px (13 dots).
        assert_eq!(r.dots().len(), 11 + 13);
        let mut again = rec.clone();
        write_route(&back, &mut again);
        assert_eq!(again, rec);
    }
}
