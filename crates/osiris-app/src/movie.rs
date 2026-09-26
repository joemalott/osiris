//! The original's Bink movies (`BINKS/High/*.bik`), played full window: the picture
//! scaled to fit with black bars, the sound through the audio device, the pictures
//! following the sound's clock. A click or Escape, Space or Enter skips the movie.
//! A movie missing from the install (or one that won't open) is skipped without a
//! word, and the player can turn movies off (Options > Sound).

use osiris_audio::{Audio, StreamSound};
use osiris_render::{DynamicHandle, Renderer, Space};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

/// The movies, by the names the original gives their files.
pub const INTRO: &str = "Intro_big";
pub const WIN_GRAND: &str = "win_grand_big";

/// Key passed to `Renderer::upload_dynamic` for the movie's picture.
const TEXTURE_KEY: u32 = 0x4249_4b00;

/// Frames decoded ahead of the one on screen.
const AHEAD: usize = 6;

/// The movie file called `name`, if the install has it.
pub fn path(data: &Path, name: &str) -> Option<PathBuf> {
    let dir = data.join("BINKS").join("High");
    let exact = dir.join(format!("{name}.bik"));
    if exact.is_file() {
        return Some(exact);
    }
    // Installs differ in the case of their file names.
    let want = format!("{name}.bik").to_lowercase();
    std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().to_lowercase() == want))
}

enum Decoded {
    Frame(usize, Vec<u8>),
    End,
}

pub struct Movie {
    width: u32,
    height: u32,
    fps: f64,
    frames: usize,
    rx: Receiver<Decoded>,
    /// The next frame, decoded but not yet due.
    next: Option<(usize, Vec<u8>)>,
    /// The frame on screen, and whether it still needs uploading.
    shown: Option<usize>,
    picture: Vec<u8>,
    dirty: bool,
    handle: Option<DynamicHandle>,
    decoded_all: bool,
    sound: Option<StreamSound>,
    /// When the movie started, the clock without sound.
    start: Instant,
    pub done: bool,
}

