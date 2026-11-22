//! Chat lines that play sounds, in the lobby and in a game: /nuke and the soundboard's
//! others, /play <label> for any of the game's sound labels, and /sounds to list them.
//! A sound command goes as chat does; each computer plays it as the line comes (the
//! sender's as it is said), so an original client sees only the line.  The host's
//! dashboard plays it too (`tddash::PageSound`).

use crate::tddash::PageSound;

/// A sound a chat line plays: the soundboard's (its name on the dashboard's page, and its
/// file), or a game sound label.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatSound {
    File(&'static str, &'static str),
    Label(String),
}

/// The program's own files (`tddash::NUKE_FILE`, `tddash::wilhelm_file`), not in the
/// game's archives, by the names the sound plays them under (`register_files`).
pub const NUKE_SOUND: &str = "rustcraft3\\nuke.mp3";
pub const WILHELM_SOUND: &str = "rustcraft3\\wilhelm.wav";

/// The soundboard: the dashboard's sounds, each its own command (/nuke), with its name
/// on the dashboard's page and its file.
fn board() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    crate::tddash::SOUNDS
        .iter()
        .map(|&(n, f)| (n.split('.').next().unwrap_or(n), n, f))
        .chain([("nuke", "nuke.mp3", NUKE_SOUND), ("wilhelm", "wilhelm", WILHELM_SOUND)])
}

fn board_sound(name: &str) -> Option<ChatSound> {
    board().find(|b| b.0.eq_ignore_ascii_case(name)).map(|b| ChatSound::File(b.1, b.2))
}

/// A chat line's /`command`, case aside: what follows it, if the line is that command.
pub fn command<'a>(text: &'a str, command: &str) -> Option<&'a str> {
    let rest = text.trim_start().strip_prefix('/')?;
    let (word, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    word.eq_ignore_ascii_case(command).then(|| args.trim())
}

/// What a chat line plays: a soundboard sound by its own command (/nuke) or by /play,
/// or else, by /play, one of `labels` whatever its case.
pub fn parse(text: &str, labels: &[String]) -> Option<ChatSound> {
    let rest = text.trim_start().strip_prefix('/')?;
    let word = rest.split(char::is_whitespace).next().unwrap_or(rest);
    let name = command(text, "play").unwrap_or(word);
    if let Some(s) = board_sound(name) {
        return Some(s);
    }
    let name = command(text, "play")?;
    labels.iter().find(|l| l.eq_ignore_ascii_case(name)).map(|l| ChatSound::Label(l.clone()))
}

/// Whether a line is for this computer alone, with the lines to show for it: /sounds,
/// or a /play of no sound (which is not said).
pub fn local_reply(text: &str, labels: &[String]) -> Option<Vec<String>> {
    if let Some(filter) = command(text, "sounds") {
        return Some(list(filter, labels));
    }
    let name = command(text, "play")?;
    parse(text, labels).is_none().then(|| vec![note(&format!("No sound is named \"{name}\" (/sounds lists them)."))])
}

fn note(s: &str) -> String {
    format!("|cff999999{s}|r")
}

/// /sounds [filter]: the soundboard, and the labels /play takes matching the filter.
fn list(filter: &str, labels: &[String]) -> Vec<String> {
    const SHOWN: usize = 40;
    let filter = filter.trim().to_ascii_lowercase();
    if filter.is_empty() {
        let board: Vec<String> = board().map(|b| format!("/{}", b.0)).collect();
        return vec![
            note(&format!("Sounds for everyone: {}", board.join(" "))),
            note(&format!("Or /play <name> for any of the game's {} sounds; /sounds <part of a name> lists those that match.", labels.len())),
        ];
    }
    let found: Vec<&String> = labels.iter().filter(|l| l.to_ascii_lowercase().contains(&filter)).collect();
    if found.is_empty() {
        return vec![note(&format!("No sound's name has \"{filter}\" in it."))];
    }
    let shown: Vec<&str> = found.iter().take(SHOWN).map(|s| s.as_str()).collect();
    let more = found.len().saturating_sub(SHOWN);
    let tail = if more > 0 { format!(" (and {more} more)") } else { String::new() };
    vec![note(&format!("{}{tail}", shown.join(", ")))]
}

/// Gives the sound the program's own files, so the soundboard plays them as the game's.
pub fn register_files(a: &mut w3_audio::Audio) {
    if let Ok(b) = std::fs::read(crate::tddash::NUKE_FILE) {
        a.add_file(NUKE_SOUND, b);
    }
    if let Some(b) = crate::tddash::wilhelm_file() {
        a.add_file(WILHELM_SOUND, b);
    }
}

/// Plays a chat line's sound on this computer; what the dashboard's pages play for it.
pub fn play(a: &mut w3_audio::Audio, s: ChatSound) -> Option<PageSound> {
    match s {
        ChatSound::File(page, f) => {
            a.play_file(f, 1.0, 1.0, None, (0.0, 0.0, 0.0), false);
            Some(PageSound::Named(page))
        }
        // A label's variant chosen here, the pages given that one's file.
        ChatSound::Label(l) => {
            let (i, file) = a.choose_variant(&l)?;
            a.play_label_variant(&l, i, None);
            a.file_bytes(&file).map(PageSound::File)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_commands_play_the_soundboard_or_a_label() {
        let labels = vec!["PeasantYesAttack".to_string(), "Footman".to_string()];
        assert_eq!(parse("/nuke", &labels), Some(ChatSound::File("nuke.mp3", NUKE_SOUND)));
        assert_eq!(parse("/Wilhelm", &labels), Some(ChatSound::File("wilhelm", WILHELM_SOUND)));
        assert_eq!(parse("/KaChing please", &labels), Some(ChatSound::File("kaching.wav", "Sound\\Interface\\ReceiveGold.wav")));
        assert_eq!(parse("/play death", &labels), Some(ChatSound::File("death.wav", "Units\\Human\\Footman\\FootmanDeath.wav")));
        assert_eq!(parse("/play peasantyesattack", &labels), Some(ChatSound::Label("PeasantYesAttack".into())));
        // A label plays by /play alone; plain lines and unknown names play nothing.
        assert_eq!(parse("/footman", &labels), None);
        assert_eq!(parse("/play nothing", &labels), None);
        assert_eq!(parse("nuke", &labels), None);
        // /sounds and a /play of no sound are answered here; a sound or a line is said.
        assert!(local_reply("/sounds foot", &labels).is_some_and(|l| l[0].contains("Footman")));
        assert!(local_reply("/play nothing", &labels).is_some_and(|l| l[0].contains("No sound is named")));
        assert!(local_reply("/nuke", &labels).is_none());
        assert!(local_reply("hello", &labels).is_none());
    }
}
