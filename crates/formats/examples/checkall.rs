//! Parses every MDX and BLP in the given archives and reports failures.
fn main() {
    let mut ok = 0;
    let mut bad = 0;
    for path in std::env::args().skip(1) {
        let ar = w3_mpq::Archive::open(&path).expect("open");
        for name in ar.listfile() {
            let lower = name.to_ascii_lowercase();
            let Ok(data) = ar.read(&name) else { continue };
            let res = if lower.ends_with(".mdx") {
                w3_formats::mdx::Model::parse(&data).map(|_| ())
            } else if lower.ends_with(".blp") {
                w3_formats::blp::decode_texture(&name, &data).map(|_| ())
            } else {
                continue;
            };
            match res {
                Ok(()) => ok += 1,
                Err(e) => {
                    bad += 1;
                    println!("FAIL {name}: {e}");
                }
            }
        }
    }
    println!("ok={ok} failed={bad}");
}
