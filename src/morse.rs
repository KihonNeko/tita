//! Table Morse et calcul des durées (vitesse caractère + espacement Farnsworth).

use std::time::Duration;

/// (caractère, motif) — `.` = point (ti), `-` = trait (ta).
pub const TABLE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."),
    ('F', "..-."), ('G', "--."), ('H', "...."), ('I', ".."), ('J', ".---"),
    ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"),
    ('P', ".--."), ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"),
    ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"), ('Y', "-.--"),
    ('Z', "--.."),
    ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
    ('.', ".-.-.-"), (',', "--..--"), ('?', "..--.."), ('/', "-..-."), ('=', "-...-"),
    ('+', ".-.-."), ('-', "-....-"), ('\'', ".----."), ('!', "-.-.--"), ('@', ".--.-."),
];

/// Motif d'un caractère (insensible à la casse), `None` si inconnu.
pub fn pattern(c: char) -> Option<&'static str> {
    let c = c.to_ascii_uppercase();
    TABLE.iter().find(|(k, _)| *k == c).map(|(_, p)| *p)
}

/// Décodage inverse : motif -> caractère.
pub fn decode(pat: &str) -> Option<char> {
    TABLE.iter().find(|(_, p)| *p == pat).map(|(c, _)| *c)
}

/// Motif lisible pour l'affichage : `.-` -> `· −`.
pub fn pretty(c: char) -> String {
    pattern(c)
        .map(|p| {
            p.chars()
                .map(|s| if s == '.' { "·" } else { "−" })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// Vitesse : `char_wpm` = vitesse des caractères, `eff_wpm` = vitesse effective
/// (Farnsworth : espaces allongés si `eff_wpm < char_wpm`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub char_wpm: f32,
    pub eff_wpm: f32,
}

impl Timing {
    pub fn new(char_wpm: f32, eff_wpm: f32) -> Self {
        let char_wpm = char_wpm.clamp(5.0, 60.0);
        Timing { char_wpm, eff_wpm: eff_wpm.clamp(2.0, char_wpm) }
    }
    /// Durée d'un point, en secondes.
    pub fn dit(&self) -> f32 {
        1.2 / self.char_wpm
    }
    /// Silence entre deux caractères, en secondes.
    pub fn char_gap(&self) -> f32 {
        if self.eff_wpm >= self.char_wpm {
            return 3.0 * self.dit();
        }
        3.0 * self.farnsworth_delay() / 19.0
    }
    /// Silence entre deux mots, en secondes.
    pub fn word_gap(&self) -> f32 {
        if self.eff_wpm >= self.char_wpm {
            return 7.0 * self.dit();
        }
        7.0 * self.farnsworth_delay() / 19.0
    }
    // Formule ARRL : délai total ajouté par mot « PARIS ».
    fn farnsworth_delay(&self) -> f32 {
        (60.0 * self.char_wpm - 37.2 * self.eff_wpm) / (self.eff_wpm * self.char_wpm)
    }
}

/// Segment sonore : son (`on`) ou silence, durée en secondes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub on: bool,
    pub secs: f32,
}

/// Segments d'un seul caractère, SANS silence final.
pub fn char_segments(c: char, t: Timing) -> Vec<Seg> {
    let mut out = Vec::new();
    if let Some(p) = pattern(c) {
        for (i, s) in p.chars().enumerate() {
            if i > 0 {
                out.push(Seg { on: false, secs: t.dit() });
            }
            out.push(Seg { on: true, secs: if s == '.' { t.dit() } else { 3.0 * t.dit() } });
        }
    }
    out
}

/// Segments d'un texte : silences entre caractères et entre mots (espaces),
/// SANS silence final. Les caractères inconnus sont ignorés.
pub fn text_segments(text: &str, t: Timing) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    let mut pending_gap: Option<f32> = None;
    for c in text.chars() {
        if c.is_whitespace() {
            if !out.is_empty() {
                pending_gap = Some(t.word_gap());
            }
            continue;
        }
        let segs = char_segments(c, t);
        if segs.is_empty() {
            continue;
        }
        if let Some(g) = pending_gap.take() {
            out.push(Seg { on: false, secs: g });
        }
        out.extend(segs);
        pending_gap = Some(t.char_gap());
    }
    out
}

#[allow(dead_code)]
pub fn segments_duration(segs: &[Seg]) -> Duration {
    Duration::from_secs_f32(segs.iter().map(|s| s.secs).sum())
}

/// Durée d'un caractère seul (sans silence final).
#[allow(dead_code)]
pub fn char_duration(c: char, t: Timing) -> Duration {
    segments_duration(&char_segments(c, t))
}

/// Durée d'un texte (sans silence final).
#[allow(dead_code)]
pub fn text_duration(text: &str, t: Timing) -> Duration {
    segments_duration(&text_segments(text, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_is_50_units() {
        // « PARIS » + espace mot = 50 points à vitesse standard.
        let t = Timing::new(20.0, 20.0);
        let d = text_duration("PARIS", t).as_secs_f32() + t.word_gap();
        assert!((d - 50.0 * t.dit()).abs() < 1e-4);
    }

    #[test]
    fn farnsworth_matches_effective_speed() {
        // À 20/10, un mot PARIS (avec son espace mot) doit durer 60/10 = 6 s.
        let t = Timing::new(20.0, 10.0);
        let d = text_duration("PARIS", t).as_secs_f32() + t.word_gap();
        assert!((d - 6.0).abs() < 1e-3, "{d}");
    }

    #[test]
    fn table_roundtrip_and_unique() {
        for (c, p) in TABLE {
            assert_eq!(decode(p), Some(*c));
            assert_eq!(pattern(c.to_ascii_lowercase()), Some(*p));
        }
    }
}
