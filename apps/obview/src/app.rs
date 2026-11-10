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

use obv_io::{Model, Progress, Stage};
use obv_render::Camera;
use obv_ui::{Cursor, FrameInput, Id, Key, Modifiers, Ui, Vec2, tr, vec2};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, NamedKey, PhysicalKey},
    window::{CursorIcon, Theme as WinTheme, Window, WindowId},
};

use crate::{
    convert_ui::{Request, Status},
    frame::{EdgeCache, SceneKey, mesh_inputs},
    gfx::Gfx,
    inspector::{self, Inspector},
    loading::{Loading, Notice},
    navigation::{NavContext, Navigator, WHEEL_NOTCH_PX},
    settings::{RendererChoice, Settings},
    shell::{self, AppState, Tool},
    tools::{self, CaptureSize, MeasurePoint, Snap},
    update,
    views::ViewStore,
};

/// Entrées accumulées entre deux images.
#[derive(Default)]
struct Pending {
    pointer: Option<Vec2>,
    buttons: [bool; 3],
    /// Appuis survenus depuis la dernière image : visibles au moins une image même si le bouton est déjà relâché.
    latched: [bool; 3],
    /// Appuis gauches depuis la dernière image (double-clic reçu en un seul lot quand les images sont lentes).
    left_presses: u8,
    modifiers: Modifiers,
    /// Modificateurs tenus au moment d'une frappe (Ctrl+F reçu en un seul lot d'événements).
    key_mods: Modifiers,
    scroll: Vec2,
    text: String,
    keys: Vec<Key>,
    held: HashSet<Key>,
}

/// Réveil de la boucle d'événements par un fil de travail.
#[derive(Debug, Clone, Copy)]
pub struct Wake;

type Loaded = (PathBuf, Result<Model, String>);

/// Réponses du fil des mises à jour (D-32).
enum UpdateMsg {
    Checked(Result<Option<update::Release>, String>, bool),
    Progress(f32),
    Installed(update::Release, Result<bool, String>),
}
type Converted = Result<(obv_mesh::Report, PathBuf), String>;

pub struct App {
    gfx: Option<Gfx>,
    ui: Ui,
    state: AppState,
    model: Arc<Model>,
    proxy: EventLoopProxy<Wake>,
    /// Lecture en cours : réponse du fil, avancement partagé, début.
    loading: Option<(Receiver<Loaded>, Arc<Progress>, Instant)>,
    /// Modèle lu, installé à l'image suivante : la carte d'ouverture montre d'abord « Préparation » (l'envoi au
    /// GPU se fait sur le fil de la fenêtre).
    ready: Option<(PathBuf, Model, bool)>,
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
    /// Vues enregistrées de tous les fichiers.
    views: ViewStore,
    /// Clé du fichier ouvert dans `views`.
    file_key: String,
    /// Dernier pointeur pour lequel le point de mesure survolé a été calculé.
    measure_at: Option<Vec2>,
    /// Dernier clic dans la vue : instant et pièce touchée (double-clic).
    last_click: Option<(f64, Option<usize>)>,
    /// Choix de moteur appliqué à la fenêtre actuelle.
    applied_renderer: RendererChoice,
    /// Appuis gauches reçus dans l'image de l'appui en cours.
    press_burst: u8,
    /// Fil des mises à jour en cours (recherche, ou téléchargement puis installation).
    updating: Option<Receiver<UpdateMsg>>,
    /// Installation lancée : fermeture après l'image en cours.
    quit: bool,
    /// Thème imposé à la barre de titre de la fenêtre (`None` : celui du système).
    window_theme: Option<WinTheme>,
    /// Préparation d'un environnement d'éclairage (clé, résultat) et des aperçus (D-33).
    env_job: Option<Receiver<(String, Result<obv_render::light::Environment, String>)>>,
    thumbs_job: Option<Receiver<crate::lighting::Thumbs>>,
}

/// Lit un fichier : STEP / IGES par OpenCASCADE (D-27), les maillages par les lecteurs d'Obview.
pub fn load(path: &std::path::Path) -> Result<Model, String> {
    load_with(path, &Progress::default())
}

/// Comme [`load`], en publiant l'avancement dans `progress` (étapes mesurées pour STEP / IGES).
pub fn load_with(path: &std::path::Path, progress: &Progress) -> Result<Model, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let r = if obv_cad::EXTENSIONS.contains(&ext.as_str()) {
        obv_cad::read_with(path, obv_cad::Options::default(), progress).map_err(|e| e.to_string())
    } else {
        progress.stage(Stage::Read);
        obv_io::read(path).map_err(|e| e.to_string())
    };
    progress.stage(Stage::Prepare);
    r
}

