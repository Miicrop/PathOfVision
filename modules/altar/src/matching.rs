//! Match OCR lines of one altar box against the mod cards.

use std::cmp::Ordering;

use crate::data::{Card, Target};
use crate::fill::{NUMBER_RE, fill_values, find_all};
use rapidfuzz::distance::indel;

use crate::text::normalize;

pub const MIN_LINE_SCORE: f64 = 80.0;
/// A line may also count for templates scoring at most this much below its best.
pub const LINE_MARGIN: f64 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// Index into the cards slice.
    pub card: usize,
    /// Indices into the OCR lines, in template order.
    pub lines: Vec<usize>,
    /// Rounded to 0.1.
    pub score: f64,
    pub values: Vec<f64>,
    /// Informational: atlas passives push values past the base range.
    pub in_range: bool,
    /// Some lines of a multi-line mod were not readable.
    pub partial: bool,
    pub variant: usize,
    /// OCR line per template line, None if hidden.
    pub assign: Vec<Option<usize>>,
}

impl Match {
    /// The mod's clean text with the rolled values filled in where they were read.
    pub fn display(&self, cards: &[Card], ocr_lines: &[String]) -> Vec<String> {
        cards[self.card].lines[self.variant]
            .iter()
            .zip(&self.assign)
            .map(|(text, i)| match i {
                Some(i) => fill_values(text, &ocr_lines[*i]),
                None => text.clone(),
            })
            .collect()
    }
}

/// One way a card's variant can explain some OCR lines.
struct Candidate {
    card: usize,
    variant: usize,
    assign: Vec<Option<usize>>,
    score: f64,
}

fn values(line: &str) -> Vec<f64> {
    // Python float() also accepts non-ASCII digits; those become NaN here (never in range).
    find_all(&NUMBER_RE, line)
        .into_iter()
        .map(|n| n.parse().unwrap_or(f64::NAN))
        .collect()
}

/// Python `round(x, 1)`: exact decimal value, ties to even.
fn round1(x: f64) -> f64 {
    format!("{x:.1}").parse().unwrap()
}

