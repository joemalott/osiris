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

You need the original game data. The GOG or Steam release, *Pharaoh + Cleopatra*, works,
as does the original CD install. Osiris looks for the Pharaoh install (the folder
holding the game's `Data/` folder, maps and `mission1.pak`):

1. the folder you picked before
2. the folder Osiris sits in, or a `PharaohData` folder next to it (next to `Osiris.app`
   on macOS)
3. a `PharaohData` folder in your user folder for Osiris: `~/Library/Application
   Support/Osiris` on macOS, `%APPDATA%\Osiris` on Windows, `~/.local/share/osiris` on Linux
4. the usual GOG and Steam install folders

If it finds none, it asks you to choose the Pharaoh folder and remembers it.

Or pass it directly: `osiris --data /path/to/PharaohData`.

Builds for macOS (Apple Silicon), Windows and Linux are linked under
[Download](#download). They are unsigned.
On macOS, right-click `Osiris.app` and choose Open the first time. On Windows,
SmartScreen warns about an unrecognised app: click More info, then Run anyway.

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

Without `--screenshot` the script runs on the city and the game opens on it, paused,
to play on. `save NAME` writes the city into the current family's saves, where Load
Saved Game lists it.

### A test megacity

`megacity S [R]` lays out a big city from the map with the game's own build commands:
floodplain farms with work camps and granaries, meadow farms with a water lift and
ditches, workshops and storage yards, and blocks of housing and services (seed `S`,
region `R` tiles each way from its centre, 60 by default). Of the campaign maps tried,
mission 39 (Tut in the Valley) gives it the most floodplain: 97 farms, and the city
grows to about 9,000 people in six years.
Mission 23 (Thinis) is smaller, 40 floodplain farms and about 8,000 people, but has
fertile meadow, so it also shows meadow farms watered by a water lift and ditches.
`allowall` lets the city use every building, `noinvasions` keeps the raids away, and
`safe` turns off fire and collapse while nothing is staffed yet (`unsafe` turns them
back on). A year is 9,792 ticks, and `status` prints people, walkers, houses, the flood,
the farms and the food.

Build it and play it (the game opens paused on the city):

```
osiris --mission 39 --script "allowall; noinvasions; safe; megacity 1"
```

Build it, run it five years, and save it as "Test megacity" for Load Saved Game:

```
osiris --mission 39 --script "allowall; noinvasions; safe; megacity 1; ticks 48960; save Test megacity" --screenshot megacity.png
```

Benchmark it: a year of ticks on the grown city, then 30 frames at 4K.

```
OSIRIS_BENCH_FRAMES=30 osiris --mission 39 --size 3840x2160 --script "allowall; noinvasions; safe; megacity 1; ticks 48960; timed 9792; status; zoom 0.5" --screenshot megacity-4k.png
```

`osiris-tools` inspects the data files: it can dump sprite packs to PNG, print text
groups and messages, and parse every map and campaign mission.

Games can be recorded and replayed. Every city is recorded while it is played (its
starting save, each command with its tick, and a hash of the city at the end of each
month); `--record FILE` keeps the last one, and after a crash the game so far is
written to `crash.osiris-replay` beside `osiris.log`. A script records with its
`record FILE` step. `osiris --replay FILE` plays a recording back in the window;
with `--screenshot` it runs headless and says whether the city came out the same or
in which month it first differed, as `osiris-tools replay <game dir> FILE...` does.
The recordings in `crates/osiris-sim/tests/replays` are regression tests; they need
the game data, so they don't run in CI:

```
cargo test --release -p osiris-sim --test replays -- --ignored
```

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
