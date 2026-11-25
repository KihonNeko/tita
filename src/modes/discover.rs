//! Découverte — À IMPLÉMENTER.

use crate::app::{Ctx, Nav, Screen};
use crate::ui;
use crossterm::event::KeyEvent;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct Discover;

impl Discover {
    pub fn new(_chars: Vec<char>) -> Self {
        Discover
    }
}

impl Screen for Discover {
    fn on_key(&mut self, _key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        Nav::Back
    }

    fn draw(&mut self, f: &mut Frame, _ctx: &Ctx) {
        let area = ui::frame(f, "Découverte", &[("Échap", "retour")]);
        f.render_widget(Paragraph::new("Bientôt disponible."), area);
    }
}
