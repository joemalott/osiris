# Osiris

An open-source engine for Sierra's *Pharaoh* (1999) and *Cleopatra: Queen of the Nile*,
written in Rust. It plays the original game from your own copy of the data files.

Osiris is a new implementation, not a fork. The file formats are read from scratch, and the
simulation is a deterministic library with no global state, so headless runs and tests
behave identically every time.

## Status

Early. What works today:

- every `.sg3`/`.555` sprite pack decodes (plain, RLE, isometric footprint + top, mirrored)
- every scenario `.map` and all 53 campaign missions in `mission1.pak` load, including the
  PKWare-compressed chunks

## Game data

You need the original game (the GOG "Pharaoh Gold" release works). Point Osiris at the
install directory, the one containing `Data/`, `Maps/` and `mission1.pak`. By default it
looks for `PharaohData/` next to the workspace.

## Building

```
cargo build --release
cargo run --release -p osiris-tools -- check-sg3 PharaohData/Data
cargo run --release -p osiris-tools -- check-maps PharaohData
cargo run --release -p osiris-tools -- dump-sprites PharaohData/Data Pharaoh_General out/
```

## Layout

- `crates/osiris-formats` readers for the original data files
- `crates/osiris-sim` the simulation
- `crates/osiris-app` the game
- `crates/osiris-tools` command-line inspection tools

## License

GPL-3.0-or-later. Format knowledge was cross-checked against
[Julius](https://github.com/bvschaik/julius) and [Akhenaten](https://github.com/dalerank/Akhenaten);
no code was copied from them. *Pharaoh* is a trademark of its owners, and no game data is
included here.
