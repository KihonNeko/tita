//! Sortie audio : sinusoïde générée, envoyée en PCM brut à `pw-cat` ou `aplay`
//! (aucune dépendance de compilation sur ALSA). Un thread écrit le flux en temps
//! réel par petits blocs, ce qui donne un timing précis à l'échantillon près.

use crate::morse::{self, Seg, Timing};
use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const RATE: u32 = 44_100;
const CHUNK: usize = 221; // ~5 ms
const LEAD: Duration = Duration::from_millis(40); // avance max sur le temps réel
const RAMP_SECS: f32 = 0.005; // fondu anti-clic

/// Fenêtre (estimée) pendant laquelle un texte mis en file sera audible.
#[derive(Clone, Copy, Debug)]
pub struct PlaySpan {
    pub start: Instant,
    pub end: Instant,
}

struct Shared {
    queue: Mutex<VecDeque<(bool, u64)>>, // (son ?, échantillons restants)
    queued: AtomicU64,                   // total d'échantillons en file
    key_down: AtomicBool,                // manipulateur (mode émission)
    freq: AtomicU32,                     // Hz (f32 bits)
    volume: AtomicU32,                   // 0..1 (f32 bits)
    quit: AtomicBool,
}

pub struct Audio {
    shared: Arc<Shared>,
    child: Option<Child>,
    /// `false` si aucun lecteur audio n'a pu être lancé (mode muet).
    pub available: bool,
}

