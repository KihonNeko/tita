//! tita — apprendre le Morse à l'oreille.
//!
//! Le cœur de l'application : logique, écrans et son. Deux coquilles s'en
//! servent : le binaire `tita` (terminal) et `tita-mobile` (Android, même
//! grille de caractères dessinée en pixels et pilotée au doigt).

pub mod app;
pub mod audio;
pub mod config;
pub mod curriculum;
pub mod data;
pub mod i18n;
pub mod modes;
pub mod morse;
pub mod picker;
pub mod stats;
pub mod ui;

pub use ratatui;
pub use crossterm;
