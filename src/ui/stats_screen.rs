//! Statistiques (lecture seule) : vue d'ensemble, tableau par caractère,
//! dernières sessions.

use crate::app::{Ctx, Nav, Screen};
use crate::ui::{self, acc_color, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

const TABS: &[&str] = &["Vue d'ensemble", "Caractères", "Sessions"];
/// Largeur d'une cellule du tableau par caractère : `K █████ 100% 9999× 1234ms`.
const CELL_W: usize = 25;
const BAR_W: usize = 5;
const MAX_COLS: usize = 4;
const LAST_SESSIONS: usize = 10;
const TOP_CONFUSIONS: usize = 8;

pub struct StatsScreen {
    tab: usize,
    scroll: usize,
}

impl StatsScreen {
    pub fn new() -> Self {
        StatsScreen { tab: 0, scroll: 0 }
    }
}

/// Durée lisible : `45 s`, `12 min 05 s`, `3 h 07 min`.
fn human(secs: u64) -> String {
    if secs >= 3600 {
        format!("{} h {:02} min", secs / 3600, secs % 3600 / 60)
    } else if secs >= 60 {
        format!("{} min {:02} s", secs / 60, secs % 60)
    } else {
        format!("{secs} s")
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// Complète `spans` avec des espaces jusqu'à `width` colonnes.
fn pad(spans: &mut Vec<Span<'static>>, used: usize, width: usize) {
    if width > used {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
}

fn overview(ctx: &Ctx) -> Vec<Line<'static>> {
    let st = &ctx.stats;
    let track = ctx.cfg.track;
    let unlocked = st.unlocked(track);
    let total = track.order().len();
    let total_secs: u64 = st.sessions.iter().map(|s| s.secs as u64).sum();
    let row = |label: &str, value: String| {
        Line::from(vec![Span::styled(format!("{label:<22}"), dim()), Span::styled(value, bold(Color::White))])
    };
    let mut lines = vec![
        row("Série", plural(st.streak() as u64, "jour", "jours")),
        row("Aujourd'hui", human(st.secs_today() as u64)),
        row("Temps total", human(total_secs)),
        row("Sessions", st.sessions.len().to_string()),
        Line::from(vec![
            Span::styled(format!("{:<22}", "Caractères débloqués"), dim()),
            Span::styled(format!("{unlocked} / {total}"), bold(Color::White)),
            Span::styled(
                format!("  {}  {}", ui::bar(unlocked as f32 / total.max(1) as f32, 20), track.label()),
                dim(),
            ),
        ]),
    ];
    if let Some(acc) = st.recent_accuracy(&ctx.chars()) {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<22}", "Précision récente"), dim()),
            Span::styled(format!("{:.0} %", acc * 100.0), bold(acc_color(acc))),
            Span::styled("  (caractères débloqués)", dim()),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Principales confusions", bold(ui::ACCENT)));
    let conf = st.top_confusions(TOP_CONFUSIONS);
    if conf.is_empty() {
        lines.push(Line::styled("Aucune confusion enregistrée.", dim()));
    }
    for (sent, typed, n) in conf {
        lines.push(Line::from(vec![
            Span::styled(format!("  {sent}"), bold(Color::White)),
            Span::styled(" pris pour ", dim()),
            Span::styled(typed.to_string(), bold(ui::ERR)),
            Span::styled(format!("  ×{n}"), dim()),
        ]));
    }
    lines
}

/// Une cellule du tableau par caractère, de largeur `CELL_W`.
fn char_cell(c: char, locked: bool, ctx: &Ctx) -> Vec<Span<'static>> {
    let stat = ctx.stats.char(c).filter(|s| s.seen > 0);
    let Some(s) = stat else {
        let text = format!("{c} {}    —     —      —", "·".repeat(BAR_W));
        return vec![Span::styled(text, dim())];
    };
    let acc = s.recent_accuracy().unwrap_or(0.0);
    let color = acc_color(acc);
    let name = if locked { dim() } else { bold(Color::White) };
    let rt = if s.rt_ms > 0.0 {
        format!("{:>4.0}ms", s.rt_ms.min(9999.0))
    } else {
        "     —".to_string()
    };
    vec![
        Span::styled(format!("{c} "), name),
        Span::styled(ui::bar(acc, BAR_W), Style::default().fg(color)),
        Span::styled(format!(" {:>3.0}%", acc * 100.0), Style::default().fg(color)),
        Span::raw(format!(" {:>4}×", s.seen.min(9999))),
        Span::styled(format!(" {rt}"), dim()),
    ]
}

/// Nombre de colonnes du tableau pour une largeur donnée.
fn columns(width: usize) -> usize {
    ((width + 1) / (CELL_W + 1)).clamp(1, MAX_COLS)
}

impl StatsScreen {
    fn chars_table(&mut self, ctx: &Ctx, area: Rect) -> Vec<Line<'static>> {
        let track = ctx.cfg.track;
        let order = track.order();
        let unlocked = ctx.stats.unlocked(track);
        let cols = columns(area.width as usize);
        let col_w = (area.width as usize / cols).clamp(CELL_W + 1, CELL_W + 5);
        let rows = order.len().div_ceil(cols);
        // En-tête + légende : deux lignes hors tableau.
        let visible = (area.height as usize).saturating_sub(2).max(1);
        self.scroll = self.scroll.min(rows.saturating_sub(visible));

        let head = format!("  {:<10} {:>5} {:>6}", "précision", "vu", "réact.");
        let mut header = Vec::new();
        for _ in 0..cols {
            header.push(Span::styled(head.clone(), dim()));
            pad(&mut header, CELL_W, col_w);
        }
        let mut lines = vec![Line::from(header)];
        for r in self.scroll..(self.scroll + visible).min(rows) {
            let mut spans = Vec::new();
            for col in 0..cols {
                // Remplissage colonne par colonne : l'ordre d'apprentissage se lit de haut en bas.
                let Some(&c) = order.get(col * rows + r) else { break };
                spans.extend(char_cell(c, col * rows + r >= unlocked, ctx));
                pad(&mut spans, CELL_W, col_w);
            }
            lines.push(Line::from(spans));
        }
        let more = if rows > visible { "↑↓ pour faire défiler · " } else { "" };
        lines.push(Line::styled(
            format!("{more}Précision : 30 dernières réponses · en gris : jamais vu"),
            dim(),
        ));
        lines
    }
}

fn sessions(ctx: &Ctx) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        format!("{:<10}  {:<18} {:>6}  {:>9}  {}", "Date", "Mode", "Durée", "Précision", "Vitesse"),
        dim(),
    )];
    for s in ctx.stats.sessions.iter().rev().take(LAST_SESSIONS) {
        let acc = s.accuracy();
        let mode: String = s.mode.chars().take(18).collect();
        lines.push(Line::from(vec![
            Span::raw(format!("{:<10}  {:<18} {:>6}  ", s.date, mode, ui::mmss(s.secs as u64))),
            Span::styled(format!("{:>7.0} %", acc * 100.0), Style::default().fg(acc_color(acc))),
            Span::styled(format!("  {:.0}/{:.0} mots/min", s.char_wpm, s.eff_wpm), dim()),
        ]));
    }
    if ctx.stats.sessions.is_empty() {
        lines.push(Line::styled("Aucune session terminée pour l'instant.", dim()));
    } else if ctx.stats.sessions.len() > LAST_SESSIONS {
        lines.push(Line::styled(
            format!("… et {} plus anciennes.", ctx.stats.sessions.len() - LAST_SESSIONS),
            dim(),
        ));
    }
    lines
}

