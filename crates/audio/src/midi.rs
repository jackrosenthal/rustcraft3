//! MIDI sounds (UI\SoundInfo\MIDISounds.slk): standard MIDI files played with DLS
//! Level 1 instrument banks, as the environment ambience uses.

use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

const RATE: u32 = 44100;

/// A sample's playback parameters (wsmp).
#[derive(Clone, Copy, Debug, Default)]
struct WaveSample {
    unity_note: u8,
    fine_tune: i16,
    /// Gain in decibels.
    gain_db: f32,
    /// Loop start and length in samples.
    looped: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default)]
struct Wave {
    rate: u32,
    data: Vec<f32>,
    sample: Option<WaveSample>,
}

/// Volume envelope (EG1) in seconds and sustain level.
#[derive(Clone, Copy, Debug)]
struct Envelope {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl Default for Envelope {
    fn default() -> Self {
        Envelope { attack: 0.0, decay: 0.0, sustain: 1.0, release: 0.0 }
    }
}

#[derive(Clone, Debug)]
struct Region {
    keys: (u8, u8),
    velocities: (u8, u8),
    wave: usize,
    sample: Option<WaveSample>,
    envelope: Option<Envelope>,
}

#[derive(Clone, Debug)]
struct Instrument {
    drums: bool,
    bank: u32,
    program: u32,
    regions: Vec<Region>,
    envelope: Option<Envelope>,
}

/// A DLS bank.
#[derive(Debug, Default)]
pub struct Bank {
    instruments: Vec<Instrument>,
    waves: Vec<Wave>,
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Iterates the chunks in `d`: (id, LIST type if a list, body).
fn chunks(d: &[u8]) -> Vec<([u8; 4], Option<[u8; 4]>, &[u8])> {
    let mut out = Vec::new();
    let mut o = 0;
    while o + 8 <= d.len() {
        let id = [d[o], d[o + 1], d[o + 2], d[o + 3]];
        let len = u32_at(d, o + 4) as usize;
        let body = &d[o + 8..(o + 8 + len).min(d.len())];
        if &id == b"LIST" && body.len() >= 4 {
            out.push((id, Some([body[0], body[1], body[2], body[3]]), &body[4..]));
        } else {
            out.push((id, None, body));
        }
        o += 8 + len + (len & 1);
    }
    out
}

fn wsmp(d: &[u8]) -> WaveSample {
    let size = u32_at(d, 0) as usize;
    let loops = u32_at(d, 16);
    let looped = (loops > 0).then(|| {
        let l = size;
        (u32_at(d, l + 8), u32_at(d, l + 12))
    });
    WaveSample {
        unity_note: u16_at(d, 4) as u8,
        fine_tune: u16_at(d, 6) as i16,
        // Relative gain units: 1/655360 dB.
        gain_db: u32_at(d, 8) as i32 as f32 / 655360.0,
        looped,
    }
}

/// EG1 from art1 connection blocks.
fn art1(d: &[u8]) -> Envelope {
    let size = u32_at(d, 0) as usize;
    let n = u32_at(d, 4) as usize;
    let mut e = Envelope::default();
    // Time cents to seconds; the minimum value means zero.
    let tc = |v: i32| if v == i32::MIN { 0.0 } else { 2f32.powf(v as f32 / (1200.0 * 65536.0)) };
    for i in 0..n {
        let o = size + i * 12;
        let (source, dest, scale) = (u16_at(d, o), u16_at(d, o + 4), u32_at(d, o + 8) as i32);
        if source != 0 {
            continue;
        }
        match dest {
            0x206 => e.attack = tc(scale),
            0x207 => e.decay = tc(scale),
            0x209 => e.release = tc(scale),
            // Tenths of a percent.
            0x20a => e.sustain = (scale as f32 / 65536.0 / 1000.0).clamp(0.0, 1.0),
            _ => {}
        }
    }
    e
}

fn articulation(list: &[u8]) -> Option<Envelope> {
    chunks(list).into_iter().find(|(id, _, _)| id == b"art1").map(|(_, _, b)| art1(b))
}

impl Bank {
    pub fn parse(d: &[u8]) -> Option<Bank> {
        if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"DLS " {
            return None;
        }
        let top = chunks(&d[12..]);
        let mut bank = Bank::default();
        let mut cues: Vec<u32> = Vec::new();
        let mut pool: &[u8] = &[];
        for (id, list, body) in &top {
            match (id, list.as_ref()) {
                (b"LIST", Some(b"lins")) => {
                    for (_, l, ins) in chunks(body) {
                        if l != Some(*b"ins ") {
                            continue;
                        }
                        let mut inst = Instrument { drums: false, bank: 0, program: 0, regions: Vec::new(), envelope: None };
                        for (cid, cl, c) in chunks(ins) {
                            match (&cid, cl.as_ref()) {
                                (b"insh", _) => {
                                    let b = u32_at(c, 4);
                                    inst.drums = b & 0x8000_0000 != 0;
                                    inst.bank = b & 0x7fff_ffff;
                                    inst.program = u32_at(c, 8);
                                }
                                (b"LIST", Some(b"lart")) => inst.envelope = articulation(c),
                                (b"LIST", Some(b"lrgn")) => {
                                    for (_, rl, rgn) in chunks(c) {
                                        if rl != Some(*b"rgn ") {
                                            continue;
                                        }
                                        let mut r = Region { keys: (0, 127), velocities: (0, 127), wave: 0, sample: None, envelope: None };
                                        for (rid, rll, rc) in chunks(rgn) {
                                            match (&rid, rll.as_ref()) {
                                                (b"rgnh", _) => {
                                                    r.keys = (u16_at(rc, 0) as u8, u16_at(rc, 2) as u8);
                                                    r.velocities = (u16_at(rc, 4) as u8, u16_at(rc, 6) as u8);
                                                }
                                                (b"wsmp", _) => r.sample = Some(wsmp(rc)),
                                                (b"wlnk", _) => r.wave = u32_at(rc, 8) as usize,
                                                (b"LIST", Some(b"lart")) => r.envelope = articulation(rc),
                                                _ => {}
                                            }
                                        }
                                        inst.regions.push(r);
                                    }
                                }
                                _ => {}
                            }
                        }
                        bank.instruments.push(inst);
                    }
                }
                (b"ptbl", _) => {
                    let size = u32_at(body, 0) as usize;
                    let n = u32_at(body, 4) as usize;
                    cues = (0..n).map(|i| u32_at(body, size + i * 4)).collect();
                }
                (b"LIST", Some(b"wvpl")) => pool = body,
                _ => {}
            }
        }
        // Pool table offsets count from the start of the wave pool's data.
        for &off in &cues {
            let o = off as usize;
            let Some(w) = chunks(pool.get(o..)?).into_iter().next() else { continue };
            let mut wave = Wave::default();
            let (mut bits, mut channels) = (16, 1);
            for (id, _, b) in chunks(w.2) {
                match &id {
                    b"fmt " => {
                        channels = u16_at(b, 2).max(1) as usize;
                        wave.rate = u32_at(b, 4);
                        bits = u16_at(b, 14);
                    }
                    b"data" => {
                        // The first channel.
                        wave.data = match bits {
                            8 => b.iter().step_by(channels).map(|&s| (s as f32 - 128.0) / 128.0).collect(),
                            _ => b.chunks_exact(2).step_by(channels).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect(),
                        };
                    }
                    b"wsmp" => wave.sample = Some(wsmp(b)),
                    _ => {}
                }
            }
            bank.waves.push(wave);
        }
        Some(bank)
    }

