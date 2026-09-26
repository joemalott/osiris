use anyhow::{Context, Result, bail};
use osiris_formats::campaign::CampaignEntry;
use osiris_formats::{
    Campaign, ChunkFile, Layout, MessageTable, MissionPak, Model, Scenario, Sg3, Sprite, TextTable,
};
use std::path::{Path, PathBuf};

mod terrain_check;

const USAGE: &str = "usage:
  osiris-tools check-sg3 <Data dir>                  decode every image in every .sg3
  osiris-tools dump-sprites <Data dir> <pak> <out>   write each image of <pak> as PNG
  osiris-tools info <Data dir> <pak>                 list groups and records
  osiris-tools check-images <game dir>              check every map's tile images
  osiris-tools check-terrain-images <game dir> [-v] redraw every map's terrain and compare
  osiris-tools check-maps <game dir>                 parse every .map and campaign mission
  osiris-tools text <game dir> <group>               print all strings of a text group
  osiris-tools message <game dir> <id>               print one Pharaoh_MM.eng entry
  osiris-tools model <game dir> <difficulty>         print buildings/houses/figures for a difficulty
  osiris-tools campaign <game dir>                   print the campaign.txt structure
  osiris-tools empire <game dir> <mission|map path>  print the empire's cities and routes
  osiris-tools roundtrip-map <map> <out>             read a .map and write it back out
  osiris-tools replay <game dir> <replay>...         play recorded games back, report the first divergence";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check-sg3", dir] => check_sg3(Path::new(dir)),
        ["dump-sprites", dir, pak, out] => dump_sprites(Path::new(dir), pak, Path::new(out)),
        ["info", dir, pak] => info(Path::new(dir), pak),
        ["check-maps", dir] => check_maps(Path::new(dir)),
        ["check-images", dir] => check_images(Path::new(dir)),
        ["holes", dir, what] => holes(Path::new(dir), what),
        ["check-terrain-images", dir] => terrain_check::check_terrain_images(Path::new(dir), false),
        ["check-terrain-images", dir, "-v"] => terrain_check::check_terrain_images(Path::new(dir), true),
        ["goals", dir, what] => goals(Path::new(dir), what),
        ["image-histogram", dir, what] => image_histogram(Path::new(dir), what),
        ["dump-grids", dir, what, out] => dump_grids(Path::new(dir), what, Path::new(out)),
        ["text", dir, group] => text_cmd(Path::new(dir), group),
        ["message", dir, id] => message_cmd(Path::new(dir), id),
        ["model", dir, difficulty] => model_cmd(Path::new(dir), difficulty),
        ["campaign", dir] => campaign_cmd(Path::new(dir)),
        ["empire", dir, what] => empire_cmd(Path::new(dir), what),
        ["roundtrip-map", map, out] => roundtrip_map(Path::new(map), Path::new(out)),
        ["replay", dir, files @ ..] if !files.is_empty() => replay(Path::new(dir), files),
        _ => bail!("{USAGE}"),
    }
}

fn sg3_names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            (p.extension()?.eq_ignore_ascii_case("sg3"))
                .then(|| p.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    Ok(names)
}

fn check_sg3(dir: &Path) -> Result<()> {
    let (mut total, mut failed) = (0usize, 0usize);
    for name in sg3_names(dir)? {
        let pak = Sg3::open(dir, &name).with_context(|| name.clone())?;
        let mut pak_failed = 0;
        for i in 0..pak.len() {
            total += 1;
            if let Err(e) = pak.decode(i) {
                if pak_failed < 3 {
                    eprintln!("  {e}");
                }
                pak_failed += 1;
            }
        }
        failed += pak_failed;
        println!(
            "{name:24} v{} {:5} images, {pak_failed} failed",
            pak.version,
            pak.len()
        );
    }
    println!("{total} images, {failed} failed");
    if failed > 0 {
        bail!("{failed} images failed to decode");
    }
    Ok(())
}

fn info(dir: &Path, name: &str) -> Result<()> {
    let pak = Sg3::open(dir, name)?;
    println!("{pak:?}\nbitmaps: {:?}", pak.bitmap_names);
    for g in 0..pak.group_starts.len() {
        if let Some(s) = pak.group_start(g) {
            println!("group {g:3} starts at {s}");
        }
    }
    for (i, r) in pak.records.iter().enumerate() {
        println!(
            "{i:5} {:4}x{:<4} {:?} rle={} ext={} top={} mirror={} anim={} off=({},{}) bmp={}",
            r.width,
            r.height,
            r.kind,
            r.compressed as u8,
            r.external as u8,
            r.has_isometric_top as u8,
            r.mirror_offset,
            r.num_animation_sprites,
            r.sprite_offset_x,
            r.sprite_offset_y,
            pak.bitmap_name(r)
        );
    }
    Ok(())
}

