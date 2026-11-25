//! Tirage pondéré : les caractères faibles reviennent plus souvent.

use crate::stats::Stats;
use rand::Rng;

/// Tire un caractère parmi `chars`, pondéré par `Stats::weight`, en évitant si
/// possible de répéter `avoid` (le dernier caractère tiré).
pub fn pick(chars: &[char], stats: &Stats, avoid: Option<char>, rng: &mut impl Rng) -> char {
    assert!(!chars.is_empty());
    let pool: Vec<char> = if chars.len() > 1 {
        chars.iter().copied().filter(|c| Some(*c) != avoid).collect()
    } else {
        chars.to_vec()
    };
    let weights: Vec<f32> = pool.iter().map(|c| stats.weight(*c)).collect();
    let total: f32 = weights.iter().sum();
    let mut x = rng.random::<f32>() * total;
    for (c, w) in pool.iter().zip(&weights) {
        if x < *w {
            return *c;
        }
        x -= w;
    }
    *pool.last().unwrap()
}

/// Un groupe de `len` caractères tirés avec `pick` (ex. groupes de 5 Koch).
pub fn group(chars: &[char], stats: &Stats, len: usize, rng: &mut impl Rng) -> String {
    let mut out = String::new();
    let mut last = None;
    for _ in 0..len {
        let c = pick(chars, stats, last, rng);
        out.push(c);
        last = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_only_from_pool_and_avoids_repeat() {
        let stats = Stats::default();
        let mut rng = rand::rng();
        let chars = ['E', 'T'];
        for _ in 0..50 {
            assert_eq!(pick(&chars, &stats, Some('E'), &mut rng), 'T');
        }
        assert_eq!(group(&chars, &stats, 5, &mut rng).len(), 5);
    }
}
