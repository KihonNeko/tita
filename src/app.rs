//! Cadre de l'application : contexte partagé, trait `Screen`, boucle principale.

use crate::audio::Audio;
use crate::config::Config;
use crate::morse::Timing;
use crate::stats::{self, Session, Stats};
use crate::ui::{self, Tap, Zone};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;
use std::time::{Duration, Instant};

/// État partagé par tous les écrans.
pub struct Ctx {
    pub cfg: Config,
    pub stats: Stats,
    pub audio: Audio,
}

/// Bilan d'une session, à passer à `Ctx::finish`.
pub struct Summary {
    /// Nom du mode affiché et enregistré ("Lettres", "Koch", "Flux", ...).
    pub mode: String,
    pub elapsed: Duration,
    /// Caractères présentés / corrects.
    pub total: u32,
    pub correct: u32,
    /// Vitesse utilisée (finale, si elle a varié pendant la session).
    pub timing: Timing,
    /// Ce mode peut-il débloquer de nouveaux caractères ?
    pub can_unlock: bool,
    /// Caractères ratés pendant la session : (caractère, nombre d'erreurs).
    pub misses: Vec<(char, u32)>,
    /// Lignes libres ajoutées au bilan (ex. "Vitesse max : 12 mots/min").
    pub notes: Vec<String>,
}

impl Ctx {
    /// Contexte pour les tests : audio muet, rien n'est écrit sur disque.
    #[cfg(test)]
    pub fn test() -> Ctx {
        let cfg = Config { ephemeral: true, ..Config::default() };
        let stats = Stats { ephemeral: true, ..Stats::default() };
        Ctx { cfg, stats, audio: Audio::new("none", 650.0, 0.0) }
    }

    pub fn timing(&self) -> Timing {
        self.cfg.timing()
    }

    /// Caractères débloqués du parcours actif, dans l'ordre d'apprentissage.
    pub fn chars(&self) -> Vec<char> {
        self.cfg.track.chars(self.stats.unlocked(self.cfg.track))
    }

    /// Réapplique fréquence et volume de la config à la sortie audio.
    pub fn apply_audio(&self) {
        self.audio.set_tone(self.cfg.freq, self.cfg.volume);
    }

    /// Termine une session : enregistre, sauvegarde, débloque si mérité, et
    /// renvoie la navigation vers l'écran de bilan. À appeler en fin de mode :
    /// `return ctx.finish(summary);`
    pub fn finish(&mut self, s: Summary) -> Nav {
        self.audio.stop();
        if s.total == 0 {
            return Nav::Back;
        }
        let session = Session {
            date: stats::today(),
            mode: s.mode.clone(),
            secs: s.elapsed.as_secs() as u32,
            total: s.total,
            correct: s.correct,
            char_wpm: s.timing.char_wpm,
            eff_wpm: s.timing.eff_wpm,
        };
        let acc = session.accuracy();
        self.stats.sessions.push(session);

        let mut new_chars = Vec::new();
        let track = self.cfg.track;
        let before = self.stats.unlocked(track);
        let thr = self.cfg.unlock_threshold;
        let mastered = self.stats.recent_accuracy(&self.chars()).unwrap_or(0.0) >= thr;
        if s.can_unlock && s.total >= 20 && acc >= thr && mastered {
            let after = track.next_unlock(before);
            if after > before {
                self.stats.set_unlocked(track, after);
                new_chars = track.order()[before..after].to_vec();
            }
        }
        let _ = self.stats.save();
        Nav::Replace(Box::new(ui::results::Results::new(s, new_chars)))
    }
}

/// Navigation renvoyée par un écran.
pub enum Nav {
    Stay,
    /// Revenir à l'écran précédent.
    Back,
    /// Empiler un nouvel écran.
    Push(Box<dyn Screen>),
    /// Remplacer l'écran courant.
    Replace(Box<dyn Screen>),
    Quit,
}