fn dump_sprites(dir: &Path, name: &str, out: &Path) -> Result<()> {
    let pak = Sg3::open(dir, name)?;
    std::fs::create_dir_all(out)?;
    let mut written = 0;
    for i in 0..pak.len() {
        let sprite = pak.decode(i)?;
        if sprite.width == 0 || sprite.height == 0 {
            continue;
        }
        write_png(&out.join(format!("{name}_{i:05}.png")), &sprite)?;
        written += 1;
    }
    println!("wrote {written} PNGs to {}", out.display());
    Ok(())
}

pub fn write_png(path: &PathBuf, sprite: &Sprite) -> Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, sprite.width, sprite.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?
        .write_image_data(sprite.pixels.as_flattened())?;
    Ok(())
}

fn describe(name: &str, s: &Scenario) {
    let i = &s.info;
    println!(
        "{name:32} v{} {}x{} start={} year={} funds={} climate={} entry=({},{}) exit=({},{}) camera={:?} \"{}\"",
        s.version,
        i.width,
        i.height,
        i.start_offset,
        i.start_year,
        i.initial_funds,
        i.climate,
        i.entry_point.x,
        i.entry_point.y,
        i.exit_point.x,
        i.exit_point.y,
        s.camera,
        i.subtitle
    );
    if i.monuments.iter().any(|&m| m != 0) || i.win.monuments.enabled {
        println!("    monuments {:?} goal {:?}", i.monuments, i.win.monuments);
    }
    if i.win.time_limit.enabled || i.win.survival_time.enabled {
        println!("    time limit {:?} survival {:?}", i.win.time_limit, i.win.survival_time);
    }
    let herds = |pts: &[osiris_formats::scenario::TilePoint]| pts.iter().filter(|p| p.x > 0).map(|p| (p.x, p.y)).collect::<Vec<_>>();
    if i.predator_herd_points.iter().chain(&i.prey_herd_points).any(|p| p.x > 0) {
        println!("    herds: animals {} predators {:?} (alt {}) prey {:?}", i.has_animals, herds(&i.predator_herd_points), i.alt_predator_type, herds(&i.prey_herd_points));
    }
    let burial: Vec<(usize, u32)> = i.burial_provisions_required.iter().copied().enumerate().filter(|p| p.1 > 0).collect();
    if !burial.is_empty() {
        println!("    burial provisions {burial:?}");
    }
}

/// Reads a map, writes its scenario back into its own chunks and saves the result, then
/// checks the copy reads back to the same chunks.
fn roundtrip_map(map: &Path, out: &Path) -> Result<()> {
    let data = std::fs::read(map).with_context(|| map.display().to_string())?;
    let file = ChunkFile::parse(&data, Layout::Map).with_context(|| map.display().to_string())?;
    let scenario = Scenario::from_chunks(&file)?;
    let written = scenario.to_chunk_file(file.clone())?.to_bytes();
    std::fs::write(out, &written).with_context(|| out.display().to_string())?;
    let back = ChunkFile::parse(&written, Layout::Map)?;
    for name in file.names() {
        if file.get(name) != back.get(name) {
            bail!("chunk {name} differs after the round trip");
        }
    }
    println!(
        "{} ({} bytes) -> {} ({} bytes), all {} chunks identical",
        map.display(),
        data.len(),
        out.display(),
        written.len(),
        file.names().count()
    );
    Ok(())
}

fn check_maps(game: &Path) -> Result<()> {
    let mut maps: Vec<PathBuf> = std::fs::read_dir(game.join("Maps"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("map")))
        .collect();
    maps.sort();
    for p in &maps {
        let data = std::fs::read(p)?;
        let file = ChunkFile::parse(&data, Layout::Map).with_context(|| p.display().to_string())?;
        if file.trailing != 0 {
            bail!("{}: {} trailing bytes", p.display(), file.trailing);
        }
        describe(
            &p.file_name().unwrap().to_string_lossy(),
            &Scenario::from_chunks(&file)?,
        );
    }
    let pak = MissionPak::open(&game.join("mission1.pak"))?;
    let mut n = 0;
    for i in 0..pak.slots() {
        if pak.entry(i).is_none() {
            continue;
        }
        let file = pak.chunks(i).with_context(|| format!("mission {i}"))?;
        describe(
            &format!("mission1.pak #{i} (+{})", file.trailing),
            &pak.scenario(i)?,
        );
        n += 1;
    }
    println!("{} maps and {n} campaign missions parsed", maps.len());
    Ok(())
}

