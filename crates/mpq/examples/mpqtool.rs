//! `mpqtool list <archive>` | `mpqtool cat <archive> <name>` | `mpqtool verify <archive>`
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = w3_mpq::Archive::open(&a[2]).expect("open");
    match a[1].as_str() {
        "list" => ar.listfile().iter().for_each(|n| println!("{n}")),
        "cat" => std::io::stdout().write_all(&ar.read(&a[3]).expect("read")).unwrap(),
        "verify" => {
            let (mut ok, mut bad) = (0, 0);
            for n in ar.listfile() {
                match ar.read(&n) {
                    Ok(_) => ok += 1,
                    Err(e) => {
                        bad += 1;
                        println!("FAIL {n}: {e}");
                    }
                }
            }
            println!("ok={ok} failed={bad}");
        }
        _ => eprintln!("unknown command"),
    }
}
