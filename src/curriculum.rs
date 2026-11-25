//! Parcours d'apprentissage : leçons par groupes de lettres, ou ordre Koch.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    /// Leçons par groupes (E T, I M S O, ...).
    Lessons,
    /// Ordre Koch classique (K M U R E S N A P T L ...), un caractère à la fois.
    Koch,
}

pub struct Lesson {
    title: &'static str,
    title_en: &'static str,
    pub chars: &'static str,
}

impl Lesson {
    /// Titre dans la langue de l'interface.
    pub fn title(&self) -> &'static str {
        crate::i18n::t(self.title, self.title_en)
    }
}

pub const LESSONS: &[Lesson] = &[
    Lesson { title: "Lettres fondamentales", title_en: "Core letters", chars: "ET" },
    Lesson { title: "Lettres courantes", title_en: "Common letters", chars: "IMSO" },
    Lesson { title: "Lettres étendues", title_en: "More letters", chars: "ANRW" },
    Lesson { title: "Lettres avancées", title_en: "Advanced letters", chars: "DKGU" },
    Lesson { title: "Lettres restantes I", title_en: "Remaining letters I", chars: "LPJB" },
    Lesson { title: "Lettres restantes II", title_en: "Remaining letters II", chars: "XCYZ" },
    Lesson { title: "Fin de l'alphabet", title_en: "End of the alphabet", chars: "QFHV" },
    Lesson { title: "Chiffres 1-5", title_en: "Digits 1-5", chars: "12345" },
    Lesson { title: "Chiffres 6-0", title_en: "Digits 6-0", chars: "67890" },
    Lesson { title: "Ponctuation", title_en: "Punctuation", chars: ".?,/=" },
    Lesson { title: "Ponctuation II", title_en: "Punctuation II", chars: "+-'!@" },
    Lesson { title: "Parenthèses et deux-points", title_en: "Brackets and colons", chars: "():;" },
    Lesson { title: "Symboles rares", title_en: "Rare symbols", chars: "\"_$" },
    Lesson { title: "Lettres accentuées", title_en: "Accented letters", chars: "ÉÀÈÇ" },
    Lesson { title: "Signaux spéciaux", title_en: "Special signals", chars: "&<>#*" },
];

pub const KOCH_ORDER: &str =
    "KMURESNAPTLWI.JZ=FOY,VG5/Q92H8B?47C1D60X3+-'!@():;\"_$ÉÀÈÇ&<>#*";

impl Track {
    pub fn label(self) -> &'static str {
        match self {
            Track::Lessons => crate::i18n::t("Leçons par lettres", "Lessons by letters"),
            Track::Koch => "Koch",
        }
    }

    /// Tous les caractères du parcours, dans l'ordre d'apprentissage.
    pub fn order(self) -> Vec<char> {
        match self {
            Track::Lessons => LESSONS.iter().flat_map(|l| l.chars.chars()).collect(),
            Track::Koch => KOCH_ORDER.chars().collect(),
        }
    }

    /// Nombre de caractères débloqués au tout début.
    pub fn initial(self) -> usize {
        2
    }

    /// Nouveau total débloqué après un déblocage (un groupe entier pour les
    /// leçons, un seul caractère pour Koch). Renvoie `unlocked` si tout est fait.
    pub fn next_unlock(self, unlocked: usize) -> usize {
        let total = self.order().len();
        match self {
            Track::Koch => (unlocked + 1).min(total),
            Track::Lessons => {
                let mut acc = 0;
                for l in LESSONS {
                    acc += l.chars.chars().count();
                    if acc > unlocked {
                        return acc;
                    }
                }
                total
            }
        }
    }

    /// Les `unlocked` premiers caractères du parcours.
    pub fn chars(self, unlocked: usize) -> Vec<char> {
        self.order().into_iter().take(unlocked.max(self.initial())).collect()
    }
}

/// Index (base 0) de la leçon contenant le `n`-ième caractère débloqué.
pub fn lesson_index(unlocked: usize) -> usize {
    let mut acc = 0;
    for (i, l) in LESSONS.iter().enumerate() {
        acc += l.chars.chars().count();
        if acc >= unlocked {
            return i;
        }
    }
    LESSONS.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morse;

    #[test]
    fn all_chars_exist_and_are_unique() {
        for t in [Track::Lessons, Track::Koch] {
            let o = t.order();
            for c in &o {
                assert!(morse::pattern(*c).is_some(), "{c}");
            }
            let mut d = o.clone();
            d.sort();
            d.dedup();
            assert_eq!(d.len(), o.len());
            // Chaque parcours couvre toute la table.
            assert_eq!(o.len(), morse::TABLE.len());
        }
    }

    #[test]
    fn unlock_steps() {
        assert_eq!(Track::Lessons.next_unlock(2), 6);
        assert_eq!(Track::Lessons.next_unlock(6), 10);
        assert_eq!(Track::Koch.next_unlock(2), 3);
        let n = Track::Koch.order().len();
        assert_eq!(Track::Koch.next_unlock(n), n);
    }
}
