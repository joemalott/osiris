//! `Pharaoh_Model_{VeryEasy,Easy,Normal,Hard,Impossible}.txt` (building and house
//! stats) and `Figure_model*.txt` (unit stats), one difficulty setting per file.
//!
//! These are plain text, edited by hand and meant to be re-compiled ("hardcoded
//! back into the compiled code" per the file's own header), so parsing is line by
//! line and defensive rather than a strict grammar:
//!
//! - Buildings: `ALL BUILDINGS` introduces lines like
//!   `46,Apothecary,{,30,1,1,-1,1,5,20,0,0,25,},,,,...` -- id, name, then the
//!   numbers between `{` and `}` (cost, desirability value, step, step size, range,
//!   employees, fire risk, damage risk, and sometimes a couple more). Trailing
//!   commas after `}` are column padding for the original text editor and ignored.
//! - Houses: `ALL HOUSES` introduces lines like
//!   `House 1 - Small Hut,{,-99,-10,0,...,4,1.25,,,,,,` -- name, then 22 documented
//!   fields (see [`HouseModel`]), then a handful of undocumented trailing fields
//!   (some numeric, some text like `"2 by 2"`, and some blank). These lines have no
//!   closing `}`, so everything after `{` to end of line is kept.
//! - Figures (a separate file): lines like `1,Immigrant,{,1,10,0,...,0,},...` for
//!   ordinary figures, or `0,Hyksos Swordsmen,Infantry,{,3,150,...,}` for the
//!   `ALL ENEMIES` military units, which have an extra category field before `{`
//!   and whose ids restart per nation (not globally unique).

use crate::{Error, Result};

/// One row from the `ALL BUILDINGS` section.
#[derive(Debug, Clone, Default)]
pub struct BuildingModel {
    pub id: u32,
    pub name: String,
    pub cost: f64,
    pub desirability_value: f64,
    pub desirability_step: f64,
    pub desirability_step_size: f64,
    pub desirability_range: f64,
    pub employees: f64,
    pub fire_risk: f64,
    pub damage_risk: f64,
    /// Every number between `{` and `}`, in file order (the eight fields above are
    /// `values[0..8]`; some buildings have a couple more we haven't named).
    pub values: Vec<f64>,
}

/// One row from the `ALL HOUSES` section. Field order/meaning per the file's own
/// header comment.
#[derive(Debug, Clone, Default)]
pub struct HouseModel {
    /// House tier, parsed from the leading number in `name` (e.g. 20 for
    /// `"House 20 - Palatial Estate (x16)"`).
    pub level: u32,
    pub name: String,
    pub devolve_desirability: f64,
    pub evolve_desirability: f64,
    pub entertainment: f64,
    pub water: f64,
    pub religion: f64,
    pub education: f64,
    pub market: f64,
    pub dentist: f64,
    pub physician: f64,
    pub health: f64,
    pub food: f64,
    pub pottery: f64,
    pub linen: f64,
    pub jewelry: f64,
    pub beer: f64,
    pub crime_increment: f64,
    pub crime_base: f64,
    pub prosperity: f64,
    pub capacity: f64,
    pub tax_multiplier: f64,
    pub malaria_increment: f64,
    pub disease_increment: f64,
    /// Every token between `{` and end of line, trimmed, in file order: the 22
    /// fields above as `raw[0..22]`, then undocumented trailing fields (numeric
    /// strings, footprint text like `"2 by 2"`, or blanks).
    pub raw: Vec<String>,
}

/// One row from a `Figure_model*.txt` file.
#[derive(Debug, Clone, Default)]
pub struct FigureModel {
    pub id: u32,
    pub name: String,
    /// Present only for the `ALL ENEMIES` military entries (e.g. `"Infantry"`,
    /// `"Warship"`); absent for ordinary figures.
    pub category: Option<String>,
    pub kind: f64,
    pub hit_points: f64,
    pub attack: f64,
    pub armor: f64,
    pub armor_vs_missiles: f64,
    pub missile_attack: f64,
    pub missile_range: f64,
    pub missile_rate_of_fire: f64,
    pub speed: f64,
    pub frequency: f64,
    /// Every number between `{` and `}`, in file order (the ten fields above are
    /// `values[0..10]`; the file's header calls the remaining four "something").
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Buildings,
    Houses,
}

