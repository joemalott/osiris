//! Sound fed while it plays, for movies: the decoder pushes samples as it gets them,
//! and the samples played so far are the clock the pictures follow.

use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Shared {
    queue: Mutex<VecDeque<f32>>,
    /// Samples (of every channel) handed to the output so far.
    played: AtomicU64,
    /// No more samples will come: once the queue runs dry, silence plays on (and
    /// counts), so the clock keeps going for pictures left after the sound.
    finished: AtomicBool,
    stopped: AtomicBool,
}

/// Where a decoder pushes the samples of a [`StreamSound`]. Cheap to clone and send
/// to another thread.
#[derive(Clone)]
pub struct StreamFeed {
    shared: Arc<Shared>,
}

impl StreamFeed {
    /// Queues interleaved samples, full scale at 1.0.
    pub fn push(&self, samples: &[f32]) {
        self.shared.queue.lock().unwrap().extend(samples);
    }

    /// Says no more samples will come.
    pub fn finish(&self) {
        self.shared.finished.store(true, Ordering::Relaxed);
    }

    /// Whether the sound was stopped (so the decoder can stop too).
    pub fn stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Relaxed)
    }
}

/// A sound being played from samples pushed to its [`StreamFeed`]; it stops when
/// dropped.
pub struct StreamSound {
    shared: Arc<Shared>,
    rate: u32,
    channels: u16,
    player: Option<rodio::Player>,
}

impl StreamSound {
    /// A sound that isn't played (there is no output device), whose feed still takes
    /// samples; [`StreamSound::position`] is then `None`.
    pub fn silent(rate: u32, channels: u16) -> StreamSound {
        StreamSound { shared: Arc::default(), rate, channels, player: None }
    }

    pub(crate) fn play(mixer: &rodio::mixer::Mixer, rate: u32, channels: u16, volume: f32) -> StreamSound {
        let mut sound = StreamSound::silent(rate, channels);
        let player = rodio::Player::connect_new(mixer);
        player.set_volume(volume.clamp(0.0, 1.0));
        player.append(StreamSource {
            shared: sound.shared.clone(),
            channels: NonZero::new(channels.max(1)).unwrap(),
            rate: NonZero::new(rate.max(1)).unwrap(),
            buffer: Vec::new(),
            at: 0,
            silent: false,
        });
        sound.player = Some(player);
        sound
    }

    pub fn feed(&self) -> StreamFeed {
        StreamFeed { shared: self.shared.clone() }
    }

    /// How far the sound has played, or `None` if it isn't being played at all.
    pub fn position(&self) -> Option<Duration> {
        self.player.as_ref()?;
        let frames = self.shared.played.load(Ordering::Relaxed) / self.channels.max(1) as u64;
        Some(Duration::from_secs_f64(frames as f64 / self.rate.max(1) as f64))
    }
}

impl Drop for StreamSound {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Relaxed);
        if let Some(p) = &self.player {
            p.stop();
        }
    }
}

struct StreamSource {
    shared: Arc<Shared>,
    channels: NonZero<u16>,
    rate: NonZero<u32>,
    /// Samples taken from the queue a batch at a time, to lock it seldom.
    buffer: Vec<f32>,
    at: usize,
    /// The buffer is silence played while waiting for samples.
    silent: bool,
}

/// Samples taken from the queue at once (about 10 ms).
const BATCH: usize = 512;

impl Iterator for StreamSource {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.shared.stopped.load(Ordering::Relaxed) {
            return None;
        }
        if self.at == self.buffer.len() {
            self.buffer.clear();
            self.at = 0;
            let mut queue = self.shared.queue.lock().unwrap();
            // Whole frames only, so the channels stay in step.
            let ch = self.channels.get() as usize;
            let n = queue.len().min(BATCH) / ch * ch;
            self.buffer.extend(queue.drain(..n));
            self.silent = n == 0;
            if self.silent {
                // Waiting on the decoder: a frame of silence, which counts only once
                // the sound is over.
                self.buffer.resize(ch, 0.0);
            }
        }
        let v = self.buffer[self.at];
        self.at += 1;
        if !self.silent || self.shared.finished.load(Ordering::Relaxed) {
            self.shared.played.fetch_add(1, Ordering::Relaxed);
        }
        Some(v as rodio::Sample)
    }
}

impl rodio::Source for StreamSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
