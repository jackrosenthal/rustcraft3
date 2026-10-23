//! INI-style profile text files (`UnitFunc.txt`, `war3skins.txt`, `MiscData.txt`, ...).
//! Later files merge into earlier ones, key by key.

use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct Profile {
    /// Lower-cased section name -> lower-cased key -> raw value.
    pub sections: HashMap<String, HashMap<String, String>>,
}

impl Profile {
    pub fn parse(text: &str) -> Profile {
        let mut p = Profile::default();
        p.merge(text);
        p
    }

    pub fn merge(&mut self, text: &str) {
        let mut cur: Option<String> = None;
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}').trim_end();
            if line.is_empty() || line.starts_with("//") || line.starts_with(';') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(end) = rest.find(']') {
                    let name = rest[..end].to_ascii_lowercase();
                    self.sections.entry(name.clone()).or_default();
                    cur = Some(name);
                    continue;
                }
            }
            let (Some(sec), Some(eq)) = (&cur, line.find('=')) else { continue };
            let key = line[..eq].trim().to_ascii_lowercase();
            let val = line[eq + 1..].to_string();
            self.sections.get_mut(sec).unwrap().insert(key, val);
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_ascii_lowercase())?
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// A value with surrounding quotes removed.
    pub fn get_str(&self, section: &str, key: &str) -> Option<String> {
        self.get(section, key).map(|v| unquote(v.trim()).to_string())
    }

    /// The comma-separated fields of a value, honoring quotes.
    pub fn get_list(&self, section: &str, key: &str) -> Vec<String> {
        self.get(section, key).map(split_list).unwrap_or_default()
    }
}

pub fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

pub fn split_list(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in v.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}