/// A parsed `Pharaoh_Model_*.txt`: buildings and houses for one difficulty.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub buildings: Vec<BuildingModel>,
    pub houses: Vec<HouseModel>,
}

impl Model {
    pub fn parse(text: &str) -> Result<Self> {
        let mut section = Section::None;
        let mut buildings = Vec::new();
        let mut houses = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            match trimmed {
                "ALL BUILDINGS" => {
                    section = Section::Buildings;
                    continue;
                }
                "ALL HOUSES" => {
                    section = Section::Houses;
                    continue;
                }
                _ => {}
            }
            match section {
                Section::Buildings => {
                    if let Some(b) = parse_building_line(trimmed)? {
                        buildings.push(b);
                    }
                }
                Section::Houses => {
                    if let Some(h) = parse_house_line(trimmed) {
                        houses.push(h);
                    }
                }
                Section::None => {}
            }
        }
        Ok(Self { buildings, houses })
    }
}

/// Parses a `Figure_model*.txt` file into its rows, in file order.
pub fn parse_figures(text: &str) -> Result<Vec<FigureModel>> {
    let mut figures = Vec::new();
    for line in text.lines() {
        if let Some(f) = parse_figure_line(line.trim())? {
            figures.push(f);
        }
    }
    Ok(figures)
}

fn parse_number(context: &str, tok: &str) -> Result<f64> {
    tok.trim()
        .parse::<f64>()
        .map_err(|_| Error::Invalid(format!("{context}: bad number {tok:?}")))
}

fn parse_building_line(line: &str) -> Result<Option<BuildingModel>> {
    if line.is_empty() {
        return Ok(None);
    }
    let tokens: Vec<&str> = line.split(',').collect();
    let Some(id) = tokens.first().and_then(|t| t.trim().parse::<u32>().ok()) else {
        return Ok(None);
    };
    let Some(&name) = tokens.get(1) else {
        return Ok(None);
    };
    if tokens.get(2) != Some(&"{") {
        return Ok(None);
    }
    let mut values = Vec::new();
    for tok in &tokens[3..] {
        let tok = tok.trim();
        if tok == "}" {
            break;
        }
        values.push(parse_number(line, tok)?);
    }
    let get = |i: usize| values.get(i).copied().unwrap_or(0.0);
    Ok(Some(BuildingModel {
        id,
        name: name.to_string(),
        cost: get(0),
        desirability_value: get(1),
        desirability_step: get(2),
        desirability_step_size: get(3),
        desirability_range: get(4),
        employees: get(5),
        fire_risk: get(6),
        damage_risk: get(7),
        values,
    }))
}

fn parse_house_line(line: &str) -> Option<HouseModel> {
    if !line.starts_with("House ") {
        return None;
    }
    let tokens: Vec<&str> = line.split(',').collect();
    let name = (*tokens.first()?).to_string();
    if tokens.get(1) != Some(&"{") {
        return None;
    }
    let mut raw: Vec<String> = tokens[2..].iter().map(|t| t.trim().to_string()).collect();
    // Most house lines have no closing `}` (padding commas run to end of line), but
    // tolerate one if present.
    if let Some(pos) = raw.iter().position(|t| t == "}") {
        raw.truncate(pos);
    }
    while raw.last().is_some_and(String::is_empty) {
        raw.pop();
    }
    let num = |i: usize| {
        raw.get(i)
            .map(|s| s.as_str())
            .unwrap_or("0")
            .parse::<f64>()
            .unwrap_or(0.0)
    };
    let level = name
        .strip_prefix("House ")
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or(0);
    Some(HouseModel {
        level,
        name,
        devolve_desirability: num(0),
        evolve_desirability: num(1),
        entertainment: num(2),
        water: num(3),
        religion: num(4),
        education: num(5),
        market: num(6),
        dentist: num(7),
        physician: num(8),
        health: num(9),
        food: num(10),
        pottery: num(11),
        linen: num(12),
        jewelry: num(13),
        beer: num(14),
        crime_increment: num(15),
        crime_base: num(16),
        prosperity: num(17),
        capacity: num(18),
        tax_multiplier: num(19),
        malaria_increment: num(20),
        disease_increment: num(21),
        raw,
    })
}