pub trait Screen {
    /// Appelé quand l'écran devient actif (à l'ouverture, et au retour d'un
    /// écran empilé par-dessus).
    fn on_enter(&mut self, _ctx: &mut Ctx) {}
    /// Appelé quand l'écran est quitté ou recouvert.
    fn on_leave(&mut self, _ctx: &mut Ctx) {}
    /// Touche appuyée (les répétitions et relâchements ne passent pas ici).
    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav;
    /// Touche relâchée — seulement si l'écran a activé le protocole clavier
    /// étendu du terminal (voir mode émission), ou au doigt sur mobile.
    fn on_key_release(&mut self, _key: KeyEvent, _ctx: &mut Ctx) {}
    /// Élément de liste touché du doigt (zones déclarées avec `ui::tap_item`).
    fn on_item(&mut self, _index: usize, _ctx: &mut Ctx) -> Nav {
        Nav::Stay
    }
    /// Appelé environ toutes les 5 ms : logique temporelle (minuteries, flux).
    fn on_tick(&mut self, _ctx: &mut Ctx) -> Nav {
        Nav::Stay
    }
    fn draw(&mut self, f: &mut Frame, ctx: &Ctx);
}

/// Un doigt posé sur l'écran.
struct Finger {
    id: u64,
    /// Zone touchée à l'appui (`None` : à côté de tout bouton).
    zone: Option<Zone>,
    /// Dernière rangée connue, pour le défilement au doigt.
    row: u16,
}

/// La pile d'écrans et son contexte, sans rien savoir de l'affichage : le
/// terminal (`run`) et la coquille mobile la pilotent de la même façon.
pub struct Shell {
    pub ctx: Ctx,
    stack: Vec<Box<dyn Screen>>,
    fingers: Vec<Finger>,
    quit: bool,
}

impl Shell {
    pub fn new(mut ctx: Ctx, mut first: Box<dyn Screen>) -> Shell {
        first.on_enter(&mut ctx);
        Shell { ctx, stack: vec![first], fingers: Vec::new(), quit: false }
    }

    /// Faux quand l'application doit se fermer.
    pub fn running(&self) -> bool {
        !self.quit && !self.stack.is_empty()
    }

    fn apply(&mut self, nav: Nav) {
        match nav {
            Nav::Stay => return,
            Nav::Quit => {
                self.quit = true;
                return;
            }
            Nav::Back => {
                if let Some(mut s) = self.stack.pop() {
                    s.on_leave(&mut self.ctx);
                }
            }
            Nav::Push(s) => {
                if let Some(top) = self.stack.last_mut() {
                    top.on_leave(&mut self.ctx);
                }
                self.stack.push(s);
            }
            Nav::Replace(s) => {
                if let Some(mut old) = self.stack.pop() {
                    old.on_leave(&mut self.ctx);
                }
                self.stack.push(s);
            }
        }
        self.ctx.audio.set_key(false);
        // Les doigts encore posés appartenaient à l'écran précédent.
        self.fingers.clear();
        if let Some(top) = self.stack.last_mut() {
            top.on_enter(&mut self.ctx);
        }
    }

