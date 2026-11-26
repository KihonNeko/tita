//! Boucle d'événements : fenêtre, doigts, clavier, rythme d'affichage.

use crate::view::{Insets, View};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tita::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Période de la logique temporelle (minuteries, décodage du manipulateur).
const TICK: Duration = Duration::from_millis(5);
/// Période d'affichage quand rien ne presse.
const FRAME: Duration = Duration::from_millis(33);
/// Identifiant du « doigt » de la souris, dans la fenêtre d'essai.
const MOUSE: u64 = u64::MAX;

type Surface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

struct App {
    view: View,
    /// Taille de la fenêtre d'essai sur PC, en pixels logiques.
    size: Option<(u32, u32)>,
    window: Option<Arc<Window>>,
    surface: Option<Surface>,
    /// Une entrée vient d'arriver : composer sans attendre la prochaine image.
    dirty: bool,
    last_frame: Instant,
    mouse: (f64, f64),
}

/// Touche du clavier (physique, ou « retour » d'Android) -> touche de terminal.
fn key_code(key: &Key) -> Option<KeyCode> {
    Some(match key {
        Key::Named(named) => match named {
            NamedKey::Enter => KeyCode::Enter,
            // Le geste ou bouton « retour » d'Android arrive comme une touche.
            NamedKey::Escape | NamedKey::BrowserBack | NamedKey::GoBack => KeyCode::Esc,
            NamedKey::Backspace => KeyCode::Backspace,
            NamedKey::Delete => KeyCode::Delete,
            NamedKey::Tab => KeyCode::Tab,
            NamedKey::Space => KeyCode::Char(' '),
            NamedKey::ArrowUp => KeyCode::Up,
            NamedKey::ArrowDown => KeyCode::Down,
            NamedKey::ArrowLeft => KeyCode::Left,
            NamedKey::ArrowRight => KeyCode::Right,
            NamedKey::Home => KeyCode::Home,
            NamedKey::End => KeyCode::End,
            NamedKey::PageUp => KeyCode::PageUp,
            NamedKey::PageDown => KeyCode::PageDown,
            _ => return None,
        },
        Key::Character(s) => KeyCode::Char(s.chars().next()?),
        _ => return None,
    })
}

impl App {
    /// Marges des barres système (Android) ; aucune ailleurs.
    fn insets(&self, _window: &Window, _w: usize, _h: usize) -> Insets {
        #[cfg(target_os = "android")]
        {
            use winit::platform::android::WindowExtAndroid;
            let r = _window.content_rect();
            let (l, t, rt, b) = (r.left.max(0) as usize, r.top.max(0) as usize, r.right.max(0) as usize, r.bottom.max(0) as usize);
            // Tant que le système n'a rien dit, le rectangle est vide.
            if rt > l && b > t && rt <= _w && b <= _h {
                return Insets { left: l, top: t, right: _w - rt, bottom: _h - b };
            }
        }
        Insets::default()
    }

    fn relayout(&mut self) {
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        let (w, h) = (size.width as usize, size.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let insets = self.insets(&window, w, h);
        self.view.layout(w, h, window.scale_factor() as f32, insets);
        self.dirty = true;
    }

    fn paint(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else { return };
        self.view.paint(&mut buffer, w.get() as usize, h.get() as usize);
        if let Err(e) = buffer.present() {
            log::warn!("affichage : {e}");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let mut attrs = Window::default_attributes().with_title("tita");
            if let Some((w, h)) = self.size {
                attrs = attrs.with_inner_size(LogicalSize::new(w, h));
            }
            match event_loop.create_window(attrs) {
                Ok(w) => self.window = Some(Arc::new(w)),
                Err(e) => {
                    log::error!("fenêtre : {e}");
                    event_loop.exit();
                    return;
                }
            }
        }
        // Sur Android la surface meurt à chaque mise en arrière-plan : on la
        // recrée à chaque retour.
        let window = self.window.clone().expect("fenêtre créée");
        self.surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()))
            .map_err(|e| log::error!("surface : {e}"))
            .ok();
        self.view.shell.resume();
        self.relayout();
        window.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface = None;
        self.view.shell.pause();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.relayout(),
            WindowEvent::RedrawRequested => {
                self.view.compose();
                self.paint();
            }
            WindowEvent::Touch(touch) => {
                let (x, y) = (touch.location.x, touch.location.y);
                match touch.phase {
                    TouchPhase::Started => self.view.touch_down(touch.id, x, y),
                    TouchPhase::Moved => self.view.touch_move(touch.id, x, y),
                    TouchPhase::Ended | TouchPhase::Cancelled => self.view.touch_up(touch.id),
                }
                self.dirty = true;
            }
            // Fenêtre d'essai sur PC : la souris fait office de doigt.
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x, position.y);
                self.view.touch_move(MOUSE, position.x, position.y);
                self.dirty = true;
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                match state {
                    ElementState::Pressed => self.view.touch_down(MOUSE, self.mouse.0, self.mouse.1),
                    ElementState::Released => self.view.touch_up(MOUSE),
                }
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. } if !event.repeat => {
                if let Some(code) = key_code(&event.logical_key) {
                    let key = KeyEvent::new(code, KeyModifiers::NONE);
                    match event.state {
                        ElementState::Pressed => {
                            self.view.shell.key(key);
                        }
                        ElementState::Released => self.view.shell.release(key),
                    }
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.view.shell.running() {
            event_loop.exit();
            return;
        }
        if self.surface.is_none() {
            // En arrière-plan : plus rien à animer, on dort jusqu'au retour.
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        self.view.shell.tick();
        if !self.view.shell.running() {
            event_loop.exit();
            return;
        }
        if self.dirty || self.last_frame.elapsed() >= FRAME {
            self.dirty = false;
            self.last_frame = Instant::now();
            if self.view.compose() {
                self.paint();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + TICK));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.view.shell.close();
    }
}

/// Lance l'application dans une fenêtre. `size` : taille de la fenêtre
/// d'essai sur PC, en pixels logiques (ignorée sur Android, plein écran).
pub fn run(event_loop: EventLoop<()>, size: Option<(u32, u32)>) {
    let mut app = App {
        view: View::new(crate::start()),
        size,
        window: None,
        surface: None,
        dirty: true,
        last_frame: Instant::now(),
        mouse: (0.0, 0.0),
    };
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("boucle d'événements : {e}");
    }
}
