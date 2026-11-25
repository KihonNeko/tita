//! Lettres : un caractère joue, tu tapes la touche. Session chronométrée ou
//! sans fin. La logique temporelle passe par `key_at` / `tick_at`, qui
//! reçoivent l'instant courant (testable sans attendre).

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::morse;
use crate::picker;
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rand::Rng;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Pause après une bonne réponse (session chronométrée).
const NEXT_DELAY: Duration = Duration::from_millis(300);
/// Durée du retour vert en mode sans fin (le caractère suivant joue déjà).
const FLASH: Duration = Duration::from_millis(400);
const HISTORY: usize = 20;

#[derive(Clone, Copy)]
enum Phase {
    /// « Prêt ? » : rien n'a encore joué.
    Ready,
    /// Le caractère courant a été joué, on attend la frappe.
    Listen,
    /// Bonne réponse affichée, le suivant part à `until`.
    Pause { until: Instant },
    /// Erreur : le caractère attendu est montré, il faut le taper.
    Fix { typed: char },
}

pub struct Drill {
    endless: bool,
    phase: Phase,
    current: Option<char>,
    /// Fin (estimée) du son du caractère courant.
    sound_end: Instant,
    started: Option<Instant>,
    /// Instant de la dernière frappe utile : borne la durée de la session.
    last_input: Option<Instant>,
    total: u32,
    correct: u32,
    streak: u32,
    best_streak: u32,
    last_rt: Option<Duration>,
    rt_sum: Duration,
    rt_n: u32,
    history: VecDeque<(char, bool)>,
    misses: Vec<(char, u32)>,
    /// Caractères ratés à représenter : (caractère, tirages restants).
    retry: Vec<(char, u32)>,
    flash: Option<(char, Instant)>,
}

impl Drill {
    pub fn new(endless: bool) -> Self {
        Drill {
            endless,
            phase: Phase::Ready,
            current: None,
            sound_end: Instant::now(),
            started: None,
            last_input: None,
            total: 0,
            correct: 0,
            streak: 0,
            best_streak: 0,
            last_rt: None,
            rt_sum: Duration::ZERO,
            rt_n: 0,
            history: VecDeque::new(),
            misses: Vec::new(),
            retry: Vec::new(),
            flash: None,
        }
    }

