//! Short HUD labels per card (`data/labels.json`) and keyword colors (`data/keywords.json`).
//! Both files are edited by hand and reloaded when they change. Port of `prototype/labels.py`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use fancy_regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::fill::{VALUE_SLOT_RE, find_all};

/// "#" = next value, "#2" = second value.
static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\d+)?").unwrap());
/// Sign, value or range, percent.
static SLOT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(-?)(\(-?[\d.]+--?[\d.]+\)|\d+(?:\.\d+)?)(%?)").unwrap());

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Label {
    pub alias: String,
    pub icon: String,
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// `{"labels": {id: {alias, icon}}}`; a missing or broken file gives an empty map.
pub fn load_labels(path: &Path) -> HashMap<String, Label> {
    let Some(Value::Object(entries)) = read_json(path).and_then(|v| v.get("labels").cloned()) else {
        return HashMap::new();
    };
    let field = |e: &Value, key: &str| e.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
    entries
        .iter()
        .map(|(id, e)| (id.clone(), Label { alias: field(e, "alias"), icon: field(e, "icon") }))
        .collect()
}

/// `{"keywords": {word: color}}`, longest word first; a missing or broken file gives an empty list.
pub fn load_keywords(path: &Path) -> Vec<(String, String)> {
    let Some(Value::Object(entries)) = read_json(path).and_then(|v| v.get("keywords").cloned()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> =
        entries.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
    out.sort_by_key(|(k, _)| std::cmp::Reverse(k.chars().count()));
    out
}

/// Fills the alias placeholders with the values of the displayed mod text.
///
/// Values not read in game stay as their range ("(15-30)"); placeholders without a value stay "#".
pub fn render_alias(alias: &str, display: &[String]) -> String {
    let values: Vec<&str> = display.iter().flat_map(|l| find_all(&VALUE_SLOT_RE, l)).collect();
    let mut next_slot = 0usize;
    let mut out = String::new();
    let mut last = 0;
    for caps in PLACEHOLDER_RE.captures_iter(alias) {
        let caps = caps.unwrap();
        let whole = caps.get(0).unwrap();
        out.push_str(&alias[last..whole.start()]);
        last = whole.end();
        let slot = match caps.get(1) {
            Some(n) => n.as_str().parse::<usize>().ok().and_then(|n| n.checked_sub(1)),
            None => {
                next_slot += 1;
                Some(next_slot - 1)
            }
        };
        out.push_str(slot.and_then(|n| values.get(n).copied()).unwrap_or(whole.as_str()));
    }
    out.push_str(&alias[last..]);
    out
}

/// A piece of HUD text; `color` is a CSS color for keywords. The page sets it as text, never HTML.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Segment {
    pub text: String,
    pub color: Option<String>,
}

fn push_plain(out: &mut Vec<Segment>, text: &str) {
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(Segment { color: None, text: last }) => last.push_str(text),
        _ => out.push(Segment { text: text.to_string(), color: None }),
    }
}

#[cfg(test)]
thread_local! {
    /// Keyword regexes built on this thread (tests only).
    static REGEX_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Colors keywords in HUD text. Built once per keywords reload: the regex is not rebuilt per mod.
pub struct KeywordMatcher {
    /// (lowercase word, color), longest word first.
    colors: Vec<(String, String)>,
    re: Option<Regex>,
}

impl KeywordMatcher {
    pub fn new(keywords: &[(String, String)]) -> KeywordMatcher {
        let mut sorted: Vec<&(String, String)> = keywords.iter().collect();
        sorted.sort_by_key(|(k, _)| std::cmp::Reverse(k.chars().count()));
        let words: Vec<String> = sorted.iter().map(|(k, _)| fancy_regex::escape(k).into_owned()).collect();
        let re = if words.is_empty() {
            None
        } else {
            #[cfg(test)]
            REGEX_BUILDS.with(|c| c.set(c.get() + 1));
            Regex::new(&format!(r"(?i)\b(?:{})\b", words.join("|"))).ok()
        };
        let colors = sorted.into_iter().map(|(k, c)| (k.to_lowercase(), c.clone())).collect();
        KeywordMatcher { colors, re }
    }

    /// The text split into segments, every keyword (whole word, any case) in its color.
    pub fn color(&self, text: &str) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        let Some(re) = &self.re else {
            push_plain(&mut out, text);
            return out;
        };
        let mut last = 0;
        for m in re.find_iter(text) {
            let m = m.unwrap();
            let lower = m.as_str().to_lowercase();
            push_plain(&mut out, &text[last..m.start()]);
            match self.colors.iter().find(|(k, _)| *k == lower) {
                Some((_, c)) => out.push(Segment { text: m.as_str().to_string(), color: Some(c.clone()) }),
                None => push_plain(&mut out, m.as_str()),
            }
            last = m.end();
        }
        push_plain(&mut out, &text[last..]);
        out
    }
}

