//! `w3zedit <in.w3z> <out.w3z> life <unit tag> <value>`: a saved game with a unit's
//! life changed (its indexed real 1's value), written back in the port's container;
//! `drop1`: its second table left with its classes' definitions alone.
use w3_formats::w3z::{First, SaveGame};
use w3_formats::codes;

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
        // "header0": the object manager header's two first words, its update counter
        // and parity, and its four groups' counts zeroed (1024 for the counter).
        "header0" => {
            let h = &mut m.header;
            let mut put = |k: usize, v: u32| h[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            put(0, 0);
            put(1, 0);
            put(4, 1024);
            put(5, 0);
            for g in 0..4 {
                put(12 + 5 * g, 0);
            }
        }
        // "none": as it was (a control).
        "none" => {}
        // "drop1": the second table's objects but its classes' definitions left out.
        "drop1" => {
            let before = objects[1].len();
            objects[1].retain(|o| &o.entry.kind.to_be_bytes() == codes::class::AGENT_DEFINITION);
            println!("second table {before} -> {}", objects[1].len());
        }
        // "dropghosts [free]": the fog ghosts (`+wgh`) left out with the position and
        // two regions after each; with "free", their slots (as tags) on the second
        // table's free list.
        "dropghosts" => {
            let mut gone: Vec<u32> = Vec::new();
            for (k, o) in objects[1].iter().enumerate() {
                if &o.entry.kind.to_be_bytes() == codes::class::FOG_GHOST {
                    gone.extend(objects[1][k..(k + 4).min(objects[1].len())].iter().map(|x| x.entry.tag));
                }
            }
            let before = objects[1].len();
            objects[1].retain(|o| !gone.contains(&o.entry.tag));
            println!("second table {before} -> {}", objects[1].len());
            if a.get(4).is_some_and(|v| v == "free") {
                let mut out = Vec::new();
                for &t in gone.iter().rev() {
                    out.extend(t.to_le_bytes());
                }
                out.extend(u32::MAX.to_le_bytes());
                m.free[1] = out;
            }
        }
        // "dropkinds <kind,...>": the second table's objects of those kinds left out
        // (alone, without their children), and the fog ghosts with theirs.
        "dropkinds" => {
            let kinds: Vec<&str> = a[4].split(',').collect();
            let mut gone: Vec<u32> = Vec::new();
            for (k, o) in objects[1].iter().enumerate() {
                let kind = String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).into_owned();
                if kind == "+wgh" {
                    gone.extend(objects[1][k..(k + 4).min(objects[1].len())].iter().map(|x| x.entry.tag));
                } else if kinds.contains(&kind.as_str()) {
                    gone.push(o.entry.tag);
                }
            }
            let before = objects[1].len();
            objects[1].retain(|o| !gone.contains(&o.entry.tag));
            println!("second table {before} -> {}", objects[1].len());
        }
        other => panic!("unknown edit {other}"),
    }
    m.set_objects(&objects);
    std::fs::write(&a[2], s.to_file(&header)).unwrap();
}
