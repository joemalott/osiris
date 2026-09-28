//! Sound options, as the original's window (Options > Sound, text group 46): music,
//! speech, sound effects and city sounds, each switched on or off by its button and
//! set between quiet and loud with a pair of arrows. Unlike the original there is
//! no Cancel: every change is heard and saved at once, and OK (or a click outside,
//! or a right-click) closes the window. The settings are the player's, kept in the
//! user folder (`sound.txt`) for every game, and the window also opens from the
//! family menu (the original only has it in the city). A fifth button, not in the
//! original, turns the movies off (movie.rs).

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
    /// Whether the intro and closing movies play.
    pub movies: bool,
}

impl Default for SoundPrefs {
    /// Quiet to start with; music lowest, as it otherwise drowns the rest.
    fn default() -> Self {
        let on = |volume| Channel { on: true, volume };
        Self { music: on(20), speech: on(60), effects: on(40), city: on(40), movies: true }
    }
}

const FILE: &str = "sound.txt";

impl SoundPrefs {
    /// The saved settings (lines `music on 35`, and `movies on`), or the defaults.
    pub fn load(user_dir: &Path) -> Self {
        let mut p = Self::default();
        let Ok(text) = std::fs::read_to_string(user_dir.join(FILE)) else { return p };
        for line in text.lines() {
            let mut it = line.split_whitespace();
            if let (Some("movies"), Some(on)) = (it.clone().next(), it.clone().nth(1)) {
                p.movies = on == "on";
                continue;
            }
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
        let text = line("music", self.music) + &line("speech", self.speech) + &line("effects", self.effects) + &line("city", self.city) + if self.movies { "movies on\n" } else { "movies off\n" };
        let _ = std::fs::write(user_dir.join(FILE), text);
    }

    /// The movies' volume: their narration is speech, so the speech setting's.
    pub fn movie_volume(&self) -> f32 {
        if self.speech.on { self.speech.volume as f32 / 100.0 } else { 0.0 }
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
    /// The settings changed: store them.
    Changed,
    Closed,
}

/// The open window.
pub struct SoundWindow;

/// Window size in 16-pixel panel blocks.
const W: i32 = 24;
const H: i32 = 18;

impl SoundWindow {
    /// Draws the window centred on the screen and handles `click`. Changes apply to
    /// `prefs` (and the sound) at once, so the player hears them; a click outside the
    /// window closes it.
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
        ui.label(Font::NormalBlackOnLight, &t, x + 16.0, y + 48.0);
        let t = ui.t(46, 11);
        ui.label(Font::NormalBlackOnLight, &t, x + 280.0, y + 48.0);
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
            ui.label(Font::NormalBlackOnLight, &format!("{}%", c.volume), x + 326.0, ry + 4.0);
        }
        // The movies have no text of their own in the original's tables.
        let t = if prefs.movies { crate::lang::tr("Movies are on") } else { crate::lang::tr("Movies are off") };
        if ui.button([x + 16.0, y + 68.0 + 30.0 * 4.0, 224.0, 20.0], t, Font::NormalBlackOnLight) {
            prefs.movies = !prefs.movies;
        }
        let ok = [x + (w - 34.0) / 2.0, y + h - 52.0, 34.0, 34.0];
        let close = ui.image_button(img.ok_cancel + ui.hot(ok) as u32, ok[0], ok[1], ok[2], ok[3]);
        let outside = ui.click.is_some_and(|[cx, cy]| cx < x || cy < y || cx >= x + w || cy >= y + h);
        let changed = *prefs != before;
        if changed && let Some(a) = audio {
            prefs.apply(a);
            a.play_effect("BUTTON.WAV");
        }
        if close || outside {
            Outcome::Closed
        } else if changed {
            Outcome::Changed
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
        p.movies = false;
        p.save(&dir);
        assert_eq!(SoundPrefs::load(&dir), p);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
