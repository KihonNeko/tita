//! Flux — flux continu qui n'attend pas, vitesse auto-adaptative.

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::audio::PlaySpan;
use crate::data;
use crate::i18n::t;
use crate::morse::{self, Timing};
use crate::picker;
use crate::stats::Stats;
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use rand::Rng;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Avance d'audio gardée en file.
const LOOKAHEAD: Duration = Duration::from_millis(400);
/// Taille du bandeau et de la fenêtre glissante de précision.
const WINDOW: usize = 30;
/// Nombre de caractères à attendre entre deux changements de vitesse.
const COOLDOWN: u32 = 15;
const MIN_WPM: f32 = 4.0;
/// Durée d'affichage de la flèche après un changement de vitesse.
const ARROW_SECS: u64 = 4;
/// Il faut au moins ce nombre de mots disponibles pour en mélanger au flux.
const MIN_WORDS: usize = 10;

// ---------------------------------------------------------------------------
// Attribution frappe ↔ caractère

#[derive(Clone, Copy, Debug)]
struct Sent {
    seq: u64,
    c: char,
    span: PlaySpan,
    /// Fin du 2e caractère suivant : passé cet instant, c'est raté.
    deadline: Option<Instant>,
}

/// Caractères envoyés qui attendent encore une réponse, du plus ancien au
/// plus récent.
#[derive(Default)]
struct Tracker {
    waiting: VecDeque<Sent>,
    seq: u64,
}

impl Tracker {
    fn push(&mut self, c: char, span: PlaySpan) {
        for w in &mut self.waiting {
            if w.seq + 2 == self.seq {
                w.deadline = Some(span.end);
            }
        }
        self.waiting.push_back(Sent { seq: self.seq, c, span, deadline: None });
        self.seq += 1;
    }

    /// Retire et renvoie les caractères restés sans réponse alors que le 2e
    /// caractère suivant a fini de jouer.
    fn expire(&mut self, now: Instant) -> Vec<char> {
        let mut out = Vec::new();
        while self.waiting.front().is_some_and(|w| w.deadline.is_some_and(|d| d <= now)) {
            out.push(self.waiting.pop_front().unwrap().c);
        }
        out
    }

    /// Attribue une frappe au plus ancien caractère déjà commencé et sans
    /// réponse. Renvoie ce caractère et le temps de réaction (depuis la fin du
    /// son, borné à 0), ou `None` si aucun caractère n'attend.
    fn answer(&mut self, now: Instant) -> Option<(char, Duration)> {
        let w = *self.waiting.front()?;
        if w.span.start > now {
            return None;
        }
        self.waiting.pop_front();
        Some((w.c, now.saturating_duration_since(w.span.end)))
    }
}

// ---------------------------------------------------------------------------
// Adaptation de la vitesse

struct Adapt {
    eff: f32,
    /// Plafond : la vitesse des caractères.
    max: f32,
    peak: f32,
    window: VecDeque<bool>,
    since_change: u32,
}

impl Adapt {
    fn new(eff: f32, char_wpm: f32) -> Self {
        let eff = eff.min(char_wpm);
        Adapt { eff, max: char_wpm, peak: eff, window: VecDeque::new(), since_change: 0 }
    }

    fn accuracy(&self) -> Option<f32> {
        if self.window.is_empty() {
            return None;
        }
        Some(self.window.iter().filter(|b| **b).count() as f32 / self.window.len() as f32)
    }

    /// Enregistre un résultat ; renvoie `Some(+1.0)` / `Some(-1.0)` si la
    /// vitesse effective vient de changer.
    fn push(&mut self, ok: bool) -> Option<f32> {
        self.window.push_back(ok);
        if self.window.len() > WINDOW {
            self.window.pop_front();
        }
        self.since_change += 1;
        if self.since_change < COOLDOWN {
            return None;
        }
        let acc = self.accuracy()?;
        let new = if acc >= 0.90 {
            (self.eff + 1.0).min(self.max)
        } else if acc < 0.75 {
            (self.eff - 1.0).max(MIN_WPM.min(self.eff))
        } else {
            self.eff
        };
        if new == self.eff {
            return None;
        }
        let delta = new - self.eff;
        self.eff = new;
        self.peak = self.peak.max(new);
        self.since_change = 0;
        Some(delta.signum())
    }
}

