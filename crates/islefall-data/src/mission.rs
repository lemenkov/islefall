// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Mission texts: the `*.english` files in `netstorm.tarc`. Each starts
//! with a `[Header]` of `key = value` lines (start money, the players'
//! technologies, which `.fort` to load) before the briefing pages:
//! sections named `[A.]`, `[A1.]`, `[B.]` and so on in a small markup
//! (`<h2>`, `<p>`, `<br>`, `<i>`, `<c>`) with `$Button=label,action,arg`
//! lines, and result sections such as `[Succeeded][BadTeamDead]` and
//! `[Failed]`.

use std::collections::BTreeMap;

use crate::tarc::Archive;

/// A page of a mission's text, ready to show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// The markup as plain text: headings and paragraphs on their own lines.
    pub text: String,
    pub buttons: Vec<Button>,
}

/// `$Button=label,action,arg`: `Tell` opens the page `arg`, `DoNothing`
/// closes the text, `MissionBegin` starts the mission `arg`, `LeaveBattle`
/// leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub action: String,
    pub arg: String,
}

/// The markup as plain text.
pub fn plain_text(markup: &str) -> String {
    let mut out = String::new();
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open..];
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_ascii_lowercase();
        match tag.as_str() {
            "p" | "/h2" | "/h1" | "/h3" => out.push_str("\n\n"),
            "br" => out.push('\n'),
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    // Lines of one paragraph run together; blank lines part paragraphs.
    let mut paragraphs: Vec<String> = Vec::new();
    for block in out.split("\n\n") {
        let lines: Vec<String> = block.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
        let joined = if block.contains('\n') && lines.iter().filter(|l| !l.is_empty()).count() > 1 && !block.trim_start().starts_with(char::is_whitespace) {
            lines.iter().filter(|l| !l.is_empty()).cloned().collect::<Vec<_>>().join(" ")
        } else {
            lines.join(" ").trim().to_string()
        };
        if !joined.trim().is_empty() {
            paragraphs.push(joined.trim().to_string());
        }
    }
    paragraphs.join("\n\n")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mission {
    /// Header keys lower-cased, values with quotes stripped.
    pub header: BTreeMap<String, String>,
    /// Every section after the header: its name line lower-cased, as
    /// written (`[a.]`, `[succeeded][badteamdead]`), and its body.
    pub sections: Vec<(String, String)>,
}

impl Mission {
    pub fn parse(text: &str) -> Mission {
        let mut header = BTreeMap::new();
        let mut sections: Vec<(String, String)> = Vec::new();
        let mut in_header = false;
        let mut seen_header = false;
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_header = !seen_header && line.eq_ignore_ascii_case("[header]");
                if in_header {
                    seen_header = true;
                } else {
                    sections.push((line.to_ascii_lowercase(), String::new()));
                }
                continue;
            }
            if in_header {
                if line.is_empty() || line.starts_with("//") {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim().trim_end_matches(';').trim().trim_matches('"').to_string();
                    header.insert(k.trim().to_ascii_lowercase(), v);
                }
            } else if let Some((_, body)) = sections.last_mut() {
                body.push_str(raw.trim_end_matches('\r'));
                body.push('\n');
            }
        }
        Mission { header, sections }
    }

    /// The mission text of that name (`tutorial1`, `thewarbegins`).
    pub fn load(archive: &Archive, stem: &str) -> Option<Mission> {
        let index = archive.find(&format!("{stem}.english"))?;
        Some(Mission::parse(&archive.read_text(index)))
    }

    /// The scenario this mission plays on: the header's `loadFort`, else
    /// the mission's own name, else that name without its difficulty.
    pub fn fort_name(&self, stem: &str, archive: &Archive) -> Option<String> {
        let stem = stem.to_ascii_lowercase();
        let mut candidates: Vec<String> = Vec::new();
        if let Some(f) = self.get("loadfort") {
            candidates.push(f.to_ascii_lowercase());
        }
        candidates.push(stem.clone());
        for suffix in ["easy", "hard"] {
            if let Some(base) = stem.strip_suffix(suffix) {
                candidates.push(base.to_string());
            }
        }
        candidates.into_iter().find(|c| archive.find(&format!("{c}.fort")).is_some())
    }

    fn page_of(body: &str) -> Page {
        let mut markup = String::new();
        let mut buttons = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("$Button=").or_else(|| t.strip_prefix("$button=")) {
                let mut parts = rest.splitn(3, ',');
                let label = parts.next().unwrap_or("").trim().to_string();
                let action = parts.next().unwrap_or("").trim().to_string();
                let arg = parts.next().unwrap_or("").trim().to_string();
                buttons.push(Button { label, action, arg });
            } else if t.starts_with('$') || t.starts_with("<$") {
                // Engine commands (`$Checked`, `<$Config,...>`): not text.
            } else {
                markup.push_str(line);
                markup.push('\n');
            }
        }
        Page { text: plain_text(&markup), buttons }
    }

    /// The page named `name` (`A.`, `a1.`, with or without brackets).
    pub fn page(&self, name: &str) -> Option<Page> {
        let want = format!("[{}]", name.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase());
        self.sections.iter().find(|(n, _)| *n == want).map(|(_, b)| Mission::page_of(b))
    }

    /// The lesson and briefing pages in file order: sections named by a
    /// letter, an optional number and a full stop.
    pub fn pages(&self) -> Vec<String> {
        self.sections
            .iter()
            .map(|(n, _)| n.trim_matches(|c| c == '[' || c == ']').to_string())
            .filter(|n| {
                let mut c = n.chars();
                c.next().is_some_and(|f| f.is_ascii_alphabetic()) && n.ends_with('.') && n[1..n.len() - 1].chars().all(|d| d.is_ascii_digit())
            })
            .collect()
    }

    /// What the mission says when it is won or lost: the first section
    /// tagged `[Succeeded]` or `[Failed]`.
    pub fn result(&self, succeeded: bool) -> Option<Page> {
        let tag = if succeeded { "[succeeded]" } else { "[failed]" };
        self.sections.iter().find(|(n, _)| n.contains(tag)).map(|(_, b)| Mission::page_of(b))
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
            let base = e.basename().to_ascii_lowercase();
            if base.strip_suffix(".english").unwrap_or(&base) == want {
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

    #[test]
    fn pages_buttons_and_results_are_read() {
        let text = "[Header]\nloadFort = \"Somewhere\"\ntitle=\"A Test\"\n\n[A.]\n<h2>Hello</h2>\n<p>\nFirst line\nof a paragraph.\n<p>\n<i>Second</i> one.<br>After a break.\n$Button=MORE,Tell,A1.\n$Button=Play Mission,DoNothing,0\n\n[A1.]\nMore.\n$Button=BACK,Tell,A.\n\n[Succeeded][BadTeamDead]\n<h2>Success!</h2>\n<$Config,Done=1>\nWell done.\n\n[Failed]\nAlas.\n[END]\n";
        let m = Mission::parse(text);
        assert_eq!(m.get("loadfort"), Some("Somewhere"));
        assert_eq!(m.pages(), vec!["a.".to_string(), "a1.".to_string()]);
        let a = m.page("A.").unwrap();
        assert_eq!(a.text, "Hello\n\nFirst line of a paragraph.\n\nSecond one. After a break.");
        assert_eq!(a.buttons.len(), 2);
        assert_eq!((a.buttons[0].label.as_str(), a.buttons[0].action.as_str(), a.buttons[0].arg.as_str()), ("MORE", "Tell", "A1."));
        assert_eq!(m.page("[a1.]").unwrap().buttons[0].arg, "A.");
        assert_eq!(m.result(true).unwrap().text, "Success!\n\nWell done.");
        assert_eq!(m.result(false).unwrap().text, "Alas.");
        assert!(m.page("Z.").is_none());
    }
}
