//! `unnamed <archive> <outdir>`: writes the files the archive's listfile does not name,
//! as <index>.bin, for looking at what it leaves out.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[1]).expect("open");
    let names = ar.listfile();
    let out = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(out).unwrap();
    let list = ar.unnamed(&names);
    println!("{} named, {} unnamed", names.len(), list.len());
    for i in list {
        if let Ok(d) = ar.read_index(i) {
            std::fs::write(out.join(format!("{i}.bin")), d).unwrap();
        }
    }
}
