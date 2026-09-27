//! Near-miss name suggestions for unresolved-identifier diagnostics
//! (spec §13: "unresolved identifier with scope and near-miss
//! suggestions"). Plain Levenshtein distance; no new dependency, and
//! deterministic given a deterministic candidate order (spec §14).

/// Levenshtein edit distance between two strings, by byte.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<u8> = a.bytes().collect();
    let b: Vec<u8> = b.bytes().collect();

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for (i, &ac) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &bc) in b.iter().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

/// The candidate closest to `name` by edit distance, if any candidate is
/// close enough to be worth suggesting. Ties keep the first candidate in
/// iteration order, so callers should pass candidates in declaration order
/// for deterministic output.
pub fn nearest_match<'a, I>(name: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let threshold = (name.chars().count() / 3).max(2);

    let mut best: Option<(usize, &'a str)> = None;
    for candidate in candidates {
        let distance = edit_distance(name, candidate);
        if distance == 0 || distance > threshold {
            continue;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_a_close_typo() {
        assert_eq!(
            nearest_match("capHeigt", ["baseline", "capHeight", "xHeight"]),
            Some("capHeight")
        );
    }

    #[test]
    fn suggests_nothing_when_too_far() {
        assert_eq!(nearest_match("zzz", ["baseline", "capHeight"]), None);
    }

    #[test]
    fn never_suggests_an_exact_match() {
        assert_eq!(nearest_match("baseline", ["baseline"]), None);
    }
}