// ---------------------------------------------------------------------------
// Contenu

/// `s` en majuscules s'il est non vide et entièrement composé de `chars`.
fn usable(s: &str, chars: &[char]) -> Option<String> {
    let up = s.to_uppercase();
    (!up.is_empty() && up.chars().all(|c| chars.contains(&c))).then_some(up)
}

/// Prochain élément du flux (groupe, mot, indicatif ou abréviation), fait
/// uniquement de caractères de `chars` (non vide).
fn next_item(
    chars: &[char],
    words: &[String],
    abbrs: &[String],
    stats: &Stats,
    rng: &mut impl Rng,
) -> String {
    let r: f32 = rng.random();
    if words.len() >= MIN_WORDS && r < 0.5 {
        return words[rng.random_range(0..words.len())].clone();
    }
    // Hors mots : ~15 % d'indicatifs, ~15 % d'abréviations quand c'est possible.
    let r: f32 = rng.random();
    if r < 0.15 {
        for _ in 0..4 {
            if let Some(cs) = usable(&data::callsign(rng), chars) {
                return cs;
            }
        }
    } else if r < 0.30 && !abbrs.is_empty() {
        return abbrs[rng.random_range(0..abbrs.len())].clone();
    }
    let len = rng.random_range(1..=5);
    picker::group(chars, stats, len, rng)
}

// ---------------------------------------------------------------------------
// Écran

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mark {
    Good,
    Wrong,
    Missed,
}

pub struct Flux {
    running: bool,
    started: Instant,
    chars: Vec<char>,
    words: Vec<String>,
    abbrs: Vec<String>,
    /// Caractères restants de l'élément en cours d'envoi.
    item: VecDeque<char>,
    tracker: Tracker,
    adapt: Adapt,
    /// Derniers caractères résolus (jamais ceux encore en attente).
    band: VecDeque<(char, Mark)>,
    total: u32,
    correct: u32,
    streak: u32,
    best_streak: u32,
    misses: Vec<(char, u32)>,
    /// Dernier changement de vitesse : (sens, instant).
    last_change: Option<(f32, Instant)>,
}

impl Flux {
    pub fn new() -> Self {
        Flux {
            running: false,
            started: Instant::now(),
            chars: Vec::new(),
            words: Vec::new(),
            abbrs: Vec::new(),
            item: VecDeque::new(),
            tracker: Tracker::default(),
            adapt: Adapt::new(MIN_WPM, MIN_WPM),
            band: VecDeque::new(),
            total: 0,
            correct: 0,
            streak: 0,
            best_streak: 0,
            misses: Vec::new(),
            last_change: None,
        }
    }

    fn start(&mut self, ctx: &mut Ctx, now: Instant) {
        ctx.audio.stop();
        let t = ctx.timing();
        self.chars = ctx
            .chars()
            .into_iter()
            .map(|c| crate::morse::up(c))
            .filter(|c| morse::pattern(*c).is_some())
            .collect();
        self.words = data::words(ctx.cfg.lang, &self.chars)
            .into_iter()
            .filter_map(|w| usable(w, &self.chars))
            .collect();
        self.abbrs = data::abbreviations()
            .iter()
            .filter_map(|(a, _)| usable(a, &self.chars))
            .collect();
        self.adapt = Adapt::new(t.eff_wpm, t.char_wpm);
        self.started = now;
        self.running = true;
    }

    fn timing(&self) -> Timing {
        Timing::new(self.adapt.max, self.adapt.eff)
    }

    /// Enregistre un caractère résolu : `typed = None` s'il a été raté.
    fn resolve(&mut self, ctx: &mut Ctx, c: char, typed: Option<char>, rt: Option<Duration>, now: Instant) {
        ctx.stats.record(c, typed, rt);
        let ok = typed == Some(c);
        self.total += 1;
        if ok {
            self.correct += 1;
            self.streak += 1;
            self.best_streak = self.best_streak.max(self.streak);
        } else {
            self.streak = 0;
            match self.misses.iter_mut().find(|m| m.0 == c) {
                Some(m) => m.1 += 1,
                None => self.misses.push((c, 1)),
            }
        }
        let mark = match typed {
            _ if ok => Mark::Good,
            Some(_) => Mark::Wrong,
            None => Mark::Missed,
        };
        self.band.push_back((c, mark));
        if self.band.len() > WINDOW {
            self.band.pop_front();
        }
        if let Some(dir) = self.adapt.push(ok) {
            self.last_change = Some((dir, now));
        }
    }

