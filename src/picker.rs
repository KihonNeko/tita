//! Tirage pondéré : les caractères faibles reviennent plus souvent.

use crate::stats::Stats;
use rand::Rng;

/// Tire un caractère parmi `chars`, pondéré par `Stats::weight`. Répéter
/// `avoid` (le dernier caractère tiré) reste possible mais moins probable :
/// l'interdire rendrait la suite devinable quand il y a peu de caractères.
pub fn pick(chars: &[char], stats: &Stats, avoid: Option<char>, rng: &mut impl Rng) -> char {
    assert!(!chars.is_empty());
    let weights: Vec<f32> = chars
        .iter()
        .map(|c| stats.weight(*c) * if Some(*c) == avoid { 0.5 } else { 1.0 })
        .collect();
    let total: f32 = weights.iter().sum();
    let mut x = rng.random::<f32>() * total;
    for (c, w) in chars.iter().zip(&weights) {
        if x < *w {
            return *c;
        }
        x -= w;
    }
    *chars.last().unwrap()
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
    fn picks_only_from_pool_and_can_repeat() {
        let stats = Stats::default();
        let mut rng = rand::rng();
        let chars = ['E', 'T'];
        let n = (0..400).filter(|_| pick(&chars, &stats, Some('E'), &mut rng) == 'E').count();
        // Répétition possible (≈ 1/3) mais minoritaire.
        assert!(n > 60 && n < 200, "{n}");
        let g = group(&chars, &stats, 5, &mut rng);
        assert!(g.len() == 5 && g.chars().all(|c| chars.contains(&c)));
    }
}
