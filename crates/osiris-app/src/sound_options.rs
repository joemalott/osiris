//! Sound options, as the original's window (Options > Sound, text group 46): music,
//! speech, sound effects and city sounds, each switched on or off by its button and
//! set between quiet and loud with a pair of arrows. OK keeps the changes, Cancel
//! puts back what was set when the window opened. The settings are the player's,
//! kept in the user folder (`sound.txt`) for every game, and the window also opens
//! from the family menu (the original only has it in the city).

use crate::widgets::{Ui, UiImages};
use osiris_audio::Audio;
use osiris_render::Renderer;
use osiris_formats::TextTable;
use osiris_ui::{Font, PanelImages};
use std::path::Path;

/// One channel: on or off, and its volume in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel {
    pub on: bool,
    pub volume: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundPrefs {
    pub music: Channel,
    pub speech: Channel,
    pub effects: Channel,
    pub city: Channel,
}

impl Default for SoundPrefs {
    /// Music starts well below the rest, which it otherwise drowns.
    fn default() -> Self {
        let on = |volume| Channel { on: true, volume };
        Self { music: on(35), speech: on(100), effects: on(80), city: on(80) }
    }
}

const FILE: &str = "sound.txt";

impl SoundPrefs {
    /// The saved settings (lines `music on 35`), or the defaults.
    pub fn load(user_dir: &Path) -> Self {
        let mut p = Self::default();
        let Ok(text) = std::fs::read_to_string(user_dir.join(FILE)) else { return p };
        for line in text.lines() {
            let mut it = line.split_whitespace();
            let (Some(name), Some(on), Some(vol)) = (it.next(), it.next(), it.next()) else { continue };
            let Ok(volume) = vol.parse::<i32>() else { continue };
            let c = Channel { on: on == "on", volume: volume.clamp(0, 100) };
            match name {
                "music" => p.music = c,
                "speech" => p.speech = c,
                "effects" => p.effects = c,
                "city" => p.city = c,
                _ => {}
            }
        }
        p
    }

    pub fn save(&self, user_dir: &Path) {
        let line = |name: &str, c: Channel| format!("{name} {} {}\n", if c.on { "on" } else { "off" }, c.volume);
        let text = line("music", self.music) + &line("speech", self.speech) + &line("effects", self.effects) + &line("city", self.city);
        let _ = std::fs::write(user_dir.join(FILE), text);
    }

    /// Sets the players' volumes; a channel switched off plays silent.
    pub fn apply(&self, audio: &Audio) {
        let level = |c: Channel| if c.on { c.volume as f32 / 100.0 } else { 0.0 };
        audio.set_music_volume(level(self.music));
        audio.set_speech_volume(level(self.speech));
        audio.set_effects_volume(level(self.effects));
        audio.set_city_volume(level(self.city));
    }
}

/// What the window asks of its owner after a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Open,
    /// Closed with OK: store the settings.
    Keep,
    /// Closed with Cancel: the settings are back as they were.
    Cancelled,
}

/// The open window, holding the settings it opened with for Cancel.
pub struct SoundWindow {
    before: SoundPrefs,
}

/// Window size in 16-pixel panel blocks.
const W: i32 = 24;
const H: i32 = 18;

impl SoundWindow {
    pub fn new(prefs: SoundPrefs) -> Self {
        Self { before: prefs }
    }

    /// The settings the window opened with.
    pub fn before(&self) -> SoundPrefs {
        self.before
    }

    /// Draws the window centred on the screen and handles `click`. Changes apply to
    /// `prefs` (and the sound) at once, so the player hears them.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, r: &mut Renderer, panels: &PanelImages, img: UiImages, text: &TextTable, cursor: [f32; 2], click: Option<[f32; 2]>, prefs: &mut SoundPrefs, audio: Option<&Audio>) -> Outcome {
        let (w, h) = (W as f32 * 16.0, H as f32 * 16.0);
        let x = ((r.screen[0] - w) / 2.0).floor();
        let y = ((r.screen[1] - h) / 2.0).floor();
        osiris_ui::panel::outer_panel(r, panels, x, y, W, H);
        let mut ui = Ui { r, panels, img, text, cursor, click };
        let t = ui.t(46, 0);
        ui.centred(Font::LargeBlackOnLight, &t, x, y + 14.0, w);
        let t = ui.t(46, 10);
        ui.label(Font::SmallPlain, &t, x + 16.0, y + 48.0);
        let t = ui.t(46, 11);
        ui.label(Font::SmallPlain, &t, x + 280.0, y + 48.0);
        let before = *prefs;
        // Each row: its button's "off"/"on" texts (46, 1-8) and the channel.
        let rows: [(usize, &mut Channel); 4] = [(1, &mut prefs.music), (3, &mut prefs.speech), (5, &mut prefs.effects), (7, &mut prefs.city)];
        for (i, (label, c)) in rows.into_iter().enumerate() {
            let ry = y + 68.0 + 30.0 * i as f32;
            let t = ui.t(46, label + c.on as usize);
            if ui.button([x + 16.0, ry, 224.0, 20.0], &t, Font::NormalBlackOnLight) {
                c.on = !c.on;
            }
            if ui.arrow(x + 264.0, ry, false) {
                c.volume = (c.volume - 5).max(0);
            }
            if ui.arrow(x + 288.0, ry, true) {
                c.volume = (c.volume + 5).min(100);
            }
            ui.label(Font::SmallPlain, &format!("{}%", c.volume), x + 326.0, ry + 4.0);
        }
        let ok = [x + w / 2.0 - 40.0, y + h - 40.0 - 12.0, 34.0, 34.0];
        let cancel = [x + w / 2.0 + 20.0, y + h - 40.0 - 12.0, 34.0, 34.0];
        let keep = ui.image_button(img.ok_cancel + ui.hot(ok) as u32, ok[0], ok[1], ok[2], ok[3]);
        let back = ui.image_button(img.ok_cancel + 4 + ui.hot(cancel) as u32, cancel[0], cancel[1], cancel[2], cancel[3]);
        if back {
            *prefs = self.before;
        }
        if (*prefs != before || back)
            && let Some(a) = audio
        {
            prefs.apply(a);
            a.play_effect("BUTTON.WAV");
        }
        if keep {
            Outcome::Keep
        } else if back {
            Outcome::Cancelled
        } else {
            Outcome::Open
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_save_and_load() {
        let dir = std::env::temp_dir().join(format!("osiris-sound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(SoundPrefs::load(&dir), SoundPrefs::default());
        let mut p = SoundPrefs::default();
        p.music = Channel { on: false, volume: 20 };
        p.city.volume = 55;
        p.save(&dir);
        assert_eq!(SoundPrefs::load(&dir), p);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
