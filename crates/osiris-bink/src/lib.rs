//! A decoder for the Bink movies Pharaoh plays (`BINKS/High/*.bik`): Bink 1 video and
//! Bink audio (RDFT), in pure Rust with no dependencies.
//!
//! This is a port of FFmpeg's Bink demuxer (libavformat/bink.c, Copyright (c) 2008-2010
//! Peter Ross, Copyright (c) 2009 Daniel Verkamp) and decoders (see [`video`] and
//! [`audio`]). FFmpeg's code is licensed LGPL-2.1-or-later; these ports are used under
//! GPL-3.0-or-later, as the LGPL allows, like the rest of Osiris.
//!
//! ```no_run
//! let mut movie = osiris_bink::Movie::open("PharaohData/BINKS/High/Intro_big.bik").unwrap();
//! let mut samples = Vec::new();
//! let mut rgba = vec![0; movie.width() as usize * movie.height() as usize * 4];
//! while movie.next_frame(&mut samples).unwrap() {
//!     movie.to_rgba(&mut rgba);
//! }
//! ```

mod audio;
mod bits;
mod tables;
mod video;

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub use video::Plane;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Not a Bink file, or a damaged one.
    Invalid(&'static str),
    /// A Bink variant this decoder doesn't handle.
    Unsupported(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Invalid(what) => write!(f, "invalid Bink data: {what}"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// An audio track's format, from the file header.
#[derive(Clone, Copy, Debug)]
pub struct AudioTrack {
    pub sample_rate: u32,
    pub channels: u16,
    pub flags: u16,
    pub id: u32,
}

/// What the file header says.
#[derive(Clone, Debug)]
pub struct Header {
    /// 'f' for Pharaoh's files ("BIKf").
    pub revision: u8,
    pub frames: u32,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub video_flags: u32,
    pub audio: Vec<AudioTrack>,
}

/// Where a frame's video data starts in the packet buffer, and its first audio
/// track's packets (offset, length).
type Packet = (usize, Vec<(usize, usize)>);

struct FrameEntry {
    pos: u64,
    size: u32,
    #[allow(dead_code)]
    keyframe: bool,
}

/// An open movie, decoded a frame at a time.
pub struct Movie<R> {
    reader: R,
    pub header: Header,
    index: Vec<FrameEntry>,
    next: usize,
    video: video::VideoDecoder,
    /// The first audio track's decoder (the only one played), if it has one this can decode.
    audio: Option<audio::AudioDecoder>,
    packet: Vec<u8>,
}

impl Movie<BufReader<File>> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Movie::new(BufReader::new(File::open(path)?))
    }
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

impl<R: Read + Seek> Movie<R> {
    pub fn new(mut reader: R) -> Result<Self> {
        let mut h = [0u8; 44];
        reader.read_exact(&mut h)?;
        if &h[0..3] != b"BIK" {
            return Err(Error::Invalid("no BIK signature"));
        }
        let revision = h[3];
        let file_size = u32_at(&h, 4) as u64 + 8;
        let frames = u32_at(&h, 8);
        if frames == 0 || frames > 1_000_000 {
            return Err(Error::Invalid("frame count"));
        }
        let (width, height) = (u32_at(&h, 20), u32_at(&h, 24));
        let (fps_num, fps_den) = (u32_at(&h, 28), u32_at(&h, 32));
        if width == 0 || height == 0 || width > 7680 || height > 4800 || fps_num == 0 || fps_den == 0 {
            return Err(Error::Invalid("size or frame rate"));
        }
        let video_flags = u32_at(&h, 36);
        let num_tracks = u32_at(&h, 40);
        if num_tracks > 256 {
            return Err(Error::Invalid("audio track count"));
        }
        if revision == b'k' {
            read_u32(&mut reader)?;
        }
        let mut audio_tracks = Vec::new();
        if num_tracks > 0 {
            // The largest decoded size of each track.
            reader.seek(SeekFrom::Current(4 * num_tracks as i64))?;
            let mut formats = vec![0u8; 4 * num_tracks as usize];
            reader.read_exact(&mut formats)?;
            for f in formats.chunks(4) {
                let flags = u16::from_le_bytes([f[2], f[3]]);
                audio_tracks.push(AudioTrack {
                    sample_rate: u16::from_le_bytes([f[0], f[1]]) as u32,
                    channels: if flags & 0x2000 != 0 { 2 } else { 1 },
                    flags,
                    id: 0,
                });
            }
            for t in &mut audio_tracks {
                t.id = read_u32(&mut reader)?;
            }
        }
        let mut raw = vec![0u8; 4 * frames as usize];
        reader.read_exact(&mut raw)?;
        let mut index = Vec::with_capacity(frames as usize);
        for i in 0..frames as usize {
            let entry = u32_at(&raw, 4 * i);
            let next = if i + 1 == frames as usize { file_size as u32 } else { u32_at(&raw, 4 * (i + 1)) };
            let (pos, next) = ((entry & !1) as u64, (next & !1) as u64);
            if next <= pos {
                return Err(Error::Invalid("frame index"));
            }
            index.push(FrameEntry { pos, size: (next - pos) as u32, keyframe: entry & 1 != 0 || i == 0 });
        }
        let video = video::VideoDecoder::new(width, height, revision, video_flags)?;
        let audio = match audio_tracks.first() {
            Some(t) => audio::AudioDecoder::new(t.sample_rate, t.flags).ok(),
            None => None,
        };
        Ok(Movie {
            reader,
            header: Header { revision, frames, width, height, fps_num, fps_den, video_flags, audio: audio_tracks },
            index,
            next: 0,
            video,
            audio,
            packet: Vec::new(),
        })
    }

    pub fn width(&self) -> u32 {
        self.header.width
    }

    pub fn height(&self) -> u32 {
        self.header.height
    }

    pub fn frames(&self) -> u32 {
        self.header.frames
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        self.header.fps_num as f64 / self.header.fps_den as f64
    }

    /// The played audio track's sample rate and channel count, if there is one.
    pub fn audio_format(&self) -> Option<(u32, u16)> {
        self.audio.as_ref().map(|a| (a.sample_rate, a.channels))
    }

    /// The index of the frame [`Movie::next_frame`] decodes next.
    pub fn position(&self) -> usize {
        self.next
    }

    /// Reads the next frame's packet into `self.packet`, returning where its video
    /// data starts and the audio packets' (offset, length) for the first track.
    fn read_packet(&mut self) -> Result<Option<Packet>> {
        let Some(entry) = self.index.get(self.next) else { return Ok(None) };
        let size = entry.size as usize;
        self.reader.seek(SeekFrom::Start(entry.pos))?;
        self.packet.clear();
        self.packet.resize(size + bits::PADDING, 0);
        self.reader.read_exact(&mut self.packet[..size])?;
        self.next += 1;
        let mut at = 0;
        let mut first_track = Vec::new();
        for track in 0..self.header.audio.len() {
            if size - at < 4 {
                return Err(Error::Invalid("audio size past the packet"));
            }
            let audio_size = u32_at(&self.packet, at) as usize;
            at += 4;
            if audio_size > size - at {
                return Err(Error::Invalid("audio size past the packet"));
            }
            if track == 0 && audio_size >= 4 {
                first_track.push((at, audio_size));
            }
            at += audio_size;
        }
        Ok(Some((at, first_track)))
    }

    /// Decodes the next frame, appending its audio (interleaved, full scale at 1.0) to
    /// `samples`. Returns false after the last frame.
    pub fn next_frame(&mut self, samples: &mut Vec<f32>) -> Result<bool> {
        let Some((video_at, audio)) = self.read_packet()? else { return Ok(false) };
        if let Some(dec) = &mut self.audio {
            for (at, len) in audio {
                // A damaged audio packet is dropped, as FFmpeg does.
                let _ = dec.decode(&self.packet[at..], len, samples);
            }
        }
        let len = self.packet.len() - bits::PADDING - video_at;
        self.video.decode(&self.packet[video_at..], len)?;
        Ok(true)
    }

    /// Decodes only the next frame's audio, leaving the picture alone (the picture
    /// can't be decoded after this without starting over).
    pub fn next_audio(&mut self, samples: &mut Vec<f32>) -> Result<bool> {
        let Some((_, audio)) = self.read_packet()? else { return Ok(false) };
        if let Some(dec) = &mut self.audio {
            for (at, len) in audio {
                let _ = dec.decode(&self.packet[at..], len, samples);
            }
        }
        Ok(true)
    }

    /// The last decoded frame's planes: Y, U, V (half size) and alpha if the movie has it.
    pub fn planes(&self) -> &[Plane] {
        &self.video.cur
    }

    /// Converts the last decoded frame to RGBA (limited-range BT.601, as Bink is),
    /// `width * height * 4` bytes.
    pub fn to_rgba(&self, out: &mut [u8]) {
        let (w, h) = (self.header.width as usize, self.header.height as usize);
        let p = &self.video.cur;
        let alpha = p.get(3);
        for y in 0..h {
            let yrow = &p[0].data[y * p[0].stride..];
            let urow = &p[1].data[(y / 2) * p[1].stride..];
            let vrow = &p[2].data[(y / 2) * p[2].stride..];
            let orow = &mut out[y * w * 4..(y + 1) * w * 4];
            for x in 0..w {
                let c = (yrow[x] as i32 - 16) * 298;
                let d = urow[x / 2] as i32 - 128;
                let e = vrow[x / 2] as i32 - 128;
                let o = &mut orow[x * 4..x * 4 + 4];
                o[0] = ((c + 409 * e + 128) >> 8).clamp(0, 255) as u8;
                o[1] = ((c - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
                o[2] = ((c + 516 * d + 128) >> 8).clamp(0, 255) as u8;
                o[3] = alpha.map_or(255, |a| a.data[y * a.stride + x]);
            }
        }
    }
}
