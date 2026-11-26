//! tita sur mobile : la même interface que dans le terminal, dessinée en
//! pixels dans une fenêtre et pilotée au doigt.
//!
//! - `render` : la grille de caractères -> pixels ;
//! - `view` : l'application et sa grille, les touchers ;
//! - `window` : la boucle d'événements (Android, ou fenêtre d'essai sur PC).

pub mod render;
pub mod view;
pub mod window;

use tita::app::{Ctx, Shell};
use tita::audio::Audio;
use tita::config::Config;
use tita::stats::Stats;
use tita::ui::menu::Menu;

/// L'application telle qu'elle démarre sur mobile : réglages et progression
/// chargés, mise en page tactile, menu à l'écran.
pub fn start() -> Shell {
    tita::ui::touch::set_mobile(true);
    let cfg = Config::load();
    tita::i18n::set(cfg.ui_lang);
    let audio = Audio::new(&cfg.backend, cfg.freq, cfg.volume);
    Shell::new(Ctx { cfg, stats: Stats::load(), audio }, Box::new(Menu::new()))
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use tita::config::{self, Lang};
    use winit::event_loop::EventLoop;
    use winit::platform::android::activity::WindowManagerFlags;
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("tita"),
    );
    // Sans cela une panique disparaît sans laisser de trace dans logcat.
    std::panic::set_hook(Box::new(|info| log::error!("{info}")));

    // Pas de dossier utilisateur ni de variable LANG sur Android.
    if let Some(dir) = app.internal_data_path() {
        config::set_data_dir(dir);
    }
    let french = app.config().language().is_some_and(|l| l.to_lowercase().starts_with("fr"));
    config::set_system_lang(if french { Lang::Fr } else { Lang::En });
    // Une session d'écoute ne touche pas l'écran pendant de longues secondes.
    app.set_window_flags(WindowManagerFlags::KEEP_SCREEN_ON, WindowManagerFlags::empty());

    match EventLoop::builder().with_android_app(app).build() {
        Ok(event_loop) => window::run(event_loop, None),
        Err(e) => log::error!("boucle d'événements : {e}"),
    }
    // Tout est déjà enregistré ; sortir pour de bon rend la main au système.
    std::process::exit(0);
}
