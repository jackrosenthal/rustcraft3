//! `w3zhead <save.w3z>`: a saved game's header as the original wrote it.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = w3_formats::w3z::SaveGame::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("map {} game {:?} info {:?} {:?} stat {} bytes words {:?}", s.map, s.game_name, s.info_name, s.info_password, s.info_stat.len(), s.info_words);
    // W3ZHEAD_STAT: the stat's coded bytes.
    if std::env::var_os("W3ZHEAD_STAT").is_some() {
        println!("stat {}", s.info_stat.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }
    println!("slots {:02x?}", s.slots);
    println!("objects word {:08x} map xoro {:08x} sha1 {}", s.objects_word, s.map_xoro, s.map_sha1.iter().map(|b| format!("{b:02x}")).collect::<String>());
    // The objects' sum over the records as read (the word's, when it is the original's).
    match s.objects_sum() {
        Ok(sum) => println!("objects' sum {sum:08x}{}", if sum == s.objects_word { ", the word" } else { "" }),
        Err(e) => println!("objects' sum not reckoned: {e}"),
    }
    println!("local slot {} player {} name {:?} preferences {:?}, game time {} ms", s.local_slot, s.local_player, s.player_name, s.preferences, s.game_ms);
    let Some(m) = &s.objects else {
        println!("no object manager read (not 1.27b's layout), {} bytes after", s.rest.len());
        return;
    };
    println!("objects {} and {}, classes {}, clocks {:?}", m.tables[0].len(), m.tables[1].len(), m.classes.len(), m.clocks);
    // The manager's header (`FUN_6f15c9b0`) as words (reals as reals), and the free
    // lists' slots (each list's words, -1 ending it).
    let words: Vec<String> = m.header.chunks(4).filter(|c| c.len() == 4).enumerate().map(|(k, c)| {
        let w = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        if matches!(k, 2 | 3 | 8 | 10 | 13 | 15 | 18 | 20 | 23 | 25) { format!("{}", f32::from_bits(w)) } else { format!("{w}") }
    }).collect();
    println!("manager header {}", words.join(" "));
    // W3ZHEAD_STATE=<k>: each player's state k (common.j's numbering: 9 placed).
    if let Some(k) = std::env::var("W3ZHEAD_STATE").ok().and_then(|v| v.parse::<u32>().ok()) {
        let states = s.player_states().unwrap_or_default();
        println!("state {k} by player: {:?}", states.iter().map(|p| p.iter().find(|(i, _)| *i == k).map(|(_, v)| *v)).collect::<Vec<_>>());
    }
    // W3ZHEAD_BIRTHS=<from>-<to>: the main table's objects born in that range, by birth.
    if let Some((lo, hi)) = std::env::var("W3ZHEAD_BIRTHS").ok().and_then(|v| v.split_once('-').and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))) {
        let mut v: Vec<_> = m.tables[0].iter().filter(|t| (lo..=hi).contains(&t.birth)).collect();
        v.sort_by_key(|t| t.birth);
        for t in v {
            println!("birth {} slot {} {}", t.birth, t.tag, String::from_utf8_lossy(&t.kind.to_be_bytes()));
        }
    }
    for (k, f) in m.free.iter().enumerate() {
        let slots: Vec<i64> = f.chunks(4).filter(|c| c.len() == 4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64).collect();
        println!("free list {k}: {} words, last {:?}", slots.len(), &slots[slots.len().saturating_sub(14)..]);
    }
    println!("data sections {:?}, bounds {:?}, {} bytes after", m.data.iter().map(|d| d.len()).collect::<Vec<_>>(), m.bounds, s.rest.len());
    // The record written back: whole, then from each part as read.
    let record = w3_formats::container::unpack(&std::fs::read(&a[1]).unwrap()).unwrap().1;
    println!("record written back: {}", if s.to_record() == record { "the same" } else { "differs" });
    if let Ok(objects) = m.objects() {
        let mut back = objects.clone();
        let (mut same, mut differ) = (0, 0);
        for o in back.iter_mut().flatten() {
            let (first, second) = (o.first().unwrap().to_bytes(), o.second().unwrap().to_bytes(o.entry.kind).unwrap());
            if first == o.first && second == o.second {
                same += 1;
            } else {
                if differ < 5 {
                    let code = |c: u32| String::from_utf8_lossy(&c.to_be_bytes()).into_owned();
                    println!("  {} written back differs: first {} of {}, second {} of {}", code(o.entry.kind), first.len(), o.first.len(), second.len(), o.second.len());
                }
                differ += 1;
            }
            (o.first, o.second) = (first, second);
        }
        let mut m2 = m.clone();
        m2.set_objects(&back);
        let mut s2 = s.clone();
        s2.objects = Some(m2);
        let g = s.state().unwrap();
        s2.rest = g.to_bytes().unwrap();
        println!("records written back: {same} the same, {differ} differ; state written back: {}; whole: {}", if s2.rest == s.rest { "the same" } else { "differs" }, if s2.to_record() == record { "the same" } else { "differs" });
        // W3Z_WRITE=<file>: the save as written back, in the port's container.
        if let Ok(out) = std::env::var("W3Z_WRITE") {
            let (h, _) = w3_formats::container::unpack(&std::fs::read(&a[1]).unwrap()).unwrap();
            std::fs::write(&out, s2.to_file(&h)).unwrap();
            println!("wrote {out}");
        }
        if let Some((_, f)) = g.parts.iter().find(|(n, _)| *n == "script data") {
            if let [w3_formats::w3z::Field::Bytes(b)] = &f[..] {
                let d = w3_formats::w3z::ScriptData::parse(b).unwrap();
                println!("script data written back: {}", if &d.to_bytes().unwrap() == b { "the same" } else { "differs" });
            }
        }
    }
    if let Ok(objects) = m.objects() {
        // Each class's count and its records' sizes.
        let mut by: std::collections::BTreeMap<String, (usize, usize, usize)> = Default::default();
        let code = |c: u32| String::from_utf8_lossy(&c.to_le_bytes()).chars().rev().collect::<String>();
        for o in &objects[0] {
            let e = by.entry(format!("{}{}", code(o.entry.kind), code(o.entry.family))).or_default();
            e.0 += 1;
            e.1 = e.1.max(o.first.len());
            e.2 = e.2.max(o.second.len());
        }
        for (k, (n, a, b)) in by {
            println!("  {k} {n} records up to {a} and {b} bytes");
        }
        // The first records read by class, and those that do not.
        let (mut read, mut raw, mut bad) = (0, 0, 0);
        let mut raws: std::collections::BTreeMap<String, usize> = Default::default();
        for o in objects.iter().flatten() {
            match o.first() {
                Ok(w3_formats::w3z::First::Raw(_)) => {
                    raw += 1;
                    *raws.entry(format!("{}{}", code(o.entry.kind), code(o.entry.family))).or_insert(0usize) += 1;
                }
                Ok(_) => read += 1,
                Err(e) => {
                    if bad < 5 {
                        println!("  {:08x} {:08x}: {e}", o.entry.kind, o.entry.family);
                    }
                    bad += 1
                }
            }
        }
        println!("first records: {read} read, {raw} not yet ({raws:?}), {bad} wrong");
        let (mut read, mut raw, mut bad) = (0, 0, 0);
        let mut raws: std::collections::BTreeMap<String, usize> = Default::default();
        for o in objects.iter().flatten() {
            match o.second() {
                Ok(w3_formats::w3z::Second::Raw(_)) => {
                    raw += 1;
                    *raws.entry(code(o.entry.kind)).or_insert(0usize) += 1;
                }
                Ok(_) => read += 1,
                Err(e) => {
                    if bad < 5 {
                        println!("  {}: {e}", code(o.entry.kind));
                    }
                    bad += 1
                }
            }
        }
        println!("second records: {read} read, {raw} not yet ({} kinds), {bad} wrong", raws.len());
        match s.state() {
            Ok(g) => {
                for (name, f) in &g.parts {
                    let mut t = format!("{f:?}");
                    if t.len() > 600 {
                        t = format!("{}...{}", &t[..300], &t[t.len() - 300..]);
                    }
                    println!("state {name}: {t}");
                }
                println!("state: {} bytes after, starting {:02x?}", g.rest.len(), &g.rest[..g.rest.len().min(160)]);
                if let Some((_, f)) = g.parts.iter().find(|(n, _)| *n == "script data") {
                    if let [w3_formats::w3z::Field::Bytes(b)] = &f[..] {
                        match w3_formats::w3z::ScriptData::parse(b) {
                            Ok(d) => {
                                println!("scripts: {} code places, {} thread places", d.code_places, d.thread_places);
                                for (k, c) in d.code.iter().enumerate() {
                                    if let Some(c) = c {
                                        println!("  code {}: {} words, {} strings", k + 1, c.words.len(), c.strings.len());
                                        // W3Z_CODE=<n>: the first n instructions, each its four bytes and word.
                                        if let Ok(n) = std::env::var("W3Z_CODE").map(|v| v.parse::<usize>().unwrap()) {
                                            for (i, w) in c.words.chunks(2).take(n).enumerate() {
                                                println!("    {i:5}: {:02x?} {:#x} ({})", w[0].to_le_bytes(), w[1], w[1] as i32);
                                            }
                                            for (i, t) in c.strings.iter().take(n).enumerate() {
                                                println!("    string {}: {t:?}", i + 1);
                                            }
                                        }
                                    }
                                }
                                for ((place, m), t) in d.machines.iter().zip(&d.threads) {
                                    println!("  machine {place}: code {} at {} returns {:?} functions {} globals {} frames {} stack {} strings {} threads {}", m.code, m.at, m.returns, m.functions.len(), m.globals.len(), m.frames.len(), m.stack.len(), m.strings.len(), t.len());
                                    for (n, v) in m.globals.iter().take(4) {
                                        println!("    global {n}: {v:?}");
                                    }
                                    // W3Z_GLOBALS=<kind>: the globals of a type with a value.
                                    if let Ok(k) = std::env::var("W3Z_GLOBALS") {
                                        for (n, v) in m.globals.iter().filter(|(_, v)| v.kind.to_string() == k && v.value != w3_formats::w3z::Contents::One(0)) {
                                            println!("    global {n}: {v:?}");
                                        }
                                    }
                                    for t in t.iter().take(3) {
                                        println!("    thread of {} at {}: {} frames, stack {}", t.machine, t.at, t.frames.len(), t.stack.len());
                                    }
                                }
                                println!("  owners {:?}", d.owners);
                            }
                            Err(e) => println!("scripts: {e}"),
                        }
                    }
                }
            }
            Err(e) => println!("state: {e}"),
        }
        if std::env::var("W3Z_KINDS").is_ok() {
            let mut v: Vec<_> = raws.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            println!("{v:?}");
        }
    }
}
