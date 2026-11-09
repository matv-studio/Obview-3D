//! Fenêtre de l'application : boucle d'événements, entrées, navigation, sélection au clic, présentation.
//! Une image n'est rendue que lorsqu'une entrée, une animation ou un chargement l'exige.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{Receiver, channel},
    },
    time::Instant,
};

use obv_gpu::{FRAMES_IN_FLIGHT, Gpu, GpuDesc, PresentMode, Swapchain};
use obv_io::Model;
use obv_render::{Camera, SceneRenderer};
use obv_ui::{Cursor, FrameInput, Id, Key, Modifiers, Ui, Vec2, gpu::UiRenderer, vec2};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, NamedKey, PhysicalKey},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{CursorIcon, Theme as WinTheme, Window, WindowId},
};

use crate::{
    convert_ui::{Request, Status},
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

/// Réveil de la boucle d'événements par un fil de travail.
#[derive(Debug, Clone, Copy)]
pub struct Wake;

type Loaded = (PathBuf, Result<Model, String>);
type Converted = Result<(obv_mesh::Report, PathBuf), String>;

pub struct App {
    gfx: Option<Gfx>,
    ui: Ui,
    state: AppState,
    model: Arc<Model>,
    proxy: EventLoopProxy<Wake>,
    loading: Option<Receiver<Loaded>>,
    converting: Option<(Receiver<Converted>, Arc<AtomicU32>)>,
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
    /// Le cube de vue a été glissé depuis l'appui : le relâchement n'est pas un clic.
    cube_dragged: bool,
}

fn load(path: &std::path::Path) -> Result<Model, String> {
    obv_io::read(path).map_err(|e| e.to_string())
}

impl App {
    pub fn new(path: Option<PathBuf>, proxy: EventLoopProxy<Wake>) -> Self {
        let model = demo::bracket();
        let mut state = AppState::new("support.step", &model);
        state.settings = Settings::load();
        let ui = Ui::new(shell::theme(&state));
        let mut app = App {
            gfx: None,
            ui,
            state,
            model: Arc::new(model),
            proxy,
            loading: None,
            converting: None,
            nav: Navigator::default(),
            pending: Pending::default(),
            start: Instant::now(),
            scene_key: None,
            edges: None,
            framed: false,
            click_origin: None,
            last_save: 0.0,
            error: None,
            cube_dragged: false,
        };
        if let Some(p) = path {
            app.open(p);
        }
        app
    }

    /// Lit `path` dans un fil séparé ; l'interface reste vivante (indicateur dans la barre du haut).
    fn open(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        let proxy = self.proxy.clone();
        self.state.loading = path.file_name().map(|n| n.to_string_lossy().into_owned());
        std::thread::spawn(move || {
            let r = load(&path);
            let _ = tx.send((path, r));
            let _ = proxy.send_event(Wake);
        });
        self.loading = Some(rx);
    }

    fn finish_open(&mut self, path: PathBuf, m: Model) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut s = AppState::new(&name, &m);
        std::mem::swap(&mut s.settings, &mut self.state.settings);
        s.system_dark = self.state.system_dark;
        s.viewport = self.state.viewport.clone();
        s.inspector = self.state.inspector;
        self.state = s;
        self.model = Arc::new(m);
        self.scene_key = None;
        self.edges = None;
        self.framed = false;
    }

    /// Réponses des fils de travail.
    fn poll_jobs(&mut self) {
        if let Some(rx) = &self.loading
            && let Ok((path, r)) = rx.try_recv()
        {
            self.loading = None;
            self.state.loading = None;
            match r {
                Ok(m) => self.finish_open(path, m),
                Err(e) => self.error = Some(e),
            }
        }
        if let Some((rx, progress)) = &self.converting {
            match rx.try_recv() {
                Ok(Ok((r, path))) => {
                    self.state.convert.status = Status::Done {
                        triangles_in: r.triangles_in(),
                        triangles_out: r.triangles_out(),
                        deviation_mm: r.deviation() as f64 * self.model.unit_m * 1000.0,
                        bytes: r.bytes,
                        path,
                    };
                    self.converting = None;
                }
                Ok(Err(e)) => {
                    self.state.convert.status = Status::Failed(e);
                    self.converting = None;
                }
                Err(_) => self.state.convert.status = Status::Running(f32::from_bits(progress.load(Ordering::Relaxed))),
            }
        }
    }

    /// Demandes de l'interface : dialogues natifs, fils de travail.
    fn handle_requests(&mut self) {
        let window = self.gfx.as_ref().map(|g| g.window.clone());
        for req in std::mem::take(&mut self.state.requests) {
            match req {
                Request::OpenFile => {
                    let mut d = rfd::FileDialog::new()
                        .set_title("Ouvrir")
                        .add_filter("Modèles 3D", obv_io::EXTENSIONS)
                        .add_filter("Tous les fichiers", &["*"]);
                    if let Some(w) = &window {
                        d = d.set_parent(w.as_ref());
                    }
                    if let Some(p) = d.pick_file() {
                        self.open(p);
                    }
                }
                Request::Convert => {
                    let opts = obv_mesh::ConvertOptions {
                        format: self.state.convert.format,
                        tolerance_m: self.state.convert.tolerance_mm / 1000.0,
                        keep_silhouette: self.state.convert.keep_silhouette,
                        compress: self.state.convert.compress,
                    };
                    let stem = std::path::Path::new(&self.state.file_name)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "modele".into());
                    let ext = opts.format.extension();
                    let mut d = rfd::FileDialog::new()
                        .set_title("Convertir")
                        .set_file_name(format!("{stem}.{ext}"))
                        .add_filter(opts.format.label(), &[ext]);
                    if let Some(w) = &window {
                        d = d.set_parent(w.as_ref());
                    }
                    let Some(path) = d.save_file() else { continue };
                    self.start_conversion(path, opts);
                }
                Request::OpenResult(p) => self.open(p),
                Request::Reveal(p) => reveal(&p),
            }
        }
    }

    pub fn start_conversion(&mut self, path: PathBuf, opts: obv_mesh::ConvertOptions) {
        let (tx, rx) = channel();
        let progress = Arc::new(AtomicU32::new(0));
        let (model, proxy, prog) = (self.model.clone(), self.proxy.clone(), progress.clone());
        std::thread::spawn(move || {
            let p2 = proxy.clone();
            let r = obv_mesh::convert(&model, &path, opts, &|f| {
                prog.store(f.to_bits(), Ordering::Relaxed);
                let _ = p2.send_event(Wake);
            })
            .map(|r| (r, path))
            .map_err(|e| e.to_string());
            let _ = tx.send(r);
            let _ = proxy.send_event(Wake);
        });
        self.state.convert.status = Status::Running(0.0);
        self.converting = Some((rx, progress));
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
        self.poll_jobs();
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
        // Cube de vue : clic sur une face, une arête ou un coin ; glisser pour tourner.
        let cube = Id::new("cube de vue");
        if let Some(r) = self.ui.rect_of(cube) {
            let cam = self.state.camera;
            if self.ui.clicked(cube)
                && let Some(p) = input.pointer
                && !std::mem::take(&mut self.cube_dragged)
                && let Some(dir) = crate::viewcube::pick(r, cam.yaw as f32, cam.pitch as f32, p)
            {
                let (yaw, pitch) = crate::viewcube::angles(dir, cam.yaw);
                self.nav.animate_to(&cam, Camera { yaw, pitch, ..cam }, now);
            } else if self.ui.clicked(cube) {
                self.cube_dragged = false;
            } else if self.ui.dragging(cube).is_some() && !self.ui.press_started(cube) {
                let d = self.ui.pointer_delta();
                if d != Vec2::default() {
                    crate::navigation::orbit(
                        &mut self.state.camera,
                        cam.target,
                        -d.x as f64 * 0.0075,
                        d.y as f64 * 0.0075,
                    );
                    self.cube_dragged = true;
                }
            }
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
        if !self.state.requests.is_empty() {
            self.handle_requests();
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        let Some(gfx) = &mut self.gfx else { return };

        // Géométrie : envoyée seulement quand la sélection, la visibilité ou l'accent changent.
        let accent = to_srgb8(shell::selection_color(&self.ui.theme));
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

/// Montre le fichier dans le gestionnaire de fichiers du système.
fn reveal(p: &std::path::Path) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg("-R").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(p.parent().unwrap_or(p)).spawn();
    if let Err(e) = r {
        eprintln!("Obview 3D : {e}");
    }
}

impl ApplicationHandler<Wake> for App {
    fn user_event(&mut self, _el: &ActiveEventLoop, _e: Wake) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

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
    let el = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    el.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(path, el.create_proxy());
    el.run_app(&mut app).map_err(|e| e.to_string())
}
