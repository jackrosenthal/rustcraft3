//! Environmental effects: the reverberation the original asked EAX for.  The sound
//! environment (NewSoundEnvironment) is a row of UI\SoundInfo\EnvironmentSounds.slk in
//! I3DL2 terms: its decay time, its room, reflections, and reverb levels in millibels,
//! and the reflections' and reverb's delays.  Each 3D sound sends to it at its
//! category's EAX2Room level (its label's EAXFlags in UI\SoundInfo\EAXDefs.slk).
//! Without EAX hardware the reverberation is rendered here: an early reflection after
//! ReflectionsDelay and a late tail of comb and all-pass filters after ReverbDelay more,
//! decaying by 60 dB over DecayTime.

use std::time::Duration;

use rodio::Source;
use rodio::source::SeekError;

/// A sound environment's parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub decay_time: f32,
    pub decay_hf_ratio: f32,
    /// Room, Reflections, and Reverb, in millibels.
    pub room: f32,
    pub reflections: f32,
    pub reverb: f32,
    pub reflections_delay: f32,
    pub reverb_delay: f32,
}

fn level(mb: f32) -> f32 {
    10f32.powf(mb.min(0.0) / 2000.0)
}

/// Freeverb's comb and all-pass lengths at 44.1 kHz.
const COMBS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASSES: [usize; 2] = [556, 441];

struct Channel {
    delay: Vec<f32>,
    combs: Vec<(Vec<f32>, usize, f32, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
}

/// A source with its reverberation added, which goes on for the decay time after the
/// source ends.
pub struct Reverb<S: Source> {
    inner: S,
    channels: u16,
    rate: u32,
    chans: Vec<Channel>,
    pos: usize,
    early_at: usize,
    late_at: usize,
    early: f32,
    late: f32,
    feedback: Vec<f32>,
    damp: f32,
    channel: usize,
    /// Samples left of the tail once the source has ended.
    tail: Option<usize>,
}

impl<S: Source> Reverb<S> {
    /// `send` is the sound's level into the environment, in millibels.
    pub fn new(inner: S, env: Environment, send: f32) -> Reverb<S> {
        let channels = inner.channels().get();
        let rate = inner.sample_rate().get();
        let scale = rate as f32 / 44100.0;
        let early_at = (env.reflections_delay.max(0.0) * rate as f32) as usize;
        let late_at = ((env.reflections_delay + env.reverb_delay).max(0.0) * rate as f32) as usize;
        let size = late_at.max(early_at) + 1;
        let feedback: Vec<f32> = COMBS
            .iter()
            .map(|&n| {
                let secs = n as f32 * scale / rate as f32;
                if env.decay_time > 0.0 { 10f32.powf(-3.0 * secs / env.decay_time) } else { 0.0 }
            })
            .collect();
        let chans = (0..channels)
            .map(|_| Channel {
                delay: vec![0.0; size],
                combs: COMBS.iter().map(|&n| (vec![0.0; ((n as f32 * scale) as usize).max(1)], 0, 0.0, 0.0)).collect(),
                allpasses: ALLPASSES.iter().map(|&n| (vec![0.0; ((n as f32 * scale) as usize).max(1)], 0)).collect(),
            })
            .collect();
        let send = level(send);
        Reverb {
            inner,
            channels,
            rate,
            chans,
            pos: 0,
            early_at,
            late_at,
            early: send * level(env.room + env.reflections),
            late: send * level(env.room + env.reverb) / COMBS.len() as f32,
            feedback,
            // High frequencies decay faster as DecayHFRatio falls below one.
            damp: (1.0 - env.decay_hf_ratio).clamp(0.0, 0.9),
            channel: 0,
            tail: None,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let (damp, early_at, late_at, early, late) = (self.damp, self.early_at, self.late_at, self.early, self.late);
        let c = &mut self.chans[self.channel];
        let n = c.delay.len();
        c.delay[self.pos % n] = input;
        let tap = |d: usize| c.delay[(self.pos + n - d.min(n - 1)) % n];
        let reflected = tap(early_at);
        let x = tap(late_at);
        let mut sum = 0.0;
        for (k, (buf, i, store, _)) in c.combs.iter_mut().enumerate() {
            let out = buf[*i];
            *store = out * (1.0 - damp) + *store * damp;
            buf[*i] = x + *store * self.feedback[k];
            *i = (*i + 1) % buf.len();
            sum += out;
        }
        for (buf, i) in c.allpasses.iter_mut() {
            let b = buf[*i];
            buf[*i] = sum + b * 0.5;
            sum = b - sum;
            *i = (*i + 1) % buf.len();
        }
        early * reflected + late * sum
    }
}

impl<S: Source> Iterator for Reverb<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let dry = match self.tail {
            None => match self.inner.next() {
                Some(s) => s,
                None => {
                    // The tail: the delay and the decay, in samples for each channel.
                    let len = (self.late_at + (self.decay_secs() * self.rate as f32) as usize) * self.channels as usize;
                    self.tail = Some(len);
                    0.0
                }
            },
            Some(0) => return None,
            Some(ref mut left) => {
                *left -= 1;
                0.0
            }
        };
        let wet = self.process(dry);
        self.channel += 1;
        if self.channel >= self.channels as usize {
            self.channel = 0;
            self.pos += 1;
        }
        Some(dry + wet)
    }
}

impl<S: Source> Reverb<S> {
    /// How long the tail rings: until the combs have fallen 60 dB.
    fn decay_secs(&self) -> f32 {
        let g = self.feedback.iter().cloned().fold(0.0, f32::max);
        if g <= 0.0 {
            return 0.0;
        }
        // g per comb length; 60 dB is 10^-3.
        let comb = COMBS[COMBS.len() - 1] as f32 / 44100.0;
        (comb * (-3.0 / g.log10())).min(10.0)
    }
}

impl<S: Source> Source for Reverb<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(self.channels).unwrap()
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

    #[test]
    fn a_click_rings_and_dies_away() {
        let dungeon = Environment { decay_time: 3.2, decay_hf_ratio: 1.0, room: 0.0, reflections: -1000.0, reverb: -1300.0, reflections_delay: 0.05, reverb_delay: 0.02 };
        let mut click = vec![0.0f32; 4410];
        click[0] = 1.0;
        let src = rodio::buffer::SamplesBuffer::new(rodio::ChannelCount::new(1).unwrap(), rodio::SampleRate::new(44100).unwrap(), click);
        let out: Vec<f32> = Reverb::new(src, dungeon, 0.0).collect();
        // The tail outlasts the click, every sample is finite, and the end is quiet.
        assert!(out.len() > 44100);
        assert!(out.iter().all(|s| s.is_finite()));
        let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>();
        let n = out.len();
        assert!(energy(&out[4410..8820]) > 0.0);
        assert!(energy(&out[n - 4410..]) < energy(&out[4410..8820]) * 0.01);
    }
}
