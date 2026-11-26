//! Aides d'affichage communes à tous les écrans.

pub mod menu;
pub mod results;
pub mod settings;
pub mod stats_screen;
pub mod touch;

pub use touch::{mobile, Face, Tap, Zone};

use crate::i18n::{is_en, t};
use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub const ACCENT: Color = Color::Cyan;
pub const OK: Color = Color::Green;
pub const ERR: Color = Color::Red;
pub const WARN: Color = Color::Yellow;
pub const DIM: Color = Color::DarkGray;

pub fn bold(c: Color) -> Style {
    Style::default().fg(c).add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

/// Dessine le cadre standard (titre en haut, raccourcis en bas) et renvoie la
/// zone centrale où l'écran dessine son contenu.
/// `hints` : paires (touche, action), ex. `[("Échap", "quitter")]`.
///
/// Sur mobile, les raccourcis deviennent des boutons : « Échap » est la
/// flèche de retour en haut à gauche, les autres forment la barre du bas.
pub fn frame(f: &mut Frame, title: &str, hints: &[(&str, &str)]) -> Rect {
    if mobile() {
        return frame_mobile(f, title, hints);
    }
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let header = Line::from(vec![
        Span::styled(" tita ", bold(Color::Black).bg(ACCENT)),
        Span::raw("  "),
        Span::styled(title.to_string(), bold(Color::White)),
    ]);
    f.render_widget(
        Paragraph::new(header).block(Block::default().borders(Borders::BOTTOM).border_style(dim())),
        head,
    );
    let mut spans = Vec::new();
    for (k, a) in hints {
        spans.push(Span::styled(format!(" {k} "), bold(ACCENT)));
        spans.push(Span::styled(format!("{a}  "), dim()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), foot);
    Rect { x: body.x + 1, width: body.width.saturating_sub(2), ..body }
}

// ─────────────────────────────── mobile ───────────────────────────────

/// Hauteur d'un bouton de la barre du bas, en rangées.
const BUTTON_H: u16 = 3;
/// Largeur d'un bouton flèche, en colonnes.
const ARROW_W: u16 = 7;
/// Rangées laissées au contenu au-dessus du clavier avant de réduire les touches.
const MIN_BODY: u16 = 9;

/// Modificateur détourné : la coquille mobile dessine ces cellules en grand.
pub const BIG: Modifier = Modifier::RAPID_BLINK;
/// Modificateur détourné : texte un peu agrandi (titres, lettres des touches).
pub const TITLE: Modifier = Modifier::SLOW_BLINK;

/// Touche(s) désignée(s) par le libellé d'un raccourci : (flèche affichée, touche).
/// `None` : rien à toucher (retour, ou raccourci propre au clavier).
fn hint_keys(label: &str) -> Vec<(Option<char>, KeyCode)> {
    match label.trim() {
        "Entrée" | "Enter" => return vec![(None, KeyCode::Enter)],
        "Espace" | "Space" => return vec![(None, KeyCode::Char(' '))],
        "Tab" => return vec![(None, KeyCode::Tab)],
        "⌫" | "Retour arr." | "Backspace" => return vec![(None, KeyCode::Backspace)],
        _ => {}
    }
    let mut keys = Vec::new();
    for c in label.chars() {
        let code = match c {
            '↑' => KeyCode::Up,
            '↓' => KeyCode::Down,
            '←' => KeyCode::Left,
            '→' => KeyCode::Right,
            ' ' => continue,
            _ => return Vec::new(),
        };
        keys.push((Some(c), code));
    }
    keys
}

fn is_back(label: &str) -> bool {
    matches!(label.trim(), "Échap" | "Esc")
}

struct Button {
    label: String,
    code: KeyCode,
    arrow: bool,
}

fn buttons(hints: &[(&str, &str)]) -> Vec<Button> {
    let mut out = Vec::new();
    for (k, action) in hints {
        let keys = hint_keys(k);
        let alone = keys.len() == 1;
        for (arrow, code) in keys {
            out.push(match arrow {
                // Une flèche seule garde son action (« ← revoir »).
                Some(a) if alone => Button { label: format!("{a} {action}"), code, arrow: false },
                Some(a) => Button { label: a.to_string(), code, arrow: true },
                None => Button { label: action.to_string(), code, arrow: false },
            });
        }
    }
    out
}

/// Écrit `label` centré dans `rect` (replié si besoin).
fn label_in(f: &mut Frame, rect: Rect, label: &str, style: Style) {
    let inner = Rect { x: rect.x + 1, width: rect.width.saturating_sub(2), ..rect };
    let p = Paragraph::new(Line::styled(label.to_string(), style))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    let h = (p.line_count(inner.width) as u16).clamp(1, rect.height);
    f.render_widget(p, centered(inner, inner.width, h));
}

/// Bouton tactile : fond dessiné par la coquille, libellé centré.
pub fn button(f: &mut Frame, rect: Rect, label: &str, code: KeyCode, face: Face) {
    let style = match face {
        Face::Accent => bold(Color::Black),
        _ => bold(ACCENT),
    };
    touch::zone(rect, Some(Tap::Key(code)), face);
    label_in(f, rect, label, style);
}

fn frame_mobile(f: &mut Frame, title: &str, hints: &[(&str, &str)]) -> Rect {
    let btns = buttons(hints);
    let back = hints.iter().any(|(k, _)| is_back(k));
    let foot_h = if btns.is_empty() { 0 } else { BUTTON_H + 1 };
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(foot_h),
    ])
    .areas(f.area());

    // Pas de filet sous le titre : un titre agrandi, et de l'air autour.
    let mut header = Vec::new();
    if back {
        header.push(Span::styled(" ‹ ", bold(ACCENT).add_modifier(TITLE)));
        touch::key(Rect { width: head.width.min(7), ..head }, KeyCode::Esc);
    } else {
        header.push(Span::raw(" "));
        header.push(Span::styled(" tita ", bold(Color::Black).bg(ACCENT)));
        header.push(Span::raw("  "));
    }
    header.push(Span::styled(title.to_string(), bold(Color::White).add_modifier(TITLE)));
    f.render_widget(Paragraph::new(vec![Line::raw(""), Line::from(header)]), head);

    if !btns.is_empty() {
        // Une rangée de marge au-dessus, puis les boutons côte à côte.
        let bar = Rect { x: foot.x + 1, y: foot.y + 1, width: foot.width.saturating_sub(2), height: BUTTON_H };
        let arrows = btns.iter().filter(|b| b.arrow).count() as u16;
        let texts = btns.len() as u16 - arrows;
        let gaps = btns.len() as u16 - 1;
        let arrow_w = if texts == 0 { (bar.width.saturating_sub(gaps)) / arrows.max(1) } else { ARROW_W };
        let rest = bar.width.saturating_sub(gaps + arrows * arrow_w);
        let mut x = bar.x;
        let mut texts_left = texts;
        let mut rest_left = rest;
        for b in &btns {
            let w = if b.arrow {
                arrow_w
            } else {
                let w = rest_left / texts_left.max(1);
                rest_left -= w;
                texts_left -= 1;
                w
            };
            let rect = Rect { x, y: bar.y, width: w, height: bar.height };
            // « Entrée » est l'action principale de l'écran.
            let main = b.code == KeyCode::Enter;
            button(f, rect, &b.label, b.code, if main { Face::Accent } else { Face::Solid });
            x += w + 1;
        }
    }
    Rect { x: body.x + 1, width: body.width.saturating_sub(2), ..body }
}

