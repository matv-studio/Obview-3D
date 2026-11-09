//! Fenêtre de l'application : boucle d'événements, entrées, navigation, sélection au clic, présentation.
//! Une image n'est rendue que lorsqu'une entrée, une animation ou un chargement l'exige.

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Instant};

use obv_gpu::{FRAMES_IN_FLIGHT, Gpu, GpuDesc, PresentMode, Swapchain};
use obv_io::Model;
use obv_render::{Camera, SceneRenderer};
use obv_ui::{Cursor, FrameInput, Id, Key, Modifiers, Ui, Vec2, gpu::UiRenderer, vec2};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, NamedKey, PhysicalKey},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{CursorIcon, Theme as WinTheme, Window, WindowId},
};

use crate::{
    demo,
    frame::{EdgeCache, SceneKey, draw_viewport, mesh_inputs},
    inspector::{self, Inspector},
    navigation::{NavContext, Navigator, WHEEL_NOTCH_PX},
    settings::{Settings, resolve},
    shell::{self, AppState},
};

/// Entrées accumulées entre deux images.
#[derive(Default)]
struct Pending {
    pointer: Option<Vec2>,
    buttons: [bool; 3],
    /// Appuis survenus depuis la dernière image : visibles au moins une image même si le bouton est déjà relâché.
    latched: [bool; 3],
    modifiers: Modifiers,
    /// Modificateurs tenus au moment d'une frappe (Ctrl+F reçu en un seul lot d'événements).
    key_mods: Modifiers,
    scroll: Vec2,
    text: String,
    keys: Vec<Key>,
    held: HashSet<Key>,
}

/// Ressources graphiques. Ordre des champs = ordre de libération : la chaîne d'échange avant la fenêtre.
struct Gfx {
    swapchain: Swapchain,
    scene: SceneRenderer,
    ui_renderer: UiRenderer,
    window: Arc<Window>,
    gpu: Gpu,
}

pub struct App {
    gfx: Option<Gfx>,
    ui: Ui,
    state: AppState,
    model: Model,
    nav: Navigator,
    pending: Pending,
    start: Instant,
    scene_key: Option<SceneKey>,
    edges: Option<EdgeCache>,
    framed: bool,
    /// Appui gauche dans la vue 3D : position, pour distinguer un clic d'un glisser.
    click_origin: Option<Vec2>,
    last_save: f64,
    error: Option<String>,
}

fn load(path: &std::path::Path) -> Result<Model, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "glb" => obv_io::read_glb(path).map_err(|e| e.to_string()),
        _ => Err(format!("format non pris en charge pour l'instant : .{ext}")),
    }
}

