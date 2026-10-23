//! Sound effects and music.  Sounds are named by labels in the UI\SoundInfo tables, which
//! give each label its files, volume, pitch, and 3D distances; music plays from
//! semicolon-separated playlists.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use rodio::{Decoder, Player, Source};

pub mod midi;
use w3_formats::slk::Table;
use w3_mpq::Chain;

/// The sound tables, in the order labels are looked up.
const TABLES: &[&str] = &[
    "UI\\SoundInfo\\UISounds.slk",
    "UI\\SoundInfo\\UnitAckSounds.slk",
    "UI\\SoundInfo\\UnitCombatSounds.slk",
    "UI\\SoundInfo\\AnimSounds.slk",
    "UI\\SoundInfo\\AbilitySounds.slk",
    "UI\\SoundInfo\\AmbienceSounds.slk",
    "UI\\SoundInfo\\EnvironmentSounds.slk",
    "UI\\SoundInfo\\DialogSounds.slk",
    "UI\\SoundInfo\\MIDISounds.slk",
];

/// One label from the sound tables.
#[derive(Clone, Debug, Default)]
pub struct SoundDef {
    pub files: Vec<String>,
    /// 0 to 127.
    pub volume: f32,
    pub pitch: f32,
    pub pitch_variance: f32,
    pub flags: Vec<String>,
    pub min_distance: f32,
    pub max_distance: f32,
    pub cutoff: f32,
    pub channel: i32,
}

impl SoundDef {
    pub fn has_flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x.eq_ignore_ascii_case(f))
    }
}

fn parse_defs(t: &Table, out: &mut HashMap<String, SoundDef>) {
    let get = |row: &str, col: &str| t.get(row, col).map(|v| v.as_str()).unwrap_or_default();
    let num = |row: &str, col: &str, d: f32| get(row, col).trim().parse::<f32>().unwrap_or(d);
    for name in &t.order {
        let dir = get(name, "DirectoryBase");
        let files = get(name, "FileNames")
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(|f| format!("{dir}{f}"))
            .collect();
        let def = SoundDef {
            files,
            volume: num(name, "Volume", 127.0),
            pitch: num(name, "Pitch", 1.0),
            pitch_variance: num(name, "PitchVariance", 0.0),
            flags: get(name, "Flags").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            min_distance: num(name, "MinDistance", 0.0),
            max_distance: num(name, "MaxDistance", 10000.0),
            cutoff: num(name, "DistanceCutoff", 0.0),
            channel: num(name, "Channel", 0.0) as i32,
        };
        out.entry(name.to_ascii_lowercase()).or_insert(def);
    }
}

/// A playing sound.
struct Voice {
    id: u64,
    player: Player,
    /// World position for 3D sounds.
    position: Option<[f32; 3]>,
    volume: f32,
    distances: (f32, f32, f32),
}

pub struct Audio {
    sink: Option<rodio::MixerDeviceSink>,
    chain: Arc<Chain>,
    defs: HashMap<String, SoundDef>,
    /// Model event codes (e.g. FBCL) to sound labels, from AnimLookups.slk.
    pub anim_lookups: HashMap<String, String>,
    files: HashMap<String, Option<Arc<[u8]>>>,
    voices: Vec<Voice>,
    next_id: u64,
    listener: [f32; 3],
    music: Option<Player>,
    playlist: Vec<String>,
    playlist_pos: usize,
    pub sound_volume: f32,
    pub music_volume: f32,
    rng: u64,
    /// MIDISounds.slk: label to (MIDI file, DLS file, volume 0 to 127).
    midi_defs: HashMap<String, (String, String, f32)>,
    banks: HashMap<String, Option<Arc<midi::Bank>>>,
    songs: HashMap<String, Option<Arc<midi::Song>>>,
}

