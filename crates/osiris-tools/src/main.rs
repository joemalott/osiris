use anyhow::{Context, Result, bail};
use osiris_formats::{ChunkFile, Layout, MissionPak, Scenario, Sg3, Sprite};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage:
  osiris-tools check-sg3 <Data dir>                  decode every image in every .sg3
  osiris-tools dump-sprites <Data dir> <pak> <out>   write each image of <pak> as PNG
  osiris-tools info <Data dir> <pak>                 list groups and records
  osiris-tools check-maps <game dir>                 parse every .map and campaign mission";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["check-sg3", dir] => check_sg3(Path::new(dir)),
        ["dump-sprites", dir, pak, out] => dump_sprites(Path::new(dir), pak, Path::new(out)),
        ["info", dir, pak] => info(Path::new(dir), pak),
        ["check-maps", dir] => check_maps(Path::new(dir)),
        _ => bail!("{USAGE}"),
    }
}

fn sg3_names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            (p.extension()?.eq_ignore_ascii_case("sg3")).then(|| p.file_stem()?.to_str().map(str::to_owned))?
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
        println!("{name:24} v{} {:5} images, {pak_failed} failed", pak.version, pak.len());
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
            r.width, r.height, r.kind, r.compressed as u8, r.external as u8, r.has_isometric_top as u8,
            r.mirror_offset, r.num_animation_sprites, r.sprite_offset_x, r.sprite_offset_y, pak.bitmap_name(r)
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
    enc.write_header()?.write_image_data(sprite.pixels.as_flattened())?;
    Ok(())
}

fn describe(name: &str, s: &Scenario) {
    let i = &s.info;
    println!(
        "{name:32} v{} {}x{} start={} year={} funds={} climate={} entry=({},{}) exit=({},{}) \"{}\"",
        s.version, i.width, i.height, i.start_offset, i.start_year, i.initial_funds, i.climate,
        i.entry_point.x, i.entry_point.y, i.exit_point.x, i.exit_point.y, i.subtitle
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
        describe(&p.file_name().unwrap().to_string_lossy(), &Scenario::from_chunks(&file)?);
    }
    let pak = MissionPak::open(&game.join("mission1.pak"))?;
    let mut n = 0;
    for i in 0..pak.slots() {
        if pak.entry(i).is_none() {
            continue;
        }
        let file = pak.scenario(i).with_context(|| format!("mission {i}"))?;
        describe(&format!("mission1.pak #{i} (+{})", file.trailing), &Scenario::from_chunks(&file)?);
        n += 1;
    }
    println!("{} maps and {n} campaign missions parsed", maps.len());
    Ok(())
}
