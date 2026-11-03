//! Filling the numbers read in game into a mod's clean data text.

use std::collections::HashMap;
use std::sync::LazyLock;

use fancy_regex::Regex;

pub(crate) static NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?").unwrap());
/// "(15-30)" or "100".
static VALUE_SLOT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(-?[\d.]+--?[\d.]+\)|\d+(?:\.\d+)?").unwrap());
static COMMA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?<=\d),(?=\d)").unwrap());
static NON_WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Z#]+").unwrap());

/// All non-overlapping matches of `re` in `text`, as strings.
pub(crate) fn find_all<'t>(re: &Regex, text: &'t str) -> Vec<&'t str> {
    re.find_iter(text).map(|m| m.unwrap().as_str()).collect()
}

/// Python `re.split` for a pattern without groups.
fn split<'t>(re: &Regex, text: &'t str) -> Vec<&'t str> {
    let mut out = Vec::new();
    let mut last = 0;
    for m in re.find_iter(text) {
        let m = m.unwrap();
        out.push(&text[last..m.start()]);
        last = m.end();
    }
    out.push(&text[last..]);
    out
}

/// Words of `[A-Z#]+` after replacing value slots with " # " and uppercasing.
fn tokenize(text: &str) -> Vec<String> {
    let t = VALUE_SLOT_RE.replace_all(text, " # ").to_uppercase();
    split(&NON_WORD_RE, &t)
        .into_iter()
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Longest common block of `a[alo..ahi]` and `b[blo..bhi]` (difflib `find_longest_match`, no junk).
fn find_longest_match(
    a: &[String],
    b2j: &HashMap<&str, Vec<usize>>,
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
    let mut j2len: HashMap<usize, usize> = HashMap::new();
    for (i, ai) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut new_j2len = HashMap::new();
        if let Some(js) = b2j.get(ai.as_str()) {
            for &j in js {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j
                    .checked_sub(1)
                    .and_then(|p| j2len.get(&p))
                    .copied()
                    .unwrap_or(0)
                    + 1;
                new_j2len.insert(j, k);
                if k > bestsize {
                    (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                }
            }
        }
        j2len = new_j2len;
    }
    // Without junk the found block cannot be extended, so difflib's extension loops are no-ops.
    (besti, bestj, bestsize)
}

/// Port of `difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks()`:
/// `(i, j, n)` triples with `a[i..i+n] == b[j..j+n]`, adjacent blocks merged,
/// ending with the sentinel `(a.len(), b.len(), 0)`.
pub fn matching_blocks(a: &[String], b: &[String]) -> Vec<(usize, usize, usize)> {
    let mut b2j: HashMap<&str, Vec<usize>> = HashMap::new();
    for (j, w) in b.iter().enumerate() {
        b2j.entry(w.as_str()).or_default().push(j);
    }
    let (la, lb) = (a.len(), b.len());
    let mut queue = vec![(0, la, 0, lb)];
    let mut blocks = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = find_longest_match(a, &b2j, alo, ahi, blo, bhi);
        if k > 0 {
            blocks.push((i, j, k));
            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }
    blocks.sort();
    let (mut i1, mut j1, mut k1) = (0, 0, 0);
    let mut out = Vec::new();
    for (i2, j2, k2) in blocks {
        if i1 + k1 == i2 && j1 + k1 == j2 {
            k1 += k2;
        } else {
            if k1 > 0 {
                out.push((i1, j1, k1));
            }
            (i1, j1, k1) = (i2, j2, k2);
        }
    }
    if k1 > 0 {
        out.push((i1, j1, k1));
    }
    out.push((la, lb, 0));
    out
}

/// Put the numbers read in game into the data text's value slots.
///
/// Slots are matched to OCR numbers by the words around them, because the two texts do
/// not always have the same numbers ("100% chance to create Shocked Ground ... 3 seconds"
/// in game vs. "Create Shocked Ground ... 3 seconds" in the data).
pub fn fill_values(data_line: &str, ocr_line: &str) -> String {
    let ocr_line = COMMA_RE.replace_all(ocr_line, "."); // 4,3% -> 4.3%
    let data_tokens = split(&VALUE_SLOT_RE, data_line); // text pieces between the slots
    let slots = find_all(&VALUE_SLOT_RE, data_line);
    let data_words = tokenize(data_line);
    let ocr_words = tokenize(&ocr_line);
    let mut ocr_values = find_all(&NUMBER_RE, &ocr_line).into_iter();
    let ocr_slot_values: Vec<Option<&str>> = ocr_words
        .iter()
        .map(|w| if w == "#" { ocr_values.next() } else { None })
        .collect();

    let slot_at: Vec<usize> = data_words
        .iter()
        .enumerate()
        .filter(|(_, w)| *w == "#")
        .map(|(k, _)| k)
        .collect();
    let mut filled: HashMap<usize, Option<&str>> = HashMap::new();
    for (bi, bj, size) in matching_blocks(&data_words, &ocr_words) {
        for offset in 0..size {
            let (d, o) = (bi + offset, bj + offset);
            if data_words[d] == "#"
                && let Some(n) = slot_at.iter().position(|&k| k == d)
            {
                filled.insert(n, ocr_slot_values[o]);
            }
        }
    }
    let mut out = data_tokens[0].to_string();
    for (n, slot) in slots.iter().enumerate() {
        out += filled.get(&n).copied().flatten().unwrap_or(slot);
        out += data_tokens[n + 1];
    }
    out
}
