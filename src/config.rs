//! Réglages utilisateur, sauvegardés en TOML dans ~/.config/tita/config.toml.

use crate::curriculum::Track;
use crate::morse::Timing;
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Fr,
    En,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Parcours actif : leçons par lettres ou ordre Koch.
    pub track: Track,
    /// Vitesse des caractères (mots/minute).
    pub char_wpm: f32,
    /// Vitesse effective (Farnsworth), <= char_wpm.
    pub eff_wpm: f32,
    /// Fréquence du son (Hz).
    pub freq: f32,
    /// Volume 0..1.
    pub volume: f32,
    /// Durée d'une session chronométrée (secondes).
    pub session_secs: u32,
    /// Précision (0..1) à atteindre pour débloquer de nouveaux caractères.
    pub unlock_threshold: f32,
    /// Montrer le motif ·− après une erreur.
    pub show_pattern_on_error: bool,
    /// Langue des listes de mots.
    pub lang: Lang,
    /// Langue de l'interface (par défaut : celle du système).
    #[serde(default = "system_lang")]
    pub ui_lang: Lang,
    /// "auto" ; sous Linux aussi "pw-cat" ou "aplay".
    pub backend: String,
    /// Si vrai, `save` ne fait rien (tests).
    #[serde(skip)]
    pub ephemeral: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            track: Track::Lessons,
            char_wpm: 20.0,
            eff_wpm: 8.0,
            freq: 650.0,
            volume: 0.6,
            session_secs: 300,
            unlock_threshold: 0.90,
            show_pattern_on_error: true,
            lang: Lang::Fr,
            ui_lang: Lang::Fr,
            backend: "auto".into(),
            ephemeral: false,
        }
    }
}

/// Français si la langue du système est le français, anglais sinon.
fn system_lang() -> Lang {
    let var = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
    match var {
        Some(v) if v.to_lowercase().starts_with("fr") => Lang::Fr,
        _ => Lang::En,
    }
}

pub fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tita")
}

fn path() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("config.toml"))
}

impl Config {
    pub fn timing(&self) -> Timing {
        Timing::new(self.char_wpm, self.eff_wpm)
    }

    pub fn load() -> Config {
        // Fichier absent = texte vide : les valeurs par défaut par champ
        // s'appliquent (dont la langue du système).
        let text = path().and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
        toml::from_str(&text)
            .or_else(|_| toml::from_str(""))
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        if self.ephemeral {
            return Ok(());
        }
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(p, toml::to_string_pretty(self)?)?;
        }
        Ok(())
    }
}
