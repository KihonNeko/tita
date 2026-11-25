//! Radio : indicatifs, abréviations et codes Q, échange complet simulé.
//!
//! Un sous-menu ouvre soit l'écran de `words.rs` (indicatifs, abréviations),
//! soit l'écran `Qso` défini ici (copie libre d'un échange).

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::audio::PlaySpan;
use crate::modes::words::{align, diff_spans, Kind, Tally, Words};
use crate::ui::{self, bold, dim};
use crate::{data, morse};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::time::Instant;

// ------------------------------------------------------------ disponibilité

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Topic {
    Callsigns,
    Abbreviations,
    Qso,
}

const TOPICS: [Topic; 3] = [Topic::Callsigns, Topic::Abbreviations, Topic::Qso];

impl Topic {
    fn label(self) -> &'static str {
        match self {
            Topic::Callsigns => "Indicatifs",
            Topic::Abbreviations => "Abréviations et codes Q",
            Topic::Qso => "Échange complet",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Topic::Callsigns => "Des indicatifs de stations, à copier un par un.",
            Topic::Abbreviations => "Les abréviations du trafic, avec leur sens à la correction.",
            Topic::Qso => "Un contact simulé, à copier transmission par transmission.",
        }
    }
}

fn alnum() -> impl Iterator<Item = char> {
    ('A'..='Z').chain('0'..='9')
}

/// Caractères dont le contenu a besoin pour être varié.
fn required(topic: Topic) -> Vec<char> {
    let mut v: Vec<char> = match topic {
        Topic::Callsigns => alnum().collect(),
        Topic::Abbreviations => {
            data::abbreviations().iter().flat_map(|(code, _)| code.chars()).collect()
        }
        Topic::Qso => alnum().chain(['=']).collect(),
    };
    v.sort();
    v.dedup();
    v
}

/// Y a-t-il assez de contenu réalisable avec les caractères `allowed` ?
fn ready(topic: Topic, allowed: &[char]) -> bool {
    match topic {
        Topic::Callsigns => {
            let letters = allowed.iter().filter(|c| c.is_ascii_uppercase()).count();
            data::callsign_bases(allowed) >= 4 && letters >= 6
        }
        Topic::Abbreviations => {
            let n = data::abbreviations()
                .iter()
                .filter(|(code, _)| code.chars().all(|c| allowed.contains(&c)))
                .count();
            n >= 8
        }
        Topic::Qso => required(topic).iter().all(|c| allowed.contains(c)),
    }
}

/// Caractères nécessaires pas encore débloqués, dans l'ordre d'apprentissage
/// `order` (ceux qui n'y figurent pas viennent à la fin).
fn missing(topic: Topic, allowed: &[char], order: &[char]) -> Vec<char> {
    let mut v: Vec<char> = required(topic).into_iter().filter(|c| !allowed.contains(c)).collect();
    v.sort_by_key(|c| order.iter().position(|o| o == c).unwrap_or(usize::MAX));
    v
}

// ---------------------------------------------------------------- sous-menu

pub struct Radio {
    sel: usize,
    /// Contenu indisponible : caractères qui manquent pour le sujet choisi.
    gate: Option<Vec<char>>,
}

impl Radio {
    pub fn new() -> Self {
        Radio { sel: 0, gate: None }
    }

    fn open(&self, force: bool) -> Nav {
        let screen: Box<dyn Screen> = match TOPICS[self.sel] {
            Topic::Callsigns => Box::new(Words::with(Kind::Callsigns, force)),
            Topic::Abbreviations => Box::new(Words::with(Kind::Abbreviations, force)),
            Topic::Qso => Box::new(Qso::new(force)),
        };
        Nav::Push(screen)
    }
}

