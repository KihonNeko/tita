//! Koch — copie de groupes de 5 caractères à pleine vitesse, correction à la fin.

use crate::app::{Ctx, Nav, Screen, Summary};
use crate::audio::PlaySpan;
use crate::morse;
use crate::picker;
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::time::{Duration, Instant};

const GROUP_LEN: usize = 5;
/// Avance d'audio gardée en file.
const LOOKAHEAD: Duration = Duration::from_millis(400);
/// Délai pour finir de taper après la fin de l'audio.
const FINISH_PAUSE: Duration = Duration::from_secs(8);
/// Chaque frappe pendant ce délai le prolonge d'au moins ceci.
const FINISH_EXTEND: Duration = Duration::from_secs(4);

/// Un groupe corrigé : caractères envoyés, réponse à chaque position, et
/// caractères tapés en trop.
#[derive(Debug, PartialEq)]
struct Graded {
    cells: Vec<(char, Option<char>)>,
    extra: usize,
}

/// Aligne le i-ème groupe tapé sur le i-ème groupe envoyé, position par
/// position ; un caractère manquant est raté (`None`).
fn grade(sent: &[String], typed: &str) -> Vec<Graded> {
    let typed: Vec<Vec<char>> = typed
        .split_whitespace()
        .map(|g| g.chars().map(|c| crate::morse::up(c)).collect())
        .collect();
    sent.iter()
        .enumerate()
        .map(|(i, g)| {
            let t = typed.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
            let cells: Vec<_> = g.chars().enumerate().map(|(j, c)| (c, t.get(j).copied())).collect();
            Graded { extra: t.len().saturating_sub(cells.len()), cells }
        })
        .collect()
}

fn score(graded: &[Graded]) -> (u32, u32) {
    let total = graded.iter().map(|g| g.cells.len()).sum::<usize>() as u32;
    let correct = graded
        .iter()
        .flat_map(|g| &g.cells)
        .filter(|(s, t)| *t == Some(*s))
        .count() as u32;
    (total, correct)
}

enum Phase {
    Ready,
    Listening,
    /// L'audio est fini : on laisse finir la saisie jusqu'à l'échéance.
    Finishing(Instant),
    Review,
}

pub struct Koch {
    phase: Phase,
    started: Instant,
    elapsed: Duration,
    /// Groupes mis en file et fenêtre audio de chacun de leurs caractères.
    groups: Vec<String>,
    spans: Vec<Vec<PlaySpan>>,
    /// Plus aucun groupe ne sera ajouté.
    scheduled_all: bool,
    input: String,
    graded: Vec<Graded>,
    scroll: u16,
}

impl Koch {
    pub fn new() -> Self {
        Koch {
            phase: Phase::Ready,
            started: Instant::now(),
            elapsed: Duration::ZERO,
            groups: Vec::new(),
            spans: Vec::new(),
            scheduled_all: false,
            input: String::new(),
            graded: Vec::new(),
            scroll: 0,
        }
    }

    fn start(&mut self, ctx: &mut Ctx, now: Instant) {
        ctx.audio.stop();
        self.started = now;
        self.phase = Phase::Listening;
    }

    /// Met en file le caractère suivant (et le silence qui le suit).
    fn schedule_next(&mut self, ctx: &mut Ctx, now: Instant) {
        let group_done = self.spans.last().is_none_or(|s| s.len() >= GROUP_LEN);
        if group_done {
            let chars = ctx.chars();
            if chars.is_empty() {
                self.scheduled_all = true;
                return;
            }
            self.groups.push(picker::group(&chars, &ctx.stats, GROUP_LEN, &mut rand::rng()));
            self.spans.push(Vec::new());
        }
        let t = ctx.timing();
        let group = self.groups.last().unwrap();
        let spans = self.spans.last_mut().unwrap();
        let Some(c) = group.chars().nth(spans.len()) else { return };
        spans.push(ctx.audio.play_segments(&morse::char_segments(c, t)));
        if spans.len() < GROUP_LEN {
            ctx.audio.silence(Duration::from_secs_f32(t.char_gap()));
        } else if now.saturating_duration_since(self.started).as_secs() >= ctx.cfg.session_secs as u64 {
            // Temps écoulé : ce groupe est le dernier.
            self.scheduled_all = true;
        } else {
            ctx.audio.silence(Duration::from_secs_f32(t.word_gap()));
        }
    }