    fn name(&self) -> &'static str {
        if self.endless { "Lettres sans fin" } else { "Lettres" }
    }

    fn elapsed(&self, now: Instant) -> Duration {
        self.started.map(|s| now.saturating_duration_since(s)).unwrap_or_default()
    }

    fn time_up(&self, ctx: &Ctx, now: Instant) -> bool {
        !self.endless && self.elapsed(now) >= Duration::from_secs(ctx.cfg.session_secs as u64)
    }

    fn avg_rt(&self) -> Option<Duration> {
        (self.rt_n > 0).then(|| self.rt_sum / self.rt_n)
    }

    fn finish(&mut self, ctx: &mut Ctx) -> Nav {
        let elapsed = match (self.started, self.last_input) {
            (Some(s), Some(l)) => l.saturating_duration_since(s),
            _ => Duration::ZERO,
        };
        let mut notes = Vec::new();
        if let Some(avg) = self.avg_rt() {
            notes.push(format!("Réaction    {} ms en moyenne", avg.as_millis()));
        }
        if self.best_streak > 1 {
            notes.push(format!("Série       {} bonnes réponses d'affilée", self.best_streak));
        }
        ctx.finish(Summary {
            mode: self.name().into(),
            elapsed,
            total: self.total,
            correct: self.correct,
            timing: ctx.timing(),
            can_unlock: true,
            misses: std::mem::take(&mut self.misses),
            notes,
        })
    }

    /// (Re)joue le caractère courant. `cut` : coupe d'abord ce qui joue.
    fn play_current(&mut self, ctx: &Ctx, cut: bool) {
        let Some(c) = self.current else { return };
        let t = ctx.timing();
        if cut {
            ctx.audio.stop();
            ctx.audio.silence(Duration::from_millis(250));
        } else if !ctx.audio.is_idle() {
            // Réponse donnée avant la fin du son : garder un espace minimal.
            ctx.audio.silence(Duration::from_secs_f32(3.0 * t.dit()));
        }
        self.sound_end = ctx.audio.play(&c.to_string(), t).end;
    }

    /// Passe au caractère suivant, ou termine si le temps est écoulé.
    fn next_char(&mut self, ctx: &mut Ctx, now: Instant) -> Nav {
        let chars = ctx.chars();
        if chars.is_empty() || self.time_up(ctx, now) {
            return self.finish(ctx);
        }
        let last = self.current;
        for r in &mut self.retry {
            r.1 = r.1.saturating_sub(1);
        }
        let due = self
            .retry
            .iter()
            .position(|(c, n)| *n == 0 && Some(*c) != last && chars.contains(c));
        let c = match due {
            Some(i) => self.retry.remove(i).0,
            None => picker::pick(&chars, &ctx.stats, last, &mut rand::rng()),
        };
        // Tiré par hasard avant son retour programmé : inutile de le reprogrammer.
        self.retry.retain(|(r, _)| *r != c);
        self.current = Some(c);
        self.phase = Phase::Listen;
        self.play_current(ctx, false);
        Nav::Stay
    }

    fn after_answer(&mut self, ctx: &mut Ctx, now: Instant) -> Nav {
        if self.endless {
            self.next_char(ctx, now)
        } else {
            self.phase = Phase::Pause { until: now + NEXT_DELAY };
            Nav::Stay
        }
    }

    fn answer(&mut self, typed: char, ctx: &mut Ctx, now: Instant) -> Nav {
        let Some(c) = self.current else { return Nav::Stay };
        let rt = now.saturating_duration_since(self.sound_end);
        ctx.stats.record(c, Some(typed), Some(rt));
        self.total += 1;
        self.last_input = Some(now);
        let ok = typed == c;
        self.history.push_back((c, ok));
        while self.history.len() > HISTORY {
            self.history.pop_front();
        }
        if ok {
            self.correct += 1;
            self.streak += 1;
            self.best_streak = self.best_streak.max(self.streak);
            self.last_rt = Some(rt);
            self.rt_sum += rt;
            self.rt_n += 1;
            self.flash = Some((c, now));
            self.after_answer(ctx, now)
        } else {
            self.streak = 0;
            match self.misses.iter_mut().find(|(m, _)| *m == c) {
                Some(m) => m.1 += 1,
                None => self.misses.push((c, 1)),
            }
            self.retry.retain(|(r, _)| *r != c);
            self.retry.push((c, rand::rng().random_range(2..=3)));
            self.phase = Phase::Fix { typed };
            self.play_current(ctx, true);
            Nav::Stay
        }
    }

    pub fn key_at(&mut self, key: KeyEvent, ctx: &mut Ctx, now: Instant) -> Nav {
        if key.code == KeyCode::Esc {
            return self.finish(ctx);
        }
        if let Phase::Ready = self.phase {
            self.started = Some(now);
            return self.next_char(ctx, now);
        }
        let KeyCode::Char(k) = key.code else { return Nav::Stay };
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return Nav::Stay;
        }
        if k == ' ' {
            if matches!(self.phase, Phase::Listen | Phase::Fix { .. }) {
                self.play_current(ctx, true);
            }
            return Nav::Stay;
        }
        let typed = k.to_ascii_uppercase();
        if morse::pattern(typed).is_none() {
            return Nav::Stay;
        }
        match self.phase {
            Phase::Listen => self.answer(typed, ctx, now),
            Phase::Fix { .. } if Some(typed) == self.current => {
                // Frappe de correction : non enregistrée.
                self.last_input = Some(now);
                self.flash = None;
                self.after_answer(ctx, now)
            }
            _ => Nav::Stay,
        }
    }

    pub fn tick_at(&mut self, ctx: &mut Ctx, now: Instant) -> Nav {
        match self.phase {
            Phase::Pause { until } if now >= until => self.next_char(ctx, now),
            _ => Nav::Stay,
        }
    }

    fn status_line(&self, ctx: &Ctx, now: Instant) -> Line<'static> {
        let secs = self.elapsed(now).as_secs();
        let (label, clock) = if self.endless {
            ("Écoulé ", ui::mmss(secs))
        } else {
            ("Reste ", ui::mmss((ctx.cfg.session_secs as u64).saturating_sub(secs)))
        };
        let mut spans = vec![Span::styled(label, dim()), Span::styled(clock, bold(ui::ACCENT))];
        spans.push(Span::styled("   Précision ", dim()));
        if self.total > 0 {
            let acc = self.correct as f32 / self.total as f32;
            spans.push(Span::styled(format!("{:.0} %", acc * 100.0), bold(acc_color(acc))));
            spans.push(Span::styled(format!(" ({}/{})", self.correct, self.total), dim()));
        } else {
            spans.push(Span::styled("—", dim()));
        }
        spans.push(Span::styled("   Série ", dim()));
        spans.push(Span::raw(self.streak.to_string()));
        spans.push(Span::styled("   Réaction ", dim()));
        match self.last_rt {
            Some(rt) => spans.push(Span::raw(format!("{} ms", rt.as_millis()))),
            None => spans.push(Span::styled("—", dim())),
        }
        if self.endless {
            if let Some(avg) = self.avg_rt() {
                spans.push(Span::styled("   Moyenne ", dim()));
                spans.push(Span::styled(format!("{} ms", avg.as_millis()), bold(ui::ACCENT)));
            }
        }
        Line::from(spans)
    }
}

