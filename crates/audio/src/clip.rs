//! The sound effects' own mixer.  Each sound is decoded once and kept at the output's
//! rate and channels (`Clip`); the sounds playing are summed here a block at a time
//! (`Mix`), each with its pitch, its gain, and its send to the environment's
//! reverberation, which is one for them all (`reverb::Room`).  The output's mixer sees
//! a single source.  (A rodio player per sound converted and wrapped every sample of
//! every sound, which with a field of towers firing was most of the audio thread.)

use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{Decoder, Source};

use crate::reverb::{Environment, Room};

/// Longer sounds stream from their files instead: kept whole, a long ambience would
/// hold tens of megabytes.
pub const LONGEST: Duration = Duration::from_secs(20);

/// Frames mixed at once: about 5 ms, so a change of volume or a new sound waits at
/// most that.
const BLOCK: usize = 256;

/// A decoded sound: interleaved samples at `rate`, `channels` to a frame.
pub struct Clip {
    pub channels: u16,
    pub rate: u32,
    pub samples: Arc<[f32]>,
}

impl Clip {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn bytes(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }

    /// A file's sound converted to `channels` and `rate`; None when it does not decode
    /// or runs longer than `LONGEST`.
    pub fn decode(data: Arc<[u8]>, channels: u16, rate: u32) -> Option<Clip> {
        let d = Decoder::new(Cursor::new(data)).ok()?;
        let (from_ch, from_rate) = (d.channels().get() as usize, d.sample_rate().get());
        if d.total_duration().is_some_and(|t| t > LONGEST) {
            return None;
        }
        let limit = (LONGEST.as_secs_f64() * from_rate as f64) as usize * from_ch;
        let src: Vec<f32> = d.take(limit + 1).collect();
        if src.len() > limit || from_ch == 0 {
            return None;
        }
        let frames = src.len() / from_ch;
        let to_ch = channels.max(1) as usize;
        // Channels: one to many repeats it; many to one averages them; otherwise each
        // takes its own, wrapping.
        let frame = |f: usize, c: usize| -> f32 {
            let base = f * from_ch;
            if to_ch == 1 && from_ch > 1 {
                src[base..base + from_ch].iter().sum::<f32>() / from_ch as f32
            } else {
                src[base + c % from_ch]
            }
        };
        // The rate: linear interpolation between neighboring frames, as the output's
        // own converter does, done once.
        let out_frames = if from_rate == rate { frames } else { ((frames as u64 * rate as u64) / from_rate.max(1) as u64) as usize };
        let mut out = Vec::with_capacity(out_frames * to_ch);
        let step = from_rate as f64 / rate.max(1) as f64;
        for k in 0..out_frames {
            let p = k as f64 * step;
            let i = p as usize;
            let t = (p - i as f64) as f32;
            for c in 0..to_ch {
                let a = frame(i.min(frames.saturating_sub(1)), c);
                let s = if t > 0.0 && i + 1 < frames { a + (frame(i + 1, c) - a) * t } else { a };
                out.push(s);
            }
        }
        Some(Clip { channels: to_ch as u16, rate, samples: out.into() })
    }
}

/// A sound playing in the mix.
struct Voice {
    handle: u64,
    samples: Arc<[f32]>,
    frames: usize,
    /// The frame being played, and how far toward the next.
    pos: f64,
    /// Frames per output frame: its pitch.
    step: f64,
    looping: bool,
    /// Its gain, and the gain the last block ended at (a change ramps over a block).
    gain: f32,
    gain_was: f32,
    /// Its share into the reverberation, of its gain.
    send: f32,
}

struct State {
    voices: Vec<Voice>,
    room: Option<Room>,
    next: u64,
}

/// The game's side of the mix: sounds started, changed, and stopped by handle.
#[derive(Clone)]
pub struct Mix {
    state: Arc<Mutex<State>>,
    channels: u16,
    rate: u32,
}

