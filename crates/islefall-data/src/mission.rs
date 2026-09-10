// SPDX-License-Identifier: Apache-2.0
//! Mission texts: the `*.english` files in `netstorm.tarc`. Each starts
//! with a `[Header]` of `key = value` lines (start money, the players'
//! technologies, which `.fort` to load) before the briefing pages.

use std::collections::BTreeMap;

use crate::tarc::Archive;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mission {
    /// Header keys lower-cased, values with quotes stripped.
    pub header: BTreeMap<String, String>,
}

impl Mission {
    pub fn parse(text: &str) -> Mission {
        let mut header = BTreeMap::new();
        let mut in_header = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                if in_header {
                    break;
                }
                in_header = line.eq_ignore_ascii_case("[header]");
                continue;
            }
            if !in_header || line.is_empty() || line.starts_with("//") {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_end_matches(';').trim().trim_matches('"').to_string();
                header.insert(k.trim().to_ascii_lowercase(), v);
            }
        }
        Mission { header }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.header.get(&key.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }

    /// Type stems (lower-cased) of a `;`-separated tech list such as `myTech`.
    pub fn techs(&self, key: &str) -> Vec<String> {
        self.get(key).map(|v| v.split(';').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
    }

    /// The tech list of computer player `n` (1-based): `ai1Tech`, or the
    /// plain `aiTech` for the only opponent.
    pub fn ai_techs(&self, n: usize) -> Vec<String> {
        let numbered = self.techs(&format!("ai{n}tech"));
        if !numbered.is_empty() {
            return numbered;
        }
        self.techs("aitech")
    }

    /// Find the mission text for a scenario: the `.english` whose header
    /// loads that fort, else the one sharing its name.
    pub fn for_fort(archive: &Archive, fort: &str) -> Option<Mission> {
        let want = fort.to_ascii_lowercase();
        let mut by_name = None;
        for (i, e) in archive.entries().iter().enumerate() {
            if !e.extension().eq_ignore_ascii_case("english") {
                continue;
            }
            let m = Mission::parse(&archive.read_text(i));
            if m.get("loadfort").is_some_and(|f| f.eq_ignore_ascii_case(&want)) {
                return Some(m);
            }
            if e.basename().to_ascii_lowercase() == want {
                by_name = Some(m);
            }
        }
        by_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_header_only() {
        let m = Mission::parse("[Header]\nmissionType = \"Tutorial\"\nloadFort = \"CaptureThePriest\"\nmyStartMoney = 2000\nai2Tech = \"rainCannon;rainBattery\"\ntitle = \"X\"\n[A.]\nnot = this\n");
        assert_eq!(m.get("LOADFORT"), Some("CaptureThePriest"));
        assert_eq!(m.int("myStartMoney"), Some(2000));
        assert_eq!(m.ai_techs(2), vec!["raincannon", "rainbattery"]);
        assert!(m.ai_techs(1).is_empty());
        assert_eq!(m.get("not"), None);
    }
}
