//! Réglages : liste modifiable au clavier, enregistrée à la sortie.

use crate::app::{Ctx, Nav, Screen};
use crate::config::Lang;
use crate::curriculum::Track;
use crate::i18n::t;
use crate::ui::{self, bold, dim};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    UiLang,
    Track,
    CharWpm,
    EffWpm,
    Freq,
    Volume,
    Session,
    Threshold,
    ShowPattern,
    Lang,
    Unlocked,
}

const ITEMS: &[Item] = &[
    Item::UiLang,
    Item::Track,
    Item::CharWpm,
    Item::EffWpm,
    Item::Freq,
    Item::Volume,
    Item::Session,
    Item::Threshold,
    Item::ShowPattern,
    Item::Lang,
    Item::Unlocked,
];

pub const CHAR_WPM: (f32, f32) = (10.0, 40.0);
pub const EFF_WPM_MIN: f32 = 4.0;
pub const FREQ: (f32, f32) = (400.0, 1000.0);
pub const SESSION_MIN: (i32, i32) = (1, 30);
pub const THRESHOLD_PCT: (i32, i32) = (70, 100);

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::UiLang => t("Langue de l'interface", "Interface language"),
            Item::Track => t("Parcours", "Track"),
            Item::CharWpm => t("Vitesse des caractères", "Character speed"),
            Item::EffWpm => t("Vitesse effective", "Effective speed"),
            Item::Freq => t("Tonalité", "Tone"),
            Item::Volume => "Volume",
            Item::Session => t("Durée de session", "Session length"),
            Item::Threshold => t("Seuil de déblocage", "Unlock threshold"),
            Item::ShowPattern => t("Motif après une erreur", "Pattern after an error"),
            Item::Lang => t("Langue des mots", "Word list language"),
            Item::Unlocked => t("Caractères débloqués", "Unlocked characters"),
        }
    }

    fn help(self) -> &'static str {
        match self {
            Item::UiLang => t(
                "Langue des menus, des écrans et de l'aide. Le changement est immédiat. \
                 Les listes de mots ont leur propre réglage : « Langue des mots ».",
                "Language of the menus, screens and help. The change applies at once. \
                 Word lists have their own setting: \"Word list language\".",
            ),
            Item::Track => t(
                "Ordre d'apprentissage. Leçons : par petits groupes de lettres (E T, puis I M S O...). \
                 Koch : un caractère à la fois, dans l'ordre classique K M U R E S... \
                 Chaque parcours garde sa propre progression.",
                "Learning order. Lessons: small groups of letters (E T, then I M S O...). \
                 Koch: one character at a time, in the classic order K M U R E S... \
                 Each track keeps its own progress.",
            ),
            Item::CharWpm => t(
                "Vitesse à laquelle chaque caractère est joué (durée des points et des traits). \
                 Garde-la élevée, 18 à 20 mots/min : tu apprends le son global du caractère \
                 au lieu de compter les points.",
                "Speed at which each character is played (length of dots and dashes). \
                 Keep it high, 18 to 20 wpm: you learn the overall sound of the character \
                 instead of counting dots.",
            ),
            Item::EffWpm => t(
                "Vitesse d'ensemble, silences compris. Plus basse que la vitesse des caractères, \
                 elle allonge seulement les pauses entre caractères et entre mots (méthode \
                 Farnsworth) : les caractères sonnent pareil, tu as plus de temps pour répondre. \
                 Remonte-la peu à peu.",
                "Overall speed, silences included. Lower than the character speed, it only \
                 lengthens the pauses between characters and between words (Farnsworth \
                 method): characters sound the same, you get more time to answer. \
                 Raise it little by little.",
            ),
            Item::Freq => t(
                "Hauteur du son, en hertz. Choisis une note confortable ; 600 à 700 Hz est courant.",
                "Pitch of the sound, in hertz. Pick a comfortable note; 600 to 700 Hz is common.",
            ),
            Item::Volume => t(
                "Volume du son. Appuie sur Espace pour écouter le résultat.",
                "Sound volume. Press Space to hear the result.",
            ),
            Item::Session => t(
                "Durée des sessions chronométrées. Mieux vaut court et tous les jours que long et rare.",
                "Length of timed sessions. Short and daily beats long and rare.",
            ),
            Item::Threshold => t(
                "Précision à atteindre en fin de session pour débloquer de nouveaux caractères. \
                 90 % est la règle habituelle.",
                "Accuracy to reach at the end of a session to unlock new characters. \
                 90 % is the usual rule.",
            ),
            Item::ShowPattern => t(
                "Afficher le motif (· −) du caractère après une erreur. Désactive-le pour ne travailler qu'à l'oreille.",
                "Show the character's pattern (· −) after an error. Turn it off to work by ear only.",
            ),
            Item::Lang => t(
                "Langue des listes de mots utilisées dans les modes Mots et Copie de tête.",
                "Language of the word lists used in the Words and Head copy modes.",
            ),
            Item::Unlocked => t(
                "Nombre de caractères du parcours actif déjà débloqués. ← en retire un, → en ajoute un : \
                 utile si tu connais déjà une partie de l'alphabet.",
                "Number of characters of the active track already unlocked. ← removes one, → adds one: \
                 useful if you already know part of the alphabet.",
            ),
        }
    }
}