impl Screen for Radio {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if self.gate.is_some() {
            match key.code {
                KeyCode::Enter => {
                    self.gate = None;
                    return self.open(true);
                }
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => self.gate = None,
                _ => {}
            }
            return Nav::Stay;
        }
        let n = TOPICS.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = (self.sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1) % n,
            KeyCode::Enter | KeyCode::Char(' ') => {
                let topic = TOPICS[self.sel];
                let allowed = ctx.chars();
                if ready(topic, &allowed) {
                    return self.open(false);
                }
                self.gate = Some(missing(topic, &allowed, &ctx.cfg.track.order()));
            }
            KeyCode::Esc | KeyCode::Char('q') => return Nav::Back,
            _ => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let topic = TOPICS[self.sel];
        if let Some(miss) = &self.gate {
            let area = ui::frame(
                f,
                &format!("Radio – {}", topic.label()),
                &[("Entrée", "essayer quand même"), ("Échap", "retour")],
            );
            let mut list: String =
                miss.iter().take(18).map(|c| format!("{c} ")).collect::<String>();
            if miss.len() > 18 {
                list.push('…');
            }
            let lines = vec![
                Line::styled("Pas encore assez de caractères débloqués pour ce contenu.", bold(ui::WARN)),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("À débloquer d'abord : ", dim()),
                    Span::styled(list, bold(Color::White)),
                ]),
                Line::styled("Continue les modes Lettres ou Koch pour y arriver.", dim()),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("Entrée", bold(ui::ACCENT)),
                    Span::raw(" : essayer quand même avec tous les caractères"),
                ]),
                Line::from(vec![Span::styled("Échap", bold(ui::ACCENT)), Span::raw(" : retour")]),
            ];
            let h = lines.len() as u16 + 2;
            f.render_widget(
                Paragraph::new(lines).wrap(Wrap { trim: false }),
                ui::centered(area, 64, h),
            );
            return;
        }

        let area = ui::frame(f, "Radio", &[("↑↓", "choisir"), ("Entrée", "lancer"), ("Échap", "retour")]);
        let allowed = ctx.chars();
        let mut lines = vec![Line::styled("Que veux-tu copier ?", dim()), Line::raw("")];
        for (i, t) in TOPICS.iter().enumerate() {
            let mut spans = vec![if i == self.sel {
                Span::styled(format!(" ▶ {}", t.label()), bold(ui::ACCENT))
            } else {
                Span::raw(format!("   {}", t.label()))
            }];
            if !ready(*t, &allowed) {
                spans.push(Span::styled("  (caractères à débloquer)", dim()));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(topic.desc(), dim()));
        let h = lines.len() as u16;
        f.render_widget(Paragraph::new(lines), ui::centered(area, 64, h));
    }
}

// ----------------------------------------------------------- échange complet

const MAX_INPUT: usize = 160;

/// Notation d'une copie libre : alignement mot par mot, puis position par
/// position ; les espaces ne comptent pas, les mots tapés en trop non plus.
fn score_copy(sent: &str, typed: &str) -> Vec<(char, Option<char>)> {
    let mut words = typed.split_whitespace();
    sent.split_whitespace().flat_map(|w| align(w, words.next().unwrap_or(""))).collect()
}

/// Répartit des colonnes de largeurs données (séparées par une espace) en
/// rangées d'au plus `max` de large : renvoie le nombre de colonnes par rangée.
fn pack_rows(widths: &[usize], max: usize) -> Vec<usize> {
    let mut rows = Vec::new();
    let (mut n, mut used) = (0, 0);
    for w in widths {
        if n > 0 && used + 1 + w > max {
            rows.push(n);
            (n, used) = (0, 0);
        }
        used += if n == 0 { *w } else { w + 1 };
        n += 1;
    }
    if n > 0 {
        rows.push(n);
    }
    rows
}

/// Texte réel et copie l'un sous l'autre, mot sous mot, différences en rouge.
fn review_lines(sent: &str, typed: &str, width: usize) -> Vec<Line<'static>> {
    let sent: Vec<&str> = sent.split_whitespace().collect();
    let typed: Vec<&str> = typed.split_whitespace().collect();
    let cols = sent.len().max(typed.len());
    let word = |v: &[&str], i: usize| v.get(i).copied().unwrap_or("").to_string();
    let widths: Vec<usize> = (0..cols)
        .map(|i| word(&sent, i).chars().count().max(word(&typed, i).chars().count()).max(1))
        .collect();
    let mut lines = Vec::new();
    let mut i = 0;
    for n in pack_rows(&widths, width.max(1)) {
        let (mut top, mut bottom) = (Vec::new(), Vec::new());
        for col in i..i + n {
            let (s, t) = (word(&sent, col), word(&typed, col));
            let pad = |len: usize| Span::raw(" ".repeat(widths[col] + 1 - len));
            let shown = if s.is_empty() {
                vec![Span::styled(t.clone(), bold(ui::ERR))] // mot en trop
            } else {
                diff_spans(&s, &t)
            };
            let shown_len = t.chars().count().max(s.chars().count());
            top.push(Span::styled(s.clone(), bold(Color::White)));
            top.push(pad(s.chars().count()));
            bottom.extend(shown);
            bottom.push(pad(shown_len));
        }
        i += n;
        lines.push(Line::from(top));
        lines.push(Line::from(bottom));
        lines.push(Line::raw(""));
    }
    lines
}