fn image_histogram(game: &Path, what: &str) -> Result<()> {
    let s = load_any(game, what)?;
    let lib = osiris_formats::ImageLibrary::open(&game.join("Data"))?;
    let mut per_pack: std::collections::BTreeMap<String, (usize, u32, u32)> = Default::default();
    let (mut draw, mut unresolved) = (0, 0);
    for y in 0..s.info.height {
        for x in 0..s.info.width {
            let off = s.offset(x, y).unwrap();
            let id = s.images[off];
            if s.edges[off] & 0x40 == 0 {
                continue;
            }
            draw += 1;
            match lib.resolve(id) {
                Some(p) => {
                    let e = per_pack
                        .entry(lib.pack(p.pack).name.clone())
                        .or_insert((0, u32::MAX, 0));
                    e.0 += 1;
                    e.1 = e.1.min(id);
                    e.2 = e.2.max(id);
                }
                None => unresolved += 1,
            }
        }
    }
    println!("{draw} draw tiles, {unresolved} unresolved");
    for (k, v) in per_pack {
        println!("{k:20} {:6} ids {}..{}", v.0, v.1, v.2);
    }
    Ok(())
}

/// Checks every map's and campaign mission's tile images, as a new city draws them
/// (see `osiris_sim::terrain_images::redraw_on_load`): each drawn tile must resolve to
/// an image outside the walker and interface packs. Prints the maps whose tiles don't,
/// with an example, and fails if any do.
fn check_images(game: &Path) -> Result<()> {
    let lib = osiris_formats::ImageLibrary::open(&game.join("Data"))?;
    let defs = osiris_sim::Defs::load(&lib).map_err(anyhow::Error::msg)?;
    let not_ground = ["SprMain", "SprMain2", "SprAmbient", "Pharaoh_Fonts", "Empire", "Pharaoh_Unloaded"];
    let sources = terrain_check::sources(game)?;
    let mut bad_sources = 0;
    for (name, s) in &sources {
        let (mut drawn, mut unknown, mut wrong) = (0, 0, 0);
        let mut example = None;
        let mut map = osiris_sim::map::Map::from_scenario(s);
        osiris_sim::terrain_images::redraw_on_load(&mut map, &defs, s.version);
        for y in 0..map.height {
            for x in 0..map.width {
                let id = map.images.at_or(x, y, 0);
                if map.edges.at_or(x, y, 0) & 0x40 == 0 || id == 0 {
                    continue;
                }
                drawn += 1;
                match lib.resolve(id) {
                    None => {
                        unknown += 1;
                        example.get_or_insert(format!("{x},{y} id {id} unknown"));
                    }
                    Some(img) if not_ground.contains(&lib.pack(img.pack).name.as_str()) => {
                        wrong += 1;
                        example.get_or_insert(format!("{x},{y} id {id} from {}", lib.pack(img.pack).name));
                    }
                    Some(_) => {}
                }
            }
        }
        if unknown + wrong > 0 {
            bad_sources += 1;
            println!("{name:24} v{:3} {:6} of {drawn} tiles wrong ({unknown} unknown, {wrong} from walker or interface packs), e.g. {}", s.version, unknown + wrong, example.unwrap_or_default());
        }
    }
    println!("{} maps and missions checked, {bad_sources} with wrong tiles", sources.len());
    if bad_sources > 0 {
        bail!("{bad_sources} maps or missions have tiles drawn with the wrong images");
    }
    Ok(())
}

/// Tiles of a map with nothing to draw: no draw flag and not covered by a larger
/// image, or image 0. Prints each with its terrain and edge bits.
fn holes(game: &Path, what: &str) -> Result<()> {
    let s = load_any(game, what)?;
    let mut n = 0;
    let mut kinds: std::collections::BTreeMap<(u32, bool), usize> = Default::default();
    for y in 0..s.info.height {
        for x in 0..s.info.width {
            let off = s.offset(x, y).unwrap();
            let (img, edge, t) = (s.images[off], s.edges[off], s.terrain[off]);
            let outside = t & 0x0008_0000 != 0;
            let hole = !outside && (img == 0 || (edge & 0x40 == 0 && edge & 0x3f == 0));
            if hole {
                n += 1;
                *kinds.entry((t, img == 0)).or_insert(0) += 1;
                if n <= 5 {
                    println!("{x},{y} image {img} edge {edge:#04x} terrain {t:#010x}");
                }
            }
        }
    }
    for ((t, blank), c) in kinds {
        println!("  terrain {t:#010x}{}: {c}", if blank { " (no image)" } else { "" });
    }
    println!("{n} holes");
    Ok(())
}