    /// Touche appuyée. Renvoie vrai si l'écran a changé.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let Some(top) = self.stack.last_mut() else { return false };
        let nav = top.on_key(key, &mut self.ctx);
        let changed = !matches!(nav, Nav::Stay);
        self.apply(nav);
        changed
    }

    pub fn release(&mut self, key: KeyEvent) {
        if let Some(top) = self.stack.last_mut() {
            top.on_key_release(key, &mut self.ctx);
        }
    }

    pub fn tick(&mut self) {
        let Some(top) = self.stack.last_mut() else { return };
        let nav = top.on_tick(&mut self.ctx);
        self.apply(nav);
    }

    /// Dessine l'écran courant et relève ses zones tactiles.
    pub fn draw(&mut self, f: &mut Frame) {
        ui::touch::clear();
        if let Some(top) = self.stack.last_mut() {
            top.draw(f, &self.ctx);
        }
    }

    /// Doigt posé sur la cellule (col, row) de la dernière image dessinée.
    pub fn touch_down(&mut self, id: u64, col: u16, row: u16) {
        self.fingers.retain(|t| t.id != id);
        let zone = ui::touch::hit(col, row);
        self.fingers.push(Finger { id, zone, row });
        match zone.and_then(|z| z.tap) {
            Some(Tap::Key(code)) => {
                self.key(KeyEvent::new(code, KeyModifiers::NONE));
            }
            Some(Tap::Item(i)) => {
                if let Some(top) = self.stack.last_mut() {
                    let nav = top.on_item(i, &mut self.ctx);
                    self.apply(nav);
                }
            }
            None => {}
        }
    }

    /// Doigt déplacé : hors de tout bouton, un glissement vertical fait
    /// défiler (une touche ↑ ou ↓ par rangée parcourue).
    pub fn touch_move(&mut self, id: u64, _col: u16, row: u16) {
        let Some(t) = self.fingers.iter_mut().find(|t| t.id == id) else { return };
        if t.zone.is_some() || row == t.row {
            return;
        }
        let (code, steps) = if row < t.row {
            (KeyCode::Down, t.row - row)
        } else {
            (KeyCode::Up, row - t.row)
        };
        t.row = row;
        for _ in 0..steps {
            if self.key(KeyEvent::new(code, KeyModifiers::NONE)) {
                break;
            }
        }
    }

    /// Doigt levé : relâche la touche qu'il tenait (manipulateur).
    pub fn touch_up(&mut self, id: u64) {
        let Some(i) = self.fingers.iter().position(|t| t.id == id) else { return };
        let t = self.fingers.remove(i);
        if let Some(Tap::Key(code)) = t.zone.and_then(|z| z.tap) {
            self.release(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Zones actuellement tenues par un doigt (retour visuel).
    pub fn pressed(&self) -> Vec<Rect> {
        self.fingers.iter().filter_map(|t| t.zone.filter(|z| z.tap.is_some()).map(|z| z.rect)).collect()
    }

    /// L'application passe en arrière-plan : silence, et tout est enregistré
    /// (sur mobile, le processus peut être tué sans prévenir).
    pub fn pause(&mut self) {
        for t in std::mem::take(&mut self.fingers) {
            if let Some(Tap::Key(code)) = t.zone.and_then(|z| z.tap) {
                self.release(KeyEvent::new(code, KeyModifiers::NONE));
            }
        }
        self.ctx.audio.suspend(true);
        let _ = self.ctx.stats.save();
        let _ = self.ctx.cfg.save();
    }

    /// Retour au premier plan après `pause`.
    pub fn resume(&mut self) {
        self.ctx.audio.suspend(false);
    }

    /// Fermeture : quitte l'écran courant, coupe le son, enregistre.
    pub fn close(&mut self) {
        if let Some(mut s) = self.stack.pop() {
            s.on_leave(&mut self.ctx);
        }
        self.stack.clear();
        self.ctx.audio.stop();
        let _ = self.ctx.stats.save();
        let _ = self.ctx.cfg.save();
    }
}

/// Boucle principale dans le terminal.
pub fn run(ctx: Ctx, first: Box<dyn Screen>) -> Result<()> {
    let mut term = ratatui::init();
    let mut shell = Shell::new(ctx, first);
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    let mut dirty = true;

    let res = (|| -> Result<()> {
        while shell.running() {
            let mut changed = false;
            if event::poll(Duration::from_millis(5))? {
                while !changed && event::poll(Duration::ZERO)? {
                    match event::read()? {
                        Event::Key(k) => {
                            match k.kind {
                                KeyEventKind::Press => {
                                    if k.code == KeyCode::Char('c')
                                        && k.modifiers.contains(KeyModifiers::CONTROL)
                                    {
                                        return Ok(());
                                    }
                                    changed = shell.key(k);
                                }
                                KeyEventKind::Release => shell.release(k),
                                KeyEventKind::Repeat => {}
                            }
                            dirty = true;
                        }
                        Event::Resize(..) => dirty = true,
                        _ => {}
                    }
                }
            }
            if !changed {
                shell.tick();
            }
            if !shell.running() {
                break;
            }
            if dirty || changed || last_draw.elapsed() >= Duration::from_millis(33) {
                term.draw(|f| shell.draw(f))?;
                last_draw = Instant::now();
                dirty = false;
            }
        }
        Ok(())
    })();

    shell.close();
    ratatui::restore();
    res
}

/// Aide de test : rend un écran dans un terminal virtuel et renvoie le texte.
#[cfg(test)]
pub fn render_to_string(screen: &mut dyn Screen, ctx: &Ctx, w: u16, h: u16) -> String {
    use ratatui::backend::TestBackend;
    let mut term = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
    ui::touch::clear();
    term.draw(|f| screen.draw(f, ctx)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..h {
        for x in 0..w {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes;
    use crate::morse;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Dessine la coquille dans une grille de téléphone et renvoie le texte.
    fn frame(shell: &mut Shell, term: &mut Terminal<TestBackend>) -> String {
        term.draw(|f| shell.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// Touche (pose puis lève le doigt) la zone qui envoie `tap`.
    fn tap(shell: &mut Shell, tap: Tap) {
        let zone = ui::touch::zones().into_iter().rev().find(|z| z.tap == Some(tap)).expect("zone tactile");
        shell.touch_down(1, zone.rect.x + zone.rect.width / 2, zone.rect.y + zone.rect.height / 2);
        shell.touch_up(1);
    }

    fn all_screens() -> Vec<Box<dyn Screen>> {
        vec![
            Box::new(ui::menu::Menu::new()),
            Box::new(modes::drill::Drill::new(false)),
            Box::new(modes::drill::Drill::new(true)),
            Box::new(modes::discover::Discover::new(vec!['E', 'T'])),
            Box::new(modes::koch::Koch::new()),
            Box::new(modes::flux::Flux::new()),
            Box::new(modes::words::Words::new()),
            Box::new(modes::headcopy::HeadCopy::new()),
            Box::new(modes::radio::Radio::new()),
            Box::new(modes::send::SendMode::new()),
            Box::new(ui::stats_screen::StatsScreen::new()),
            Box::new(ui::settings::Settings::new()),
        ]
    }

    #[test]
    fn every_screen_fits_a_phone_and_keeps_its_buttons_on_screen() {
        ui::touch::set_mobile(true);
        // Du petit téléphone (34 rangées) au grand, tout débloqué ou non.
        for (h, unlocked) in [(34, 2), (41, 2), (34, morse::TABLE.len()), (41, morse::TABLE.len())] {
            for (i, screen) in all_screens().into_iter().enumerate() {
                let mut ctx = Ctx::test();
                ctx.stats.set_unlocked(ctx.cfg.track, unlocked);
                let mut shell = Shell::new(ctx, screen);
                let mut term = Terminal::new(TestBackend::new(40, h)).unwrap();
                frame(&mut shell, &mut term);
                // Lance la session s'il y en a une, puis redessine.
                shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                frame(&mut shell, &mut term);
                let zones = ui::touch::zones();
                for z in &zones {
                    assert!(
                        z.rect.x + z.rect.width <= 40 && z.rect.y + z.rect.height <= h,
                        "écran {i}, {h} rangées : {z:?} déborde"
                    );
                }
                // Deux boutons visibles ne se recouvrent jamais.
                let faces: Vec<_> = zones.iter().filter(|z| z.face != ui::Face::None).collect();
                for (a, za) in faces.iter().enumerate() {
                    for zb in &faces[a + 1..] {
                        assert!(!za.rect.intersects(zb.rect), "écran {i}, {h} rangées : {za:?} / {zb:?}");
                    }
                }
            }
        }
        ui::touch::set_mobile(false);
    }

    #[test]
    fn a_letters_session_can_be_played_by_touch() {
        ui::touch::set_mobile(true);
        let mut shell = Shell::new(Ctx::test(), Box::new(ui::menu::Menu::new()));
        let mut term = Terminal::new(TestBackend::new(40, 41)).unwrap();
        let s = frame(&mut shell, &mut term);
        assert!(s.contains("Lettres") && !s.contains("Quitter"), "{s}");

        tap(&mut shell, Tap::Item(0));
        let s = frame(&mut shell, &mut term);
        assert!(s.contains("Touche l'écran") && s.contains("‹"), "{s}");
        // Seules les lettres débloquées répondent.
        let active = ui::touch::zones().iter().filter(|z| matches!(z.tap, Some(Tap::Key(KeyCode::Char(_))))).count();
        assert_eq!(active, 2 + 1, "E, T et « rejouer »");

        // Toucher l'écran lance la session ; une mauvaise lettre montre la bonne.
        shell.touch_down(1, 20, 8);
        shell.touch_up(1);
        assert!(frame(&mut shell, &mut term).contains('?'));
        for c in ['E', 'T'] {
            tap(&mut shell, Tap::Key(KeyCode::Char(c)));
            frame(&mut shell, &mut term);
        }
        assert!(shell.ctx.stats.chars.values().map(|c| c.seen).sum::<u32>() >= 1);

        // La flèche de retour termine la session : bilan, puis menu.
        tap(&mut shell, Tap::Key(KeyCode::Esc));
        assert!(frame(&mut shell, &mut term).contains("Bilan"));
        tap(&mut shell, Tap::Key(KeyCode::Enter));
        assert!(frame(&mut shell, &mut term).contains("Lettres sans fin"));
        assert!(shell.running());
        ui::touch::set_mobile(false);
    }

    #[test]
    fn the_straight_key_follows_the_finger() {
        ui::touch::set_mobile(true);
        let mut shell = Shell::new(Ctx::test(), Box::new(modes::send::SendMode::new()));
        let mut term = Terminal::new(TestBackend::new(40, 41)).unwrap();
        let s = frame(&mut shell, &mut term);
        assert!(s.contains("MANIPULATEUR") && s.contains("point") && s.contains("trait"), "{s}");
        let key = ui::touch::zones().into_iter().find(|z| z.tap == Some(Tap::Key(KeyCode::Char(' ')))).unwrap();
        shell.touch_down(7, key.rect.x + 2, key.rect.y + 1);
        assert_eq!(shell.pressed(), vec![key.rect]);
        // Tant que le doigt reste posé, l'écran montre l'appui en cours.
        assert!(frame(&mut shell, &mut term).contains('▮'));
        shell.touch_up(7);
        assert!(shell.pressed().is_empty());
        assert!(!frame(&mut shell, &mut term).contains('▮'));
        ui::touch::set_mobile(false);
    }

    #[test]
    fn dragging_outside_buttons_scrolls() {
        ui::touch::set_mobile(true);
        let mut ctx = Ctx::test();
        ctx.stats.record('E', Some('E'), None);
        let mut shell = Shell::new(ctx, Box::new(ui::stats_screen::StatsScreen::new()));
        let mut term = Terminal::new(TestBackend::new(40, 34)).unwrap();
        frame(&mut shell, &mut term);
        tap(&mut shell, Tap::Item(1));
        let before = frame(&mut shell, &mut term);
        // Le doigt remonte de cinq rangées au milieu du tableau.
        shell.touch_down(3, 20, 20);
        shell.touch_move(3, 20, 15);
        shell.touch_up(3);
        let after = frame(&mut shell, &mut term);
        assert_ne!(before, after, "le tableau a défilé");
        ui::touch::set_mobile(false);
    }

    #[test]
    fn menu_renders() {
        let ctx = Ctx::test();
        let mut m = ui::menu::Menu::new();
        let s = render_to_string(&mut m, &ctx, 100, 30);
        assert!(s.contains("Lettres") && s.contains("Flux"));
    }
}