impl Screen for StatsScreen {
    fn on_key(&mut self, key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        let n = TABS.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return Nav::Back,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.tab = (self.tab + 1) % n;
                self.scroll = 0;
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.tab = (self.tab + n - 1) % n;
                self.scroll = 0;
            }
            KeyCode::Char(c @ '1'..='3') => {
                self.tab = c as usize - '1' as usize;
                self.scroll = 0;
            }
            // Le défilement est borné au moment du dessin.
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1).min(1000),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10).min(1000),
            KeyCode::PageUp | KeyCode::Home => self.scroll = 0,
            _ => {}
        }
        Nav::Stay
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let empty = ctx.stats.sessions.is_empty() && ctx.stats.chars.values().all(|c| c.seen == 0);
        if empty {
            let area = ui::frame(f, "Statistiques", &[("Échap", "retour")]);
            let track = ctx.cfg.track;
            let lines = vec![
                Line::styled("Pas encore de statistiques.", bold(ui::ACCENT)),
                Line::raw(""),
                Line::raw("Lance une session de « Lettres » depuis le menu : ta précision, tes temps de réaction, tes confusions et tes sessions apparaîtront ici."),
                Line::raw(""),
                Line::styled(
                    format!(
                        "Parcours {} : {} caractères débloqués sur {}.",
                        track.label(),
                        ctx.stats.unlocked(track),
                        track.order().len()
                    ),
                    dim(),
                ),
            ];
            let w = area.width.min(64);
            let h = area.height.min(8);
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), ui::centered(area, w, h));
            return;
        }

        let area = ui::frame(
            f,
            "Statistiques",
            &[("Tab ←→", "onglet"), ("↑↓", "défiler"), ("Échap", "retour")],
        );
        let [tabs, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
        let mut spans = Vec::new();
        for (i, name) in TABS.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  │  ", dim()));
            }
            let style = if i == self.tab { bold(ui::ACCENT) } else { dim() };
            spans.push(Span::styled(name.to_string(), style));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), tabs);

        let lines = match self.tab {
            0 => overview(ctx),
            1 => self.chars_table(ctx, body),
            _ => sessions(ctx),
        };
        f.render_widget(Paragraph::new(lines), body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;
    use crate::curriculum::Track;
    use crate::stats::{self, Session};
    use crossterm::event::KeyModifiers;
    use std::time::Duration;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn filled() -> Ctx {
        let mut ctx = Ctx::test();
        let order = ctx.cfg.track.order();
        ctx.stats.set_unlocked(ctx.cfg.track, 10);
        for (i, c) in order.iter().enumerate().take(12) {
            for n in 0..20 {
                let ok = n % (i + 2) != 0;
                let typed = if ok { *c } else { 'H' };
                ctx.stats.record(*c, Some(typed), Some(Duration::from_millis(400 + 50 * i as u64)));
            }
        }
        for _ in 0..12 {
            ctx.stats.record('S', Some('H'), None);
        }
        for i in 0..13u32 {
            ctx.stats.sessions.push(Session {
                date: stats::today(),
                mode: if i == 12 { "Flux".into() } else { "Lettres".into() },
                secs: 300,
                total: 100,
                correct: 80 + i,
                char_wpm: 20.0,
                eff_wpm: 8.0,
            });
        }
        ctx
    }

    #[test]
    fn empty_shows_welcome() {
        let ctx = Ctx::test();
        let mut s = StatsScreen::new();
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Pas encore de statistiques"), "{out}");
        assert!(out.contains("Lettres"));
        for (w, h) in [(20, 6), (1, 1), (200, 60)] {
            render_to_string(&mut s, &ctx, w, h);
        }
    }

    #[test]
    fn overview_when_filled() {
        let mut ctx = filled();
        let mut s = StatsScreen::new();
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(!out.contains("Pas encore"));
        assert!(out.contains("Série") && out.contains("1 jour"), "{out}");
        assert!(out.contains("1 h 05 min"), "{out}");
        assert!(out.contains("10 / 41"), "{out}");
        assert!(out.contains("S pris pour H"), "{out}");
        assert!(out.contains("×16"), "{out}");
        assert!(matches!(s.on_key(k(KeyCode::Esc), &mut ctx), Nav::Back));
    }

    #[test]
    fn char_table_shows_all_41_in_80x24() {
        let mut ctx = filled();
        let mut s = StatsScreen::new();
        s.on_key(k(KeyCode::Tab), &mut ctx);
        for track in [Track::Lessons, Track::Koch] {
            ctx.cfg.track = track;
            let out = render_to_string(&mut s, &ctx, 80, 24);
            // Chaque caractère du parcours ouvre une cellule (suivi d'une barre ou de points).
            for c in track.order() {
                let found = out.lines().any(|l| {
                    let cs: Vec<char> = l.chars().collect();
                    cs.windows(3).enumerate().any(|(i, w)| {
                        (i == 0 || cs[i - 1] == ' ')
                            && w[0] == c
                            && w[1] == ' '
                            && matches!(w[2], '█' | '░' | '·')
                    })
                });
                assert!(found, "{c} absent ({track:?}) :\n{out}");
            }
            assert!(out.contains("précision"));
            assert!(!out.contains("défiler ") || !out.contains("↑↓ pour faire défiler"), "{out}");
        }
        ctx.cfg.track = Track::Lessons;
        let out = render_to_string(&mut s, &ctx, 80, 24);
        // E : 20 réponses, une sur deux juste, 400 ms.
        assert!(out.contains("E ███░░  50%   20×  400ms") || out.contains("E ██░░░  50%   20×  400ms"), "{out}");
    }

    #[test]
    fn char_table_scrolls_when_narrow() {
        let mut ctx = filled();
        let mut s = StatsScreen::new();
        s.on_key(k(KeyCode::Char('2')), &mut ctx);
        let out = render_to_string(&mut s, &ctx, 30, 12);
        assert!(out.contains("E "), "{out}");
        for _ in 0..500 {
            s.on_key(k(KeyCode::Down), &mut ctx);
        }
        let out = render_to_string(&mut s, &ctx, 30, 12);
        // Dernier caractère du parcours visible, défilement borné.
        assert!(out.lines().any(|l| l.trim_start().starts_with("= ")), "{out}");
        assert!(s.scroll < 41);
        assert_eq!(columns(78), 3);
        assert_eq!(columns(10), 1);
        assert_eq!(columns(500), MAX_COLS);
    }

    #[test]
    fn sessions_tab_lists_last_ten() {
        let mut ctx = filled();
        let mut s = StatsScreen::new();
        s.on_key(k(KeyCode::Left), &mut ctx);
        assert_eq!(s.tab, 2);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Flux") && out.contains("05:00") && out.contains("20/8 mots/min"), "{out}");
        assert_eq!(out.matches(&stats::today()).count(), 10, "{out}");
        assert!(out.contains("92 %") && !out.contains("82 %"), "{out}");
        assert!(out.contains("3 plus anciennes"), "{out}");
    }

    #[test]
    fn renders_at_any_size() {
        let mut ctx = filled();
        let mut s = StatsScreen::new();
        for _ in 0..TABS.len() {
            for (w, h) in [(80, 24), (20, 6), (1, 1), (200, 60), (27, 5)] {
                render_to_string(&mut s, &ctx, w, h);
            }
            s.on_key(k(KeyCode::Down), &mut ctx);
            s.on_key(k(KeyCode::Right), &mut ctx);
        }
        assert_eq!(s.tab, 0);
    }

    #[test]
    fn human_durations() {
        assert_eq!(human(45), "45 s");
        assert_eq!(human(725), "12 min 05 s");
        assert_eq!(human(3600 * 3 + 7 * 60 + 12), "3 h 07 min");
    }
}
