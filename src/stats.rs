//! Statistiques d'apprentissage, sauvegardées en JSON dans ~/.local/share/tita/.

use crate::config;
use crate::curriculum::Track;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const RECENT: usize = 30;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CharStat {
    pub seen: u32,
    pub correct: u32,
    /// Temps de réaction moyen (moyenne glissante exponentielle), en ms.
    pub rt_ms: f32,
    /// Derniers résultats (true = juste), les plus récents à la fin.
    pub recent: Vec<bool>,
}

impl CharStat {
    /// Précision récente (0..1), `None` si jamais vu.
    pub fn recent_accuracy(&self) -> Option<f32> {
        if self.recent.is_empty() {
            return None;
        }
        Some(self.recent.iter().filter(|b| **b).count() as f32 / self.recent.len() as f32)
    }
}

/// Résumé d'une session terminée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    /// Date locale AAAA-MM-JJ.
    pub date: String,
    /// Nom du mode ("Lettres", "Koch", "Flux", ...).
    pub mode: String,
    pub secs: u32,
    /// Caractères présentés / corrects.
    pub total: u32,
    pub correct: u32,
    pub char_wpm: f32,
    pub eff_wpm: f32,
}

impl Session {
    pub fn accuracy(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.correct as f32 / self.total as f32 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub chars: BTreeMap<char, CharStat>,
    /// Confusions : clé "XY" = X envoyé, Y tapé.
    pub confusions: BTreeMap<String, u32>,
    pub sessions: Vec<Session>,
    /// Nombre de caractères débloqués par parcours ("lessons" / "koch").
    pub unlocked: BTreeMap<String, usize>,
    /// Si vrai, `save` ne fait rien (tests).
    #[serde(skip)]
    pub ephemeral: bool,
}

fn path() -> Option<PathBuf> {
    config::dirs().map(|d| d.data_dir().join("stats.json"))
}

fn track_key(t: Track) -> &'static str {
    match t {
        Track::Lessons => "lessons",
        Track::Koch => "koch",
    }
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl Stats {
    pub fn load() -> Stats {
        path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
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
            fs::write(p, serde_json::to_string(self)?)?;
        }
        Ok(())
    }

    /// Enregistre une réponse. `typed = None` : caractère raté (pas de réponse).
    /// `rt` : temps de réaction, si mesuré.
    pub fn record(&mut self, sent: char, typed: Option<char>, rt: Option<Duration>) {
        let sent = sent.to_ascii_uppercase();
        let typed = typed.map(|c| c.to_ascii_uppercase());
        let ok = typed == Some(sent);
        let s = self.chars.entry(sent).or_default();
        s.seen += 1;
        if ok {
            s.correct += 1;
            if let Some(rt) = rt {
                let ms = rt.as_secs_f32() * 1000.0;
                s.rt_ms = if s.rt_ms == 0.0 { ms } else { 0.8 * s.rt_ms + 0.2 * ms };
            }
        } else if let Some(t) = typed {
            *self.confusions.entry(format!("{sent}{t}")).or_default() += 1;
        }
        s.recent.push(ok);
        if s.recent.len() > RECENT {
            s.recent.remove(0);
        }
    }

    pub fn char(&self, c: char) -> Option<&CharStat> {
        self.chars.get(&c.to_ascii_uppercase())
    }

    /// Précision récente moyenne sur un ensemble de caractères ; un caractère
    /// jamais vu compte pour 0. `None` si `chars` est vide.
    pub fn recent_accuracy(&self, chars: &[char]) -> Option<f32> {
        if chars.is_empty() {
            return None;
        }
        let sum: f32 = chars
            .iter()
            .map(|c| self.char(*c).and_then(|s| s.recent_accuracy()).unwrap_or(0.0))
            .sum();
        Some(sum / chars.len() as f32)
    }

    /// Poids de tirage : plus élevé pour les caractères faibles, lents ou neufs.
    pub fn weight(&self, c: char) -> f32 {
        let Some(s) = self.char(c) else { return 4.0 };
        let acc = s.recent_accuracy().unwrap_or(0.0);
        let novelty = if s.seen < 15 { 2.0 } else { 0.0 };
        let slow = ((s.rt_ms - 600.0) / 600.0).clamp(0.0, 1.5);
        1.0 + 6.0 * (1.0 - acc) + novelty + slow
    }

    /// Les confusions les plus fréquentes : (envoyé, tapé, nombre).
    pub fn top_confusions(&self, n: usize) -> Vec<(char, char, u32)> {
        let mut v: Vec<_> = self
            .confusions
            .iter()
            .filter_map(|(k, n)| {
                let mut it = k.chars();
                Some((it.next()?, it.next()?, *n))
            })
            .collect();
        v.sort_by(|a, b| b.2.cmp(&a.2));
        v.truncate(n);
        v
    }

    pub fn unlocked(&self, t: Track) -> usize {
        self.unlocked.get(track_key(t)).copied().unwrap_or(0).max(t.initial())
    }

    pub fn set_unlocked(&mut self, t: Track, n: usize) {
        self.unlocked.insert(track_key(t).into(), n.clamp(t.initial(), t.order().len()));
    }

    /// Nombre de jours consécutifs avec au moins une session (jusqu'à
    /// aujourd'hui ou hier).
    pub fn streak(&self) -> u32 {
        use chrono::{Duration as D, Local, NaiveDate};
        let mut days: Vec<NaiveDate> = self
            .sessions
            .iter()
            .filter_map(|s| NaiveDate::parse_from_str(&s.date, "%Y-%m-%d").ok())
            .collect();
        days.sort();
        days.dedup();
        let today = Local::now().date_naive();
        let mut expect = match days.last() {
            Some(d) if *d == today || *d == today - D::days(1) => *d,
            _ => return 0,
        };
        let mut n = 0;
        for d in days.iter().rev() {
            if *d != expect {
                break;
            }
            n += 1;
            expect -= D::days(1);
        }
        n
    }

    /// Temps total d'entraînement aujourd'hui, en secondes.
    pub fn secs_today(&self) -> u32 {
        let t = today();
        self.sessions.iter().filter(|s| s.date == t).map(|s| s.secs).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_tracks_accuracy_and_confusions() {
        let mut s = Stats::default();
        s.record('s', Some('S'), Some(Duration::from_millis(500)));
        s.record('S', Some('H'), None);
        s.record('S', None, None);
        let c = s.char('S').unwrap();
        assert_eq!((c.seen, c.correct), (3, 1));
        assert_eq!(s.top_confusions(5), vec![('S', 'H', 1)]);
        assert!(s.weight('S') > 1.0);
    }
}