fn load_any(game: &Path, what: &str) -> Result<Scenario> {
    Ok(if let Ok(n) = what.parse::<usize>() {
        MissionPak::open(&game.join("mission1.pak"))?.scenario(n)?
    } else {
        Scenario::load_map(Path::new(what))?
    })
}

/// Writes the image grid (u32 LE) followed by the edge grid (u8) for offline analysis.
fn dump_grids(game: &Path, what: &str, out: &Path) -> Result<()> {
    let s = load_any(game, what)?;
    let mut buf: Vec<u8> = s.images.iter().flat_map(|v| v.to_le_bytes()).collect();
    buf.extend_from_slice(&s.edges);
    buf.extend(s.terrain.iter().flat_map(|v| v.to_le_bytes()));
    buf.extend_from_slice(&s.moisture);
    std::fs::write(out, buf)?;
    Ok(())
}

fn goals(game: &Path, what: &str) -> Result<()> {
    let s = load_any(game, what)?;
    let i = &s.info;
    println!(
        "{} | funds {} | year {} | rank {} | open_play {}",
        i.subtitle, i.initial_funds, i.start_year, i.player_rank, i.is_open_play
    );
    println!("{:#?}", i.win);
    println!(
        "gods_known {:?} empire {} climate {} animals {}",
        i.gods_known, i.empire_id, i.climate, i.has_animals
    );
    println!("reserved (allowed-building flags?) {:?}", i.reserved);
    Ok(())
}

fn text_cmd(game: &Path, group: &str) -> Result<()> {
    let data = std::fs::read(game.join("Pharaoh_Text.eng"))?;
    let table = TextTable::parse(&data)?;
    let group: usize = group.parse().context("group must be a number")?;
    let len = table.group_len(group);
    if len == 0 {
        bail!(
            "group {group} is empty or unused ({} groups total)",
            table.group_count()
        );
    }
    for i in 0..len {
        println!("{i:4} {:?}", table.get(group, i).unwrap());
    }
    Ok(())
}

fn message_cmd(game: &Path, id: &str) -> Result<()> {
    let data = std::fs::read(game.join("Pharaoh_MM.eng"))?;
    let table = MessageTable::parse(&data)?;
    let id: usize = id.parse().context("id must be a number")?;
    let m = table
        .get(id)
        .with_context(|| format!("no message {id} ({} entries total)", table.len()))?;
    println!(
        "id={} kind={} category={} data={} pos={:?} size={:?} delay={}",
        m.id, m.kind, m.category, m.data, m.pos, m.size, m.delay
    );
    println!("image1={:?} image2={:?}", m.image1, m.image2);
    println!("video={:?} sound={:?}", m.video, m.sound);
    println!("title: {:?}", m.title);
    println!("subtitle: {:?}", m.subtitle);
    println!("content:\n{}", m.content);
    Ok(())
}

fn model_cmd(game: &Path, difficulty: &str) -> Result<()> {
    let path = game.join(format!("Pharaoh_Model_{difficulty}.txt"));
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    let model = Model::parse(&text)?;
    println!(
        "{}: {} buildings, {} houses",
        path.display(),
        model.buildings.len(),
        model.houses.len()
    );
    for b in &model.buildings {
        println!(
            "  building {:3} {:24} cost={:<6} employees={}",
            b.id, b.name, b.cost, b.employees
        );
    }
    for h in &model.houses {
        println!(
            "  house {:2} {:34} capacity={:<6} tax_mult={}",
            h.level, h.name, h.capacity, h.tax_multiplier
        );
    }

    let figure_path = game.join(format!("Figure_model_{}.txt", difficulty.to_lowercase()));
    let figure_text =
        std::fs::read_to_string(&figure_path).with_context(|| figure_path.display().to_string())?;
    let figures = osiris_formats::model::parse_figures(&figure_text)?;
    println!("{}: {} figures", figure_path.display(), figures.len());
    for f in &figures {
        println!(
            "  figure {:3} {:28} {:9} hp={} attack={}",
            f.id,
            f.name,
            f.category.as_deref().unwrap_or(""),
            f.hit_points,
            f.attack
        );
    }
    Ok(())
}

