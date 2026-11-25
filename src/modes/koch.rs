//! Koch — À IMPLÉMENTER.

use crate::app::{Ctx, Nav, Screen};
use crate::ui;
use crossterm::event::KeyEvent;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct Koch;

impl Koch {
    pub fn new() -> Self {
        Koch
    }
}

impl Screen for Koch {
    fn on_key(&mut self, _key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        Nav::Back
    }

    fn draw(&mut self, f: &mut Frame, _ctx: &Ctx) {
        let area = ui::frame(f, "Koch", &[("Échap", "retour")]);
        f.render_widget(Paragraph::new("Bientôt disponible."), area);
    }
}
