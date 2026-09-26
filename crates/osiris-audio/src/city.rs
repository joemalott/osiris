//! Which sound the original plays for what, as tables of facts read from Pharaoh.exe
//! (the loaders FUN_00539d60, FUN_0053bcf0 and FUN_0053b130 and the walker speech table
//! at 0x5fb16c). File names are as the original spells them; the data's own spelling
//! differs in case, which [`crate::Audio`] looks past. Some names have no file in the
//! shipped data (`plaza1.wav`, `housing.wav`, `desert4.wav`...): the original loads
//! nothing for them and plays silence when it picks one, and so does Osiris.
//!
//! Building and figure types are the original's numbers, which Osiris shares.

/// Per building type: the sound its information window opens with (`AUDIO/Wavs`), and
/// up to three sounds, one of which it makes when the city picks it (see
/// [`crate::Audio::play_city_sound`]).
pub const BUILDINGS: &[(u16, &str, &[&str])] = &[
    (6, "wall1_r.wav", &["wall1.wav"]),
    (10, "", &["housing.wav"]),
    (11, "", &["housing.wav"]),
    (12, "", &["housing.wav"]),
    (13, "", &["housing.wav"]),
    (14, "", &["housing.wav"]),
    (15, "", &["housing.wav"]),
    (16, "", &["housing.wav"]),
    (17, "", &["housing.wav"]),
    (18, "", &["housing.wav"]),
    (19, "", &["housing.wav"]),
    (20, "", &["housing.wav"]),
    (21, "", &["housing.wav"]),
    (22, "", &["housing.wav"]),
    (23, "", &["housing.wav"]),
    (24, "", &["housing.wav"]),
    (25, "", &["housing.wav"]),
    (26, "", &["housing.wav"]),
    (27, "", &["housing.wav"]),
    (28, "", &["housing.wav"]),
    (29, "", &["housing.wav"]),
    (30, "music_r.wav", &["music.wav"]),
    (31, "juggler_r.wav", &["juggler.wav"]),
    (32, "bullfight_r.wav", &["bullfight.wav"]),
    (33, "dance_r.wav", &["dance.wav"]),
    (34, "music_school_r.wav", &["music_school.wav"]),
    (35, "dance_school_r.wav", &["dance_school.wav"]),
    (36, "juggler_school_r.wav", &["juggler_school.wav"]),
    (37, "bullfight_school_r.wav", &["bullfight_school.wav"]),
    (38, "plaza1_r.wav", &["plaza1.wav"]),
    (39, "park_r.wav", &["park1.wav", "park2.wav", "park3.wav"]),
    (40, "f_chariot_r.wav", &["f_chariot.wav"]),
    (41, "statue1_r.wav", &["statue1.wav"]),
    (42, "statue2_r.wav", &["statue2.wav"]),
    (43, "statue3_r.wav", &["statue3.wav"]),
    (44, "f_archer_r.wav", &["f_archer.wav"]),
    (45, "f_infantry_r.wav", &["f_infantry.wav"]),
    (46, "apothecary_r.wav", &["apothecary.wav"]),
    (47, "embalm_r.wav", &["embalm.wav"]),
    (49, "dentist_r.wav", &["dentist.wav"]),
    (51, "school_scribe_r.wav", &["school_scribe.wav"]),
    (53, "school_scholar_r.wav", &["school_scholar.wav"]),
    (55, "police_r.wav", &["police.wav"]),
    (58, "gate1_r.wav", &["gate1.wav"]),
    (59, "tower1_r.wav", &["tower1.wav"]),
    (60, "tem_osiris_r.wav", &["tem_osiris_s.wav"]),
    (61, "tem_ra_r.wav", &["tem_ra_s.wav"]),
    (62, "tem_ptah_r.wav", &["tem_ptah_s.wav"]),
    (63, "tem_seth_r.wav", &["tem_seth_s.wav"]),
    (64, "tem_bast_r.wav", &["tem_bast_s.wav"]),
    (65, "tem_osiris_r.wav", &["tem_osirus_l.wav"]),
    (66, "tem_ra_r.wav", &["tem_ra_l.wav"]),
    (67, "tem_ptah_r.wav", &["tem_ptah_l.wav"]),
    (68, "tem_seth_r.wav", &["tem_seth_l.wav"]),
    (69, "tem_bast_r.wav", &["tem_bast_l.wav"]),
    (70, "market_r.wav", &["market1.wav"]),
    (71, "granary1_r.wav", &["granary1.wav"]),
    (72, "warehouse1_r.wav", &["warehouse1.wav"]),
    (73, "storage_r.wav", &["storage.wav"]),
    (74, "shipyrd_r.wav", &["shipyrd.wav"]),
    (75, "dock1_r.wav", &["dock1.wav"]),
    (76, "fishwharf_r.wav", &["fishwharf.wav"]),
    (77, "gov_man1_r.wav", &["gov_man1.wav"]),
    (78, "gov_man2_r.wav", &["gov_man2.wav"]),
    (79, "gov_man3_r.wav", &["gov_man3.wav"]),
    (81, "eng_r.wav", &["eng.wav"]),
    (86, "taxfarm_r.wav", &["taxfarm.wav"]),
    (87, "taxfarm_r.wav", &["taxfarm.wav"]),
    (90, "waterwheel_r.wav", &["waterwheel.wav"]),
    (92, "well_r.wav", &["well.wav"]),
    (94, "academy1_r.wav", &["academy1.wav"]),
    (95, "barrack_r.wav", &["barrack.wav"]),
    (100, "barleyfarm_r.wav", &["barleyfarm.wav"]),
    (101, "flaxfarm_r.wav", &["flaxfarm.wav"]),
    (102, "wheatfarm_r.wav", &["wheatfarm.wav"]),
    (103, "lettucefarm_r.wav", &["lettucefarm.wav"]),
    (104, "pomfarm_r.wav", &["pomfarm.wav"]),
    (105, "chickfarm_r.wav", &["chickfarm.wav"]),
    (106, "stone_r.wav", &["stone.wav"]),
    (107, "marble_r.wav", &["marble.wav"]),
    (108, "lumber_r.wav", &["lumber.wav"]),
    (109, "clay_r.wav", &["clay.wav", "clay.wav"]),
    (110, "brewery_r.wav", &["brewery.wav"]),
    (111, "weaver_r.wav", &["weaver.wav"]),
    (112, "weapon_r.wav", &["weapon.wav"]),
    (113, "jewelry_r.wav", &["jewelry.wav"]),
    (114, "pottery_r.wav", &["pottery.wav"]),
    (115, "lo_hunt_r.wav", &["lo_hunt.wav"]),
    (135, "irrigation_r.wav", &["irrigation.wav"]),
    (136, "ferry_land_r.wav", &["ferry_land.wav"]),
    (140, "tem_osiris_r.wav", &["shr_osiris.wav"]),
    (141, "tem_ra_r.wav", &["shr_ra.wav"]),
    (142, "tem_ptah_r.wav", &["shr_ptah.wav"]),
    (143, "tem_seth_r.wav", &["shr_seth.wav"]),
    (144, "tem_bast_r.wav", &["shr_bast.wav"]),
    (161, "gold_r.wav", &["gold.wav"]),
    (162, "gem_r.wav", &["gem.wav"]),
    (167, "fireman_r.wav", &["fireman.wav"]),
    (168, "wall2_r.wav", &["wall2.wav"]),
    (169, "wall3_r.wav", &["wall3.wav"]),
    (170, "gate1_r.wav", &["gate1.wav"]),
    (171, "gate1_r.wav", &["gate1.wav"]),
    (172, "tower3_r.wav", &["tower3.wav"]),
    (173, "tower2_r.wav", &["tower2.wav"]),
    (177, "lo_carpenter_r.wav", &["lo_carpenter.wav"]),
    (178, "lo_brick_r.wav", &["lo_brick.wav"]),
    (179, "lo_stone_r.wav", &["lo_stone.wav", "lo_stone.wav", "lo_stone.wav"]),
    (180, "cistern_r.wav", &["well.wav"]),
    (181, "transwharf_r.wav", &["transwharf.wav"]),
    (182, "warwharf_r.wav", &["warwharf.wav"]),
    (184, "court_r.wav", &["court.wav"]),
    (187, "palace1_r.wav", &["palace1.wav"]),
    (188, "palace2_r.wav", &["palace2.wav"]),
    (189, "palace3_r.wav", &["palace3.wav"]),
    (190, "market_r.wav", &["market2.wav"]),
    (194, "cowfarm_r.wav", &["cowfarm.wav"]),
    (195, "reedfarm_r.wav", &["reedfarm.wav"]),
    (196, "figfarm_r.wav", &["figfarm.wav"]),
    (199, "peasant_r.wav", &["peasant.wav"]),
    (203, "paper_r.wav", &["paper.wav"]),
    (204, "brick_r.wav", &["brick.wav"]),
    (205, "chariot_r.wav", &["chariot.wav"]),
    (206, "doctor_r.wav", &["doctor.wav"]),
    (216, "granite_r.wav", &["granite.wav"]),
    (217, "gold_r.wav", &["gold.wav"]),
    (221, "stone_r.wav", &["stone.wav"]),
    (224, "flaxfarm_r.wav", &["flaxfarm.wav"]),
    (226, "zoo_r.wav", &["bullfight.wav", "zoo_r.wav"]),
    (231, "artisans_r.wav", &["artisans_r.wav"]),
    (232, "lampmaker_r.wav", &["lampmaker_r.wav"]),
    (233, "paintmaker_r.wav", &["paintmaker_r.wav"]),
];

