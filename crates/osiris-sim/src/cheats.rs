//! The original's cheat codes: typed into the cheat box the app opens with
//! Ctrl+Alt+C, exactly as spelled (case sensitive) in the 1999 release. See
//! `notes/cheats.md` for the source list, what each does, and the one Osiris
//! addition at the end of the match below, which isn't in the original.
//!
//! `apply` runs one code and returns what happened; the world has no text of its
//! own for "wrong god" or "unknown cheat", so the caller (the app) turns that
//! outcome into whatever it shows the player.

use crate::buildings::kind;
use crate::invasions::invader;
use crate::military::Company;
use crate::religion::{self, BAST, OSIRIS, PTAH, RA, SETH};
use crate::world::World;

/// What running a typed code did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Applied; the original's own message, if it has one for this, is already
    /// posted to the notice log.
    Applied,
    /// Recognized, but this code needs the given god worshipped first, as in the
    /// original (an index into `religion::NAMES`).
    NeedsGod(usize),
    /// Recognized, but Osiris doesn't model this one (see notes/cheats.md).
    NotModeled,
    /// Not in the original; carrying it out needs state the world doesn't have
    /// (the saved campaign progress), so the app must handle it itself.
    NeedsApp,
    /// Not a recognized code.
    Unknown,
}

fn missing(world: &World, g: usize) -> bool {
    world.religion.gods.get(g).is_none_or(|god| god.status == religion::status::UNKNOWN)
}

