//! Aides d'affichage communes à tous les écrans.

pub mod menu;
pub mod results;
pub mod settings;
pub mod stats_screen;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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
pub fn frame(f: &mut Frame, title: &str, hints: &[(&str, &str)]) -> Rect {
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