fn campaign_cmd(game: &Path) -> Result<()> {
    let text = std::fs::read_to_string(game.join("campaign.txt"))?;
    let c = Campaign::parse(&text)?;
    println!(
        "{} mission names, {} sections",
        c.mission_names.len(),
        c.sections.len()
    );
    for (i, name) in c.mission_names.iter().enumerate() {
        println!("  mission name {i:3} {name}");
    }
    for section in &c.sections {
        println!("[{}]", section.name);
        for entry in &section.entries {
            match entry {
                CampaignEntry::Mission(m) => println!(
                    "  mission {:3} intro_mm={} victory_text={} path={} merge={:?}",
                    m.id, m.intro_mm, m.victory_text, m.path_id, m.merge_paths
                ),
                CampaignEntry::ChoiceScreen { screen, choices } => {
                    println!(
                        "  choicescreen graphic={} title_text={} choices={}",
                        screen.graphic_id,
                        screen.title_text_id,
                        choices.len()
                    );
                    for choice in choices {
                        println!(
                            "    choice path={} pos=({},{}) text={}",
                            choice.path_id, choice.x, choice.y, choice.text_id
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn empire_cmd(game: &Path, what: &str) -> Result<()> {
    let s = match what.parse::<usize>() {
        Ok(n) => MissionPak::open(&game.join("mission1.pak"))?.scenario(n)?,
        Err(_) => Scenario::load_map(Path::new(what))?,
    };
    let text = TextTable::parse(&std::fs::read(game.join("Pharaoh_Text.eng"))?)?;
    let e = &s.info;
    println!("empire id {} ", e.empire_id);
    for (i, o) in s.empire.objects.iter().enumerate().filter(|(_, o)| o.in_use && o.kind == osiris_formats::empire::object::CITY) {
        let name = text.get(195, o.city_name_id as usize).unwrap_or("?");
        let demand: Vec<String> = o.demand.iter().enumerate().filter(|(_, d)| **d > 0).map(|(r, d)| format!("{r}:{d}")).collect();
        println!(
            "obj {i:3} city {:2} {name:14} type {} route {} open {} cost {} at ({},{}) name {} image {} sells {:?} buys {:?} demand {:?}",
            o.city_name_id, o.city_type, o.trade_route_id, o.trade_route_open, o.trade_route_cost, o.x, o.y, o.text_align, o.image_id, o.sells, o.buys, demand
        );
    }
    for (i, r) in s.empire.routes.iter().enumerate().filter(|(_, r)| r.in_use) {
        println!("route {i}: type {} step {} {} points {:?}", r.route_type, r.step, r.points.len(), r.points.first());
    }
    println!("prices {:?}", s.empire.prices.iter().take(12).collect::<Vec<_>>());
    for (i, e) in s.events.iter().enumerate() {
        println!(
            "event {i:3} type {:2} trig {:2} y{} m{} time {:?} item {:?} amount {:?} loc {:?} months {} sender {} sub {} city {} tag {} chain c{} r{} l{} d{} reasons {:?}",
            e.kind, e.trigger, e.year, e.month, (e.time.min, e.time.max), e.item, e.amount, e.location, e.months, e.sender, e.subtype, e.city, e.tag, e.on_completed, e.on_refusal, e.on_too_late, e.on_defeat, e.reasons
        );
    }
    Ok(())
}

/// Plays each recorded game back from its start and says whether it came out the
/// same, or where it first went another way. Fails if any differed.
fn replay(game: &Path, files: &[&str]) -> Result<()> {
    use osiris_sim::replay::{Player, Replay};
    let library = osiris_formats::ImageLibrary::open(&game.join("Data"))?;
    let defs = std::sync::Arc::new(osiris_sim::Defs::load(&library).map_err(anyhow::Error::msg)?);
    let balances = osiris_sim::Balance::load_all(game).map_err(anyhow::Error::msg)?;
    let mut failed = 0;
    for file in files {
        let replay = Replay::read(Path::new(file)).map_err(anyhow::Error::msg)?;
        let (mut world, mut player) = Player::start(replay, defs.clone(), balances.clone()).map_err(anyhow::Error::msg)?;
        let started = std::time::Instant::now();
        match player.run(&mut world) {
            Ok((commands, months)) => println!("{file}: identical ({commands} commands, {months} months, {} ticks, {:.1?})", world.time.total_ticks, started.elapsed()),
            Err(d) => {
                println!("{file}: {d}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} replays diverged", files.len());
    }
    Ok(())
}
