//! Émission : l'utilisateur manipule au clavier, l'appli décode son rythme.
//!
//! Deux façons de manipuler, utilisables en même temps :
//! - manipulateur droit (Espace maintenu), seulement si le terminal signale le
//!   relâchement des touches (protocole clavier étendu « kitty ») ;
//! - deux touches (`.`/`j`/← pour le point, `-`/`k`/→ pour le trait), partout.

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::morse::{self, Seg};
use crate::picker;
use crate::ui::{self, bold, dim};
use crossterm::event::{
    KeyCode, KeyEvent, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

// ───────────────────────── Décodeur de rythme (pur) ─────────────────────────

/// Silence (en points) qui termine un caractère.
const CHAR_GAP_UNITS: f32 = 3.0;
/// Silence (en points) qui ajoute un espace.
const WORD_GAP_UNITS: f32 = 7.0;
/// Un appui plus long que ce nombre de points est un trait.
const DASH_THRESHOLD_UNITS: f32 = 2.0;
/// Bornes de l'adaptation au rythme réel, relatives au point nominal.
const ADAPT_MIN: f32 = 0.5;
const ADAPT_MAX: f32 = 2.5;
/// Poids d'un nouvel appui dans l'estimation du point.
const ADAPT_RATE: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Elem {
    Dot,
    Dash,
}

impl Elem {
    fn symbol(self) -> char {
        match self {
            Elem::Dot => '.',
            Elem::Dash => '-',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoded {
    /// Caractère terminé ; `ch` vaut `None` si le motif n'existe pas.
    Char { ch: Option<char>, pattern: String },
    /// Silence long : espace entre deux mots.
    Space,
}

/// Transforme des instants d'appui/relâchement en éléments, puis en caractères.
/// Aucune horloge interne : tous les instants sont fournis par l'appelant.
pub struct Decoder {
    /// Durée nominale du point.
    base: Duration,
    /// Durée du point estimée d'après le rythme réel (manipulateur droit).
    unit: Duration,
    /// Unité servant à mesurer le silence en cours.
    gap_unit: Duration,
    pattern: String,
    down: Option<Instant>,
    /// Fin (audible) du dernier élément.
    last_end: Option<Instant>,
    space_due: bool,
}

impl Decoder {
    pub fn new(dit: Duration) -> Self {
        let dit = dit.max(Duration::from_millis(1));
        Decoder {
            base: dit,
            unit: dit,
            gap_unit: dit,
            pattern: String::new(),
            down: None,
            last_end: None,
            space_due: false,
        }
    }

    /// Durée du point actuellement estimée.
    pub fn unit(&self) -> Duration {
        self.unit
    }

    /// Motif en cours de saisie (`.` et `-`).
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Instant du début de l'appui en cours, s'il y en a un.
    pub fn down_since(&self) -> Option<Instant> {
        self.down
    }

    /// Manipulateur enfoncé. Sans effet s'il l'est déjà (répétitions clavier).
    pub fn key_down(&mut self, now: Instant) {
        if self.down.is_none() {
            self.down = Some(now);
        }
    }

    /// Manipulateur relâché : classe l'appui en point ou trait.
    pub fn key_up(&mut self, now: Instant) -> Option<Elem> {
        let start = self.down.take()?;
        let held = now.saturating_duration_since(start);
        let elem = if held.as_secs_f32() < DASH_THRESHOLD_UNITS * self.unit.as_secs_f32() {
            Elem::Dot
        } else {
            Elem::Dash
        };
        // Adaptation légère : le point estimé glisse vers le rythme observé.
        let base = self.base.as_secs_f32();
        let sample = match elem {
            Elem::Dot => held.as_secs_f32(),
            Elem::Dash => held.as_secs_f32() / 3.0,
        }
        .clamp(ADAPT_MIN * base, ADAPT_MAX * base);
        let unit = (1.0 - ADAPT_RATE) * self.unit.as_secs_f32() + ADAPT_RATE * sample;
        self.unit = Duration::from_secs_f32(unit.clamp(ADAPT_MIN * base, ADAPT_MAX * base));
        self.gap_unit = self.unit;
        self.pattern.push(elem.symbol());
        self.last_end = Some(now);
        Some(elem)
    }

    /// Abandonne l'appui en cours sans produire d'élément.
    pub fn cancel_key(&mut self) {
        self.down = None;
    }

    /// Ajoute un élément déjà classé (mode deux touches) dont le son se termine
    /// à `end` (éventuellement dans le futur). Les silences se mesurent alors
    /// avec le point nominal.
    pub fn push(&mut self, elem: Elem, end: Instant) {
        self.pattern.push(elem.symbol());
        self.last_end = Some(end);
        self.gap_unit = self.base;
    }

    /// À appeler régulièrement : renvoie un caractère terminé ou un espace
    /// quand le silence depuis le dernier élément est assez long.
    pub fn poll(&mut self, now: Instant) -> Option<Decoded> {
        if self.down.is_some() {
            return None;
        }
        let silence = now.saturating_duration_since(self.last_end?).as_secs_f32();
        let unit = self.gap_unit.as_secs_f32();
        if !self.pattern.is_empty() {
            if silence >= CHAR_GAP_UNITS * unit {
                let pattern = std::mem::take(&mut self.pattern);
                self.space_due = true;
                return Some(Decoded::Char { ch: morse::decode(&pattern), pattern });
            }
        } else if self.space_due && silence >= WORD_GAP_UNITS * unit {
            self.space_due = false;
            return Some(Decoded::Space);
        }
        None
    }

    /// Oublie la saisie en cours (le rythme estimé est conservé).
    pub fn reset(&mut self) {
        self.pattern.clear();
        self.down = None;
        self.last_end = None;
        self.space_due = false;
    }
}

/// `.-` -> `· −`.
fn pretty(pattern: &str) -> String {
    pattern
        .chars()
        .map(|s| if s == '.' { "·" } else { "−" })
        .collect::<Vec<_>>()
        .join(" ")
}

// ───────────────────────────────── Écran ─────────────────────────────────

/// Nombre de cibles d'un seul caractère avant de passer aux groupes.
const SINGLE_ROUNDS: u32 = 5;
/// Pause avant la cible suivante (plus longue après une erreur, pour lire).
const NEXT_DELAY: Duration = Duration::from_millis(1200);
const NEXT_DELAY_ERR: Duration = Duration::from_millis(2500);
/// Au-delà, un appui est considéré comme bloqué (relâchement perdu).
const STUCK_AFTER: Duration = Duration::from_secs(2);
/// File audio maximale en mode deux touches (les appuis en trop sont ignorés).
const MAX_QUEUE: Duration = Duration::from_millis(1500);
const FREE_TEXT_MAX: usize = 600;

struct Miss {
    expected: char,
    got: Option<char>,
    pattern: String,
}

pub struct SendMode {
    /// Le terminal signale-t-il les relâchements ? `None` : pas encore testé.
    releases: Option<bool>,
    /// Vrai si nous avons activé le protocole clavier étendu (à désactiver).
    pushed: bool,
    free: bool,
    dec: Decoder,
    target: Vec<char>,
    /// Caractères décodés pour la cible en cours : (reçu, juste ?).
    results: Vec<(char, bool)>,
    rounds: u32,
    next_at: Option<Instant>,
    last_miss: Option<Miss>,
    total: u32,
    correct: u32,
    misses: Vec<(char, u32)>,
    free_text: String,
    started: Option<Instant>,
    /// Appui bloqué coupé d'office : ignorer Espace jusqu'au relâchement.
    muted: bool,
    last_space: Option<Instant>,
}

impl SendMode {
    pub fn new() -> Self {
        SendMode {
            releases: None,
            pushed: false,
            free: false,
            dec: Decoder::new(Duration::from_millis(60)),
            target: Vec::new(),
            results: Vec::new(),
            rounds: 0,
            next_at: None,
            last_miss: None,
            total: 0,
            correct: 0,
            misses: Vec::new(),
            free_text: String::new(),
            started: None,
            muted: false,
            last_space: None,
        }
    }

    /// Pour les tests : force la prise en charge (ou non) des relâchements.
    #[cfg(test)]
    fn with_releases(releases: bool) -> Self {
        let mut s = SendMode::new();
        s.releases = Some(releases);
        s
    }

    fn straight(&self) -> bool {
        self.releases == Some(true)
    }

    fn new_target(&mut self, ctx: &Ctx) {
        let chars = ctx.chars();
        let mut rng = rand::rng();
        self.results.clear();
        self.next_at = None;
        if chars.is_empty() {
            self.target.clear();
            return;
        }
        if self.rounds < SINGLE_ROUNDS {
            let avoid = self.target.first().copied();
            self.target = vec![picker::pick(&chars, &ctx.stats, avoid, &mut rng)];
        } else {
            use rand::Rng;
            let len = rng.random_range(2..=4);
            self.target = picker::group(&chars, &ctx.stats, len, &mut rng).chars().collect();
        }
        self.rounds += 1;
    }

    fn handle(&mut self, d: Decoded, ctx: &Ctx, now: Instant) {
        if self.free {
            match d {
                Decoded::Char { ch, .. } => self.free_text.push(ch.unwrap_or('?')),
                Decoded::Space => {
                    if !self.free_text.is_empty() && !self.free_text.ends_with(' ') {
                        self.free_text.push(' ');
                    }
                }
            }
            let n = self.free_text.chars().count();
            if n > FREE_TEXT_MAX {
                self.free_text = self.free_text.chars().skip(n - FREE_TEXT_MAX).collect();
            }
            return;
        }
        let Decoded::Char { ch, pattern } = d else { return };
        let Some(&expected) = self.target.get(self.results.len()) else { return };
        let ok = ch == Some(expected);
        self.results.push((ch.unwrap_or('?'), ok));
        self.total += 1;
        if ok {
            self.correct += 1;
        } else {
            match self.misses.iter_mut().find(|(c, _)| *c == expected) {
                Some(m) => m.1 += 1,
                None => self.misses.push((expected, 1)),
            }
            self.last_miss = Some(Miss { expected, got: ch, pattern });
        }
        if self.results.len() >= self.target.len() {
            let had_error = self.results.iter().any(|(_, ok)| !ok);
            let delay = if had_error && ctx.cfg.show_pattern_on_error { NEXT_DELAY_ERR } else { NEXT_DELAY };
            self.next_at = Some(now + delay);
        }
    }

    fn element(&mut self, elem: Elem, ctx: &Ctx, now: Instant) {
        if ctx.audio.pending() > MAX_QUEUE {
            return;
        }
        let dit = ctx.timing().dit();
        let secs = match elem {
            Elem::Dot => dit,
            Elem::Dash => 3.0 * dit,
        };
        let span = ctx
            .audio
            .play_segments(&[Seg { on: true, secs }, Seg { on: false, secs: dit }]);
        // Ne jamais dater la fin d'un élément avant l'appui lui-même.
        let end = (span.start + Duration::from_secs_f32(secs)).max(now);
        self.dec.push(elem, end);
        self.started.get_or_insert(now);
    }

    fn key(&mut self, key: KeyEvent, ctx: &mut Ctx, now: Instant) -> Nav {
        match key.code {
            KeyCode::Esc => {
                ctx.audio.set_key(false);
                if self.total == 0 {
                    return Nav::Back;
                }
                let mut notes = Vec::new();
                if self.straight() {
                    notes.push(format!(
                        "Ton rythme : point d'environ {} ms",
                        self.dec.unit().as_millis()
                    ));
                }
                return ctx.finish(Summary {
                    mode: "Émission".into(),
                    elapsed: self.started.map(|s| now.saturating_duration_since(s)).unwrap_or_default(),
                    total: self.total,
                    correct: self.correct,
                    timing: ctx.timing(),
                    can_unlock: false,
                    misses: std::mem::take(&mut self.misses),
                    notes,
                });
            }
            KeyCode::Tab => {
                self.free = !self.free;
                self.dec.reset();
                ctx.audio.set_key(false);
                ctx.audio.stop();
                if !self.free && (self.target.is_empty() || self.results.len() >= self.target.len()) {
                    self.new_target(ctx);
                }
            }
            KeyCode::Enter if !self.free => {
                self.dec.reset();
                ctx.audio.set_key(false);
                ctx.audio.stop();
                self.new_target(ctx);
            }
            KeyCode::Backspace | KeyCode::Delete => {
                self.dec.reset();
                ctx.audio.set_key(false);
                ctx.audio.stop();
                if self.free {
                    self.free_text.clear();
                }
            }
            KeyCode::Char(' ') if self.straight() => {
                let prev = self.last_space.replace(now);
                if self.muted {
                    // Les répétitions clavier arrivent comme des appuis : on ne
                    // réarme qu'après un vrai silence (ou un relâchement).
                    let quiet = prev.is_none_or(|p| now.saturating_duration_since(p) > Duration::from_secs(1));
                    if !quiet {
                        return Nav::Stay;
                    }
                    self.muted = false;
                }
                if self.dec.down_since().is_none() {
                    self.dec.key_down(now);
                    ctx.audio.set_key(true);
                }
            }
            KeyCode::Char('.') | KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Left => {
                self.element(Elem::Dot, ctx, now)
            }
            KeyCode::Char('-') | KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Right => {
                self.element(Elem::Dash, ctx, now)
            }
            _ => {}
        }
        Nav::Stay
    }

    fn release(&mut self, key: KeyEvent, ctx: &mut Ctx, now: Instant) {
        if key.code != KeyCode::Char(' ') {
            return;
        }
        ctx.audio.set_key(false);
        if self.muted {
            self.muted = false;
            return;
        }
        if self.dec.key_up(now).is_some() {
            self.started.get_or_insert(now);
        }
    }

    fn tick(&mut self, ctx: &mut Ctx, now: Instant) {
        if let Some(since) = self.dec.down_since() {
            if now.saturating_duration_since(since) > STUCK_AFTER {
                self.dec.cancel_key();
                ctx.audio.set_key(false);
                self.muted = true;
                self.last_space = Some(now);
            }
        }
        while let Some(d) = self.dec.poll(now) {
            self.handle(d, ctx, now);
        }
        if !self.free && self.next_at.is_some_and(|t| now >= t) {
            self.dec.reset();
            self.new_target(ctx);
        }
    }
}

/// Le terminal signale-t-il les relâchements de touche ?
fn probe_releases() -> bool {
    if cfg!(test) || !io::stdout().is_terminal() {
        return false;
    }
    matches!(crossterm::terminal::supports_keyboard_enhancement(), Ok(true))
}

impl Screen for SendMode {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
        ctx.audio.set_key(false);
        ctx.apply_audio();
        let unit = self.dec.unit();
        self.dec = Decoder::new(Duration::from_secs_f32(ctx.timing().dit()));
        if self.started.is_some() {
            // Retour sur l'écran : garder le rythme déjà estimé si possible.
            let base = self.dec.base.as_secs_f32();
            self.dec.unit = Duration::from_secs_f32(
                unit.as_secs_f32().clamp(ADAPT_MIN * base, ADAPT_MAX * base),
            );
        }
        if self.releases.is_none() {
            self.releases = Some(probe_releases());
        }
        if self.straight() && !self.pushed && !cfg!(test) {
            let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
            if crossterm::execute!(io::stdout(), PushKeyboardEnhancementFlags(flags)).is_ok() {
                self.pushed = true;
            } else {
                self.releases = Some(false);
            }
        }
        if self.target.is_empty() {
            self.new_target(ctx);
        }
    }

    fn on_leave(&mut self, ctx: &mut Ctx) {
        ctx.audio.set_key(false);
        ctx.audio.stop();
        self.dec.cancel_key();
        self.pop_flags();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        self.key(key, ctx, Instant::now())
    }

    fn on_key_release(&mut self, key: KeyEvent, ctx: &mut Ctx) {
        self.release(key, ctx, Instant::now());
    }

    fn on_tick(&mut self, ctx: &mut Ctx) -> Nav {
        self.tick(ctx, Instant::now());
        Nav::Stay
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let keys = if self.straight() { "Espace . -" } else { ". -" };
        let mut hints = vec![(keys, "manipuler"), ("Tab", if self.free { "exercice" } else { "mode libre" })];
        if self.free {
            hints.push(("Retour arr.", "effacer"));
        } else {
            hints.push(("Entrée", "passer"));
        }
        hints.push(("Échap", "terminer"));
        let title = if self.free { "Émission — mode libre" } else { "Émission — exercice" };
        let area = ui::frame(f, title, &hints);

        let mut lines: Vec<Line> = Vec::new();
        if self.free {
            lines.push(Line::styled("Manipule ce que tu veux : tout est décodé ici.", dim()));
            lines.push(Line::raw(""));
            // Ne garder que la fin du texte si la place manque.
            let room = (area.width as usize).max(1) * 4;
            let n = self.free_text.chars().count();
            let tail: String = self.free_text.chars().skip(n.saturating_sub(room)).collect();
            if tail.is_empty() {
                lines.push(Line::styled("…", dim()));
            } else {
                lines.push(Line::styled(tail, bold(Color::White)));
            }
        } else {
            lines.push(Line::styled("Émets le texte affiché.", dim()));
            lines.push(Line::raw(""));
            let pos = self.results.len();
            let mut top = vec![Span::styled("À émettre  ", dim())];
            let mut bottom = vec![Span::styled("Décodé     ", dim())];
            for (i, c) in self.target.iter().enumerate() {
                let style = match self.results.get(i) {
                    Some((_, true)) => bold(ui::OK),
                    Some((_, false)) => bold(ui::ERR),
                    None if i == pos => bold(ui::ACCENT).add_modifier(Modifier::UNDERLINED),
                    None => bold(Color::White),
                };
                top.push(Span::styled(c.to_string(), style));
                top.push(Span::raw("  "));
                match self.results.get(i) {
                    Some((got, ok)) => {
                        let color = if *ok { ui::OK } else { ui::ERR };
                        bottom.push(Span::styled(got.to_string(), Style::default().fg(color)));
                    }
                    None => bottom.push(Span::styled("·", dim())),
                }
                bottom.push(Span::raw("  "));
            }
            lines.push(Line::from(top));
            lines.push(Line::from(bottom));
        }
        lines.push(Line::raw(""));

        let pat = self.dec.pattern();
        let mut input = vec![Span::styled("Saisie     ", dim())];
        if pat.is_empty() {
            input.push(Span::styled("—", dim()));
        } else {
            input.push(Span::styled(pretty(pat), bold(ui::ACCENT)));
        }
        if self.dec.down_since().is_some() {
            input.push(Span::styled("  ▮", bold(ui::WARN)));
        }
        lines.push(Line::from(input));

        if !self.free {
            match &self.last_miss {
                Some(m) => {
                    let mut spans = vec![Span::styled("Erreur     ", dim())];
                    let got = match m.got {
                        Some(c) => format!("reçu {c}  {}", pretty(&m.pattern)),
                        None => format!("reçu ?  {} (motif inconnu)", pretty(&m.pattern)),
                    };
                    spans.push(Span::styled(got, Style::default().fg(ui::ERR)));
                    if ctx.cfg.show_pattern_on_error {
                        spans.push(Span::styled(
                            format!("   attendu {}  {}", m.expected, morse::pretty(m.expected)),
                            bold(ui::OK),
                        ));
                    } else {
                        spans.push(Span::styled(format!("   attendu {}", m.expected), bold(ui::OK)));
                    }
                    lines.push(Line::from(spans));
                }
                None => lines.push(Line::raw("")),
            }
            lines.push(Line::raw(""));
            let mut score = vec![Span::raw(format!("Justes  {} / {}", self.correct, self.total))];
            if self.total > 0 {
                let acc = self.correct as f32 / self.total as f32;
                score.push(Span::styled(format!("   {:.0} %", acc * 100.0), bold(ui::acc_color(acc))));
            }
            lines.push(Line::from(score));
        }
        lines.push(Line::raw(""));

        let nominal = ctx.timing();
        if self.straight() {
            let unit = self.dec.unit().as_secs_f32();
            lines.push(Line::styled(
                format!(
                    "Espace : appui court = point, appui long = trait. Ton point ≈ {:.0} ms ({:.0} mots/min, réglé à {:.0}).",
                    unit * 1000.0,
                    1.2 / unit,
                    nominal.char_wpm
                ),
                dim(),
            ));
            lines.push(Line::styled("Ou deux touches :  .  j  ←  = point     -  k  →  = trait", dim()));
        } else {
            lines.push(Line::styled(
                "Deux touches :  .  j  ←  = point     -  k  →  = trait",
                dim(),
            ));
            lines.push(Line::styled(
                "Ton terminal ne signale pas le relâchement des touches : pas de manipulateur droit (Espace) ici.",
                dim(),
            ));
        }
        lines.push(Line::styled(
            "Un silence de 3 points termine le caractère, 7 points ajoutent un espace.",
            dim(),
        ));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

impl SendMode {
    /// Désactive le protocole clavier étendu, uniquement si nous l'avons activé.
    fn pop_flags(&mut self) {
        if self.pushed {
            self.pushed = false;
            let _ = crossterm::execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
    }
}

impl Drop for SendMode {
    fn drop(&mut self) {
        self.pop_flags();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;
    use crossterm::event::KeyModifiers;

    const DIT: Duration = Duration::from_millis(100);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Appuie `held` ms à partir de `*t`, puis avance `*t` de `gap` ms après le relâchement.
    fn tap(d: &mut Decoder, t: &mut Instant, held: u64, gap: u64) -> Option<Elem> {
        d.key_down(*t);
        *t += ms(held);
        let e = d.key_up(*t);
        *t += ms(gap);
        e
    }

    #[test]
    fn classifies_dots_and_dashes() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        assert_eq!(tap(&mut d, &mut t, 100, 100), Some(Elem::Dot));
        assert_eq!(tap(&mut d, &mut t, 300, 100), Some(Elem::Dash));
        assert_eq!(tap(&mut d, &mut t, 150, 100), Some(Elem::Dot));
        assert_eq!(tap(&mut d, &mut t, 260, 100), Some(Elem::Dash));
        assert_eq!(d.pattern(), ".-.-");
        // Relâchement sans appui : rien.
        assert_eq!(d.key_up(t), None);
    }

    #[test]
    fn gap_ends_char_then_adds_space() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        tap(&mut d, &mut t, 100, 100);
        assert_eq!(d.poll(t), None, "un silence d'un point ne termine rien");
        tap(&mut d, &mut t, 300, 0);
        assert_eq!(d.poll(t + ms(200)), None);
        assert_eq!(
            d.poll(t + ms(310)),
            Some(Decoded::Char { ch: Some('A'), pattern: ".-".into() })
        );
        assert_eq!(d.pattern(), "");
        assert_eq!(d.poll(t + ms(600)), None);
        assert_eq!(d.poll(t + ms(710)), Some(Decoded::Space));
        // Un seul espace, même après un long silence.
        assert_eq!(d.poll(t + ms(5000)), None);
    }

    #[test]
    fn no_space_before_first_char_and_unknown_pattern() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        assert_eq!(d.poll(t + ms(10_000)), None);
        for _ in 0..9 {
            tap(&mut d, &mut t, 100, 100);
        }
        assert_eq!(
            d.poll(t + ms(400)),
            Some(Decoded::Char { ch: None, pattern: ".........".into() })
        );
    }

    #[test]
    fn held_key_postpones_char_end() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        tap(&mut d, &mut t, 100, 100);
        d.key_down(t);
        // Répétition clavier pendant l'appui : ignorée.
        d.key_down(t + ms(50));
        assert_eq!(d.poll(t + ms(2000)), None);
        t += ms(300);
        assert_eq!(d.key_up(t), Some(Elem::Dash));
        assert_eq!(d.poll(t + ms(100)), None);
        assert!(matches!(d.poll(t + ms(400)), Some(Decoded::Char { ch: Some('A'), .. })));
    }

    #[test]
    fn two_words() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        let mut out = String::new();
        // « E T », un espace entre les deux, puis « E » collé.
        for (held, gap) in [(100u64, 800u64), (300, 350), (100, 350)] {
            tap(&mut d, &mut t, held, 0);
            let end = t + ms(gap);
            // Sonder toutes les 5 ms comme le fait l'interface.
            while t < end {
                t += ms(5);
                match d.poll(t) {
                    Some(Decoded::Char { ch, .. }) => out.push(ch.unwrap_or('?')),
                    Some(Decoded::Space) => out.push(' '),
                    None => {}
                }
            }
        }
        assert_eq!(out, "E TE");
    }

    #[test]
    fn adapts_slightly_and_stays_bounded() {
        // Opérateur plus lent que le réglage : points de 180 ms, traits de 540 ms.
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        for _ in 0..30 {
            assert_eq!(tap(&mut d, &mut t, 180, 180), Some(Elem::Dot));
            assert_eq!(tap(&mut d, &mut t, 540, 180), Some(Elem::Dash));
        }
        assert!(d.unit() > ms(150) && d.unit() < ms(200), "{:?}", d.unit());

        // Des appuis interminables ne font pas dériver l'estimation sans limite.
        for _ in 0..50 {
            tap(&mut d, &mut t, 5000, 100);
        }
        assert!(d.unit() <= ms(251), "{:?}", d.unit());
        // Ni des appuis minuscules vers zéro.
        for _ in 0..50 {
            tap(&mut d, &mut t, 1, 100);
        }
        assert!(d.unit() >= ms(49), "{:?}", d.unit());
    }

    #[test]
    fn pushed_elements_use_nominal_gaps() {
        let mut d = Decoder::new(DIT);
        let t = Instant::now();
        // Élément dont le son finit dans le futur : rien avant la fin + 3 points.
        d.push(Elem::Dash, t + ms(300));
        d.push(Elem::Dot, t + ms(500));
        assert_eq!(d.poll(t), None);
        assert_eq!(d.poll(t + ms(790)), None);
        assert_eq!(
            d.poll(t + ms(800)),
            Some(Decoded::Char { ch: Some('N'), pattern: "-.".into() })
        );
        assert_eq!(d.poll(t + ms(1200)), Some(Decoded::Space));
    }

    #[test]
    fn reset_and_cancel() {
        let mut d = Decoder::new(DIT);
        let mut t = Instant::now();
        tap(&mut d, &mut t, 100, 0);
        d.key_down(t);
        d.cancel_key();
        assert_eq!(d.key_up(t + ms(100)), None);
        d.reset();
        assert_eq!(d.pattern(), "");
        assert_eq!(d.poll(t + ms(10_000)), None);
    }

    #[test]
    fn enter_and_leave_without_terminal() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::new();
        s.on_enter(&mut ctx);
        assert!(!s.pushed && !s.straight());
        assert_eq!(s.target.len(), 1);
        assert!(ctx.chars().contains(&s.target[0]));
        s.on_leave(&mut ctx);
        // Retour sur l'écran puis sortie : toujours aucun Push/Pop.
        s.on_enter(&mut ctx);
        s.on_leave(&mut ctx);
        assert!(!s.pushed);
    }

    #[test]
    fn two_key_exercise_scores_and_finishes() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::with_releases(false);
        s.on_enter(&mut ctx);
        let t = Instant::now();

        // Échap sans rien avoir émis : simple retour.
        assert!(matches!(s.key(k(KeyCode::Esc), &mut ctx, t), Nav::Back));

        s.target = vec!['A', 'N'];
        s.key(k(KeyCode::Char('.')), &mut ctx, t);
        s.key(k(KeyCode::Right), &mut ctx, t);
        assert_eq!(s.dec.pattern(), ".-");
        // Espace n'est pas un manipulateur sans relâchements.
        s.key(k(KeyCode::Char(' ')), &mut ctx, t);
        assert!(s.dec.down_since().is_none());
        s.tick(&mut ctx, t + ms(5000));
        assert_eq!(s.results, vec![('A', true)]);

        // Mauvais caractère : « E » au lieu de « N ».
        let t = t + ms(6000);
        s.key(k(KeyCode::Char('j')), &mut ctx, t);
        s.tick(&mut ctx, t + ms(5000));
        assert_eq!((s.total, s.correct), (2, 1));
        assert_eq!(s.misses, vec![('N', 1)]);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("attendu N") && out.contains("reçu E"), "{out}");

        // La cible suivante arrive après la pause.
        s.tick(&mut ctx, t + ms(9000));
        assert!(s.results.is_empty() && !s.target.is_empty());

        let nav = s.key(k(KeyCode::Esc), &mut ctx, t + ms(10_000));
        assert!(matches!(nav, Nav::Replace(_)));
        let last = ctx.stats.sessions.last().unwrap();
        assert_eq!((last.mode.as_str(), last.total, last.correct), ("Émission", 2, 1));
        assert!(ctx.stats.chars.is_empty(), "l'émission ne touche pas aux stats de réception");
        assert_eq!(ctx.stats.unlocked(ctx.cfg.track), ctx.cfg.track.initial());
    }

    #[test]
    fn straight_key_with_releases() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::with_releases(true);
        s.on_enter(&mut ctx);
        assert!(!s.pushed, "aucune séquence n'est écrite pendant les tests");
        s.target = vec!['N'];
        let dit = Duration::from_secs_f32(ctx.timing().dit());
        let mut t = Instant::now();
        let space = k(KeyCode::Char(' '));

        s.key(space, &mut ctx, t);
        // Répétition clavier reçue comme un appui : ignorée.
        s.key(space, &mut ctx, t + dit);
        t += dit * 3;
        s.release(space, &mut ctx, t);
        t += dit;
        s.key(space, &mut ctx, t);
        t += dit;
        s.release(space, &mut ctx, t);
        assert_eq!(s.dec.pattern(), "-.");
        s.tick(&mut ctx, t + dit * 4);
        assert_eq!(s.results, vec![('N', true)]);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Espace"));
    }

    #[test]
    fn stuck_key_is_released() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::with_releases(true);
        s.on_enter(&mut ctx);
        let t = Instant::now();
        let space = k(KeyCode::Char(' '));
        s.key(space, &mut ctx, t);
        s.tick(&mut ctx, t + ms(2500));
        assert!(s.dec.down_since().is_none() && s.muted);
        assert_eq!(s.dec.pattern(), "");
        // Les répétitions qui suivent restent ignorées...
        s.key(space, &mut ctx, t + ms(2530));
        assert!(s.dec.down_since().is_none());
        // ... jusqu'au relâchement.
        s.release(space, &mut ctx, t + ms(2600));
        s.key(space, &mut ctx, t + ms(2700));
        assert!(s.dec.down_since().is_some());
    }

    #[test]
    fn free_mode_decodes_everything() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::with_releases(false);
        s.on_enter(&mut ctx);
        let mut t = Instant::now();
        s.key(k(KeyCode::Tab), &mut ctx, t);
        assert!(s.free);
        // « T », espace, puis un motif inconnu.
        s.key(k(KeyCode::Char('-')), &mut ctx, t);
        t += ms(5000);
        s.tick(&mut ctx, t);
        t += ms(5000);
        s.tick(&mut ctx, t);
        for _ in 0..7 {
            s.key(k(KeyCode::Char('.')), &mut ctx, t);
            t += ms(1);
        }
        s.tick(&mut ctx, t + ms(20_000));
        assert_eq!(s.free_text.trim_end(), "T ?");
        assert_eq!(s.total, 0);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("T ?") && out.contains("mode libre"), "{out}");
        s.key(k(KeyCode::Backspace), &mut ctx, t);
        assert!(s.free_text.is_empty());
        s.key(k(KeyCode::Tab), &mut ctx, t);
        assert!(!s.free);
        assert!(matches!(s.key(k(KeyCode::Esc), &mut ctx, t), Nav::Back));
    }

    #[test]
    fn groups_after_first_rounds() {
        let mut ctx = Ctx::test();
        let mut s = SendMode::with_releases(false);
        s.on_enter(&mut ctx);
        for _ in 0..SINGLE_ROUNDS - 1 {
            assert_eq!(s.target.len(), 1);
            s.key(k(KeyCode::Enter), &mut ctx, Instant::now());
        }
        s.key(k(KeyCode::Enter), &mut ctx, Instant::now());
        assert!((2..=4).contains(&s.target.len()));
    }

    #[test]
    fn renders_at_any_size() {
        let mut ctx = Ctx::test();
        for releases in [false, true] {
            let mut s = SendMode::with_releases(releases);
            // Avant on_enter (pas de cible) puis après, dans les deux modes.
            for (w, h) in [(80, 24), (20, 6), (1, 1)] {
                render_to_string(&mut s, &ctx, w, h);
            }
            s.on_enter(&mut ctx);
            for _ in 0..2 {
                for (w, h) in [(80, 24), (20, 6), (1, 1), (200, 60)] {
                    render_to_string(&mut s, &ctx, w, h);
                }
                s.key(k(KeyCode::Tab), &mut ctx, Instant::now());
            }
            s.on_leave(&mut ctx);
        }
        let mut s = SendMode::with_releases(false);
        s.on_enter(&mut ctx);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Émission") && out.contains("À émettre"), "{out}");
    }
}