struct Qso {
    force: bool,
    txs: Vec<String>,
    idx: usize,
    input: String,
    /// Correction de la transmission courante affichée.
    review: bool,
    span: Option<PlaySpan>,
    started: Option<Instant>,
    tally: Tally,
    copied: u32,
}

impl Qso {
    fn new(force: bool) -> Self {
        Qso {
            force,
            txs: Vec::new(),
            idx: 0,
            input: String::new(),
            review: false,
            span: None,
            started: None,
            tally: Tally::default(),
            copied: 0,
        }
    }

    /// Un échange n'utilisant que les caractères débloqués (sauf si forcé).
    fn generate(&self, ctx: &Ctx) -> Vec<String> {
        let mut rng = rand::rng();
        let allowed = ctx.chars();
        let mut q = data::qso(&mut rng);
        if self.force {
            return q;
        }
        for _ in 0..300 {
            if q.iter().all(|t| t.chars().all(|c| c == ' ' || allowed.contains(&c))) {
                break;
            }
            q = data::qso(&mut rng);
        }
        q
    }

    fn play(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
        self.span = self.txs.get(self.idx).map(|t| ctx.audio.play(t, ctx.timing()));
    }

    fn playing(&self) -> bool {
        self.span.is_some_and(|s| Instant::now() < s.end)
    }

    fn grade(&mut self, ctx: &mut Ctx) {
        let Some(sent) = self.txs.get(self.idx) else { return };
        ctx.audio.stop();
        self.span = None;
        self.tally.record(ctx, &score_copy(sent, &self.input));
        self.copied += 1;
        self.review = true;
    }

    fn finish(&mut self, ctx: &mut Ctx) -> Nav {
        ctx.finish(Summary {
            mode: "Radio – échange complet".into(),
            elapsed: self.started.map(|t| t.elapsed()).unwrap_or_default(),
            total: self.tally.total,
            correct: self.tally.correct,
            timing: ctx.timing(),
            can_unlock: false,
            misses: self.tally.misses(),
            notes: vec![format!("Transmissions copiées : {} sur {}", self.copied, self.txs.len())],
        })
    }
}

