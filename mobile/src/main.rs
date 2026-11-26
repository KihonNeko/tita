//! Sur PC : essayer l'interface mobile sans téléphone.
//!
//! - `tita-mobile` ouvre une fenêtre aux proportions d'un téléphone ; la
//!   souris fait office de doigt.
//! - `tita-mobile shots DOSSIER [LARGEUR HAUTEUR ÉCHELLE] [--en]` enregistre
//!   une capture PNG de chaque écran, sans rien afficher.
//!
//! Sur Android ce binaire ne sert pas : l'application est la bibliothèque,
//! chargée par NativeActivity.

#[cfg(not(target_os = "android"))]
mod desktop;

fn main() {
    #[cfg(not(target_os = "android"))]
    desktop::main();
}
