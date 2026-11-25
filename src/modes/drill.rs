//! Lettres — À IMPLÉMENTER.

use crate::app::{Ctx, Nav, Screen};
use crate::ui;
use crossterm::event::KeyEvent;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct Drill;

impl Drill {
    pub fn new(_endless: bool) -> Self {
        Drill
    }
}

impl Screen for Drill {
    fn on_key(&mut self, _key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        Nav::Back
    }

    fn draw(&mut self, f: &mut Frame, _ctx: &Ctx) {
        let area = ui::frame(f, "Lettres", &[("Échap", "retour")]);
        f.render_widget(Paragraph::new("Bientôt disponible."), area);
    }
}
