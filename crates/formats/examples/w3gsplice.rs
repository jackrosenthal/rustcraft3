//! `w3gsplice <template.w3g> <actions> <out.w3g> [turns]`: a replay with a template's
//! record (its game, map, slots, and seed) and the actions of an actions file ("turn
//! pid hex" a line, as the `tags` example takes them) in place of its turns' own, cut
//! to `turns` turns.  Its checksums are the template's (a replay played back is not
//! checked).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut r = w3_formats::w3g::Replay::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let mut acts: std::collections::BTreeMap<u32, Vec<(u8, Vec<u8>)>> = Default::default();
    for l in std::fs::read_to_string(&a[2]).unwrap().lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = l.split_whitespace().collect();
        if w.len() < 3 {
            continue;
        }
        let bytes = (0..w[2].len() / 2).map(|i| u8::from_str_radix(&w[2][2 * i..2 * i + 2], 16).unwrap()).collect();
        acts.entry(w[0].parse().unwrap()).or_default().push((w[1].parse().unwrap(), bytes));
    }
    let limit: u32 = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(u32::MAX);
    let (mut turn, mut out) = (0u32, Vec::new());
    for rec in r.records.drain(..) {
        if let w3_formats::w3g::Record::Turn { ms, .. } = rec {
            turn += 1;
            if turn > limit {
                break;
            }
            out.push(w3_formats::w3g::Record::Turn { ms, actions: acts.get(&turn).cloned().unwrap_or_default() });
            continue;
        }
        out.push(rec);
    }
    r.records = out;
    r.length_ms = turn.min(limit) * 100;
    std::fs::write(&a[3], r.to_bytes()).unwrap();
    println!("{} turns, {} with actions", turn.min(limit), acts.len());
}