/// Sounds of the land where no building stands, by group: 0 open land (never picked),
/// 1 water, 2 rock, 3 meadow, 4 floodplain, 5 trees.
pub const TERRAIN: [&[&str]; 6] = [
    &["desert1.wav", "desert2.wav", "desert3.wav", "desert4.wav", "desert5.wav", "desert6.wav", "desert7.wav"],
    &["water1.wav", "water2.wav", "water3.wav", "water4.wav", "water5.wav", "water6.wav", "water7.wav"],
    &["rock1.wav", "rock2.wav", "rock3.wav", "rock4.wav", "rock5.wav", "rock6.wav", "rock7.wav"],
    &["farm1.wav", "farm2.wav", "farm3.wav", "farm4.wav", "farm5.wav", "farm6.wav", "farm7.wav"],
    &["flood1.wav", "flood2.wav", "flood3.wav", "flood4.wav", "flood5.wav", "flood6.wav", "flood7.wav"],
    &["forest1.wav", "forest2.wav", "forest3.wav", "forest4.wav", "forest5.wav", "forest6.wav", "forest7.wav"],
];

/// Figure sounds by row, each with five slots: 0 clicked, 1 roaming, 2 attacking,
/// 3 dying, 4 running. Rows 0-8 are the animals (hyena, lion, scorpion, asp, ostrich,
/// antelope, hippo, crocodile, birds); 21-28 men dying, 29-32 women dying.
pub const FIGURE_SOUNDS: [[&str; 5]; 33] = [
    ["hyena_r.wav", "hyenaroam.wav", "hyenaatk.wav", "hyenadie.wav", ""],
    ["lion_r.wav", "lion_r.wav", "lion_r.wav", "liondie.wav", ""],
    ["scorpion_r.wav", "scorpion_r.wav", "scorpion_r.wav", "scorpiondie.wav", ""],
    ["asp_r.wav", "asp_r.wav", "asp_r.wav", "aspdie.wav", ""],
    ["ostrich_r.wav", "ostrichroam.wav", "", "ostrichdie.wav", "ostrichrun.wav"],
    ["antelop_r.wav", "anteloproam.wav", "", "antelopdie.wav", "anteloprun.wav"],
    ["hippo_r.wav", "", "hippoatk.wav", "hippodie.wav", ""],
    ["croc_r.wav", "", "crocatk.wav", "crocdie.wav", ""],
    ["bird_r.wav", "birdroam.wav", "", "birddie.wav", ""],
    ["", "", "sentry_atk.wav", "", ""],
    ["", "", "police_atk.wav", "", ""],
    ["", "", "arch_atk.wav", "", ""],
    ["", "", "spear_atk.wav", "", ""],
    ["", "", "chariot_atk.wav", "chariot_die.wav", ""],
    ["", "", "hunt_ant_atk.wav", "", ""],
    ["", "", "hunt_ost_atk.wav", "", ""],
    ["", "", "hunt_bird_atk.wav", "", ""],
    ["", "", "ship_arrow.wav", "ship_die.wav", ""],
    ["", "", "arch_atk.wav", "", ""],
    ["", "", "sword_atk.wav", "", ""],
    ["", "", "spear_atk.wav", "", ""],
    ["", "", "", "die1.wav", ""],
    ["", "", "", "die2.wav", ""],
    ["", "", "", "die3.wav", ""],
    ["", "", "", "die4.wav", ""],
    ["", "", "", "die5.wav", ""],
    ["", "", "", "die6.wav", ""],
    ["", "", "", "die7.wav", ""],
    ["", "", "", "die8.wav", ""],
    ["", "", "", "dieg1.wav", ""],
    ["", "", "", "dieg2.wav", ""],
    ["", "", "", "dieg3.wav", ""],
    ["", "", "", "dieg4.wav", ""],
];