fn parse_figure_line(line: &str) -> Result<Option<FigureModel>> {
    if line.is_empty() {
        return Ok(None);
    }
    let tokens: Vec<&str> = line.split(',').collect();
    let Some(id) = tokens.first().and_then(|t| t.trim().parse::<u32>().ok()) else {
        return Ok(None);
    };
    let Some(brace) = tokens.iter().position(|&t| t == "{") else {
        return Ok(None);
    };
    if brace < 2 {
        return Ok(None);
    }
    let name = tokens[1].to_string();
    let category = (brace >= 3).then(|| tokens[2].to_string());
    let mut values = Vec::new();
    for tok in &tokens[brace + 1..] {
        let tok = tok.trim();
        if tok == "}" {
            break;
        }
        values.push(parse_number(line, tok)?);
    }
    let get = |i: usize| values.get(i).copied().unwrap_or(0.0);
    Ok(Some(FigureModel {
        id,
        name,
        category,
        kind: get(0),
        hit_points: get(1),
        attack: get(2),
        armor: get(3),
        armor_vs_missiles: get(4),
        missile_attack: get(5),
        missile_range: get(6),
        missile_rate_of_fire: get(7),
        speed: get(8),
        frequency: get(9),
        values,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
ALL BUILDINGS

	    a   b   c   d   e   f   g   h
	   CST DES STP SZE RGE EMP FRI DRI

0,Nothing,{,0,0,0,0,0,0,0,0,0,0,},,,,,,,,,,
46,Apothecary,{,30,1,1,-1,1,5,20,0,0,25,},,,,,,,,,,

ALL HOUSES
House 1 - Small Hut,{,-99,-10,0,0,0,0,0,0,0,0,0,0,0,0,0,30,25,5,5,1,50,40,2.5,,4,1.25,,,,,,
House 20 - Palatial Estate (x16),{,85,100,90,2,3,2,1,1,1,2,3,1,1,2,1,3,5,1900,200,16,-120,-120,1600,4 by 4,16,118.75,,,,,,

End of model data.
";

    #[test]
    fn parses_buildings_and_houses() {
        let m = Model::parse(SAMPLE).unwrap();
        assert_eq!(m.buildings.len(), 2);
        let apothecary = m.buildings.iter().find(|b| b.id == 46).unwrap();
        assert_eq!(apothecary.name, "Apothecary");
        assert_eq!(apothecary.cost, 30.0);
        assert_eq!(apothecary.employees, 5.0);

        assert_eq!(m.houses.len(), 2);
        let house20 = &m.houses[1];
        assert_eq!(house20.level, 20);
        assert_eq!(house20.capacity, 200.0);
        assert_eq!(house20.raw[23], "4 by 4");
    }

    #[test]
    fn parses_figures_with_and_without_category() {
        let text = "\
1,Immigrant,{,1,10,0,0,0,0,0,0,6,0,0,0,0,0,},,0,0
0,Hyksos Swordsmen,Infantry,{,3,150,15,6,2,0,0,0,6,60,0,0,0,0,}
";
        let figures = parse_figures(text).unwrap();
        assert_eq!(figures.len(), 2);
        assert_eq!(figures[0].name, "Immigrant");
        assert_eq!(figures[0].category, None);
        assert_eq!(figures[0].hit_points, 10.0);
        assert_eq!(figures[1].name, "Hyksos Swordsmen");
        assert_eq!(figures[1].category.as_deref(), Some("Infantry"));
        assert_eq!(figures[1].hit_points, 150.0);
    }
}
