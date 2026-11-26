//! Dessine la grille de caractères en pixels : même rendu qu'un terminal,
//! avec en plus des touches à coins arrondis sous les zones tactiles.

use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use tita::ratatui::buffer::Buffer;
use tita::ratatui::layout::Rect;
use tita::ratatui::style::{Color, Modifier};
use tita::ui::{Face, Zone, BIG, TITLE};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
/// Pour les rares symboles absents de la police principale (✓ ⌫ ⚠ …).
const FALLBACK: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

/// Couleurs, au format 0x00RRGGBB.
pub const BG: u32 = 0x0e1116;
const FG: u32 = 0xd5dae2;
const KEY: u32 = 0x27303d;
const KEY_DOWN: u32 = 0x3d4a5e;
const GHOST: u32 = 0x151a21;
const ACCENT: u32 = 0x5cd0de;
const ACCENT_DOWN: u32 = 0xb4eef4;
const CARD: u32 = 0x171d26;
const CARD_DOWN: u32 = 0x263140;
const SELECTED: u32 = 0x163842;

/// Hauteur d'une cellule, en largeurs de cellule.
const ASPECT: f32 = 2.1;
/// Agrandissement des caractères marqués `BIG`, en pourcentage.
const BIG_SCALE: u32 = 300;
/// Agrandissement des textes marqués `TITLE`, en pourcentage.
const TITLE_SCALE: u32 = 135;

/// Couleur de terminal -> couleur d'écran.
fn rgb(c: Color, default: u32) -> u32 {
    match c {
        Color::Reset => default,
        Color::Black => BG,
        Color::Red | Color::LightRed => 0xf2777a,
        Color::Green | Color::LightGreen => 0x7bd88f,
        Color::Yellow | Color::LightYellow => 0xf5c86a,
        Color::Blue | Color::LightBlue => 0x6aa9f5,
        Color::Magenta | Color::LightMagenta => 0xc792ea,
        Color::Cyan | Color::LightCyan => ACCENT,
        Color::Gray => 0xb4bcc8,
        Color::DarkGray => 0x7d8796,
        Color::White => 0xffffff,
        Color::Rgb(r, g, b) => (r as u32) << 16 | (g as u32) << 8 | b as u32,
        Color::Indexed(i) => {
            if i >= 232 {
                let v = 8 + (i as u32 - 232) * 10;
                v << 16 | v << 8 | v
            } else if i >= 16 {
                let i = i as u32 - 16;
                let level = |n: u32| if n == 0 { 0 } else { 55 + n * 40 };
                level(i / 36) << 16 | level(i / 6 % 6) << 8 | level(i % 6)
            } else {
                default
            }
        }
    }
}

/// Mélange `fg` sur `bg` selon `a` (0..=255).
#[inline]
fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    if a >= 255 {
        return fg;
    }
    let mix = |shift: u32| {
        let (b, f) = (bg >> shift & 0xff, fg >> shift & 0xff);
        (b * (255 - a) + f * a + 127) / 255 << shift
    };
    mix(16) | mix(8) | mix(0)
}

/// Image en cours de dessin : `w` × `h` pixels 0x00RRGGBB, ligne par ligne.
pub struct Canvas<'a> {
    pub px: &'a mut [u32],
    pub w: usize,
    pub h: usize,
}