fn spawn_player(backend: &str) -> Result<Child> {
    let rate = RATE.to_string();
    let try_pw = || {
        Command::new("pw-cat")
            .args(["--playback", "--raw", "--format", "s16", "--rate", &rate])
            .args(["--channels", "1", "--latency", "20ms", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    let try_aplay = || {
        Command::new("aplay")
            .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r", &rate])
            .args(["--buffer-time=40000", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    match backend {
        "pw-cat" => try_pw().map_err(|e| anyhow!("pw-cat : {e}")),
        "aplay" => try_aplay().map_err(|e| anyhow!("aplay : {e}")),
        "none" => Err(anyhow!("audio désactivé")),
        _ => try_pw()
            .or_else(|_| try_aplay())
            .map_err(|e| anyhow!("ni pw-cat ni aplay disponibles : {e}")),
    }
}

impl Audio {
    /// `backend` : "auto", "pw-cat", "aplay" ou "none" (muet, pour les tests). N'échoue jamais : sans lecteur,
    /// l'audio est muet mais le timing (PlaySpan, pending) reste simulé.
    pub fn new(backend: &str, freq: f32, volume: f32) -> Audio {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            queued: AtomicU64::new(0),
            key_down: AtomicBool::new(false),
            freq: AtomicU32::new(freq.to_bits()),
            volume: AtomicU32::new(volume.to_bits()),
            quit: AtomicBool::new(false),
        });
        let mut child = spawn_player(backend).ok();
        let sink = child.as_mut().and_then(|c| c.stdin.take());
        let available = sink.is_some();
        let sh = shared.clone();
        thread::spawn(move || run(sh, sink));
        Audio { shared, child, available }
    }

    pub fn set_tone(&self, freq: f32, volume: f32) {
        self.shared.freq.store(freq.to_bits(), Ordering::Relaxed);
        self.shared.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Met des segments en file (à la suite de ce qui joue déjà).
    pub fn play_segments(&self, segs: &[Seg]) -> PlaySpan {
        let mut q = self.shared.queue.lock().unwrap();
        let before = self.shared.queued.load(Ordering::Relaxed);
        let mut added = 0u64;
        for s in segs {
            let n = (s.secs * RATE as f32).round() as u64;
            if n > 0 {
                q.push_back((s.on, n));
                added += n;
            }
        }
        self.shared.queued.fetch_add(added, Ordering::Relaxed);
        let start = Instant::now() + LEAD + samples_dur(before);
        PlaySpan { start, end: start + samples_dur(added) }
    }

    /// Joue un texte (sans silence final). Se met en file après l'audio en cours.
    pub fn play(&self, text: &str, t: Timing) -> PlaySpan {
        self.play_segments(&morse::text_segments(text, t))
    }

    /// Ajoute un silence en file.
    pub fn silence(&self, d: Duration) -> PlaySpan {
        self.play_segments(&[Seg { on: false, secs: d.as_secs_f32() }])
    }

    /// Vide la file : le son s'arrête (avec fondu) presque immédiatement.
    pub fn stop(&self) {
        let mut q = self.shared.queue.lock().unwrap();
        q.clear();
        self.shared.queued.store(0, Ordering::Relaxed);
    }

    /// Durée d'audio encore en file.
    pub fn pending(&self) -> Duration {
        samples_dur(self.shared.queued.load(Ordering::Relaxed))
    }

    pub fn is_idle(&self) -> bool {
        self.shared.queued.load(Ordering::Relaxed) == 0
    }

    /// Manipulateur : son continu tant que `down` est vrai (mode émission).
    pub fn set_key(&self, down: bool) {
        self.shared.key_down.store(down, Ordering::Relaxed);
    }

    /// Attend la fin de la file (usage CLI, pas dans l'interface).
    pub fn wait_idle(&self) {
        while !self.is_idle() {
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(LEAD + Duration::from_millis(150));
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        if let Some(mut c) = self.child.take() {
            thread::sleep(Duration::from_millis(30));
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn samples_dur(n: u64) -> Duration {
    Duration::from_secs_f64(n as f64 / RATE as f64)
}

fn run(sh: Arc<Shared>, mut sink: Option<std::process::ChildStdin>) {
    let ramp_step = 1.0 / (RAMP_SECS * RATE as f32);
    let (mut env, mut phase) = (0.0f32, 0.0f32);
    let t0 = Instant::now();
    let mut written: u64 = 0;
    let mut buf = [0u8; CHUNK * 2];
    while !sh.quit.load(Ordering::Relaxed) {
        // Rester au plus LEAD en avance sur le temps réel.
        let ahead = samples_dur(written).saturating_sub(t0.elapsed());
        if ahead > LEAD {
            thread::sleep(ahead - LEAD);
        }
        let freq = f32::from_bits(sh.freq.load(Ordering::Relaxed));
        let vol = f32::from_bits(sh.volume.load(Ordering::Relaxed));
        let dphase = std::f32::consts::TAU * freq / RATE as f32;
        let key = sh.key_down.load(Ordering::Relaxed);
        {
            let mut q = sh.queue.lock().unwrap();
            let mut consumed = 0u64;
            for i in 0..CHUNK {
                let on = match q.front_mut() {
                    Some(seg) => {
                        let on = seg.0;
                        seg.1 -= 1;
                        consumed += 1;
                        if seg.1 == 0 {
                            q.pop_front();
                        }
                        on
                    }
                    None => false,
                } || key;
                env = if on { (env + ramp_step).min(1.0) } else { (env - ramp_step).max(0.0) };
                let gain = 0.5 - 0.5 * (std::f32::consts::PI * env).cos();
                phase = (phase + dphase) % std::f32::consts::TAU;
                let s = (phase.sin() * gain * vol * 0.8 * i16::MAX as f32) as i16;
                buf[i * 2..i * 2 + 2].copy_from_slice(&s.to_le_bytes());
            }
            // saturating : stop() peut avoir remis le compteur à zéro entre-temps
            let cur = sh.queued.load(Ordering::Relaxed);
            sh.queued.store(cur.saturating_sub(consumed), Ordering::Relaxed);
        }
        written += CHUNK as u64;
        if let Some(s) = sink.as_mut() {
            if s.write_all(&buf).is_err() {
                sink = None; // lecteur mort : on continue en muet
            }
        }
    }
}
