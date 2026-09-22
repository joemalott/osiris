use anyhow::{Context, Result, bail};
use osiris_formats::campaign::CampaignEntry;
use osiris_formats::{
    Campaign, ChunkFile, Layout, MessageTable, MissionPak, Model, Scenario, Sg3, Sprite, TextTable,
};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage:
  osiris-tools check-sg3 <Data dir>                  decode every image in every .sg3
  osiris-tools dump-sprites <Data dir> <pak> <out>   write each image of <pak> as PNG
  osiris-tools info <Data dir> <pak>                 list groups and records
  osiris-tools check-maps <game dir>                 parse every .map and campaign mission
  osiris-tools text <game dir> <group>               print all strings of a text group
  osiris-tools message <game dir> <id>               print one Pharaoh_MM.eng entry
  osiris-tools model <game dir> <difficulty>         print buildings/houses/figures for a difficulty
  osiris-tools campaign <game dir>                   print the campaign.txt structure";

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
        ["goals", dir, what] => goals(Path::new(dir), what),
        ["image-histogram", dir, what] => image_histogram(Path::new(dir), what),
        ["dump-grids", dir, what, out] => dump_grids(Path::new(dir), what, Path::new(out)),
        ["text", dir, group] => text_cmd(Path::new(dir), group),
        ["message", dir, id] => message_cmd(Path::new(dir), id),
        ["model", dir, difficulty] => model_cmd(Path::new(dir), difficulty),
        ["campaign", dir] => campaign_cmd(Path::new(dir)),
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
        "{name:32} v{} {}x{} start={} year={} funds={} climate={} entry=({},{}) exit=({},{}) \"{}\"",
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
        i.subtitle
    );
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
                        .entry(lib.pack(p.pack).sg3.name.clone())
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
