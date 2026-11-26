//! Interface tactile : chaque écran déclare, en se dessinant, les zones de la
//! grille qui réagissent au doigt. Dans le terminal rien n'est déclaré.
//!
//! L'état est propre au thread courant (l'interface n'en utilise qu'un), ce
//! qui permet aux tests de passer en mode mobile sans se gêner entre eux.

use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use std::cell::RefCell;

/// Ce que déclenche une zone touchée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    /// Comme si cette touche était appuyée, puis relâchée quand le doigt se lève.
    Key(KeyCode),
    /// Élément de liste : appelle `Screen::on_item`.
    Item(usize),
}

/// Aspect d'une zone, dessiné par la coquille mobile sous le texte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Rien : la zone est invisible (ligne de liste, écran entier).
    None,
    /// Touche ou bouton plein.
    Solid,
    /// Bouton mis en avant (action principale, manipulateur).
    Accent,
    /// Touche inactive : simple contour.
    Ghost,
    /// Ligne de liste : une carte qui déborde d'une demi-rangée au-dessus et
    /// au-dessous de son texte.
    Card,
    /// Carte de la ligne sélectionnée.
    Selected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub rect: Rect,
    /// `None` : la zone avale le toucher sans rien faire (touche inactive).
    pub tap: Option<Tap>,
    pub face: Face,
}

#[derive(Default)]
struct State {
    mobile: bool,
    zones: Vec<Zone>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Active la mise en page tactile pour le thread courant.
pub fn set_mobile(on: bool) {
    STATE.with(|s| s.borrow_mut().mobile = on);
}

/// Vrai dans la coquille mobile : écrans étroits, pilotés au doigt.
pub fn mobile() -> bool {
    STATE.with(|s| s.borrow().mobile)
}

/// Oublie les zones de l'image précédente (appelé avant chaque dessin).
pub fn clear() {
    STATE.with(|s| s.borrow_mut().zones.clear());
}

/// Déclare une zone. Sans effet dans le terminal. Une zone déclarée après
/// une autre passe par-dessus.
pub fn zone(rect: Rect, tap: Option<Tap>, face: Face) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.mobile && rect.width > 0 && rect.height > 0 {
            s.zones.push(Zone { rect, tap, face });
        }
    });
}

/// Zone invisible qui envoie une touche.
pub fn key(rect: Rect, code: KeyCode) {
    zone(rect, Some(Tap::Key(code)), Face::None);
}

/// Zone invisible qui choisit un élément de liste.
pub fn item(rect: Rect, index: usize) {
    zone(rect, Some(Tap::Item(index)), Face::None);
}

/// Zone sous la cellule (col, row) : la dernière déclarée l'emporte.
pub fn hit(col: u16, row: u16) -> Option<Zone> {
    STATE.with(|s| {
        s.borrow()
            .zones
            .iter()
            .rev()
            .find(|z| {
                col >= z.rect.x
                    && col < z.rect.x + z.rect.width
                    && row >= z.rect.y
                    && row < z.rect.y + z.rect.height
            })
            .copied()
    })
}

/// Toutes les zones de la dernière image, dans l'ordre de déclaration.
pub fn zones() -> Vec<Zone> {
    STATE.with(|s| s.borrow().zones.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_only_exist_on_mobile_and_last_wins() {
        let r = Rect::new(0, 0, 10, 3);
        clear();
        key(r, KeyCode::Enter);
        assert!(hit(1, 1).is_none(), "rien dans le terminal");

        set_mobile(true);
        key(r, KeyCode::Enter);
        zone(Rect::new(2, 0, 2, 3), None, Face::Ghost);
        assert_eq!(hit(0, 0).unwrap().tap, Some(Tap::Key(KeyCode::Enter)));
        assert_eq!(hit(2, 1).unwrap().tap, None, "la touche inactive avale le toucher");
        assert!(hit(10, 0).is_none() && hit(0, 3).is_none());
        clear();
        assert!(hit(0, 0).is_none());
        set_mobile(false);
    }
}