impl Mix {
    /// A mix at the output's channels and rate, and its source for the output.
    pub fn new(channels: u16, rate: u32) -> (Mix, MixSource) {
        let state = Arc::new(Mutex::new(State { voices: Vec::new(), room: None, next: 1 }));
        let channels = channels.max(1);
        let src = MixSource { state: state.clone(), channels: channels as usize, rate, buf: Vec::new(), wet: Vec::new(), at: 0 };
        (Mix { state, channels, rate }, src)
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Plays a clip (at the mix's channels and rate) at `pitch` times its speed; `send`
    /// is its share into the reverberation (0 for none).
    pub fn play(&self, clip: &Clip, pitch: f32, looping: bool, gain: f32, send: f32) -> u64 {
        let mut s = self.state.lock().unwrap();
        let handle = s.next;
        s.next += 1;
        s.voices.push(Voice { handle, samples: clip.samples.clone(), frames: clip.frames(), pos: 0.0, step: pitch.max(0.01) as f64, looping, gain, gain_was: gain, send });
        handle
    }

    pub fn set_gain(&self, handle: u64, gain: f32) {
        if let Some(v) = self.state.lock().unwrap().voices.iter_mut().find(|v| v.handle == handle) {
            v.gain = gain;
        }
    }

    /// To a time into the sound as recorded.
    pub fn seek(&self, handle: u64, at: Duration) {
        let rate = self.rate as f64;
        if let Some(v) = self.state.lock().unwrap().voices.iter_mut().find(|v| v.handle == handle) {
            v.pos = at.as_secs_f64() * rate;
        }
    }

    pub fn stop(&self, handle: u64) {
        self.state.lock().unwrap().voices.retain(|v| v.handle != handle);
    }

    pub fn playing(&self, handle: u64) -> bool {
        self.state.lock().unwrap().voices.iter().any(|v| v.handle == handle)
    }

    /// The environment the sends reverberate in, or none.
    pub fn set_room(&self, env: Option<Environment>) {
        self.state.lock().unwrap().room = env.map(|e| Room::new(self.channels, self.rate, e, 0.0));
    }
}

/// The mix as the output plays it: silent, never ending, while nothing sounds.
pub struct MixSource {
    state: Arc<Mutex<State>>,
    channels: usize,
    rate: u32,
    /// The block being played out, and the sends of it to reverberate.
    buf: Vec<f32>,
    wet: Vec<f32>,
    at: usize,
}

impl MixSource {
    fn render(&mut self) {
        let ch = self.channels;
        self.buf.clear();
        self.buf.resize(BLOCK * ch, 0.0);
        self.wet.clear();
        self.wet.resize(BLOCK * ch, 0.0);
        self.at = 0;
        let mut s = self.state.lock().unwrap();
        let State { voices, room, .. } = &mut *s;
        let sending = room.is_some();
        voices.retain_mut(|v| {
            let ramp = (v.gain - v.gain_was) / BLOCK as f32;
            let mut ended = false;
            for f in 0..BLOCK {
                if v.pos >= v.frames as f64 {
                    if !v.looping || v.frames == 0 {
                        ended = true;
                        break;
                    }
                    v.pos %= v.frames as f64;
                }
                let i = v.pos as usize;
                let t = (v.pos - i as f64) as f32;
                // A looping sound's last frame leads into its first.
                let j = if i + 1 < v.frames { i + 1 } else if v.looping { 0 } else { i };
                let g = v.gain_was + ramp * f as f32;
                let (a, b) = (&v.samples[i * ch..i * ch + ch], &v.samples[j * ch..j * ch + ch]);
                let out = &mut self.buf[f * ch..f * ch + ch];
                if t == 0.0 {
                    for c in 0..ch {
                        out[c] += a[c] * g;
                    }
                } else {
                    for c in 0..ch {
                        out[c] += (a[c] + (b[c] - a[c]) * t) * g;
                    }
                }
                if sending && v.send > 0.0 {
                    let wet = &mut self.wet[f * ch..f * ch + ch];
                    for c in 0..ch {
                        wet[c] += (a[c] + (b[c] - a[c]) * t) * g * v.send;
                    }
                }
                v.pos += v.step;
            }
            v.gain_was = v.gain;
            !ended
        });
        if let Some(room) = room.as_mut() {
            for (o, w) in self.buf.iter_mut().zip(&self.wet) {
                *o += room.next(*w);
            }
        }
    }
}

impl Iterator for MixSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.buf.len() {
            self.render();
        }
        let s = self.buf[self.at];
        self.at += 1;
        Some(s)
    }
}