impl Screen for Drill {
    fn on_leave(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        self.key_at(key, ctx, Instant::now())
    }

    fn on_tick(&mut self, ctx: &mut Ctx) -> Nav {
        self.tick_at(ctx, Instant::now())
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let now = Instant::now();
        let area = ui::frame(f, self.name(), &[("Espace", "rejouer"), ("Échap", "terminer")]);
        let [top, mid, bottom] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        if !matches!(self.phase, Phase::Ready) {
            f.render_widget(Paragraph::new(self.status_line(ctx, now)), top);
        }

        let mut lines: Vec<Line> = Vec::new();
        match self.phase {
            Phase::Ready => {
                lines.push(Line::styled("Prêt ? Appuie sur une touche.", bold(ui::ACCENT)));
                lines.push(Line::raw(""));
                lines.push(Line::styled("Un caractère joue, tu tapes sa touche.", dim()));
                let rule = if self.endless {
                    "Pas de limite de durée : Échap pour terminer.".to_string()
                } else {
                    format!("Session de {}.", ui::mmss(ctx.cfg.session_secs as u64))
                };
                lines.push(Line::styled(rule, dim()));
            }
            Phase::Listen => {
                lines.push(Line::styled("?", bold(ui::ACCENT)));
                lines.push(Line::raw(""));
                match self.flash {
                    Some((c, t)) if self.endless && now.saturating_duration_since(t) < FLASH => {
                        lines.push(Line::styled(format!("✓ {c}"), bold(ui::OK)))
                    }
                    _ => lines.push(Line::styled("Écoute, puis tape le caractère.", dim())),
                }
            }
            Phase::Pause { .. } => {
                let c = self.current.unwrap_or(' ');
                lines.push(Line::styled(c.to_string(), bold(ui::OK)));
                lines.push(Line::raw(""));
                lines.push(Line::styled("✓", bold(ui::OK)));
            }
            Phase::Fix { typed } => {
                let c = self.current.unwrap_or(' ');
                lines.push(Line::styled(c.to_string(), bold(ui::ERR)));
                lines.push(Line::raw(""));
                if ctx.cfg.show_pattern_on_error {
                    lines.push(Line::raw(morse::pretty(c)));
                    lines.push(Line::raw(""));
                }
                lines.push(Line::styled(
                    format!("Tu as tapé {typed}. C'était {c} : tape-le pour continuer."),
                    dim(),
                ));
            }
        }
        let h = lines.len() as u16;
        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            ui::centered(mid, mid.width, h),
        );