/// Extensions proposées à l'ouverture.
pub fn extensions() -> Vec<&'static str> {
    let mut v: Vec<&str> = Vec::new();
    if obv_cad::available() {
        v.extend_from_slice(obv_cad::EXTENSIONS);
    }
    v.extend_from_slice(obv_io::EXTENSIONS);
    v
}

impl App {
    pub fn new(path: Option<PathBuf>, proxy: EventLoopProxy<Wake>) -> Self {
        // Démarrage sans modèle : la vue propose d'ouvrir un fichier.
        let model = Model::default();
        let mut state = AppState::new("", &model);
        state.settings = Settings::load();
        state.recent = crate::recent::load();
        let ui = Ui::new(shell::theme(&state));
        let mut app = App {
            gfx: None,
            ui,
            state,
            model: Arc::new(model),
            proxy,
            loading: None,
            ready: None,
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
            views: ViewStore::load(),
            file_key: String::new(),
            measure_at: None,
            last_click: None,
            press_burst: 0,
            updating: None,
            quit: false,
            window_theme: None,
            env_job: None,
            thumbs_job: None,
            applied_renderer: RendererChoice::Auto,
        };
        app.state.views.list = app.views.0.get(&app.file_key).cloned().unwrap_or_default();
        if let Some(p) = path {
            app.open(p);
        }
        app
    }

    /// Lit `path` dans un fil séparé ; l'interface reste vivante (ligne d'activité, carte d'ouverture). Une
    /// lecture en cours est annulée.
    fn open(&mut self, path: PathBuf) {
        self.cancel_open();
        self.state.notice = None;
        let (tx, rx) = channel();
        let proxy = self.proxy.clone();
        let progress = Arc::new(Progress::default());
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.state.loading = Some(Loading::new(name));
        let p = progress.clone();
        std::thread::spawn(move || {
            let r = load_with(&path, &p);
            let _ = tx.send((path, r));
            let _ = proxy.send_event(Wake);
        });
        self.loading = Some((rx, progress, Instant::now()));
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    /// Abandonne la lecture en cours : OpenCASCADE s'arrête au plus tôt ; les autres lecteurs finissent seuls,
    /// leur résultat est ignoré.
    fn cancel_open(&mut self) {
        if let Some((_, p, _)) = self.loading.take() {
            p.cancel();
        }
        self.ready = None;
        self.state.loading = None;
    }

    fn finish_open(&mut self, path: PathBuf, m: Model) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut s = AppState::new(&name, &m);
        std::mem::swap(&mut s.settings, &mut self.state.settings);
        s.system_dark = self.state.system_dark;
        s.viewport = self.state.viewport.clone();
        s.inspector = self.state.inspector;
        s.renderer = self.state.renderer.clone();
        s.update = self.state.update.clone();
        s.lighting = std::mem::take(&mut self.state.lighting);
        s.right_tab = self.state.right_tab;
        s.recent = std::mem::take(&mut self.state.recent);
        crate::recent::push(&mut s.recent, &path);
        if let Err(e) = crate::recent::save(&s.recent) {
            eprintln!("Obview 3D : fichiers récents : {e}");
        }
        self.file_key = std::fs::canonicalize(&path).unwrap_or(path).to_string_lossy().into_owned();
        s.views.list = self.views.0.get(&self.file_key).cloned().unwrap_or_default();
        self.state = s;
        self.model = Arc::new(m);
        self.scene_key = None;
        self.edges = None;
        self.framed = false;
    }