    fn last_end(&self) -> Option<Instant> {
        self.spans.last().and_then(|s| s.last()).map(|s| s.end)
    }

    /// Nombre de groupes dont l'écoute a commencé à l'instant `now`.
    fn groups_heard(&self, now: Instant) -> usize {
        self.spans.iter().filter(|s| s.first().is_some_and(|p| p.start <= now)).count()
    }

    fn to_finishing(&mut self, ctx: &mut Ctx, now: Instant) {
        ctx.audio.stop();
        self.elapsed = now.saturating_duration_since(self.started);
        self.phase = Phase::Finishing(now + FINISH_PAUSE);
    }

    /// Corrige, enregistre les statistiques et passe à l'écran de comparaison.
    fn correct(&mut self, ctx: &mut Ctx) {
        self.graded = grade(&self.groups, &self.input);
        for (sent, typed) in self.graded.iter().flat_map(|g| &g.cells) {
            ctx.stats.record(*sent, *typed, None);
        }
        self.scroll = 0;
        self.phase = Phase::Review;
    }

    fn summary(&self, ctx: &Ctx) -> Summary {
        let (total, correct) = score(&self.graded);
        let mut misses: Vec<(char, u32)> = Vec::new();
        for (s, t) in self.graded.iter().flat_map(|g| &g.cells) {
            if *t != Some(*s) {
                match misses.iter_mut().find(|m| m.0 == *s) {
                    Some(m) => m.1 += 1,
                    None => misses.push((*s, 1)),
                }
            }
        }
        Summary {
            mode: "Koch".into(),
            elapsed: self.elapsed,
            total,
            correct,
            timing: ctx.timing(),
            can_unlock: true,
            misses,
            notes: vec![format!("Groupes     {}", self.graded.len())],
        }
    }

    fn edit_input(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(' ') => {
                if !self.input.is_empty() && !self.input.ends_with(' ') {
                    self.input.push(' ');
                }
            }
            KeyCode::Char(c) if !c.is_control() => self.input.push(crate::morse::up(c)),
            _ => return false,
        }
        true
    }

    fn tick_at(&mut self, ctx: &mut Ctx, now: Instant) -> Nav {
        match self.phase {
            Phase::Listening => {
                if !self.scheduled_all && ctx.audio.pending() < LOOKAHEAD {
                    self.schedule_next(ctx, now);
                }
                if self.scheduled_all && self.last_end().is_none_or(|e| now >= e) {
                    if self.groups.is_empty() {
                        return Nav::Back;
                    }
                    self.to_finishing(ctx, now);
                }
            }
            Phase::Finishing(deadline) if now >= deadline => self.correct(ctx),
            _ => {}
        }
        Nav::Stay
    }

    fn key_at(&mut self, key: KeyEvent, ctx: &mut Ctx, now: Instant) -> Nav {
        match self.phase {
            Phase::Ready => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => self.start(ctx, now),
                KeyCode::Esc | KeyCode::Char('q') => return Nav::Back,
                _ => {}
            },
            Phase::Listening => match key.code {
                KeyCode::Esc => {
                    // On ne note que les groupes entièrement joués.
                    ctx.audio.stop();
                    let done = self
                        .spans
                        .iter()
                        .filter(|s| s.len() >= GROUP_LEN && s.last().is_some_and(|p| p.end <= now))
                        .count();
                    self.groups.truncate(done);
                    self.spans.truncate(done);
                    self.scheduled_all = true;
                    if done == 0 {
                        return Nav::Back;
                    }
                    self.to_finishing(ctx, now);
                }
                code => {
                    self.edit_input(code);
                }
            },
            Phase::Finishing(deadline) => match key.code {
                KeyCode::Enter | KeyCode::Esc => self.correct(ctx),
                code => {
                    if self.edit_input(code) {
                        self.phase = Phase::Finishing(deadline.max(now + FINISH_EXTEND));
                    }
                }
            },
            Phase::Review => match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                    let s = self.summary(ctx);
                    return ctx.finish(s);
                }
                KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(9),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(9),
                _ => {}
            },
        }
        Nav::Stay
    }

    /// Ligne de saisie : la fin du texte tapé, suivie d'un curseur.
    fn input_line(&self, width: u16) -> Line<'static> {
        let max = (width as usize).saturating_sub(3);
        let n = self.input.chars().count();
        let tail: String = self.input.chars().skip(n.saturating_sub(max)).collect();
        Line::from(vec![
            Span::styled("> ", dim()),
            Span::styled(tail, bold(Color::White)),
            Span::styled("▏", bold(ui::ACCENT)),
        ])
    }

    fn review_lines(&self, width: u16) -> Vec<Line<'static>> {
        const LABEL: usize = 8;
        let (total, correct) = score(&self.graded);
        let acc = correct as f32 / total.max(1) as f32;
        let mut lines = vec![
            Line::from(vec![
                Span::raw("Précision "),
                Span::styled(format!("{:.0} %", acc * 100.0), bold(acc_color(acc))),
                Span::styled(format!("   ({correct} / {total}) · {} groupes", self.graded.len()), dim()),
            ]),
            Line::raw(""),
        ];
        let per_row = ((width as usize).saturating_sub(LABEL) / (GROUP_LEN + 2)).max(1);
        for row in self.graded.chunks(per_row) {
            let mut sent = vec![Span::styled(format!("{:<LABEL$}", "Envoyé"), dim())];
            let mut typed = vec![Span::styled(format!("{:<LABEL$}", "Tapé"), dim())];
            for g in row {
                for (s, t) in &g.cells {
                    let ok = *t == Some(*s);
                    let style = if ok { Style::default().fg(Color::White) } else { bold(ui::ERR) };
                    sent.push(Span::styled(s.to_string(), style));
                    typed.push(match t {
                        Some(c) if ok => Span::styled(c.to_string(), Style::default().fg(ui::OK)),
                        Some(c) => Span::styled(c.to_string(), bold(ui::ERR)),
                        None => Span::styled("_", bold(ui::ERR)),
                    });
                }
                sent.push(Span::raw("  "));
                typed.push(if g.extra > 0 { Span::styled("+ ", bold(ui::ERR)) } else { Span::raw("  ") });
            }
            lines.push(Line::from(sent));
            lines.push(Line::from(typed));
            lines.push(Line::raw(""));
        }
        lines
    }
}