impl App {
    pub fn new(path: Option<PathBuf>) -> Self {
        let (model, name) = match &path {
            Some(p) => match load(p) {
                Ok(m) => (m, p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                Err(e) => {
                    eprintln!("Obview 3D : {e}");
                    (demo::bracket(), "support.step".to_owned())
                }
            },
            None => (demo::bracket(), "support.step".to_owned()),
        };
        let mut state = AppState::new(&name, &model);
        state.settings = Settings::load();
        let ui = Ui::new(shell::theme(&state));
        App {
            gfx: None,
            ui,
            state,
            model,
            nav: Navigator::default(),
            pending: Pending::default(),
            start: Instant::now(),
            scene_key: None,
            edges: None,
            framed: false,
            click_origin: None,
            last_save: 0.0,
            error: None,
        }
    }

    fn open(&mut self, path: PathBuf) {
        match load(&path) {
            Ok(m) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let mut s = AppState::new(&name, &m);
                std::mem::swap(&mut s.settings, &mut self.state.settings);
                s.system_dark = self.state.system_dark;
                s.viewport = self.state.viewport.clone();
                self.state = s;
                self.model = m;
                self.scene_key = None;
                self.edges = None;
                self.framed = false;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn init_graphics(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let attrs = Window::default_attributes()
            .with_title("Obview 3D")
            .with_inner_size(LogicalSize::new(1600.0, 900.0))
            .with_min_inner_size(LogicalSize::new(960.0, 600.0));
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let display = window.display_handle().map_err(|e| e.to_string())?.as_raw();
        let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let exts = obv_gpu::required_instance_extensions(display).map_err(|e| e.to_string())?;
        let validation = std::env::args().any(|a| a == "--validation");
        let gpu = Gpu::new(&GpuDesc { validation, instance_extensions: exts, presentation: true })
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        // SAFETY: la fenêtre vit dans `Gfx` aussi longtemps que la chaîne d'échange (ordre des champs).
        let swapchain = unsafe {
            Swapchain::new(&gpu, display, handle, (size.width, size.height), PresentMode::LowLatency)
                .map_err(|e| e.to_string())?
        };
        let scene = SceneRenderer::new(&gpu, swapchain.format, swapchain.depth_format).map_err(|e| e.to_string())?;
        let ui_renderer = UiRenderer::new(&gpu, swapchain.format, Some(swapchain.depth_format), 2048, FRAMES_IN_FLIGHT)
            .map_err(|e| e.to_string())?;
        self.state.system_dark = window.theme().is_none_or(|t| t == WinTheme::Dark);
        self.gfx = Some(Gfx { gpu, window, swapchain, scene, ui_renderer });
        Ok(())
    }

    /// Entrées de l'image. Le second résultat demande une image de plus (relâchement d'un appui éclair).
    fn frame_input(p: &mut Pending, start: Instant, size: Vec2, scale: f32) -> (FrameInput, bool) {
        let buttons = std::array::from_fn(|i| p.buttons[i] || p.latched[i]);
        let again = (0..3).any(|i| p.latched[i] && !p.buttons[i]);
        p.latched = [false; 3];
        let m = std::mem::take(&mut p.key_mods);
        let modifiers = Modifiers {
            shift: p.modifiers.shift || m.shift,
            ctrl: p.modifiers.ctrl || m.ctrl,
            alt: p.modifiers.alt || m.alt,
        };
        let input = FrameInput {
            size,
            scale,
            time: start.elapsed().as_secs_f64(),
            pointer: p.pointer,
            buttons,
            modifiers,
            scroll: std::mem::take(&mut p.scroll),
            text: std::mem::take(&mut p.text),
            keys: std::mem::take(&mut p.keys),
            held: p.held.iter().copied().collect(),
        };
        (input, again)
    }

    fn redraw(&mut self) {
        let Some(gfx) = &mut self.gfx else { return };
        let phys = gfx.window.inner_size();
        if phys.width == 0 || phys.height == 0 {
            return;
        }
        let scale = gfx.window.scale_factor() as f32;
        let size = vec2(phys.width as f32 / scale, phys.height as f32 / scale);
        let (input, again) = Self::frame_input(&mut self.pending, self.start, size, scale);
        let now = input.time;
        let prev_buttons = self.ui.input.buttons;
        self.ui.begin_frame(input.clone());
        shell::handle(&mut self.ui, &mut self.state);

        // Navigation et sélection dans la vue 3D.
        let view = self.state.viewport.get();
        let over_ui = self.ui.pointer_over_ui() || self.ui.is_active_any();
        let scene = &gfx.scene;
        let cam_now = self.state.camera;
        let mut pick = |p: Vec2| -> Option<[f64; 3]> {
            let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
            scene.pick(&cam_now, [view.width() * scale, view.height() * scale], px).ok().flatten().map(|h| h.position)
        };
        let ctx = NavContext { input: &input, view, over_ui, settings: &self.state.settings };
        let moved = self.nav.update(&ctx, &mut self.state.camera, &mut pick);
        let left_pressed = input.buttons[0] && !prev_buttons[0];
        let left_released = !input.buttons[0] && prev_buttons[0];
        let plain_left =
            resolve(self.state.settings.navigation, obv_ui::MouseButton::Left, input.modifiers, &input.held).is_none();
        if left_pressed
            && plain_left
            && !over_ui
            && let Some(p) = input.pointer
            && view.contains(p)
        {
            self.click_origin = Some(p);
        }
        if left_released && let (Some(o), Some(p)) = (self.click_origin.take(), input.pointer) {
            let d = p - o;
            if d.x.abs() + d.y.abs() < 4.0 && self.state.inspector == Inspector::Bands {
                if let Some(m) = inspector::band_at(view, &self.state, p) {
                    self.state.inspector = Inspector::Single(m);
                }
            } else if d.x.abs() + d.y.abs() < 4.0 {
                let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
                let hit = gfx.scene.pick(&self.state.camera, [view.width() * scale, view.height() * scale], px);
                self.state.selected = hit.ok().flatten().map(|h| h.id as usize);
            }
        }
        if self.ui.clicked(Id::new("cadrer"))
            || (!self.ui.text_focused() && input.keys.contains(&Key::Char('f')) && !input.modifiers.ctrl)
        {
            frame_all(&mut self.nav, &self.state.camera, &self.model, view, now);
        }
        if self.ui.clicked(Id::new("cadrer pièce"))
            && let Some(i) = self.state.selected
            && let Some((lo, hi)) = self.model.parts.get(i).and_then(|p| p.mesh.bounds())
        {
            let mut to = self.state.camera;
            to.frame(lo, hi, (view.width() / view.height().max(1.0)) as f64);
            self.nav.animate_to(&self.state.camera, to, now);
        }
        if !self.framed && view.width() > 1.0 {
            if let Some((lo, hi)) = self.model.bounds() {
                self.state.camera = Camera::default();
                self.state.camera.frame(lo, hi, (view.width() / view.height()) as f64);
            }
            self.framed = true;
        }

        self.ui.theme = shell::theme(&self.state);
        let mut root = shell::build(&mut self.ui, &mut self.state);
        self.ui.finish(&mut root);

        // Géométrie : envoyée seulement quand la sélection, la visibilité ou l'accent changent.
        let accent = to_srgb8(self.ui.theme.color.accent);
        if self.state.inspector != Inspector::Off && self.edges.is_none() {
            self.edges = Some(EdgeCache::compute(&self.model));
        }
        let key = SceneKey::new(&self.state, accent, self.edges.is_some());
        if self.scene_key.as_ref() != Some(&key) {
            let meshes = mesh_inputs(&self.model, &self.state, accent, self.edges.as_ref());
            if let Err(e) = gfx.scene.set_meshes(&meshes) {
                self.error = Some(e.to_string());
            }
            self.scene_key = Some(key);
        }

        let c = &self.ui.theme.color;
        let bg = [c.app_bg.r, c.app_bg.g, c.app_bg.b, 1.0];
        let extent = gfx.swapchain.extent;
        let (ui_r, scene_r, dl, atlas, theme, state) =
            (&mut gfx.ui_renderer, &gfx.scene, &self.ui.dl, &mut self.ui.atlas, &self.ui.theme, &self.state);
        let mut prep_err = None;
        let ui_cell = std::cell::RefCell::new(ui_r);
        let r = gfx.swapchain.present(
            bg,
            |cmd, slot| {
                if let Err(e) = ui_cell.borrow_mut().prepare(cmd, slot, dl, atlas) {
                    prep_err = Some(e);
                }
            },
            |cmd| {
                draw_viewport(scene_r, cmd, state, theme, scale);
                ui_cell.borrow().draw(cmd, extent);
            },
        );
        if let Err(e) = r {
            self.error = Some(e.to_string());
        }
        if let Some(e) = prep_err {
            self.error = Some(e.to_string());
        }
        if let Some(e) = self.error.take() {
            eprintln!("Obview 3D : {e}");
        }

        // Pointeur : forme demandée par l'interface, ou celle du geste de navigation.
        let icon = if self.nav.dragging() {
            CursorIcon::Grabbing
        } else {
            match self.ui.cursor {
                Cursor::Default => CursorIcon::Default,
                Cursor::Pointer => CursorIcon::Pointer,
                Cursor::Text => CursorIcon::Text,
                Cursor::Grab => CursorIcon::Grab,
                Cursor::Grabbing => CursorIcon::Grabbing,
                Cursor::ResizeHorizontal => CursorIcon::EwResize,
                Cursor::ResizeVertical => CursorIcon::NsResize,
            }
        };
        gfx.window.set_cursor(icon);

        if self.state.settings_dirty && !self.ui.is_active_any() && now - self.last_save > 0.3 {
            if let Err(e) = self.state.settings.save() {
                eprintln!("Obview 3D : réglages non enregistrés : {e}");
            }
            self.state.settings_dirty = false;
            self.last_save = now;
        }
        if again || moved || self.nav.animating() || self.ui.wants_redraw() || self.state.settings_dirty || !self.framed
        {
            gfx.window.request_redraw();
        }
    }
}

fn frame_all(nav: &mut Navigator, cam: &Camera, model: &Model, view: obv_ui::Rect, now: f64) {
    if let Some((lo, hi)) = model.bounds() {
        let mut to = *cam;
        to.frame(lo, hi, (view.width() / view.height().max(1.0)) as f64);
        nav.animate_to(cam, to, now);
    }
}

fn to_srgb8(c: obv_ui::Color) -> [u8; 4] {
    let h = c.to_hex();
    [(h >> 16) as u8, (h >> 8) as u8, h as u8, 255]
}

fn map_key(code: PhysicalKey, logical: &winit::keyboard::Key) -> Option<Key> {
    if let PhysicalKey::Code(c) = code {
        let numpad = match c {
            KeyCode::Numpad0 => Some(Key::Numpad(0)),
            KeyCode::Numpad1 => Some(Key::Numpad(1)),
            KeyCode::Numpad2 => Some(Key::Numpad(2)),
            KeyCode::Numpad3 => Some(Key::Numpad(3)),
            KeyCode::Numpad4 => Some(Key::Numpad(4)),
            KeyCode::Numpad5 => Some(Key::Numpad(5)),
            KeyCode::Numpad6 => Some(Key::Numpad(6)),
            KeyCode::Numpad7 => Some(Key::Numpad(7)),
            KeyCode::Numpad8 => Some(Key::Numpad(8)),
            KeyCode::Numpad9 => Some(Key::Numpad(9)),
            KeyCode::NumpadDecimal => Some(Key::NumpadDecimal),
            KeyCode::NumpadAdd => Some(Key::NumpadAdd),
            KeyCode::NumpadSubtract => Some(Key::NumpadSubtract),
            KeyCode::NumpadEnter => Some(Key::Enter),
            // Rangée des chiffres : par position, pour les claviers AZERTY (Cinema 4D : 1, 2, 3 tenus).
            KeyCode::Digit1 => Some(Key::Char('1')),
            KeyCode::Digit2 => Some(Key::Char('2')),
            KeyCode::Digit3 => Some(Key::Char('3')),
            _ => None,
        };
        if numpad.is_some() {
            return numpad;
        }
    }
    use winit::keyboard::Key as L;
    match logical {
        L::Named(n) => Some(match n {
            NamedKey::Escape => Key::Escape,
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Space => Key::Space,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            _ => return None,
        }),
        L::Character(s) => s.chars().next().map(|c| Key::Char(c.to_lowercase().next().unwrap_or(c))),
        _ => None,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_none() {
            if let Err(e) = self.init_graphics(el) {
                eprintln!("Obview 3D : {e}");
                el.exit();
                return;
            }
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = &mut self.gfx else { return };
        let scale = gfx.window.scale_factor() as f32;
        let mut redraw = true;
        match event {
            WindowEvent::CloseRequested => {
                if gfx.gpu.validation_errors() > 0 {
                    eprintln!("Obview 3D : {} erreurs de validation Vulkan", gfx.gpu.validation_errors());
                }
                if self.state.settings_dirty {
                    let _ = self.state.settings.save();
                }
                el.exit();
                return;
            }
            WindowEvent::Resized(s) => gfx.swapchain.resize(s.width, s.height),
            WindowEvent::ScaleFactorChanged { .. } => {
                let s = gfx.window.inner_size();
                gfx.swapchain.resize(s.width, s.height);
            }
            WindowEvent::ThemeChanged(t) => self.state.system_dark = t == WinTheme::Dark,
            WindowEvent::CursorMoved { position, .. } => {
                self.pending.pointer = Some(vec2(position.x as f32 / scale, position.y as f32 / scale));
            }
            WindowEvent::CursorLeft { .. } => {
                if !self.pending.buttons.iter().any(|b| *b) {
                    self.pending.pointer = None;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let i = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                    _ => return,
                };
                let down = state == ElementState::Pressed;
                self.pending.buttons[i] = down;
                if down {
                    self.pending.latched[i] = true;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(x, y) => vec2(x * WHEEL_NOTCH_PX, y * WHEEL_NOTCH_PX),
                    MouseScrollDelta::PixelDelta(p) => vec2(p.x as f32 / scale, p.y as f32 / scale),
                };
                self.pending.scroll = self.pending.scroll + d;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.pending.modifiers = Modifiers { shift: s.shift_key(), ctrl: s.control_key(), alt: s.alt_key() };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(event.physical_key, &event.logical_key);
                if std::env::var_os("OBV_DEBUG_KEYS").is_some() {
                    eprintln!("touche {:?} {:?} {:?} -> {key:?}", event.physical_key, event.logical_key, event.text);
                }
                if event.state == ElementState::Pressed {
                    let m = self.pending.modifiers;
                    let km = &mut self.pending.key_mods;
                    *km = Modifiers { shift: km.shift || m.shift, ctrl: km.ctrl || m.ctrl, alt: km.alt || m.alt };
                    if let Some(k) = key {
                        self.pending.keys.push(k);
                        self.pending.held.insert(k);
                    }
                    if let Some(t) = &event.text
                        && !self.pending.modifiers.ctrl
                        && !matches!(key, Some(Key::Numpad(_)) | Some(Key::NumpadAdd) | Some(Key::NumpadSubtract))
                    {
                        self.pending.text.push_str(t);
                    }
                } else if let Some(k) = key {
                    self.pending.held.remove(&k);
                }
            }
            WindowEvent::Focused(false) => {
                self.pending.held.clear();
                self.pending.buttons = [false; 3];
            }
            WindowEvent::DroppedFile(path) => self.open(path),
            WindowEvent::RedrawRequested => {
                self.redraw();
                redraw = false;
            }
            _ => redraw = false,
        }
        if redraw && let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

/// Lance la fenêtre. `path` : fichier à ouvrir.
pub fn run(path: Option<PathBuf>) -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(path);
    el.run_app(&mut app).map_err(|e| e.to_string())
}
