use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::data::Target;

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

static RANGE_RE: LazyLock<Regex> = LazyLock::new(|| re(r"\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)"));
static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| re(r"(?<![\w.#])\d+(?:\.\d+)?"));
static O_RE: LazyLock<Regex> = LazyLock::new(|| re(r"(?<=\d)[O](?=[\d%])|(?<=[\d.])O\b"));
static SPACE_PCT_RE: LazyLock<Regex> = LazyLock::new(|| re(r"(?<=\d) +(?=%)"));
static COMMA_RE: LazyLock<Regex> = LazyLock::new(|| re(r"(?<=\d),(?=\d)"));
static WS_RE: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

/// Replace every value or value range with '#'.
pub fn to_template(line: &str) -> String {
    let s = RANGE_RE.replace_all(line, "#");
    NUMBER_RE.replace_all(&s, "#").into_owned()
}

/// Uppercase template of a line, robust to typical OCR slips.
pub fn normalize(text: &str) -> String {
    let t = text.to_uppercase().replace(['—', '–'], "-");
    let t = O_RE.replace_all(&t, "0");
    let t = SPACE_PCT_RE.replace_all(&t, "");
    let t = COMMA_RE.replace_all(&t, ".");
    let t = to_template(&t);
    WS_RE.replace_all(&t, " ").trim().to_string()
}

fn indel_ratio(a: &[char], b: &[char]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 100.0;
    }
    // LCS length by dynamic programming.
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for &ca in a {
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb { prev[j] + 1 } else { cur[j].max(prev[j + 1]) };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    100.0 * (2 * prev[b.len()]) as f64 / total as f64
}

/// Normalized Indel similarity, 0-100 (rapidfuzz `fuzz.ratio`).
pub fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    indel_ratio(&a, &b)
}

/// Best ratio of the shorter string against same-length windows of the longer one,
/// plus the shorter prefixes and suffixes at both ends (rapidfuzz `fuzz.partial_ratio`).
pub fn partial_ratio(short: &str, long: &str) -> f64 {
    let mut s: Vec<char> = short.chars().collect();
    let mut l: Vec<char> = long.chars().collect();
    if s.len() > l.len() {
        std::mem::swap(&mut s, &mut l);
    }
    if s.is_empty() {
        return if l.is_empty() { 100.0 } else { 0.0 };
    }
    let n = s.len();
    let mut best = 0.0f64;
    for k in 1..n {
        best = best.max(indel_ratio(&s, &l[..k]));
        best = best.max(indel_ratio(&s, &l[l.len() - k..]));
    }
    for i in 0..=(l.len() - n) {
        best = best.max(indel_ratio(&s, &l[i..i + n]));
    }
    best
}

/// Which side a "<X> GAINS:" header line addresses, if it is one.
pub fn parse_header(text: &str) -> Option<Target> {
    let up = text.to_uppercase();
    let up = up.trim();
    if up.chars().count() > 28 || partial_ratio("GAIN", up) < 75.0 {
        return None;
    }
    let mut best: Option<(Target, f64)> = None;
    for (key, target) in [("PLAYER", Target::Player), ("MINION", Target::Minions), ("BOSS", Target::Boss)] {
        let s = partial_ratio(key, up);
        if best.is_none_or(|(_, b)| s > b) {
            best = Some((target, s));
        }
    }
    best.filter(|&(_, s)| s >= 80.0).map(|(t, _)| t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fixes_ocr_slips() {
        assert_eq!(normalize("2,5% chance to drop an additional Jeweller's Orb"), normalize("2.5% CHANCE TO DROP AN ADDITIONAL JEWELLER'S ORB"));
        assert_eq!(normalize("1O% increased Quantity"), "#% INCREASED QUANTITY");
        assert_eq!(normalize("33 % chance"), "#% CHANCE");
        assert_eq!(normalize("Final Boss drops (2-4) additional Chaos Orbs"), "FINAL BOSS DROPS # ADDITIONAL CHAOS ORBS");
        assert_eq!(normalize("-(60-40)% to Cold Resistance"), "-#% TO COLD RESISTANCE");
        assert_eq!(normalize("3.2% x1.5 ab#2"), "#% X1.5 AB#2");
    }

    #[test]
    fn ratio_matches_rapidfuzz() {
        assert!((ratio("THIS IS A TEST", "THIS IS A TEST!") - 96.5517).abs() < 1e-3);
        assert!((ratio("#% INCREASED QUANTITY", "#% INCREASED QUANTlTY") - 95.2381).abs() < 1e-3);
    }

    #[test]
    fn partial_ratio_matches_rapidfuzz() {
        for (a, b, want) in [
            ("GAIN", "MINIONS GAN:", 75.0),
            ("PLAYER", "PLAVER GAINS:", 83.3333),
            ("BOSS", "MAP B0SS GAINS:", 75.0),
            ("GAIN", "NEARBY ENEMIES GAIN 100%", 100.0),
            ("GAIN", "AIN:", 85.7143),
            ("BOSS", "SS GAINS", 66.6667),
        ] {
            assert!((partial_ratio(a, b) - want).abs() < 1e-3, "{a} / {b}");
        }
    }

    #[test]
    fn parse_header_cases() {
        assert_eq!(parse_header("PLAYER GAINS:"), Some(Target::Player));
        assert_eq!(parse_header("ELDRITCH MINIONS GAIN:"), Some(Target::Minions));
        assert_eq!(parse_header("MINIONS GAIN:"), Some(Target::Minions));
        assert_eq!(parse_header("MAP Boss GAINS:"), Some(Target::Boss));
        assert_eq!(parse_header("NEARBY ENEMIES GAIN 100% OF THEIR PHYSICAL DAMAGE AS EXTRA COLD DAMAGE"), None);
        assert_eq!(parse_header("GAIN 82% OF PHYSICAL DAMAGE AS EXTRA FIRE DAMAGE"), None);
    }
}