        let hist: Vec<Span> = self
            .history
            .iter()
            .map(|(c, ok)| Span::styled(format!("{c} "), Style::default().fg(if *ok { ui::OK } else { ui::ERR })))
            .collect();
        f.render_widget(Paragraph::new(Line::from(hist)).alignment(Alignment::Center), bottom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    /// Démarre une session et renvoie (écran, contexte, caractère courant).
    fn started(endless: bool) -> (Drill, Ctx, char) {
        let mut ctx = Ctx::test();
        let mut d = Drill::new(endless);
        d.on_enter(&mut ctx);
        assert!(render_to_string(&mut d, &ctx, 80, 24).contains("Prêt ? Appuie sur une touche"));
        assert!(ctx.audio.is_idle(), "rien ne joue avant la première touche");
        assert!(matches!(d.key_at(key('x'), &mut ctx, Instant::now()), Nav::Stay));
        assert!(!ctx.audio.is_idle());
        let c = d.current.unwrap();
        (d, ctx, c)
    }

    fn other(ctx: &Ctx, c: char) -> char {
        ctx.chars().into_iter().find(|x| *x != c).expect("au moins deux caractères débloqués")
    }

    #[test]
    fn correct_answer_is_recorded_with_reaction_time() {
        let (mut d, mut ctx, c) = started(false);
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(!s.contains('−') && !s.contains('·'), "aucun motif pendant l'écoute");

        let t = d.sound_end + Duration::from_millis(420);
        assert!(matches!(d.key_at(key(c.to_ascii_lowercase()), &mut ctx, t), Nav::Stay));
        let st = ctx.stats.char(c).unwrap();
        assert_eq!((st.seen, st.correct), (1, 1));
        assert!((st.rt_ms - 420.0).abs() < 1.0);
        assert_eq!((d.total, d.correct, d.streak), (1, 1, 1));
        assert_eq!(d.last_rt, Some(Duration::from_millis(420)));
        assert!(render_to_string(&mut d, &ctx, 80, 24).contains("420 ms"));

        // Le suivant part après ~300 ms, pas avant.
        assert!(matches!(d.tick_at(&mut ctx, t + Duration::from_millis(100)), Nav::Stay));
        assert!(matches!(d.phase, Phase::Pause { .. }));
        assert!(matches!(d.tick_at(&mut ctx, t + Duration::from_millis(310)), Nav::Stay));
        assert!(matches!(d.phase, Phase::Listen));
    }

    #[test]
    fn early_answer_has_zero_reaction_time() {
        let (mut d, mut ctx, c) = started(false);
        let t = d.sound_end - Duration::from_millis(10);
        d.key_at(key(c), &mut ctx, t);
        assert_eq!(d.last_rt, Some(Duration::ZERO));
    }

    #[test]
    fn wrong_answer_requires_correction() {
        let (mut d, mut ctx, c) = started(false);
        let w = other(&ctx, c);
        let t = d.sound_end + Duration::from_millis(500);
        assert!(matches!(d.key_at(key(w), &mut ctx, t), Nav::Stay));
        assert!(matches!(d.phase, Phase::Fix { .. }));
        let st = ctx.stats.char(c).unwrap();
        assert_eq!((st.seen, st.correct), (1, 0));
        assert_eq!(ctx.stats.top_confusions(1), vec![(c, w, 1)]);
        assert!(!ctx.audio.is_idle(), "le son est rejoué");
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(s.contains("C'était") && (s.contains('−') || s.contains('·')));

        // Sans le réglage, pas de motif.
        ctx.cfg.show_pattern_on_error = false;
        let s = render_to_string(&mut d, &ctx, 80, 24);
        assert!(!s.contains('−') && !s.contains('·'));

        // Autre mauvaise touche, espace, touche hors table : toujours bloqué.
        for k in [w, ' ', '#'] {
            d.key_at(key(k), &mut ctx, t + Duration::from_millis(200));
            assert!(matches!(d.phase, Phase::Fix { .. }));
        }
        // La frappe de correction débloque sans être enregistrée.
        d.key_at(key(c), &mut ctx, t + Duration::from_millis(900));
        assert!(matches!(d.phase, Phase::Pause { .. }));
        assert_eq!(ctx.stats.char(c).unwrap().seen, 1);
        assert_eq!((d.total, d.correct, d.streak), (1, 0, 0));
        assert_eq!(d.misses, vec![(c, 1)]);
    }

    #[test]
    fn missed_char_comes_back_soon() {
        let (mut d, mut ctx, c) = started(true);
        let w = other(&ctx, c);
        let mut t = d.sound_end + Duration::from_millis(300);
        d.key_at(key(w), &mut ctx, t);
        d.key_at(key(c), &mut ctx, t);
        let mut seen = Vec::new();
        for _ in 0..3 {
            let cur = d.current.unwrap();
            seen.push(cur);
            t = d.sound_end + Duration::from_millis(300);
            d.key_at(key(cur), &mut ctx, t);
        }
        assert!(seen.contains(&c), "{c} doit revenir en 3 tirages : {seen:?}");
        assert!(d.retry.is_empty());
    }

    #[test]
    fn space_replays_and_unknown_keys_are_ignored() {
        let (mut d, mut ctx, c) = started(false);
        let before = d.sound_end;
        let now = Instant::now();
        d.key_at(key(' '), &mut ctx, now);
        d.key_at(key('#'), &mut ctx, now);
        d.key_at(KeyEvent::from(KeyCode::Enter), &mut ctx, now);
        d.key_at(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL), &mut ctx, now);
        assert_eq!(d.total, 0);
        assert!(ctx.stats.chars.is_empty());
        assert!(matches!(d.phase, Phase::Listen));
        assert_eq!(d.current, Some(c));
        assert!(d.sound_end > before, "le temps de réaction repart de la fin du son rejoué");
    }

