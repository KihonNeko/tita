//! La vue : l'application, sa grille de caractères, et la correspondance
//! entre pixels de l'écran et cellules de la grille.

use crate::render::{Canvas, Renderer};
use tita::app::Shell;
use tita::ratatui::backend::TestBackend;
use tita::ratatui::buffer::Buffer;
use tita::ratatui::layout::Rect;
use tita::ratatui::Terminal;
use tita::ui::Zone;

/// Colonnes visées sur un téléphone tenu droit.
const COLUMNS: f32 = 40.0;
/// Largeur maximale d'une cellule, en pixels indépendants de la densité :
/// au-delà (tablette), on gagne des colonnes plutôt que du corps.
const MAX_CELL_DP: f32 = 11.5;

/// Marges à laisser libres autour de la grille (barres système), en pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: usize,
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
}

/// Ce qui a été peint la dernière fois : sert à ne repeindre qu'au changement.
#[derive(PartialEq)]
struct Painted {
    buffer: Buffer,
    zones: Vec<Zone>,
    pressed: Vec<Rect>,
}

pub struct View {
    pub shell: Shell,
    term: Terminal<TestBackend>,
    renderer: Renderer,
    /// Coin haut gauche de la grille dans la fenêtre, en pixels.
    origin: (usize, usize),
    zones: Vec<Zone>,
    painted: Option<Painted>,
}

impl View {
    pub fn new(shell: Shell) -> View {
        View {
            shell,
            term: Terminal::new(TestBackend::new(40, 40)).expect("terminal virtuel"),
            renderer: Renderer::new(),
            origin: (0, 0),
            zones: Vec::new(),
            painted: None,
        }
    }

    /// Adapte la grille à une fenêtre de `w` × `h` pixels. `scale` : pixels
    /// par pixel indépendant de la densité.
    pub fn layout(&mut self, w: usize, h: usize, scale: f32, insets: Insets) {
        let avail_w = w.saturating_sub(insets.left + insets.right).max(1);
        let avail_h = h.saturating_sub(insets.top + insets.bottom).max(1);
        let cell_dp = (avail_w as f32 / scale.max(0.1) / COLUMNS).min(MAX_CELL_DP);
        self.renderer.set_cell_width((cell_dp * scale).floor() as usize);
        let (cw, ch) = (self.renderer.cell_w, self.renderer.cell_h);
        let cols = (avail_w / cw).clamp(1, 400);
        let rows = (avail_h / ch).clamp(1, 400);
        self.origin = (insets.left + (avail_w - cols * cw) / 2, insets.top + (avail_h - rows * ch) / 2);
        self.term.backend_mut().resize(cols as u16, rows as u16);
        self.painted = None;
    }

    /// Taille de la grille : (colonnes, rangées).
    pub fn grid(&self) -> (u16, u16) {
        let area = self.term.backend().buffer().area;
        (area.width, area.height)
    }

    /// Cellule sous le pixel (x, y) ; bornée à la grille, pour qu'un doigt
    /// posé dans la marge touche quand même la cellule voisine.
    pub fn cell_at(&self, x: f64, y: f64) -> (u16, u16) {
        let (cols, rows) = self.grid();
        let col = ((x - self.origin.0 as f64) / self.renderer.cell_w as f64).floor();
        let row = ((y - self.origin.1 as f64) / self.renderer.cell_h as f64).floor();
        (col.clamp(0.0, cols as f64 - 1.0) as u16, row.clamp(0.0, rows as f64 - 1.0) as u16)
    }

    pub fn touch_down(&mut self, id: u64, x: f64, y: f64) {
        let (col, row) = self.cell_at(x, y);
        self.shell.touch_down(id, col, row);
    }

    pub fn touch_move(&mut self, id: u64, x: f64, y: f64) {
        let (col, row) = self.cell_at(x, y);
        self.shell.touch_move(id, col, row);
    }

    pub fn touch_up(&mut self, id: u64) {
        self.shell.touch_up(id);
    }

    /// Dessine l'écran courant dans la grille. Renvoie vrai s'il diffère de
    /// ce qui est affiché (il faut alors repeindre).
    pub fn compose(&mut self) -> bool {
        let shell = &mut self.shell;
        if self.term.draw(|f| shell.draw(f)).is_err() {
            return false;
        }
        self.zones = tita::ui::touch::zones();
        match &self.painted {
            Some(p) => {
                p.buffer != *self.term.backend().buffer() || p.zones != self.zones || p.pressed != self.shell.pressed()
            }
            None => true,
        }
    }

