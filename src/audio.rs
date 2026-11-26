//! Sortie audio : sinusoïde générée à la volée.
//!
//! - Linux : PCM brut envoyé à `pw-cat` ou `aplay` (aucune dépendance de
//!   compilation sur ALSA). Un thread écrit le flux en temps réel par petits blocs.
//! - Windows / macOS : sortie native via `cpal`, générée dans le rappel audio.
//!
//! Dans les deux cas le timing est précis à l'échantillon près.

use crate::morse::{self, Seg, Timing};
use std::collections::VecDeque;
use std::io::Write;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PIPE_RATE: u32 = 44_100;
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
    rate: AtomicU32,                     // échantillons par seconde de la sortie
    key_down: AtomicBool,                // manipulateur (mode émission)
    freq: AtomicU32,                     // Hz (f32 bits)
    volume: AtomicU32,                   // 0..1 (f32 bits)
    suspended: AtomicBool,               // sortie native en pause (arrière-plan)
    quit: AtomicBool,
}

/// Générateur : enveloppe et phase conservées d'un bloc au suivant.
#[derive(Default)]
struct Gen {
    env: f32,
    phase: f32,
}

impl Gen {
    /// Produit `frames` échantillons (-1..1) en consommant la file.
    fn render(&mut self, sh: &Shared, frames: usize, mut put: impl FnMut(usize, f32)) {
        let rate = sh.rate.load(Ordering::Relaxed) as f32;
        let ramp_step = 1.0 / (RAMP_SECS * rate);
        let freq = f32::from_bits(sh.freq.load(Ordering::Relaxed));
        let vol = f32::from_bits(sh.volume.load(Ordering::Relaxed));
        let dphase = std::f32::consts::TAU * freq / rate;
        let key = sh.key_down.load(Ordering::Relaxed);
        let mut q = sh.queue.lock().unwrap();
        let mut consumed = 0u64;
        for i in 0..frames {
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
            self.env = if on { (self.env + ramp_step).min(1.0) } else { (self.env - ramp_step).max(0.0) };
            let gain = 0.5 - 0.5 * (std::f32::consts::PI * self.env).cos();
            self.phase = (self.phase + dphase) % std::f32::consts::TAU;
            put(i, self.phase.sin() * gain * vol * 0.8);
        }
        // saturating : stop() peut avoir remis le compteur à zéro entre-temps
        let cur = sh.queued.load(Ordering::Relaxed);
        sh.queued.store(cur.saturating_sub(consumed), Ordering::Relaxed);
    }
}

pub struct Audio {
    shared: Arc<Shared>,
    child: Option<Child>,
    /// `false` si aucune sortie audio n'a pu être ouverte (mode muet).
    pub available: bool,
}

#[cfg(target_os = "linux")]
fn spawn_player(backend: &str) -> Option<Child> {
    use std::process::{Command, Stdio};
    let rate = PIPE_RATE.to_string();
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
        "pw-cat" => try_pw().ok(),
        "aplay" => try_aplay().ok(),
        _ => try_pw().or_else(|_| try_aplay()).ok(),
    }
}

