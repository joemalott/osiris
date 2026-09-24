# Osiris

Osiris is an open-source engine for Sierra's *Pharaoh* (1999) and its expansion
*Cleopatra: Queen of the Nile*, written in Rust. It plays the original campaign and
custom maps from your own copy of the game's data files, with the original art, text,
sound and rules.

It is a new implementation started from scratch, not a fork of any existing project.
The aim is to be faithful to the original game and efficient: the simulation is a
plain library with no global state and a seeded random number generator, so a
scripted run gives the same result every time, and the renderer packs every sprite into
GPU texture atlases and draws the city in a few batched calls.

## Download

| | Latest release | Snapshot (every commit) |
|---|---|---|
| macOS (Apple Silicon) | [osiris-macos-arm64.zip](../../releases/latest/download/osiris-macos-arm64.zip) | [osiris-macos-arm64.zip](../../releases/download/snapshot/osiris-macos-arm64.zip) |
| Windows x64 | [osiris-windows-x64.zip](../../releases/latest/download/osiris-windows-x64.zip) | [osiris-windows-x64.zip](../../releases/download/snapshot/osiris-windows-x64.zip) |
| Linux x64 | [osiris-linux-x64.tar.gz](../../releases/latest/download/osiris-linux-x64.tar.gz) | [osiris-linux-x64.tar.gz](../../releases/download/snapshot/osiris-linux-x64.tar.gz) |

All versions are on the [releases page](../../releases). You also need the original
game data; see [Playing](#playing).

## Status

Playable but unfinished. Expect bugs, and expect things that differ from the original.

What is in:

- the campaign and custom maps, with briefings, goals, winning and losing, and saved games
- housing and immigration, water, food, farming and the Nile's flood, industry, storage
  yards, bazaars, trade by land and river, taxes and the treasury
- religion and festivals, health and plague, crime, entertainment, education, fire and
  collapse, the four ratings and all the overseers
- scenario events: Pharaoh's requests, gifts, price and wage changes, floods and
  invasions, with the original messages
- the army and navy: recruiters, forts and companies with their orders, the military
  academy, walls, towers, gatehouses, warships and transports, and land and sea invasions
- monuments: mastabas, every pyramid family, obelisks and the Sphinx, built phase by phase
  by laborers and guild craftsmen with materials dragged from storage, and burial
  provisions for tombs

What is missing or incomplete: tomb robbers, predators such as crocodiles and hippos,
and the original's cheats. Some numbers the original never documented are placeholders, and the code
says so where they are.

### Game rules

The in-game Rules window (F2) can switch off gods, floods, disasters, fire, disease and
building collapse, or share workers across the whole city instead of making buildings
find them. By default everything plays as in the original.

## Playing

You need the original game data. The GOG release, *Pharaoh + Cleopatra*, works. Osiris
looks for a folder named `PharaohData` holding the game's `Data/` folder, maps and
`mission1.pak`. It checks, in order:

1. next to the Osiris program, or next to `Osiris.app` on macOS
2. your user folder for Osiris: `~/Library/Application Support/Osiris` on macOS,
   `%APPDATA%\Osiris` on Windows, `~/.local/share/osiris` on Linux
3. the current directory

Or pass it directly: `osiris --data /path/to/PharaohData`.

Builds for macOS (Apple Silicon), Windows and Linux are linked under
[Download](#download). They are unsigned.
On macOS, right-click `Osiris.app` and choose Open the first time.

## Building

With a recent stable Rust toolchain:

```
cargo build --release
cargo run --release -p osiris-app -- --data /path/to/PharaohData
```

On Linux you also need the ALSA development package (`libasound2-dev` on Debian and
Ubuntu). `packaging/macos/bundle.sh` builds `dist/Osiris.app`.

The game can run headless for testing. A script of build and inspect steps runs
against a map, and the result is saved as a screenshot:

```
osiris --mission 3 --script "road 70,76 100,76; build 10 72,77; ticks 5000; report" --screenshot out.png
```

`osiris-tools` inspects the data files: it can dump sprite packs to PNG, print text
groups and messages, and parse every map and campaign mission.

## Layout

- `crates/osiris-formats`: readers for the original data files (sprites, maps, saves,
  text, the campaign and model files)
- `crates/osiris-sim`: the simulation, with the game's tables in `data/`
- `crates/osiris-render`: the sprite renderer (wgpu)
- `crates/osiris-ui`: fonts, panels and message windows in the original style
- `crates/osiris-audio`: music, speech and city sounds
- `crates/osiris-app`: the game
- `crates/osiris-tools`: command-line inspection tools

## Credits

Osiris stands on the work of others who kept these games alive.

[dalerank](https://github.com/dalerank) created
[Akhenaten](https://github.com/dalerank/Akhenaten), the open-source reimplementation of
*Pharaoh* that has done more than any other project to document how the game works.
Before that came [CaesarIA](https://github.com/dalerank/caesaria-game), a remake of
*Caesar III*. Many facts in Osiris's data tables (building and walker types, image
groups, event and enemy ids, balance constants) were read from Akhenaten and are
credited in each file's header. No code was copied from it: Osiris is an independent
program with its own design, written fresh in Rust to try to make everything efficient.

[Julius](https://github.com/bvschaik/julius) and
[Augustus](https://github.com/Keriew/augustus) documented the file formats and
mechanics of *Caesar III*, which *Pharaoh* shares much with.

*Pharaoh* and *Cleopatra* were made by Impressions Games and published by Sierra.
They are trademarks of their owners. This project is not affiliated with them and
includes none of the game's data.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
