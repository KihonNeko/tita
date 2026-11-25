//! tita — apprendre le Morse à l'oreille, dans le terminal.

mod app;
mod audio;
mod config;
mod curriculum;
mod data;
mod modes;
mod morse;
mod picker;
mod stats;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tita", version, about = "Apprendre le Morse à l'oreille, dans le terminal")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Joue un texte en Morse puis quitte
    Play {
        text: Vec<String>,
        /// Vitesse des caractères (mots/minute)
        #[arg(long)]
        wpm: Option<f32>,
        /// Vitesse effective (Farnsworth)
        #[arg(long)]
        eff: Option<f32>,
        /// Fréquence du son (Hz)
        #[arg(long)]
        freq: Option<f32>,
    },
    /// Affiche le motif Morse d'un texte
    Show { text: Vec<String> },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::load();
    match cli.cmd {
        Some(Cmd::Show { text }) => {
            for c in text.join(" ").chars() {
                match morse::pattern(c) {
                    Some(p) => println!("{}  {}", crate::morse::up(c), p),
                    None => println!(),
                }
            }
            Ok(())
        }
        Some(Cmd::Play { text, wpm, eff, freq }) => {
            let wpm = wpm.unwrap_or(cfg.char_wpm);
            let t = morse::Timing::new(wpm, eff.unwrap_or(wpm));
            let audio = audio::Audio::new(&cfg.backend, freq.unwrap_or(cfg.freq), cfg.volume);
            if !audio.available {
                anyhow::bail!(if cfg!(target_os = "linux") {
                    "pas de sortie audio : installe pipewire (pw-cat) ou alsa-utils (aplay)"
                } else {
                    "pas de sortie audio : aucun périphérique de sortie utilisable"
                });
            }
            audio.play(&text.join(" "), t);
            audio.wait_idle();
            Ok(())
        }
        None => {
            let audio = audio::Audio::new(&cfg.backend, cfg.freq, cfg.volume);
            let ctx = app::Ctx { cfg, stats: stats::Stats::load(), audio };
            app::run(ctx, Box::new(ui::menu::Menu::new()))
        }
    }
}
