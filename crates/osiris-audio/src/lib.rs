//! Music, sound effects, speech and city ambience playback for Osiris.
//!
//! [`Audio`] is the single entry point: it owns every rodio handle the crate needs (the
//! output device, a music player, a speech player, and one looping player per active city
//! ambience channel) and exposes plain methods for the game loop to call. There is no global
//! or process-wide audio state; everything lives on the `Audio` value the caller holds.
//!
//! If no audio output device is available, [`Audio::new`] still succeeds: it returns an
//! `Audio` whose methods are all silent no-ops, so callers do not need to special-case
//! "no sound card" environments (headless builds, CI, etc.).
//!
//! ```no_run
//! use std::path::Path;
//! use osiris_audio::{Audio, Track};
//!
//! let audio = Audio::new(Path::new("PharaohData")).expect("audio init");
//! audio.play_music(&Track::new("Ra"));
//! audio.update_music(1200); // called periodically with the current city population
//! audio.play_effect("BUTTON.WAV");
//! audio.set_city_sounds(&[(46, 0.8)]); // building id 46 = apothecary
//! ```

mod city_sounds;
pub mod error;
mod music;
#[cfg(test)]
mod test_support;

pub use error::{Error, Result};
pub use music::Track;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rodio::Source as _;

/// Entry point for all audio playback: music, one-shot effects, speech and looping city
/// ambience. See the [module docs](crate) for an overview.
///
/// Cheap to pass around by `&Audio`; every method takes `&self`.
/// The music's starting volume, below effects and speech.
pub const DEFAULT_MUSIC_VOLUME: f32 = 0.35;

pub struct Audio {
    inner: Option<Inner>,
}

struct Inner {
    game_dir: PathBuf,
    // Kept alive only to keep the output stream open; dropping it stops all playback.
    _device: rodio::MixerDeviceSink,
    mixer: rodio::mixer::Mixer,
    music_player: rodio::Player,
    speech_player: rodio::Player,

    music_rows: Vec<music::MusicRow>,
    music_current: Mutex<Option<String>>,

    city_sound_table: HashMap<u32, city_sounds::ChannelSound>,
    city_channels: Mutex<HashMap<u32, rodio::Player>>,

    effects_volume: Mutex<f32>,
}

impl Audio {
    /// Opens the default audio output device and prepares playback rooted at `game_dir`
    /// (the original game's data directory, e.g. `PharaohData`, containing `music.txt` and
    /// an `AUDIO/` folder with `Music/`, `Ambient/`, `Wavs/` and `Voice/`).
    ///
    /// Never fails because a sound device is unavailable: in that case this returns `Ok` with
    /// an `Audio` that silently does nothing. It can fail if the crate's own embedded
    /// `city_sounds.toml` cannot be parsed, which would indicate a bug in this crate rather
    /// than a problem with `game_dir`.
    pub fn new(game_dir: &Path) -> Result<Audio> {
        let device = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(device) => device,
            Err(_) => return Ok(Audio { inner: None }),
        };

        let mixer = device.mixer().clone();
        let music_player = rodio::Player::connect_new(&mixer);
        // Full-volume music drowns the city's sounds; start it well below them.
        music_player.set_volume(DEFAULT_MUSIC_VOLUME);
        let speech_player = rodio::Player::connect_new(&mixer);

        let city_sound_table =
            city_sounds::parse(city_sounds::EMBEDDED_TOML).map_err(|source| Error::Config {
                what: "data/city_sounds.toml",
                source,
            })?;

        let music_rows = std::fs::read_to_string(game_dir.join("music.txt"))
            .map(|text| music::parse_music_txt(&text))
            .unwrap_or_else(|_| {
                eprintln!(
                    "osiris-audio: no music.txt under {game_dir:?}, city music selection disabled"
                );
                Vec::new()
            });