    fn find(&self, drums: bool, bank: u32, program: u32) -> Option<&Instrument> {
        let matching = |b: bool| self.instruments.iter().find(|i| i.drums == drums && i.program == program && (!b || i.bank == bank));
        matching(true).or_else(|| matching(false))
    }
}

#[derive(Clone, Copy, Debug)]
enum Event {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    Program { channel: u8, program: u8 },
    Control { channel: u8, control: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
}

/// A standard MIDI file flattened to events at times in seconds.
#[derive(Debug, Default)]
pub struct Song {
    events: Vec<(f64, Event)>,
    length: f64,
}

fn varlen(d: &[u8], o: &mut usize) -> u32 {
    let mut v = 0u32;
    while *o < d.len() {
        let b = d[*o];
        *o += 1;
        v = (v << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    v
}

impl Song {
    pub fn parse(d: &[u8]) -> Option<Song> {
        if d.len() < 14 || &d[0..4] != b"MThd" {
            return None;
        }
        let tracks = u16::from_be_bytes([d[10], d[11]]) as usize;
        let division = u16::from_be_bytes([d[12], d[13]]);
        if division & 0x8000 != 0 {
            return None;
        }
        let ppq = division.max(1) as f64;
        // (tick, order, event or tempo).
        let mut raw: Vec<(u64, usize, Result<Event, u32>)> = Vec::new();
        let mut end_tick = 0u64;
        let mut o = 8 + u32::from_be_bytes([d[4], d[5], d[6], d[7]]) as usize;
        for _ in 0..tracks {
            if o + 8 > d.len() || &d[o..o + 4] != b"MTrk" {
                break;
            }
            let len = u32::from_be_bytes([d[o + 4], d[o + 5], d[o + 6], d[o + 7]]) as usize;
            let t = &d[o + 8..(o + 8 + len).min(d.len())];
            o += 8 + len;
            let (mut p, mut tick, mut status) = (0usize, 0u64, 0u8);
            while p < t.len() {
                tick += varlen(t, &mut p) as u64;
                let Some(&b) = t.get(p) else { break };
                if b & 0x80 != 0 {
                    status = b;
                    p += 1;
                }
                let ch = status & 0x0f;
                let arg = |i: usize| t.get(p + i).copied().unwrap_or(0);
                let n = raw.len();
                match status & 0xf0 {
                    0x80 => {
                        raw.push((tick, n, Ok(Event::NoteOff { channel: ch, key: arg(0) })));
                        p += 2;
                    }
                    0x90 => {
                        let e = if arg(1) == 0 { Event::NoteOff { channel: ch, key: arg(0) } } else { Event::NoteOn { channel: ch, key: arg(0), velocity: arg(1) } };
                        raw.push((tick, n, Ok(e)));
                        p += 2;
                    }
                    0xa0 => p += 2,
                    0xb0 => {
                        raw.push((tick, n, Ok(Event::Control { channel: ch, control: arg(0), value: arg(1) })));
                        p += 2;
                    }
                    0xc0 => {
                        raw.push((tick, n, Ok(Event::Program { channel: ch, program: arg(0) })));
                        p += 1;
                    }
                    0xd0 => p += 1,
                    0xe0 => {
                        let v = ((arg(1) as i16) << 7 | arg(0) as i16) - 8192;
                        raw.push((tick, n, Ok(Event::PitchBend { channel: ch, value: v })));
                        p += 2;
                    }
                    _ => match status {
                        0xff => {
                            let kind = arg(0);
                            p += 1;
                            let l = varlen(t, &mut p) as usize;
                            if kind == 0x51 && l >= 3 {
                                let us = (t[p] as u32) << 16 | (t[p + 1] as u32) << 8 | t[p + 2] as u32;
                                raw.push((tick, n, Err(us)));
                            }
                            p += l;
                            if kind == 0x2f {
                                break;
                            }
                        }
                        0xf0 | 0xf7 => {
                            let l = varlen(t, &mut p) as usize;
                            p += l;
                        }
                        _ => break,
                    },
                }
            }
            end_tick = end_tick.max(tick);
        }
        raw.sort_by_key(|(t, n, _)| (*t, *n));
        // Ticks to seconds through the tempo changes (500000 microseconds a beat at
        // first).
        let mut song = Song::default();
        let (mut last_tick, mut secs, mut tempo) = (0u64, 0f64, 500_000f64);
        for (tick, _, e) in raw {
            secs += (tick - last_tick) as f64 * tempo / 1e6 / ppq;
            last_tick = tick;
            match e {
                Ok(e) => song.events.push((secs, e)),
                Err(us) => tempo = us as f64,
            }
        }
        song.length = secs + (end_tick.saturating_sub(last_tick)) as f64 * tempo / 1e6 / ppq;
        Some(song)
    }
}

#[derive(Clone, Copy)]
struct Channel {
    program: u8,
    bank: u32,
    volume: f32,
    expression: f32,
    pan: f32,
    /// Semitones.
    bend: f32,
}

impl Default for Channel {
    fn default() -> Self {
        Channel { program: 0, bank: 0, volume: 100.0 / 127.0, expression: 1.0, pan: 0.5, bend: 0.0 }
    }
}

struct Voice {
    channel: u8,
    key: u8,
    wave: usize,
    pos: f64,
    /// Samples advanced per output sample, before pitch bend.
    step: f64,
    looped: Option<(u32, u32)>,
    gain: f32,
    env: Envelope,
    /// Seconds since the note started, and since it was released.
    age: f32,
    released: Option<(f32, f32)>,
}

impl Voice {
    fn level(&self) -> f32 {
        let e = &self.env;
        let held = |t: f32| {
            if t < e.attack {
                t / e.attack.max(1e-4)
            } else if t < e.attack + e.decay {
                // Decay runs from full to silence over `decay`, stopping at sustain.
                (1.0 - (t - e.attack) / e.decay.max(1e-4)).max(e.sustain)
            } else {
                e.sustain
            }
        };
        match self.released {
            None => held(self.age),
            Some((at, level)) => {
                // Release runs from full to silence over `release` (at least 10 ms to
                // avoid a click).
                let r = e.release.max(0.01);
                (level - (self.age - at) / r).max(0.0)
            }
        }
    }
}

/// Plays a song with a bank, looping.
pub struct Player {
    bank: Arc<Bank>,
    song: Arc<Song>,
    next: usize,
    time: f64,
    channels: [Channel; 16],
    voices: Vec<Voice>,
    volume: f32,
    frame: [f32; 2],
    half: usize,
}

impl Player {
    pub fn new(bank: Arc<Bank>, song: Arc<Song>, volume: f32) -> Player {
        Player { bank, song, next: 0, time: 0.0, channels: [Channel::default(); 16], voices: Vec::new(), volume, frame: [0.0; 2], half: 2 }
    }

    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { channel, key, velocity } => {
                let c = self.channels[channel as usize];
                let drums = channel == 9;
                let Some(inst) = self.bank.find(drums, c.bank, c.program as u32) else { return };
                let Some(r) = inst.regions.iter().find(|r| (r.keys.0..=r.keys.1).contains(&key) && (r.velocities.0..=r.velocities.1).contains(&velocity)) else { return };
                let Some(w) = self.bank.waves.get(r.wave) else { return };
                let s = r.sample.or(w.sample).unwrap_or(WaveSample { unity_note: 60, ..Default::default() });
                let semis = key as f64 - s.unity_note as f64 + s.fine_tune as f64 / 100.0;
                // Key velocity attenuates by 40 log10(velocity / 127) decibels.
                let vel_db = 40.0 * (velocity as f32 / 127.0).log10();
                let gain = 10f32.powf((vel_db + s.gain_db) / 20.0);
                self.voices.push(Voice {
                    channel,
                    key,
                    wave: r.wave,
                    pos: 0.0,
                    step: w.rate as f64 / RATE as f64 * 2f64.powf(semis / 12.0),
                    looped: s.looped.filter(|l| l.1 > 0),
                    gain,
                    env: r.envelope.or(inst.envelope).unwrap_or_default(),
                    age: 0.0,
                    released: None,
                });
            }
            Event::NoteOff { channel, key } => {
                for v in self.voices.iter_mut().filter(|v| v.channel == channel && v.key == key && v.released.is_none()) {
                    v.released = Some((v.age, v.level()));
                }
            }
            Event::Program { channel, program } => self.channels[channel as usize].program = program,
            Event::Control { channel, control, value } => {
                let c = &mut self.channels[channel as usize];
                let v = value as f32 / 127.0;
                match control {
                    0 => c.bank = (c.bank & 0x7f) | (value as u32) << 8,
                    32 => c.bank = (c.bank & !0x7f) | value as u32,
                    7 => c.volume = v,
                    10 => c.pan = v,
                    11 => c.expression = v,
                    120 | 123 => {
                        for voice in self.voices.iter_mut().filter(|x| x.channel == channel && x.released.is_none()) {
                            voice.released = Some((voice.age, voice.level()));
                        }
                    }
                    121 => {
                        *c = Channel { program: c.program, bank: c.bank, ..Channel::default() };
                    }
                    _ => {}
                }
            }
            // Two semitones either way.
            Event::PitchBend { channel, value } => self.channels[channel as usize].bend = value as f32 / 8192.0 * 2.0,
        }
    }

    fn render_frame(&mut self) -> [f32; 2] {
        while let Some(&(t, e)) = self.song.events.get(self.next) {
            if t > self.time {
                break;
            }
            self.handle(e);
            self.next += 1;
        }
        let dt = 1.0 / RATE as f32;
        let mut out = [0.0f32; 2];
        let bank = self.bank.clone();
        for v in &mut self.voices {
            let Some(w) = bank.waves.get(v.wave) else { continue };
            let c = &self.channels[v.channel as usize];
            let i = v.pos as usize;
            let Some(&a) = w.data.get(i) else {
                v.released = Some((v.age, 0.0));
                continue;
            };
            let b = w.data.get(i + 1).copied().unwrap_or(a);
            let f = (v.pos - i as f64) as f32;
            // Channel volume and expression attenuate like velocity.
            let ch_gain = (c.volume * c.expression).powi(2);
            let s = (a + (b - a) * f) * v.gain * v.level() * ch_gain;
            out[0] += s * (1.0 - c.pan).sqrt();
            out[1] += s * c.pan.sqrt();
            v.pos += v.step * 2f64.powf(c.bend as f64 / 12.0);
            if let Some((start, len)) = v.looped {
                let end = (start + len) as f64;
                if v.pos >= end {
                    v.pos -= len as f64;
                }
            }
            v.age += dt;
        }
        self.voices.retain(|v| v.released.is_none_or(|_| v.level() > 0.0) && (v.looped.is_some() || (v.pos as usize) < bank.waves.get(v.wave).map_or(0, |w| w.data.len())));
        self.time += dt as f64;
        if self.next >= self.song.events.len() && self.time >= self.song.length.max(1.0) {
            // Loop from the start.
            self.next = 0;
            self.time = 0.0;
        }
        [out[0] * self.volume, out[1] * self.volume]
    }
}

impl Iterator for Player {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.half >= 2 {
            self.frame = self.render_frame();
            self.half = 0;
        }
        let s = self.frame[self.half];
        self.half += 1;
        Some(s)
    }
}

impl Source for Player {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(2).unwrap()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(RATE).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