/// Clavier tactile à afficher sous le contenu.
pub struct Keys<'a> {
    /// Caractères actifs ; les autres lettres restent visibles mais inertes.
    pub chars: &'a [char],
    /// Ajouter une touche d'effacement.
    pub backspace: bool,
    /// Ajouter une barre d'espace.
    pub space: bool,
}

const DIGITS: &str = "1234567890";
/// Rangées de symboles : affichées seulement si un de leurs caractères est actif.
const SYMBOLS: [&str; 3] = [".,?/=+-'!@", "():;\"_$", "ÉÀÈÇ&<>#*"];

/// Rangées de lettres, selon la langue de l'interface.
fn letter_rows() -> [&'static str; 3] {
    if is_en() {
        ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"]
    } else {
        ["AZERTYUIOP", "QSDFGHJKLM", "WXCVBN"]
    }
}

/// Une touche du clavier : (libellé, touche envoyée, largeur en emplacements).
type KeyDef = (String, KeyCode, u16);

fn key_rows(keys: &Keys) -> Vec<Vec<KeyDef>> {
    let any = |row: &str| row.chars().any(|c| keys.chars.contains(&c));
    let row = |s: &str| -> Vec<KeyDef> { s.chars().map(|c| (c.to_string(), KeyCode::Char(c), 1)).collect() };
    let mut rows = Vec::new();
    if any(DIGITS) {
        rows.push(row(DIGITS));
    }
    let letters = letter_rows();
    rows.push(row(letters[0]));
    rows.push(row(letters[1]));
    let mut last = row(letters[2]);
    if keys.backspace {
        last.push(("⌫".into(), KeyCode::Backspace, 2));
    }
    rows.push(last);
    for s in SYMBOLS {
        if any(s) {
            rows.push(row(s));
        }
    }
    if keys.space {
        rows.push(vec![(t("espace", "space").into(), KeyCode::Char(' '), 6)]);
    }
    rows
}

