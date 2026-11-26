//! Mots : un mot joue, tu le tapes, correction immédiate.
//!
//! Le même écran, paramétré par `Kind`, sert aussi à la copie de tête
//! (`headcopy.rs`) et aux indicatifs / abréviations du mode radio (`radio.rs`).

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::audio::PlaySpan;
use crate::i18n::t;
use crate::modes::headcopy;
use crate::ui::{self, bold, dim};
use crate::{data, morse, picker};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rand::Rng;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::layout::Alignment;
use ratatui::Frame;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

/// En dessous de ce nombre de mots disponibles, on complète avec des
/// pseudo-mots aléatoires.
const MIN_POOL: usize = 8;
/// Un mot raté revient après ce nombre d'autres mots.
const RETRY_AFTER: u32 = 3;
const MAX_INPUT: usize = 12;

// ---------------------------------------------------------------- notation

/// Aligne la saisie sur le texte attendu, position par position : pour chaque
/// caractère attendu, le caractère tapé à la même place (`None` s'il manque).
/// Les caractères tapés en trop sont ignorés ici.
pub fn align(expected: &str, typed: &str) -> Vec<(char, Option<char>)> {
    let mut t = typed.chars();
    expected.chars().map(|c| (c, t.next())).collect()
}

/// La saisie colorée face au texte attendu : juste en vert, faux ou en trop
/// en rouge, `_` rouge pour chaque caractère manquant.
pub fn diff_spans(expected: &str, typed: &str) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = align(expected, typed)
        .into_iter()
        .map(|(e, t)| match t {
            Some(t) if t == e => Span::styled(t.to_string(), bold(ui::OK)),
            Some(t) => Span::styled(t.to_string(), bold(ui::ERR)),
            None => Span::styled("_", bold(ui::ERR)),
        })
        .collect();
    let extra: String = typed.chars().skip(expected.chars().count()).collect();
    if !extra.is_empty() {
        out.push(Span::styled(extra, bold(ui::ERR)));
    }
    out
}

/// Compteurs d'une session, en caractères.
#[derive(Default)]
pub struct Tally {
    pub total: u32,
    pub correct: u32,
    pub misses: BTreeMap<char, u32>,
}

impl Tally {
    /// Enregistre des paires (envoyé, tapé) dans les statistiques et le bilan.
    pub fn record(&mut self, ctx: &mut Ctx, pairs: &[(char, Option<char>)]) {
        for (sent, typed) in pairs {
            ctx.stats.record(*sent, *typed, None);
            self.total += 1;
            if *typed == Some(*sent) {
                self.correct += 1;
            } else {
                *self.misses.entry(*sent).or_default() += 1;
            }
        }
    }

    pub fn accuracy(&self) -> Option<f32> {
        (self.total > 0).then(|| self.correct as f32 / self.total as f32)
    }

    pub fn misses(&self) -> Vec<(char, u32)> {
        self.misses.iter().map(|(c, n)| (*c, *n)).collect()
    }
}

// ------------------------------------------------------------------- écran

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Words,
    HeadCopy,
    Callsigns,
    Abbreviations,
}

impl Kind {
    pub fn mode(self) -> &'static str {
        match self {
            Kind::Words => t("Mots", "Words"),
            Kind::HeadCopy => t("Copie de tête", "Head copy"),
            Kind::Callsigns => t("Radio – indicatifs", "Radio – callsigns"),
            Kind::Abbreviations => t("Radio – abréviations", "Radio – abbreviations"),
        }
    }

    /// Nom de ce qu'on copie, pour les messages.
    fn unit(self) -> &'static str {
        match self {
            Kind::Words | Kind::HeadCopy => t("mots", "words"),
            Kind::Callsigns => t("indicatifs", "callsigns"),
            Kind::Abbreviations => t("abréviations", "abbreviations"),
        }
    }
}

#[derive(Clone)]
struct Item {
    text: String,
    /// Signification (abréviations), affichée à la correction.
    meaning: Option<&'static str>,
}

