//! Écran de bilan de fin de session.

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::modes::discover::Discover;
use crate::morse;
use crate::i18n::t;
use crate::tr;
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use ratatui::Frame;

pub struct Results {
    s: Summary,
    new_chars: Vec<char>,
}

impl Results {
    pub fn new(s: Summary, new_chars: Vec<char>) -> Self {
        Results { s, new_chars }
    }
}

impl Screen for Results {
    fn on_key(&mut self, key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') | KeyCode::Char('q') => {
                if self.new_chars.is_empty() || key.code == KeyCode::Esc {
                    Nav::Back
                } else {
                    Nav::Replace(Box::new(Discover::new(std::mem::take(&mut self.new_chars))))
                }
            }
            _ => Nav::Stay,
        }
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let hint = if self.new_chars.is_empty() {
            t("retour", "back")
        } else {
            t("découvrir les nouveautés", "discover the new characters")
        };
        let mut hints = vec![(t("Entrée", "Enter"), hint)];
        if !self.new_chars.is_empty() {
            hints.push((t("Échap", "Esc"), t("retour", "back")));
        }
        let area = ui::frame(f, &tr!("Bilan — {}", "Summary — {}", self.s.mode), &hints);
        let s = &self.s;
        let acc = s.correct as f32 / s.total.max(1) as f32;
        let mut lines = vec![
            Line::from(vec![
                Span::raw(t("Précision   ", "Accuracy    ")),
                Span::styled(format!("{:.0} %", acc * 100.0), bold(acc_color(acc))),
                Span::styled(format!("   ({} / {})", s.correct, s.total), dim()),
            ]),
            Line::from(tr!("Durée       {}", "Duration    {}", ui::mmss(s.elapsed.as_secs()))),
            Line::from(tr!(
                "Vitesse     {:.0} mots/min (effective {:.0})",
                "Speed       {:.0} wpm (effective {:.0})",
                s.timing.char_wpm, s.timing.eff_wpm
            )),
        ];
        for n in &s.notes {
            lines.push(Line::from(n.clone()));
        }
        lines.push(Line::raw(""));
        if !s.misses.is_empty() {
            let mut m = s.misses.clone();
            m.sort_by(|a, b| b.1.cmp(&a.1));
            let mut spans = vec![Span::raw(t("À retravailler  ", "To work on  "))];
            for (c, n) in m.iter().take(8) {
                spans.push(Span::styled(format!("{c}"), bold(ui::ERR)));
                spans.push(Span::styled(format!("×{n}  "), dim()));
            }
            lines.push(Line::from(spans));
            lines.push(Line::raw(""));
        }
        if !self.new_chars.is_empty() {
            let mut spans = vec![Span::styled(t("Débloqué !  ", "Unlocked!  "), bold(ui::OK))];
            for c in &self.new_chars {
                spans.push(Span::styled(format!("{c} "), bold(ui::ACCENT)));
                spans.push(Span::styled(format!("{}   ", morse::pretty(*c)), dim()));
            }
            lines.push(Line::from(spans));
        } else if s.can_unlock {
            let need = ctx.cfg.unlock_threshold * 100.0;
            lines.push(Line::styled(
                tr!(
                    "Prochain déblocage à {need:.0} % de précision (20 caractères minimum).",
                    "Next unlock at {need:.0}% accuracy (20 characters minimum)."
                ),
                dim(),
            ));
        }
        lines.push(Line::styled(
            tr!(
                "Aujourd'hui : {} d'entraînement · série de {} jour(s)",
                "Today: {} of training · {}-day streak",
                ui::mmss(ctx.stats.secs_today() as u64),
                ctx.stats.streak()
            ),
            dim(),
        ));
        ui::text(f, lines, area, 64, Alignment::Left);
    }
}