/// Sur mobile, dessine le clavier tactile en bas de `area` et renvoie la
/// place qui reste au-dessus. Dans le terminal, renvoie `area` telle quelle.
pub fn keyboard(f: &mut Frame, area: Rect, keys: Keys) -> Rect {
    if !mobile() {
        return area;
    }
    let rows = key_rows(&keys);
    let n = rows.len() as u16;
    let key_h = if area.height >= n * 3 + MIN_BODY { 3 } else { 2 };
    let kb_h = (n * key_h).min(area.height);
    // Le clavier prend toute la largeur de l'écran, pas seulement le contenu.
    let full = f.area();
    let slot = (full.width / 10).max(1);
    let top = area.y + area.height - kb_h;
    for (r, row) in rows.iter().enumerate() {
        let slots: u16 = row.iter().map(|k| k.2).sum();
        let mut x = full.x + (full.width.saturating_sub(slots * slot)) / 2;
        let y = top + r as u16 * key_h;
        if y + key_h > area.y + area.height {
            break;
        }
        for (label, code, w) in row {
            let rect = Rect { x, y, width: w * slot, height: key_h };
            x += w * slot;
            let active = match code {
                KeyCode::Char(c) if *c != ' ' => keys.chars.contains(c),
                _ => true,
            };
            // Une lettre seule est agrandie : plus lisible sous le doigt.
            let cap = if label.chars().count() == 1 { TITLE } else { Modifier::empty() };
            if active {
                touch::zone(rect, Some(Tap::Key(*code)), Face::Solid);
                label_in(f, rect, label, bold(Color::White).add_modifier(cap));
            } else {
                touch::zone(rect, None, Face::Ghost);
                label_in(f, rect, label, dim().add_modifier(cap));
            }
        }
    }
    Rect { height: area.height.saturating_sub(kb_h + 1), ..area }
}

/// Carte de liste tactile dont le texte occupe `rows` rangées à partir de
/// `y` ; la rangée suivante doit rester vide (la carte y déborde). Renvoie la
/// zone où écrire le texte. Toucher la carte appelle `Screen::on_item(index)`.
pub fn card(f: &Frame, area: Rect, y: u16, rows: u16, index: usize, selected: bool) -> Rect {
    let full = f.area();
    // Toute la bande réagit, marge comprise ; la carte visible est par-dessus.
    let band = Rect { x: full.x, y, width: full.width, height: rows + 1 }.intersection(full);
    touch::item(band, index);
    let face = if selected { Face::Selected } else { Face::Card };
    touch::zone(Rect { x: full.x, y, width: full.width, height: rows }, Some(Tap::Item(index)), face);
    Rect { x: area.x, y, width: area.width, height: rows }
}

/// Une ligne avec `left` à gauche et `right` calé à droite, sur `width` colonnes.
pub fn split_line(left: Span<'static>, right: Span<'static>, width: u16) -> Line<'static> {
    let used = left.content.chars().count() + right.content.chars().count();
    let gap = (width as usize).saturating_sub(used).max(1);
    Line::from(vec![left, Span::raw(" ".repeat(gap)), right])
}

/// Paragraphe replié à la largeur disponible (au plus `max_w` colonnes) et
/// centré verticalement dans `area`.
pub fn text(f: &mut Frame, lines: Vec<Line>, area: Rect, max_w: u16, align: Alignment) {
    let w = max_w.min(area.width);
    let p = Paragraph::new(lines).alignment(align).wrap(Wrap { trim: false });
    let h = p.line_count(w) as u16;
    f.render_widget(p, centered(area, w, h));
}

/// Le caractère vedette d'un écran (« ? », la lettre à apprendre). Sur
/// mobile il est dessiné en grand, entouré de rangées vides.
pub fn hero(lines: &mut Vec<Line<'static>>, text: String, style: Style) {
    if mobile() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(text, style.add_modifier(BIG)));
        lines.push(Line::raw(""));
    } else {
        lines.push(Line::styled(text, style));
    }
}

/// Rectangle de taille (w, h) centré dans `area` (rogné si trop grand).
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Barre de progression textuelle, ex. `████████░░░░`.
pub fn bar(ratio: f32, width: usize) -> String {
    let n = (ratio.clamp(0.0, 1.0) * width as f32).round() as usize;
    format!("{}{}", "█".repeat(n), "░".repeat(width - n))
}

