//! `jasscmp <save.w3z> <code block> <source files...>`: the sources compiled as the
//! original compiles them (`w3_jass::bytecode`) against a save's code block: the first
//! instruction and string that differ.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = w3_formats::w3z::SaveGame::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    let d = s.script_data().unwrap();
    let code = d.code[a[2].parse::<usize>().unwrap() - 1].as_ref().unwrap();
    let srcs: Vec<(String, String)> = a[3..].iter().map(|f| (f.clone(), String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned())).collect();
    let ours = w3_jass::bytecode::compile(srcs.iter().map(|(n, s)| (n.as_str(), s.as_str()))).unwrap();
    println!("words: ours {}, theirs {}; strings: ours {}, theirs {}", ours.words.len(), code.words.len(), ours.strings.len(), code.strings.len());
    match ours.strings.iter().zip(&code.strings).position(|(x, y)| x != y) {
        Some(k) => println!("strings differ at {}: ours {:?}, theirs {:?}", k + 1, &ours.strings[k..(k + 4).min(ours.strings.len())], &code.strings[k..(k + 4).min(code.strings.len())]),
        None => println!("strings agree for {}", ours.strings.len().min(code.strings.len())),
    }
    let name = |strings: &[String], id: u32| strings.get(id as usize - 1).cloned().unwrap_or_default();
    match ours.words.chunks(2).zip(code.words.chunks(2)).position(|(x, y)| x != y) {
        Some(k) => {
            println!("instructions differ at {k}:");
            for (tag, words, strings) in [("ours", &ours.words, &ours.strings), ("theirs", &code.words, &code.strings)] {
                for i in k.saturating_sub(6)..(k + 6).min(words.len() / 2) {
                    let [b0, b1, b2, o] = words[2 * i].to_le_bytes();
                    let arg = words[2 * i + 1];
                    let n = if (1..=strings.len() as u32).contains(&arg) && matches!(o, 3 | 5..=10 | 0xe | 0xf | 0x10 | 0x11 | 0x12 | 0x15 | 0x16) { name(strings, arg) } else { String::new() };
                    println!("  {tag:6} {i:6}: {b0:3} {b1:3} {b2:3} {o:#04x} {arg} {n}");
                }
            }
        }
        None => println!("instructions agree for {}", ours.words.len().min(code.words.len()) / 2),
    }
}
