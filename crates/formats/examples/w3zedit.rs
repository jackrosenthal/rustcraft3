//! `w3zedit <in.w3z> <out.w3z> life <unit tag> <value>`: a saved game with a unit's
//! life changed (its indexed real 1's value), written back in the port's container.
use w3_formats::w3z::{First, SaveGame};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let (header, _) = w3_formats::container::unpack(&bytes).unwrap();
    let mut s = SaveGame::read(&bytes).unwrap();
    let m = s.objects.as_mut().unwrap();
    let mut objects = m.objects().unwrap();
    match a[3].as_str() {
        "life" => {
            let (tag, value): (u32, f32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
            let unit = objects.iter().flatten().find(|o| o.entry.tag == tag).expect("no such object");
            let Ok(First::Agent { refs, .. }) = unit.first() else { panic!("not an agent") };
            for o in objects.iter_mut().flatten() {
                if !refs.iter().any(|r| (r.0, r.1) == (o.entry.tag, o.entry.birth)) {
                    continue;
                }
                if let Ok(First::Real { base, mut values }) = o.first() {
                    if base.object.words[0] == 1 {
                        println!("life {} -> {value}", values[0]);
                        values[0] = value;
                        o.first = First::Real { base, values }.to_bytes();
                    }
                }
            }
        }
        other => panic!("unknown edit {other}"),
    }
    m.set_objects(&objects);
    std::fs::write(&a[2], s.to_file(&header)).unwrap();
}