/// Couleur selon une précision 0..1.
pub fn acc_color(acc: f32) -> Color {
    if acc >= 0.9 {
        OK
    } else if acc >= 0.75 {
        WARN
    } else {
        ERR
    }
}

/// `mm:ss`.
pub fn mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morse;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(w: u16, h: u16, body: impl FnOnce(&mut Frame)) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        touch::clear();
        term.draw(body).unwrap();
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

    #[test]
    fn mobile_frame_turns_hints_into_buttons() {
        touch::set_mobile(true);
        let s = draw(40, 30, |f| {
            frame(f, "Koch", &[("Espace", "groupe suivant"), ("↑↓", "défiler"), ("Échap", "arrêter")]);
        });
        assert!(s.contains("‹") && s.contains("Koch") && s.contains("groupe suivant"));
        assert!(!s.contains("Espace") && !s.contains("arrêter"), "{s}");
        let taps: Vec<Tap> = touch::zones().iter().filter_map(|z| z.tap).collect();
        for code in [KeyCode::Esc, KeyCode::Char(' '), KeyCode::Up, KeyCode::Down] {
            assert!(taps.contains(&Tap::Key(code)), "{code:?}");
        }
        // Le retour est en haut à gauche, les boutons tout en bas.
        assert_eq!(touch::hit(2, 1).unwrap().tap, Some(Tap::Key(KeyCode::Esc)));
        assert_eq!(touch::hit(3, 28).unwrap().tap, Some(Tap::Key(KeyCode::Char(' '))));
        touch::set_mobile(false);
    }

    #[test]
    fn desktop_frame_is_unchanged() {
        let s = draw(80, 10, |f| {
            frame(f, "Koch", &[("Espace", "groupe suivant"), ("Échap", "arrêter")]);
        });
        assert!(s.contains(" tita ") && s.contains(" Espace groupe suivant") && s.contains("Échap"));
        assert!(touch::zones().is_empty());
    }

    #[test]
    fn keyboard_has_a_key_for_every_character() {
        touch::set_mobile(true);
        for en in [false, true] {
            crate::i18n::set(if en { crate::config::Lang::En } else { crate::config::Lang::Fr });
            let all: Vec<char> = morse::TABLE.iter().map(|(c, _)| *c).collect();
            let mut left = Rect::default();
            draw(40, 40, |f| {
                left = keyboard(f, f.area(), Keys { chars: &all, backspace: true, space: true });
            });
            let taps: Vec<Tap> = touch::zones().iter().filter_map(|z| z.tap).collect();
            for c in &all {
                assert!(taps.contains(&Tap::Key(KeyCode::Char(*c))), "{c}");
            }
            assert!(taps.contains(&Tap::Key(KeyCode::Backspace)) && taps.contains(&Tap::Key(KeyCode::Char(' '))));
            // Les touches ne se chevauchent pas et restent dans l'écran.
            let zones = touch::zones();
            for (i, a) in zones.iter().enumerate() {
                assert!(a.rect.x + a.rect.width <= 40 && a.rect.y + a.rect.height <= 40);
                for b in &zones[i + 1..] {
                    assert!(!a.rect.intersects(b.rect), "{a:?} {b:?}");
                }
            }
            assert!(left.height >= MIN_BODY, "{left:?}");
        }
        crate::i18n::set(crate::config::Lang::Fr);
        touch::set_mobile(false);
    }

    #[test]
    fn keyboard_starts_small_and_locks_unknown_letters() {
        touch::set_mobile(true);
        let s = draw(40, 36, |f| {
            keyboard(f, f.area(), Keys { chars: &['E', 'T'], backspace: false, space: false });
        });
        assert!(s.contains('A') && !s.contains('1') && !s.contains('?'), "{s}");
        let zones = touch::zones();
        assert_eq!(zones.len(), 26);
        assert_eq!(zones.iter().filter(|z| z.tap.is_some()).count(), 2);
        // Grandes touches tant que la place le permet.
        assert!(zones.iter().all(|z| z.rect.height == 3 && z.rect.width == 4));
        touch::set_mobile(false);
    }

    #[test]
    fn keyboard_is_absent_in_the_terminal() {
        let area = Rect::new(0, 0, 80, 20);
        let mut left = Rect::default();
        let s = draw(80, 20, |f| left = keyboard(f, area, Keys { chars: &['E'], backspace: true, space: true }));
        assert_eq!(left, area);
        assert!(s.trim().is_empty());
    }
}
