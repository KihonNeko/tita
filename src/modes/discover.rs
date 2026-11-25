//! Découverte : présentation des caractères (son + motif), puis courte
//! vérification à l'oreille quand la liste est petite.

use crate::app::{Ctx, Nav, Screen};
use crate::morse;
use crate::ui::{self, bold, dim};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rand::seq::SliceRandom;
use ratatui::layout::Alignment;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::{Duration, Instant};

/// Au-delà de ce nombre de caractères, pas de mini-vérification.
const CHECK_MAX: usize = 6;
const FLASH: Duration = Duration::from_millis(500);

enum Phase {
    /// Présentation du caractère `idx`.
    Present,
    /// Vérification : `queue[0]` est joué sans être affiché.
    Check {
        queue: Vec<char>,
        /// Mauvaise touche tapée : le caractère attendu est montré.
        wrong: Option<char>,
    },
}

pub struct Discover {
    chars: Vec<char>,
    idx: usize,
    phase: Phase,
    /// Dernier caractère réussi en vérification (bref retour vert).
    flash: Option<(char, Instant)>,
}

impl Discover {
    pub fn new(chars: Vec<char>) -> Self {
        let chars = chars.into_iter().filter(|c| morse::pattern(*c).is_some()).collect();
        Discover { chars, idx: 0, phase: Phase::Present, flash: None }
    }

    fn has_check(&self) -> bool {
        self.chars.len() <= CHECK_MAX
    }

    /// Joue le caractère présenté : deux fois, séparées par un silence.
    fn play_present(&self, ctx: &Ctx) {
        let Some(&c) = self.chars.get(self.idx) else { return };
        let s = c.to_string();
        ctx.audio.stop();
        ctx.audio.silence(Duration::from_millis(150));
        ctx.audio.play(&s, ctx.timing());
        ctx.audio.silence(Duration::from_millis(700));
        ctx.audio.play(&s, ctx.timing());
    }

    fn play_check(&self, ctx: &Ctx) {
        if let Phase::Check { queue, .. } = &self.phase {
            if let Some(c) = queue.first() {
                ctx.audio.stop();
                ctx.audio.silence(Duration::from_millis(350));
                ctx.audio.play(&c.to_string(), ctx.timing());
            }
        }
    }

    fn start_check(&mut self, ctx: &Ctx) {
        let mut queue = self.chars.clone();
        queue.shuffle(&mut rand::rng());
        self.phase = Phase::Check { queue, wrong: None };
        self.flash = None;
        self.play_check(ctx);
    }

    fn key_present(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        match key.code {
            KeyCode::Char(' ') => self.play_present(ctx),
            KeyCode::Right | KeyCode::Enter => {
                if self.idx + 1 < self.chars.len() {
                    self.idx += 1;
                    self.play_present(ctx);
                } else if self.has_check() {
                    self.start_check(ctx);
                } else {
                    return Nav::Back;
                }
            }
            KeyCode::Left => {
                if self.idx > 0 {
                    self.idx -= 1;
                    self.play_present(ctx);
                }
            }
            KeyCode::Home => {
                if self.idx != 0 {
                    self.idx = 0;
                    self.play_present(ctx);
                }
            }
            KeyCode::End => {
                if self.idx + 1 != self.chars.len() {
                    self.idx = self.chars.len() - 1;
                    self.play_present(ctx);
                }
            }
            _ => {}
        }
        Nav::Stay
    }

    fn key_check(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        match key.code {
            KeyCode::Left => {
                // Retour à la présentation, sur le dernier caractère.
                self.phase = Phase::Present;
                self.idx = self.chars.len() - 1;
                self.play_present(ctx);
                return Nav::Stay;
            }
            KeyCode::Char(' ') => {
                self.play_check(ctx);
                return Nav::Stay;
            }
            KeyCode::Char(k) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                let typed = k.to_ascii_uppercase();
                if morse::pattern(typed).is_none() {
                    return Nav::Stay;
                }
                let Phase::Check { queue, wrong } = &mut self.phase else { return Nav::Stay };
                let expected = queue[0];
                if typed == expected {
                    if wrong.take().is_some() {
                        // Réussi seulement après avoir vu la réponse : il reviendra.
                        queue.rotate_left(1);
                    } else {
                        queue.remove(0);
                        self.flash = Some((expected, Instant::now()));
                    }
                    if queue.is_empty() {
                        ctx.audio.stop();
                        return Nav::Back;
                    }
                } else if wrong.is_none() {
                    *wrong = Some(typed);
                } else {
                    return Nav::Stay;
                }
                self.play_check(ctx);
            }
            _ => {}
        }
        Nav::Stay
    }
}