/// Runs cheat `code` against `world`. See the module docs and notes/cheats.md.
pub fn apply(world: &mut World, code: &str) -> Outcome {
    match code {
        "Treasure Chest" => {
            if world.treasury < 15_000 {
                world.treasury += 1000;
            }
            Outcome::Applied
        }
        "Pharaohs Tomb" => {
            world.win_now();
            Outcome::Applied
        }
        "Fury of Seth" => {
            world.fury_of_seth();
            Outcome::Applied
        }
        // The original's exact troop counts aren't recoverable from the available
        // decompile (see notes/cheats.md); 16 is a plausible small raiding party,
        // chosen only so the cheat does something. Point 1 is the first land point,
        // 9 the first sea point (see `World::invade_now`).
        "mockattack1" => {
            world.invade_now(invader::ENEMY, 16, 1);
            Outcome::Applied
        }
        "mockattack2" => {
            world.invade_now(invader::ENEMY, 16, 9);
            Outcome::Applied
        }
        "Bounty" if missing(world, OSIRIS) => Outcome::NeedsGod(OSIRIS),
        "Bounty" => {
            world.adjust_next_flood_quality(40);
            world.post("message_blessing_inundation_from_osiris", None, true);
            Outcome::Applied
        }
        "Mummys Curse" if missing(world, OSIRIS) => Outcome::NeedsGod(OSIRIS),
        "Mummys Curse" => {
            world.adjust_next_flood_quality(-40);
            world.post("message_wrath_of_osiris", None, true);
            Outcome::Applied
        }
        // Fan sources disagree on the case of the middle word ("from"/"From"); both
        // are accepted rather than betting on either.
        "Life from Death" | "Life From Death" if missing(world, OSIRIS) => Outcome::NeedsGod(OSIRIS),
        "Life from Death" | "Life From Death" => {
            world.religion.osiris_double_harvest = true;
            world.post("message_blessing_from_osiris", None, true);
            Outcome::Applied
        }
        "Underworld" if missing(world, OSIRIS) => Outcome::NeedsGod(OSIRIS),
        "Underworld" => {
            world.religion.osiris_flood_destroys = 1;
            world.post("message_osiris_is_upset", None, true);
            Outcome::Applied
        }
        // Cleopatra: the locust swarm is Osiris's other major curse, alongside
        // Mummys Curse's flood-quality drop above.
        "Crop Busters" if missing(world, OSIRIS) => Outcome::NeedsGod(OSIRIS),
        "Crop Busters" => {
            world.religion.osiris_locusts = true;
            world.post("message_wrath_of_osiris_2", None, true);
            Outcome::Applied
        }
        "Sun Disk" if missing(world, RA) => Outcome::NeedsGod(RA),
        "Sun Disk" => {
            world.ratings.change_kingdom(15);
            world.post("message_blessing_reputation_from_ra", None, true);
            Outcome::Applied
        }
        "Mesektet" if missing(world, RA) => Outcome::NeedsGod(RA),
        "Mesektet" => {
            world.ratings.change_kingdom(-15);
            world.post("message_wrath_of_ra", None, true);
            Outcome::Applied
        }
        "Pharaohs Glory" if missing(world, RA) => Outcome::NeedsGod(RA),
        "Pharaohs Glory" => {
            world.religion.ra_export_months = 12;
            world.post("message_blessing_trade_from_ra", None, true);
            Outcome::Applied
        }
        "Bird of Prey" if missing(world, RA) => Outcome::NeedsGod(RA),
        "Bird of Prey" => {
            world.religion.ra_trade_down2_months = 12;
            world.post("message_wrath_of_ra_2", None, true);
            Outcome::Applied
        }
        "Supreme Craftsman" if missing(world, PTAH) => Outcome::NeedsGod(PTAH),
        "Supreme Craftsman" => {
            let key = if world.ptah_fills_yard() { "message_blessing_trade_from_ptah" } else { "message_blessing_from_ptah" };
            world.post(key, None, true);
            Outcome::Applied
        }
        "Noble Djed" if missing(world, PTAH) => Outcome::NeedsGod(PTAH),
        "Noble Djed" => {
            world.ptah_stocks_workshops();
            world.post("message_minor_blessing_from_ptah", None, true);
            Outcome::Applied
        }
        "Big Dave" if missing(world, PTAH) => Outcome::NeedsGod(PTAH),
        "Big Dave" => {
            let key = if world.ptah_razes_industry() { "message_wrath_of_ptah_2" } else { "message_wrath_of_ptah" };
            world.post(key, None, true);
            Outcome::Applied
        }
        // Cleopatra: the frog plague is Ptah's other major curse, alongside Big
        // Dave's razed industry above.
        "Amphibious Assault" if missing(world, PTAH) => Outcome::NeedsGod(PTAH),
        "Amphibious Assault" => {
            world.frogs();
            world.post("message_wrath_of_ptah_4", None, true);
            Outcome::Applied
        }
        "Grenow" if missing(world, PTAH) => Outcome::NeedsGod(PTAH),
        "Grenow" => {
            let yard = world.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).max_by_key(|b| world.total_stored(b.id)).map(|b| b.id);
            let key = match yard {
                Some(id) => {
                    world.destroy(id, true);
                    "message_ptah_is_upset"
                }
                None => "message_wrath_of_ptah",
            };
            world.post(key, None, true);
            Outcome::Applied
        }
        "Typhonian Relief" if missing(world, SETH) => Outcome::NeedsGod(SETH),
        "Typhonian Relief" => {
            world.religion.seth_protects = true;
            world.post("message_minor_blessing_from_seth", None, true);
            Outcome::Applied
        }
        "Spirit of Typhon" if missing(world, SETH) => Outcome::NeedsGod(SETH),
        "Spirit of Typhon" => {
            world.religion.seth_crush = 10;
            world.post("message_blessing_trade_from_seth", None, true);
            Outcome::Applied
        }
        "Seth Strikes" if missing(world, SETH) => Outcome::NeedsGod(SETH),
        "Seth Strikes" => {
            // Seth takes the most experienced company (the last of the equals) and
            // burns its fort, as his own minor curse does (religion.rs).
            let best = world.military.companies.iter().filter(|c| c.fort != 0).fold(None, |best: Option<&Company>, c| match best {
                Some(b) if b.experience > c.experience => Some(b),
                _ => Some(c),
            });
            let key = match best.map(|c| c.fort) {
                Some(fort) => {
                    world.destroy(fort, true);
                    "message_seth_is_upset"
                }
                None => "message_wrath_of_seth_noeffect",
            };
            world.post(key, None, true);
            Outcome::Applied
        }
        // Cleopatra: the hailstorm is Seth's other major curse, alongside the
        // boat-sinking Fury of Seth uses above (see `World::fury_of_seth`).
        "Hail to the Chief" if missing(world, SETH) => Outcome::NeedsGod(SETH),
        "Hail to the Chief" => {
            world.hailstorm();
            world.post("message_hailstorm_wrath_of_seth", None, true);
            Outcome::Applied
        }
        "Cat Nip" if missing(world, BAST) => Outcome::NeedsGod(BAST),
        "Cat Nip" => {
            world.bast_bounty();
            world.post("message_blessing_from_bast", None, true);
            Outcome::Applied
        }
        "Meow" if missing(world, BAST) => Outcome::NeedsGod(BAST),
        "Meow" => {
            world.bast_festival_now();
            Outcome::Applied
        }
        "Kitty Litter" if missing(world, BAST) => Outcome::NeedsGod(BAST),
        "Kitty Litter" => {
            world.start_plague(true);
            Outcome::Applied
        }
        "Cat Fight" if missing(world, BAST) => Outcome::NeedsGod(BAST),
        "Cat Fight" => {
            // Fire takes the twenty finest houses, as Bast's major curse does.
            let mut houses: Vec<(u8, u32)> = world.buildings.iter().filter_map(|b| b.house.as_ref().map(|h| (h.level, b.id))).collect();
            houses.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let ids: Vec<u32> = houses.into_iter().take(20).map(|(_, id)| id).collect();
            for id in ids {
                world.destroy(id, true);
            }
            world.post("message_wrath_of_bast", None, true);
            Outcome::Applied
        }
        // Recognized, but Osiris has no matching mechanic yet: no hippo figure, no
        // tomb-robbers that single out burial goods, no river-to-blood event, no
        // pyramid speed-up, no mummy horde.
        "Hippo Stomp" | "Side Show" | "Jail Break" | "Crimson Tide" | "Ancient Astronauts" | "Mummys Revenge" => Outcome::NotModeled,
        // Not in the original: marks every campaign mission won. The world has no
        // campaign progress of its own (that's saved by family, by the app), so
        // the app carries this one out.
        "Unlock All Missions" => Outcome::NeedsApp,
        _ => Outcome::Unknown,
    }
}