/// `load` for `Watched`: the keywords file as a ready matcher.
pub fn load_keyword_matcher(path: &Path) -> KeywordMatcher {
    KeywordMatcher::new(&load_keywords(path))
}

/// One rolled value as shown text, and whether it was read in game (else it is the range).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RolledValue {
    pub text: String,
    pub read: bool,
}

/// The values that roll (ranges in the data) as shown text, and whether each was read in game.
///
/// Fixed numbers ("lasting 3 seconds", "100% chance") are left out; they say nothing about the roll.
pub fn rolled_values(data: &[String], display: &[String]) -> Vec<RolledValue> {
    let mut out = Vec::new();
    for (d_line, s_line) in data.iter().zip(display) {
        for (d, s) in SLOT_RE.captures_iter(d_line).zip(SLOT_RE.captures_iter(s_line)) {
            let (d, s) = (d.unwrap(), s.unwrap());
            if !d[2].starts_with('(') {
                continue;
            }
            let shown = &s[2];
            let read = !shown.starts_with('(');
            let value = if read { shown } else { &shown[1..shown.len() - 1] };
            out.push(RolledValue { text: format!("{}{}{}", &d[1], value, &d[3]), read });
        }
    }
    out
}

/// A file's parsed content, reloaded when the file's modification time changes.
pub struct Watched<T> {
    path: PathBuf,
    load: fn(&Path) -> T,
    mtime: Option<SystemTime>,
    value: T,
}

impl<T> Watched<T> {
    pub fn new(path: PathBuf, load: fn(&Path) -> T) -> Watched<T> {
        let mtime = Self::mtime_of(&path);
        let value = load(&path);
        Watched { path, load, mtime, value }
    }