enum Phase {
    /// Le mot joue ou a joué ; on attend la saisie.
    Input,
    /// Correction affichée jusqu'à `until`.
    Feedback { ok: bool, until: Instant },
}

pub struct Words {
    kind: Kind,
    /// Ignorer les caractères débloqués (mode radio forcé).
    force: bool,
    pool: Vec<Item>,
    /// La liste est trop courte : complétée par des pseudo-mots.
    padded: bool,
    /// Mots ratés à représenter : (numéro de mot à partir duquel, mot).
    retry: VecDeque<(u32, Item)>,
    current: Option<Item>,
    typed: String,
    phase: Phase,
    span: Option<PlaySpan>,
    started: Option<Instant>,
    tally: Tally,
    done: u32,
    done_ok: u32,
    /// Copie de tête : longueur maximale actuelle, réussites d'affilée.
    max_len: usize,
    streak: u32,
    best_len: usize,
}

impl Words {
    pub fn new() -> Self {
        Words::with(Kind::Words, false)
    }

    pub fn with(kind: Kind, force: bool) -> Self {
        Words {
            kind,
            force,
            pool: Vec::new(),
            padded: false,
            retry: VecDeque::new(),
            current: None,
            typed: String::new(),
            phase: Phase::Input,
            span: None,
            started: None,
            tally: Tally::default(),
            done: 0,
            done_ok: 0,
            max_len: headcopy::START_LEN,
            streak: 0,
            best_len: headcopy::START_LEN,
        }
    }

    /// Caractères du clavier tactile : les débloqués, plus tout ce que le
    /// contenu peut demander quand le mode est forcé.
    fn keys(&self, ctx: &Ctx) -> Vec<char> {
        let mut chars = ctx.chars();
        if self.force {
            chars.extend(('A'..='Z').chain('0'..='9').chain(['/']));
            chars.extend(self.pool.iter().flat_map(|i| i.text.chars()));
        }
        chars
    }

    fn build_pool(&mut self, ctx: &Ctx) {
        let chars = ctx.chars();
        let plain = |text: &str| Item { text: text.to_string(), meaning: None };
        self.pool = match self.kind {
            Kind::Words | Kind::HeadCopy => {
                data::words(ctx.cfg.lang, &chars).into_iter().map(plain).collect()
            }
            Kind::Abbreviations => data::abbreviations()
                .iter()
                .filter(|(code, _)| self.force || code.chars().all(|c| chars.contains(&c)))
                .map(|(code, meaning)| Item { text: code.to_string(), meaning: Some(*meaning) })
                .collect(),
            Kind::Callsigns => Vec::new(), // tirés à la volée
        };
        self.padded =
            matches!(self.kind, Kind::Words | Kind::HeadCopy) && self.pool.len() < MIN_POOL;
    }

    fn fresh_item(&self, ctx: &Ctx) -> Item {
        let mut rng = rand::rng();
        let chars = ctx.chars();
        let pseudo = |rng: &mut rand::rngs::ThreadRng, max: usize| Item {
            text: picker::group(&chars, &ctx.stats, rng.random_range(2..=max.clamp(2, 4)), rng),
            meaning: None,
        };
        match self.kind {
            Kind::Callsigns => {
                let text = (!self.force)
                    .then(|| data::callsign_from(&mut rng, &chars))
                    .flatten()
                    .unwrap_or_else(|| data::callsign(&mut rng));
                Item { text, meaning: None }
            }
            Kind::Abbreviations if self.pool.is_empty() => {
                let (code, meaning) = data::abbreviations()[0];
                Item { text: code.into(), meaning: Some(meaning) }
            }
            Kind::Abbreviations => self.pool[rng.random_range(0..self.pool.len())].clone(),
            Kind::Words => {
                let slot = rng.random_range(0..self.pool.len().max(MIN_POOL));
                self.pool.get(slot).cloned().unwrap_or_else(|| pseudo(&mut rng, 4))
            }
            Kind::HeadCopy => {
                let lens: Vec<usize> = self.pool.iter().map(|i| i.text.len()).collect();
                let cands = headcopy::candidates(&lens, self.max_len);
                if cands.is_empty() || (self.padded && rng.random_bool(0.5)) {
                    pseudo(&mut rng, self.max_len)
                } else {
                    self.pool[cands[rng.random_range(0..cands.len())]].clone()
                }
            }
        }
    }