impl Audio {
    /// Opens the default output device; without one, every call does nothing.
    pub fn new(chain: Arc<Chain>) -> Audio {
        let sink = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut s) => {
                s.log_on_drop(false);
                Some(s)
            }
            Err(e) => {
                log::warn!("no audio output: {e}");
                None
            }
        };
        let mut defs = HashMap::new();
        for t in TABLES {
            if let Ok(s) = chain.read_string(t) {
                parse_defs(&Table::parse(&s), &mut defs);
            }
        }
        let mut midi_defs = HashMap::new();
        if let Ok(s) = chain.read_string("UI\\SoundInfo\\MIDISounds.slk") {
            let t = Table::parse(&s);
            let get = |row: &str, col: &str| t.get(row, col).map(|v| v.as_str()).unwrap_or_default();
            for k in &t.order {
                let dir = get(k, "DirectoryBase");
                let sep = if dir.ends_with('\\') { "" } else { "\\" };
                let volume = get(k, "Volume").trim().parse().unwrap_or(127.0);
                midi_defs.insert(k.to_ascii_lowercase(), (format!("{dir}{sep}{}", get(k, "MIDIFileName")), format!("{dir}{sep}{}", get(k, "DLSFileName")), volume));
            }
        }
        let mut anim_lookups = HashMap::new();
        if let Ok(s) = chain.read_string("UI\\SoundInfo\\AnimLookups.slk") {
            let t = Table::parse(&s);
            for k in &t.order {
                if let Some(v) = t.get(k, "SoundLabel") {
                    anim_lookups.insert(k.to_ascii_uppercase(), v.as_str());
                }
            }
        }
        Audio {
            sink,
            chain,
            defs,
            anim_lookups,
            files: HashMap::new(),
            voices: Vec::new(),
            next_id: 1,
            listener: [0.0; 3],
            music: None,
            playlist: Vec::new(),
            playlist_pos: 0,
            sound_volume: 1.0,
            music_volume: 1.0,
            rng: 0x2545_f491_4f6c_dd1d,
            midi_defs,
            banks: HashMap::new(),
            songs: HashMap::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.sink.is_some()
    }

    pub fn def(&self, label: &str) -> Option<&SoundDef> {
        self.defs.get(&label.to_ascii_lowercase())
    }

    fn random(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn file(&mut self, path: &str) -> Option<Arc<[u8]>> {
        let key = path.to_ascii_lowercase();
        if let Some(f) = self.files.get(&key) {
            return f.clone();
        }
        // Sounds named .wav sometimes ship only as .mp3, and the reverse.
        let mut names = vec![path.to_string()];
        let lower = path.to_ascii_lowercase();
        if let Some(stem) = lower.strip_suffix(".wav") {
            names.push(format!("{}.mp3", &path[..stem.len()]));
        } else if let Some(stem) = lower.strip_suffix(".mp3") {
            names.push(format!("{}.wav", &path[..stem.len()]));
        }
        let data: Option<Arc<[u8]>> = names.iter().find_map(|n| self.chain.read(n).ok()).map(Arc::from);
        if data.is_none() {
            log::debug!("missing sound {path}");
        }
        self.files.insert(key, data.clone());
        data
    }

    /// Distance attenuation: full volume inside MinDistance, falling off with distance,
    /// and silent past the cutoff.
    fn gain_at(&self, pos: Option<[f32; 3]>, (min, max, cutoff): (f32, f32, f32)) -> f32 {
        let Some(p) = pos else { return 1.0 };
        let d = ((p[0] - self.listener[0]).powi(2) + (p[1] - self.listener[1]).powi(2) + (p[2] - self.listener[2]).powi(2)).sqrt();
        if cutoff > 0.0 && d > cutoff {
            return 0.0;
        }
        let d = d.min(max.max(min));
        if d <= min.max(1.0) { 1.0 } else { min.max(1.0) / d }
    }

    /// Plays a labeled sound; `position` makes it 3D.  Returns an id for [`Audio::stop`].
    pub fn play_label(&mut self, label: &str, position: Option<[f32; 3]>) -> Option<u64> {
        if let Some(m) = self.midi_defs.get(&label.to_ascii_lowercase()).cloned() {
            return self.play_midi(&m.0, &m.1, m.2 / 127.0);
        }
        let n = self.def(label)?.files.len() as u64;
        if n == 0 {
            return None;
        }
        let i = (self.random() % n) as usize;
        self.play_label_variant(label, i, position)
    }

    /// Plays a label's first file over and over, not positioned.
    pub fn play_label_looping(&mut self, label: &str) -> Option<u64> {
        let def = self.def(label)?.clone();
        let file = def.files.first()?.clone();
        self.play_file(&file, def.volume / 127.0, def.pitch, None, (def.min_distance, def.max_distance, def.cutoff), true)
    }

    /// Plays one file of a label by index (for lines played in order).
    pub fn play_label_variant(&mut self, label: &str, index: usize, position: Option<[f32; 3]>) -> Option<u64> {
        let def = self.def(label)?.clone();
        let file = def.files.get(index)?.clone();
        let pitch = if def.has_flag("RANDOMPITCH") && def.pitch_variance > 0.0 {
            let r = (self.random() % 10001) as f32 / 10000.0 * 2.0 - 1.0;
            def.pitch + r * def.pitch_variance
        } else {
            def.pitch
        };
        let pos = if def.has_flag("WANT3D") { position } else { None };
        self.play_file(&file, def.volume / 127.0, pitch, pos, (def.min_distance, def.max_distance, def.cutoff), def.has_flag("LOOPING"))
    }

    /// Plays a MIDI file with a DLS bank, looping.
    pub fn play_midi(&mut self, mid: &str, dls: &str, volume: f32) -> Option<u64> {
        let sink = self.sink.as_ref()?;
        let mixer = sink.mixer().clone();
        let bank = match self.banks.get(&dls.to_ascii_lowercase()) {
            Some(b) => b.clone(),
            None => {
                let b = self.chain.read(dls).ok().and_then(|d| midi::Bank::parse(&d)).map(Arc::new);
                self.banks.insert(dls.to_ascii_lowercase(), b.clone());
                b
            }
        }?;
        let song = match self.songs.get(&mid.to_ascii_lowercase()) {
            Some(s) => s.clone(),
            None => {
                let s = self.chain.read(mid).ok().and_then(|d| midi::Song::parse(&d)).map(Arc::new);
                self.songs.insert(mid.to_ascii_lowercase(), s.clone());
                s
            }
        }?;
        let player = Player::connect_new(&mixer);
        player.set_volume(volume * self.sound_volume);
        player.append(midi::Player::new(bank, song, 1.0));
        let id = self.next_id;
        self.next_id += 1;
        self.voices.push(Voice { id, player, position: None, volume, distances: (0.0, 0.0, 0.0) });
        Some(id)
    }

    /// Plays a sound file with a volume from 0 to 1.
    pub fn play_file(&mut self, path: &str, volume: f32, pitch: f32, position: Option<[f32; 3]>, distances: (f32, f32, f32), looping: bool) -> Option<u64> {
        let sink = self.sink.as_ref()?;
        let gain = self.gain_at(position, distances);
        if position.is_some() && gain <= 0.0 && !looping {
            return None;
        }
        let mixer = sink.mixer().clone();
        let data = self.file(path)?;
        let decoder = match Decoder::new(Cursor::new(data)) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("sound {path}: {e}");
                return None;
            }
        };
        let player = Player::connect_new(&mixer);
        player.set_speed(pitch.max(0.01));
        player.set_volume(volume * gain * self.sound_volume);
        if looping {
            player.append(decoder.repeat_infinite());
        } else {
            player.append(decoder);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.voices.push(Voice { id, player, position, volume, distances });
        Some(id)
    }

    pub fn stop(&mut self, id: u64) {
        self.voices.retain(|v| v.id != id);
    }

    pub fn is_playing(&self, id: u64) -> bool {
        self.voices.iter().any(|v| v.id == id && !v.player.empty())
    }

    pub fn set_position(&mut self, id: u64, position: [f32; 3]) {
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.position = Some(position);
        }
    }

    /// Length of a sound file in milliseconds, when its format reports one.
    pub fn file_duration_ms(&mut self, path: &str) -> Option<u32> {
        let data = self.file(path)?;
        let d = Decoder::new(Cursor::new(data)).ok()?;
        if let Some(t) = d.total_duration() {
            return Some(t.as_millis() as u32);
        }
        // Formats without a length in their header (MP3) are decoded and counted.
        let (rate, channels) = (d.sample_rate().get() as u64, d.channels().get() as u64);
        let samples = d.count() as u64;
        Some((samples * 1000 / (rate * channels).max(1)) as u32)
    }

    /// Plays a semicolon-separated list of music files in order, from `start`.
    pub fn play_music(&mut self, list: &str, random: bool, start: usize) {
        self.playlist = list.split(';').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
        if self.playlist.is_empty() {
            return;
        }
        self.playlist_pos = if random { (self.random() % self.playlist.len() as u64) as usize } else { start % self.playlist.len() };
        self.start_track();
    }

    fn start_track(&mut self) {
        let Some(sink) = self.sink.as_ref() else { return };
        let mixer = sink.mixer().clone();
        let Some(path) = self.playlist.get(self.playlist_pos).cloned() else { return };
        self.music = None;
        let Some(data) = self.file(&path) else { return };
        match Decoder::new(Cursor::new(data)) {
            Ok(d) => {
                let p = Player::connect_new(&mixer);
                p.set_volume(self.music_volume);
                p.append(d);
                self.music = Some(p);
            }
            Err(e) => log::warn!("music {path}: {e}"),
        }
    }

    pub fn stop_music(&mut self) {
        self.music = None;
        self.playlist.clear();
    }

    /// Moves the listener, updates 3D volumes, and advances the playlist.
    pub fn update(&mut self, listener: [f32; 3]) {
        self.listener = listener;
        self.voices.retain(|v| !v.player.empty());
        for i in 0..self.voices.len() {
            let v = &self.voices[i];
            if v.position.is_some() {
                let g = self.gain_at(v.position, v.distances);
                let v = &self.voices[i];
                v.player.set_volume(v.volume * g * self.sound_volume);
            }
        }
        if self.music.as_ref().is_some_and(|m| m.empty()) && !self.playlist.is_empty() {
            self.playlist_pos = (self.playlist_pos + 1) % self.playlist.len();
            self.start_track();
        }
    }
}