impl Screen for Qso {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        if self.started.is_some() {
            return;
        }
        self.started = Some(Instant::now());
        self.txs = self.generate(ctx);
        self.play(ctx);
    }

    fn on_leave(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if key.code == KeyCode::Esc {
            return self.finish(ctx);
        }
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return Nav::Stay;
        }
        if key.code == KeyCode::Tab {
            self.play(ctx);
            return Nav::Stay;
        }
        if self.review {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                if self.idx + 1 >= self.txs.len() {
                    return self.finish(ctx);
                }
                self.idx += 1;
                self.input.clear();
                self.review = false;
                self.play(ctx);
            }
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Backspace => {
                self.input.pop();
            }
            // Entrée à vide pendant l'écoute : sans doute une fausse manœuvre.
            KeyCode::Enter if !self.input.trim().is_empty() || !self.playing() => self.grade(ctx),
            KeyCode::Char(' ') => {
                if !self.input.is_empty() && !self.input.ends_with(' ') {
                    self.input.push(' ');
                }
            }
            KeyCode::Char(c) => {
                let c = c.to_ascii_uppercase();
                if morse::pattern(c).is_some() && self.input.chars().count() < MAX_INPUT {
                    self.input.push(c);
                }
            }
            _ => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, f: &mut Frame, _ctx: &Ctx) {
        let hints: &[(&str, &str)] = if self.review {
            &[("Entrée", "suite"), ("Tab", "réécouter"), ("Échap", "terminer")]
        } else {
            &[("Entrée", "corriger"), ("Tab", "réécouter du début"), ("Échap", "terminer")]
        };
        let area = ui::frame(f, "Radio – échange complet", hints);
        let mut status = format!("Transmission {} sur {}", (self.idx + 1).min(self.txs.len()), self.txs.len());
        if let Some(acc) = self.tally.accuracy() {
            status.push_str(&format!(" · {:.0} %", acc * 100.0));
        }
        let mut lines = vec![Line::styled(status, dim()), Line::raw("")];
        if self.review {
            let sent = self.txs.get(self.idx).map(String::as_str).unwrap_or("");
            lines.push(Line::styled("Le texte émis, et ta copie dessous :", dim()));
            lines.push(Line::raw(""));
            lines.extend(review_lines(sent, &self.input, area.width as usize));
            let last = self.idx + 1 >= self.txs.len();
            lines.push(Line::styled(
                if last { "Entrée : terminer l'échange." } else { "Entrée : transmission suivante." },
                dim(),
            ));
        } else {
            lines.push(if self.playing() {
                Line::styled("Écoute et note ce que tu copies, espaces compris.", dim())
            } else {
                Line::styled("Transmission terminée. Entrée pour voir la correction.", bold(ui::ACCENT))
            });
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("› ", dim()),
                Span::styled(self.input.clone(), bold(Color::White)),
                Span::styled("▏", bold(ui::ACCENT)),
            ]));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;
    use crate::curriculum::Track;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn ctx_with(unlocked: usize) -> Ctx {
        let mut ctx = Ctx::test();
        ctx.stats.set_unlocked(ctx.cfg.track, unlocked);
        ctx
    }

    fn type_text(s: &mut dyn Screen, ctx: &mut Ctx, text: &str) {
        for c in text.chars() {
            s.on_key(key(KeyCode::Char(c)), ctx);
        }
    }

    #[test]
    fn copy_is_scored_word_by_word() {
        let pairs = score_copy("CQ DE F4ABC K", "CQ DE F4ABD");
        assert_eq!(pairs.len(), 10); // espaces non comptés
        let ok = pairs.iter().filter(|(s, t)| Some(*s) == *t).count();
        assert_eq!(ok, 8); // C faux, K manquant
        assert_eq!(pairs[8], ('C', Some('D')));
        assert_eq!(pairs[9], ('K', None));
        // Un mot trop court ne décale pas les suivants.
        let pairs = score_copy("TNX FER QSO", "TN FER QSO EXTRA");
        assert_eq!(pairs.iter().filter(|(s, t)| Some(*s) == *t).count(), 8);
        assert_eq!(pairs[2], ('X', None));
        assert!(score_copy("73", "").iter().all(|(_, t)| t.is_none()));
        assert!(score_copy("", "CQ").is_empty());
    }

    #[test]
    fn rows_fit_the_width() {
        assert_eq!(pack_rows(&[2, 2, 5, 1], 80), vec![4]);
        assert_eq!(pack_rows(&[2, 2, 5, 1], 5), vec![2, 1, 1]);
        assert_eq!(pack_rows(&[9, 9], 3), vec![1, 1]);
        assert!(pack_rows(&[], 10).is_empty());
    }

    #[test]
    fn review_puts_copy_under_text() {
        let text = |l: &Line| l.spans.iter().map(|s| s.content.to_string()).collect::<String>();
        let lines = review_lines("CQ DE F4ABC", "CQ D F4ABD ZZ", 80);
        assert_eq!(text(&lines[0]).trim_end(), "CQ DE F4ABC");
        assert_eq!(text(&lines[1]).trim_end(), "CQ D_ F4ABD ZZ");
        let red: String = lines[1]
            .spans
            .iter()
            .filter(|s| s.style.fg == Some(ui::ERR))
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(red, "_DZZ");
        // Rangées étroites : rien ne dépasse.
        for l in review_lines("CQ CQ CQ DE F4ABC F4ABC K", "CQ", 12) {
            assert!(text(&l).trim_end().chars().count() <= 12);
        }
    }

    #[test]
    fn readiness_follows_unlocked_characters() {
        let lessons = Track::Lessons;
        let two = lessons.chars(2);
        let letters = lessons.chars(26);
        let all = lessons.chars(41);
        for t in TOPICS {
            assert!(!ready(t, &two), "{t:?}");
            assert!(ready(t, &all), "{t:?}");
            assert!(!missing(t, &two, &lessons.order()).is_empty());
            assert!(missing(t, &all, &lessons.order()).is_empty());
        }
        // L'alphabet suffit aux abréviations, pas aux indicatifs ni aux échanges.
        assert!(ready(Topic::Abbreviations, &letters));
        assert!(!ready(Topic::Callsigns, &letters));
        assert!(!ready(Topic::Qso, &letters));
        assert_eq!(missing(Topic::Callsigns, &letters, &lessons.order()), ('1'..='9').chain(['0']).collect::<Vec<_>>());
        // Manquants dans l'ordre d'apprentissage.
        assert_eq!(missing(Topic::Callsigns, &two, &lessons.order())[..4], ['I', 'M', 'S', 'O']);
        // Koch : quelques préfixes et un chiffre suffisent aux indicatifs.
        assert!(ready(Topic::Callsigns, &Track::Koch.chars(26)));
    }

    #[test]
    fn submenu_navigates_and_opens() {
        let mut ctx = ctx_with(41);
        let mut r = Radio::new();
        r.on_enter(&mut ctx);
        let s = render_to_string(&mut r, &ctx, 80, 24);
        assert!(s.contains("Indicatifs") && s.contains("Abréviations et codes Q") && s.contains("Échange complet"));
        assert!(!s.contains("à débloquer"));
        assert_eq!(r.sel, 0);
        r.on_key(key(KeyCode::Up), &mut ctx);
        assert_eq!(r.sel, 2);
        r.on_key(key(KeyCode::Down), &mut ctx);
        r.on_key(key(KeyCode::Down), &mut ctx);
        assert_eq!(r.sel, 1);
        for sel in 0..3 {
            r.sel = sel;
            assert!(matches!(r.on_key(key(KeyCode::Enter), &mut ctx), Nav::Push(_)));
            assert!(r.gate.is_none());
        }
        assert!(matches!(r.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
    }

    #[test]
    fn missing_characters_show_a_gate_that_can_be_forced() {
        let mut ctx = ctx_with(2);
        let mut r = Radio::new();
        r.on_enter(&mut ctx);
        assert!(render_to_string(&mut r, &ctx, 80, 24).contains("à débloquer"));
        for sel in 0..3 {
            r.sel = sel;
            assert!(matches!(r.on_key(key(KeyCode::Enter), &mut ctx), Nav::Stay));
            assert!(r.gate.is_some());
            let s = render_to_string(&mut r, &ctx, 80, 24);
            assert!(s.contains("Pas encore assez de caractères"), "{s}");
            assert!(s.contains("À débloquer d'abord : I M S O"), "{s}");
            assert!(s.contains("essayer quand même avec tous les caractères"), "{s}");
            render_to_string(&mut r, &ctx, 20, 6);
            // Échap revient au sous-menu, sans quitter le mode.
            assert!(matches!(r.on_key(key(KeyCode::Esc), &mut ctx), Nav::Stay));
            assert!(r.gate.is_none());
            // Entrée force.
            r.on_key(key(KeyCode::Enter), &mut ctx);
            assert!(matches!(r.on_key(key(KeyCode::Enter), &mut ctx), Nav::Push(_)));
            assert!(r.gate.is_none());
        }
    }

    #[test]
    fn callsigns_and_abbreviations_use_the_word_screen() {
        let mut ctx = ctx_with(41);
        let mut w = Words::with(Kind::Callsigns, false);
        w.on_enter(&mut ctx);
        let call = w.current_text();
        assert!(call.chars().any(|c| c.is_ascii_digit()), "{call}");
        assert!(!render_to_string(&mut w, &ctx, 80, 24).contains(&call));
        type_text(&mut w, &mut ctx, &call);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(render_to_string(&mut w, &ctx, 80, 24).contains("Juste"));
        assert!(matches!(w.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().mode, "Radio – indicatifs");

        let mut w = Words::with(Kind::Abbreviations, false);
        w.on_enter(&mut ctx);
        let code = w.current_text();
        let meaning = data::abbreviations().iter().find(|(c, _)| *c == code).unwrap().1;
        // Une signification très courte peut coïncider avec un texte fixe de l'écran.
        if meaning.chars().count() >= 12 {
            assert!(!render_to_string(&mut w, &ctx, 100, 24).contains(meaning));
        }
        type_text(&mut w, &mut ctx, &code);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        // La signification apparaît à la correction.
        assert!(render_to_string(&mut w, &ctx, 100, 24).contains(meaning));
        assert!(matches!(w.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().mode, "Radio – abréviations");
    }

    #[test]
    fn unforced_content_stays_within_unlocked_characters() {
        // Alphabet seul : abréviations sans chiffres.
        let mut ctx = ctx_with(26);
        let chars = ctx.chars();
        let mut w = Words::with(Kind::Abbreviations, false);
        w.on_enter(&mut ctx);
        for _ in 0..60 {
            let code = w.current_text();
            assert!(code.chars().all(|c| chars.contains(&c)), "{code}");
            type_text(&mut w, &mut ctx, &code);
            w.on_key(key(KeyCode::Enter), &mut ctx);
            w.on_key(key(KeyCode::Enter), &mut ctx);
        }
        // Koch partiel : indicatifs composés des seuls caractères connus.
        let mut ctx = Ctx::test();
        ctx.cfg.track = Track::Koch;
        ctx.stats.set_unlocked(Track::Koch, 26);
        let chars = ctx.chars();
        let mut w = Words::with(Kind::Callsigns, false);
        w.on_enter(&mut ctx);
        for _ in 0..60 {
            let call = w.current_text();
            assert!(call.chars().all(|c| chars.contains(&c)), "{call}");
            type_text(&mut w, &mut ctx, &call);
            w.on_key(key(KeyCode::Enter), &mut ctx);
            w.on_key(key(KeyCode::Enter), &mut ctx);
        }
        // Sans « ? » ni « / » : les échanges proposés s'en passent.
        let mut ctx = ctx_with(41);
        ctx.stats.unlocked.clear();
        ctx.cfg.track = Track::Koch;
        let n = Track::Koch.order().iter().position(|c| *c == '?').unwrap();
        ctx.stats.set_unlocked(Track::Koch, n);
        let chars = ctx.chars();
        if ready(Topic::Qso, &chars) {
            let q = Qso::new(false).generate(&ctx);
            assert!(q.iter().all(|t| !t.contains('?')));
        }
    }

    #[test]
    fn qso_is_copied_transmission_by_transmission() {
        let mut ctx = ctx_with(41);
        let mut q = Qso::new(false);
        q.on_enter(&mut ctx);
        let n = q.txs.len();
        assert!((3..=5).contains(&n));
        let first = q.txs[0].clone();
        // Le texte émis n'est pas montré avant la correction.
        let s = render_to_string(&mut q, &ctx, 120, 30);
        assert!(s.contains(&format!("Transmission 1 sur {n}")) && !s.contains(&first), "{s}");
        // Entrée à vide pendant l'écoute : ignorée.
        q.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(!q.review);
        // Copie avec une faute sur la dernière lettre, espaces doublés ignorés.
        let mut copy = first.to_lowercase().replace(' ', "  ");
        copy.pop();
        copy.push('?');
        type_text(&mut q, &mut ctx, &copy);
        q.on_key(key(KeyCode::Char('x')), &mut ctx);
        q.on_key(key(KeyCode::Backspace), &mut ctx);
        assert_eq!(q.input.len(), first.len());
        q.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(q.review);
        let chars = first.chars().filter(|c| *c != ' ').count() as u32;
        assert_eq!((q.tally.total, q.tally.correct), (chars, chars - 1));
        let s = render_to_string(&mut q, &ctx, 120, 30);
        assert!(s.contains(&first), "{s}");
        // Suite : transmission suivante, saisie vidée.
        q.on_key(key(KeyCode::Enter), &mut ctx);
        assert_eq!(q.idx, 1);
        assert!(!q.review && q.input.is_empty() && q.playing());
        // Jusqu'au bout : bilan.
        for i in 1..n {
            ctx.audio.stop();
            q.span = None;
            q.on_key(key(KeyCode::Enter), &mut ctx);
            assert!(q.review);
            let nav = q.on_key(key(KeyCode::Enter), &mut ctx);
            assert_eq!(matches!(nav, Nav::Replace(_)), i == n - 1);
        }
        let s = ctx.stats.sessions.last().unwrap();
        assert_eq!(s.mode, "Radio – échange complet");
        assert!(s.total > chars && s.correct == chars - 1);
    }

    #[test]
    fn qso_escape_and_small_terminals() {
        let mut ctx = ctx_with(2);
        let mut q = Qso::new(true);
        q.on_enter(&mut ctx);
        assert!(matches!(q.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            let mut q = Qso::new(true);
            q.on_enter(&mut ctx);
            render_to_string(&mut q, &ctx, w, h);
            type_text(&mut q, &mut ctx, "cq cq de f4abc/p = hw?");
            render_to_string(&mut q, &ctx, w, h);
            q.on_key(key(KeyCode::Enter), &mut ctx);
            render_to_string(&mut q, &ctx, w, h);
            let mut r = Radio::new();
            render_to_string(&mut r, &ctx, w, h);
        }
    }
}