    fn next_item(&mut self, ctx: &mut Ctx) {
        let last = self.current.take().map(|i| i.text);
        let due = self.retry.front().is_some_and(|(at, _)| *at <= self.done);
        let item = if due {
            self.retry.pop_front().unwrap().1
        } else {
            // Éviter de rejouer le même mot deux fois de suite.
            let mut item = self.fresh_item(ctx);
            for _ in 0..8 {
                if Some(&item.text) != last.as_ref() {
                    break;
                }
                item = self.fresh_item(ctx);
            }
            item
        };
        self.typed.clear();
        self.phase = Phase::Input;
        ctx.audio.stop();
        self.span = Some(ctx.audio.play(&item.text, ctx.timing()));
        self.current = Some(item);
    }

    fn playing(&self) -> bool {
        self.span.is_some_and(|s| Instant::now() < s.end)
    }

    /// Copie de tête : pas de saisie tant que le mot joue.
    fn locked(&self) -> bool {
        self.kind == Kind::HeadCopy && self.playing()
    }

    fn replay(&mut self, ctx: &mut Ctx) {
        if let Some(item) = &self.current {
            ctx.audio.stop();
            self.span = Some(ctx.audio.play(&item.text, ctx.timing()));
        }
    }

    fn validate(&mut self, ctx: &mut Ctx) {
        let Some(item) = self.current.clone() else { return };
        let ok = self.typed == item.text;
        self.tally.record(ctx, &align(&item.text, &self.typed));
        self.done += 1;
        ctx.audio.stop();
        let mut wait = Duration::from_millis(if item.meaning.is_some() { 2200 } else { 900 });
        if ok {
            self.done_ok += 1;
            self.span = None;
        } else {
            // Réécoute, cette fois avec le texte sous les yeux.
            let span = ctx.audio.play(&item.text, ctx.timing());
            wait += span.end.saturating_duration_since(Instant::now());
            self.span = Some(span);
            self.retry.push_back((self.done + RETRY_AFTER, item));
        }
        if self.kind == Kind::HeadCopy {
            (self.max_len, self.streak) = headcopy::adapt(self.max_len, self.streak, ok);
            self.best_len = self.best_len.max(self.max_len);
        }
        self.phase = Phase::Feedback { ok, until: Instant::now() + wait };
    }

