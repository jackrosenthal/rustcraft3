//! Renders a MIDI file with a DLS bank to a WAV: midirender <archive> <mid> <dls> <seconds> <out.wav>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).unwrap();
    let bank = w3_audio::midi::Bank::parse(&ar.read(&a[3]).unwrap()).expect("bank");
    let song = w3_audio::midi::Song::parse(&ar.read(&a[2]).unwrap()).expect("song");
    let secs: f32 = a[4].parse().unwrap();
    let mut p = w3_audio::midi::Player::new(std::sync::Arc::new(bank), std::sync::Arc::new(song), 1.0);
    let n = (secs * 44100.0) as usize * 2;
    let samples: Vec<f32> = (0..n).map(|_| p.next().unwrap()).collect();
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    println!("peak {peak:.3} rms {rms:.4}");
    let mut w = Vec::new();
    let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    for v in [16u32] { w.extend_from_slice(&v.to_le_bytes()); }
    for v in [1u16, 2] { w.extend_from_slice(&v.to_le_bytes()); }
    for v in [44100u32, 44100 * 4] { w.extend_from_slice(&v.to_le_bytes()); }
    for v in [4u16, 16] { w.extend_from_slice(&v.to_le_bytes()); }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    std::fs::write(&a[5], w).unwrap();
}
