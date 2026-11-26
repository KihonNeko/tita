//! Menu principal.

use crate::app::{Ctx, Nav, Screen};
use crate::curriculum::{self, Track};
use crate::modes;
use crate::i18n::t;
use crate::tr;
use crate::ui::{self, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// (nom fr, nom en, description fr, description en)
const ITEMS: &[(&str, &str, &str, &str)] = &[
    ("Lettres", "Letters",
     "Une lettre joue, tu tapes la touche. Le cœur de l'apprentissage.",
     "A letter plays, you press its key. The core of the training."),
    ("Lettres sans fin", "Endless letters",
     "Pareil, sans limite de durée : travaille le temps de réaction.",
     "Same, with no time limit: trains reaction time."),
    ("Découverte", "Discover",
     "Réécouter les caractères débloqués avec leur motif.",
     "Listen again to the unlocked characters with their pattern."),
    ("Koch", "Koch",
     "Groupes de 5 caractères à pleine vitesse, score à la fin.",
     "Groups of 5 characters at full speed, graded at the end."),
    ("Flux", "Stream",
     "Flux continu qui ne t'attend pas, vitesse auto-adaptative.",
     "A continuous stream that does not wait for you, self-adjusting speed."),
    ("Mots", "Words",
     "Mots fréquents composés des lettres déjà apprises.",
     "Common words made of the letters you already know."),
    ("Copie de tête", "Head copy",
     "Le mot entier joue, tu ne tapes qu'à la fin.",
     "The whole word plays, you only type at the end."),
    ("Radio", "Radio",
     "Indicatifs, codes Q, abréviations, échanges simulés.",
     "Callsigns, Q codes, abbreviations, simulated contacts."),
    ("Émission", "Sending",
     "Manipule au clavier, l'appli décode ton rythme.",
     "Key with the keyboard, the app decodes your rhythm."),
    ("Statistiques", "Statistics",
     "Précision et temps de réaction par caractère, confusions.",
     "Accuracy and reaction time per character, confusions."),
    ("Réglages", "Settings",
     "Vitesses, tonalité, volume, parcours, durée de session, langue.",
     "Speeds, tone, volume, learning order, session length, language."),
    ("Quitter", "Quit", "", ""),
];

/// Descriptions courtes pour l'écran étroit du mobile (fr, en), dans l'ordre
/// de `ITEMS`.
const SHORT: &[(&str, &str)] = &[
    ("Une lettre joue, tu tapes la touche.", "A letter plays, you press its key."),
    ("Sans limite de durée : réflexes.", "No time limit: trains reflexes."),
    ("Réécouter les caractères débloqués.", "Hear the unlocked characters again."),
    ("Groupes de 5 à pleine vitesse.", "Groups of 5 at full speed."),
    ("Flux continu, vitesse adaptative.", "Continuous stream, adaptive speed."),
    ("Mots faits des lettres apprises.", "Words made of letters you know."),
    ("Le mot joue, tu tapes à la fin.", "The word plays, then you type it."),
    ("Indicatifs, codes Q, échanges.", "Callsigns, Q codes, contacts."),
    ("Manipule, l'appli décode ton rythme.", "Key it, the app decodes your rhythm."),
    ("Précision, réaction, confusions.", "Accuracy, reaction time, confusions."),
    ("Vitesses, son, parcours, langue.", "Speeds, sound, track, language."),
];

pub struct Menu {
    sel: usize,
}

impl Menu {
    pub fn new() -> Self {
        Menu { sel: 0 }
    }

    fn open(&self, ctx: &Ctx) -> Nav {
        let s: Box<dyn Screen> = match self.sel {
            0 => Box::new(modes::drill::Drill::new(false)),
            1 => Box::new(modes::drill::Drill::new(true)),
            2 => Box::new(modes::discover::Discover::new(ctx.chars())),
            3 => Box::new(modes::koch::Koch::new()),
            4 => Box::new(modes::flux::Flux::new()),
            5 => Box::new(modes::words::Words::new()),
            6 => Box::new(modes::headcopy::HeadCopy::new()),
            7 => Box::new(modes::radio::Radio::new()),
            8 => Box::new(modes::send::SendMode::new()),
            9 => Box::new(ui::stats_screen::StatsScreen::new()),
            10 => Box::new(ui::settings::Settings::new()),
            _ => return Nav::Quit,
        };
        Nav::Push(s)
    }

    /// Menu tactile : chaque mode est une large ligne à toucher. Pas de
    /// « Quitter » : sur mobile on sort avec le geste de retour.
    fn draw_mobile(&mut self, f: &mut Frame, ctx: &Ctx) {
        let area = ui::frame(f, t("Le Morse à l'oreille", "Morse code by ear"), &[]);
        let track = ctx.cfg.track;
        let unlocked = ctx.stats.unlocked(track);
        let total = track.order().len();
        let place = match track {
            Track::Lessons => {
                let i = curriculum::lesson_index(unlocked);
                tr!("Leçon {} — {}", "Lesson {} — {}", i + 1, curriculum::LESSONS[i].title())
            }
            Track::Koch => tr!("Koch — {unlocked} caractères", "Koch — {unlocked} characters"),
        };
        let mut status = vec![
            Line::styled(place, bold(ui::ACCENT)),
            Line::styled(format!("{} {unlocked}/{total}", ui::bar(unlocked as f32 / total as f32, 20)), dim()),
            Line::styled(
                tr!(
                    "{:.0}/{:.0} mots/min · série {} j · {} auj.",
                    "{:.0}/{:.0} wpm · {}-day streak · {} today",
                    ctx.cfg.char_wpm,
                    ctx.cfg.eff_wpm,
                    ctx.stats.streak(),
                    ui::mmss(ctx.stats.secs_today() as u64)
                ),
                dim(),
            ),
        ];
        if !ctx.audio.available {
            status.push(Line::styled(t("⚠ pas de sortie audio", "⚠ no audio output"), bold(ui::ERR)));
        }
        let status = Paragraph::new(status).wrap(ratatui::widgets::Wrap { trim: true });
        let status_h = (status.line_count(area.width) as u16).min(area.height);
        f.render_widget(status, ratatui::layout::Rect { height: status_h, ..area });

        // Une carte par mode : nom et description si la place le permet,
        // nom seul sur un petit écran.
        let n = SHORT.len() as u16;
        let room = area.height.saturating_sub(status_h + 1);
        let pitch = (room / n).clamp(2, 3);
        let rows = pitch - 1;
        for (i, (fr, en)) in SHORT.iter().enumerate() {
            let y = area.y + status_h + 1 + i as u16 * pitch;
            if y + rows > area.y + area.height {
                break;
            }
            let row = ui::card(f, area, y, rows, i, false);
            let name = Span::styled(t(ITEMS[i].0, ITEMS[i].1), bold(ratatui::style::Color::White));
            let mut lines = vec![ui::split_line(name, Span::styled("›", dim()), row.width)];
            if rows > 1 {
                lines.push(Line::styled(t(fr, en), dim()));
            }
            f.render_widget(Paragraph::new(lines), row);
        }
    }
}

impl Screen for Menu {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
        ctx.apply_audio();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = (self.sel + ITEMS.len() - 1) % ITEMS.len(),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1) % ITEMS.len(),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = ITEMS.len() - 1,
            KeyCode::Enter | KeyCode::Char(' ') => return self.open(ctx),
            KeyCode::Esc | KeyCode::Char('q') => return Nav::Quit,
            _ => {}
        }
        Nav::Stay
    }

    fn on_item(&mut self, index: usize, ctx: &mut Ctx) -> Nav {
        if index >= SHORT.len() {
            return Nav::Stay;
        }
        self.sel = index;
        self.open(ctx)
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        if ui::mobile() {
            return self.draw_mobile(f, ctx);
        }
        let area = ui::frame(
            f,
            t("Apprendre le Morse à l'oreille", "Learn Morse code by ear"),
            &[
                ("↑↓", t("choisir", "choose")),
                (t("Entrée", "Enter"), t("lancer", "start")),
                ("q", t("quitter", "quit")),
            ],
        );
        let [status, _, list, desc] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(ITEMS.len() as u16),
            Constraint::Length(2),
        ])
        .areas(area);

        let track = ctx.cfg.track;
        let unlocked = ctx.stats.unlocked(track);
        let total = track.order().len();
        let chars: String = ctx.chars().iter().map(|c| format!("{c} ")).collect();
        let place = match track {
            Track::Lessons => {
                let i = curriculum::lesson_index(unlocked);
                tr!("Leçon {} — {}", "Lesson {} — {}", i + 1, curriculum::LESSONS[i].title())
            }
            Track::Koch => tr!("Koch — {unlocked} caractères", "Koch — {unlocked} characters"),
        };
        let mut l3 = vec![Span::styled(
            tr!(
                "{:.0}/{:.0} mots/min · série {} j · aujourd'hui {}",
                "{:.0}/{:.0} wpm · {}-day streak · today {}",
                ctx.cfg.char_wpm,
                ctx.cfg.eff_wpm,
                ctx.stats.streak(),
                ui::mmss(ctx.stats.secs_today() as u64)
            ),
            dim(),
        )];
        if !ctx.audio.available {
            l3.push(Span::styled(if cfg!(target_os = "linux") {
                    t(
                        "   ⚠ pas de sortie audio (pw-cat / aplay introuvable)",
                        "   ⚠ no audio output (pw-cat / aplay not found)",
                    )
                } else {
                    t("   ⚠ pas de sortie audio", "   ⚠ no audio output")
                }, bold(ui::ERR)));
        }
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(place, bold(ui::ACCENT)),
                    Span::styled(format!("   {} {unlocked}/{total}", ui::bar(unlocked as f32 / total as f32, 20)), dim()),
                ]),
                Line::from(vec![Span::styled(t("Débloqués : ", "Unlocked: "), dim()), Span::raw(chars)]),
                Line::from(l3),
            ]),
            status,
        );

        let lines: Vec<Line> = ITEMS
            .iter()
            .enumerate()
            .map(|(i, (fr, en, _, _))| {
                let name = t(fr, en);
                if i == self.sel {
                    Line::styled(format!(" ▶ {name}"), bold(ui::ACCENT))
                } else {
                    Line::raw(format!("   {name}"))
                }
            })
            .collect();
        f.render_widget(Paragraph::new(lines), list);
        f.render_widget(Paragraph::new(Line::styled(t(ITEMS[self.sel].2, ITEMS[self.sel].3), dim())), desc);
    }
}
