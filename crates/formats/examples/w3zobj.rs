//! `w3zobj <save.w3z> <kind> [n]`: a saved game's objects of a kind (as "+w3u"), the
//! first n (default 1), each record decoded, with the objects its references name.
use w3_formats::w3z::{Field, Ref, SavedObject};

fn code(c: u32) -> String {
    String::from_utf8_lossy(&c.to_be_bytes()).into_owned()
}

fn refs(f: &[Field], out: &mut Vec<Ref>) {
    for f in f {
        match f {
            Field::Ref(r) | Field::Timer(r, _, _) => out.push(*r),
            Field::List(g) => g.iter().for_each(|g| refs(g, out)),
            _ => {}
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = w3_formats::w3z::SaveGame::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    let objects = s.objects.as_ref().unwrap().objects().unwrap();
    let all: Vec<&SavedObject> = objects.iter().flatten().collect();
    let by_ref = |r: Ref| all.iter().find(|o| (o.entry.tag, o.entry.birth) == (r.0, r.1));
    // "named": the units, players, and handles as `SaveGame` names them.
    if a[2] == "named" {
        let handles = s.handles().unwrap();
        let units = s.units().unwrap();
        let with = units.iter().filter(|u| handles.iter().any(|(r, _)| (r.0, r.1) == (u.tag, u.birth))).count();
        println!("{} units, {with} with handles; {} handles", units.len(), handles.len());
        for u in units.iter().filter(|u| !handles.iter().any(|(r, _)| (r.0, r.1) == (u.tag, u.birth))) {
            println!("  without a handle: {} owner {} tag {}", code(u.type_id), u.owner, u.tag);
        }
        for u in units.iter().take(3) {
            println!("  {u:?} handle {:?}", handles.iter().find(|(r, _)| (r.0, r.1) == (u.tag, u.birth)).map(|h| h.1));
        }
        for (k, p) in s.player_states().unwrap().iter().enumerate().take(2) {
            println!("  player {k}: {p:?}");
        }
        return;
    }
    // "players": each player's indexed flags.
    if a[2] == "players" {
        for o in all.iter().filter(|o| code(o.entry.kind) == "+ply") {
            let mut line = format!("{}", o.entry.tag);
            if let Ok(w3_formats::w3z::First::Agent { refs, .. }) = o.first() {
                for r in refs {
                    if let Some(Ok(w3_formats::w3z::First::Flag { base, value })) = by_ref(r).map(|t| t.first()) {
                        line += &format!(" {}:{}", base.object.words[0], value as i32);
                    }
                }
            }
            println!("{line}");
        }
        return;
    }
    // "units": each unit's type, its position cluster's reals, and its indexed reals.
    if a[2] == "units" {
        for o in all.iter().filter(|o| code(o.entry.kind) == "+w3u") {
            let Ok(w3_formats::w3z::Second::Unit { widget, .. }) = o.second() else { continue };
            let mut line = format!("{} {} w24 {} s {:?}", o.entry.tag, code(widget.word), widget.owner, widget.shorts);
            if let Ok(w3_formats::w3z::First::Agent { refs, .. }) = o.first() {
                for r in refs {
                    match by_ref(r).map(|t| t.first()) {
                        Some(Ok(w3_formats::w3z::First::Cluster { values, .. })) => line += &format!(" pos {values:?}"),
                        Some(Ok(w3_formats::w3z::First::Real { base, values })) => line += &format!(" rel{} {values:?}", base.object.words[0]),
                        Some(Ok(w3_formats::w3z::First::Flag { base, value })) => line += &format!(" flg{} {value}", base.object.words[0]),
                        Some(Ok(w3_formats::w3z::First::Pos { base, values, .. })) => line += &format!(" pos{} {values:?}", base.object.words[0]),
                        Some(Ok(w3_formats::w3z::First::Raw(_))) | None => line += " ?",
                        Some(Ok(_)) => line += &format!(" {}", by_ref(r).map(|t| code(t.entry.kind)).unwrap_or_default()),
                        Some(Err(e)) => line += &format!(" ({e})"),
                    }
                }
            }
            println!("{line}");
        }
        return;
    }
    // "handles [from] [n]": the handle table in handle order, each handle's object's kind.
    if a[2] == "handles" {
        let mut hs = s.handles().unwrap();
        hs.sort_by_key(|h| h.1);
        let from = a.get(3).map_or(0, |v| u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap());
        let n: usize = a.get(4).map_or(40, |v| v.parse().unwrap());
        for (r, h) in hs.iter().filter(|h| h.1 >= from).take(n) {
            let kind = by_ref(*r).map(|o| format!("{} {}", code(o.entry.kind), code(o.entry.family))).unwrap_or("none".into());
            println!("{h:#x} {r:?} {kind}");
        }
        return;
    }
    // "tag <n>": the object with the tag, its first record, and its owner (its first
    // link's last reference).
    if a[2] == "tag" {
        let t: u32 = a[3].parse().unwrap();
        for o in all.iter().filter(|o| o.entry.tag == t) {
            println!("{} {} birth {}: {:?}", code(o.entry.kind), code(o.entry.family), o.entry.birth, o.first());
            if let Ok(w3_formats::w3z::First::Real { base, .. } | w3_formats::w3z::First::Flag { base, .. }) = o.first() {
                let owner = base.object.links[0].3;
                if let Some(p) = by_ref(owner) {
                    println!("  owner {owner:?}: {} {}", code(p.entry.kind), code(p.entry.family));
                }
            }
        }
        return;
    }
    // "find <hex word>": the objects whose records hold the word, and where.
    if a[2] == "find" {
        let w = u32::from_str_radix(&a[3], 16).unwrap().to_le_bytes();
        let at = |b: &[u8]| b.windows(4).position(|x| x == w);
        for o in &all {
            if let Some(k) = at(&o.first) {
                println!("{} {} tag {}: first record at {k} of {}", code(o.entry.kind), code(o.entry.family), o.entry.tag, o.first.len());
            }
            if let Some(k) = at(&o.second) {
                println!("{} {} tag {}: second record at {k} of {}", code(o.entry.kind), code(o.entry.family), o.entry.tag, o.second.len());
            }
        }
        let m = s.objects.as_ref().unwrap();
        for (k, d) in [&m.header, &m.free[0], &m.free[1], &m.block, &m.data[0], &m.data[1]].iter().enumerate() {
            if let Some(p) = at(d) {
                println!("object manager section {k} at {p}");
            }
        }
        let mut p = 0;
        while let Some(k) = at(&s.rest[p..]) {
            println!("game state at {}", p + k);
            p += k + 1;
        }
        return;
    }
    let n: usize = a.get(3).map_or(1, |n| n.parse().unwrap());
    for o in all.iter().filter(|o| code(o.entry.kind) == a[2]).take(n) {
        println!("{} {} tag {} birth {}", code(o.entry.kind), code(o.entry.family), o.entry.tag, o.entry.birth);
        println!("  first: {:?}", o.first());
        let second = o.second().unwrap();
        let mut rs = Vec::new();
        match &second {
            w3_formats::w3z::Second::Unit { fields, widget, .. } => {
                println!("  widget: {widget:?}");
                for (k, f) in fields.iter().enumerate() {
                    println!("  field {k}: {f:?}");
                }
                refs(fields, &mut rs);
            }
            w3_formats::w3z::Second::Agent { own, tail, .. } => {
                println!("  own: {own:?}\n  tail: {tail:?}");
                refs(tail, &mut rs);
            }
            other => println!("  second: {other:?}"),
        }
        if let Ok(w3_formats::w3z::First::Agent { agent, refs: r, .. }) = o.first() {
            rs.push(agent);
            rs.extend(r);
        }
        for r in rs {
            if r.0 == u32::MAX {
                continue;
            }
            match by_ref(r) {
                Some(t) => {
                    let first = format!("{:?}", t.first());
                    println!("  -> {:?}: {} {}: {}", r, code(t.entry.kind), code(t.entry.family), &first[..first.len().min(400)]);
                }
                None => println!("  -> {r:?}: none"),
            }
        }
    }
}
