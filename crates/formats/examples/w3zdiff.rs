//! `w3zdiff <a.w3z> <b.w3z> [kind] [n]`: two saved games' objects compared slot by
//! slot: by kind, the objects only one save has, and those whose records differ, each
//! of the first n (default 3) with its records' first differing fields as decoded.
use std::collections::BTreeMap;

use w3_formats::w3z::{Field, First, SaveGame, Second, SavedObject};

fn code(c: u32) -> String {
    String::from_utf8_lossy(&c.to_be_bytes()).into_owned()
}

/// The fields of a record as decoded, each its place and text.
fn first_fields(o: &SavedObject) -> Vec<(String, String)> {
    match o.first() {
        Ok(First::Agent { object, agent, refs, links }) => vec![("object".into(), format!("{object:?}")), ("agent".into(), format!("{agent:?}")), ("refs".into(), format!("{refs:?}")), ("links".into(), format!("{links:?}"))],
        Ok(f) => vec![("first".into(), format!("{f:?}"))],
        Err(e) => vec![("first".into(), format!("unread: {e:?}"))],
    }
}

fn second_fields(o: &SavedObject) -> Vec<(String, String)> {
    let list = |what: &str, f: &[Field]| f.iter().enumerate().map(|(k, f)| (format!("{what}{k}"), format!("{f:?}"))).collect::<Vec<_>>();
    match o.second() {
        Ok(Second::Agent { own, handlers, tail }) => {
            let mut v = list("own", &own);
            v.push(("keyed".into(), format!("{:?}", handlers.keyed)));
            v.push(("fixed".into(), format!("{:?}", handlers.fixed)));
            for (k, e) in handlers.events.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
                v.push((format!("event {:#x}", 0xd0000 + k), format!("{e:?}")));
            }
            v.extend(list("tail", &tail));
            v
        }
        Ok(Second::Unit { own, handlers, widget, lists, fields }) => {
            let mut v = vec![("own".into(), format!("{own:#x}")), ("keyed".into(), format!("{:?}", handlers.keyed)), ("fixed".into(), format!("{:?}", handlers.fixed))];
            for (k, e) in handlers.events.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
                v.push((format!("event {:#x}", 0xd0000 + k), format!("{e:?}")));
            }
            v.push(("widget".into(), format!("{widget:?}")));
            v.extend(lists.iter().enumerate().map(|(k, l)| (format!("list{k}"), format!("{l:?}"))));
            v.extend(list("field", &fields));
            v
        }
        Ok(Second::Whole(f)) => list("whole", &f),
        Ok(s) => vec![("second".into(), format!("{s:?}"))],
        Err(e) => vec![("second".into(), format!("unread: {e:?}"))],
    }
}

/// The fields that differ between two decodings, by place (a place only one has
/// counted as differing).
fn differing(a: &[(String, String)], b: &[(String, String)]) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let bm: BTreeMap<&str, &str> = b.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let am: BTreeMap<&str, &str> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    for (k, v) in a {
        match bm.get(k.as_str()) {
            Some(w) if *w == v => {}
            w => out.push((k.clone(), v.clone(), w.unwrap_or(&"-").to_string())),
        }
    }
    for (k, v) in b {
        if !am.contains_key(k.as_str()) {
            out.push((k.clone(), "-".into(), v.clone()));
        }
    }
    out
}

