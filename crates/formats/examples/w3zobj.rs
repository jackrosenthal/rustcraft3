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
        for i in s.items().unwrap() {
            println!("  item {} owner {} pos {:?} life {} of {}", code(i.type_id), i.owner, i.pos, i.life, i.max_life);
        }
        for (k, p) in s.player_states().unwrap().iter().enumerate().take(2) {
            println!("  player {k}: {p:?}");
        }
        return;
    }
    // "dests": each destructable's own fields, widget owner and type, and properties.
    if a[2] == "dests" {
        for o in all.iter().filter(|o| code(o.entry.kind) == "+w3d") {
            let Ok(w3_formats::w3z::Second::Destructable { own, widget, bytes, words, .. }) = o.second() else { continue };
            let mut line = format!("{} {} own {:?} w {} {:?} bytes {:?} words {:?}", o.entry.tag, code(widget.word), own, widget.owner, widget.shorts, bytes, words);
            if let Ok(w3_formats::w3z::First::Agent { refs, .. }) = o.first() {
                for r in refs {
                    match by_ref(r).map(|t| t.first()) {
                        Some(Ok(w3_formats::w3z::First::Real { base, values })) => line += &format!(" rel{} {values:?} at {}", base.object.words[0], base.real),
                        Some(Ok(w3_formats::w3z::First::Flag { base, value })) => line += &format!(" flg{} {value}", base.object.words[0]),
                        Some(Ok(w3_formats::w3z::First::Pos { values, .. })) => line += &format!(" pos {values:?}"),
                        Some(Ok(_)) => line += &format!(" {}", by_ref(r).map(|t| code(t.entry.kind)).unwrap_or_default()),
                        _ => line += " ?",
                    }
                }
            }
            println!("{line}");
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
    // "seed": the slot table's random seed (after its slots, nine bytes each).
    // "sizes": the object manager's sections' sizes.
    if a[2] == "sizes" {
        let m = s.objects.as_ref().unwrap();
        println!("header {} tables {} {} free {} {} block {}", m.header.len(), m.tables[0].len(), m.tables[1].len(), m.free[0].len(), m.free[1].len(), m.block.len());
        println!("data {:?}", m.data.iter().map(|d| d.len()).collect::<Vec<_>>());
        println!("rest {}", s.rest.len());
        return;
    }
    // "pathing": the pathing maps' frames and a census of the pathing map's flags.
    if a[2] == "pathing" {
        let m = s.objects.as_ref().unwrap();
        let p = w3_formats::w3z::PathingMaps::parse(&m.data[0]).unwrap();
        assert_eq!(p.to_bytes(), m.data[0], "the pathing maps written back differ");
        println!("refs {:?} tail {:?}", p.refs, p.tail);
        for x in &p.proximity {
            let mut flags = std::collections::BTreeMap::new();
            for c in &x.cells {
                *flags.entry(c.2 >> 24).or_insert(0) += 1;
            }
            println!("proximity {:?} {:?}: {} cells, {} objects, flags {flags:x?}", x.frame, x.words, x.cells.len(), x.cells.iter().map(|c| c.3.len()).sum::<usize>());
        }
        for g in &p.grids {
            println!("grid {:?}", g.frame);
        }
        let kinds: std::collections::HashMap<u32, String> = m.tables.iter().flatten().map(|e| (e.tag, format!("{}/{}", code(e.kind), code(e.family)))).collect();
        let mut census = std::collections::BTreeMap::new();
        for c in &p.proximity[1].cells {
            for r in &c.3 {
                *census.entry(kinds.get(&r.0).cloned().unwrap_or(format!("? {}", r.0))).or_insert(0) += 1;
            }
        }
        println!("objects in cells: {census:?}");
        for r in p.refs.iter().chain(&p.tail) {
            println!("ref {r:?}: {:?}", kinds.get(&r.0));
        }
        // Each region's cells as the pathing map lists them, against its record.
        let mut listed: std::collections::HashMap<u32, Vec<(u32, u32)>> = Default::default();
        for c in &p.proximity[1].cells {
            for r in &c.3 {
                listed.entry(r.0).or_default().push((c.0, c.1));
            }
        }
        let mut census = std::collections::BTreeMap::new();
        let mut shown = 0;
        for o in &all {
            let Ok(w3_formats::w3z::First::Region { refs, word, bounds, words }) = o.first() else { continue };
            let cells = listed.get(&o.entry.tag).cloned().unwrap_or_default();
            let span = (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]);
            let inside = cells.iter().all(|&(x, y)| y >= bounds[0] && y < bounds[2] && x >= bounds[1] && x < bounds[3]);
            let key = (refs[0].0, word >> 24, cells.is_empty(), cells.len() as u32 == span, inside);
            *census.entry(key).or_insert(0) += 1;
            if !cells.is_empty() && cells.len() as u32 != span && shown < 4 {
                shown += 1;
                println!("region {} {word:x} {bounds:?} {words:x?}: {} cells of {span}: {:?}", o.entry.tag, cells.len(), &cells[..cells.len().min(12)]);
            }
        }
        for (k, v) in census {
            println!("map {} kind {:x} none {} full {} inside {}: {v}", k.0, k.1, k.2, k.3, k.4);
        }
        // The order of a cell's objects.
        let mut orders = std::collections::BTreeMap::new();
        for c in p.proximity[1].cells.iter().filter(|c| c.3.len() > 1) {
            let up = c.3.windows(2).all(|w| w[0].0 < w[1].0);
            let down = c.3.windows(2).all(|w| w[0].0 > w[1].0);
            *orders.entry((up, down)).or_insert(0) += 1;
        }
        println!("cells' orders (ascending, descending): {orders:?}");
        return;
    }
    // "w3d": each destructable's record's fields that vary, a line each, tab-separated.
    if a[2] == "w3d" {
        for o in all.iter().filter(|o| code(o.entry.kind) == "+w3d") {
            let Ok(w3_formats::w3z::Second::Destructable { own, handlers, widget, bytes, words }) = o.second() else { continue };
            let sp = widget.sprite.as_ref().unwrap();
            let (rs, pos) = widget.refs.clone().unwrap_or_default();
            println!(
                "{}\t{:?}\t{}\t{:x}\t{}\t{}\t{:x}\t{:?}\t{:?}\t{}\t{}\t{:?}\t{:?}\t{}\t{:x}\t{:?}\t{:?}\t{}\t{:?}\t{}\t{:?}\t{:x?}\t{:?}",
                o.entry.tag, own, sp.model, sp.real.to_bits(), sp.short, sp.byte, sp.word, sp.reals, sp.words, sp.last_real, sp.last_byte, sp.flags, sp.entries,
                widget.owner, widget.word, widget.shorts, rs.iter().map(|r| r.0).collect::<Vec<_>>(), pos[0], pos[1], widget.last_real, (widget.bytes, widget.byte), (bytes, words), handlers.fixed[0]
            );
        }
        return;
    }
    // "census": each kind's objects and their records' bytes, most bytes first.
    if a[2] == "census" {
        let mut by: std::collections::HashMap<String, (usize, usize)> = Default::default();
        for o in &all {
            let e = by.entry(format!("{}/{}", code(o.entry.kind), code(o.entry.family))).or_default();
            e.0 += 1;
            e.1 += o.first.len() + o.second.len() + 16;
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
        let total: usize = v.iter().map(|x| x.1 .1).sum();
        println!("{} kinds, {} bytes", v.len(), total);
        for (k, (n, b)) in v.iter().take(a.get(3).map_or(40, |n| n.parse().unwrap())) {
            println!("{k}: {n} objects, {b} bytes");
        }
        return;
    }
    // "section <k> <file>": the object manager's data section k written to a file.
    if a[2] == "section" {
        let m = s.objects.as_ref().unwrap();
        std::fs::write(&a[4], &m.data[a[3].parse::<usize>().unwrap()]).unwrap();
        return;
    }
    if a[2] == "seed" {
        let n = s.slots[0] as usize;
        let at = 1 + 9 * n;
        println!("{:08x}", u32::from_le_bytes(s.slots[at..at + 4].try_into().unwrap()));
        return;
    }
    // "free": the object manager's free sections, as words.
    if a[2] == "free" {
        let m = s.objects.as_ref().unwrap();
        for (k, f) in m.free.iter().enumerate() {
            let words: Vec<u32> = f.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
            println!("free {k}: {} words {:?}", words.len(), &words[..words.len().min(40)]);
        }
        let h: Vec<u32> = m.header.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        println!("header {:?}", h);
        return;
    }
    // "objtable": the first table's objects, a line each: tag, kind, birth.
    if a[2] == "objtable" {
        for o in &objects[0] {
            println!("{} {} {}", o.entry.tag, code(o.entry.kind), o.entry.birth);
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