impl Screen for Koch {
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
        let now = Instant::now();
        match self.phase {
            Phase::Ready => {
                let area = ui::frame(f, "Koch", &[("Entrée", "commencer"), ("Échap", "retour")]);
                let chars: String = ctx.chars().iter().map(|c| format!("{c} ")).collect();
                let lines = vec![
                    Line::styled("Prêt ?", bold(ui::ACCENT)),
                    Line::raw(""),
                    Line::raw("Des groupes de 5 caractères vont défiler sans s'arrêter."),
                    Line::raw("Tape ce que tu entends, Espace entre les groupes."),
                    Line::raw("Une lettre t'échappe ? Laisse-la, reprends à la suivante."),
                    Line::raw("La correction arrive à la fin."),
                    Line::raw(""),
                    Line::from(vec![Span::styled("Caractères  ", dim()), Span::raw(chars)]),
                    Line::from(vec![
                        Span::styled("Vitesse     ", dim()),
                        Span::raw(format!(
                            "{:.0} mots/min (effective {:.0}) · {}",
                            ctx.cfg.char_wpm,
                            ctx.cfg.eff_wpm.min(ctx.cfg.char_wpm),
                            ui::mmss(ctx.cfg.session_secs as u64)
                        )),
                    ]),
                ];
                let h = lines.len() as u16 + 2;
                f.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: false }),
                    ui::centered(area, 60, h),
                );
            }
            Phase::Listening => {
                let area = ui::frame(
                    f,
                    "Koch",
                    &[("Espace", "groupe suivant"), ("⌫", "corriger"), ("Échap", "arrêter")],
                );
                let total = ctx.cfg.session_secs as u64;
                let secs = now.saturating_duration_since(self.started).as_secs();
                let left = total.saturating_sub(secs);
                let ratio = if total == 0 { 1.0 } else { secs as f32 / total as f32 };
                let status = if left == 0 {
                    "dernier groupe".to_string()
                } else {
                    format!("reste {}", ui::mmss(left))
                };
                let lines = vec![
                    Line::from(vec![
                        Span::styled(ui::bar(ratio, 20), dim()),
                        Span::styled(
                            format!("  {status} · groupe {}", self.groups_heard(now).max(1)),
                            dim(),
                        ),
                    ]),
                    Line::raw(""),
                    self.input_line(area.width.min(60)),
                    Line::raw(""),
                    Line::styled("Écoute et tape au fil de l'eau. Rien n'est corrigé avant la fin.", dim()),
                ];
                f.render_widget(Paragraph::new(lines), ui::centered(area, 64, 5));
            }
            Phase::Finishing(deadline) => {
                let area = ui::frame(f, "Koch", &[("Entrée", "valider"), ("⌫", "corriger")]);
                let left = deadline.saturating_duration_since(now).as_secs() + 1;
                let lines = vec![
                    Line::styled("Terminé. Finis ta saisie.", bold(ui::ACCENT)),
                    Line::raw(""),
                    self.input_line(area.width.min(60)),
                    Line::raw(""),
                    Line::styled(format!("Entrée pour valider (correction dans {left} s)."), dim()),
                ];
                f.render_widget(Paragraph::new(lines), ui::centered(area, 64, 5));
            }
            Phase::Review => {
                let area = ui::frame(f, "Koch — correction", &[("Entrée", "bilan"), ("↑↓", "défiler")]);
                let lines = self.review_lines(area.width);
                let max = (lines.len() as u16).saturating_sub(area.height);
                self.scroll = self.scroll.min(max);
                f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), area);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    /// Fait avancer la mise en file sans attendre le temps réel : la file
    /// audio est vidée avant chaque tic.
    fn schedule_chars(k: &mut Koch, ctx: &mut Ctx, n: usize) {
        for _ in 0..n {
            ctx.audio.stop();
            let now = Instant::now();
            assert!(matches!(k.tick_at(ctx, now), Nav::Stay));
        }
    }

    fn render_all_sizes(k: &mut Koch, ctx: &Ctx) {
        for (w, h) in [(80, 24), (20, 6), (1, 1)] {
            render_to_string(k, ctx, w, h);
        }
    }

    #[test]
    fn grade_aligns_group_by_group() {
        let sent = vec!["ABCDE".to_string(), "FGHIJ".to_string(), "KLMNO".to_string()];
        let g = grade(&sent, "abcde  fgx  ");
        assert_eq!(g[0].cells.iter().filter(|(s, t)| *t == Some(*s)).count(), 5);
        assert_eq!(
            g[1].cells,
            vec![('F', Some('F')), ('G', Some('G')), ('H', Some('X')), ('I', None), ('J', None)]
        );
        assert!(g[2].cells.iter().all(|(_, t)| t.is_none()));
        assert_eq!(score(&g), (15, 7));
    }

    #[test]
    fn grade_ignores_extra_chars_and_groups() {
        let sent = vec!["ABCDE".to_string()];
        let g = grade(&sent, "ABCDEFG HIJKL");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].extra, 2);
        assert_eq!(score(&g), (5, 5));
        // Une lettre sautée décale le reste du groupe, pas les groupes suivants.
        let sent = vec!["ABCDE".to_string(), "FGHIJ".to_string()];
        assert_eq!(score(&grade(&sent, "ACDE FGHIJ")), (10, 6));
        assert_eq!(score(&grade(&sent, "")), (10, 0));
    }

    #[test]
    fn full_session_records_and_finishes() {
        let mut ctx = Ctx::test();
        ctx.cfg.session_secs = 0; // un seul groupe
        let mut k = Koch::new();
        k.on_enter(&mut ctx);
        render_all_sizes(&mut k, &ctx);
        assert!(render_to_string(&mut k, &ctx, 80, 24).contains("Prêt ?"));
        assert!(matches!(k.on_key(key(KeyCode::Enter), &mut ctx), Nav::Stay));
        schedule_chars(&mut k, &mut ctx, GROUP_LEN + 3);
        assert_eq!(k.groups.len(), 1);
        assert_eq!(k.spans[0].len(), GROUP_LEN);
        assert!(k.scheduled_all);
        render_all_sizes(&mut k, &ctx);
        // Le texte envoyé n'apparaît nulle part pendant l'écoute.
        assert!(matches!(k.phase, Phase::Listening));

        // Tape le groupe avec une faute à la dernière position.
        let sent = k.groups[0].clone();
        for c in sent.chars().take(GROUP_LEN - 1) {
            k.on_key(key(KeyCode::Char(c.to_ascii_lowercase())), &mut ctx);
        }
        k.on_key(key(KeyCode::Char('#')), &mut ctx);
        k.on_key(key(KeyCode::Char('#')), &mut ctx);
        k.on_key(key(KeyCode::Backspace), &mut ctx);
        assert!(ctx.stats.chars.is_empty(), "rien n'est corrigé pendant l'écoute");

        let end = k.last_end().unwrap();
        k.tick_at(&mut ctx, end + Duration::from_millis(1));
        assert!(matches!(k.phase, Phase::Finishing(_)));
        render_all_sizes(&mut k, &ctx);
        assert!(matches!(k.on_key(key(KeyCode::Enter), &mut ctx), Nav::Stay));
        assert!(matches!(k.phase, Phase::Review));
        let seen: u32 = ctx.stats.chars.values().map(|c| c.seen).sum();
        let ok: u32 = ctx.stats.chars.values().map(|c| c.correct).sum();
        assert_eq!((seen, ok), (5, 4));
        render_all_sizes(&mut k, &ctx);
        let s = render_to_string(&mut k, &ctx, 80, 24);
        assert!(s.contains(&sent) && s.contains("Envoyé") && s.contains("80 %"), "{s}");
        k.on_key(key(KeyCode::Down), &mut ctx);
        render_all_sizes(&mut k, &ctx);
        assert!(matches!(k.on_key(key(KeyCode::Enter), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().mode, "Koch");
        assert_eq!(ctx.stats.sessions.last().unwrap().total, 5);
    }

    #[test]
    fn session_continues_until_time_is_up() {
        let mut ctx = Ctx::test();
        ctx.cfg.session_secs = 300;
        let mut k = Koch::new();
        k.on_enter(&mut ctx);
        k.on_key(key(KeyCode::Enter), &mut ctx);
        schedule_chars(&mut k, &mut ctx, GROUP_LEN + 2);
        assert_eq!(k.groups.len(), 2);
        assert!(!k.scheduled_all);
        // Temps écoulé : le groupe en cours se termine, puis plus rien.
        for _ in 0..GROUP_LEN {
            ctx.audio.stop();
            let late = k.started + Duration::from_secs(301);
            if matches!(k.phase, Phase::Listening) {
                k.tick_at(&mut ctx, late);
            }
        }
        assert_eq!(k.groups.len(), 2);
        assert_eq!(k.spans[1].len(), GROUP_LEN);
        assert!(matches!(k.phase, Phase::Finishing(_)));
        // Sans validation, la correction arrive seule après le délai.
        k.tick_at(&mut ctx, Instant::now() + Duration::from_secs(400));
        assert!(matches!(k.phase, Phase::Review));
    }

    #[test]
    fn escape_keeps_only_fully_played_groups() {
        let mut ctx = Ctx::test();
        let mut k = Koch::new();
        k.on_enter(&mut ctx);
        k.on_key(key(KeyCode::Enter), &mut ctx);
        schedule_chars(&mut k, &mut ctx, GROUP_LEN + 2);
        let first_done = k.spans[0].iter().map(|s| s.end).max().unwrap();
        // Le 2e groupe est entamé mais pas fini (seulement 2 caractères en file).
        let now = first_done.max(k.last_end().unwrap()) + Duration::from_millis(1);
        assert!(matches!(k.key_at(key(KeyCode::Esc), &mut ctx, now), Nav::Stay));
        assert_eq!(k.groups.len(), 1);
        assert!(matches!(k.phase, Phase::Finishing(_)));
        assert!(ctx.audio.is_idle());
        k.on_key(key(KeyCode::Char('x')), &mut ctx);
        k.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(matches!(k.on_key(key(KeyCode::Enter), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().total, 5);
    }

    #[test]
    fn escape_before_first_group_goes_back() {
        let mut ctx = Ctx::test();
        let mut k = Koch::new();
        k.on_enter(&mut ctx);
        assert!(matches!(Koch::new().on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        k.on_key(key(KeyCode::Enter), &mut ctx);
        schedule_chars(&mut k, &mut ctx, 2);
        assert!(matches!(k.on_key(key(KeyCode::Esc), &mut ctx), Nav::Back));
        assert!(ctx.stats.chars.is_empty() && ctx.stats.sessions.is_empty());
    }

    #[test]
    fn input_editing() {
        let mut k = Koch::new();
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('a'),
            KeyCode::Char(' '),
            KeyCode::Char(' '),
            KeyCode::Char('b'),
            KeyCode::Char('c'),
            KeyCode::Backspace,
        ] {
            assert!(k.edit_input(code));
        }
        assert!(!k.edit_input(KeyCode::Left));
        assert_eq!(k.input, "A B");
    }
}