/// Walker speech (`AUDIO/Voice/Walker`): twenty phrases for each speech row, row 1
/// first. An empty name is the original's `nothing.wav`.
pub const VOICES: [[&str; 20]; 51] = [
    ["Immigrant_e01.wav", "Immigrant_e02.wav", "Immigrant_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["Emigrant_e01.wav", "Emigrant_e02.wav", "Emigrant_e03.wav", "Emigrant_e04.wav", "Emigrant_e05.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["vagrant_e01.wav", "vagrant_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["thief_e01.wav", "thief_e02.wav", "thief_e03.wav", "thief_e04.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["disease_e01.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["scribe_g01.wav", "scribe_g02.wav", "scribe_g03.wav", "scribe_g04.wav", "scribe_g05.wav", "scribe_g06.wav", "scribe_g07.wav", "scribe_g08.wav", "scribe_g09.wav", "scribe_g10.wav", "scribe_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["hunt_ant_e01.wav", "hunt_ant_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["hunt_ostrich_e01.wav", "hunt_ostrich_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["hunt_bird_e01.wav", "hunt_bird_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["fishing_e01.wav", "fishing_e02.wav", "fishing_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["worker_g01.wav", "worker_g02.wav", "worker_g03.wav", "worker_g04.wav", "worker_g05.wav", "worker_g06.wav", "worker_g07.wav", "worker_g08.wav", "worker_g09.wav", "worker_g10.wav", "worker_e01.wav", "worker_e02.wav", "worker_e03.wav", "", "", "", "", "", "", ""],
    ["labor_g01.wav", "labor_g02.wav", "labor_g03.wav", "labor_g04.wav", "labor_g05.wav", "labor_g06.wav", "labor_g07.wav", "labor_g08.wav", "labor_g09.wav", "labor_g10.wav", "labor_e01.wav", "labor_e02.wav", "", "", "", "", "", "", "", ""],
    ["woodcutter_e01.wav", "woodcutter_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["reed_e01.wav", "reed_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["carpenter_e01.wav", "carpenter_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["brick_e01.wav", "brick_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["stone_e01.wav", "stone_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["mkt_buyer_g01.wav", "mkt_buyer_g02.wav", "mkt_buyer_g03.wav", "mkt_buyer_g04.wav", "mkt_buyer_g05.wav", "mkt_buyer_g06.wav", "mkt_buyer_g07.wav", "mkt_buyer_g08.wav", "mkt_buyer_g09.wav", "mkt_buyer_g10.wav", "mkt_buyer_e01.wav", "mkt_buyer_e02.wav", "", "", "", "", "", "", "", ""],
    ["mkt_seller_e01.wav", "mkt_seller_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["marketboy_e01.wav", "marketboy_e02.wav", "marketboy_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["cartpusher_e01.wav", "cartpusher_e02.wav", "cartpusher_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["dockpusher_e01.wav", "dockpusher_e02.wav", "dockpusher_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["barge_e01.wav", "barge_e02.wav", "barge_e03.wav", "barge_e04.wav", "barge_e05.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["caravan_e01.wav", "caravan_e02.wav", "caravan_e03.wav", "caravan_e04.wav", "caravan_e05.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["juggler_g01.wav", "juggler_g02.wav", "juggler_g03.wav", "juggler_g04.wav", "juggler_g05.wav", "juggler_g06.wav", "juggler_g07.wav", "juggler_g08.wav", "juggler_g09.wav", "juggler_g10.wav", "juggler_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["musician_g01.wav", "musician_g02.wav", "musician_g03.wav", "musician_g04.wav", "musician_g05.wav", "musician_g06.wav", "musician_g07.wav", "musician_g08.wav", "musician_g09.wav", "musician_g10.wav", "musician_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["dancer_g01.wav", "dancer_g02.wav", "dancer_g03.wav", "dancer_g04.wav", "dancer_g05.wav", "dancer_g06.wav", "dancer_g07.wav", "dancer_g08.wav", "dancer_g09.wav", "dancer_g10.wav", "dancer_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["senet_g01.wav", "senet_g02.wav", "senet_g03.wav", "senet_g04.wav", "senet_g05.wav", "senet_g06.wav", "senet_g07.wav", "senet_g08.wav", "senet_g09.wav", "senet_g10.wav", "senet_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["priest_osiris_g01.wav", "priest_osiris_g02.wav", "priest_osiris_g03.wav", "priest_osiris_g04.wav", "priest_osiris_g05.wav", "priest_osiris_g06.wav", "priest_osiris_g07.wav", "priest_osiris_g08.wav", "priest_osiris_g09.wav", "priest_osiris_g10.wav", "priest_osiris_e01.wav", "priest_osiris_e02.wav", "", "", "", "", "", "", "", ""],
    ["priest_ra_g01.wav", "priest_ra_g02.wav", "priest_ra_g03.wav", "priest_ra_g04.wav", "priest_ra_g05.wav", "priest_ra_g06.wav", "priest_ra_g07.wav", "priest_ra_g08.wav", "priest_ra_g09.wav", "priest_ra_g10.wav", "priest_ra_e01.wav", "priest_ra_e02.wav", "", "", "", "", "", "", "", ""],
    ["priest_ptah_g01.wav", "priest_ptah_g02.wav", "priest_ptah_g03.wav", "priest_ptah_g04.wav", "priest_ptah_g05.wav", "priest_ptah_g06.wav", "priest_ptah_g07.wav", "priest_ptah_g08.wav", "priest_ptah_g09.wav", "priest_ptah_g10.wav", "priest_ptah_e01.wav", "priest_ptah_e02.wav", "", "", "", "", "", "", "", ""],
    ["priest_seth_g01.wav", "priest_seth_g02.wav", "priest_seth_g03.wav", "priest_seth_g04.wav", "priest_seth_g05.wav", "priest_seth_g06.wav", "priest_seth_g07.wav", "priest_seth_g08.wav", "priest_seth_g09.wav", "priest_seth_g10.wav", "priest_seth_e01.wav", "priest_seth_e02.wav", "", "", "", "", "", "", "", ""],
    ["priest_bast_g01.wav", "priest_bast_g02.wav", "priest_bast_g03.wav", "priest_bast_g04.wav", "priest_bast_g05.wav", "priest_bast_g06.wav", "priest_bast_g07.wav", "priest_bast_g08.wav", "priest_bast_g09.wav", "priest_bast_g10.wav", "priest_bast_e01.wav", "priest_bast_e02.wav", "priest_bast_e03.wav", "priest_bast_e04.wav", "", "", "", "", "", ""],
    ["teacher_g01.wav", "teacher_g02.wav", "teacher_g03.wav", "teacher_g04.wav", "teacher_g05.wav", "teacher_g06.wav", "teacher_g07.wav", "teacher_g08.wav", "teacher_g09.wav", "teacher_g10.wav", "teacher_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["library_g01.wav", "library_g02.wav", "library_g03.wav", "library_g04.wav", "library_g05.wav", "library_g06.wav", "library_g07.wav", "library_g08.wav", "library_g09.wav", "library_g10.wav", "library_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["water_g01.wav", "water_g02.wav", "water_g03.wav", "water_g04.wav", "water_g05.wav", "water_g06.wav", "water_g07.wav", "water_g08.wav", "water_g09.wav", "water_g10.wav", "", "", "", "", "", "", "", "", "", ""],
    ["dentist_g01.wav", "dentist_g02.wav", "dentist_g03.wav", "dentist_g04.wav", "dentist_g05.wav", "dentist_g06.wav", "dentist_g07.wav", "dentist_g08.wav", "dentist_g09.wav", "dentist_g10.wav", "", "", "", "", "", "", "", "", "", ""],
    ["doctor_g01.wav", "doctor_g02.wav", "doctor_g03.wav", "doctor_g04.wav", "doctor_g05.wav", "doctor_g06.wav", "doctor_g07.wav", "doctor_g08.wav", "doctor_g09.wav", "doctor_g10.wav", "doctor_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["embalmer_g01.wav", "embalmer_g02.wav", "embalmer_g03.wav", "embalmer_g04.wav", "embalmer_g05.wav", "embalmer_g06.wav", "embalmer_g07.wav", "embalmer_g08.wav", "embalmer_g09.wav", "embalmer_g10.wav", "embalmer_e01.wav", "", "", "", "", "", "", "", "", ""],
    ["", "apothecary_e02.wav", "apothecary_e03.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["fireman_g01.wav", "fireman_g02.wav", "fireman_g03.wav", "fireman_g04.wav", "fireman_g05.wav", "fireman_g06.wav", "fireman_g07.wav", "fireman_g08.wav", "fireman_g09.wav", "fireman_g10.wav", "fireman_e01.wav", "fireman_e02.wav", "fireman_e03.wav", "", "", "", "", "", "", ""],
    ["engineer_g01.wav", "engineer_g02.wav", "engineer_g03.wav", "engineer_g04.wav", "engineer_g05.wav", "engineer_g06.wav", "engineer_g07.wav", "engineer_g08.wav", "engineer_g09.wav", "engineer_g10.wav", "engineer_e01.wav", "engineer_e02.wav", "", "", "", "", "", "", "", ""],
    ["police_g01.wav", "police_g02.wav", "police_g03.wav", "police_g04.wav", "police_g05.wav", "police_g06.wav", "police_g07.wav", "police_g08.wav", "police_g09.wav", "police_g10.wav", "police_e01.wav", "police_e02.wav", "police_e03.wav", "police_e04.wav", "police_e05.wav", "police_e06.wav", "police_e07.wav", "police_e08.wav", "", ""],
    ["taxman_g01.wav", "taxman_g02.wav", "taxman_g03.wav", "taxman_g04.wav", "taxman_g05.wav", "taxman_g06.wav", "taxman_g07.wav", "taxman_g08.wav", "taxman_g09.wav", "taxman_g10.wav", "taxman_e01.wav", "taxman_e02.wav", "taxman_e03.wav", "", "", "", "", "", "", ""],
    ["magistrate_g01.wav", "magistrate_g02.wav", "magistrate_g03.wav", "magistrate_g04.wav", "magistrate_g05.wav", "magistrate_g06.wav", "magistrate_g07.wav", "magistrate_g08.wav", "magistrate_g09.wav", "magistrate_g10.wav", "magistrate_e01.wav", "magistrate_e02.wav", "magistrate_e03.wav", "magistrate_e04.wav", "magistrate_e05.wav", "", "", "", "", ""],
    ["artisan_e01.wav", "artisan_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["robber_e01.wav", "robber_e02.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["warship_e01.wav", "warship_e02.wav", "warship_e03.wav", "warship_e04.wav", "warship_e05.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["transport_e01.wav", "transport_e02.wav", "transport_e03.wav", "transport_e04.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["CHARIOT.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
    ["guard_e01.wav", "guard_e02.wav", "guard_e03.wav", "guard_e04.wav", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", ""],
];

/// The zookeeper's phrases (speech row 108).
pub const ZOOKEEPER: [&str; 10] = ["zookeeper_e01.wav", "zookeeper_e02.wav", "zookeeper_e03.wav", "zookeeper_e04.wav", "zookeeper_e05.wav", "zookeeper_e06.wav", "zookeeper_e07.wav", "zookeeper_e08.wav", "zookeeper_e09.wav", "zookeeper_e10.wav"];

/// The speech row of each figure type (0x6012ec, less 201): -1 none; -2 the hunter,
/// whose row is his prey's; -3 the priest, whose row is his temple's god; -4 by the
/// figure's second type byte.
pub const VOICE_ROWS: [i16; 110] = [-1, 1, 2, 3, 21, 12, -1, 44, 42, 21, 41, -1, -1, -1, -1, 25, 26, 27, 28, 24, 23, 24, -1, 4, 47, 10, 19, -3, -1, 34, 35, 37, 38, 40, 39, 11, -1, -1, 21, 18, 6, -1, 51, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 20, -1, -1, -1, -1, -1, -1, -2, -1, 13, -1, 49, 48, 15, 16, 17, -1, -1, -1, 11, -1, 36, 43, 45, 14, -4, -1, -1, -1, -1, 11, -1, 5, -1, -1, -1, -1, -1, -1, 108, -1, -1, 46, -1];

/// Osiris's meadow farms are the original's farms of the same crop.
fn original_type(kind: u16) -> u16 {
    match kind {
        311..=316 => kind - 211,
        317 => 224,
        318 => 196,
        k => k,
    }
}

/// A building type's information-window sound and its city sounds.
pub fn building(kind: u16) -> Option<(&'static str, &'static [&'static str])> {
    let kind = original_type(kind);
    BUILDINGS.binary_search_by_key(&kind, |b| b.0).ok().map(|i| (BUILDINGS[i].1, BUILDINGS[i].2))
}

/// What a building adds to the city's ambience near the middle of the view
/// (FUN_0053b830): 1 people, 2 industry, 3 farming (never counted), 4 monuments.
pub fn ambient_kind(kind: u16) -> u8 {
    match original_type(kind) {
        6 | 10..=45 | 47 | 49 | 51 | 53 | 55 | 58 | 59 | 70 | 77..=79 | 81 | 90 | 92 | 94 | 95 | 135 | 140..=145 | 151..=160 | 167..=173 | 184 | 206 | 226 => 1,
        71..=76 | 106..=115 | 136 | 161 | 162 | 177..=179 | 181 | 182 | 204 | 205 | 217 | 221 | 231 => 2,
        86 | 87 | 101..=104 | 224 => 3,
        183 | 210 => 4,
        _ => 0,
    }
}

/// The ambience (`AUDIO/Ambient`) for the kind of place filling the middle of the view
/// (FUN_0053b960), `None` for silence. The people's hum grows with the city.
pub fn ambience(place: usize, population: i32) -> Option<&'static str> {
    Some(match place {
        1 => match population {
            ..500 => "housing1.mp3",
            500..2000 => "housing2.mp3",
            2000..5000 => "housing3.mp3",
            5000..8000 => "housing4.mp3",
            _ => "housing5.mp3",
        },
        2 => "industrial.mp3",
        3 | 7 => "meadow.mp3",
        // A constant of 10000 against 999 always takes the first; neither file ships.
        4 => "monument_f.mp3",
        6 => "desert.mp3",
        8 => "floodplain.mp3",
        9 => "rock.mp3",
        10 => "water.mp3",
        11 => "vegetation.mp3",
        _ => return None,
    })
}

/// The kind of place a bare tile with terrain bits `t` counts as for the ambience:
/// 10 water, 9 rock, 7 meadow, 8 floodplain, 11 trees and shrubs, 6 open land.
pub fn land_place(t: u32) -> usize {
    if t & 4 != 0 && t & 0x8_0000 == 0 {
        10
    } else if t & 2 != 0 {
        9
    } else if t & 0x800 != 0 {
        7
    } else if t & 0x5_0000 != 0 {
        8
    } else if t & 0x11 != 0 {
        11
    } else if t & 0x80 != 0 {
        7
    } else {
        6
    }
}

/// The group of land sounds a bare tile with terrain bits `t` makes, if any.
pub fn land_group(t: u32) -> Option<usize> {
    if t & 1 != 0 {
        Some(5)
    } else if t & 2 != 0 {
        Some(2)
    } else if t & 4 != 0 && t & 0x8_0000 == 0 {
        Some(1)
    } else if t & 0x800 != 0 {
        Some(3)
    } else if t & 0x1_0000 != 0 {
        Some(4)
    } else {
        None
    }
}

/// The figure-sound row an animal of original type `kind` answers a click with.
pub fn animal_row(kind: u16) -> Option<usize> {
    Some(match kind {
        83 => 0,
        103 => 1,
        104 => 2,
        102 => 3,
        69 => 4,
        70 => 5,
        84 => 6,
        82 => 7,
        68 => 8,
        _ => return None,
    })
}

/// The figure-sound row of a blow struck by a figure of original type `kind`
/// (FUN_004b5210), if it makes one.
pub fn attack_row(kind: u16) -> Option<usize> {
    Some(match kind {
        0xb | 0x2b | 0x37 => 0xb,
        0xc | 0x2c | 0x36 => 0xc,
        0xd | 0x2d | 0x38 | 99 => 0xd,
        0x4e | 0x5d | 100 => 0x11,
        _ => return None,
    })
}

/// The figure-sound row of a figure of original type `kind` falling
/// (FUN_004b5050); `roll` picks among the cries.
pub fn death_row(kind: u16, roll: usize) -> usize {
    match kind {
        0xd | 0x2d | 0x38 => 0xd,
        0x1a => 0x1d + roll % 4,
        0x44 => 8,
        0x45 => 4,
        0x46 => 5,
        0x4d | 0x4e | 0x5c | 0x5d | 100 | 0x65 => 0x11,
        0x52 => 7,
        0x53 => 0,
        0x54 => 6,
        0x66 => 3,
        0x67 => 1,
        0x68 => 2,
        _ => 0x15 + roll % 8,
    }
}

/// Whether a figure of original type `kind` is heard, at a quarter of the volume, when
/// it is off the screen (FUN_0053b4a0); other figures are heard only on screen.
pub fn heard_off_screen(kind: u16) -> bool {
    matches!(kind, 0xb | 0xc | 0xd | 0x2b | 0x2c | 0x2d | 0x36 | 0x37 | 0x38 | 0x4d | 0x4e | 0x5c | 0x5d | 99 | 100 | 0x65)
}

/// The walker speech file for speech `row` (1-based) and `phrase`.
pub fn voice(row: i16, phrase: usize) -> Option<&'static str> {
    let name = if row == 108 { ZOOKEEPER.get(phrase)? } else { VOICES.get((row as usize).checked_sub(1)?)?.get(phrase)? };
    (!name.is_empty()).then_some(*name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_and_whole() {
        assert!(BUILDINGS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(building(92), Some(("well_r.wav", &["well.wav"][..])));
        assert_eq!(building(313), building(102));
        assert_eq!(voice(38, 0), Some("doctor_g01.wav"));
        assert_eq!(voice(108, 9), Some("zookeeper_e10.wav"));
        assert_eq!(voice(1, 5), None);
        assert_eq!(VOICE_ROWS[27], -3);
    }

    #[test]
    fn places_by_terrain() {
        assert_eq!(land_place(0), 6);
        assert_eq!(land_place(4), 10);
        assert_eq!(land_place(4 | 0x8_0000), 6);
        assert_eq!(land_place(0x1_0000), 8);
        assert_eq!(land_group(0x1_0000), Some(4));
        assert_eq!(land_group(0x40), None);
        assert_eq!(ambience(1, 4999), Some("housing3.mp3"));
        assert_eq!(ambience(1, 8000), Some("housing5.mp3"));
    }
}
