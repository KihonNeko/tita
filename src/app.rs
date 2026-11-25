//! Cadre de l'application : contexte partagé, trait `Screen`, boucle principale.

use crate::audio::Audio;
use crate::config::Config;
use crate::morse::Timing;
use crate::stats::{self, Session, Stats};
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
    /// étendu du terminal (voir mode émission).
    fn on_key_release(&mut self, _key: KeyEvent, _ctx: &mut Ctx) {}
    /// Appelé environ toutes les 5 ms : logique temporelle (minuteries, flux).
    fn on_tick(&mut self, _ctx: &mut Ctx) -> Nav {
        Nav::Stay
    }
    fn draw(&mut self, f: &mut Frame, ctx: &Ctx);
}

pub fn run(mut ctx: Ctx, first: Box<dyn Screen>) -> Result<()> {
    let mut term = ratatui::init();
    let mut stack: Vec<Box<dyn Screen>> = vec![first];
    stack[0].on_enter(&mut ctx);
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    let mut dirty = true;

    let res = (|| -> Result<()> {
        loop {
            let mut navs: Vec<Nav> = Vec::new();
            if event::poll(Duration::from_millis(5))? {
                while event::poll(Duration::ZERO)? {
                    match event::read()? {
                        Event::Key(k) => {
                            let top = stack.last_mut().unwrap();
                            match k.kind {
                                KeyEventKind::Press => {
                                    if k.code == KeyCode::Char('c')
                                        && k.modifiers.contains(KeyModifiers::CONTROL)
                                    {
                                        navs.push(Nav::Quit);
                                    } else {
                                        navs.push(top.on_key(k, &mut ctx));
                                    }
                                }
                                KeyEventKind::Release => top.on_key_release(k, &mut ctx),
                                KeyEventKind::Repeat => {}
                            }
                            dirty = true;
                        }
                        Event::Resize(..) => dirty = true,
                        _ => {}
                    }
                    if navs.iter().any(|n| !matches!(n, Nav::Stay)) {
                        break;
                    }
                }
            }
            if navs.iter().all(|n| matches!(n, Nav::Stay)) {
                navs.push(stack.last_mut().unwrap().on_tick(&mut ctx));
            }
            for nav in navs {
                match nav {
                    Nav::Stay => continue,
                    Nav::Quit => return Ok(()),
                    Nav::Back => {
                        if let Some(mut s) = stack.pop() {
                            s.on_leave(&mut ctx);
                        }
                        if stack.is_empty() {
                            return Ok(());
                        }
                    }
                    Nav::Push(s) => {
                        stack.last_mut().unwrap().on_leave(&mut ctx);
                        stack.push(s);
                    }
                    Nav::Replace(s) => {
                        if let Some(mut old) = stack.pop() {
                            old.on_leave(&mut ctx);
                        }
                        stack.push(s);
                    }
                }
                ctx.audio.set_key(false);
                stack.last_mut().unwrap().on_enter(&mut ctx);
                dirty = true;
                break;
            }
            if dirty || last_draw.elapsed() >= Duration::from_millis(33) {
                let top = stack.last_mut().unwrap();
                term.draw(|f| top.draw(f, &ctx))?;
                last_draw = Instant::now();
                dirty = false;
            }
        }
    })();

    if let Some(mut s) = stack.pop() {
        s.on_leave(&mut ctx);
    }
    ctx.audio.stop();
    ratatui::restore();
    let _ = ctx.stats.save();
    let _ = ctx.cfg.save();
    res
}

/// Aide de test : rend un écran dans un terminal virtuel et renvoie le texte.
#[cfg(test)]
pub fn render_to_string(screen: &mut dyn Screen, ctx: &Ctx, w: u16, h: u16) -> String {
    use ratatui::backend::TestBackend;
    let mut term = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
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

    #[test]
    fn menu_renders() {
        let ctx = Ctx::test();
        let mut m = ui::menu::Menu::new();
        let s = render_to_string(&mut m, &ctx, 100, 30);
        assert!(s.contains("Lettres") && s.contains("Flux"));
    }
}