    /// Peint la dernière grille composée dans `px` (`w` × `h`, 0x00RRGGBB).
    pub fn paint(&mut self, px: &mut [u32], w: usize, h: usize) {
        if px.len() < w * h {
            return;
        }
        let buffer = self.term.backend().buffer().clone();
        let pressed = self.shell.pressed();
        let mut canvas = Canvas { px, w, h };
        self.renderer.draw(&mut canvas, &buffer, &self.zones, &pressed, self.origin.0, self.origin.1);
        self.painted = Some(Painted { buffer, zones: self.zones.clone(), pressed });
    }

    /// Le texte de la grille, ligne par ligne (tests et diagnostics).
    pub fn text(&self) -> String {
        let buf = self.term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tita::app::Ctx;
    use tita::audio::Audio;
    use tita::config::Config;
    use tita::stats::Stats;
    use tita::ui::menu::Menu;

    fn view() -> View {
        tita::ui::touch::set_mobile(true);
        let cfg = Config { ephemeral: true, ..Config::default() };
        let stats = Stats { ephemeral: true, ..Stats::default() };
        let ctx = Ctx { cfg, stats, audio: Audio::new("none", 650.0, 0.0) };
        View::new(Shell::new(ctx, Box::new(Menu::new())))
    }

    #[test]
    fn a_phone_gets_forty_columns() {
        let mut v = view();
        // 1080 × 2340 à 420 dpi : un téléphone courant.
        v.layout(1080, 2340, 2.625, Insets::default());
        assert_eq!(v.grid(), (40, 41));
        // Petit écran 720 × 1280 à 320 dpi.
        v.layout(720, 1280, 2.0, Insets::default());
        assert_eq!(v.grid().0, 40);
        assert!(v.grid().1 >= 33, "{:?}", v.grid());
        // Une tablette garde un corps lisible et gagne des colonnes.
        v.layout(1600, 2560, 2.0, Insets::default());
        assert!(v.grid().0 > 60, "{:?}", v.grid());
    }

    #[test]
    fn insets_keep_the_grid_clear_of_system_bars() {
        let mut v = view();
        v.layout(1080, 2340, 2.625, Insets { top: 80, bottom: 130, ..Insets::default() });
        assert!(v.origin.1 >= 80);
        let (_, rows) = v.grid();
        assert!(v.origin.1 + rows as usize * v.renderer.cell_h <= 2340 - 130);
        // Un doigt dans la marge retombe sur la cellule la plus proche.
        assert_eq!(v.cell_at(-5.0, 3.0), (0, 0));
        assert_eq!(v.cell_at(5000.0, 5000.0), (39, rows - 1));
    }

    #[test]
    fn tapping_a_menu_line_opens_the_mode_and_back_returns() {
        let mut v = view();
        v.layout(1080, 2340, 2.625, Insets::default());
        assert!(v.compose());
        let mut px = vec![0; 1080 * 2340];
        v.paint(&mut px, 1080, 2340);
        assert!(!v.compose(), "rien n'a changé : pas de nouvelle peinture");
        assert!(px.iter().any(|p| *p != crate::render::BG), "quelque chose est dessiné");

        // Touche la ligne « Statistiques ».
        let text = v.text();
        let row = text.lines().position(|l| l.contains("Statistiques")).expect("ligne du menu");
        let (cw, ch) = (v.renderer.cell_w as f64, v.renderer.cell_h as f64);
        v.touch_down(1, 10.0 * cw, v.origin.1 as f64 + (row as f64 + 0.5) * ch);
        v.touch_up(1);
        assert!(v.compose());
        assert!(v.text().contains("Pas encore de statistiques"), "{}", v.text());

        // La flèche de retour, en haut à gauche.
        v.touch_down(2, 2.0 * cw, v.origin.1 as f64 + 1.5 * ch);
        v.touch_up(2);
        v.compose();
        assert!(v.text().contains("Lettres sans fin"), "{}", v.text());
        tita::ui::touch::set_mobile(false);
    }
}
