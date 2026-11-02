//! `w3zedit <in.w3z> <out.w3z> life <unit tag> <value>`: a saved game with a unit's
//! life changed (its indexed real 1's value), written back in the port's container;
//! `drop1`: its second table left with its classes' definitions alone; `append
//! <file>`: the file's bytes after the record.
use w3_formats::w3z::{First, SaveGame};
use w3_formats::codes;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let (header, _) = w3_formats::container::unpack(&bytes).unwrap();
    let mut s = SaveGame::read(&bytes).unwrap();
    let mut objects = s.objects.as_ref().unwrap().objects().unwrap();
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
            let h = &mut s.objects.as_mut().unwrap().header;
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
        // "append <file>": the file's bytes after the record's last.
        "append" => {
            let extra = std::fs::read(&a[4]).unwrap();
            s.rest.extend(extra);
        }
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
                s.objects.as_mut().unwrap().free[1] = out;
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
        // "copycam <from.w3z>": the camera's objects (`+cam` and the eighteen made after
        // it) take another save's records, tag for tag.
        "copycam" => {
            let from = SaveGame::read(&std::fs::read(&a[4]).unwrap()).unwrap();
            let theirs = from.objects.as_ref().unwrap().objects().unwrap();
            let cam = objects[1].iter().position(|o| &o.entry.kind.to_be_bytes() == codes::class::CAMERA).expect("no camera");
            for o in objects[1][cam..cam + 19].iter_mut() {
                let t = theirs[1].iter().find(|x| x.entry.tag == o.entry.tag && x.entry.kind == o.entry.kind).expect("no such object in the other save");
                (o.first, o.second) = (t.first.clone(), t.second.clone());
            }
            println!("camera records from {}", a[4]);
        }
        // "camtarget <x> <y>": the camera's target point (its record's first point and
        // the pair before its bounds) moved, its other values kept.
        "camtarget" => {
            use w3_formats::w3z::{Field, Second};
            let (x, y): (f32, f32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
            let o = objects[1].iter_mut().find(|o| &o.entry.kind.to_be_bytes() == codes::class::CAMERA).expect("no camera");
            let Ok(Second::Agent { own, handlers, mut tail }) = o.second() else { panic!("camera not read") };
            if let Some(Field::List(points)) = tail.get_mut(19) {
                points[0][0] = Field::Real(x);
                points[0][1] = Field::Real(y);
            }
            tail[46] = Field::Real(x);
            tail[47] = Field::Real(y);
            o.second = Second::Agent { own, handlers, tail }.to_bytes(o.entry.kind).unwrap();
            println!("camera target {x} {y}");
        }
        // "graft <from.w3z> <pieces>": pieces of another save in place of this one's, by
        // comma: header (the record's fields before the object manager), client (the
        // second table, objects and entries), clocks, queues (data[1]), free1, block,
        // mheader (the manager's header), state:<part>, script.
        "graft" => {
            let from = SaveGame::read(&std::fs::read(&a[4]).unwrap()).unwrap();
            let fm = from.objects.as_ref().unwrap();
            let theirs = fm.objects().unwrap();
            let mut state_parts: Vec<String> = Vec::new();
            for piece in a[5].split(',') {
                match piece {
                    "header" => {
                        s.map = from.map.clone();
                        s.game_name = from.game_name.clone();
                        s.info_name = from.info_name.clone();
                        s.info_password = from.info_password.clone();
                        s.info_stat = from.info_stat.clone();
                        s.info_words = from.info_words;
                        s.slots = from.slots.clone();
                        s.objects_word = from.objects_word;
                        s.map_xoro = from.map_xoro;
                        s.map_sha1 = from.map_sha1;
                        s.local_slot = from.local_slot;
                        s.local_player = from.local_player;
                        s.player_name = from.player_name.clone();
                        s.preferences = from.preferences;
                        s.game_ms = from.game_ms;
                        s.game_word = from.game_word;
                    }
                    "client" => objects[1] = theirs[1].clone(),
                    "clocks" => s.objects.as_mut().unwrap().clocks = fm.clocks,
                    "queues" => s.objects.as_mut().unwrap().data[1] = fm.data[1].clone(),
                    "free1" => s.objects.as_mut().unwrap().free[1] = fm.free[1].clone(),
                    "block" => s.objects.as_mut().unwrap().block = fm.block.clone(),
                    "mheader" => s.objects.as_mut().unwrap().header = fm.header.clone(),
                    p if p.starts_with("state:") || p == "script" => state_parts.push(p.trim_start_matches("state:").replace("script", "script data")),
                    other => panic!("unknown piece {other}"),
                }
            }
            if !state_parts.is_empty() {
                let mut g = s.state().unwrap();
                let fg = from.state().unwrap();
                for (name, f) in g.parts.iter_mut() {
                    if state_parts.iter().any(|p| p == name) {
                        *f = fg.parts.iter().find(|(n, _)| n == name).unwrap().1.clone();
                    }
                }
                s.rest = g.to_bytes().unwrap();
            }
            println!("grafted {}", a[5]);
        }
        other => panic!("unknown edit {other}"),
    }
    let m = s.objects.as_mut().unwrap();
    m.set_objects(&objects);
    std::fs::write(&a[2], s.to_file(&header)).unwrap();
}
