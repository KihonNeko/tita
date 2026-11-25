//! Contenus d'entraînement : mots, indicatifs, abréviations, échanges radio.
//! À IMPLÉMENTER (les signatures ci-dessous sont fixes).

use crate::config::Lang;
use rand::Rng;

/// Mots fréquents de la langue, en MAJUSCULES sans accents, composés
/// uniquement de caractères présents dans `allowed`. Peut être vide.
pub fn words(_lang: Lang, _allowed: &[char]) -> Vec<&'static str> {
    Vec::new()
}

/// Abréviations et codes Q radioamateur : (code, signification en français).
pub fn abbreviations() -> &'static [(&'static str, &'static str)] {
    &[]
}

/// Indicatif radioamateur aléatoire plausible (ex. "F4ABC", "DL1XY").
pub fn callsign(_rng: &mut impl Rng) -> String {
    "F4ABC".into()
}

/// Échange radio (QSO) simulé, court, en plusieurs transmissions.
pub fn qso(_rng: &mut impl Rng) -> Vec<String> {
    Vec::new()
}
