//! Sur PC : essayer l'interface mobile sans téléphone.
//!
//! - `cargo run -p tita-mobile --example preview` ouvre une fenêtre aux
//!   proportions d'un téléphone ; la souris fait office de doigt.
//! - `cargo run -p tita-mobile --example preview -- shots DOSSIER
//!   [LARGEUR HAUTEUR ÉCHELLE] [--en]` enregistre une capture PNG de chaque
//!   écran, sans rien afficher.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Duration;
use tita::app::{Ctx, Screen, Shell};
use tita::audio::Audio;
use tita::config::{Config, Lang};
use tita::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tita::modes;
use tita::stats::{Session, Stats};
use tita::ui;
use tita_mobile::view::{Insets, View};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("shots") {
        let en = args.iter().any(|a| a == "--en");
        let rest: Vec<&String> = args[1..].iter().filter(|a| *a != "--en").collect();
        let dir = rest.first().map(|s| s.as_str()).unwrap_or("shots");
        let num = |i: usize, default: f32| rest.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
        let (w, h, scale) = (num(1, 1080.0) as usize, num(2, 2340.0) as usize, num(3, 2.625));
        if let Err(e) = shots(Path::new(dir), w, h, scale, en) {
            eprintln!("tita-mobile: {e}");
            std::process::exit(1);
        }
        return;
    }
    match winit::event_loop::EventLoop::new() {
        Ok(event_loop) => tita_mobile::window::run(event_loop, Some((412, 892))),
        Err(e) => {
            eprintln!("tita-mobile: {e}");
            std::process::exit(1);
        }
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Une application muette qui n'écrit rien sur disque, avec `unlocked`
/// caractères débloqués et un peu d'historique si `history`.
fn scene(first: Box<dyn Screen>, unlocked: usize, history: bool, en: bool) -> Shell {
    let lang = if en { Lang::En } else { Lang::Fr };
    let cfg = Config { ephemeral: true, ui_lang: lang, lang, ..Config::default() };
    let mut stats = Stats { ephemeral: true, ..Stats::default() };
    stats.set_unlocked(cfg.track, unlocked);
    if history {
        let chars = cfg.track.chars(unlocked);
        for (i, c) in chars.iter().enumerate() {
            for n in 0..12 {
                let wrong = (i * 7 + n * 3) % 9 == 0;
                let typed = if wrong { chars[(i + 3) % chars.len()] } else { *c };
                stats.record(*c, Some(typed), Some(Duration::from_millis(380 + (i * 37 % 400) as u64)));
            }
        }
        for (day, mode, total, correct) in [(3, "Lettres", 140, 118), (2, "Koch", 95, 83), (1, "Flux", 210, 190)] {
            stats.sessions.push(Session {
                date: format!("2026-11-{:02}", 20 + day),
                mode: mode.into(),
                secs: 300,
                total,
                correct,
                char_wpm: 20.0,
                eff_wpm: 8.0,
            });
        }
    }
    tita::i18n::set(lang);
    // Muet, mais sans l'avertissement « pas de sortie audio » sur les captures.
    let mut audio = Audio::new("none", 650.0, 0.0);
    audio.available = true;
    Shell::new(Ctx { cfg, stats, audio }, first)
}

fn shots(dir: &Path, w: usize, h: usize, scale: f32, en: bool) -> Result<(), Box<dyn std::error::Error>> {
    ui::touch::set_mobile(true);
    std::fs::create_dir_all(dir)?;
    let all = tita::morse::TABLE.len();
    type Setup = Box<dyn Fn(&mut View)>;
    let keys = |codes: Vec<KeyCode>| -> Setup {
        Box::new(move |v: &mut View| {
            for c in &codes {
                v.shell.key(key(*c));
            }
        })
    };
    let chars = |s: &str| s.chars().map(KeyCode::Char).collect::<Vec<_>>();
    let none = || keys(Vec::new());
    let scenes: Vec<(&str, Shell, Setup)> = vec![
        ("01-menu", scene(Box::new(ui::menu::Menu::new()), 2, false, en), none()),
        ("02-letters-ready", scene(Box::new(modes::drill::Drill::new(false)), 6, false, en), none()),
        ("03-letters-listen", scene(Box::new(modes::drill::Drill::new(false)), 6, true, en), keys(vec![KeyCode::Enter])),
        (
            "04-letters-error",
            scene(Box::new(modes::drill::Drill::new(false)), 6, true, en),
            keys(vec![KeyCode::Enter, KeyCode::Char('X')]),
        ),
        ("05-letters-all", scene(Box::new(modes::drill::Drill::new(true)), all, true, en), keys(vec![KeyCode::Enter])),
        ("06-discover", scene(Box::new(modes::discover::Discover::new(vec!['I', 'M', 'S', 'O'])), 6, false, en), none()),
        (
            "07-koch",
            scene(Box::new(modes::koch::Koch::new()), 14, true, en),
            keys([vec![KeyCode::Enter], chars("ETIMS OANRW DK")].concat()),
        ),
        ("08-stream", scene(Box::new(modes::flux::Flux::new()), 14, true, en), keys(vec![KeyCode::Enter])),
        ("09-words", scene(Box::new(modes::words::Words::new()), 26, true, en), keys(chars("MO"))),
        ("10-radio", scene(Box::new(modes::radio::Radio::new()), 26, true, en), none()),
        (
            "11-contact",
            scene(Box::new(modes::radio::Radio::new()), all, true, en),
            Box::new(|v: &mut View| {
                v.shell.key(key(KeyCode::Down));
                v.shell.key(key(KeyCode::Down));
                v.shell.key(key(KeyCode::Enter));
                for c in "CQ CQ DE F4".chars() {
                    v.shell.key(key(KeyCode::Char(c)));
                }
            }),
        ),
        ("12-sending", scene(Box::new(modes::send::SendMode::new()), 10, true, en), none()),
        ("13-settings", scene(Box::new(ui::settings::Settings::new()), 10, true, en), keys(vec![KeyCode::Down; 2])),
        ("14-stats", scene(Box::new(ui::stats_screen::StatsScreen::new()), 18, true, en), none()),
        ("15-stats-chars", scene(Box::new(ui::stats_screen::StatsScreen::new()), 18, true, en), keys(vec![KeyCode::Tab])),
        (
            "16-summary",
            scene(Box::new(modes::drill::Drill::new(false)), 6, true, en),
            keys([vec![KeyCode::Enter], chars("XEXTXI"), vec![KeyCode::Esc]].concat()),
        ),
    ];
    for (name, shell, setup) in scenes {
        let mut view = View::new(shell);
        view.layout(w, h, scale, Insets::default());
        setup(&mut view);
        view.compose();
        let mut px = vec![0u32; w * h];
        view.paint(&mut px, w, h);
        let path = dir.join(format!("{name}.png"));
        write_png(&path, &px, w, h)?;
        std::fs::write(dir.join(format!("{name}.txt")), view.text())?;
        println!("{}  {:?}", path.display(), view.grid());
    }
    Ok(())
}

fn write_png(path: &Path, px: &[u32], w: usize, h: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let mut bytes = Vec::with_capacity(px.len() * 3);
    for p in px {
        bytes.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    writer.write_image_data(&bytes)?;
    Ok(())
}
