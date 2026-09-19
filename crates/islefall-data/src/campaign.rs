// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! The campaign's chapters: the `offical<N>.english` menus in
//! `netstorm.tarc`, each a title and a list of
//! `$Checked=<label>,MissionBegin,<mission>,...` lines in play order.
//! Two-digit files are the same chapters at other difficulties.

use crate::tarc::Archive;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chapter {
    /// The menu file's name, lower-cased, without its extension.
    pub file: String,
    pub title: String,
    pub missions: Vec<Entry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    /// The mission text's name, lower-cased (`tutorial1`).
    pub mission: String,
}

/// Read one chapter menu; `None` when it lists no missions.
pub fn parse_chapter(file: &str, text: &str) -> Option<Chapter> {
    let mut title = String::new();
    let mut missions = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("$Checked=").or_else(|| t.strip_prefix("$checked=")) {
            let parts: Vec<&str> = rest.split(',').collect();
            if parts.len() >= 3 && parts[1].trim().eq_ignore_ascii_case("MissionBegin") {
                missions.push(Entry { label: parts[0].trim().to_string(), mission: parts[2].trim().to_ascii_lowercase() });
            }
        } else if title.is_empty() {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case("title") {
                    title = v.trim().trim_matches('"').to_string();
                }
            }
        }
    }
    (!missions.is_empty()).then(|| Chapter { file: file.to_ascii_lowercase(), title, missions })
}

/// How hard the campaign's later chapters are: the menu files come in
/// three sets, `offical3` (easy), `offical31` (normal) and `offical32` (hard).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub fn parse(s: &str) -> Option<Difficulty> {
        match s.trim().to_ascii_lowercase().as_str() {
            "easy" => Some(Difficulty::Easy),
            "normal" => Some(Difficulty::Normal),
            "hard" => Some(Difficulty::Hard),
            _ => None,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Difficulty::Easy => "",
            Difficulty::Normal => "1",
            Difficulty::Hard => "2",
        }
    }
}

/// The chapters in order, `offical1`, `offical2` and so on, each from its
/// menu at that difficulty where there is one, else from the plain one.
pub fn chapters(archive: &Archive, difficulty: Difficulty) -> Vec<Chapter> {
    let mut found: std::collections::BTreeMap<u32, (bool, Chapter)> = std::collections::BTreeMap::new();
    for (i, e) in archive.entries().iter().enumerate() {
        if !e.extension().eq_ignore_ascii_case("english") {
            continue;
        }
        let stem = e.basename().to_ascii_lowercase();
        let stem = stem.strip_suffix(".english").unwrap_or(&stem).to_string();
        let Some(digits) = stem.strip_prefix("offical") else { continue };
        if digits.is_empty() || digits.len() > 2 || !digits.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let n = digits[..1].parse::<u32>().unwrap_or(0);
        let exact = &digits[1..] == difficulty.suffix();
        if !exact && digits.len() != 1 {
            continue;
        }
        if found.get(&n).is_some_and(|(was_exact, _)| *was_exact) {
            continue;
        }
        if let Some(c) = parse_chapter(&stem, &archive.read_text(i)) {
            found.insert(n, (exact, c));
        }
    }
    found.into_values().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chapter_menu_lists_its_missions_in_order() {
        let text = "title = \"Early Lessons\"\n\n[Overview]\n<h2>Early Lessons</h2>\n$Checked=1 First Steps,MissionBegin,Lesson1,{DoneLesson1},1\n$Checked=2 Next Steps,MissionBegin,Lesson2,{DoneLesson2},{DoneLesson1}\n$Button=Back,Tell,Menu\n";
        let c = parse_chapter("Offical1", text).unwrap();
        assert_eq!(c.file, "offical1");
        assert_eq!(c.title, "Early Lessons");
        assert_eq!(c.missions, vec![Entry { label: "1 First Steps".into(), mission: "lesson1".into() }, Entry { label: "2 Next Steps".into(), mission: "lesson2".into() }]);
        assert!(parse_chapter("x", "title = \"Nothing\"\n").is_none());
    }
}
