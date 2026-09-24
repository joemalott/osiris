//! The empire map stored in scenarios: its objects (cities, labels, ornaments, route
//! markers), the trade route paths, and the trade price table.
//!
//! Each of the 200 object records is 62 bytes followed by the city's trade amounts:
//! three 32-bit masks (with two bytes of padding) in files before version 160, one tier
//! byte per resource afterwards.

use crate::Result;
use crate::bytes::Reader;
use crate::chunks::ChunkFile;

pub const MAX_OBJECTS: usize = 200;
pub const MAX_ROUTES: usize = 50;
/// Resource slots in per-resource tables (index 0 unused).
pub const RESOURCES: usize = 36;

/// Object kinds on the empire map.
pub mod object {
    pub const CITY: u8 = 1;
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
    pub city_name_id: u8,
    pub trade_route_id: u8,
    pub trade_route_open: bool,
    pub trade_route_cost: u16,
    /// Resources the city sells (up to 14 ids, 0 = none).
    pub sells: Vec<u8>,
    /// Resources the city buys (up to 8 ids).
    pub buys: Vec<u8>,
    /// Trade amount tier per resource: 0 none, 1 = 1500, 2 = 2500, 3 = 4000 a year.
    pub demand: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct EmpireRoute {
    pub in_use: bool,
    /// 1 land, 2 sea.
    pub route_type: u8,
    /// Pixels between the dots drawn along the route (the original takes 5 when the
    /// file's value is 0 or over 50).
    pub step: u8,
    pub points: Vec<(i32, i32)>,
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

    /// The route id's path, if it is in use.
    pub fn route(&self, id: u8) -> Option<&EmpireRoute> {
        self.routes.get(id as usize).filter(|r| r.in_use)
    }
}

fn parse_object(data: &[u8], version: i32) -> Result<EmpireObject> {
    let mut r = Reader::new(data, "empire object");
    let mut o = EmpireObject { kind: r.u8()?, in_use: r.u8()? != 0, demand: vec![0; RESOURCES], ..Default::default() };
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
    o.sells = r.bytes(14)?.iter().copied().filter(|&v| v != 0).collect();
    r.skip(8)?;
    o.buys = r.bytes(8)?.iter().copied().filter(|&v| v != 0).collect();
    r.skip(2)?;
    if version < 160 {
        r.skip(2)?;
        let (t40, t25, t15) = (r.u32()?, r.u32()?, r.u32()?);
        for (res, d) in o.demand.iter_mut().enumerate().take(32) {
            let bit = 1u32 << res;
            *d = if t40 & bit != 0 {
                3
            } else if t25 & bit != 0 {
                2
            } else if t15 & bit != 0 {
                1
            } else {
                0
            };
        }
    } else {
        o.demand = r.bytes(RESOURCES)?.to_vec();
    }
    Ok(o)
}

fn parse_route(data: &[u8]) -> Result<EmpireRoute> {
    let mut r = Reader::new(data, "empire route");
    let step = r.u8()?;
    r.skip(7)?;
    let mut points = Vec::with_capacity(50);
    for _ in 0..50 {
        let (x, y) = (r.u16()? as i32, r.u16()? as i32);
        let used = r.u8()? != 0;
        r.skip(1)?;
        points.push((x, y, used));
    }
    r.skip(12)?;
    let route_type = r.u8()?;
    let num_points = r.u8()? as usize;
    let in_use = r.u8()? != 0;
    Ok(EmpireRoute {
        in_use,
        route_type,
        step,
        points: points.into_iter().take(num_points).map(|(x, y, _)| (x, y)).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_record_sizes() {
        // 62 fixed bytes plus the trade amounts.
        assert_eq!(15200 / MAX_OBJECTS, 76);
        assert_eq!(19600 / MAX_OBJECTS, 98);
        assert_eq!(16200 / MAX_ROUTES, 324);
    }
}
