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
    // "vary <kind>": for an agent kind's records, each own and tail field's distinct
    // values (up to six) across its objects, and its handlers' shapes.
    if a[2] == "vary" {
        let kind = a[3].as_bytes();
        let mut own: Vec<std::collections::BTreeMap<String, u32>> = Vec::new();
        let mut tail: Vec<std::collections::BTreeMap<String, u32>> = Vec::new();
        let mut shapes: std::collections::BTreeMap<String, u32> = Default::default();
        let mut n = 0;
        for o in all.iter().filter(|o| o.entry.kind.to_be_bytes() == kind[..4]) {
            let Ok(w3_formats::w3z::Second::Agent { own: ow, handlers, tail: tl }) = o.second() else { continue };
            n += 1;
            for (k, f) in ow.iter().enumerate() {
                if own.len() <= k { own.push(Default::default()); }
                *own[k].entry(format!("{f:?}").replace("Ref(4294967295, 4294967295)", "-")).or_insert(0) += 1;
            }
            for (k, f) in tl.iter().enumerate() {
                if tail.len() <= k { tail.push(Default::default()); }
                *tail[k].entry(format!("{f:?}").replace("Ref(4294967295, 4294967295)", "-")).or_insert(0) += 1;
            }
            let ev: Vec<String> = handlers.events.iter().enumerate().filter(|(_, e)| !e.is_empty()).map(|(k, e)| format!("{k:#x}x{}", e.len())).collect();
            *shapes.entry(format!("keyed {} fixed {:?} events {:?}", handlers.keyed.len(), handlers.fixed.iter().map(|f| f.len()).collect::<Vec<_>>(), ev)).or_insert(0) += 1;
        }
        println!("{n} objects");
        for (k, m) in own.iter().enumerate() {
            let vals: Vec<String> = m.iter().take(6).map(|(v, c)| format!("{v} x{c}")).collect();
            println!("own {k}: {}{}", vals.join(" | "), if m.len() > 6 { format!(" ... ({} values)", m.len()) } else { String::new() });
        }
        for (k, m) in tail.iter().enumerate() {
            let vals: Vec<String> = m.iter().take(6).map(|(v, c)| format!("{v} x{c}")).collect();
            println!("tail {k}: {}{}", vals.join(" | "), if m.len() > 6 { format!(" ... ({} values)", m.len()) } else { String::new() });
        }
        for (s, c) in &shapes {
            println!("handlers x{c}: {s}");
        }
        return;
    }
    // "unitfields <type>": each unit of a type's own record's fields, one a line,
    // numbered.
    if a[2] == "unitfields" {
        for o in all.iter().filter(|o| code(o.entry.kind) == "+w3u") {
            let Ok(w3_formats::w3z::Second::Unit { widget, fields, .. }) = o.second() else { continue };
            if code(widget.word) != a[3] {
                continue;
            }
            println!("unit {}", o.entry.tag);
            for (k, f) in fields.iter().enumerate() {
                println!("  {k} {f:?}");
            }
        }
        return;
    }
    // "unitsec": a unit's own record by its tag, besides its fields: its word, handlers
    // (the event lists not empty), widget, and lists not empty.
    if a[2] == "unitsec" {
        let tag: Option<u32> = a[3].parse().ok();
        for o in all.iter().filter(|o| code(o.entry.kind) == "+w3u" && tag.is_none_or(|t| o.entry.tag == t)) {
            let Ok(w3_formats::w3z::Second::Unit { own, handlers, widget, lists, .. }) = o.second() else { continue };
            println!("unit {} {}", o.entry.tag, code(widget.word));
            println!("own {own:#x}");
            println!("keyed {:?}", handlers.keyed);
            println!("fixed {:?}", handlers.fixed);
            for (k, e) in handlers.events.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
                println!("event {k:#x} {e:?}");
            }
            println!("widget {widget:?}");
            for (k, l) in lists.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
                println!("list {k} {l:?}");
            }
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
    // "secondsizes": each object's birth, kind, and second record's size, by birth.
    if a[2] == "secondsizes" {
        let mut v: Vec<_> = all.iter().map(|o| (o.entry.birth, code(o.entry.kind), o.second.len())).collect();
        v.sort();
        for (b, k, n) in v {
            println!("{b} {k} {n}");
        }
        return;
    }
    // "sizes": the object manager's sections' sizes.
    if a[2] == "sizes" {
        let m = s.objects.as_ref().unwrap();
        println!("header {} tables {} {} free {} {} block {}", m.header.len(), m.tables[0].len(), m.tables[1].len(), m.free[0].len(), m.free[1].len(), m.block.len());
        println!("data {:?}", m.data.iter().map(|d| d.len()).collect::<Vec<_>>());
        println!("rest {}", s.rest.len());
        return;
    }
    // "scripts": the script data's code blocks, machines, and threads in brief.
    if a[2] == "scripts" {
        let d = s.script_data().unwrap();
        println!("code places {} thread places {}", d.code_places, d.thread_places);
        for (k, c) in d.code.iter().enumerate() {
            if let Some(c) = c {
                println!("code {}: {} words, {} strings, the last {:?}", k + 1, c.words.len(), c.strings.len(), &c.strings[c.strings.len().saturating_sub(4)..]);
            }
        }
        for (k, (place, m)) in d.machines.iter().enumerate() {
            println!("machine {k} at {place}: code {} flag {} real {} word {} at {} returns {} functions {} globals {} frames {} stack {} strings {}", m.code, m.flag, m.real, m.word, m.at, m.returns.len(), m.functions.len(), m.globals.len(), m.frames.len(), m.stack.len(), m.strings.len());
            for t in d.threads.get(k).into_iter().flatten() {
                println!("  thread machine {} at {} frames {} flag {} real {} word {} stack {}", t.machine, t.at, t.frames.len(), t.flag, t.real, t.word, t.stack.len());
                for f in &t.frames {
                    println!("    frame word {} arguments {:?} locals {:?}", f.word, f.arguments.iter().map(|v| (v.kind, v.word, v.value.clone())).collect::<Vec<_>>(), f.locals.iter().map(|(n, v)| (n, v.kind, v.word, v.value.clone())).collect::<Vec<_>>());
                }
                let used: Vec<String> = t.stack.iter().enumerate().filter(|(_, v)| v.kind != 0 || v.word != 0 || !matches!(v.value, w3_formats::w3z::Contents::One(0))).map(|(k, v)| format!("{k}:{}/{}/{:?}", v.kind, v.word, v.value)).collect();
                println!("    registers set {}: {:?}", used.len(), &used[..used.len().min(16)]);
            }
        }
        println!("owners {:?}", d.owners);
        if let Some((_, m)) = d.machines.first() {
            let mut kinds: std::collections::BTreeMap<(u32, bool), (u32, String)> = Default::default();
            for (name, v) in &m.globals {
                let arr = matches!(v.value, w3_formats::w3z::Contents::Array(_));
                let e = kinds.entry((v.kind, arr)).or_insert((0, String::new()));
                e.0 += 1;
                if e.1.is_empty() { e.1 = format!("{name} = {:?} word {} length {}", v.value, v.word, v.length); }
            }
            for ((k, arr), (n, ex)) in kinds {
                println!("global kind {k} array {arr}: {n}, e.g. {}", &ex[..ex.len().min(160)]);
            }
            println!("functions {:?}", &m.functions);
            println!("strings {:?}", &m.strings);
            let nonzero: Vec<String> = m.stack.iter().enumerate().filter(|(_, v)| !matches!(v.value, w3_formats::w3z::Contents::One(0)) || v.kind != 0).take(12).map(|(k, v)| format!("{k}:{}/{:?}", v.kind, v.value)).collect();
            println!("registers in use {:?}", nonzero);
        }
        return;
    }
    // "globals": the first machine's globals in order, each its name, kind, word, length,
    // and value.
    if a[2] == "globals" {
        let d = s.script_data().unwrap();
        for (name, v) in &d.machines[0].1.globals {
            println!("{name} {} {} {} {:?}", v.kind, v.word, v.length, v.value);
        }
        return;
    }
    // "fog <out>": the fog's head, its four maps' census of values, and its tail; the
    // maps' shorts to <out> (map by map, row by row).
    if a[2] == "fog" {
        let st = s.state().unwrap();
        let f = &st.parts.iter().find(|(n, _)| *n == "fog").unwrap().1;
        println!("head {:?}", &f[..6]);
        let mut raw = Vec::new();
        for (k, m) in f[6..10].iter().enumerate() {
            let w3_formats::w3z::Field::List(m) = m else { continue };
            let mut c: std::collections::BTreeMap<u16, usize> = Default::default();
            for v in m {
                if let [w3_formats::w3z::Field::Short(x)] = v[..] {
                    *c.entry(x).or_insert(0) += 1;
                    raw.extend(x.to_le_bytes());
                }
            }
            println!("map {k}: {} shorts, {} values: {:?}", m.len(), c.len(), c.iter().take(12).collect::<Vec<_>>());
        }
        println!("tail {:?}", &f[10..]);
        if let Some(out) = a.get(3) {
            std::fs::write(out, raw).unwrap();
        }
        return;
    }
    // "part <name>": a state part's fields, one a line, lists by their lengths and
    // distinct values.
    if a[2] == "part" {
        let g = s.state().unwrap();
        let (_, f) = g.parts.iter().find(|(n, _)| *n == a[3]).expect("no such part");
        for (k, x) in f.iter().enumerate() {
            match x {
                w3_formats::w3z::Field::List(rows) => {
                    let mut seen: Vec<String> = rows.iter().map(|r| format!("{r:?}")).collect();
                    seen.sort();
                    seen.dedup();
                    println!("{k}: list of {}, {} distinct: {}", rows.len(), seen.len(), seen.iter().take(6).cloned().collect::<Vec<_>>().join(" "));
                    if std::env::var("FULL").is_ok_and(|v| v == k.to_string()) {
                        println!("{}", rows.iter().map(|r| match r[..] { [w3_formats::w3z::Field::Byte(b)] => b.to_string(), _ => "?".into() }).collect::<Vec<_>>().join(""));
                    }
                }
                other => println!("{k}: {other:?}"),
            }
        }
        return;
    }
    // "state": the game's state's parts, each its fields in brief.
    if a[2] == "state" {
        let st = s.state().unwrap();
        for (name, fields) in &st.parts {
            let text = format!("{fields:?}");
            println!("{name}: {} fields, {} chars: {}", fields.len(), text.len(), &text[..text.len().min(a.get(3).and_then(|v| v.parse().ok()).unwrap_or(300))]);
        }
        println!("rest {} bytes", st.rest.len());
        return;
    }
    // "grids": each grid's census of cell words, and a sample.
    if a[2] == "grids" {
        let m = s.objects.as_ref().unwrap();
        let p = w3_formats::w3z::PathingMaps::parse(&m.data[0]).unwrap();
        for g in &p.grids {
            let mut c = std::collections::BTreeMap::new();
            for w in &g.cells {
                *c.entry(*w).or_insert(0) += 1;
            }
            println!("grid {}x{}: {:x?}", g.frame.width, g.frame.height, c.iter().map(|(w, n)| (*w, *n)).collect::<Vec<_>>());
        }
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
    // "owners <kind>": that kind's objects' owners (their properties' first link set's
    // last reference), counted by the owner's kind, with their values for flags.
    if a[2] == "owners" {
        let kinds: std::collections::HashMap<u32, String> = all.iter().map(|o| (o.entry.tag, code(o.entry.kind))).collect();
        let mut c = std::collections::BTreeMap::new();
        for o in all.iter().filter(|o| code(o.entry.kind) == a[3]) {
            let owner = match o.first() {
                Ok(w3_formats::w3z::First::Flag { base, value }) => (kinds.get(&base.object.links[0].3 .0).cloned(), Some(value)),
                Ok(w3_formats::w3z::First::Real { base, .. }) | Ok(w3_formats::w3z::First::Pos { base, .. }) => (kinds.get(&base.object.links[0].3 .0).cloned(), None),
                _ => (None, None),
            };
            *c.entry((owner.0.unwrap_or("-".into()), owner.1)).or_insert(0) += 1;
        }
        for (k, v) in c {
            println!("{v} {k:?}");
        }
        return;
    }
    // "classtable": the registry for docs/reference/w3z_classes.txt: code, parent, tag,
    // birth, and, when its definition's array is not the empty default, its four words
    // and each entry's eleven, all in hex.
    if a[2] == "classtable" {
        let m = s.objects.as_ref().unwrap();
        let defs: std::collections::HashMap<u32, &w3_formats::w3z::SavedObject> = all.iter().filter(|o| code(o.entry.kind) == "^adf").map(|o| (o.entry.tag, &**o)).collect();
        for c in &m.classes {
            let mut line = format!("{} {} {:x} {}", code(c[0]), code(c[1]), c[2], c[3]);
            if let Some(Ok(w3_formats::w3z::First::AgentDef { array, .. })) = defs.get(&c[2]).map(|o| o.first()) {
                if array.words != [0, 0, 8, 0] || !array.entries.is_empty() {
                    line += &format!(" {}", array.words.iter().map(|w| format!("{w:x}")).collect::<Vec<_>>().join(","));
                    for e in &array.entries {
                        line += &format!(" {}", e.iter().map(|w| format!("{w:x}")).collect::<Vec<_>>().join(","));
                    }
                }
            }
            println!("{line}");
        }
        return;
    }
    // "table1": the second table's kinds, counted.
    if a[2] == "table1" {
        let m = s.objects.as_ref().unwrap();
        let mut c = std::collections::BTreeMap::new();
        for e in &m.tables[1] {
            *c.entry(code(e.kind)).or_insert(0) += 1;
        }
        println!("{c:?}");
        let non: Vec<_> = m.tables[1].iter().filter(|e| code(e.kind) != "^adf").map(|e| (code(e.kind), e.tag & 0x7fffffff, e.birth)).collect();
        println!("{:?}", &non[..non.len().min(40)]);
        return;
    }
    // "classes": the registered classes, a line each: code, parent, tags.
    if a[2] == "classes" {
        let m = s.objects.as_ref().unwrap();
        for c in &m.classes {
            println!("{} {} {:x} {}", code(c[0]), code(c[1]), c[2], c[3]);
        }
        return;
    }
    // "families": each kind with its family, as the tables hold them.
    if a[2] == "families" {
        let m = s.objects.as_ref().unwrap();
        let mut pairs = std::collections::BTreeSet::new();
        for (k, t) in m.tables.iter().enumerate() {
            for e in t {
                pairs.insert((k, code(e.kind), code(e.family)));
            }
        }
        for (k, kind, family) in pairs {
            println!("{k} {kind} {family}");
        }
        return;
    }
    // "manager": the object manager's small sections in hex, its clocks, words, bounds.
    if a[2] == "manager" {
        let m = s.objects.as_ref().unwrap();
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        println!("header {}", hex(&m.header));
        println!("free0 {} free1 {}", m.free[0].len(), hex(&m.free[1]));
        println!("free0 head {}", hex(&m.free[0][..m.free[0].len().min(64)]));
        println!("block {}", hex(&m.block));
        println!("word {:x} words {:x?} bounds {:?} clocks {:?}", m.word, m.words, m.bounds, m.clocks);
        println!("classes {} first {:x?}", m.classes.len(), &m.classes[..3]);
        println!("game ms {} word {:x}", s.game_ms, s.game_word);
        return;
    }
    // "dumpkind <kind> <file>": each object of a kind, a line each: its tag, birth, and
    // its two records in hex.
    if a[2] == "dumpkind" {
        let mut text = String::new();
        for o in all.iter().filter(|o| code(o.entry.kind) == a[3]) {
            let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
            text += &format!("{} {} {} {}\n", o.entry.tag, o.entry.birth, hex(&o.first), hex(&o.second));
        }
        std::fs::write(&a[4], text).unwrap();
        return;
    }
    // "census": each kind's objects and their records' bytes, most bytes first.
    // "raw": the kinds whose second records the reader keeps raw or cannot read, with
    // counts and an example tag.
    if a[2] == "raw" {
        let mut by: std::collections::BTreeMap<String, (usize, u32)> = Default::default();
        for o in &all {
            let what = match o.second() {
                Ok(w3_formats::w3z::Second::Raw(_)) => "raw",
                Err(_) => "error",
                _ => continue,
            };
            let e = by.entry(format!("{} {what}", code(o.entry.kind))).or_insert((0, o.entry.tag));
            e.0 += 1;
        }
        for (k, (n, t)) in by {
            println!("{k}: {n} (e.g., {t})");
        }
        return;
    }
    // "kind <code>...": each object of the kinds given, its tag and its second record
    // (no references 0xffffffff written out, nor its empty handler lists).
    if a[2] == "kind" {
        for o in all.iter().filter(|o| a[3..].contains(&code(o.entry.kind))) {
            let text = format!("{:?}", o.second()).replace("Word(4294967295)", "W-").replace("Ref(4294967295, 4294967295)", "R-").replace("Timer(R-, 0, Timing(None))", "T-");
            let text = match (text.find("handlers: Handlers {"), text.find(", tail")) {
                (Some(x), Some(y)) if y > x => format!("{}{}", &text[..x], &text[y + 2..]),
                _ => text,
            };
            println!("{} {} {}", code(o.entry.kind), o.entry.tag, text);
        }
        return;
    }
    // "births <from> <to>": the live objects born in that range, by birth.
    if a[2] == "births" {
        let (lo, hi): (u32, u32) = (a[3].parse().unwrap(), a[4].parse().unwrap());
        let mut v: Vec<_> = all.iter().filter(|o| (lo..=hi).contains(&o.entry.birth)).map(|o| (o.entry.birth, o.entry.tag, code(o.entry.kind))).collect();
        v.sort();
        for (b, t, k) in v {
            println!("{b} {t} {k}");
        }
        return;
    }
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
    // "nodes": every timing node in the first records (the property module's), a line
    // each by its count: its count, due, period, word, periodic, and its object's tag
    // and kind.
    if a[2] == "nodes" {
        let mut lines = Vec::new();
        for o in &all {
            let Ok(f) = o.first() else { continue };
            let text = format!("{f:?}");
            let kind = String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).to_string();
            let mut rest = text.as_str();
            while let Some(i) = rest.find("Timing(Some((") {
                rest = &rest[i + 13..];
                let end = rest.find("))").unwrap_or(rest.len());
                let parts: Vec<&str> = rest[..end].split(", ").collect();
                if parts.len() == 5 {
                    lines.push((parts[2].parse::<u32>().unwrap_or(0), format!("{} due {} period {} word {} periodic {} owner {} {kind}", parts[2], parts[0], parts[1], parts[3], parts[4], o.entry.tag)));
                }
            }
        }
        lines.sort();
        for (_, l) in lines {
            println!("{l}");
        }
        return;
    }
    // "probe <tag>": an unread agent's second record: for each count of words after an
    // ability's own fields ("wwww wwwwwww t w") that leaves its handler lists readable,
    // the extra words (hex and as reals) and the rest after the lists.
    if a[2] == "probe" {
        let t: u32 = a[3].parse().unwrap();
        let o = all.iter().find(|o| o.entry.tag == t).expect("no such object");
        let base = a.get(4).map_or("wwww wwwwwww t w", |s| s.as_str());
        for k in 0..24 {
            let layout = format!("{base}{}", " w".repeat(k));
            let Ok((own, h, rest)) = w3_formats::w3z::probe_agent(&o.second, &layout) else { continue };
            if h.events.len() != 0x400 {
                continue;
            }
            let extra: Vec<String> = own.iter().skip(own.len() - k).map(|f| match f { w3_formats::w3z::Field::Word(w) => format!("{w:#x}/{}", f32::from_bits(*w)), x => format!("{x:?}") }).collect();
            let words: Vec<String> = rest.chunks(4).map(|c| { let w = u32::from_le_bytes([c[0], c[1], *c.get(2).unwrap_or(&0), *c.get(3).unwrap_or(&0)]); format!("{w:#x}/{}", f32::from_bits(w)) }).collect();
            println!("{k} extra [{}] keyed {} rest {} bytes: {}", extra.join(" "), h.keyed.len(), rest.len(), words.join(" "));
        }
        return;
    }
    // "timers": every running timer in the agents' second records, a line each: its
    // start count, its owner's tag and kind, its due time, period, and event.
    if a[2] == "timers" {
        fn walk(f: &w3_formats::w3z::Field, out: &mut Vec<String>) {
            use w3_formats::w3z::Field;
            match f {
                Field::Timer(_, _, t) => {
                    if let Some((due, period, count, event, repeat)) = t.0 {
                        out.push(format!("{count} due {due} period {period} event {event:#x} repeat {repeat}"));
                    }
                }
                Field::List(rows) => rows.iter().flatten().for_each(|g| walk(g, out)),
                _ => {}
            }
        }
        let mut lines = Vec::new();
        for o in &all {
            if let Ok(w3_formats::w3z::Second::Agent { own, tail, .. }) = o.second() {
                let mut found = Vec::new();
                own.iter().chain(&tail).for_each(|f| walk(f, &mut found));
                for l in found {
                    let kind = String::from_utf8_lossy(&o.entry.kind.to_be_bytes()).to_string();
                    lines.push((l.split(' ').next().unwrap().parse::<u32>().unwrap_or(0), format!("{l} owner {} {kind}", o.entry.tag)));
                }
            }
        }
        lines.sort();
        for (_, l) in lines {
            println!("{l}");
        }
        return;
    }
    // "fields <tag>": an agent's second record's tail fields, one a line with its index
    // (event lists in brief).
    if a[2] == "fields" {
        let t: u32 = a[3].parse().unwrap();
        for o in all.iter().filter(|o| o.entry.tag == t) {
            if let Ok(w3_formats::w3z::Second::Agent { own, handlers, tail }) = o.second() {
                println!("own {own:?}");
                println!("keyed {:?}", handlers.keyed);
                println!("fixed {:?}", handlers.fixed);
                for (k, f) in tail.iter().enumerate() {
                    let text = format!("{f:?}");
                    if text != "List([])" {
                        let n = if std::env::var("FULL").is_ok() { text.len() } else { text.len().min(400) };
                        println!("{k}: {}", &text[..n]);
                    }
                }
            }
        }
        return;
    }
    // "objtable2": the second table's objects, a line each: tag, kind, birth, and the
    // sizes of its records.
    if a[2] == "objtable2" {
        for o in &objects[1] {
            println!("{} {} {} {} {}", o.entry.tag, code(o.entry.kind), o.entry.birth, o.first.len(), o.second.len());
        }
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
            if std::env::var("SECOND").is_ok() {
                let text = format!("{:?}", o.second());
                // Event lists left out when empty.
                println!("  second ({} bytes): {}", o.second.len(), text.replace(", []", "").replace("[], ", ""));
            }
            if let Ok(w3_formats::w3z::First::Real { base, .. } | w3_formats::w3z::First::Flag { base, .. }) = o.first() {
                let owner = base.object.links[0].3;
                if let Some(p) = by_ref(owner) {
                    println!("  owner {owner:?}: {} {}", code(p.entry.kind), code(p.entry.family));
                }
            }
        }
        return;
    }
    // "dumpall": every object, a line each: tag, kind, family, birth, and both records
    // as read (for checking references across the save).
    if a[2] == "dumpall" {
        for o in &all {
            println!("{} {} {} {} | {:?} | {:?}", o.entry.tag, code(o.entry.kind), code(o.entry.family), o.entry.birth, o.first(), o.second());
        }
        return;
    }
    // "fit <kind> <own> <tail>": for each object of the kind, whether its second record
    // reads exactly as the ability's own fields, its handler lists, and the tail.
    if a[2] == "fit" {
        let (mut ok, mut bad) = (0, 0);
        for o in all.iter().filter(|o| code(o.entry.kind) == a[3]) {
            let fits = match w3_formats::w3z::probe_agent(&o.second, &a[4]) {
                Ok((_, h, rest)) if h.events.len() == 0x400 => matches!(w3_formats::w3z::fit_fields(&rest, &a[5]), Ok((_, 0))),
                _ => false,
            };
            if fits { ok += 1 } else { bad += 1 }
        }
        println!("{} fit {ok} of {}", a[3], ok + bad);
        return;
    }
    // "lens": every object's tag, kind, family, and its records' sizes.
    if a[2] == "lens" {
        for o in &all {
            println!("{} {} {} {} {}", o.entry.tag, code(o.entry.kind), code(o.entry.family), o.first.len(), o.second.len());
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