/// Ouvre la sortie native et la garde ouverte dans un thread dédié (le flux
/// cpal n'est pas transférable entre threads). Renvoie `true` si elle joue.
#[cfg(not(target_os = "linux"))]
fn start_native(sh: Arc<Shared>) -> bool {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, OutputCallbackInfo, SampleFormat, SizedSample, StreamConfig};

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: StreamConfig,
        sh: Arc<Shared>,
    ) -> Option<cpal::Stream> {
        let channels = config.channels as usize;
        let mut generator = Gen::default();
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &OutputCallbackInfo| {
                    let frames = data.len() / channels;
                    generator.render(&sh, frames, |i, v| {
                        let s = T::from_sample(v);
                        for out in &mut data[i * channels..(i + 1) * channels] {
                            *out = s;
                        }
                    });
                },
                |_| {},
                None,
            )
            .ok()
    }

    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let stream = (|| {
            let device = cpal::default_host().default_output_device()?;
            let supported = device.default_output_config().ok()?;
            let format = supported.sample_format();
            let config: StreamConfig = supported.into();
            sh.rate.store(config.sample_rate, Ordering::Relaxed);
            let stream = match format {
                SampleFormat::I8 => build::<i8>(&device, config, sh.clone()),
                SampleFormat::I16 => build::<i16>(&device, config, sh.clone()),
                SampleFormat::I32 => build::<i32>(&device, config, sh.clone()),
                SampleFormat::U8 => build::<u8>(&device, config, sh.clone()),
                SampleFormat::U16 => build::<u16>(&device, config, sh.clone()),
                SampleFormat::U32 => build::<u32>(&device, config, sh.clone()),
                SampleFormat::F32 => build::<f32>(&device, config, sh.clone()),
                SampleFormat::F64 => build::<f64>(&device, config, sh.clone()),
                _ => None,
            }?;
            stream.play().ok()?;
            Some(stream)
        })();
        let _ = tx.send(stream.is_some());
        if let Some(stream) = stream {
            let mut paused = false;
            while !sh.quit.load(Ordering::Relaxed) {
                // Application en arrière-plan : rendre la sortie au système.
                let want = sh.suspended.load(Ordering::Relaxed);
                if want != paused {
                    let _ = if want { stream.pause() } else { stream.play() };
                    paused = want;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
}

impl Audio {
    /// `backend` : "auto", "none" (muet, pour les tests) et, sous Linux,
    /// "pw-cat" ou "aplay". N'échoue jamais : sans sortie, l'audio est muet
    /// mais le timing (PlaySpan, pending) reste simulé.
    pub fn new(backend: &str, freq: f32, volume: f32) -> Audio {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            queued: AtomicU64::new(0),
            rate: AtomicU32::new(PIPE_RATE),
            key_down: AtomicBool::new(false),
            freq: AtomicU32::new(freq.to_bits()),
            volume: AtomicU32::new(volume.to_bits()),
            suspended: AtomicBool::new(false),
            quit: AtomicBool::new(false),
        });
        #[allow(unused_mut)]
        let mut child: Option<Child> = None;
        #[allow(unused_mut)]
        let mut sink: Option<Box<dyn Write + Send>> = None;
        let mut available = false;

        #[cfg(target_os = "linux")]
        if backend != "none" {
            child = spawn_player(backend);
            if let Some(stdin) = child.as_mut().and_then(|c| c.stdin.take()) {
                sink = Some(Box::new(stdin));
                available = true;
            }
        }
        #[cfg(not(target_os = "linux"))]
        if backend != "none" {
            available = start_native(shared.clone());
        }

        if !available || sink.is_some() {
            // Flux cadencé par un thread : vers le lecteur externe, ou dans le
            // vide (muet) pour que le timing reste réaliste.
            shared.rate.store(PIPE_RATE, Ordering::Relaxed);
            let sh = shared.clone();
            thread::spawn(move || run_paced(sh, sink));
        }
        Audio { shared, child, available }
    }

    fn samples(&self, secs: f32) -> u64 {
        (secs * self.shared.rate.load(Ordering::Relaxed) as f32).round() as u64
    }

    fn dur(&self, n: u64) -> Duration {
        Duration::from_secs_f64(n as f64 / self.shared.rate.load(Ordering::Relaxed) as f64)
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
            let n = self.samples(s.secs);
            if n > 0 {
                q.push_back((s.on, n));
                added += n;
            }
        }
        self.shared.queued.fetch_add(added, Ordering::Relaxed);
        let start = Instant::now() + LEAD + self.dur(before);
        PlaySpan { start, end: start + self.dur(added) }
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
        self.dur(self.shared.queued.load(Ordering::Relaxed))
    }

    pub fn is_idle(&self) -> bool {
        self.shared.queued.load(Ordering::Relaxed) == 0
    }

    /// Manipulateur : son continu tant que `down` est vrai (mode émission).
    pub fn set_key(&self, down: bool) {
        self.shared.key_down.store(down, Ordering::Relaxed);
    }

    /// Met la sortie native en pause (application en arrière-plan) ou la
    /// relance. La file est vidée : rien ne reprend au milieu d'un caractère.
    pub fn suspend(&self, on: bool) {
        self.stop();
        self.set_key(false);
        self.shared.suspended.store(on, Ordering::Relaxed);
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

fn run_paced(sh: Arc<Shared>, mut sink: Option<Box<dyn Write + Send>>) {
    let mut generator = Gen::default();
    let t0 = Instant::now();
    let mut written: u64 = 0;
    let mut buf = [0u8; CHUNK * 2];
    while !sh.quit.load(Ordering::Relaxed) {
        // Rester au plus LEAD en avance sur le temps réel.
        let ahead = Duration::from_secs_f64(written as f64 / PIPE_RATE as f64)
            .saturating_sub(t0.elapsed());
        if ahead > LEAD {
            thread::sleep(ahead - LEAD);
        }
        generator.render(&sh, CHUNK, |i, v| {
            let s = (v * i16::MAX as f32) as i16;
            buf[i * 2..i * 2 + 2].copy_from_slice(&s.to_le_bytes());
        });
        written += CHUNK as u64;
        if let Some(s) = sink.as_mut() {
            if s.write_all(&buf).is_err() {
                sink = None; // lecteur mort : on continue en muet
            }
        }
    }
}