/// Every way a card's variant can explain some OCR lines, in card / variant / product order.
fn candidates(norm: &[String], cards: &[Card], pool: &[usize]) -> Vec<Candidate> {
    // scores[p][vi][ti][i]
    let mut best_for_line = vec![0.0f64; norm.len()];
    let scores: Vec<Vec<Vec<Vec<Option<f64>>>>> = pool
        .iter()
        .map(|&ci| {
            cards[ci]
                .templates
                .iter()
                .map(|tpls| {
                    tpls.iter()
                        .map(|tpl| {
                            // Same as text::ratio, with the template prepared once.
                            let cmp = indel::BatchComparator::new(tpl.chars());
                            norm.iter()
                                .enumerate()
                                .map(|(i, n)| {
                                    let s = cmp.normalized_similarity(n.chars()) * 100.0;
                                    if s >= MIN_LINE_SCORE {
                                        best_for_line[i] = best_for_line[i].max(s);
                                        Some(s)
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect()
        })
        .collect();

    let mut out = Vec::new();
    for (p, &ci) in pool.iter().enumerate() {
        for (vi, tpls) in cards[ci].templates.iter().enumerate() {
            // Per template line: [(ocr line index, score)].
            let options: Vec<Vec<(Option<usize>, f64)>> = (0..tpls.len())
                .map(|ti| {
                    // Near-identical lines ("+#% TO COLD/FIRE RESISTANCE") would explode the
                    // search, so a line only counts for templates close to its best match.
                    let opts: Vec<(Option<usize>, f64)> = (0..norm.len())
                        .filter_map(|i| {
                            scores[p][vi][ti][i]
                                .filter(|&s| s >= best_for_line[i] - LINE_MARGIN)
                                .map(|s| (Some(i), s))
                        })
                        .collect();
                    // A template line may stay unmatched (hidden behind HUD) only if nothing matches it.
                    if opts.is_empty() {
                        vec![(None, 0.0)]
                    } else {
                        opts
                    }
                })
                .collect();
            // Cartesian product, last position varying fastest (itertools.product).
            let mut pos = vec![0usize; options.len()];
            'product: loop {
                let combo: Vec<(Option<usize>, f64)> =
                    pos.iter().zip(&options).map(|(&k, o)| o[k]).collect();
                let idx: Vec<usize> = combo.iter().filter_map(|c| c.0).collect();
                let distinct = idx.iter().enumerate().all(|(n, i)| !idx[..n].contains(i));
                if !idx.is_empty() && distinct {
                    let score = combo.iter().fold(0.0, |acc, c| acc + c.1) / tpls.len() as f64; // missing lines count as 0
                    out.push(Candidate {
                        card: ci,
                        variant: vi,
                        assign: combo.iter().map(|c| c.0).collect(),
                        score,
                    });
                }
                let mut t = options.len();
                loop {
                    if t == 0 {
                        break 'product;
                    }
                    t -= 1;
                    pos[t] += 1;
                    if pos[t] < options[t].len() {
                        break;
                    }
                    pos[t] = 0;
                }
            }
        }
    }
    out
}

/// Search key: (lines covered, -partial count, score sum).
#[derive(Clone, Copy)]
struct Key(usize, i64, f64);

impl Key {
    fn greater(&self, other: &Key) -> bool {
        match self.0.cmp(&other.0).then(self.1.cmp(&other.1)) {
            Ordering::Equal => self.2 > other.2,
            o => o == Ordering::Greater,
        }
    }
}

struct Search<'a> {
    candidates: &'a [Candidate],
    n_lines: usize,
    best_key: Key,
    best_set: Vec<usize>,
}

impl Search<'_> {
    fn run(&mut self, start: usize, used: &mut Vec<bool>, n_used: usize, chosen: &mut Vec<usize>) {
        let partials = chosen
            .iter()
            .filter(|&&c| self.candidates[c].assign.contains(&None))
            .count();
        let sum = chosen
            .iter()
            .fold(0.0, |acc, &c| acc + self.candidates[c].score);
        let key = Key(n_used, -(partials as i64), sum);
        if key.greater(&self.best_key) {
            self.best_key = key;
            self.best_set = chosen.clone();
        }
        if n_used == self.n_lines {
            return;
        }
        for k in start..self.candidates.len() {
            let idx: Vec<usize> = self.candidates[k]
                .assign
                .iter()
                .flatten()
                .copied()
                .collect();
            if idx.iter().all(|&i| !used[i]) {
                // The same mod can roll twice on one option.
                for &i in &idx {
                    used[i] = true;
                }
                chosen.push(k);
                self.run(k + 1, used, n_used + idx.len(), chosen);
                chosen.pop();
                for &i in &idx {
                    used[i] = false;
                }
            }
        }
    }
}

/// Assign OCR lines to cards so that every line is used at most once.
///
/// Lines of a multi-line mod are not shown in data order and can be interleaved
/// with other mods, so this is an exact-cover search over the box's lines.
/// Prefers covering more lines, then fewer partially visible mods, then higher scores.
/// Without a target (header not readable) every target is tried.
/// Returns the matches (sorted by first line) and the indices of lines nothing explained.
pub fn match_box(
    lines: &[String],
    cards: &[Card],
    target: Option<Target>,
) -> (Vec<Match>, Vec<usize>) {
    let Some(target) = target else {
        type Scored = ((usize, f64), (Vec<Match>, Vec<usize>));
        let mut best: Option<Scored> = None;
        for t in [Target::Player, Target::Minions, Target::Boss] {
            let r = match_box(lines, cards, Some(t));
            let key = (
                lines.len() - r.1.len(),
                r.0.iter().fold(0.0, |acc, m| acc + m.score),
            );
            // First maximum wins, like Python's max().
            if best
                .as_ref()
                .is_none_or(|(b, _)| key.0 > b.0 || (key.0 == b.0 && key.1 > b.1))
            {
                best = Some((key, r));
            }
        }
        return best.unwrap().1;
    };

    let norm: Vec<String> = lines.iter().map(|l| normalize(l)).collect();
    let pool: Vec<usize> = (0..cards.len())
        .filter(|&ci| cards[ci].target == target)
        .collect();
    let mut cands = candidates(&norm, cards, &pool);
    // Stable sort by descending score.
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    let mut search = Search {
        candidates: &cands,
        n_lines: norm.len(),
        best_key: Key(0, 0, 0.0),
        best_set: Vec::new(),
    };
    search.run(0, &mut vec![false; norm.len()], 0, &mut Vec::new());

    let mut matches: Vec<Match> = search
        .best_set
        .iter()
        .map(|&k| {
            let cand = &cands[k];
            let card = &cards[cand.card];
            let present: Vec<usize> = cand.assign.iter().flatten().copied().collect();
            let values: Vec<f64> = present.iter().flat_map(|&i| values(&lines[i])).collect();
            // OCR values carry no sign, so compare magnitudes.
            let bounds: Vec<(f64, f64)> = card.ranges[cand.variant]
                .iter()
                .map(|&[lo, hi]| {
                    let (a, b) = (lo.abs(), hi.abs());
                    if a <= b { (a, b) } else { (b, a) }
                })
                .collect();
            let in_range = values.len() == bounds.len()
                && values
                    .iter()
                    .zip(&bounds)
                    .all(|(&v, &(lo, hi))| lo <= v && v <= hi);
            Match {
                card: cand.card,
                partial: present.len() < cand.assign.len(),
                lines: present,
                score: round1(cand.score),
                values,
                in_range,
                variant: cand.variant,
                assign: cand.assign.clone(),
            }
        })
        .collect();
    matches.sort_by_key(|m| *m.lines.iter().min().unwrap());
    let mut explained = vec![false; lines.len()];
    for m in &matches {
        for &i in &m.lines {
            explained[i] = true;
        }
    }
    let unexplained = (0..lines.len()).filter(|&i| !explained[i]).collect();
    (matches, unexplained)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round1_is_half_even_on_exact_decimal() {
        // Python: round(92.25, 1) == 92.2, round(0.35, 1) == 0.3, round(92.75, 1) == 92.8
        assert_eq!(round1(92.25), 92.2);
        assert_eq!(round1(0.35), 0.3);
        assert_eq!(round1(92.75), 92.8);
        assert_eq!(round1(97.66666666666667), 97.7);
    }
}