    fn mtime_of(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    /// Reloads if the file changed (or appeared or vanished); true if it did.
    pub fn refresh(&mut self) -> bool {
        let mtime = Self::mtime_of(&self.path);
        if mtime == self.mtime {
            return false;
        }
        self.mtime = mtime;
        self.value = (self.load)(&self.path);
        true
    }

    pub fn get(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn rarity() -> Vec<String> {
        s(&["27% increased Rarity of Items found in this Area"])
    }

    fn sap() -> Vec<String> {
        s(&["32% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"])
    }

    #[test]
    fn alias_takes_the_read_values_in_order() {
        assert_eq!(render_alias("Rarity +#%", &rarity()), "Rarity +27%");
        assert_eq!(render_alias("Sap #%", &sap()), "Sap 32%");
    }

    #[test]
    fn numbered_placeholders_pick_a_value() {
        let lines = s(&["Final Boss drops 3 additional Orbs of Fusing", "lasting 100 seconds"]);
        assert_eq!(render_alias("Fusing ×#1 (#2 s)", &lines), "Fusing ×3 (100 s)");
    }

    #[test]
    fn unread_values_show_the_range_and_missing_ones_stay() {
        let unread = s(&["(15-30)% increased Rarity of Items found in this Area"]);
        assert_eq!(render_alias("Rarity +#%", &unread), "Rarity +(15-30)%");
        assert_eq!(render_alias("Sap", &sap()), "Sap");
        assert_eq!(render_alias("# and #", &rarity()), "27 and #");
    }

    #[test]
    fn color_keywords_regex_is_cached() {
        let colors = vec![("Fire".to_string(), "#f00".to_string())];
        let before = REGEX_BUILDS.with(|c| c.get());
        let m = KeywordMatcher::new(&colors);
        for _ in 0..3 {
            assert_eq!(m.color("a fire b").len(), 3);
        }
        assert_eq!(REGEX_BUILDS.with(|c| c.get()) - before, 1);
        // No keywords: no regex at all.
        let none = KeywordMatcher::new(&[]);
        assert_eq!(none.color("fire"), vec![Segment { text: "fire".into(), color: None }]);
        assert_eq!(REGEX_BUILDS.with(|c| c.get()) - before, 1);
    }

    #[test]
    fn keywords_are_colored_as_whole_words_in_any_case() {
        let colors = vec![("Fire".to_string(), "#f00".to_string()), ("Cold".to_string(), "#00f".to_string())];
        let out = KeywordMatcher::new(&colors).color("Phys as Extra fire & Cold <x> Firestorm");
        let colored: Vec<_> = out.iter().filter(|s| s.color.is_some()).collect();
        assert_eq!(colored.len(), 2);
        assert_eq!(colored[0], &Segment { text: "fire".into(), color: Some("#f00".into()) });
        assert_eq!(colored[1], &Segment { text: "Cold".into(), color: Some("#00f".into()) });
        let plain: String = out.iter().filter(|s| s.color.is_none()).map(|s| s.text.as_str()).collect();
        assert!(plain.contains("Firestorm") && plain.contains("<x>") && plain.contains('&'));
        assert_eq!(out.iter().map(|s| s.text.as_str()).collect::<String>(), "Phys as Extra fire & Cold <x> Firestorm");
        // Adjacent plain text is merged: plain, fire, plain, Cold, plain.
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn rolled_values_skip_fixed_numbers_and_flag_unread_ranges() {
        let rv = |text: &str, read: bool| RolledValue { text: text.into(), read };
        let data = s(&["(25-35)% chance to be Sapped when Hit", "All Damage taken from Hits can Sap you"]);
        let read = s(&["32% chance to be Sapped when Hit", &data[1]]);
        assert_eq!(rolled_values(&data, &read), vec![rv("32%", true)]);
        assert_eq!(rolled_values(&data, &data), vec![rv("25-35%", false)]);
        let shocked = s(&["Create Shocked Ground on Death, lasting 3 seconds"]);
        assert_eq!(rolled_values(&shocked, &shocked), vec![]);
        let crit = s(&["-(40-20)% to Critical Strike Multiplier per Power Charge"]);
        let crit_read = s(&["-27% to Critical Strike Multiplier per Power Charge"]);
        assert_eq!(rolled_values(&crit, &crit_read), vec![rv("-27%", true)]);
    }

    #[test]
    fn broken_label_files_give_empty_maps() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        let broken = dir.path().join("broken.json");
        let other = dir.path().join("other.json");
        std::fs::write(&broken, "{ not json").unwrap();
        std::fs::write(&other, r#"{"about": "x"}"#).unwrap();
        for p in [&missing, &broken, &other] {
            assert!(load_labels(p).is_empty(), "{}", p.display());
            assert!(load_keywords(p).is_empty(), "{}", p.display());
        }
    }

    #[test]
    fn loads_trimmed_labels_and_longest_keywords_first() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.json");
        std::fs::write(
            &p,
            r##"{"labels": {"a": {"alias": " Qty ", "icon": " i "}, "b": {}},
                "keywords": {"Fire": "#f00", "Physical": "#ccc", "Ice": "#00f"}}"##,
        )
        .unwrap();
        let labels = load_labels(&p);
        assert_eq!(labels["a"], Label { alias: "Qty".into(), icon: "i".into() });
        assert_eq!(labels["b"], Label::default());
        let kw = load_keywords(&p);
        assert_eq!(kw[0].0, "Physical");
        assert_eq!(kw.len(), 3);
    }

    #[test]
    fn watched_reloads_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("k.json");
        let write = |body: &str, age: u64| {
            std::fs::write(&p, body).unwrap();
            let t = SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        };
        write(r##"{"keywords": {"Fire": "#f00"}}"##, 60);
        let mut w = Watched::new(p.clone(), load_keywords);
        assert_eq!(w.get().len(), 1);
        assert!(!w.refresh());
        write(r##"{"keywords": {"Fire": "#f00", "Cold": "#00f"}}"##, 30);
        assert!(w.refresh());
        assert_eq!(w.get().len(), 2);
        assert!(!w.refresh());
        std::fs::remove_file(&p).unwrap();
        assert!(w.refresh());
        assert!(w.get().is_empty());
    }
}