pub struct Settings {
    sel: usize,
}

impl Settings {
    pub fn new() -> Self {
        Settings { sel: 0 }
    }

    fn item(&self) -> Item {
        ITEMS[self.sel.min(ITEMS.len() - 1)]
    }

    fn value(item: Item, ctx: &Ctx) -> String {
        let c = &ctx.cfg;
        // Chaque langue est nommée dans sa propre langue.
        let lang_name = |l: Lang| match l {
            Lang::Fr => "Français".to_string(),
            Lang::En => "English".to_string(),
        };
        match item {
            Item::UiLang => lang_name(c.ui_lang),
            Item::Track => c.track.label().to_string(),
            Item::CharWpm => crate::tr!("{:.0} mots/min", "{:.0} wpm", c.char_wpm),
            Item::EffWpm => crate::tr!("{:.0} mots/min", "{:.0} wpm", c.eff_wpm),
            Item::Freq => format!("{:.0} Hz", c.freq),
            Item::Volume => format!("{:.0} %", c.volume * 100.0),
            Item::Session => format!("{} min", minutes(c.session_secs)),
            Item::Threshold => format!("{:.0} %", c.unlock_threshold * 100.0),
            Item::ShowPattern => {
                if c.show_pattern_on_error { t("oui", "yes") } else { t("non", "no") }.to_string()
            }
            Item::Lang => lang_name(c.lang),
            Item::Unlocked => {
                format!("{} / {}", ctx.stats.unlocked(c.track), c.track.order().len())
            }
        }
    }

    /// Modifie le réglage `item` d'un cran (`dir` = -1 ou +1).
    fn adjust(item: Item, dir: i32, ctx: &mut Ctx) {
        let dir = dir.signum();
        let c = &mut ctx.cfg;
        match item {
            Item::UiLang => {
                c.ui_lang = match c.ui_lang {
                    Lang::Fr => Lang::En,
                    Lang::En => Lang::Fr,
                };
                // Effet immédiat : tout l'écran change de langue au prochain dessin.
                crate::i18n::set(c.ui_lang);
            }
            Item::Track => {
                c.track = match c.track {
                    Track::Lessons => Track::Koch,
                    Track::Koch => Track::Lessons,
                }
            }
            Item::CharWpm => {
                c.char_wpm = (c.char_wpm.round() + dir as f32).clamp(CHAR_WPM.0, CHAR_WPM.1);
                c.eff_wpm = c.eff_wpm.round().clamp(EFF_WPM_MIN, c.char_wpm);
            }
            Item::EffWpm => {
                // Si la vitesse des caractères vient d'un fichier hors bornes.
                let max = c.char_wpm.max(EFF_WPM_MIN);
                c.eff_wpm = (c.eff_wpm.round() + dir as f32).clamp(EFF_WPM_MIN, max);
            }
            Item::Freq => {
                let steps = (c.freq / 10.0).round() as i32 + dir;
                c.freq = (steps as f32 * 10.0).clamp(FREQ.0, FREQ.1);
                ctx.apply_audio();
            }
            Item::Volume => {
                let pct = ((c.volume * 100.0).round() as i32 + 5 * dir).clamp(0, 100);
                c.volume = pct as f32 / 100.0;
                ctx.apply_audio();
            }
            Item::Session => {
                let m = (minutes(c.session_secs) as i32 + dir).clamp(SESSION_MIN.0, SESSION_MIN.1);
                c.session_secs = m as u32 * 60;
            }
            Item::Threshold => {
                let pct = ((c.unlock_threshold * 100.0).round() as i32 + 5 * dir)
                    .clamp(THRESHOLD_PCT.0, THRESHOLD_PCT.1);
                c.unlock_threshold = pct as f32 / 100.0;
            }
            Item::ShowPattern => c.show_pattern_on_error = !c.show_pattern_on_error,
            Item::Lang => {
                c.lang = match c.lang {
                    Lang::Fr => Lang::En,
                    Lang::En => Lang::Fr,
                }
            }
            Item::Unlocked => {
                let track = c.track;
                let n = ctx.stats.unlocked(track) as i64 + dir as i64;
                // `set_unlocked` borne entre le minimum du parcours et son total.
                ctx.stats.set_unlocked(track, n.max(0) as usize);
            }
        }
    }