    fn expire(&mut self, ctx: &mut Ctx, now: Instant) {
        for c in self.tracker.expire(now) {
            self.resolve(ctx, c, None, None, now);
        }
    }

    /// Met en file le caractère suivant et le silence qui le suit.
    fn schedule_next(&mut self, ctx: &mut Ctx) {
        if self.item.is_empty() {
            let mut rng = rand::rng();
            self.item =
                next_item(&self.chars, &self.words, &self.abbrs, &ctx.stats, &mut rng).chars().collect();
        }
        let Some(c) = self.item.pop_front() else { return };
        let t = self.timing();
        let span = ctx.audio.play_segments(&morse::char_segments(c, t));
        let gap = if self.item.is_empty() { t.word_gap() } else { t.char_gap() };
        ctx.audio.silence(Duration::from_secs_f32(gap));
        self.tracker.push(c, span);
    }

    fn tick_at(&mut self, ctx: &mut Ctx, now: Instant) -> Nav {
        if !self.running {
            return Nav::Stay;
        }
        if self.chars.is_empty() {
            return Nav::Back;
        }
        self.expire(ctx, now);
        if ctx.audio.pending() < LOOKAHEAD {
            self.schedule_next(ctx);
        }
        Nav::Stay
    }

    fn key_at(&mut self, key: KeyEvent, ctx: &mut Ctx, now: Instant) -> Nav {
        if !self.running {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => self.start(ctx, now),
                KeyCode::Esc | KeyCode::Char('q') => return Nav::Back,
                _ => {}
            }
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Esc => {
                self.expire(ctx, now);
                let s = self.summary(now);
                return ctx.finish(s);
            }
            KeyCode::Char(k) if !k.is_whitespace() && !k.is_control() => {
                self.expire(ctx, now);
                if let Some((c, rt)) = self.tracker.answer(now) {
                    self.resolve(ctx, c, Some(crate::morse::up(k)), Some(rt), now);
                }
            }
            _ => {}
        }
        Nav::Stay
    }

    fn summary(&self, now: Instant) -> Summary {
        Summary {
            mode: t("Flux", "Stream").into(),
            elapsed: now.saturating_duration_since(self.started),
            total: self.total,
            correct: self.correct,
            timing: self.timing(),
            can_unlock: true,
            misses: self.misses.clone(),
            notes: vec![
                crate::tr!(
                    "Vitesse max {:.0} mots/min (effective)",
                    "Top speed   {:.0} wpm (effective)",
                    self.adapt.peak
                ),
                crate::tr!("Série max   {}", "Best streak {}", self.best_streak),
            ],
        }
    }

    fn draw_ready(&self, f: &mut Frame, ctx: &Ctx) {
        let area = ui::frame(
            f,
            t("Flux", "Stream"),
            &[(t("Entrée", "Enter"), t("commencer", "start")), (t("Échap", "Esc"), t("retour", "back"))],
        );
        let chars: String = ctx.chars().iter().map(|c| format!("{c} ")).collect();
        let lines = vec![
            Line::styled(t("Prêt ?", "Ready?"), bold(ui::ACCENT)),
            Line::raw(""),
            Line::raw(t(
                "Le flux ne s'arrête jamais et ne t'attend pas : tape chaque caractère dès que tu l'entends.",
                "The stream never stops and does not wait for you: type each character as soon as you hear it.",
            )),
            Line::raw(t(
                "Ne t'arrête pas sur une lettre ratée, raccroche la suivante.",
                "Don't dwell on a missed letter, catch the next one.",
            )),
            Line::raw(""),
            Line::styled(
                t(
                    "La vitesse monte quand tu suis, redescend quand tu décroches. Les espaces ne se tapent pas.",
                    "The speed goes up when you keep up, comes down when you fall behind. Spaces are not typed.",
                ),
                dim(),
            ),
            Line::raw(""),
            Line::from(vec![Span::styled(t("Caractères  ", "Characters  "), dim()), Span::raw(chars)]),
            Line::from(vec![
                Span::styled(t("Vitesse     ", "Speed       "), dim()),
                Span::raw(crate::tr!(
                    "départ à {:.0} mots/min, jusqu'à {:.0}",
                    "starts at {:.0} wpm, up to {:.0}",
                    ctx.cfg.eff_wpm.min(ctx.cfg.char_wpm),
                    ctx.cfg.char_wpm
                )),
            ]),
        ];
        let h = lines.len() as u16 + 4;
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), ui::centered(area, 64, h));
    }

    fn draw_running(&self, f: &mut Frame) {
        let now = Instant::now();
        let area = ui::frame(f, t("Flux", "Stream"), &[(t("Échap", "Esc"), t("terminer", "finish"))]);

        let mut speed = vec![
            Span::styled(t("Vitesse ", "Speed "), dim()),
            Span::styled(crate::tr!("{:.0} mots/min", "{:.0} wpm", self.adapt.eff), bold(Color::White)),
        ];
        match self.last_change {
            Some((dir, at)) if now.saturating_duration_since(at).as_secs() < ARROW_SECS => {
                speed.push(if dir > 0.0 {
                    Span::styled(" ↑", bold(ui::OK))
                } else {
                    Span::styled(" ↓", bold(ui::WARN))
                });
            }
            _ => {}
        }
        let acc = match self.adapt.accuracy() {
            Some(a) => Span::styled(crate::tr!("{:.0} %", "{:.0}%", a * 100.0), bold(acc_color(a))),
            None => Span::styled("—", dim()),
        };
        let stats = Line::from(vec![
            Span::styled(t("Précision ", "Accuracy "), dim()),
            acc,
            Span::styled(t("   Série ", "   Streak "), dim()),
            Span::styled(self.streak.to_string(), bold(Color::White)),
            Span::styled(
                crate::tr!(
                    "   {}   {} caractères",
                    "   {}   {} characters",
                    ui::mmss(now.saturating_duration_since(self.started).as_secs()),
                    self.total
                ),
                dim(),
            ),
        ]);

        // Bandeau : les plus récents à droite, rogné à la largeur disponible.
        let fit = (area.width as usize / 2).clamp(1, WINDOW);
        let skip = self.band.len().saturating_sub(fit);
        let mut band: Vec<Span> = Vec::new();
        for (c, mark) in self.band.iter().skip(skip) {
            let style = match mark {
                Mark::Good => Style::default().fg(ui::OK),
                Mark::Wrong => bold(ui::ERR),
                Mark::Missed => Style::default().fg(ui::WARN),
            };
            band.push(Span::styled(c.to_string(), style));
            band.push(Span::raw(" "));
        }
        if band.is_empty() {
            band.push(Span::styled("…", dim()));
        }

        let lines = vec![
            Line::from(speed),
            stats,
            Line::raw(""),
            Line::from(band),
            Line::raw(""),
            Line::from(vec![
                Span::styled(t("juste", "right"), Style::default().fg(ui::OK)),
                Span::styled(" · ", dim()),
                Span::styled(t("faux", "wrong"), bold(ui::ERR)),
                Span::styled(" · ", dim()),
                Span::styled(t("raté", "missed"), Style::default().fg(ui::WARN)),
                Span::styled(
                    t("   Ne t'arrête pas, raccroche la suivante.", "   Don't stop, catch the next one."),
                    dim(),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), ui::centered(area, 60, 6));
    }
}

impl Screen for Flux {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

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
        if self.running {
            self.draw_running(f);
        } else {
            self.draw_ready(f, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    /// Fenêtre audio synthétique : de `a` à `b` ms après `t0`.
    fn span(t0: Instant, a: u64, b: u64) -> PlaySpan {
        PlaySpan { start: t0 + ms(a), end: t0 + ms(b) }
    }

    #[test]
    fn key_goes_to_oldest_started_char() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        t.push('A', span(t0, 100, 200));
        t.push('B', span(t0, 500, 600));
        // Rien n'a encore commencé à jouer : frappe ignorée.
        assert_eq!(t.answer(t0 + ms(50)), None);
        // Pendant le son : temps de réaction borné à 0.
        assert_eq!(t.answer(t0 + ms(150)), Some(('A', Duration::ZERO)));
        // B n'a pas commencé : ignorée, B reste en attente.
        assert_eq!(t.answer(t0 + ms(300)), None);
        assert_eq!(t.answer(t0 + ms(850)), Some(('B', ms(250))));
        assert_eq!(t.answer(t0 + ms(900)), None);
    }

    #[test]
    fn late_key_answers_the_older_char_first() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        t.push('A', span(t0, 0, 100));
        t.push('B', span(t0, 400, 500));
        let now = t0 + ms(450);
        assert!(t.expire(now).is_empty());
        assert_eq!(t.answer(now).unwrap().0, 'A');
        assert_eq!(t.answer(now).unwrap().0, 'B');
    }

    #[test]
    fn char_expires_when_second_next_has_played() {
        let t0 = Instant::now();
        let mut t = Tracker::default();
        t.push('A', span(t0, 0, 100));
        t.push('B', span(t0, 400, 500));
        assert!(t.expire(t0 + ms(10_000)).is_empty(), "pas de 2e suivant : pas d'expiration");
        t.push('C', span(t0, 800, 900));
        assert!(t.expire(t0 + ms(899)).is_empty());
        assert_eq!(t.expire(t0 + ms(900)), vec!['A']);
        // B attend la fin de D.
        t.push('D', span(t0, 1200, 1300));
        assert!(t.expire(t0 + ms(1299)).is_empty());
        assert_eq!(t.answer(t0 + ms(1299)).unwrap().0, 'B');
        // C attend la fin de E, même si D est répondu entre-temps.
        t.push('E', span(t0, 1600, 1700));
        assert_eq!(t.expire(t0 + ms(1700)), vec!['C']);
        assert_eq!(t.answer(t0 + ms(1700)).unwrap().0, 'D');
        assert_eq!(t.waiting.len(), 1);
    }

    #[test]
    fn speed_rises_with_cooldown_and_cap() {
        let mut a = Adapt::new(8.0, 10.0);
        for i in 1..COOLDOWN {
            assert_eq!(a.push(true), None, "trop tôt ({i})");
        }
        assert_eq!(a.push(true), Some(1.0));
        assert_eq!(a.eff, 9.0);
        for _ in 1..COOLDOWN {
            assert_eq!(a.push(true), None);
        }
        assert_eq!(a.push(true), Some(1.0));
        assert_eq!(a.eff, 10.0);
        // Plafonné à la vitesse des caractères.
        for _ in 0..60 {
            assert_eq!(a.push(true), None);
        }
        assert_eq!((a.eff, a.peak), (10.0, 10.0));
    }

    #[test]
    fn speed_drops_with_floor_and_holds_in_between() {
        let mut a = Adapt::new(5.0, 20.0);
        for _ in 1..COOLDOWN {
            assert_eq!(a.push(false), None);
        }
        assert_eq!(a.push(false), Some(-1.0));
        assert_eq!(a.eff, 4.0);
        for _ in 0..60 {
            assert_eq!(a.push(false), None);
        }
        assert_eq!((a.eff, a.peak), (4.0, 5.0));

        // Entre 75 % et 90 % : rien ne bouge (4 justes sur 5 = 80 %).
        let mut a = Adapt::new(8.0, 20.0);
        for i in 0..100 {
            assert_eq!(a.push(i % 5 != 0), None);
        }
        assert_eq!(a.eff, 8.0);

        // Une vitesse de départ sous le plancher n'est pas remontée de force.
        let mut a = Adapt::new(3.0, 20.0);
        for _ in 0..40 {
            assert_eq!(a.push(false), None);
        }
        assert_eq!(a.eff, 3.0);
    }

    #[test]
    fn window_keeps_last_30() {
        let mut a = Adapt::new(20.0, 20.0);
        for _ in 0..WINDOW {
            a.push(false);
        }
        for _ in 0..WINDOW {
            a.push(true);
        }
        assert_eq!(a.window.len(), WINDOW);
        assert_eq!(a.accuracy(), Some(1.0));
    }

    #[test]
    fn content_uses_only_allowed_chars() {
        let stats = Stats::default();
        let mut rng = rand::rng();
        let chars = ['E', 'T', 'A'];
        let words: Vec<String> = (0..MIN_WORDS).map(|_| "TETE".to_string()).collect();
        let abbrs = vec!["TEA".to_string()];
        let mut saw_word = false;
        for _ in 0..300 {
            let it = next_item(&chars, &words, &abbrs, &stats, &mut rng);
            assert!(!it.is_empty() && it.chars().all(|c| chars.contains(&c)), "{it}");
            saw_word |= it == "TETE";
            // Listes vides : uniquement des groupes.
            let it = next_item(&chars, &[], &[], &stats, &mut rng);
            assert!((1..=5).contains(&it.len()) && it.chars().all(|c| chars.contains(&c)), "{it}");
        }
        assert!(saw_word);
        assert_eq!(usable("f4abc", &['F', '4', 'A', 'B', 'C']), Some("F4ABC".into()));
        assert_eq!(usable("F4ABC", &['F', 'A', 'B', 'C']), None);
        assert_eq!(usable("", &chars), None);
    }

    /// Démarre le flux et met en file `n` caractères sans attendre le temps
    /// réel (la file audio est vidée avant chaque tic).
    fn started(ctx: &mut Ctx, n: usize) -> Flux {
        let mut fl = Flux::new();
        fl.on_enter(ctx);
        assert!(matches!(fl.on_key(key(KeyCode::Enter), ctx), Nav::Stay));
        // Instant antérieur à toute fin de son : rien n'expire.
        let t0 = fl.started;
        for _ in 0..n {
            ctx.audio.stop();
            assert!(matches!(fl.tick_at(ctx, t0), Nav::Stay));
        }
        fl
    }

    #[test]
    fn right_wrong_and_missed_are_recorded() {
        let mut ctx = Ctx::test();
        let mut fl = started(&mut ctx, 1);
        assert_eq!(fl.tracker.waiting.len(), 1);
        assert!(fl.band.is_empty(), "rien dans le bandeau avant la réponse");
        let first = fl.tracker.waiting[0];

        // Trop tôt : ignorée. Espace : ignoré.
        fl.key_at(key(KeyCode::Char('x')), &mut ctx, first.span.start - ms(1));
        fl.key_at(key(KeyCode::Char(' ')), &mut ctx, first.span.end);
        assert_eq!((fl.total, fl.band.len()), (0, 0));

        // Bonne réponse, en minuscule.
        let k = first.c.to_ascii_lowercase();
        fl.key_at(key(KeyCode::Char(k)), &mut ctx, first.span.end + ms(300));
        assert_eq!((fl.total, fl.correct, fl.streak), (1, 1, 1));
        assert_eq!(fl.band.back(), Some(&(first.c, Mark::Good)));
        assert!(ctx.stats.char(first.c).unwrap().rt_ms > 0.0);
        // Plus rien n'attend : frappe ignorée.
        fl.key_at(key(KeyCode::Char('x')), &mut ctx, first.span.end + ms(400));
        assert_eq!(fl.total, 1);

        // Mauvaise réponse : rien ne s'interrompt.
        ctx.audio.stop();
        fl.tick_at(&mut ctx, fl.started);
        let second = *fl.tracker.waiting.back().unwrap();
        fl.key_at(key(KeyCode::Char('#')), &mut ctx, second.span.end);
        assert_eq!((fl.total, fl.correct, fl.streak), (2, 1, 0));
        assert_eq!(fl.band.back(), Some(&(second.c, Mark::Wrong)));
        assert_eq!(fl.misses, vec![(second.c, 1)]);

        // Sans réponse : raté quand le 2e suivant a fini de jouer.
        for _ in 0..3 {
            ctx.audio.stop();
            fl.tick_at(&mut ctx, fl.started);
        }
        let third = fl.tracker.waiting[0];
        let late = fl.tracker.waiting.iter().map(|w| w.span.end).max().unwrap();
        fl.tick_at(&mut ctx, late);
        assert_eq!((fl.total, fl.correct), (3, 1));
        assert_eq!(fl.band.back(), Some(&(third.c, Mark::Missed)));
        assert_eq!(ctx.stats.chars.values().map(|c| c.seen).sum::<u32>(), 3);
    }

    #[test]
    fn stream_only_uses_unlocked_chars_and_never_stops() {
        let mut ctx = Ctx::test();
        let allowed = ctx.chars();
        let mut fl = started(&mut ctx, 60);
        assert_eq!(fl.tracker.seq, 60);
        assert!(fl.tracker.waiting.iter().all(|w| allowed.contains(&w.c)));
        // Personne ne répond : tout finit raté, la vitesse baisse, le flux continue.
        let late = Instant::now() + Duration::from_secs(60);
        ctx.audio.stop();
        fl.tick_at(&mut ctx, late);
        assert!(fl.total >= 58 && fl.correct == 0);
        assert!(fl.band.len() <= WINDOW);
        assert!(fl.adapt.eff < ctx.cfg.eff_wpm);
        assert_eq!(ctx.cfg.eff_wpm, Ctx::test().cfg.eff_wpm, "la config n'est pas modifiée");
        assert_eq!(fl.tracker.seq, 61);
        let s = render_to_string(&mut fl, &ctx, 80, 24);
        assert!(s.contains('↓') && s.contains("0 %"), "{s}");
    }

    #[test]
    fn escape_finishes_after_one_char() {
        let mut ctx = Ctx::test();
        let mut fl = started(&mut ctx, 1);
        let w = fl.tracker.waiting[0];
        fl.key_at(key(KeyCode::Char(w.c)), &mut ctx, w.span.end);
        assert!(matches!(fl.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        assert!(ctx.audio.is_idle());
        let s = ctx.stats.sessions.last().unwrap();
        assert_eq!((s.mode.as_str(), s.total, s.correct), ("Flux", 1, 1));
    }

    #[test]
    fn escape_without_any_char_goes_back() {
        let mut ctx = Ctx::test();
        assert!(matches!(Flux::new().on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        let mut fl = started(&mut ctx, 2);
        assert!(matches!(fl.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        assert!(ctx.stats.sessions.is_empty());
    }

    #[test]
    fn renders_at_any_size() {
        let mut ctx = Ctx::test();
        let mut fl = Flux::new();
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            render_to_string(&mut fl, &ctx, w, h);
        }
        let s = render_to_string(&mut fl, &ctx, 80, 24);
        assert!(s.contains("Prêt ?") && s.contains("raccroche la suivante"), "{s}");
        // Un seul caractère en attente : `started` superpose les sons (file
        // vidée entre deux tirages), plusieurs caractères s'expireraient entre eux.
        let mut fl = started(&mut ctx, 1);
        let w = fl.tracker.waiting[0];
        fl.key_at(key(KeyCode::Char(w.c)), &mut ctx, w.span.end);
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            render_to_string(&mut fl, &ctx, w, h);
        }
        let s = render_to_string(&mut fl, &ctx, 80, 24);
        assert!(s.contains("mots/min") && s.contains("100 %") && s.contains("1 caractères"), "{s}");
    }

    #[test]
    fn english_interface() {
        crate::i18n::set(crate::config::Lang::En);
        let sizes = [(80, 24), (20, 6)];
        let mut ctx = Ctx::test();
        let mut fl = Flux::new();
        let s = render_to_string(&mut fl, &ctx, 80, 24);
        assert!(s.contains("Stream") && s.contains("Ready?") && s.contains("catch the next one"), "{s}");
        assert!(s.contains("Spaces are not typed.") && s.contains("starts at") && s.contains("wpm, up to"), "{s}");
        assert!(!s.contains("Flux") && !s.contains("Prêt") && !s.contains("mots/min") && !s.contains("Échap"), "{s}");
        render_to_string(&mut fl, &ctx, 20, 6);

        let mut fl = started(&mut ctx, 1);
        let w = fl.tracker.waiting[0];
        fl.key_at(key(KeyCode::Char(w.c)), &mut ctx, w.span.end);
        let s = render_to_string(&mut fl, &ctx, 80, 24);
        assert!(s.contains("wpm") && s.contains("Accuracy 100%") && s.contains("1 characters"), "{s}");
        assert!(s.contains("right · wrong · missed   Don't stop, catch the next one."), "{s}");
        assert!(!s.contains("Vitesse") && !s.contains("Précision") && !s.contains("raté") && !s.contains("juste"), "{s}");
        for (w, h) in sizes {
            render_to_string(&mut fl, &ctx, w, h);
        }
        let sum = fl.summary(Instant::now());
        assert_eq!(sum.mode, "Stream");
        assert!(sum.notes[0].starts_with("Top speed   ") && sum.notes[0].ends_with("wpm (effective)"));
        assert_eq!(sum.notes[1], "Best streak 1");
        crate::i18n::set(crate::config::Lang::Fr);
    }
}
