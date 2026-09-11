// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! `*.type` files: NetStorm's unit and object definitions.
//!
//! ```text
//! typename sunwalker [client] [constructor]
//! typeflags walker shadow;
//! {
//!     description = "Golem";
//!     maxHitPoints = 50;
//!     speed = 1.8;
//! }
//! // NORTH
//! A00 : : "golem.gif" #64 : "s_golem.gif" #64;
//! A05 : default gumpframe : "golem.gif" #69 : "s_golem.gif" #69;
//! ```
//!
//! Comments start with `//`. Statements end with `;` except the `typename`
//! line and the braces. Property keys are matched case-insensitively because
//! the shipped files spell some of them both ways. A frame label is a run of
//! letters (the animation, e.g. `A` for north, `AA` for a second set) followed
//! by digits (the frame number). Each frame lists one or more image
//! references, `"file.gif" #index`; the second one, when present, is the
//! shadow.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    /// A bare word that is neither a number nor a string.
    Ident(String),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Ident(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Ints are accepted where a float is expected (`speed = 1;`).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// Image file name as written, e.g. `golem.gif`.
    pub file: String,
    /// Frame index inside that image.
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameDef {
    /// Full label as written, e.g. `A05`.
    pub label: String,
    /// Letter part of the label: the animation name.
    pub animation: String,
    /// Digit part of the label.
    pub number: u32,
    /// Flags between the first two colons, e.g. `default`, `solid`, `dirt`.
    pub flags: Vec<String>,
    /// Image references in order; the second is the shadow when present.
    pub images: Vec<ImageRef>,
}

impl FrameDef {
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f.eq_ignore_ascii_case(flag))
    }

    pub fn image(&self) -> Option<&ImageRef> {
        self.images.first()
    }

    pub fn shadow(&self) -> Option<&ImageRef> {
        self.images.get(1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: String,
    /// Words after the name on the `typename` line, e.g. `constructor`.
    pub modifiers: Vec<String>,
    pub flags: Vec<String>,
    /// Properties in file order.
    pub properties: Vec<(String, Value)>,
    /// Frames in file order.
    pub frames: Vec<FrameDef>,
}

impl TypeDef {
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f.eq_ignore_ascii_case(flag))
    }

    /// First property whose key matches case-insensitively.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.properties.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_i64)
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Value::as_f64)
    }

    /// Distinct animation names in file order.
    pub fn animations(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for f in &self.frames {
            if !out.contains(&f.animation.as_str()) {
                out.push(&f.animation);
            }
        }
        out
    }

    /// Frames of one animation, in file order.
    pub fn animation(&self, name: &str) -> impl Iterator<Item = &FrameDef> {
        self.frames.iter().filter(move |f| f.animation.eq_ignore_ascii_case(name))
    }

    /// Image files referenced, in first-use order.
    pub fn image_files(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for r in self.frames.iter().flat_map(|f| &f.images) {
            if !out.iter().any(|f| f.eq_ignore_ascii_case(&r.file)) {
                out.push(&r.file);
            }
        }
        out
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Parse one `.type` file.
pub fn parse(text: &str) -> Result<TypeDef, ParseError> {
    let mut def = TypeDef { name: String::new(), modifiers: Vec::new(), flags: Vec::new(), properties: Vec::new(), frames: Vec::new() };
    let mut in_block = false;
    let mut seen_typename = false;
    // A statement may span lines; it ends at ';' outside quotes.
    let mut pending = String::new();
    let mut pending_line = 0;

    for (i, raw) in text.lines().enumerate() {
        let lineno = i + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if pending.is_empty() {
            if let Some(rest) = line.strip_prefix("typename") {
                if !rest.starts_with(char::is_whitespace) {
                    return Err(ParseError { line: lineno, message: "expected whitespace after typename".into() });
                }
                let mut words = rest.split_whitespace();
                def.name = words.next().ok_or(ParseError { line: lineno, message: "typename without a name".into() })?.to_string();
                def.modifiers = words.map(str::to_string).collect();
                seen_typename = true;
                continue;
            }
            if line == "{" {
                if in_block {
                    return Err(ParseError { line: lineno, message: "nested '{'".into() });
                }
                in_block = true;
                continue;
            }
            if line == "}" {
                if !in_block {
                    return Err(ParseError { line: lineno, message: "'}' without '{'".into() });
                }
                in_block = false;
                continue;
            }
            pending_line = lineno;
        }
        pending.push_str(line);
        pending.push(' ');
        // Dispatch every complete statement in the buffer.
        while let Some(end) = find_unquoted(&pending, ';') {
            let stmt = pending[..end].trim().to_string();
            pending = pending[end + 1..].trim_start().to_string();
            statement(&mut def, &stmt, in_block, pending_line)?;
        }
    }
    if !pending.trim().is_empty() {
        return Err(ParseError { line: pending_line, message: "statement without ';'".into() });
    }
    if in_block {
        return Err(ParseError { line: text.lines().count(), message: "missing '}'".into() });
    }
    if !seen_typename {
        return Err(ParseError { line: 1, message: "no typename".into() });
    }
    Ok(def)
}

fn statement(def: &mut TypeDef, stmt: &str, in_block: bool, line: usize) -> Result<(), ParseError> {
    if let Some(rest) = stmt.strip_prefix("typeflags") {
        def.flags = rest.split_whitespace().map(str::to_string).collect();
        return Ok(());
    }
    if in_block {
        let (key, value) = stmt.split_once('=').ok_or(ParseError { line, message: format!("expected 'key = value', got '{stmt}'") })?;
        def.properties.push((key.trim().to_string(), parse_value(value.trim())));
        return Ok(());
    }
    // Frame: LABEL : flags : "file" #n [: "file" #n ...]
    let parts: Vec<&str> = split_unquoted(stmt, ':');
    if parts.len() < 3 {
        return Err(ParseError { line, message: format!("expected a frame definition, got '{stmt}'") });
    }
    let label = parts[0].trim();
    let letters = label.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let number = label[letters..].parse::<u32>();
    if letters == 0 || number.is_err() {
        return Err(ParseError { line, message: format!("bad frame label '{label}'") });
    }
    let mut images = Vec::new();
    for part in &parts[2..] {
        let part = part.trim();
        let (file, idx) = part.rsplit_once('#').ok_or(ParseError { line, message: format!("expected '\"file\" #n', got '{part}'") })?;
        let file = file.trim().trim_matches('"').to_string();
        let index = idx.trim().parse::<u32>().map_err(|_| ParseError { line, message: format!("bad frame index in '{part}'") })?;
        images.push(ImageRef { file, index });
    }
    def.frames.push(FrameDef {
        label: label.to_string(),
        animation: label[..letters].to_string(),
        number: number.unwrap(),
        flags: parts[1].split_whitespace().map(str::to_string).collect(),
        images,
    });
    Ok(())
}

fn parse_value(v: &str) -> Value {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return Value::Str(v[1..v.len() - 1].to_string());
    }
    if let Ok(i) = v.parse::<i64>() {
        return Value::Int(i);
    }
    if let Ok(f) = v.parse::<f64>() {
        return Value::Float(f);
    }
    Value::Ident(v.to_string())
}