    /// Réponses des fils de travail.
    /// Recherche de mise à jour dans un fil ; `manual` : demandée depuis les réglages.
    fn start_update_check(&mut self, manual: bool) {
        if self.updating.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = tx.send(UpdateMsg::Checked(update::check(), manual));
            let _ = proxy.send_event(Wake);
        });
        self.updating = Some(rx);
        self.state.update = update::Status::Checking { manual };
    }

    /// Télécharge et installe la version proposée ; sans fichier installable, ouvre sa page.
    fn start_update_install(&mut self) {
        let Some(release) = self.state.update.release().cloned() else { return };
        let how = update::install_kind();
        let Some(asset) = release.asset.clone().filter(|_| how != update::Install::Manual) else {
            let _ = update::open_path(&release.page);
            return;
        };
        if self.updating.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let proxy = self.proxy.clone();
        let r = release.clone();
        std::thread::spawn(move || {
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut last = Instant::now();
            let mut on_progress = |f: f32| {
                // Au plus dix réveils par seconde : l'image suit sans tourner en continu.
                if last.elapsed().as_millis() >= 100 {
                    last = Instant::now();
                    let _ = tx.send(UpdateMsg::Progress(f));
                    let _ = proxy.send_event(Wake);
                }
            };
            let result = update::download(&asset, &mut on_progress, &cancel).and_then(|file| {
                let _ = tx.send(UpdateMsg::Progress(1.0));
                let _ = proxy.send_event(Wake);
                update::install(&file, &how)
            });
            let _ = tx.send(UpdateMsg::Installed(r, result));
            let _ = proxy.send_event(Wake);
        });
        self.updating = Some(rx);
        self.state.update = update::Status::Downloading(release, 0.0);
    }

    fn poll_update(&mut self) {
        if let Some(action) = self.state.update_request.take() {
            match action {
                update::Action::Check => self.start_update_check(true),
                update::Action::Install => self.start_update_install(),
                update::Action::OpenPage => {
                    if let Some(r) = self.state.update.release() {
                        let _ = update::open_path(&r.page);
                    }
                }
            }
        }
        let Some(rx) = &self.updating else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                UpdateMsg::Checked(r, manual) => {
                    self.updating = None;
                    if r.is_ok() {
                        self.state.settings.last_update_check = unix_now();
                        self.state.settings_dirty = true;
                    }
                    self.state.update = match r {
                        Ok(Some(rel)) => update::Status::Available(rel),
                        Ok(None) => update::Status::Current,
                        Err(error) if manual => update::Status::Failed { release: None, error },
                        // Recherche automatique : un échec (hors ligne) reste silencieux.
                        Err(_) => update::Status::Idle,
                    };
                    return;
                }
                UpdateMsg::Progress(f) => {
                    if let update::Status::Downloading(r, _) = &self.state.update {
                        self.state.update = if f >= 1.0 {
                            update::Status::Installing(r.clone())
                        } else {
                            update::Status::Downloading(r.clone(), f)
                        };
                    }
                }
                UpdateMsg::Installed(rel, r) => {
                    self.updating = None;
                    match r {
                        Ok(true) => {
                            self.state.update = update::Status::Installing(rel);
                            self.quit = true;
                        }
                        Ok(false) => self.state.update = update::Status::Available(rel),
                        Err(error) => self.state.update = update::Status::Failed { release: Some(rel), error },
                    }
                    return;
                }
            }
        }
    }

    /// Éclairage (D-33) : préparation demandée, environnement reçu envoyé au GPU, aperçus reçus.
    fn poll_lighting(&mut self) {
        if let Some(key) = self.state.lighting.request.take() {
            let file = self.state.settings.lighting.file.clone();
            let (tx, rx) = channel();
            let proxy = self.proxy.clone();
            let k = key.clone();
            std::thread::spawn(move || {
                let r = crate::lighting::build(&k, file.as_deref());
                let _ = tx.send((k, r));
                let _ = proxy.send_event(Wake);
            });
            self.env_job = Some(rx);
            self.state.lighting.loading = Some(key);
        }
        if let Some(rx) = &self.env_job
            && let Ok((key, r)) = rx.try_recv()
        {
            self.env_job = None;
            self.state.lighting.loading = None;
            match r {
                Ok(env) => {
                    if let Some(g) = &mut self.gfx
                        && let Err(e) = g.set_environment(&env)
                    {
                        eprintln!("Obview 3D : environnement : {e}");
                    }
                    self.state.lighting.env = Some(Arc::new(env));
                    self.state.lighting.key = key;
                    self.state.lighting.error = None;
                }
                Err(e) => {
                    // HDRI illisible : on revient à l'environnement affiché.
                    self.state.lighting.error = Some(e);
                    if !self.state.lighting.key.is_empty() {
                        self.state.settings.lighting.preset = self.state.lighting.key.clone();
                    }
                }
            }
        }
        if let Some(rx) = &self.thumbs_job
            && let Ok(t) = rx.try_recv()
        {
            self.thumbs_job = None;
            self.state.lighting.thumbs = t;
        }
    }

    fn start_lighting(&mut self) {
        self.state.lighting.request = Some(self.state.settings.lighting.preset.clone());
        let (tx, rx) = channel();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::lighting::thumbnails());
            let _ = proxy.send_event(Wake);
        });
        self.thumbs_job = Some(rx);
    }

    fn poll_jobs(&mut self) {
        self.poll_update();
        self.poll_lighting();
        // Modèle reçu à l'image précédente (carte « Préparation » montrée) : installé maintenant.
        if let Some((_, _, shown)) = &self.ready
            && *shown
            && let Some((path, m, _)) = self.ready.take()
        {
            self.state.loading = None;
            self.finish_open(path, m);
        }
        if let Some((rx, progress, started)) = &self.loading {
            match rx.try_recv() {
                Ok((path, r)) => {
                    if std::env::var_os("OBV_DEBUG_LOAD").is_some() {
                        eprintln!("lecture : terminée à {:.2} s", started.elapsed().as_secs_f64());
                    }
                    self.loading = None;
                    match r {
                        Ok(m) => {
                            if let Some(l) = &mut self.state.loading {
                                l.stage = Stage::Prepare;
                                l.fraction = None;
                            }
                            self.ready = Some((path, m, false));
                        }
                        Err(e) => {
                            self.state.loading = None;
                            eprintln!("Obview 3D : {}", e);
                            self.state.notice = Some(Notice {
                                title: "Impossible d'ouvrir",
                                subject: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                                detail: e,
                            });
                        }
                    }
                }
                Err(_) => {
                    let (stage, fraction) = progress.get();
                    if let Some(l) = &mut self.state.loading {
                        if l.stage != stage && std::env::var_os("OBV_DEBUG_LOAD").is_some() {
                            eprintln!("lecture : {:?} → {stage:?} à {:.2} s", l.stage, started.elapsed().as_secs_f64());
                        }
                        l.stage = stage;
                        l.fraction = fraction;
                        l.elapsed = started.elapsed().as_secs_f64();
                    }
                }
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
                Request::CancelOpen => self.cancel_open(),
                Request::OpenPath(p) => self.open(p),
                Request::LoadHdri => {
                    let mut d =
                        rfd::FileDialog::new().set_title(tr("Charger un HDRI")).add_filter("HDRI", &["hdr", "exr"]);
                    if let Some(w) = &window {
                        d = d.set_parent(w.as_ref());
                    }
                    if let Some(p) = d.pick_file() {
                        crate::lighting::set_file(&mut self.state, p);
                    }
                }
                Request::OpenFile => {
                    let mut d = rfd::FileDialog::new()
                        .set_title(tr("Ouvrir"))
                        .add_filter(tr("Modèles 3D"), &extensions())
                        .add_filter(tr("Tous les fichiers"), &["*"]);
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
                        compression: self.state.convert.compression,
                    };
                    let stem = std::path::Path::new(&self.state.file_name)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tr("modele").into());
                    let ext = opts.format.extension();
                    let mut d = rfd::FileDialog::new()
                        .set_title(tr("Convertir"))
                        .set_file_name(format!("{stem}.{ext}"))
                        .add_filter(opts.format.label(), &[ext]);
                    if let Some(w) = &window {
                        d = d.set_parent(w.as_ref());
                    }
                    let Some(path) = d.save_file() else { continue };
                    self.start_conversion(path, opts);
                }
                Request::OpenResult(p) => self.open(p),
                Request::Capture { size, transparent } => {
                    let stem = std::path::Path::new(&self.state.file_name)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tr("vue").into());
                    let mut d = rfd::FileDialog::new()
                        .set_title(tr("Capture"))
                        .set_file_name(format!("{stem}.png"))
                        .add_filter(tr("Image PNG"), &["png"]);
                    if let Some(w) = &window {
                        d = d.set_parent(w.as_ref());
                    }
                    let Some(path) = d.save_file() else { continue };
                    if let Err(e) = self.save_capture(&path, size, transparent) {
                        eprintln!("Obview 3D : capture : {e}");
                        self.state.notice = Some(Notice {
                            title: "Capture impossible",
                            subject: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            detail: e.to_string(),
                        });
                    }
                }
                Request::Reveal(p) => reveal(&p),
            }
        }
    }

    /// Rend la vue sans l'interface et l'enregistre en PNG.
    fn save_capture(&mut self, path: &std::path::Path, size: CaptureSize, transparent: bool) -> Result<(), String> {
        let Some(gfx) = &self.gfx else { return Err("pas de fenêtre".into()) };
        let v = self.state.viewport.get();
        let scale = gfx.window.scale_factor() as f32;
        let (vw, vh) = ((v.width() * scale).round() as u32, (v.height() * scale).round() as u32);
        let (w, h) = match size {
            CaptureSize::Uhd => (3840, (3840.0 * vh as f32 / vw.max(1) as f32).round() as u32),
            _ => size.pixels(vw, vh),
        };
        let accent = to_srgb8(shell::selection_color(&self.ui.theme));
        let rgba = gfx.capture(&self.model, &self.state, &self.ui.theme, accent, w, h, transparent)?;
        write_png(path, w, h, &rgba).map_err(|e| e.to_string())
    }

    /// Point de mesure sous le pointeur, avec accroche aux sommets (10 px) puis aux arêtes (8 px).
    fn measure_point(gfx: &Gfx, model: &Model, state: &AppState, scale: f32, p: Vec2) -> Option<(MeasurePoint, Snap)> {
        let view = state.viewport.get();
        let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
        let hit = gfx.pick(&state.camera, [view.width() * scale, view.height() * scale], px)?;
        let part = hit.id as usize;
        let offsets = tools::explode_offsets(state);
        let o = offsets.get(part).copied().unwrap_or([0.0; 3]);
        let local = [hit.position[0] - o[0] as f64, hit.position[1] - o[1] as f64, hit.position[2] - o[2] as f64];
        let mesh = &model.parts.get(part)?.mesh;
        let t = hit.triangle as usize * 3;
        let tri: Vec<[f64; 3]> =
            mesh.indices.get(t..t + 3)?.iter().map(|&i| mesh.positions[i as usize].map(|v| v as f64)).collect();
        let cam = &state.camera;
        let screen =
            |q: [f64; 3]| tools::to_screen(cam, view, [q[0] + o[0] as f64, q[1] + o[1] as f64, q[2] + o[2] as f64]);
        let dist = |a: Vec2| ((a.x - p.x).powi(2) + (a.y - p.y).powi(2)).sqrt();
        // Sommet le plus proche à l'écran.
        let best = tri.iter().filter_map(|q| screen(*q).map(|s| (dist(s), *q))).min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((d, q)) = best
            && d <= 10.0
        {
            return Some((MeasurePoint { part, local: q }, Snap::Vertex));
        }
        // Point le plus proche sur les arêtes du triangle (en 3D, le long de l'arête).
        let mut edge: Option<(f32, [f64; 3])> = None;
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let ab: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let ap: [f64; 3] = std::array::from_fn(|i| local[i] - a[i]);
            let len2 = ab.iter().map(|v| v * v).sum::<f64>().max(1e-300);
            let u = (ab.iter().zip(&ap).map(|(x, y)| x * y).sum::<f64>() / len2).clamp(0.0, 1.0);
            let q: [f64; 3] = std::array::from_fn(|i| a[i] + ab[i] * u);
            if let Some(s) = screen(q) {
                let d = dist(s);
                if edge.is_none_or(|(e, _)| d < e) {
                    edge = Some((d, q));
                }
            }
        }
        if let Some((d, q)) = edge
            && d <= 8.0
        {
            return Some((MeasurePoint { part, local: q }, Snap::Edge));
        }
        Some((MeasurePoint { part, local }, Snap::Surface))
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
            .with_theme(window_theme(&self.state.settings))
            .with_inner_size(LogicalSize::new(1600.0, 900.0))
            .with_min_inner_size(LogicalSize::new(960.0, 600.0))
            .with_window_icon(window_icon());
        self.create_gfx(el, with_app_id(attrs))
    }

    /// Moteur choisi : réglage, sauf `--gl` / `--vulkan` ou `OBV_RENDERER` (essais).
    fn renderer_choice(&self) -> RendererChoice {
        let args: Vec<String> = std::env::args().collect();
        let env = std::env::var("OBV_RENDERER").unwrap_or_default().to_ascii_lowercase();
        if args.iter().any(|a| a == "--gl") || env == "gl" || env == "opengl" {
            RendererChoice::OpenGl
        } else if args.iter().any(|a| a == "--vulkan") || env == "vulkan" {
            RendererChoice::Vulkan
        } else {
            self.state.settings.renderer
        }
    }

    fn create_gfx(&mut self, el: &ActiveEventLoop, attrs: winit::window::WindowAttributes) -> Result<(), String> {
        let validation = std::env::args().any(|a| a == "--validation");
        let choice = self.renderer_choice();
        let (gfx, failed) = Gfx::create(el, attrs, choice, validation)?;
        if let Some(e) = failed {
            self.state.notice = Some(Notice {
                title: if choice == RendererChoice::OpenGl { "OpenGL indisponible" } else { "Vulkan indisponible" },
                subject: gfx.description.clone(),
                detail: e,
            });
        }
        self.window_theme = window_theme(&self.state.settings);
        if self.window_theme.is_none() {
            self.state.system_dark = gfx.window.theme().is_none_or(|t| t == WinTheme::Dark);
        }
        self.state.renderer = (gfx.backend, gfx.description.clone());
        self.applied_renderer = self.state.settings.renderer;
        let mut gfx = gfx;
        if let Some(env) = &self.state.lighting.env
            && let Err(e) = gfx.set_environment(env)
        {
            eprintln!("Obview 3D : environnement : {e}");
        }
        self.gfx = Some(gfx);
        // Nouveau moteur : atlas et géométrie à renvoyer.
        let size = self.ui.atlas.size;
        self.ui.atlas.dirty = Some([0, 0, size, size]);
        self.scene_key = None;
        Ok(())
    }

    /// Change de moteur sans redémarrer : la fenêtre est recréée à la même place et à la même taille.
    fn switch_renderer(&mut self, el: &ActiveEventLoop) {
        let Some(old) = self.gfx.take() else { return };
        let w = &old.window;
        let size = w.inner_size();
        let mut attrs = Window::default_attributes()
            .with_title("Obview 3D")
            .with_theme(window_theme(&self.state.settings))
            .with_inner_size(size)
            .with_min_inner_size(LogicalSize::new(960.0, 600.0))
            .with_maximized(w.is_maximized())
            .with_window_icon(window_icon());
        if let Ok(pos) = w.outer_position() {
            attrs = attrs.with_position(pos);
        }
        drop(old);
        if let Err(e) = self.create_gfx(el, with_app_id(attrs)) {
            eprintln!("Obview 3D : {e}");
            el.exit();
            return;
        }
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
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
        let burst = std::mem::take(&mut self.pending.left_presses);
        let (input, again) = Self::frame_input(&mut self.pending, self.start, size, scale);
        let now = input.time;
        let prev_buttons = self.ui.input.buttons;
        self.ui.begin_frame(input.clone());
        // Appuis gauches cumulés pendant l'appui en cours, gardés jusqu'à l'image du relâchement (clic lu par
        // l'interface) : deux appuis dans une image lente forment un double-clic.
        let want = window_theme(&self.state.settings);
        if want != self.window_theme {
            gfx.window.set_theme(want);
            self.window_theme = want;
            if want.is_none() {
                self.state.system_dark = gfx.window.theme().is_none_or(|t| t == WinTheme::Dark);
            }
        }
        self.state.left_presses = if input.buttons[0] && !prev_buttons[0] {
            burst
        } else if input.buttons[0] || prev_buttons[0] {
            self.state.left_presses.saturating_add(burst)
        } else {
            burst
        };
        shell::handle(&mut self.ui, &mut self.state);

        // Navigation et sélection dans la vue 3D.
        let view = self.state.viewport.get();
        let over_ui = self.ui.pointer_over_ui() || self.ui.is_active_any();
        let g: &Gfx = gfx;
        let cam_now = self.state.camera;
        let mut pick = |p: Vec2| -> Option<[f64; 3]> {
            let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
            g.pick(&cam_now, [view.width() * scale, view.height() * scale], px).map(|h| h.position)
        };
        let ctx = NavContext { input: &input, view, over_ui, settings: &self.state.settings };
        let moved = self.nav.update(&ctx, &mut self.state.camera, &mut pick);
        if self.nav.dome_delta != 0.0 {
            let l = &mut self.state.settings.lighting;
            l.rotation = (l.rotation + self.nav.dome_delta).rem_euclid(360.0);
            self.state.settings_dirty = true;
        }
        let left_pressed = input.buttons[0] && !prev_buttons[0];
        let left_released = !input.buttons[0] && prev_buttons[0];
        // Un clic gauche sans déplacement sélectionne, même quand le glisser gauche navigue (schéma Obview).
        if left_pressed
            && !over_ui
            && let Some(p) = input.pointer
            && view.contains(p)
        {
            self.click_origin = Some(p);
            self.press_burst = burst;
        }
        // Appuis arrivés pendant que le bouton paraît tenu (images lentes : deux clics à cheval sur deux images).
        if !left_pressed && input.buttons[0] && burst > 0 && self.click_origin.is_some() {
            self.press_burst = self.press_burst.saturating_add(burst);
        }
        if left_released && let (Some(o), Some(p)) = (self.click_origin.take(), input.pointer) {
            let d = p - o;
            if d.x.abs() + d.y.abs() < 4.0 && self.state.inspector == Inspector::Bands {
                if let Some(m) = inspector::band_at(view, &self.state, p) {
                    self.state.inspector = Inspector::Single(m);
                }
            } else if d.x.abs() + d.y.abs() < 4.0 && self.state.tool == Tool::Measure {
                // Mesure : premier point, puis second point qui termine la cote.
                if let Some((q, _)) = Self::measure_point(gfx, &self.model, &self.state, scale, p) {
                    match self.state.measure.pending.take() {
                        Some(a) => self.state.measure.done.push([a, q]),
                        None => self.state.measure.pending = Some(q),
                    }
                }
            } else if d.x.abs() + d.y.abs() < 4.0 {
                let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
                let hit = gfx.pick(&self.state.camera, [view.width() * scale, view.height() * scale], px);
                let part = hit.map(|h| h.id as usize);
                // Double-clic sur une pièce : sélection, liste des pièces ouverte et défilée jusqu'à elle.
                let double = part.is_some()
                    && (self.press_burst >= 2
                        || (self.last_click.is_some_and(|(t, q)| now - t < 0.45 && q == part)
                            && self.state.selected == part));
                self.state.selected = part;
                if double && let Some(i) = part {
                    let s = &mut self.state;
                    s.left_open = true;
                    s.root_open = true;
                    if !s.filtered().contains(&i) {
                        s.search.clear();
                    }
                    s.reveal = Some(i);
                    self.last_click = None;
                } else {
                    self.last_click = Some((now, part));
                }
            }
        }
        // Focus : la pièce sélectionnée, sinon tout le modèle (bouton ou F).
        if self.ui.clicked(Id::new("cadrer"))
            || self.ui.clicked(Id::new("cadrer pièce"))
            || (!self.ui.text_focused() && input.keys.contains(&Key::Char('f')) && !input.modifiers.ctrl)
        {
            focus(&mut self.nav, &self.state, &self.model, view, now);
        }
        // Gizmo de navigation : clic sur un axe, une face, une arête ou un coin ; glisser pour tourner.
        let cube = Id::new("gizmo");
        let style = self.state.settings.gizmo;
        if let Some(r) = self.ui.rect_of(cube) {
            let cam = self.state.camera;
            if self.ui.clicked(cube)
                && let Some(p) = input.pointer
                && !std::mem::take(&mut self.cube_dragged)
                && let Some(dir) = crate::gizmo::pick(style, r, cam.yaw as f32, cam.pitch as f32, p)
            {
                let (yaw, pitch) = crate::gizmo::angles(dir, cam.yaw);
                self.nav.animate_to(&cam, Camera { yaw, pitch, ..cam }, now);
            } else if self.ui.clicked(cube) {
                self.cube_dragged = false;
            } else if self.ui.dragging(cube).is_some() && !self.ui.press_started(cube) {
                let d = self.ui.pointer_delta();
                if d != Vec2::default() {
                    crate::navigation::orbit(
                        &mut self.state.camera,
                        cam.pivot,
                        -d.x as f64 * 0.0075,
                        d.y as f64 * 0.0075,
                    );
                    self.cube_dragged = true;
                }
            }
        }
        // Mesure : point survolé (accroche), recalculé quand le pointeur bouge.
        if self.state.tool == Tool::Measure && !over_ui && !self.nav.dragging() {
            if input.pointer != self.measure_at {
                self.measure_at = input.pointer;
                self.state.measure.hover = input
                    .pointer
                    .filter(|p| view.contains(*p))
                    .and_then(|p| Self::measure_point(gfx, &self.model, &self.state, scale, p));
            }
        } else {
            self.state.measure.hover = None;
            self.measure_at = None;
        }
        if let Some(to) = self.state.camera_request.take() {
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
            if let Err(e) = gfx.set_meshes(&meshes) {
                self.error = Some(e);
            }
            self.scene_key = Some(key);
        }

        if let Err(e) = gfx.present(&mut self.ui, &self.state, scale) {
            self.error = Some(e);
        }
        // La carte « Préparation » est à l'écran : le modèle sera installé à l'image suivante.
        if let Some(r) = &mut self.ready
            && !r.2
        {
            r.2 = true;
            gfx.window.request_redraw();
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

        if std::mem::take(&mut self.state.views.dirty) {
            self.views.0.insert(self.file_key.clone(), self.state.views.list.clone());
            if let Err(e) = self.views.save() {
                eprintln!("Obview 3D : vues non enregistrées : {e}");
            }
        }
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

/// Cadre la pièce sélectionnée (à sa place dans la vue éclatée), sinon tout le modèle. Les bornes de profondeur
/// de la projection orthographique restent celles de la scène entière.
fn focus(nav: &mut Navigator, state: &AppState, model: &Model, view: obv_ui::Rect, now: f64) {
    let cam = &state.camera;
    let bounds = match state.selected.and_then(|i| model.parts.get(i).map(|p| (i, p))) {
        Some((i, p)) => p.mesh.bounds().map(|(lo, hi)| {
            let o = tools::explode_offsets(state).get(i).copied().unwrap_or([0.0; 3]);
            (std::array::from_fn(|k| lo[k] + o[k]), std::array::from_fn(|k| hi[k] + o[k]))
        }),
        None => model.bounds(),
    };
    if let Some((lo, hi)) = bounds {
        let mut to = *cam;
        to.frame(lo, hi, (view.width() / view.height().max(1.0)) as f64);
        to.radius = cam.radius.max(to.radius);
        nav.animate_to(cam, to, now);
    }
}

/// Barre de titre de la fenêtre au thème de l'interface (sombre par défaut, D-32) ; « Système » : celle du
/// système, qui suit ses changements.
fn window_theme(s: &Settings) -> Option<WinTheme> {
    match s.theme {
        crate::settings::ThemeChoice::Dark => Some(WinTheme::Dark),
        crate::settings::ThemeChoice::Light => Some(WinTheme::Light),
        crate::settings::ThemeChoice::System => None,
    }
}

/// Linux : classe de fenêtre X11 et identifiant Wayland `obview-3d`, ceux du fichier .desktop des paquets
/// (icône et regroupement dans la barre des tâches, D-32).
fn with_app_id(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    #[cfg(target_os = "linux")]
    {
        let attrs = winit::platform::x11::WindowAttributesExtX11::with_name(attrs, "obview-3d", "obview-3d");
        winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, "obview-3d", "obview-3d")
    }
    #[cfg(not(target_os = "linux"))]
    attrs
}

/// Secondes depuis 1970.
fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
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
            self.start_lighting();
            // Recherche automatique : au plus une fois par jour (D-32).
            let s = &self.state.settings;
            if s.check_updates && unix_now().saturating_sub(s.last_update_check) >= 20 * 3600 {
                self.start_update_check(false);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = &mut self.gfx else { return };
        let scale = gfx.window.scale_factor() as f32;
        let mut redraw = true;
        match event {
            WindowEvent::CloseRequested => {
                if gfx.validation_errors() > 0 {
                    eprintln!("Obview 3D : {} erreurs de validation Vulkan", gfx.validation_errors());
                }
                if self.state.settings_dirty {
                    let _ = self.state.settings.save();
                }
                el.exit();
                return;
            }
            WindowEvent::Resized(s) => gfx.resize(s.width, s.height),
            WindowEvent::ScaleFactorChanged { .. } => {
                let s = gfx.window.inner_size();
                gfx.resize(s.width, s.height);
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
                    if i == 0 {
                        self.pending.left_presses = self.pending.left_presses.saturating_add(1);
                    }
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
            WindowEvent::DroppedFile(path) => {
                self.state.drop_hover = false;
                self.open(path);
            }
            WindowEvent::HoveredFile(_) => self.state.drop_hover = true,
            WindowEvent::HoveredFileCancelled => self.state.drop_hover = false,
            WindowEvent::RedrawRequested => {
                self.redraw();
                // Mise à jour installée ou lancée : réglages enregistrés, application fermée (relancée ensuite).
                if self.quit {
                    let _ = self.state.settings.save();
                    let _ = self.views.save();
                    el.exit();
                    return;
                }
                // Moteur changé dans les réglages : bascule après l'image.
                if self.state.settings.renderer != self.applied_renderer {
                    self.switch_renderer(el);
                    return;
                }
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

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), png::EncodingError> {
    let f = std::fs::File::create(path)?;
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()?.write_image_data(rgba)
}

/// Icône de la fenêtre (logo 256 px intégré à l'exécutable).
fn window_icon() -> Option<winit::window::Icon> {
    let bytes: &[u8] = include_bytes!("../assets/obview-256.png");
    let mut r = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    winit::window::Icon::from_rgba(buf, info.width, info.height).ok()
}
