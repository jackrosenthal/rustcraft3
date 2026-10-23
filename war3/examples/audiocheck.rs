//! audiocheck <game dir> <label|file>...: reports each sound's decoded duration.
use std::path::Path;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let a: Vec<String> = std::env::args().collect();
    let data = war3::data::GameData::open(Path::new(&a[1]), false)?;
    let mut audio = w3_audio::Audio::new(data.chain.clone());
    println!("output device: {}", audio.enabled());
    for name in &a[2..] {
        let files = match audio.def(name) {
            Some(d) => d.files.clone(),
            None => vec![name.clone()],
        };
        for f in files {
            println!("{name}: {f} -> {:?} ms", audio.file_duration_ms(&f));
        }
    }
    Ok(())
}
