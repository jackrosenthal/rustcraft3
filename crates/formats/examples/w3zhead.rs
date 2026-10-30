//! `w3zhead <save.w3z>`: a saved game's header as the original wrote it.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = w3_formats::w3z::SaveGame::read(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("map {} game {:?} info {:?} {:?} stat {} bytes words {:?}", s.map, s.game_name, s.info_name, s.info_password, s.info_stat.len(), s.info_words);
    println!("slots {:02x?}", s.slots);
    println!("objects word {:08x} map xoro {:08x} sha1 {}", s.objects_word, s.map_xoro, s.map_sha1.iter().map(|b| format!("{b:02x}")).collect::<String>());
    println!("local slot {} player {} name {:?} preferences {:?}, {} bytes after", s.local_slot, s.local_player, s.player_name, s.preferences, s.rest.len());
}