    fn elapsed(&self) -> Duration {
        self.started.map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Passe au mot suivant, ou termine si la durée de session est écoulée.
    fn advance(&mut self, ctx: &mut Ctx) -> Nav {
        if self.elapsed().as_secs() >= ctx.cfg.session_secs as u64 {
            return self.finish(ctx);
        }
        self.next_item(ctx);
        Nav::Stay
    }

    fn finish(&mut self, ctx: &mut Ctx) -> Nav {
        let unit = self.kind.unit();
        let mut notes = Vec::new();
        if self.done > 0 {
            notes.push(crate::tr!(
                "{} {unit} justes sur {}",
                "{} of {} {unit} correct",
                self.done_ok,
                self.done
            ));
        }
        if self.kind == Kind::HeadCopy {
            notes.push(crate::tr!(
                "Longueur atteinte : {} lettres",
                "Length reached: {} letters",
                self.best_len
            ));
        }
        ctx.finish(Summary {
            mode: self.kind.mode().into(),
            elapsed: self.elapsed(),
            total: self.tally.total,
            correct: self.tally.correct,
            timing: ctx.timing(),
            can_unlock: false,
            misses: self.tally.misses(),
            notes,
        })
    }

    /// Tests : fait comme si le son et la pause de correction étaient finis.
    #[cfg(test)]
    pub fn skip_wait(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
        self.span = None;
        if let Phase::Feedback { until, .. } = &mut self.phase {
            *until = Instant::now();
        }
    }

    /// Tests : le texte en cours de lecture.
    #[cfg(test)]
    pub fn current_text(&self) -> String {
        self.current.as_ref().map(|i| i.text.clone()).unwrap_or_default()
    }

    #[cfg(test)]
    pub fn in_feedback(&self) -> bool {
        matches!(self.phase, Phase::Feedback { .. })
    }

    #[cfg(test)]
    pub fn max_len(&self) -> usize {
        self.max_len
    }
}

impl Screen for Words {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        if self.started.is_some() {
            return;
        }
        self.started = Some(Instant::now());
        self.build_pool(ctx);
        self.next_item(ctx);
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
        if let Phase::Feedback { .. } = self.phase {
            // Entrée ou Espace : ne pas attendre la fin de la correction.
            if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                return self.advance(ctx);
            }
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Tab => self.replay(ctx),
            _ if self.locked() => {}
            KeyCode::Backspace => {
                self.typed.pop();
            }
            // Une validation à vide pendant l'écoute est sans doute une
            // fausse manœuvre ; après, elle veut dire « je ne sais pas ».
            KeyCode::Enter if !self.typed.is_empty() || !self.playing() => self.validate(ctx),
            KeyCode::Char(' ') if !self.typed.is_empty() => self.validate(ctx),
            KeyCode::Char(c) => {
                let c = crate::morse::up(c);
                if morse::pattern(c).is_some() && self.typed.chars().count() < MAX_INPUT {
                    self.typed.push(c);
                }
            }
            _ => {}
        }
        Nav::Stay
    }

    fn on_tick(&mut self, ctx: &mut Ctx) -> Nav {
        match self.phase {
            Phase::Feedback { until, .. } if Instant::now() >= until => self.advance(ctx),
            _ => Nav::Stay,
        }
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let (enter, esc) = (t("Entrée", "Enter"), t("Échap", "Esc"));
        let end = t("terminer", "finish");
        let hints: Vec<(&str, &str)> = match self.phase {
            Phase::Input => vec![
                (enter, t("valider", "submit")),
                ("Tab", t("réécouter", "replay")),
                (esc, end),
            ],
            Phase::Feedback { .. } => vec![(enter, t("suivant", "next")), (esc, end)],
        };
        let mut area = ui::frame(f, self.kind.mode(), &hints);
        if matches!(self.phase, Phase::Input) {
            area = ui::keyboard(f, area, ui::Keys { chars: &self.keys(ctx), backspace: true, space: false });
        }

        let left = (ctx.cfg.session_secs as u64).saturating_sub(self.elapsed().as_secs());
        let mut status = crate::tr!(
            "Reste {} · {} {}",
            "{} left · {} {}",
            ui::mmss(left),
            self.done,
            self.kind.unit()
        );
        if let Some(acc) = self.tally.accuracy() {
            status.push_str(&format!(" · {:.0} %", acc * 100.0));
        }
        if self.kind == Kind::HeadCopy {
            status.push_str(&crate::tr!(" · jusqu'à {} lettres", " · up to {} letters", self.max_len));
        }
        let mut lines = vec![Line::styled(status, dim()), Line::raw("")];

        let expected = self.current.as_ref().map(|i| i.text.as_str()).unwrap_or("");
        match self.phase {
            Phase::Input => {
                let (msg, style) = if self.locked() {
                    (t("Écoute…", "Listen…"), bold(ui::ACCENT))
                } else if self.playing() {
                    (t("Écoute et tape au fil du son.", "Listen and type as you hear it."), dim())
                } else {
                    (t("Tape ce que tu as entendu.", "Type what you heard."), dim())
                };
                lines.push(Line::styled(msg, style));
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![
                    Span::styled("› ", dim()),
                    Span::styled(self.typed.clone(), bold(Color::White)),
                    Span::styled(if self.locked() { "" } else { "▏" }, bold(ui::ACCENT)),
                ]));
            }
            Phase::Feedback { ok, .. } => {
                lines.push(if ok {
                    Line::styled(t("Juste !", "Correct!"), bold(ui::OK))
                } else {
                    Line::styled(
                        t("Raté. Réécoute en lisant :", "Missed. Listen again while reading:"),
                        bold(ui::ERR),
                    )
                });
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![
                    Span::styled(t("Attendu  ", "Expected "), dim()),
                    Span::styled(expected.to_string(), bold(Color::White)),
                ]));
                let mut typed = vec![Span::styled(t("Tapé     ", "Typed    "), dim())];
                typed.extend(diff_spans(expected, &self.typed));
                lines.push(Line::from(typed));
                if let Some(m) = self.current.as_ref().and_then(|i| i.meaning) {
                    lines.push(Line::raw(""));
                    lines.push(Line::from(vec![
                        Span::styled(t("Sens     ", "Meaning  "), dim()),
                        Span::styled(m, bold(ui::ACCENT)),
                    ]));
                }
            }
        }
        if self.padded {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                t(
                    "Peu de mots possibles avec tes caractères : complété par des groupes au hasard.",
                    "Few words possible with your characters: random groups fill the gaps.",
                ),
                dim(),
            ));
        }
        ui::text(f, lines, area, 80, Alignment::Left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn type_text(w: &mut Words, ctx: &mut Ctx, s: &str) {
        for c in s.chars() {
            w.on_key(key(KeyCode::Char(c)), ctx);
        }
    }

    fn started(kind: Kind, unlocked: usize) -> (Words, Ctx) {
        let mut ctx = Ctx::test();
        ctx.stats.set_unlocked(ctx.cfg.track, unlocked);
        let mut w = Words::with(kind, false);
        w.on_enter(&mut ctx);
        (w, ctx)
    }

    #[test]
    fn align_position_by_position() {
        assert_eq!(align("MOT", "MOT"), vec![('M', Some('M')), ('O', Some('O')), ('T', Some('T'))]);
        assert_eq!(align("MOT", "MA"), vec![('M', Some('M')), ('O', Some('A')), ('T', None)]);
        assert_eq!(align("ET", "ETTT").len(), 2);
        assert_eq!(align("ET", ""), vec![('E', None), ('T', None)]);
    }

    #[test]
    fn diff_marks_wrong_missing_and_extra() {
        let text = |s: &[Span]| s.iter().map(|x| x.content.to_string()).collect::<String>();
        let d = diff_spans("MOT", "MA");
        assert_eq!(text(&d), "MA_");
        assert_eq!(d[0].style.fg, Some(ui::OK));
        assert_eq!(d[1].style.fg, Some(ui::ERR));
        assert_eq!(d[2].style.fg, Some(ui::ERR));
        let d = diff_spans("ET", "ETSS");
        assert_eq!(text(&d), "ETSS");
        assert_eq!(d.last().unwrap().style.fg, Some(ui::ERR));
    }

    #[test]
    fn right_word_scores_every_letter() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        let word = w.current_text();
        assert!(word.len() >= 2);
        // Rien de ce qui joue n'est affiché avant la réponse.
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(!s.contains(&word), "{s}");
        assert!(!s.contains("groupes au hasard"));
        type_text(&mut w, &mut ctx, &word.to_lowercase());
        w.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(w.in_feedback());
        assert_eq!((w.tally.total, w.tally.correct), (word.len() as u32, word.len() as u32));
        assert!(w.retry.is_empty());
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Juste") && s.contains(&word));
        // La correction passe toute seule au mot suivant.
        w.skip_wait(&mut ctx);
        assert!(matches!(w.on_tick(&mut ctx), Nav::Stay));
        assert!(!w.in_feedback());
        assert!(w.typed.is_empty());
    }

    #[test]
    fn wrong_word_is_replayed_and_comes_back() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        let word = w.current_text();
        let first = word.chars().next().unwrap();
        // Première lettre juste, puis une de trop peu.
        type_text(&mut w, &mut ctx, &first.to_string());
        w.on_key(key(KeyCode::Char('x')), &mut ctx);
        w.on_key(key(KeyCode::Backspace), &mut ctx);
        w.on_key(key(KeyCode::Char(' ')), &mut ctx);
        assert!(w.in_feedback());
        assert_eq!(w.tally.total, word.len() as u32);
        assert_eq!(w.tally.correct, 1);
        assert_eq!(ctx.stats.char(first).unwrap().correct, 1);
        // Rejoué avec le texte visible.
        assert!(w.playing());
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Raté") && s.contains(&word));
        // Il revient après quelques autres mots.
        assert_eq!(w.retry.len(), 1);
        let mut seen_again = false;
        for _ in 0..RETRY_AFTER + 1 {
            w.skip_wait(&mut ctx);
            w.on_tick(&mut ctx);
            seen_again |= w.current_text() == word && w.retry.is_empty();
            let cur = w.current_text();
            type_text(&mut w, &mut ctx, &cur);
            w.on_key(key(KeyCode::Enter), &mut ctx);
        }
        assert!(seen_again);
    }

    #[test]
    fn empty_enter_is_ignored_while_playing() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        w.on_key(key(KeyCode::Char(' ')), &mut ctx);
        assert!(!w.in_feedback());
        // Une fois le son fini, Entrée à vide veut dire « je ne sais pas ».
        w.skip_wait(&mut ctx);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(w.in_feedback());
        assert_eq!(w.tally.correct, 0);
        assert!(w.tally.total >= 2);
    }

    #[test]
    fn tab_replays_before_validating() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        let word = w.current_text();
        w.skip_wait(&mut ctx);
        assert!(!w.playing());
        w.on_key(key(KeyCode::Tab), &mut ctx);
        assert!(w.playing());
        assert_eq!(w.current_text(), word);
    }

    #[test]
    fn few_words_are_padded_with_groups() {
        // Avec E et T seuls : 4 mots, complétés par des pseudo-mots.
        let (mut w, mut ctx) = started(Kind::Words, 2);
        assert!(w.padded);
        let s = render_to_string(&mut w, &ctx, 100, 24);
        assert!(s.contains("groupes au hasard"), "{s}");
        let mut pseudo = false;
        for _ in 0..60 {
            let cur = w.current_text();
            assert!((2..=4).contains(&cur.len()), "{cur}");
            assert!(cur.chars().all(|c| c == 'E' || c == 'T'), "{cur}");
            pseudo |= !["ET", "TE", "TETE", "ETE"].contains(&cur.as_str());
            type_text(&mut w, &mut ctx, &cur);
            w.on_key(key(KeyCode::Enter), &mut ctx);
            w.on_key(key(KeyCode::Enter), &mut ctx);
        }
        assert!(pseudo);
    }

    #[test]
    fn only_unlocked_characters_are_played() {
        let (mut w, mut ctx) = started(Kind::Words, 10);
        let chars = ctx.chars();
        for _ in 0..80 {
            let cur = w.current_text();
            assert!(cur.chars().all(|c| chars.contains(&c)), "{cur}");
            type_text(&mut w, &mut ctx, &cur);
            w.on_key(key(KeyCode::Enter), &mut ctx);
            w.on_key(key(KeyCode::Enter), &mut ctx);
        }
    }

    #[test]
    fn escape_ends_with_a_summary() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        // Rien de fait : simple retour.
        assert!(matches!(w.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        let (mut w, mut ctx) = started(Kind::Words, 26);
        let word = w.current_text();
        type_text(&mut w, &mut ctx, &word);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(matches!(w.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        let s = ctx.stats.sessions.last().unwrap();
        assert_eq!(s.mode, "Mots");
        assert_eq!((s.total, s.correct), (word.len() as u32, word.len() as u32));
        assert_eq!(ctx.stats.unlocked(ctx.cfg.track), 26);
    }

    #[test]
    fn session_ends_when_time_is_up() {
        let (mut w, mut ctx) = started(Kind::Words, 26);
        ctx.cfg.session_secs = 0;
        let word = w.current_text();
        type_text(&mut w, &mut ctx, &word);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        w.skip_wait(&mut ctx);
        assert!(matches!(w.on_tick(&mut ctx), Nav::Replace(_)));
    }

    #[test]
    fn english_interface() {
        crate::i18n::set(crate::config::Lang::En);
        // Mots : écoute, saisie, correction juste puis ratée, bilan.
        let (mut w, mut ctx) = started(Kind::Words, 26);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Words") && s.contains("Listen and type as you hear it."), "{s}");
        assert!(s.contains("left · 0 words") && s.contains("Enter") && s.contains("replay"), "{s}");
        for fr in ["Mots", "Reste", "Écoute", "Entrée", "Échap", "valider", "terminer"] {
            assert!(!s.contains(fr), "{fr} : {s}");
        }
        w.skip_wait(&mut ctx);
        assert!(render_to_string(&mut w, &ctx, 80, 24).contains("Type what you heard."));
        let word = w.current_text();
        type_text(&mut w, &mut ctx, &word);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Correct!") && s.contains("Expected ") && s.contains("Typed "), "{s}");
        assert!(s.contains("next") && !s.contains("Juste") && !s.contains("Attendu"), "{s}");
        w.on_key(key(KeyCode::Enter), &mut ctx);
        w.skip_wait(&mut ctx);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Missed. Listen again while reading:") && !s.contains("Raté"), "{s}");
        assert!(matches!(w.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().mode, "Words");

        // Liste trop courte : l'avertissement tient sur 80 colonnes.
        let (mut w, ctx) = started(Kind::Words, 2);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("random groups fill the gaps.") && !s.contains("hasard"), "{s}");

        // Abréviations : le sens, en anglais, à la correction.
        let (mut w, mut ctx) = started(Kind::Abbreviations, 41);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Radio – abbreviations") && s.contains("0 abbreviations"), "{s}");
        let code = w.current_text();
        let meaning = data::abbreviations().iter().find(|(c, _)| *c == code).unwrap().1;
        assert!(meaning.is_ascii());
        type_text(&mut w, &mut ctx, &code);
        w.on_key(key(KeyCode::Enter), &mut ctx);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Meaning  ") && s.contains(meaning) && !s.contains("Sens "), "{s}");

        let (mut w, ctx) = started(Kind::Callsigns, 41);
        let s = render_to_string(&mut w, &ctx, 80, 24);
        assert!(s.contains("Radio – callsigns") && s.contains("0 callsigns"), "{s}");
        assert!(!s.contains("indicatifs"), "{s}");

        // Toutes les tailles, tous les états.
        for kind in [Kind::Words, Kind::HeadCopy, Kind::Callsigns, Kind::Abbreviations] {
            for unlocked in [2, 41] {
                for (cw, ch) in [(80, 24), (20, 6), (1, 1)] {
                    let (mut w, mut ctx) = started(kind, unlocked);
                    render_to_string(&mut w, &ctx, cw, ch);
                    w.skip_wait(&mut ctx);
                    w.on_key(key(KeyCode::Char('e')), &mut ctx);
                    render_to_string(&mut w, &ctx, cw, ch);
                    w.on_key(key(KeyCode::Enter), &mut ctx);
                    render_to_string(&mut w, &ctx, cw, ch);
                }
            }
        }
        crate::i18n::set(crate::config::Lang::Fr);
    }

    #[test]
    fn renders_at_any_size() {
        for kind in [Kind::Words, Kind::HeadCopy, Kind::Callsigns, Kind::Abbreviations] {
            for (cw, ch) in [(80, 24), (20, 6), (1, 1)] {
                let (mut w, mut ctx) = started(kind, 41);
                render_to_string(&mut w, &ctx, cw, ch);
                w.skip_wait(&mut ctx);
                w.on_key(key(KeyCode::Char('e')), &mut ctx);
                render_to_string(&mut w, &ctx, cw, ch);
                w.on_key(key(KeyCode::Enter), &mut ctx);
                render_to_string(&mut w, &ctx, cw, ch);
            }
        }
    }
}