impl Screen for Discover {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        match self.phase {
            Phase::Present => self.play_present(ctx),
            Phase::Check { .. } => self.play_check(ctx),
        }
    }

    fn on_leave(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if key.code == KeyCode::Esc || self.chars.is_empty() {
            ctx.audio.stop();
            return Nav::Back;
        }
        match self.phase {
            Phase::Present => self.key_present(key, ctx),
            Phase::Check { .. } => self.key_check(key, ctx),
        }
    }

    fn draw(&mut self, f: &mut Frame, _ctx: &Ctx) {
        let n = self.chars.len();
        let mut lines: Vec<Line> = Vec::new();
        let hints: &[(&str, &str)];
        match &self.phase {
            Phase::Present => {
                hints = &[("Espace", "rejouer"), ("← →", "précédent / suivant"), ("Échap", "retour")];
                if n == 0 {
                    lines.push(Line::styled("Aucun caractère à découvrir.", dim()));
                } else {
                    let c = self.chars[self.idx];
                    lines.push(Line::styled(format!("{} / {}", self.idx + 1, n), dim()));
                    lines.push(Line::raw(""));
                    lines.push(Line::styled(c.to_string(), bold(ui::ACCENT)));
                    lines.push(Line::raw(""));
                    lines.push(Line::styled(morse::pretty(c), bold(ratatui::style::Color::White)));
                    lines.push(Line::raw(""));
                    let strip: Vec<Span> = self
                        .chars
                        .iter()
                        .enumerate()
                        .map(|(i, ch)| {
                            let style = if i == self.idx { bold(ui::ACCENT) } else { dim() };
                            Span::styled(format!("{ch} "), style)
                        })
                        .collect();
                    lines.push(Line::from(strip));
                    lines.push(Line::raw(""));
                    let next = if self.idx + 1 < n {
                        "Écoute bien le rythme, puis passe au suivant."
                    } else if self.has_check() {
                        "Dernier caractère. Ensuite : petite vérification à l'oreille."
                    } else {
                        "Dernier caractère. Entrée pour terminer."
                    };
                    lines.push(Line::styled(next, dim()));
                }
            }
            Phase::Check { queue, wrong } => {
                hints = &[("Espace", "rejouer"), ("←", "revoir"), ("Échap", "retour")];
                lines.push(Line::styled(format!("Vérification : {} / {} réussis", n - queue.len(), n), dim()));
                lines.push(Line::raw(""));
                match wrong {
                    Some(typed) => {
                        let c = queue[0];
                        lines.push(Line::styled(c.to_string(), bold(ui::ERR)));
                        lines.push(Line::raw(""));
                        lines.push(Line::raw(morse::pretty(c)));
                        lines.push(Line::raw(""));
                        lines.push(Line::styled(
                            format!("Tu as tapé {typed}. C'était {c} : tape-le pour continuer."),
                            dim(),
                        ));
                    }
                    None => {
                        lines.push(Line::styled("?", bold(ui::ACCENT)));
                        lines.push(Line::raw(""));
                        match self.flash {
                            Some((c, t)) if t.elapsed() < FLASH => {
                                lines.push(Line::styled(format!("✓ {c}"), bold(ui::OK)))
                            }
                            _ => lines.push(Line::raw("")),
                        }
                        lines.push(Line::raw(""));
                        lines.push(Line::styled("Écoute, puis tape le caractère entendu.", dim()));
                    }
                }
            }
        }
        let area = ui::frame(f, "Découverte", hints);
        let h = lines.len() as u16;
        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            ui::centered(area, area.width, h),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    fn expected(d: &Discover) -> char {
        match &d.phase {
            Phase::Check { queue, .. } => queue[0],
            Phase::Present => panic!("pas en vérification"),
        }
    }

    #[test]
    fn presents_with_pattern_and_navigates() {
        let mut ctx = Ctx::test();
        let mut d = Discover::new(vec!['E', 'T', 'A']);
        d.on_enter(&mut ctx);
        assert!(!ctx.audio.is_idle(), "le son part à l'arrivée");
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(s.contains("1 / 3") && s.contains('·'));
        assert!(matches!(d.on_key(key(KeyCode::Right), &mut ctx), Nav::Stay));
        assert!(matches!(d.on_key(key(KeyCode::Enter), &mut ctx), Nav::Stay));
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(s.contains("3 / 3") && s.contains("· −"));
        d.on_key(key(KeyCode::Left), &mut ctx);
        assert_eq!(d.idx, 1);
        d.on_key(key(KeyCode::Left), &mut ctx);
        d.on_key(key(KeyCode::Left), &mut ctx);
        assert_eq!(d.idx, 0);
        assert!(matches!(d.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
    }

    #[test]
    fn check_requires_each_char_once() {
        let mut ctx = Ctx::test();
        let mut d = Discover::new(vec!['E', 'T']);
        d.on_enter(&mut ctx);
        d.on_key(key(KeyCode::Right), &mut ctx);
        assert!(matches!(d.on_key(key(KeyCode::Right), &mut ctx), Nav::Stay));
        assert!(matches!(d.phase, Phase::Check { .. }));
        // Rien n'est montré pendant l'écoute.
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(!s.contains('·') && !s.contains('−'));

        // Erreur : montré, puis la bonne touche ne compte pas comme réussite.
        let first = expected(&d);
        let other = if first == 'E' { 'T' } else { 'E' };
        d.on_key(key(KeyCode::Char(other.to_ascii_lowercase())), &mut ctx);
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(s.contains("C'était") && (s.contains('·') || s.contains('−')));
        assert!(matches!(d.on_key(key(KeyCode::Char(first)), &mut ctx), Nav::Stay));
        assert_eq!(expected(&d), other);

        // Touches hors table ignorées.
        d.on_key(key(KeyCode::Char('#')), &mut ctx);
        assert_eq!(expected(&d), other);

        assert!(matches!(d.on_key(key(KeyCode::Char(other)), &mut ctx), Nav::Stay));
        assert_eq!(expected(&d), first);
        assert!(matches!(d.on_key(key(KeyCode::Char(first.to_ascii_lowercase())), &mut ctx), Nav::Back));
        // Aucune statistique enregistrée.
        assert!(ctx.stats.chars.is_empty() && ctx.stats.sessions.is_empty());
    }

    #[test]
    fn long_list_skips_check() {
        let mut ctx = Ctx::test();
        let chars: Vec<char> = "ETAINMSURWDK".chars().collect();
        let mut d = Discover::new(chars);
        d.on_enter(&mut ctx);
        for _ in 0..11 {
            assert!(matches!(d.on_key(key(KeyCode::Right), &mut ctx), Nav::Stay));
        }
        assert!(render_to_string(&mut d, &ctx, 80, 24).contains("12 / 12"));
        assert!(matches!(d.on_key(key(KeyCode::Right), &mut ctx), Nav::Back));
    }

    #[test]
    fn renders_at_any_size_and_survives_empty_list() {
        let mut ctx = Ctx::test();
        let mut d = Discover::new(vec!['E', 'T']);
        d.on_enter(&mut ctx);
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            render_to_string(&mut d, &ctx, w, h);
        }
        d.on_key(key(KeyCode::Right), &mut ctx);
        d.on_key(key(KeyCode::Right), &mut ctx);
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            render_to_string(&mut d, &ctx, w, h);
        }
        let mut e = Discover::new(vec![]);
        e.on_enter(&mut ctx);
        render_to_string(&mut e, &ctx, 20, 6);
        assert!(matches!(e.on_key(key(KeyCode::Right), &mut ctx), Nav::Back));
    }
}