impl Canvas<'_> {
    fn fill(&mut self, x: i32, y: i32, w: usize, h: usize, color: u32, alpha: u32) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + w as i32).max(0) as usize).min(self.w);
        let y1 = ((y + h as i32).max(0) as usize).min(self.h);
        for row in y0..y1 {
            for p in &mut self.px[row * self.w + x0.min(x1)..row * self.w + x1] {
                *p = blend(*p, color, alpha);
            }
        }
    }

    /// Rectangle plein à coins arrondis de rayon `r`, bords adoucis.
    fn round_rect(&mut self, x: i32, y: i32, w: usize, h: usize, r: f32, color: u32) {
        let r = r.min(w as f32 / 2.0).min(h as f32 / 2.0);
        for dy in 0..h as i32 {
            let py = y + dy;
            if py < 0 || py as usize >= self.h {
                continue;
            }
            for dx in 0..w as i32 {
                let px = x + dx;
                if px < 0 || px as usize >= self.w {
                    continue;
                }
                // Distance au centre du coin le plus proche, s'il y en a un.
                let cx = (dx as f32 + 0.5).min(w as f32 - dx as f32 - 0.5);
                let cy = (dy as f32 + 0.5).min(h as f32 - dy as f32 - 0.5);
                let a = if cx < r && cy < r {
                    let d = ((r - cx).powi(2) + (r - cy).powi(2)).sqrt();
                    (r - d + 0.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                if a > 0.0 {
                    let p = &mut self.px[py as usize * self.w + px as usize];
                    *p = blend(*p, color, (a * 255.0) as u32);
                }
            }
        }
    }
}

struct Glyph {
    w: usize,
    h: usize,
    /// Position dans la cellule (agrandie), depuis son coin haut gauche.
    x: i32,
    y: i32,
    cov: Vec<u8>,
}

pub struct Renderer {
    /// Normale, grasse, secours.
    fonts: [Font; 3],
    /// Taille de police pour la cellule courante.
    px: f32,
    pub cell_w: usize,
    pub cell_h: usize,
    /// Ligne de base, depuis le haut de la cellule.
    baseline: f32,
    /// Clé : (caractère, gras, agrandissement en pourcentage).
    cache: HashMap<(char, bool, u32), Glyph>,
}

impl Renderer {
    pub fn new() -> Renderer {
        let load = |bytes: &[u8]| Font::from_bytes(bytes, FontSettings::default()).expect("police embarquée");
        let mut r = Renderer {
            fonts: [load(REGULAR), load(BOLD), load(FALLBACK)],
            px: 0.0,
            cell_w: 0,
            cell_h: 0,
            baseline: 0.0,
            cache: HashMap::new(),
        };
        r.set_cell_width(10);
        r
    }

    /// Choisit la taille de police pour des cellules de `w` pixels de large.
    pub fn set_cell_width(&mut self, w: usize) {
        let w = w.max(4);
        if w == self.cell_w {
            return;
        }
        // Chasse fixe : l'avance de n'importe quel caractère donne la largeur.
        let advance = self.fonts[0].metrics('M', 1.0).advance_width;
        self.px = w as f32 / advance;
        self.cell_w = w;
        self.cell_h = (w as f32 * ASPECT).round() as usize;
        let (ascent, descent) = self.fonts[0]
            .horizontal_line_metrics(self.px)
            .map(|m| (m.ascent, m.descent))
            .unwrap_or((self.px * 0.8, -self.px * 0.2));
        self.baseline = ((self.cell_h as f32 - (ascent - descent)) / 2.0 + ascent).round();
        self.cache.clear();
    }

    /// Vrai si une des polices sait dessiner `c`.
    pub fn has_glyph(&self, c: char) -> bool {
        self.fonts.iter().any(|f| f.lookup_glyph_index(c) != 0)
    }

    fn glyph(&mut self, c: char, bold: bool, scale: u32) -> &Glyph {
        let (fonts, px, cell_w, baseline) = (&self.fonts, self.px, self.cell_w, self.baseline);
        self.cache.entry((c, bold, scale)).or_insert_with(|| {
            let wanted = &fonts[bold as usize];
            let font = if wanted.lookup_glyph_index(c) != 0 { wanted } else { &fonts[2] };
            let s = scale as f32 / 100.0;
            let (m, cov) = font.rasterize(c, px * s);
            let box_w = cell_w as f32 * s;
            Glyph {
                w: m.width,
                h: m.height,
                x: ((box_w - m.advance_width) / 2.0 + m.xmin as f32).round() as i32,
                y: (baseline * s) as i32 - (m.ymin + m.height as i32),
                cov,
            }
        })
    }

    /// Dessine `c` agrandi de `scale` %, dans une case dont (x, y) est le coin
    /// haut gauche (case elle aussi agrandie).
    fn put(&mut self, out: &mut Canvas, c: char, bold: bool, scale: u32, x: i32, y: i32, color: u32) {
        let g = self.glyph(c, bold, scale);
        for row in 0..g.h {
            let py = y + g.y + row as i32;
            if py < 0 || py as usize >= out.h {
                continue;
            }
            for col in 0..g.w {
                let a = g.cov[row * g.w + col] as u32;
                let px = x + g.x + col as i32;
                if a == 0 || px < 0 || px as usize >= out.w {
                    continue;
                }
                let p = &mut out.px[py as usize * out.w + px as usize];
                *p = blend(*p, color, a);
            }
        }
    }

    /// Dessine `buf` dans `out`, coin haut gauche de la grille en (x0, y0).
    /// `zones` : zones tactiles de l'image, `pressed` : celles sous un doigt.
    pub fn draw(&mut self, out: &mut Canvas, buf: &Buffer, zones: &[Zone], pressed: &[Rect], x0: usize, y0: usize) {
        let (cw, ch) = (self.cell_w, self.cell_h);
        out.px.fill(BG);
        let origin = |col: u16, row: u16| ((x0 + col as usize * cw) as i32, (y0 + row as usize * ch) as i32);
        let area = buf.area;

        // 1. Fonds de cellule.
        for row in 0..area.height {
            for col in 0..area.width {
                let cell = &buf[(col, row)];
                let reversed = cell.modifier.contains(Modifier::REVERSED);
                let bg = if reversed { rgb(cell.fg, FG) } else { rgb(cell.bg, BG) };
                if bg != BG {
                    let (x, y) = origin(col, row);
                    out.fill(x, y, cw, ch, bg, 255);
                }
            }
        }

        // 2. Touches et boutons.
        let pad = (cw / 6).max(2) as i32;
        for z in zones {
            let card = matches!(z.face, Face::Card | Face::Selected);
            // Une carte s'allume aussi quand le doigt est sur sa marge.
            let down = if card { pressed.iter().any(|p| p.intersects(z.rect)) } else { pressed.contains(&z.rect) };
            let color = match z.face {
                Face::None => continue,
                Face::Solid => if down { KEY_DOWN } else { KEY },
                Face::Accent => if down { ACCENT_DOWN } else { ACCENT },
                Face::Ghost => GHOST,
                Face::Card => if down { CARD_DOWN } else { CARD },
                Face::Selected => SELECTED,
            };
            let (x, mut y) = origin(z.rect.x, z.rect.y);
            let w = (z.rect.width as usize * cw).saturating_sub(2 * pad as usize);
            let mut h = z.rect.height as usize * ch;
            if card {
                // Déborde d'une demi-rangée de chaque côté de son texte.
                y -= (ch / 2) as i32;
                h += ch;
            }
            out.round_rect(x + pad, y + pad, w, h.saturating_sub(2 * pad as usize), cw as f32 * 0.45, color);
        }

        // 3. Caractères, les grands en dernier (ils débordent sur leurs voisins).
        let line = (ch / 14).max(1);
        let is_bar = |col: u16, row: u16| {
            col < area.width && matches!(buf[(col, row)].symbol(), "█" | "░")
        };
        let mut big = Vec::new();
        for row in 0..area.height {
            // Début et pas (en pixels) du texte agrandi en cours sur la rangée.
            let mut run: Option<(u16, f32, u32)> = None;
            for col in 0..area.width {
                let cell = &buf[(col, row)];
                let titled = cell.modifier.contains(TITLE);
                if !titled {
                    run = None;
                }
                let Some(c) = cell.symbol().chars().next().filter(|c| *c != ' ') else {
                    if titled && run.is_none() {
                        run = Some(self.title_run(buf, col, row));
                    }
                    continue;
                };
                let reversed = cell.modifier.contains(Modifier::REVERSED);
                let fg = if reversed { rgb(cell.bg, BG) } else { rgb(cell.fg, FG) };
                let bold = cell.modifier.contains(Modifier::BOLD);
                let (x, y) = origin(col, row);
                if cell.modifier.contains(BIG) {
                    big.push((c, bold, x, y, fg));
                    continue;
                }
                if titled {
                    let (start, step, scale) = *run.get_or_insert_with(|| self.title_run(buf, col, row));
                    let s = scale as f32 / 100.0;
                    let dy = ((s - 1.0) * ch as f32 / 2.0) as i32;
                    let gx = if step == 0.0 {
                        // Caractère isolé (lettre d'une touche) : centré sur sa cellule.
                        x - ((s - 1.0) * cw as f32 / 2.0) as i32
                    } else {
                        origin(start, row).0 + ((col - start) as f32 * step) as i32
                    };
                    self.put(out, c, bold, scale, gx, y - dy, fg);
                    continue;
                }
                match c {
                    // Barres de progression : un trait fin aux bouts arrondis
                    // plutôt que des pavés pleins.
                    '█' | '░' => {
                        let color = if c == '█' { fg } else { blend(BG, fg, 60) };
                        let h = (ch / 3).max(3);
                        let by = y + ((ch - h) / 2) as i32;
                        let (first, last) = (col == 0 || !is_bar(col - 1, row), !is_bar(col + 1, row));
                        if first || last {
                            out.round_rect(x, by, cw, h, h as f32 / 2.0, color);
                        }
                        // Entre deux segments, pas d'arrondi : on rebouche le côté voisin.
                        match (first, last) {
                            (true, true) => {}
                            (true, false) => out.fill(x + (cw / 2) as i32, by, cw - cw / 2, h, color, 255),
                            (false, true) => out.fill(x, by, cw / 2, h, color, 255),
                            (false, false) => out.fill(x, by, cw, h, color, 255),
                        }
                    }
                    // Filets et curseurs tracés à la main : nets au pixel près.
                    '─' => out.fill(x, y + (ch / 2) as i32, cw, line, fg, 255),
                    '│' => out.fill(x + ((cw - line) / 2) as i32, y, line, ch, fg, 255),
                    '▏' => out.fill(x, y + (ch / 8) as i32, (cw / 6).max(2), ch * 3 / 4, fg, 255),
                    '▮' => out.fill(x + (cw / 5) as i32, y + (ch / 5) as i32, cw * 3 / 5, ch * 3 / 5, fg, 255),
                    _ => self.put(out, c, bold, 100, x, y, fg),
                }
                if cell.modifier.contains(Modifier::UNDERLINED) {
                    out.fill(x, y + self.baseline as i32 + line as i32 + 1, cw, line, fg, 255);
                }
            }
        }
        for (c, bold, x, y, fg) in big {
            // Centré sur sa cellule : une cellule de marge de chaque côté.
            let extra = BIG_SCALE as i32 - 100;
            self.put(out, c, bold, BIG_SCALE, x - extra * cw as i32 / 200, y - extra * ch as i32 / 200, fg);
        }
    }

    /// Texte agrandi commençant en (col, row) : (colonne de départ, pas entre
    /// caractères en pixels — 0 pour un caractère isolé —, agrandissement en %).
    /// L'agrandissement est réduit si le texte ne tiendrait pas dans la rangée.
    fn title_run(&self, buf: &Buffer, col: u16, row: u16) -> (u16, f32, u32) {
        let width = buf.area.width;
        let mut end = col;
        let mut last = col;
        while end < width && buf[(end, row)].modifier.contains(TITLE) {
            if buf[(end, row)].symbol() != " " {
                last = end;
            }
            end += 1;
        }
        let len = (last - col + 1) as u32;
        if len == 1 && end - col == 1 {
            return (col, 0.0, TITLE_SCALE);
        }
        let room = (width - col) as u32 * 100 / len;
        let scale = TITLE_SCALE.min(room).max(100);
        (col, self.cell_w as f32 * scale as f32 / 100.0, scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tout caractère non ASCII employé par l'interface doit avoir un glyphe.
    #[test]
    fn every_symbol_of_the_interface_has_a_glyph() {
        let r = Renderer::new();
        let mut missing = std::collections::BTreeSet::new();
        let mut stack = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../src"))];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    missing.extend(text.chars().filter(|c| !c.is_ascii() && !r.has_glyph(*c)));
                }
            }
        }
        assert!(missing.is_empty(), "sans glyphe : {missing:?}");
    }

    #[test]
    fn blend_reaches_both_ends() {
        assert_eq!(blend(0x102030, 0xa0b0c0, 0), 0x102030);
        assert_eq!(blend(0x102030, 0xa0b0c0, 255), 0xa0b0c0);
        let mid = blend(0x000000, 0xffffff, 128);
        assert!((mid & 0xff) > 120 && (mid & 0xff) < 136);
    }

    #[test]
    fn cells_follow_the_requested_width() {
        let mut r = Renderer::new();
        r.set_cell_width(27);
        assert_eq!((r.cell_w, r.cell_h), (27, 57));
        // La ligne de base laisse la place aux jambages.
        assert!(r.baseline > 30.0 && r.baseline < 50.0, "{}", r.baseline);
    }
}