fn strip_comment(line: &str) -> &str {
    match find_unquoted(line, '/') {
        Some(i) if line[i..].starts_with("//") => &line[..i],
        _ => line,
    }
}

/// Byte index of the first `c` outside double quotes.
fn find_unquoted(s: &str, c: char) -> Option<usize> {
    let mut quoted = false;
    for (i, ch) in s.char_indices() {
        if ch == '"' {
            quoted = !quoted;
        } else if ch == c && !quoted {
            return Some(i);
        }
    }
    None
}

fn split_unquoted(s: &str, c: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = find_unquoted(rest, c) {
        out.push(&rest[..i]);
        rest = &rest[i + c.len_utf8()..];
    }
    out.push(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLEM: &str = r#"typename sunwalker constructor
typeflags walker shadow;
{
	description="Golem";   // trailing comment
	maxHitPoints = 50;
	speed = 1.8;
	turningSpeed = 99999.0;
//	activeSound = "moveBeetle.wav";
	hotfootratiox = 0.5;
}

// NORTH
A00 : : "golem.gif" #64 : "s_golem.gif" #64;
A05 : default gumpframe : "golem.gif" #69 : "s_golem.gif" #69;
AA01 : : "golem.gif" #70;
"#;

    #[test]
    fn parses_golem() {
        let t = parse(GOLEM).unwrap();
        assert_eq!(t.name, "sunwalker");
        assert_eq!(t.modifiers, vec!["constructor"]);
        assert!(t.has_flag("shadow"));
        assert!(!t.has_flag("bomb"));
        assert_eq!(t.get_str("description"), Some("Golem"));
        assert_eq!(t.get_i64("maxHitPoints"), Some(50));
        assert_eq!(t.get_f64("speed"), Some(1.8));
        assert_eq!(t.get_f64("maxHitPoints"), Some(50.0));
        assert_eq!(t.get_f64("hotFootRatioX"), Some(0.5));
        assert_eq!(t.get("activeSound"), None);
        assert_eq!(t.frames.len(), 3);
        let f = &t.frames[1];
        assert_eq!((f.label.as_str(), f.animation.as_str(), f.number), ("A05", "A", 5));
        assert!(f.has_flag("default") && f.has_flag("GumpFrame"));
        assert_eq!(f.image().unwrap(), &ImageRef { file: "golem.gif".into(), index: 69 });
        assert_eq!(f.shadow().unwrap().file, "s_golem.gif");
        assert_eq!(t.frames[2].animation, "AA");
        assert!(t.frames[2].shadow().is_none());
        assert_eq!(t.animations(), vec!["A", "AA"]);
        assert_eq!(t.animation("a").count(), 2);
        assert_eq!(t.image_files(), vec!["golem.gif", "s_golem.gif"]);
    }

    #[test]
    fn empty_typeflags_and_multiline_statement() {
        let t = parse("typename x\ntypeflags ;\n{\n a = 1\n ;\n}\nP01 : solid dirt :\n \"a.gif\" #2;\n").unwrap();
        assert!(t.flags.is_empty());
        assert_eq!(t.get_i64("a"), Some(1));
        assert_eq!(t.frames[0].flags, vec!["solid", "dirt"]);
    }

    #[test]
    fn errors_carry_line_numbers() {
        let e = parse("typename x\n{\n oops;\n}\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = parse("typename x\nA0x : : \"a.gif\" #1;\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(parse("typeflags a;\n").is_err());
        assert!(parse("typename x\n{\n").is_err());
    }

    /// Parse every type file in the real archive.
    #[test]
    fn real_types_if_available() {
        let Ok(dir) = std::env::var("NETSTORM_DIR") else {
            eprintln!("NETSTORM_DIR not set; skipping real-file test");
            return;
        };
        let a = crate::tarc::Archive::load(format!("{dir}/netstorm.tarc")).expect("load netstorm.tarc");
        let mut n = 0;
        for (i, e) in a.entries().iter().enumerate() {
            if e.extension() != "type" {
                continue;
            }
            let t = parse(&a.read_text(i)).unwrap_or_else(|err| panic!("{}: {err}", e.basename()));
            assert!(!t.name.is_empty());
            n += 1;
        }
        assert_eq!(n, 124);
        let i = a.find("sunwalker.type").unwrap();
        let t = parse(&a.read_text(i)).unwrap();
        assert_eq!(t.get_str("theme"), Some("sun"));
        assert_eq!(t.get_i64("maxHitPoints"), Some(50));
        assert!(t.animations().len() >= 8, "at least eight walking directions");
        assert!(t.animation("A").all(|f| f.shadow().is_some()), "walker frames carry shadows");
    }
}