    #[test]
    fn timed_session_ends_after_current_answer() {
        let (mut d, mut ctx, c) = started(false);
        let late = d.started.unwrap() + Duration::from_secs(ctx.cfg.session_secs as u64 + 1);
        // Temps écoulé mais réponse en attente : on attend.
        assert!(matches!(d.tick_at(&mut ctx, late), Nav::Stay));
        assert!(matches!(d.key_at(key(c), &mut ctx, late), Nav::Stay));
        let nav = d.tick_at(&mut ctx, late + NEXT_DELAY);
        assert!(matches!(nav, Nav::Replace(_)));
        let s = ctx.stats.sessions.last().unwrap();
        assert_eq!((s.mode.as_str(), s.total, s.correct), ("Lettres", 1, 1));
        assert!(ctx.audio.is_idle());
    }

    #[test]
    fn escape_finishes_or_goes_back() {
        // Rien de joué : simple retour, aucune session enregistrée.
        let mut ctx = Ctx::test();
        let mut d = Drill::new(false);
        assert!(matches!(d.key_at(KeyEvent::from(KeyCode::Esc), &mut ctx, Instant::now()), Nav::Back));
        assert!(ctx.stats.sessions.is_empty());

        let (mut d, mut ctx, c) = started(true);
        let w = other(&ctx, c);
        let t = d.sound_end + Duration::from_millis(200);
        d.key_at(key(w), &mut ctx, t);
        let nav = d.key_at(KeyEvent::from(KeyCode::Esc), &mut ctx, t);
        assert!(matches!(nav, Nav::Replace(_)));
        let s = ctx.stats.sessions.last().unwrap();
        assert_eq!((s.mode.as_str(), s.total, s.correct), ("Lettres sans fin", 1, 0));
    }

    #[test]
    fn endless_has_no_delay_and_shows_average() {
        let (mut d, mut ctx, c) = started(true);
        let t = d.sound_end + Duration::from_millis(300);
        d.key_at(key(c), &mut ctx, t);
        assert!(matches!(d.phase, Phase::Listen), "le suivant part dès la réponse");
        let c2 = d.current.unwrap();
        d.key_at(key(c2), &mut ctx, d.sound_end + Duration::from_millis(500));
        assert_eq!(d.avg_rt(), Some(Duration::from_millis(400)));
        let s = render_to_string(&mut d, &ctx, 100, 24);
        assert!(s.contains("Moyenne") && s.contains("400 ms") && s.contains("Écoulé"));
        // Jamais de fin automatique.
        let far = Instant::now() + Duration::from_secs(36_000);
        assert!(matches!(d.tick_at(&mut ctx, far), Nav::Stay));
    }

    #[test]
    fn renders_in_every_phase_and_size() {
        let sizes = [(80, 24), (20, 6), (1, 1)];
        for endless in [false, true] {
            let mut ctx = Ctx::test();
            let mut d = Drill::new(endless);
            for (w, h) in sizes {
                render_to_string(&mut d, &ctx, w, h);
            }
            d.key_at(key('x'), &mut ctx, Instant::now());
            // Assez de réponses pour remplir l'historique, avec des erreurs.
            for i in 0..30 {
                let c = d.current.unwrap();
                let t = d.sound_end + Duration::from_millis(250);
                let k = if i % 4 == 0 { other(&ctx, c) } else { c };
                d.key_at(key(k), &mut ctx, t);
                for (w, h) in sizes {
                    render_to_string(&mut d, &ctx, w, h);
                }
                d.key_at(key(c), &mut ctx, t);
                d.tick_at(&mut ctx, t + NEXT_DELAY);
                for (w, h) in sizes {
                    render_to_string(&mut d, &ctx, w, h);
                }
            }
            assert_eq!(d.history.len(), HISTORY);
            assert!(render_to_string(&mut d, &ctx, 80, 24).contains("Précision"));
        }
    }
}
