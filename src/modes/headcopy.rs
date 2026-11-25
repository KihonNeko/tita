//! Copie de tête : le mot entier joue, la saisie n'est ouverte qu'ensuite.
//!
//! L'écran est celui de `words.rs` (`Kind::HeadCopy`) ; ce module porte la
//! progression de longueur propre à ce mode.

use crate::app::{Ctx, Nav, Screen};
use crate::modes::words::{Kind, Words};
use crossterm::event::KeyEvent;
use ratatui::Frame;

/// Longueur maximale des mots en début de session.
pub const START_LEN: usize = 3;
pub const MIN_LEN: usize = 2;
pub const MAX_LEN: usize = 8;
/// Réussites d'affilée nécessaires pour allonger les mots.
const STREAK: u32 = 3;

/// Nouvelle (longueur max, série) après un mot réussi ou raté.
pub fn adapt(max_len: usize, streak: u32, ok: bool) -> (usize, u32) {
    if !ok {
        return (max_len.saturating_sub(1).max(MIN_LEN), 0);
    }
    if streak + 1 >= STREAK {
        ((max_len + 1).min(MAX_LEN), 0)
    } else {
        (max_len, streak + 1)
    }
}

/// Indices des mots à proposer pour une longueur max donnée, d'après leurs
/// longueurs : ceux proches de la limite, sinon tous ceux qui ne la dépassent
/// pas. Vide si aucun mot n'est assez court.
pub fn candidates(lens: &[usize], max_len: usize) -> Vec<usize> {
    let within = |lo: usize| -> Vec<usize> {
        (0..lens.len()).filter(|i| (lo..=max_len).contains(&lens[*i])).collect()
    };
    let near = within(max_len.saturating_sub(1).max(MIN_LEN));
    if near.is_empty() { within(0) } else { near }
}

pub struct HeadCopy(Words);

impl HeadCopy {
    pub fn new() -> Self {
        HeadCopy(Words::with(Kind::HeadCopy, false))
    }
}

impl Screen for HeadCopy {
    fn on_enter(&mut self, ctx: &mut Ctx) {
        self.0.on_enter(ctx)
    }

    fn on_leave(&mut self, ctx: &mut Ctx) {
        self.0.on_leave(ctx)
    }

    fn on_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        self.0.on_key(key, ctx)
    }

    fn on_tick(&mut self, ctx: &mut Ctx) -> Nav {
        self.0.on_tick(ctx)
    }

    fn draw(&mut self, f: &mut Frame, ctx: &Ctx) {
        self.0.draw(f, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render_to_string;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn type_text(h: &mut HeadCopy, ctx: &mut Ctx, s: &str) {
        for c in s.chars() {
            h.on_key(key(KeyCode::Char(c)), ctx);
        }
    }

    fn started(unlocked: usize) -> (HeadCopy, Ctx) {
        let mut ctx = Ctx::test();
        ctx.stats.set_unlocked(ctx.cfg.track, unlocked);
        let mut h = HeadCopy::new();
        h.on_enter(&mut ctx);
        (h, ctx)
    }

    #[test]
    fn length_grows_after_three_and_shrinks_on_failure() {
        assert_eq!(adapt(3, 0, true), (3, 1));
        assert_eq!(adapt(3, 1, true), (3, 2));
        assert_eq!(adapt(3, 2, true), (4, 0));
        assert_eq!(adapt(4, 2, false), (3, 0));
        assert_eq!(adapt(2, 1, false), (2, 0));
        assert_eq!(adapt(8, 2, true), (8, 0));
    }

    #[test]
    fn candidates_stay_near_the_limit() {
        let lens = [2, 3, 4, 5, 8, 3];
        assert_eq!(candidates(&lens, 3), vec![0, 1, 5]);
        assert_eq!(candidates(&lens, 5), vec![2, 3]);
        // Rien près de la limite : tout ce qui ne la dépasse pas.
        assert_eq!(candidates(&lens, 7), vec![0, 1, 2, 3, 5]);
        assert_eq!(candidates(&[5, 6], 3), Vec::<usize>::new());
    }

    #[test]
    fn input_is_locked_while_the_word_plays() {
        let (mut h, mut ctx) = started(26);
        let word = h.0.current_text();
        assert!(word.len() <= START_LEN, "{word}");
        let s = render_to_string(&mut h, &ctx, 80, 24);
        assert!(s.contains("Écoute…") && !s.contains(&word), "{s}");
        // Tant que le son joue, ni saisie ni validation.
        type_text(&mut h, &mut ctx, &word);
        h.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(!h.0.in_feedback());
        h.0.skip_wait(&mut ctx);
        h.on_key(key(KeyCode::Enter), &mut ctx);
        assert!(h.0.in_feedback(), "la saisie verrouillée n'a rien retenu");
        assert_eq!(ctx.stats.sessions.len(), 0);
        let c = word.chars().next().unwrap();
        assert_eq!(ctx.stats.char(c).unwrap().correct, 0);
    }

    #[test]
    fn typing_works_once_the_sound_is_over() {
        let (mut h, mut ctx) = started(26);
        for round in 0..3 {
            let word = h.0.current_text();
            assert!(word.len() <= START_LEN, "{word}");
            h.0.skip_wait(&mut ctx);
            let s = render_to_string(&mut h, &ctx, 80, 24);
            assert!(!s.contains("Écoute…"));
            type_text(&mut h, &mut ctx, &word);
            h.on_key(key(KeyCode::Enter), &mut ctx);
            assert!(h.0.in_feedback());
            assert_eq!(h.0.max_len(), if round == 2 { START_LEN + 1 } else { START_LEN });
            h.on_key(key(KeyCode::Enter), &mut ctx);
        }
        // Un échec raccourcit.
        h.0.skip_wait(&mut ctx);
        h.on_key(key(KeyCode::Enter), &mut ctx);
        assert_eq!(h.0.max_len(), START_LEN);
        assert!(matches!(h.on_key(key(KeyCode::Esc), &mut ctx), Nav::Replace(_)));
        assert_eq!(ctx.stats.sessions.last().unwrap().mode, "Copie de tête");
    }

    #[test]
    fn works_with_two_characters_and_small_terminals() {
        let (mut h, mut ctx) = started(2);
        for _ in 0..30 {
            let word = h.0.current_text();
            assert!(word.chars().all(|c| c == 'E' || c == 'T') && word.len() >= 2, "{word}");
            render_to_string(&mut h, &ctx, 20, 6);
            h.0.skip_wait(&mut ctx);
            type_text(&mut h, &mut ctx, &word);
            h.on_key(key(KeyCode::Enter), &mut ctx);
            render_to_string(&mut h, &ctx, 80, 24);
            h.on_key(key(KeyCode::Enter), &mut ctx);
        }
    }
}