impl Source for MixSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(self.channels as u16).unwrap()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(self.rate).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported { underlying_source: std::any::type_name::<Self>() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(samples: Vec<f32>, channels: u16, rate: u32) -> Clip {
        Clip { channels, rate, samples: samples.into() }
    }

    fn take(src: &mut MixSource, n: usize) -> Vec<f32> {
        (0..n).map(|_| src.next().unwrap()).collect()
    }

    #[test]
    fn a_sound_plays_its_samples_then_the_mix_is_silent() {
        let (mix, mut src) = Mix::new(2, 100);
        let h = mix.play(&clip(vec![0.0, 1.0, 2.0, 3.0], 2, 100), 1.0, false, 1.0, 0.0);
        assert_eq!(take(&mut src, 6), vec![0.0, 1.0, 2.0, 3.0, 0.0, 0.0]);
        // Its block over, it is gone.
        take(&mut src, BLOCK * 2);
        assert!(!mix.playing(h));
    }

    #[test]
    fn sounds_sum_and_pitch_interpolates() {
        let (mix, mut src) = Mix::new(1, 100);
        mix.play(&clip(vec![0.0, 1.0, 2.0, 3.0], 1, 100), 0.5, false, 1.0, 0.0);
        mix.play(&clip(vec![1.0, 1.0], 1, 100), 1.0, false, 1.0, 0.0);
        assert_eq!(take(&mut src, 9), vec![1.0, 1.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.0, 0.0]);
    }

    #[test]
    fn a_looping_sound_comes_around_until_stopped() {
        let (mix, mut src) = Mix::new(1, 100);
        let h = mix.play(&clip(vec![0.0, 1.0, 2.0], 1, 100), 1.0, true, 1.0, 0.0);
        assert_eq!(take(&mut src, 7), vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0]);
        mix.stop(h);
        take(&mut src, BLOCK);
        assert!(take(&mut src, BLOCK).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn a_gain_change_ramps_over_a_block() {
        let (mix, mut src) = Mix::new(1, 100);
        let h = mix.play(&clip(vec![1.0; BLOCK * 3], 1, 100), 1.0, false, 1.0, 0.0);
        take(&mut src, BLOCK);
        mix.set_gain(h, 0.0);
        let b = take(&mut src, BLOCK);
        assert_eq!(b[0], 1.0);
        assert!(b.windows(2).all(|w| w[1] < w[0]));
        assert!(take(&mut src, BLOCK).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn only_sends_reverberate() {
        let env = Environment { decay_time: 1.0, decay_hf_ratio: 1.0, room: 0.0, reflections: 0.0, reverb: 0.0, reflections_delay: 0.0, reverb_delay: 0.0 };
        let (mix, mut src) = Mix::new(1, 1000);
        mix.set_room(Some(env));
        let mut click = vec![0.0; 10];
        click[0] = 1.0;
        mix.play(&clip(click.clone(), 1, 1000), 1.0, false, 1.0, 0.0);
        let dry: Vec<f32> = take(&mut src, BLOCK * 4);
        assert!(dry[20..].iter().all(|&s| s == 0.0));
        mix.play(&clip(click, 1, 1000), 1.0, false, 1.0, 1.0);
        let sent: Vec<f32> = take(&mut src, BLOCK * 4);
        assert!(sent[20..].iter().any(|&s| s != 0.0));
    }

    #[test]
    fn a_wav_decodes_at_the_outputs_rate_and_channels() {
        // A mono 16-bit WAV of 4 frames at 50 Hz, a ramp.
        let pcm: Vec<i16> = vec![0, 8192, 16384, 24576];
        let mut wav = Vec::new();
        let data_len = (pcm.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&50u32.to_le_bytes());
        wav.extend_from_slice(&100u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for s in &pcm {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        let c = Clip::decode(wav.into(), 2, 100).expect("decodes");
        assert_eq!((c.channels, c.rate, c.frames()), (2, 100, 8));
        // Each frame repeated in both channels, halfway frames between the originals.
        let left: Vec<f32> = c.samples.iter().step_by(2).copied().collect();
        let right: Vec<f32> = c.samples.iter().skip(1).step_by(2).copied().collect();
        assert_eq!(left, right);
        let want = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.75];
        assert!(left.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3), "{left:?}");
    }
}
