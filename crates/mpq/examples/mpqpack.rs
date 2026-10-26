//! mpqpack <out.w3x> <source map> [name=file ...]: a copy of a map with some files
//! replaced (or added, or with name=- left out), written as a simple uncompressed MPQ after the source's 512-byte
//! HM3W header, for testing the same scene in both games.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let (out, src) = (&a[1], &a[2]);
    let bytes = std::fs::read(src)?;
    let archive = w3_mpq::Archive::open(src)?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for name in archive.listfile() {
        if let Ok(data) = archive.read(&name) {
            files.push((name, data));
        }
    }
    for r in &a[3..] {
        let (name, path) = r.split_once('=').ok_or("expected name=file")?;
        // name=- leaves the file out.
        if path == "-" {
            files.retain(|f| !f.0.eq_ignore_ascii_case(name));
            continue;
        }
        let data = std::fs::read(path)?;
        match files.iter_mut().find(|f| f.0.eq_ignore_ascii_case(name)) {
            Some(f) => f.1 = data,
            None => files.push((name.to_string(), data)),
        }
    }
    let mut result = bytes[..512.min(bytes.len())].to_vec();
    result.extend(w3_mpq::writer::write_archive(&files));
    std::fs::write(out, result)?;
    println!("{} files", files.len());
    Ok(())
}
