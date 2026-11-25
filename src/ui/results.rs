//! Écran de bilan de fin de session.

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::modes::discover::Discover;
use crate::morse;
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
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
        let hint = if self.new_chars.is_empty() { "retour" } else { "découvrir les nouveautés" };
        let area = ui::frame(f, &format!("Bilan — {}", self.s.mode), &[("Entrée", hint)]);
        let s = &self.s;
        let acc = s.correct as f32 / s.total.max(1) as f32;
        let mut lines = vec![
            Line::from(vec![
                Span::raw("Précision   "),
                Span::styled(format!("{:.0} %", acc * 100.0), bold(acc_color(acc))),
                Span::styled(format!("   ({} / {})", s.correct, s.total), dim()),
            ]),
            Line::from(format!("Durée       {}", ui::mmss(s.elapsed.as_secs()))),
            Line::from(format!(
                "Vitesse     {:.0} mots/min (effective {:.0})",
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
            let mut spans = vec![Span::raw("À retravailler  ")];
            for (c, n) in m.iter().take(8) {
                spans.push(Span::styled(format!("{c}"), bold(ui::ERR)));
                spans.push(Span::styled(format!("×{n}  "), dim()));
            }
            lines.push(Line::from(spans));
            lines.push(Line::raw(""));
        }
        if !self.new_chars.is_empty() {
            let mut spans = vec![Span::styled("Débloqué !  ", bold(ui::OK))];
            for c in &self.new_chars {
                spans.push(Span::styled(format!("{c} "), bold(ui::ACCENT)));
                spans.push(Span::styled(format!("{}   ", morse::pretty(*c)), dim()));
            }
            lines.push(Line::from(spans));
        } else if s.can_unlock {
            let need = ctx.cfg.unlock_threshold * 100.0;
            lines.push(Line::styled(
                format!("Prochain déblocage à {need:.0} % de précision (20 caractères minimum)."),
                dim(),
            ));
        }
        lines.push(Line::styled(
            format!(
                "Aujourd'hui : {} d'entraînement · série de {} jour(s)",
                ui::mmss(ctx.stats.secs_today() as u64),
                ctx.stats.streak()
            ),
            dim(),
        ));
        let h = lines.len() as u16;
        f.render_widget(Paragraph::new(lines), ui::centered(area, 64, h));
    }
}