    fn test_sound(ctx: &Ctx) {
        ctx.audio.stop();
        ctx.apply_audio();
        ctx.audio.play("VVV", ctx.timing());
    }
}

fn minutes(secs: u32) -> u32 {
    ((secs + 30) / 60).max(1)
}

impl Screen for Settings {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        ctx.apply_audio();
    }

    fn on_leave(&mut self, ctx: &mut Ctx) {
        ctx.audio.stop();
        ctx.apply_audio();
        let _ = ctx.cfg.save();
        let _ = ctx.stats.save();
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let n = ITEMS.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = (self.sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1) % n,
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = n - 1,
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => Self::adjust(self.item(), -1, ctx),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') | KeyCode::Enter => {
                Self::adjust(self.item(), 1, ctx)
            }
            KeyCode::Char(' ') | KeyCode::Char('t') => Self::test_sound(ctx),
            // L'enregistrement se fait dans `on_leave`, quelle que soit la sortie.
            KeyCode::Esc | KeyCode::Char('q') => return Nav::Back,
            _ => {}
        }
        Nav::Stay
    }

    fn on_item(&mut self, index: usize, _ctx: &mut Ctx) -> Nav {
        if index < ITEMS.len() {
            self.sel = index;
        }
        Nav::Stay
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        let area = ui::frame(
            f,
            t("Réglages", "Settings"),
            &[
                ("↑↓", t("choisir", "select")),
                ("←→", t("modifier", "change")),
                (t("Espace", "Space"), t("tester le son", "test the sound")),
                (t("Échap", "Esc"), t("enregistrer et revenir", "save and go back")),
            ],
        );
        let current = self.item();
        let mobile = ui::mobile();
        let mut lines: Vec<Line> = Vec::new();
        for (i, item) in ITEMS.iter().enumerate() {
            let selected = *item == current;
            let value = Self::value(*item, ctx);
            if mobile {
                // Deux rangées par réglage (nom, valeur) : une cible assez
                // haute pour le doigt, qui tient sur un écran étroit.
                let y = area.y + 2 * i as u16;
                if y + 2 <= area.y + area.height {
                    let full = f.area();
                    ui::touch::item(Rect { x: full.x, y, width: full.width, height: 2 }, i);
                }
                if selected {
                    lines.push(Line::styled(format!(" ▶ {}", item.label()), bold(ui::ACCENT)));
                    lines.push(Line::from(vec![
                        Span::styled("     ◀ ", dim()),
                        Span::styled(value, bold(Color::White)),
                        Span::styled(" ▶", dim()),
                    ]));
                } else {
                    lines.push(Line::raw(format!("   {}", item.label())));
                    lines.push(Line::styled(format!("       {value}"), dim()));
                }
                continue;
            }
            let label = format!("{:<26}", item.label());
            if selected {
                lines.push(Line::from(vec![
                    Span::styled(" ▶ ", bold(ui::ACCENT)),
                    Span::styled(label, bold(ui::ACCENT)),
                    Span::styled("◀ ", dim()),
                    Span::styled(value, bold(Color::White)),
                    Span::styled(" ▶", dim()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("   "),
                    Span::raw(label),
                    Span::raw("  "),
                    Span::raw(value),
                ]));
            }
        }

        // Liste du parcours actif : débloqués en clair, le reste en gris.
        let track = ctx.cfg.track;
        let unlocked = ctx.stats.unlocked(track);
        let order = track.order();
        let per_line = ((area.width as usize).saturating_sub(5) / 2).max(1);
        // Sur mobile la place est comptée : la liste n'apparaît que sur son réglage.
        let show_list = !mobile || current == Item::Unlocked;
        if mobile {
            lines.push(Line::raw(""));
        }
        for (row, chunk) in order.chunks(per_line).enumerate().filter(|_| show_list) {
            let mut spans = vec![Span::raw("     ")];
            for (j, c) in chunk.iter().enumerate() {
                let i = row * per_line + j;
                let style = if i < unlocked { bold(ui::OK) } else { dim() };
                spans.push(Span::styled(c.to_string(), style));
                if i + 1 == unlocked {
                    spans.push(Span::styled("│", bold(ui::ACCENT)));
                } else {
                    spans.push(Span::raw(" "));
                }
            }
            lines.push(Line::from(spans));
        }
        if !mobile || show_list {
            lines.push(Line::raw(""));
        }
        lines.push(Line::styled(current.help(), dim()));
        if !ctx.audio.available {
            lines.push(Line::styled(
                t("Pas de sortie audio : le test du son est muet.", "No audio output: the sound test is silent."),
                bold(ui::ERR),
            ));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;
    use crossterm::event::KeyModifiers;

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn assert_in_bounds(ctx: &Ctx) {
        let c = &ctx.cfg;
        assert!((CHAR_WPM.0..=CHAR_WPM.1).contains(&c.char_wpm), "char {}", c.char_wpm);
        assert!(c.eff_wpm >= EFF_WPM_MIN && c.eff_wpm <= c.char_wpm, "eff {} / {}", c.eff_wpm, c.char_wpm);
        assert!((FREQ.0..=FREQ.1).contains(&c.freq), "freq {}", c.freq);
        assert!((0.0..=1.0).contains(&c.volume), "volume {}", c.volume);
        assert!((60..=1800).contains(&c.session_secs) && c.session_secs % 60 == 0);
        assert!((0.70..=1.0).contains(&c.unlock_threshold), "seuil {}", c.unlock_threshold);
        let n = ctx.stats.unlocked(c.track);
        assert!(n >= c.track.initial() && n <= c.track.order().len());
    }

    #[test]
    fn every_setting_stays_in_bounds() {
        let mut ctx = Ctx::test();
        let mut s = Settings::new();
        s.on_enter(&mut ctx);
        for i in 0..ITEMS.len() {
            s.sel = i;
            for key in [KeyCode::Right, KeyCode::Left, KeyCode::Right] {
                for _ in 0..150 {
                    assert!(matches!(s.on_key(k(key), &mut ctx), Nav::Stay));
                    assert_in_bounds(&ctx);
                }
            }
        }
        // Le réglage de langue a été basculé un nombre pair de fois.
        assert_eq!(ctx.cfg.ui_lang, Lang::Fr);
        crate::i18n::set(Lang::Fr);
    }

    #[test]
    fn reaches_exact_limits_with_exact_steps() {
        let mut ctx = Ctx::test();
        let step = |item, dir, n, ctx: &mut Ctx| {
            for _ in 0..n {
                Settings::adjust(item, dir, ctx);
            }
        };
        step(Item::CharWpm, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.char_wpm, 40.0);
        step(Item::EffWpm, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.eff_wpm, 40.0);
        step(Item::EffWpm, -1, 100, &mut ctx);
        assert_eq!(ctx.cfg.eff_wpm, 4.0);
        step(Item::CharWpm, -1, 100, &mut ctx);
        assert_eq!(ctx.cfg.char_wpm, 10.0);

        ctx.cfg.freq = 650.0;
        step(Item::Freq, 1, 1, &mut ctx);
        assert_eq!(ctx.cfg.freq, 660.0);
        step(Item::Freq, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.freq, 1000.0);
        step(Item::Freq, -1, 100, &mut ctx);
        assert_eq!(ctx.cfg.freq, 400.0);

        ctx.cfg.volume = 0.6;
        step(Item::Volume, 1, 1, &mut ctx);
        assert!((ctx.cfg.volume - 0.65).abs() < 1e-6);
        step(Item::Volume, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.volume, 1.0);
        step(Item::Volume, -1, 100, &mut ctx);
        assert_eq!(ctx.cfg.volume, 0.0);

        step(Item::Session, 1, 1, &mut ctx);
        assert_eq!(ctx.cfg.session_secs, 360);
        step(Item::Session, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.session_secs, 1800);
        step(Item::Session, -1, 100, &mut ctx);
        assert_eq!(ctx.cfg.session_secs, 60);

        step(Item::Threshold, -1, 1, &mut ctx);
        assert!((ctx.cfg.unlock_threshold - 0.85).abs() < 1e-6);
        step(Item::Threshold, -1, 100, &mut ctx);
        assert!((ctx.cfg.unlock_threshold - 0.70).abs() < 1e-6);
        step(Item::Threshold, 1, 100, &mut ctx);
        assert_eq!(ctx.cfg.unlock_threshold, 1.0);
    }

    #[test]
    fn effective_speed_follows_char_speed_down() {
        let mut ctx = Ctx::test();
        ctx.cfg.char_wpm = 20.0;
        ctx.cfg.eff_wpm = 20.0;
        for expected in [19.0, 18.0, 17.0] {
            Settings::adjust(Item::CharWpm, -1, &mut ctx);
            assert_eq!((ctx.cfg.char_wpm, ctx.cfg.eff_wpm), (expected, expected));
        }
        // Remonter la vitesse des caractères ne touche pas à la vitesse effective.
        Settings::adjust(Item::CharWpm, 1, &mut ctx);
        assert_eq!((ctx.cfg.char_wpm, ctx.cfg.eff_wpm), (18.0, 17.0));
        // La vitesse effective ne dépasse pas celle des caractères.
        for _ in 0..5 {
            Settings::adjust(Item::EffWpm, 1, &mut ctx);
        }
        assert_eq!(ctx.cfg.eff_wpm, 18.0);
    }

    #[test]
    fn out_of_range_config_is_brought_back() {
        let mut ctx = Ctx::test();
        ctx.cfg.char_wpm = 5.0;
        ctx.cfg.eff_wpm = 3.0;
        Settings::adjust(Item::EffWpm, 1, &mut ctx);
        assert!(ctx.cfg.eff_wpm >= EFF_WPM_MIN);
        Settings::adjust(Item::CharWpm, 1, &mut ctx);
        ctx.cfg.session_secs = 10;
        Settings::adjust(Item::Session, -1, &mut ctx);
        ctx.cfg.session_secs = 99_999;
        Settings::adjust(Item::Session, 1, &mut ctx);
        ctx.cfg.freq = 5000.0;
        Settings::adjust(Item::Freq, -1, &mut ctx);
        ctx.cfg.volume = 3.0;
        Settings::adjust(Item::Volume, 1, &mut ctx);
        ctx.cfg.unlock_threshold = 0.2;
        Settings::adjust(Item::Threshold, 1, &mut ctx);
        assert_in_bounds(&ctx);
    }

    #[test]
    fn toggles_and_manual_unlock() {
        let mut ctx = Ctx::test();
        Settings::adjust(Item::Track, 1, &mut ctx);
        assert_eq!(ctx.cfg.track, Track::Koch);
        Settings::adjust(Item::Lang, -1, &mut ctx);
        assert_eq!(ctx.cfg.lang, Lang::En);
        Settings::adjust(Item::ShowPattern, 1, &mut ctx);
        assert!(!ctx.cfg.show_pattern_on_error);

        // Le déblocage manuel ne concerne que le parcours actif.
        for _ in 0..3 {
            Settings::adjust(Item::Unlocked, 1, &mut ctx);
        }
        assert_eq!(ctx.stats.unlocked(Track::Koch), 5);
        assert_eq!(ctx.stats.unlocked(Track::Lessons), 2);
        assert_eq!(ctx.chars(), vec!['K', 'M', 'U', 'R', 'E']);
        for _ in 0..100 {
            Settings::adjust(Item::Unlocked, -1, &mut ctx);
        }
        assert_eq!(ctx.stats.unlocked(Track::Koch), 2);
        for _ in 0..100 {
            Settings::adjust(Item::Unlocked, 1, &mut ctx);
        }
        assert_eq!(ctx.stats.unlocked(Track::Koch), Track::Koch.order().len());
    }

    #[test]
    fn navigation_test_sound_and_exit() {
        let mut ctx = Ctx::test();
        let mut s = Settings::new();
        s.on_enter(&mut ctx);
        s.on_key(k(KeyCode::Up), &mut ctx);
        assert_eq!(s.item(), Item::Unlocked);
        s.on_key(k(KeyCode::Down), &mut ctx);
        assert_eq!(s.item(), Item::UiLang);
        s.on_key(k(KeyCode::Down), &mut ctx);
        assert_eq!(s.item(), Item::Track);

        assert!(ctx.audio.is_idle());
        s.on_key(k(KeyCode::Char(' ')), &mut ctx);
        assert!(!ctx.audio.is_idle(), "le test « VVV » est mis en file");
        // Espace ne modifie aucun réglage.
        assert_eq!(ctx.cfg.track, Track::Lessons);
        assert_eq!(ctx.cfg.ui_lang, Lang::Fr);

        assert!(matches!(s.on_key(k(KeyCode::Esc), &mut ctx), Nav::Back));
        s.on_leave(&mut ctx);
        assert!(ctx.audio.is_idle());
    }

    #[test]
    fn renders_every_item_at_any_size() {
        let mut ctx = Ctx::test();
        let mut s = Settings::new();
        let out = render_to_string(&mut s, &ctx, 80, 24);
        for item in ITEMS {
            assert!(out.contains(item.label()), "{} absent :\n{out}", item.label());
        }
        assert!(out.contains("20 mots/min") && out.contains("650 Hz") && out.contains("2 / 62"), "{out}");
        for i in 0..ITEMS.len() {
            s.sel = i;
            for (w, h) in [(80, 24), (20, 6), (1, 1), (200, 60)] {
                render_to_string(&mut s, &ctx, w, h);
            }
        }
        // L'aide de la vitesse effective explique la différence.
        s.sel = ITEMS.iter().position(|i| *i == Item::EffWpm).unwrap();
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Farnsworth"), "{out}");
        ctx.stats.set_unlocked(ctx.cfg.track, 41);
        render_to_string(&mut s, &ctx, 80, 24);
    }

    /// Mots présents uniquement dans la version française de l'écran.
    const FRENCH_ONLY: &[&str] = &[
        "Réglages", "Langue", "Parcours", "Vitesse", "Tonalité", "Durée", "Seuil", "Motif", "Caractères",
        "mots/min", "oui", "choisir", "modifier", "Espace", "Échap", "enregistrer",
    ];

    #[test]
    fn interface_language_is_first_and_switches_at_once() {
        let mut ctx = Ctx::test();
        let mut s = Settings::new();
        s.on_enter(&mut ctx);
        assert_eq!(ITEMS[0], Item::UiLang);
        assert_eq!(s.item(), Item::UiLang);
        assert_eq!(ctx.cfg.ui_lang, Lang::Fr);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Réglages") && out.contains("Langue de l'interface"), "{out}");
        assert!(out.contains("Langue des mots") && out.contains("Le changement est immédiat"), "{out}");
        assert!(!out.contains("Interface language") && !out.contains("Settings"), "{out}");

        // → : l'écran entier passe à l'anglais, sans autre action.
        s.on_key(k(KeyCode::Right), &mut ctx);
        assert_eq!(ctx.cfg.ui_lang, Lang::En);
        assert!(crate::i18n::is_en());
        // La langue des listes de mots est un réglage distinct, inchangé.
        assert_eq!(ctx.cfg.lang, Lang::Fr);
        let out = render_to_string(&mut s, &ctx, 80, 24);
        for en in ["Settings", "Interface language", "English", "Word list language", "The change applies at once", "20 wpm", "yes", "save and go back"] {
            assert!(out.contains(en), "{en} absent :\n{out}");
        }
        for fr in FRENCH_ONLY {
            assert!(!out.contains(fr), "{fr} présent :\n{out}");
        }

        // ← : retour au français, tout aussi immédiat.
        s.on_key(k(KeyCode::Left), &mut ctx);
        assert_eq!(ctx.cfg.ui_lang, Lang::Fr);
        assert!(!crate::i18n::is_en());
        let out = render_to_string(&mut s, &ctx, 80, 24);
        assert!(out.contains("Réglages") && out.contains("Langue de l'interface"), "{out}");
        assert!(!out.contains("Interface language"), "{out}");
        crate::i18n::set(Lang::Fr);
    }

    #[test]
    fn renders_in_english() {
        let ctx = Ctx::test();
        crate::i18n::set(Lang::En);
        let mut s = Settings::new();
        for i in 0..ITEMS.len() {
            s.sel = i;
            let out = render_to_string(&mut s, &ctx, 80, 24);
            for item in ITEMS {
                assert!(out.contains(item.label()), "{} absent :\n{out}", item.label());
            }
            for fr in FRENCH_ONLY {
                assert!(!out.contains(fr), "{fr} présent (réglage {i}) :\n{out}");
            }
            // L'aide (dernier paragraphe) n'est pas coupée par le bas de l'écran.
            let help = s.item().help();
            let last = help.split_whitespace().last().unwrap();
            assert!(out.contains(last), "aide tronquée (réglage {i}) :\n{out}");
            for (w, h) in [(20, 6), (1, 1), (200, 60)] {
                render_to_string(&mut s, &ctx, w, h);
            }
        }
        assert!(render_to_string(&mut s, &ctx, 80, 24).contains("Unlocked characters"));
        crate::i18n::set(Lang::Fr);
    }
}