fn clip(s: &str) -> String {
    let n = std::env::var("CLIP").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    if s.len() > n { format!("{}...", &s[..s.char_indices().take_while(|(i, _)| *i < n).last().map_or(0, |(i, c)| i + c.len_utf8())]) } else { s.to_string() }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let read = |p: &str| SaveGame::read(&std::fs::read(p).unwrap()).unwrap();
    let (x, y) = (read(&a[1]), read(&a[2]));
    let only = a.get(3).filter(|k| *k != "-").cloned();
    let n: usize = a.get(4).map_or(3, |v| v.parse().unwrap());
    let ox = x.objects.as_ref().unwrap().objects().unwrap();
    let oy = y.objects.as_ref().unwrap().objects().unwrap();
    // W3ZDIFF_SUM: the main table's objects whose part of the objects' sum differs
    // (`SavedObject::sum`), by kind, the first n of each with their first records whole.
    if std::env::var_os("W3ZDIFF_SUM").is_some() {
        let by: BTreeMap<u32, &SavedObject> = oy[0].iter().map(|o| (o.entry.tag, o)).collect();
        let mut count: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for o in &ox[0] {
            let k = code(o.entry.kind);
            let Some(p) = by.get(&o.entry.tag).filter(|p| p.entry.kind == o.entry.kind) else { continue };
            let (s1, s2) = (o.sum(), p.sum());
            if s1.as_ref().ok() == s2.as_ref().ok() {
                continue;
            }
            let c = count.entry(k.clone()).or_default();
            c.0 += 1;
            if c.0 <= n && only.as_ref().is_none_or(|q| *q == k) {
                println!("{k} {} sums {:x?} vs {:x?}\n  {:?}\n  {:?}", o.entry.tag, s1.ok(), s2.ok(), o.first(), p.first());
            }
        }
        for o in &ox[0] {
            if !by.contains_key(&o.entry.tag) {
                count.entry(code(o.entry.kind)).or_default().1 += 1;
            }
        }
        println!("by kind (sum differs, only the first's): {count:?}");
        return;
    }
    for t in 0..2 {
        let bx: BTreeMap<u32, &SavedObject> = ox[t].iter().map(|o| (o.entry.tag, o)).collect();
        let by: BTreeMap<u32, &SavedObject> = oy[t].iter().map(|o| (o.entry.tag, o)).collect();
        // By kind: (in both, the same, only the first's, only the second's, other kinds).
        let mut census: BTreeMap<String, [usize; 5]> = BTreeMap::new();
        let mut shown: BTreeMap<String, usize> = BTreeMap::new();
        for (slot, o) in &bx {
            let k = code(o.entry.kind);
            let e = census.entry(k.clone()).or_default();
            match by.get(slot) {
                None => e[2] += 1,
                Some(p) if p.entry.kind != o.entry.kind => e[4] += 1,
                Some(p) => {
                    e[0] += 1;
                    if p.first == o.first && p.second == o.second && p.entry.birth == o.entry.birth {
                        e[1] += 1;
                        continue;
                    }
                    if only.as_ref().is_some_and(|q| *q != k) {
                        continue;
                    }
                    let c = shown.entry(k.clone()).or_default();
                    *c += 1;
                    if *c > n {
                        continue;
                    }
                    let mut d = Vec::new();
                    if p.entry.birth != o.entry.birth {
                        d.push(("birth".to_string(), o.entry.birth.to_string(), p.entry.birth.to_string()));
                    }
                    if p.first != o.first {
                        d.extend(differing(&first_fields(o), &first_fields(p)));
                    }
                    if p.second != o.second {
                        d.extend(differing(&second_fields(o), &second_fields(p)));
                    }
                    println!("table {t} {k} {slot}: {} fields differ", d.len());
                    for (place, u, v) in d.iter().take(8) {
                        println!("    {place}: {}\n      vs {}", clip(u), clip(v));
                    }
                }
            }
        }
        for (slot, o) in &by {
            if !bx.contains_key(slot) {
                census.entry(code(o.entry.kind)).or_default()[3] += 1;
            }
        }
        println!("table {t}: kind: in both, the same, only the first's, only the second's, another kind in the slot");
        for (k, e) in &census {
            if e[0] != e[1] || e[2] + e[3] + e[4] > 0 {
                println!("  {k}: {e:?}");
            }
        }
    }
    // The pathing map's cells: those whose flags or regions differ.
    let maps = |s: &SaveGame| w3_formats::w3z::PathingMaps::parse(&s.objects.as_ref().unwrap().data[0]).ok();
    if let (Some(a), Some(b)) = (maps(&x), maps(&y)) {
        for k in 0..2 {
            let (ca, cb) = (&a.proximity[k].cells, &b.proximity[k].cells);
            let differ: Vec<usize> = (0..ca.len().max(cb.len())).filter(|&j| ca.get(j) != cb.get(j)).collect();
            println!("pathing map {k}: {} cells, {} differ{}", ca.len(), differ.len(), differ.first().map_or(String::new(), |&j| format!(", the first {:?} vs {:?}", ca.get(j), cb.get(j))));
        }
    }
    // The game state's parts.
    if let (Ok(a), Ok(b)) = (x.state(), y.state()) {
        for ((name, fa), (_, fb)) in a.parts.iter().zip(&b.parts) {
            let first = (0..fa.len().max(fb.len())).find(|&k| fa.get(k) != fb.get(k));
            println!("state {name}: {}", first.map_or("the same".to_string(), |k| format!("field {k}: {} vs {}", clip(&format!("{:?}", fa.get(k))), clip(&format!("{:?}", fb.get(k))))));
        }
    }
}