impl Movie {
    /// Opens movie `name` from the game data and starts it (its sound on `audio` at
    /// `volume`), or `None` if it isn't there.
    pub fn start(data: &Path, name: &str, audio: Option<&Audio>, volume: f32) -> Option<Movie> {
        let path = path(data, name)?;
        let movie = match osiris_bink::Movie::open(&path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("movie {}: {e}", path.display());
                return None;
            }
        };
        let sound = match (audio, movie.audio_format()) {
            (Some(a), Some((rate, channels))) => Some(a.play_stream(rate, channels, volume)),
            _ => None,
        };
        Some(Movie::run(name, movie, sound))
    }

    /// Opens movie `name` without sound, for screenshots.
    pub fn silent(data: &Path, name: &str) -> Option<Movie> {
        let movie = osiris_bink::Movie::open(path(data, name)?).ok()?;
        Some(Movie::run(name, movie, None))
    }

    fn run(name: &str, mut movie: osiris_bink::Movie<std::io::BufReader<std::fs::File>>, sound: Option<StreamSound>) -> Movie {
        let (width, height, fps, frames) = (movie.width(), movie.height(), movie.fps(), movie.frames() as usize);
        let (tx, rx) = sync_channel(AHEAD);
        let feed = sound.as_ref().map(|s| s.feed());
        let label = name.to_owned();
        // Decoding runs ahead on its own thread; it ends when the movie does or when
        // the screen drops the receiver (a skip).
        let _ = std::thread::Builder::new().name("movie".into()).spawn(move || {
            let mut samples = Vec::new();
            loop {
                samples.clear();
                let index = movie.position();
                match movie.next_frame(&mut samples) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(e) => {
                        log::warn!("movie {label} frame {index}: {e}");
                        break;
                    }
                }
                if let Some(f) = &feed {
                    if f.stopped() {
                        return;
                    }
                    f.push(&samples);
                }
                let mut rgba = vec![0; width as usize * height as usize * 4];
                movie.to_rgba(&mut rgba);
                if tx.send(Decoded::Frame(index, rgba)).is_err() {
                    return;
                }
            }
            if let Some(f) = &feed {
                f.finish();
            }
            let _ = tx.send(Decoded::End);
        });
        Movie {
            width,
            height,
            fps,
            frames,
            rx,
            next: None,
            shown: None,
            picture: Vec::new(),
            dirty: false,
            handle: None,
            decoded_all: false,
            sound,
            start: Instant::now(),
            done: false,
        }
    }

    /// How far the movie has played: the sound's position when it is heard,
    /// otherwise the time since it started.
    fn clock(&self) -> Duration {
        self.sound.as_ref().and_then(|s| s.position()).unwrap_or_else(|| self.start.elapsed())
    }

    /// Takes the frames that are due by `t`, keeping the latest to show.
    fn advance_to(&mut self, t: Duration) {
        let due = (t.as_secs_f64() * self.fps) as usize;
        loop {
            if let Some((i, _)) = &self.next {
                if *i > due {
                    break;
                }
                let (i, rgba) = self.next.take().unwrap();
                self.shown = Some(i);
                self.picture = rgba;
                self.dirty = true;
                continue;
            }
            match self.rx.try_recv() {
                Ok(Decoded::Frame(i, rgba)) => self.next = Some((i, rgba)),
                Ok(Decoded::End) | Err(TryRecvError::Disconnected) => {
                    self.decoded_all = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        // Over once the last frame has had its time on screen.
        if self.decoded_all && self.next.is_none() && t.as_secs_f64() >= self.frames as f64 / self.fps {
            self.done = true;
        }
    }

    pub fn update(&mut self) {
        let t = self.clock();
        self.advance_to(t);
    }

    /// Decodes up to `seconds` in, for a screenshot of that moment.
    pub fn seek_blocking(&mut self, seconds: f64) {
        let target = (seconds * self.fps) as usize;
        while self.shown.is_none_or(|s| s < target) && !self.decoded_all {
            match self.rx.recv() {
                Ok(Decoded::Frame(i, rgba)) => {
                    self.shown = Some(i);
                    self.picture = rgba;
                    self.dirty = true;
                }
                _ => self.decoded_all = true,
            }
        }
    }

    pub fn skip(&mut self) {
        self.done = true;
    }

    /// The picture's place on a screen: as large as fits, keeping its shape, centred.
    pub fn fit(&self, screen: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let (w, h) = (self.width as f32, self.height as f32);
        let k = (screen[0] / w).min(screen[1] / h);
        let size = [w * k, h * k];
        ([((screen[0] - size[0]) / 2.0).round(), ((screen[1] - size[1]) / 2.0).round()], size)
    }

    pub fn draw(&mut self, r: &mut Renderer) {
        r.rect([0.0, 0.0], r.screen, [0.0, 0.0, 0.0, 1.0], Space::Screen);
        if self.shown.is_none() {
            return;
        }
        if self.dirty || self.handle.is_none() {
            self.handle = Some(r.upload_dynamic(TEXTURE_KEY, self.width, self.height, &self.picture));
            self.dirty = false;
        }
        let Some(handle) = self.handle else { return };
        let (pos, size) = self.fit(r.screen);
        let smooth = std::mem::replace(&mut r.smooth, true);
        r.dynamic_image(handle, pos, size, Space::Screen);
        r.smooth = smooth;
    }
}

/// Whether a key press skips a movie: any key, as in the original (FUN_00413690),
/// but the modifiers and the full screen switches (F11, Alt+Enter, Ctrl+Cmd+F).
pub fn skips(code: winit::keyboard::KeyCode, alt: bool, ctrl_cmd: bool) -> bool {
    use winit::keyboard::KeyCode as K;
    let modifier = matches!(code, K::AltLeft | K::AltRight | K::ControlLeft | K::ControlRight | K::SuperLeft | K::SuperRight | K::ShiftLeft | K::ShiftRight);
    let fullscreen = code == K::F11 || (alt && matches!(code, K::Enter | K::NumpadEnter)) || (ctrl_cmd && code == K::KeyF);
    !modifier && !fullscreen
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode as K;

    #[test]
    fn any_key_skips_but_the_full_screen_switch() {
        assert!(skips(K::Escape, false, false));
        assert!(skips(K::KeyA, false, false));
        assert!(skips(K::Enter, false, false));
        assert!(!skips(K::Enter, true, false));
        assert!(!skips(K::F11, false, false));
        assert!(!skips(K::AltLeft, true, false));
        assert!(!skips(K::KeyF, false, true));
    }

    #[test]
    fn a_missing_movie_is_none() {
        assert!(path(Path::new("/nonexistent"), INTRO).is_none());
        assert!(Movie::silent(Path::new("/nonexistent"), INTRO).is_none());
    }
}