        Ok(Audio {
            inner: Some(Inner {
                game_dir: game_dir.to_path_buf(),
                _device: device,
                mixer,
                music_player,
                speech_player,
                music_rows,
                music_current: Mutex::new(None),
                city_sound_table,
                city_channels: Mutex::new(HashMap::new()),
                effects_volume: Mutex::new(1.0),
            }),
        })
    }

    /// Plays `track` as the current music, replacing whatever was playing.
    pub fn play_music(&self, track: &Track) {
        let Some(inner) = &self.inner else { return };
        let mut current = inner.music_current.lock().unwrap();
        inner.start_music_locked(track.filename(), &mut current);
    }

    /// Call periodically (e.g. once a game tick) with the current city population. Picks city
    /// music the way the original engine does: keeps the current tune playing as long as it
    /// still falls within its `music.txt` population bracket, otherwise picks a new track at
    /// random from the brackets that match `population`. See the [`music`] module docs for
    /// the exact rule and its known simplifications versus the original per-mission behavior.
    pub fn update_music(&self, population: i32) {
        let Some(inner) = &self.inner else { return };
        if inner.music_rows.is_empty() {
            return;
        }

        let mut current = inner.music_current.lock().unwrap();
        let still_fits = current
            .as_deref()
            .is_some_and(|file| music::track_fits(&inner.music_rows, file, population));
        if still_fits && !inner.music_player.empty() {
            return;
        }

        if let Some(row) = music::pick_track(&inner.music_rows, population, current.as_deref()) {
            let filename = row.file.clone();
            inner.start_music_locked(&filename, &mut current);
        }
    }

    /// Stops music playback immediately.
    pub fn stop_music(&self) {
        let Some(inner) = &self.inner else { return };
        inner.music_player.stop();
        *inner.music_current.lock().unwrap() = None;
    }

    /// Sets the music playback volume (`0.0` silent .. `1.0` full).
    pub fn set_music_volume(&self, volume: f32) {
        if let Some(inner) = &self.inner {
            inner.music_player.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    /// Sets the volume applied to future [`Audio::play_effect`] calls (`0.0` .. `1.0`).
    pub fn set_effects_volume(&self, volume: f32) {
        if let Some(inner) = &self.inner {
            *inner.effects_volume.lock().unwrap() = volume.clamp(0.0, 1.0);
        }
    }

    /// Sets the speech/briefing playback volume (`0.0` .. `1.0`).
    pub fn set_speech_volume(&self, volume: f32) {
        if let Some(inner) = &self.inner {
            inner.speech_player.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    /// Plays a one-shot UI/effect sound (button clicks, building placement, etc). Several can
    /// overlap; each call is independent.
    ///
    /// `name_or_path` is resolved as follows:
    /// - An absolute path is used as-is.
    /// - A path with more than one component (e.g. `"Ambient/HOUSING1.MP3"`) is resolved
    ///   relative to `AUDIO/`.
    /// - A bare filename (e.g. `"BUTTON.WAV"`, the UI click sound; `"BUILD.WAV"`, building
    ///   placement; `"NO.WAV"`, invalid action) is resolved relative to `AUDIO/Wavs/`, where
    ///   the original ships its UI sounds.
    pub fn play_effect(&self, name_or_path: impl AsRef<Path>) {
        let Some(inner) = &self.inner else { return };
        inner.play_effect(name_or_path.as_ref());
    }

    /// Plays a mission-briefing voice line. `path` is relative to `AUDIO/Voice/` (e.g.
    /// `"Mission/01/briefing.wav"`), or an absolute path. Replaces any speech currently
    /// playing.
    pub fn play_speech(&self, path: impl AsRef<Path>) {
        let Some(inner) = &self.inner else { return };
        inner.play_speech(path.as_ref());
    }

    /// Reports which building-type looping ambience channels should currently be audible.
    ///
    /// `active` is a list of `(building_type_id, volume)` pairs, where `building_type_id`
    /// matches the `id` field in `crates/osiris-sim/data/buildings.toml` (see
    /// `data/city_sounds.toml` for the id -> file mapping) and `volume` is `0.0..=1.0` (the
    /// caller decides how loud, e.g. based on how many of that building are visible). Building
    /// ids missing from the list, or given a volume of `0.0` or less, have their ambience
    /// stopped. Calling this again with an id that is already playing just updates its volume.
    pub fn set_city_sounds(&self, active: &[(u32, f32)]) {
        let Some(inner) = &self.inner else { return };
        inner.set_city_sounds(active);
    }
}

impl Inner {
    fn audio_path(&self, sub: impl AsRef<Path>) -> PathBuf {
        self.game_dir.join("AUDIO").join(sub.as_ref())
    }

    fn start_music_locked(&self, filename: &str, current: &mut Option<String>) {
        let path = self.audio_path(Path::new("Music").join(filename));
        match open_source(&path) {
            Ok(source) => {
                self.music_player.stop();
                self.music_player.append(source);
                *current = Some(filename.to_string());
            }
            Err(err) => eprintln!("osiris-audio: could not play music {path:?}: {err}"),
        }
    }

    fn resolve_effect_path(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            return p.to_path_buf();
        }
        let has_subdir = p
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
        if has_subdir {
            self.audio_path(p)
        } else {
            self.audio_path(Path::new("Wavs").join(p))
        }
    }

    fn play_effect(&self, name_or_path: &Path) {
        let path = self.resolve_effect_path(name_or_path);
        match open_source(&path) {
            Ok(source) => {
                let volume = *self.effects_volume.lock().unwrap();
                self.mixer.add(source.amplify(volume));
            }
            Err(err) => eprintln!("osiris-audio: could not play effect {path:?}: {err}"),
        }
    }

    fn play_speech(&self, path: &Path) {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.audio_path(Path::new("Voice").join(path))
        };
        match open_source(&full) {
            Ok(source) => {
                self.speech_player.stop();
                self.speech_player.append(source);
            }
            Err(err) => eprintln!("osiris-audio: could not play speech {full:?}: {err}"),
        }
    }

    fn set_city_sounds(&self, active: &[(u32, f32)]) {
        let mut channels = self.city_channels.lock().unwrap();

        for &(id, volume) in active {
            if volume <= 0.0 {
                continue;
            }
            if let Some(player) = channels.get(&id) {
                player.set_volume(volume.clamp(0.0, 1.0));
                continue;
            }
            let Some(entry) = self.city_sound_table.get(&id) else {
                continue;
            };
            let path = self.audio_path(&entry.file);
            match open_looped_source(&path) {
                Ok(source) => {
                    let player = rodio::Player::connect_new(&self.mixer);
                    player.set_volume(volume.clamp(0.0, 1.0));
                    player.append(source);
                    channels.insert(id, player);
                }
                Err(err) => {
                    eprintln!("osiris-audio: could not play city sound for building {id}: {err}")
                }
            }
        }

        channels.retain(|id, player| {
            let keep = active
                .iter()
                .any(|&(active_id, volume)| active_id == *id && volume > 0.0);
            if !keep {
                player.stop();
            }
            keep
        });
    }
}

type FileDecoder = rodio::Decoder<std::io::BufReader<std::fs::File>>;
type FileLoopedDecoder = rodio::decoder::LoopedDecoder<std::io::BufReader<std::fs::File>>;

fn open_source(path: &Path) -> Result<FileDecoder> {
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    rodio::Decoder::try_from(file).map_err(|source| Error::Decode {
        path: path.to_path_buf(),
        source,
    })
}

fn open_looped_source(path: &Path) -> Result<FileLoopedDecoder> {
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = std::io::BufReader::new(file);
    rodio::Decoder::new_looped(reader).map_err(|source| Error::Decode {
        path: path.to_path_buf(),
        source,
    })
}
