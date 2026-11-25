//! Langue de l'interface : français ou anglais.
//!
//! Usage dans les écrans :
//! - `t("Retour", "Back")` pour un texte fixe ;
//! - `tr!("{n} caractères", "{n} characters")` pour un texte formaté (renvoie
//!   une `String`, mêmes règles que `format!`).
//!
//! La langue est propre au thread courant (l'interface n'en utilise qu'un),
//! ce qui permet aux tests de la changer sans se gêner entre eux.

use crate::config::Lang;
use std::cell::Cell;

thread_local! {
    static EN: Cell<bool> = const { Cell::new(false) };
}

/// Choisit la langue de l'interface pour le thread courant.
pub fn set(lang: Lang) {
    EN.with(|e| e.set(lang == Lang::En));
}

pub fn is_en() -> bool {
    EN.with(|e| e.get())
}

/// Texte fixe dans la langue courante.
pub fn t(fr: &'static str, en: &'static str) -> &'static str {
    if is_en() { en } else { fr }
}

/// Texte formaté dans la langue courante : `tr!("fr {x}", "en {x}")`.
#[macro_export]
macro_rules! tr {
    ($fr:literal, $en:literal $(, $args:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $args)*)
        } else {
            format!($fr $(, $args)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_language_per_thread() {
        assert_eq!(t("Retour", "Back"), "Retour");
        let n = 3;
        set(Lang::En);
        assert_eq!(t("Retour", "Back"), "Back");
        assert_eq!(tr!("{n} jours", "{n} days"), "3 days");
        assert_eq!(tr!("{} jours", "{} days", n + 1), "4 days");
        set(Lang::Fr);
        assert_eq!(tr!("{n} jours", "{n} days"), "3 jours");
    }
}
