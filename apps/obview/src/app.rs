//! Fenêtre de l'application : boucle d'événements, entrées, navigation, sélection au clic, présentation.
//! Une image n'est rendue que lorsqu'une entrée, une animation ou un chargement l'exige.

use std::{
    collections::{HashSet, VecDeque},
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

use crate::materials::Prepared;
use crate::{
    convert_ui::{Request, Status},
    frame::{self, EdgeCache, PaneView, SceneKey, mesh_inputs},
    gfx::Gfx,
    inspector::{self, Inspector},
    loading::{Loading, Notice},
    multiview::{self, Multi, MultiRequest, Pane, Preset, Split, Zone},
    navigation::{NavContext, Navigator, WHEEL_NOTCH_PX},
    settings::{RendererChoice, Settings},
    shell::{self, AppState, Tool},
    tabs::{DocState, HOME, TabInfo, TabRequest},
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

type Loaded = Result<(Model, Prepared), String>;

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
    /// Document de l'onglet actif (D-41) ; ceux des autres onglets sont rangés dans `tabs`.
    doc: DocData,
    /// Onglets de fichiers, dans l'ordre de la barre.
    tabs: Vec<Tab>,
    next_tab: u64,
    /// Fichiers des onglets fermés (Ctrl Maj T), le dernier en fin.
    closed: Vec<PathBuf>,
    /// Lectures en attente (une à la fois : OpenCASCADE et la mémoire ne se partagent pas bien).
    queue: VecDeque<u64>,
    proxy: EventLoopProxy<Wake>,
    /// Lecture en cours : onglet, réponse du fil, avancement partagé, début.
    loading: Option<(u64, Receiver<Loaded>, Arc<Progress>, Instant)>,
    /// Modèle lu, installé à l'image suivante : la carte d'ouverture montre d'abord « Préparation » (l'envoi au
    /// GPU se fait sur le fil de la fenêtre).
    ready: Option<(u64, PathBuf, Model, Prepared, bool)>,
    converting: Option<(Receiver<Converted>, Arc<AtomicU32>)>,
    pending: Pending,
    start: Instant,
    last_save: f64,
    error: Option<String>,
    /// Le cube de vue a été glissé depuis l'appui : le relâchement n'est pas un clic.
    cube_dragged: bool,
    /// Vues enregistrées de tous les fichiers.
    views: ViewStore,
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
    /// Onglets dont la vignette reste à rendre (D-41), vignettes lues sur le disque au lancement.
    thumb_queue: Vec<u64>,
    thumbs_disk: Option<Receiver<Vec<crate::thumbs::Stored>>>,
}

/// Données d'un document propres à la fenêtre : modèle, matériaux, état des envois au GPU, navigation.
struct DocData {
    model: Arc<Model>,
    /// Matériaux du modèle (D-37), envoyés au moteur quand `materials_sent` est faux.
    materials: Prepared,
    materials_sent: bool,
    scene_key: Option<SceneKey>,
    edges: Option<EdgeCache>,
    framed: bool,
    /// Clé du fichier dans les vues enregistrées (chemin canonique ; vide sans fichier).
    file_key: String,
    nav: Navigator,
    /// Appui gauche dans la vue 3D : position, pour distinguer un clic d'un glisser.
    click_origin: Option<Vec2>,
    /// Dernier pointeur pour lequel le point de mesure survolé a été calculé.
    measure_at: Option<Vec2>,
    /// Dernier clic dans la vue : instant et pièce touchée (double-clic).
    last_click: Option<(f64, Option<usize>)>,
}

impl DocData {
    fn new(model: Model, materials: Prepared, file_key: String) -> Self {
        DocData {
            model: Arc::new(model),
            materials,
            materials_sent: false,
            scene_key: None,
            edges: None,
            framed: false,
            file_key,
            nav: Navigator::default(),
            click_origin: None,
            measure_at: None,
            last_click: None,
        }
    }

    fn empty() -> Self {
        Self::new(Model::default(), Prepared::default(), String::new())
    }
}

/// Onglet d'un fichier (D-41). Son identifiant est aussi celui de ses données sur le GPU.
struct Tab {
    id: u64,
    path: PathBuf,
    /// Chemin canonique : un fichier déjà ouvert n'ouvre pas de second onglet.
    key: PathBuf,
    /// Date de modification du fichier lu : plus récente à la réouverture, le fichier est relu.
    modified: Option<std::time::SystemTime>,
    /// Un modèle a été installé (sinon, un échec de lecture ferme l'onglet).
    loaded: bool,
    /// État et données du document quand l'onglet n'est pas l'actif.
    stash: Option<Box<(DocState, DocData)>>,
}

fn modified(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
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

/// Envoie au GPU ce qui a changé du document actif : matériaux (ouverture, changement de moteur), géométrie
/// quand la visibilité ou les arêtes changent (sélection et accent passent par les paramètres de l'image, D-37).
fn sync_gpu(gfx: &mut Gfx, doc: &mut DocData, state: &AppState) -> Result<(), String> {
    let mut r = Ok(());
    if !doc.materials_sent {
        r = gfx.set_materials(&doc.materials);
        doc.materials_sent = true;
    }
    if state.inspector != Inspector::Off && doc.edges.is_none() {
        doc.edges = Some(EdgeCache::compute(&doc.model));
    }
    let key = SceneKey::new(state, doc.edges.is_some());
    if doc.scene_key.as_ref() != Some(&key) {
        let meshes = mesh_inputs(&doc.model, state, doc.edges.as_ref());
        r = r.and(gfx.set_meshes(&meshes));
        doc.scene_key = Some(key);
    }
    r
}

/// Enregistre les vues du document `doc` si elles ont changé.
fn save_views(store: &mut ViewStore, state: &mut AppState, doc: &DocData) {
    if std::mem::take(&mut state.views.dirty) && !doc.file_key.is_empty() {
        store.0.insert(doc.file_key.clone(), state.views.list.clone());
        if let Err(e) = store.save() {
            eprintln!("Obview 3D : vues non enregistrées : {e}");
        }
    }
}

/// Rapport largeur / hauteur d'un rectangle.
fn aspect_of(r: obv_ui::Rect) -> f64 {
    (r.width() / r.height().max(1.0)) as f64
}

/// Nom du fichier d'un chemin.
fn file_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
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
    pub fn new(paths: Vec<PathBuf>, proxy: EventLoopProxy<Wake>) -> Self {
        // Démarrage sur l'accueil (D-41) : fichiers récents.
        let mut state = AppState::new("", &Model::default());
        state.settings = Settings::load();
        state.recent = crate::recent::load();
        let ui = Ui::new(shell::theme(&state));
        let mut app = App {
            gfx: None,
            ui,
            state,
            doc: DocData::empty(),
            tabs: Vec::new(),
            next_tab: 1,
            closed: Vec::new(),
            queue: VecDeque::new(),
            proxy,
            loading: None,
            ready: None,
            converting: None,
            pending: Pending::default(),
            start: Instant::now(),
            last_save: 0.0,
            error: None,
            cube_dragged: false,
            views: ViewStore::load(),
            press_burst: 0,
            updating: None,
            quit: false,
            window_theme: None,
            env_job: None,
            thumbs_job: None,
            applied_renderer: RendererChoice::Auto,
            thumb_queue: Vec::new(),
            thumbs_disk: None,
        };
        // Vignettes des fichiers récents, lues hors du fil de la fenêtre.
        let keys: Vec<PathBuf> = app.state.recent.iter().map(|r| r.path.clone()).collect();
        let (tx, rx) = channel();
        let proxy = app.proxy.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::thumbs::load(&keys));
            let _ = proxy.send_event(Wake);
        });
        app.thumbs_disk = Some(rx);
        for p in paths {
            app.open(p);
        }
        if let Some(first) = app.tabs.first().map(|t| t.id) {
            app.activate(first);
        }
        app
    }

    /// Ouvre `path` dans un nouvel onglet, lu dans un fil séparé (une lecture à la fois, les autres attendent).
    /// Un fichier déjà ouvert active son onglet ; modifié depuis sa lecture, il est relu dans cet onglet.
    fn open(&mut self, path: PathBuf) {
        self.state.notice = None;
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if let Some(t) = self.tabs.iter().find(|t| t.key == key) {
            let (id, stale) = (t.id, t.loaded && modified(&key) != t.modified);
            self.activate(id);
            if stale && !self.queue.contains(&id) && self.loading.as_ref().is_none_or(|l| l.0 != id) {
                let name = file_name(&path);
                self.state.loading = Some(Loading::new(name));
                self.queue.push_back(id);
                self.start_next_load();
            }
            return;
        }
        let id = self.next_tab;
        self.next_tab += 1;
        let name = file_name(&path);
        let mut st = DocState::new(&name, &Model::default());
        st.loading = Some(Loading::new(name));
        self.tabs.push(Tab {
            id,
            path,
            key,
            modified: None,
            loaded: false,
            stash: Some(Box::new((st, DocData::empty()))),
        });
        self.queue.push_back(id);
        self.activate(id);
        self.start_next_load();
    }

    /// Lance la lecture suivante de la file s'il n'y en a pas en cours.
    fn start_next_load(&mut self) {
        if self.loading.is_some() || self.ready.is_some() {
            return;
        }
        while let Some(id) = self.queue.pop_front() {
            let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) else { continue };
            let path = t.path.clone();
            t.modified = modified(&t.key);
            let (tx, rx) = channel();
            let proxy = self.proxy.clone();
            let progress = Arc::new(Progress::default());
            let p = progress.clone();
            std::thread::spawn(move || {
                // Textures décodées ici, hors du fil de la fenêtre.
                let r = load_with(&path, &p).map(|m| {
                    let prep = crate::materials::prepare(&m);
                    (m, prep)
                });
                let _ = tx.send(r);
                let _ = proxy.send_event(Wake);
            });
            self.loading = Some((id, rx, progress, Instant::now()));
            break;
        }
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    /// Lecture (en cours ou en attente) de l'onglet `id`, qu'il soit actif ou non.
    fn tab_loading(&mut self, id: u64) -> Option<&mut Option<Loading>> {
        if id == self.state.active_tab {
            return Some(&mut self.state.loading);
        }
        self.tabs.iter_mut().find(|t| t.id == id)?.stash.as_mut().map(|b| &mut b.0.loading)
    }

    /// Abandonne la lecture de l'onglet actif : OpenCASCADE s'arrête au plus tôt ; les autres lecteurs finissent
    /// seuls, leur résultat est ignoré. Un onglet encore vide se ferme.
    fn cancel_open(&mut self) {
        let id = self.state.active_tab;
        self.stop_load(id);
        if let Some(l) = self.tab_loading(id) {
            *l = None;
        }
        if self.tabs.iter().any(|t| t.id == id && !t.loaded) {
            self.close_tab(id);
        }
        self.start_next_load();
    }

    /// Retire l'onglet `id` de la file et arrête sa lecture.
    fn stop_load(&mut self, id: u64) {
        self.queue.retain(|&q| q != id);
        if self.loading.as_ref().is_some_and(|l| l.0 == id)
            && let Some((_, _, p, _)) = self.loading.take()
        {
            p.cancel();
        }
        if self.ready.as_ref().is_some_and(|r| r.0 == id) {
            self.ready = None;
        }
    }

    /// Installe le modèle lu dans son onglet (actif ou non).
    fn finish_open(&mut self, id: u64, path: PathBuf, m: Model, materials: Prepared) {
        let Some(ti) = self.tabs.iter().position(|t| t.id == id) else { return };
        crate::recent::push(&mut self.state.recent, &path);
        if let Err(e) = crate::recent::save(&self.state.recent) {
            eprintln!("Obview 3D : fichiers récents : {e}");
        }
        let key = self.tabs[ti].key.to_string_lossy().into_owned();
        let mut st = DocState::new(&file_name(&path), &m);
        st.views.list = self.views.0.get(&key).cloned().unwrap_or_default();
        let data = DocData::new(m, materials, key);
        self.tabs[ti].loaded = true;
        if !self.thumb_queue.contains(&id) {
            self.thumb_queue.push(id);
        }
        if id == self.state.active_tab {
            st.swap(&mut self.state);
            self.doc = data;
        } else {
            self.tabs[ti].stash = Some(Box::new((st, data)));
        }
    }

    /// Range l'onglet actif et installe l'onglet `id` (`tabs::HOME` : accueil).
    fn activate(&mut self, id: u64) {
        let current = self.state.active_tab;
        if id == current || (id != HOME && !self.tabs.iter().any(|t| t.id == id)) {
            return;
        }
        self.save_views();
        let mut st = DocState::new("", &Model::default());
        st.swap(&mut self.state);
        let data = std::mem::replace(&mut self.doc, DocData::empty());
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == current) {
            t.stash = Some(Box::new((st, data)));
        }
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id)
            && let Some(b) = t.stash.take()
        {
            let (mut st, data) = *b;
            st.swap(&mut self.state);
            self.doc = data;
        }
        self.state.active_tab = id;
        self.cube_dragged = false;
        self.select_gpu_doc();
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    /// Données GPU de l'onglet actif : choisies, ou recréées (nouveau moteur) avec tout à renvoyer.
    fn select_gpu_doc(&mut self) {
        let id = self.state.active_tab;
        if let Some(g) = &mut self.gfx
            && !g.select_doc(id)
        {
            if let Err(e) = g.open_doc(id) {
                eprintln!("Obview 3D : {e}");
            }
            self.doc.scene_key = None;
            self.doc.materials_sent = false;
        }
    }

    /// Ferme l'onglet `id` : l'onglet voisin (à droite, sinon à gauche, sinon l'accueil) devient l'actif.
    fn close_tab(&mut self, id: u64) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return };
        self.stop_load(id);
        self.thumb_queue.retain(|&q| q != id);
        // Multivue : ses vues de l'onglet disparaissent ; seule, la dernière vue redevient la vue unique.
        if self.state.multi.as_ref().is_some_and(|m| m.panes.iter().any(|p| p.tab == id)) {
            let shown = multiview::visible(&self.state);
            self.leave_pane();
            if let Some(m) = &mut self.state.multi {
                for p in m.panes.clone().iter().filter(|p| p.tab == id) {
                    m.remove(p.id);
                }
            }
            let left = self.state.multi.as_ref().map(|m| m.panes.len()).unwrap_or(0);
            if left == 0 {
                self.state.multi = None;
            } else if shown {
                self.enter_multi_active();
                if left < 2 {
                    self.dissolve();
                }
            } else if left < 2 {
                self.state.multi = None;
            }
        }
        if id == self.state.active_tab {
            let next = self.tabs.get(i + 1).or_else(|| i.checked_sub(1).and_then(|k| self.tabs.get(k)));
            let next = next.map(|t| t.id).unwrap_or(HOME);
            self.activate(next);
        }
        let t = self.tabs.remove(i);
        if t.loaded {
            self.closed.retain(|p| *p != t.path);
            self.closed.push(t.path);
        }
        if let Some(g) = &mut self.gfx {
            g.close_doc(id);
        }
        self.select_gpu_doc();
        self.start_next_load();
    }

    /// Demandes de la barre d'onglets, au début de l'image (l'image entière montre le nouvel onglet).
    fn handle_tab_requests(&mut self) {
        for r in std::mem::take(&mut self.state.tab_requests) {
            match r {
                TabRequest::Select(id) => self.select_tab(id),
                TabRequest::Close(id) => self.close_tab(id),
                TabRequest::Move(id, to) => {
                    if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
                        let t = self.tabs.remove(i);
                        self.tabs.insert(to.min(self.tabs.len()), t);
                    }
                }
                TabRequest::Cycle(d) => {
                    let order: Vec<u64> = std::iter::once(HOME).chain(self.tabs.iter().map(|t| t.id)).collect();
                    let i = order.iter().position(|&o| o == self.state.active_tab).unwrap_or(0) as i64;
                    let n = order.len() as i64;
                    self.select_tab(order[(i + d as i64).rem_euclid(n) as usize]);
                }
                TabRequest::Nth(k) => {
                    let t = if k >= 8 { self.tabs.last() } else { self.tabs.get(k) };
                    if let Some(id) = t.map(|t| t.id) {
                        self.select_tab(id);
                    }
                }
                TabRequest::Reopen => {
                    if let Some(p) = self.closed.pop() {
                        self.open(p);
                    }
                }
            }
        }
    }

    /// Range la vignette du fichier `key` dans les photos de l'interface ; sans couche libre, les vignettes de
    /// fichiers qui ne sont plus ni récents ni ouverts sont libérées.
    fn set_thumb(&mut self, key: PathBuf, rgba: &[u8]) {
        if let Some(&id) = self.state.thumbs.get(&key) {
            self.ui.photos.replace(id, rgba);
            return;
        }
        let mut photo = self.ui.photos.insert(rgba);
        if photo.is_none() {
            let keep: HashSet<PathBuf> = self
                .state
                .recent
                .iter()
                .map(|r| r.path.clone())
                .chain(self.tabs.iter().map(|t| t.key.clone()))
                .collect();
            let photos = &mut self.ui.photos;
            self.state.thumbs.retain(|k, id| {
                let k2 = keep.contains(k);
                if !k2 {
                    photos.remove(*id);
                }
                k2
            });
            photo = self.ui.photos.insert(rgba);
        }
        if let Some(id) = photo {
            self.state.thumbs.insert(key, id);
        }
    }

    /// Rend la vignette d'un onglet en attente (une par image) : modèle cadré en 16:10 sous l'éclairage
    /// courant, fond transparent. Un onglet en arrière-plan est installé le temps du rendu ; ses données GPU
    /// restent chargées, prêtes pour son affichage.
    fn make_thumbnails(&mut self) {
        if self.gfx.is_none() || self.state.lighting.env.is_none() || self.thumb_queue.is_empty() {
            return;
        }
        let id = self.thumb_queue.remove(0);
        let Some(key) = self.tabs.iter().find(|t| t.id == id && t.loaded).map(|t| t.key.clone()) else { return };
        let shot = self.peek(id, |app| {
            let gfx = app.gfx.as_mut()?;
            if let Err(e) = sync_gpu(gfx, &mut app.doc, &app.state) {
                eprintln!("Obview 3D : {e}");
            }
            let (w, h) = (obv_ui::photos::PHOTO_W, obv_ui::photos::PHOTO_H);
            let (camera, viewport) = (app.state.camera, app.state.viewport.get());
            app.state.viewport.set(obv_ui::Rect::from_xywh(0.0, 0.0, w as f32, h as f32));
            let mut cam = Camera::default();
            if let Some((lo, hi)) = app.doc.model.bounds() {
                cam.frame(lo, hi, w as f64 / h as f64);
                // Cadrage serré : la vignette est petite, la marge de la vue y paraîtrait vide.
                cam.distance *= 0.86;
            }
            app.state.camera = cam;
            let shot = gfx.capture(&app.state, &app.ui.theme, w, h, true);
            app.state.camera = camera;
            app.state.viewport.set(viewport);
            Some(shot)
        });
        match shot.flatten() {
            Some(Ok(rgba)) => {
                self.set_thumb(key.clone(), &rgba);
                crate::thumbs::save(&key, rgba);
            }
            Some(Err(e)) => eprintln!("Obview 3D : vignette : {e}"),
            None => {}
        }
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    /// Exécute `f` avec l'onglet `id` installé (son état dans `state`, ses données dans `doc`, son document
    /// choisi sur le GPU), puis remet l'onglet actif. `None` : onglet inconnu.
    fn peek<R>(&mut self, id: u64, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        if id == self.state.active_tab {
            return Some(f(self));
        }
        let i = self.tabs.iter().position(|t| t.id == id)?;
        let mut b = self.tabs[i].stash.take()?;
        let active = self.state.active_tab;
        b.0.swap(&mut self.state);
        std::mem::swap(&mut b.1, &mut self.doc);
        self.state.active_tab = id;
        self.select_gpu_doc();
        let r = f(self);
        self.state.active_tab = active;
        b.0.swap(&mut self.state);
        std::mem::swap(&mut b.1, &mut self.doc);
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.stash = Some(b);
        }
        if let Some(g) = &self.gfx {
            g.select_doc(active);
        }
        Some(r)
    }

    /// Range la caméra et le rendu de la vue active dans sa vue (multivue affichée).
    fn leave_pane(&mut self) {
        if !multiview::visible(&self.state) {
            return;
        }
        let (camera, inspector) = (self.state.camera, self.state.inspector);
        if let Some(m) = &mut self.state.multi {
            let a = m.active;
            if let Some(p) = m.pane_mut(a) {
                p.camera = camera;
                p.inspector = inspector;
            }
        }
    }

    /// Installe la caméra et le rendu de la vue active (son onglet doit être l'actif).
    fn enter_pane(&mut self) {
        let Some(m) = &self.state.multi else { return };
        let p = m.active_pane().clone();
        if p.tab != self.state.active_tab {
            return;
        }
        if let Some(r) = m.rect_of(self.state.multi_area.get(), p.id) {
            self.state.viewport.set(r);
        }
        self.state.camera = p.camera;
        self.state.inspector = p.inspector;
        self.doc.click_origin = None;
    }

    /// Rend la vue `pane` active.
    fn activate_pane(&mut self, pane: u64) {
        if multiview::visible(&self.state) && self.state.multi.as_ref().is_some_and(|m| m.active == pane) {
            return;
        }
        self.leave_pane();
        let Some(tab) = self.state.multi.as_mut().and_then(|m| {
            let tab = m.pane(pane)?.tab;
            m.active = pane;
            Some(tab)
        }) else {
            return;
        };
        self.activate(tab);
        self.enter_pane();
    }

    /// Onglet choisi par l'utilisateur : dans la multivue, sa vue devient l'active ; sinon il s'affiche seul (la
    /// multivue reste prête pour ses onglets).
    fn select_tab(&mut self, id: u64) {
        let pane = self.state.multi.as_ref().and_then(|m| {
            let a = m.active_pane();
            if a.tab == id { Some(a.id) } else { m.panes.iter().find(|p| p.tab == id).map(|p| p.id) }
        });
        match pane {
            Some(p) => self.activate_pane(p),
            None => {
                self.leave_pane();
                self.activate(id);
            }
        }
    }

    /// Caméra d'une nouvelle vue de l'onglet `tab` : la sienne (cadrée au besoin pour `aspect`).
    fn camera_for(&mut self, tab: u64, aspect: f64) -> Camera {
        self.peek(tab, |app| {
            if !app.doc.framed
                && let Some((lo, hi)) = app.doc.model.bounds()
            {
                app.state.camera = Camera::default();
                app.state.camera.frame(lo, hi, aspect);
                app.doc.framed = true;
            }
            app.state.camera
        })
        .unwrap_or_default()
    }

    /// Range les onglets de la multivue côte à côte dans la barre, à la place du premier d'entre eux.
    fn group_tabs(&mut self) {
        let Some(m) = &self.state.multi else { return };
        let group = m.tabs();
        let Some(at) = self.tabs.iter().position(|t| group.contains(&t.id)) else { return };
        let mut members: Vec<Tab> = Vec::new();
        for g in &group {
            if let Some(i) = self.tabs.iter().position(|t| t.id == *g) {
                members.push(self.tabs.remove(i));
            }
        }
        let at = at.min(self.tabs.len());
        for (k, t) in members.into_iter().enumerate() {
            self.tabs.insert(at + k, t);
        }
    }

    /// Défait la multivue : l'onglet de la vue active reste affiché seul.
    fn dissolve(&mut self) {
        self.leave_pane();
        let Some(m) = self.state.multi.take() else { return };
        let p = m.active_pane().clone();
        self.activate(p.tab);
        self.state.camera = p.camera;
        self.state.inspector = p.inspector;
    }

    /// Demandes de la multivue (au début de l'image).
    fn handle_multi_requests(&mut self) {
        for r in std::mem::take(&mut self.state.multi_requests) {
            match r {
                MultiRequest::Drop { tab, pane, zone } => {
                    if std::env::var_os("OBV_DEBUG_MULTI").is_some() {
                        eprintln!("multivue : dépôt de l'onglet {tab} dans {pane:?}, {zone:?}");
                    }
                    self.drop_tab(tab, pane, zone)
                }
                MultiRequest::Activate(p) => self.activate_pane(p),
                MultiRequest::Show { pane, tab } => {
                    self.leave_pane();
                    let area = self.state.multi_area.get();
                    let aspect = self.state.multi.as_ref().and_then(|m| m.rect_of(area, pane)).map(aspect_of);
                    let camera = self.camera_for(tab, aspect.unwrap_or(1.6));
                    if let Some(m) = &mut self.state.multi {
                        if let Some(p) = m.pane_mut(pane) {
                            p.tab = tab;
                            p.camera = camera;
                            p.inspector = Inspector::Off;
                        }
                        m.active = pane;
                    }
                    self.activate(tab);
                    self.enter_pane();
                    self.group_tabs();
                }
                MultiRequest::Close(pane) => {
                    self.leave_pane();
                    if let Some(m) = &mut self.state.multi {
                        m.remove(pane);
                    }
                    self.enter_multi_active();
                    if self.state.multi.as_ref().is_some_and(|m| m.panes.len() < 2) {
                        self.dissolve();
                    }
                }
                MultiRequest::Preset(p) => self.apply_preset(p),
            }
        }
    }

    /// Installe l'onglet de la vue active de la multivue, puis sa caméra.
    fn enter_multi_active(&mut self) {
        let Some(tab) = self.state.multi.as_ref().map(|m| m.active_pane().tab) else { return };
        self.activate(tab);
        self.enter_pane();
    }

    /// Onglet `tab` déposé dans la vue `pane` (aucune : vue unique) : partage de la vue visée.
    fn drop_tab(&mut self, tab: u64, pane: Option<u64>, zone: Zone) {
        if !self.tabs.iter().any(|t| t.id == tab) {
            return;
        }
        let target = match pane {
            Some(p) if multiview::visible(&self.state) => p,
            _ => {
                if zone == Zone::Center {
                    self.select_tab(tab);
                    return;
                }
                // Nouvelle multivue à partir de la vue unique (une ancienne multivue cachée est remplacée).
                self.state.multi = Some(Multi {
                    root: Split::Leaf(0),
                    panes: vec![Pane {
                        id: 0,
                        tab: self.state.active_tab,
                        camera: self.state.camera,
                        inspector: self.state.inspector,
                    }],
                    active: 0,
                    link_cameras: false,
                    link_selection: false,
                    next_id: 1,
                });
                self.state.multi_area.set(self.state.viewport.get());
                0
            }
        };
        self.leave_pane();
        let area = self.state.multi_area.get();
        let aspect = self
            .state
            .multi
            .as_ref()
            .and_then(|m| m.rect_of(area, target))
            .map(|r| multiview::zone_rect(r, zone))
            .map(aspect_of)
            .unwrap_or(1.0);
        let camera = if tab == self.state.active_tab { self.state.camera } else { self.camera_for(tab, aspect) };
        if let Some(m) = &mut self.state.multi {
            let id = m.split(target, zone, tab, camera);
            m.active = id;
        }
        self.activate(tab);
        self.enter_pane();
        self.group_tabs();
    }

    /// Disposition proposée : vues gardées dans l'ordre, ajoutées (onglet de la vue active) ou retirées.
    fn apply_preset(&mut self, preset: Preset) {
        if preset == Preset::Single {
            self.dissolve();
            return;
        }
        self.leave_pane();
        let Some(m) = &mut self.state.multi else { return };
        let mut ids = m.order();
        let keep = preset.panes();
        // Vues en trop : les dernières, la vue active gardée.
        while ids.len() > keep {
            let active = m.active;
            let Some(k) = ids.iter().rposition(|&i| i != active) else { break };
            let id = ids.remove(k);
            m.panes.retain(|p| p.id != id);
        }
        while ids.len() < keep {
            let a = m.active_pane().clone();
            let id = m.next_id;
            m.next_id += 1;
            m.panes.push(Pane { id, ..a });
            ids.push(id);
        }
        m.root = preset.tree(&ids);
        self.enter_multi_active();
        self.group_tabs();
    }

    /// Vues de l'image : l'active d'après `state`, les autres d'après leurs onglets (données envoyées au GPU au
    /// besoin).
    fn frame_views(&mut self) -> Vec<PaneView> {
        if !multiview::visible(&self.state) {
            return vec![PaneView::of(&self.state, self.state.viewport.get(), true)];
        }
        let m = self.state.multi.clone().expect("multivue");
        let (panes, _) = m.layout(self.state.multi_area.get());
        let mut out = Vec::new();
        for (id, r) in panes {
            let Some(p) = m.pane(id).cloned() else { continue };
            if id == m.active {
                out.push(PaneView::of(&self.state, r, true));
            } else if p.tab == self.state.active_tab {
                out.push(frame::pane_view(&self.state, r, p.camera, p.inspector));
            } else if let Some(v) = self.peek(p.tab, |app| {
                if let Some(g) = &mut app.gfx
                    && let Err(e) = sync_gpu(g, &mut app.doc, &app.state)
                {
                    eprintln!("Obview 3D : {e}");
                }
                frame::pane_view(&app.state, r, p.camera, p.inspector)
            }) {
                out.push(v);
            }
        }
        out
    }

    /// Caméras liées (D-42) : les autres vues suivent la vue active ; sélection liée : même nom de pièce.
    fn sync_panes(&mut self) {
        if !multiview::visible(&self.state) {
            return;
        }
        let Some(m) = self.state.multi.clone() else { return };
        let active_tab = self.state.active_tab;
        let sphere = |model: &Model| {
            model.bounds().map(|(lo, hi)| {
                let c = std::array::from_fn(|i| (lo[i] + hi[i]) as f64 * 0.5);
                let r = ((hi[0] - lo[0]) as f64).hypot((hi[1] - lo[1]) as f64).hypot((hi[2] - lo[2]) as f64) * 0.5;
                (r.max(1e-9), c)
            })
        };
        if m.link_cameras {
            let from = self.state.camera;
            let (r_from, c_from) = sphere(&self.doc.model).unwrap_or((1.0, [0.0; 3]));
            for p in &m.panes {
                if p.id == m.active {
                    continue;
                }
                let same = p.tab == active_tab;
                let (r_to, c_to) = if same {
                    (r_from, c_from)
                } else {
                    self.tabs
                        .iter()
                        .find(|t| t.id == p.tab)
                        .and_then(|t| t.stash.as_ref())
                        .and_then(|b| sphere(&b.1.model))
                        .unwrap_or((r_from, c_from))
                };
                let cam = multiview::linked_camera(&from, &p.camera, same, r_from, r_to, c_from, c_to);
                if let Some(q) = self.state.multi.as_mut().and_then(|mm| mm.pane_mut(p.id)) {
                    q.camera = cam;
                }
            }
        }
        if m.link_selection {
            let name = self.state.selected.and_then(|i| self.state.parts.get(i)).map(|p| p.name.clone());
            for t in &mut self.tabs {
                if t.id == active_tab || !m.panes.iter().any(|p| p.tab == t.id) {
                    continue;
                }
                if let Some(b) = &mut t.stash {
                    b.0.selected = name.as_ref().and_then(|n| b.0.parts.iter().position(|p| &p.name == n));
                }
            }
        }
    }

    /// Onglets vus par l'interface : nom, chemin, lecture.
    fn publish_tabs(&mut self) {
        let active = self.state.active_tab;
        let live = self.state.loading.as_ref().map(|l| l.overall());
        self.state.tabs = self
            .tabs
            .iter()
            .map(|t| TabInfo {
                id: t.id,
                name: file_name(&t.path),
                path: Some(t.path.clone()),
                loading: if t.id == active {
                    live
                } else {
                    t.stash.as_ref().and_then(|b| b.0.loading.as_ref().map(|l| l.overall()))
                },
                thumb: self.state.thumbs.get(&t.key).copied(),
            })
            .collect();
    }

    /// Enregistre les vues du document actif si elles ont changé.
    fn save_views(&mut self) {
        save_views(&mut self.views, &mut self.state, &self.doc);
    }

    /// Réponses des fils de travail.
    /// Recherche de mise à jour dans un fil ; `manual` : demandée depuis les réglages.
    fn start_update_check(&mut self, manual: bool) {
        if self.updating.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let proxy = self.proxy.clone();
        let prerelease = self.state.settings.prereleases;
        std::thread::spawn(move || {
            let _ = tx.send(UpdateMsg::Checked(update::check(prerelease), manual));
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
        // Recherche automatique : puis toutes les six heures tant que l'application reste ouverte (D-39).
        if self.state.settings.check_updates
            && self.updating.is_none()
            && matches!(self.state.update, update::Status::Idle | update::Status::Current)
            && unix_now().saturating_sub(self.state.settings.last_update_check) >= 6 * 3600
            && self.state.settings.last_update_check > 0
        {
            self.start_update_check(false);
        }
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
        if let Some(rx) = &self.thumbs_disk
            && let Ok(list) = rx.try_recv()
        {
            self.thumbs_disk = None;
            for (key, rgba) in list {
                // Une vignette déjà rendue dans cette séance est plus récente.
                if !self.state.thumbs.contains_key(&key) {
                    self.set_thumb(key, &rgba);
                }
            }
        }
        // Modèle reçu à l'image précédente (carte « Préparation » montrée) : installé maintenant.
        if self.ready.as_ref().is_some_and(|r| r.4)
            && let Some((id, _, m, prep, _)) = self.ready.take()
        {
            if let Some(l) = self.tab_loading(id) {
                *l = None;
            }
            let path = self.tabs.iter().find(|t| t.id == id).map(|t| t.path.clone());
            if let Some(path) = path {
                self.finish_open(id, path, m, prep);
            }
            self.start_next_load();
        }
        // Onglet en arrière-plan : pas de carte à montrer, le modèle est installé dès sa réception.
        if self.ready.as_ref().is_some_and(|r| r.0 != self.state.active_tab) {
            if let Some(r) = &mut self.ready {
                r.4 = true;
            }
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        if let Some((id, rx, progress, started)) = &self.loading {
            let (id, started) = (*id, *started);
            match rx.try_recv() {
                Ok(r) => {
                    if std::env::var_os("OBV_DEBUG_LOAD").is_some() {
                        eprintln!("lecture : terminée à {:.2} s", started.elapsed().as_secs_f64());
                    }
                    self.loading = None;
                    match r {
                        Ok((m, prep)) => {
                            if let Some(Some(l)) = self.tab_loading(id) {
                                l.stage = Stage::Prepare;
                                l.fraction = None;
                            }
                            let path =
                                self.tabs.iter().find(|t| t.id == id).map(|t| t.path.clone()).unwrap_or_default();
                            self.ready = Some((id, path, m, prep, false));
                        }
                        Err(e) => {
                            eprintln!("Obview 3D : {}", e);
                            let tab = self.tabs.iter().find(|t| t.id == id).map(|t| (t.path.clone(), t.loaded));
                            self.state.notice = Some(Notice {
                                title: "Impossible d'ouvrir",
                                subject: tab.as_ref().map(|t| file_name(&t.0)).unwrap_or_default(),
                                detail: e,
                            });
                            if let Some(l) = self.tab_loading(id) {
                                *l = None;
                            }
                            // Onglet encore vide : fermé ; fichier relu : l'onglet garde le modèle précédent.
                            if tab.is_some_and(|t| !t.1) {
                                self.close_tab(id);
                            }
                            self.start_next_load();
                        }
                    }
                }
                Err(_) => {
                    let (stage, fraction) = progress.get();
                    if let Some(Some(l)) = self.tab_loading(id) {
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
                        deviation_mm: r.deviation() as f64 * self.doc.model.unit_m * 1000.0,
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
                        layout: self.state.convert.layout,
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
        let Some(gfx) = &mut self.gfx else { return Err("pas de fenêtre".into()) };
        let v = self.state.viewport.get();
        let scale = gfx.window.scale_factor() as f32;
        let (vw, vh) = ((v.width() * scale).round() as u32, (v.height() * scale).round() as u32);
        let (w, h) = match size {
            CaptureSize::Uhd => (3840, (3840.0 * vh as f32 / vw.max(1) as f32).round() as u32),
            _ => size.pixels(vw, vh),
        };
        let rgba = gfx.capture(&self.state, &self.ui.theme, w, h, transparent)?;
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
        let (model, proxy, prog) = (self.doc.model.clone(), self.proxy.clone(), progress.clone());
        // Pièces de départ et filtres (D-38).
        let base = self.state.convert.base_parts(&self.state.parts, self.state.selected);
        let filter = self.state.convert.filter();
        // CAO : nouveau maillage du modèle exact, flèche de la moitié de l'écart et angle choisi (D-38).
        let source = (crate::convert_ui::is_cad(&self.state.file_name) && !self.doc.file_key.is_empty())
            .then(|| (PathBuf::from(&self.doc.file_key), self.state.convert.max_angle_deg));
        std::thread::spawn(move || {
            let p2 = proxy.clone();
            let report = |f: f32| {
                prog.store(f.to_bits(), Ordering::Relaxed);
                let _ = p2.send_event(Wake);
            };
            let r = convert_job(&model, source, &base, filter, &path, opts, &report)
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
        // Nouveau moteur : les données GPU de tous les onglets sont à renvoyer.
        for t in &mut self.tabs {
            if let Some(b) = &mut t.stash {
                b.1.scene_key = None;
                b.1.materials_sent = false;
            }
        }
        self.select_gpu_doc();
        self.doc.scene_key = None;
        self.doc.materials_sent = false;
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
        self.handle_tab_requests();
        self.handle_multi_requests();
        self.make_thumbnails();
        self.publish_tabs();
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
        // Multivue : un appui dans une autre vue (hors interface) la rend active avant la navigation, qui le
        // reçoit dans la même image.
        if multiview::visible(&self.state)
            && (0..3).any(|k| input.buttons[k] && !prev_buttons[k])
            && self.ui.hovered_raw().is_none()
            && let Some(p) = input.pointer
            && let Some(m) = &self.state.multi
            && let Some((id, _)) = m.layout(self.state.multi_area.get()).0.into_iter().find(|(_, r)| r.contains(p))
            && id != m.active
        {
            self.activate_pane(id);
        }
        let Some(gfx) = &mut self.gfx else { return };
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
        let moved = self.doc.nav.update(&ctx, &mut self.state.camera, &mut pick);
        if self.doc.nav.dome_delta != 0.0 {
            let l = &mut self.state.settings.lighting;
            l.rotation = (l.rotation + self.doc.nav.dome_delta).rem_euclid(360.0);
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
            self.doc.click_origin = Some(p);
            self.press_burst = burst;
        }
        // Appuis arrivés pendant que le bouton paraît tenu (images lentes : deux clics à cheval sur deux images).
        if !left_pressed && input.buttons[0] && burst > 0 && self.doc.click_origin.is_some() {
            self.press_burst = self.press_burst.saturating_add(burst);
        }
        if left_released && let (Some(o), Some(p)) = (self.doc.click_origin.take(), input.pointer) {
            let d = p - o;
            if d.x.abs() + d.y.abs() < 4.0 && self.state.inspector == Inspector::Bands {
                // Bandes (D-37, D-40) : un rendu choisi s'isole ; la vue éclairée, à droite, ferme les bandes.
                self.state.inspector = match inspector::band_at(view, &self.state, p) {
                    Some(m) => Inspector::Single(m),
                    None => Inspector::Off,
                };
            } else if d.x.abs() + d.y.abs() < 4.0 && self.state.tool == Tool::Measure {
                // Mesure : premier point, puis second point qui termine la cote.
                if let Some((q, _)) = Self::measure_point(gfx, &self.doc.model, &self.state, scale, p) {
                    match self.state.measure.pending.take() {
                        Some(a) => self.state.measure.done.push([a, q]),
                        None => self.state.measure.pending = Some(q),
                    }
                }
            } else if d.x.abs() + d.y.abs() < 4.0 {
                let px = [(p.x - view.min.x) * scale, (p.y - view.min.y) * scale];
                let hit = gfx.pick(&self.state.camera, [view.width() * scale, view.height() * scale], px);
                let part = hit.map(|h| h.id as usize);
                // Rendu isolé : un clic dans le vide rouvre les bandes, comme la visionneuse Marmoset (D-40).
                let reopen = part.is_none() && matches!(self.state.inspector, Inspector::Single(_));
                if reopen {
                    self.state.inspector = Inspector::Bands;
                }
                // Double-clic sur une pièce : sélection, liste des pièces ouverte et défilée jusqu'à elle.
                let double = !reopen
                    && part.is_some()
                    && (self.press_burst >= 2
                        || (self.doc.last_click.is_some_and(|(t, q)| now - t < 0.45 && q == part)
                            && self.state.selected == part));
                if !reopen {
                    self.state.selected = part;
                }
                if reopen {
                    self.doc.last_click = None;
                } else if double && let Some(i) = part {
                    let s = &mut self.state;
                    s.left_open = true;
                    s.root_open = true;
                    if !s.filtered().contains(&i) {
                        s.search.clear();
                    }
                    s.reveal = Some(i);
                    self.doc.last_click = None;
                } else {
                    self.doc.last_click = Some((now, part));
                }
            }
        }
        // Focus : la pièce sélectionnée, sinon tout le modèle (bouton ou F).
        if self.ui.clicked(Id::new("cadrer"))
            || self.ui.clicked(Id::new("cadrer pièce"))
            || (!self.ui.text_focused() && input.keys.contains(&Key::Char('f')) && !input.modifiers.ctrl)
        {
            focus(&mut self.doc.nav, &self.state, &self.doc.model, view, now);
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
                self.doc.nav.animate_to(&cam, Camera { yaw, pitch, ..cam }, now);
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
        if self.state.tool == Tool::Measure && !over_ui && !self.doc.nav.dragging() {
            if input.pointer != self.doc.measure_at {
                self.doc.measure_at = input.pointer;
                self.state.measure.hover = input
                    .pointer
                    .filter(|p| view.contains(*p))
                    .and_then(|p| Self::measure_point(gfx, &self.doc.model, &self.state, scale, p));
            }
        } else {
            self.state.measure.hover = None;
            self.doc.measure_at = None;
        }
        if let Some(to) = self.state.camera_request.take() {
            self.doc.nav.animate_to(&self.state.camera, to, now);
        }
        if !self.doc.framed && view.width() > 1.0 {
            if let Some((lo, hi)) = self.doc.model.bounds() {
                self.state.camera = Camera::default();
                self.state.camera.frame(lo, hi, (view.width() / view.height()) as f64);
            }
            self.doc.framed = true;
        }

        self.sync_panes();
        self.ui.theme = shell::theme(&self.state);
        let mut root = shell::build(&mut self.ui, &mut self.state);
        self.ui.finish(&mut root);
        if !self.state.requests.is_empty() {
            self.handle_requests();
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
        }
        let views = self.frame_views();
        let Some(gfx) = &mut self.gfx else { return };
        if let Err(e) = sync_gpu(gfx, &mut self.doc, &self.state) {
            self.error = Some(e);
        }

        if let Err(e) = gfx.present(&mut self.ui, &views, self.state.active_tab, scale) {
            self.error = Some(e);
        }
        // La carte « Préparation » est à l'écran : le modèle sera installé à l'image suivante.
        if let Some(r) = &mut self.ready
            && !r.4
        {
            r.4 = true;
            gfx.window.request_redraw();
        }
        if let Some(e) = self.error.take() {
            eprintln!("Obview 3D : {e}");
        }

        // Pointeur : forme demandée par l'interface, ou celle du geste de navigation.
        let icon = if self.doc.nav.dragging() {
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

        save_views(&mut self.views, &mut self.state, &self.doc);
        if self.state.settings_dirty && !self.ui.is_active_any() && now - self.last_save > 0.3 {
            if let Err(e) = self.state.settings.save() {
                eprintln!("Obview 3D : réglages non enregistrés : {e}");
            }
            self.state.settings_dirty = false;
            self.last_save = now;
        }
        let requests = !self.state.tab_requests.is_empty() || !self.state.multi_requests.is_empty();
        if again
            || moved
            || requests
            || !self.thumb_queue.is_empty()
            || self.doc.nav.animating()
            || self.ui.wants_redraw()
            || self.state.settings_dirty
            || !self.doc.framed
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

/// Travail de conversion (fil séparé) : nouveau maillage de la CAO s'il y a lieu, choix des pièces, puis
/// simplification et écriture (D-35, D-38).
fn convert_job(
    model: &Model,
    source: Option<(PathBuf, f64)>,
    base: &[bool],
    filter: obv_mesh::PartFilter,
    path: &std::path::Path,
    mut opts: obv_mesh::ConvertOptions,
    report: &(dyn Fn(f32) + Sync),
) -> Result<obv_mesh::Report, String> {
    let mut start = 0.0f32;
    let mut fresh = None;
    if let Some((src, angle)) = source {
        let options = obv_cad::Options {
            angular: angle.to_radians(),
            absolute_m: (opts.tolerance_m > 0.0).then_some(opts.tolerance_m * 0.5),
            ..Default::default()
        };
        let p = Progress::default();
        let read = std::thread::scope(|sc| {
            let h = sc.spawn(|| obv_cad::read_with(&src, options, &p));
            let mut shown = 0.0f32;
            while !h.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let (_, Some(f)) = p.get() {
                    shown = shown.max(f.clamp(0.0, 1.0) * 0.4);
                    report(shown);
                }
            }
            h.join()
        });
        // Mêmes pièces dans le même ordre : le choix des pièces garde ses indices ; sinon, maillage d'origine.
        if let Ok(Ok(m)) = read
            && m.parts.len() == model.parts.len()
        {
            fresh = Some(m);
            // La moitié de l'écart est prise par le maillage, l'autre reste à la simplification.
            opts.tolerance_m *= 0.5;
        }
        start = 0.4;
    }
    let model = fresh.as_ref().unwrap_or(model);
    let span = if filter.remove_hidden { 0.15 } else { 0.0 };
    let keep = obv_mesh::select_parts(model, base, filter, &|f| report(start + f * span));
    if !keep.iter().any(|k| *k) {
        return Err(tr("Aucune pièce à convertir").into());
    }
    let from = start + span;
    obv_mesh::convert_parts(model, Some(&keep), path, opts, &|f| report(from + f * (1.0 - from)))
        .map_err(|e| e.to_string())
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
            // Ctrl 4 à 9 : onglets (D-41), même position sur tous les claviers.
            KeyCode::Digit4 => Some(Key::Char('4')),
            KeyCode::Digit5 => Some(Key::Char('5')),
            KeyCode::Digit6 => Some(Key::Char('6')),
            KeyCode::Digit7 => Some(Key::Char('7')),
            KeyCode::Digit8 => Some(Key::Char('8')),
            KeyCode::Digit9 => Some(Key::Char('9')),
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
            // Recherche automatique : à chaque démarrage (D-39).
            if self.state.settings.check_updates {
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
/// Ouvre la fenêtre ; chaque fichier de `paths` dans un onglet (D-41), le premier actif.
pub fn run(paths: Vec<PathBuf>) -> Result<(), String> {
    let el = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    el.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(paths, el.create_proxy());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> obv_mesh::ConvertOptions {
        obv_mesh::ConvertOptions {
            format: obv_mesh::Format::Glb,
            tolerance_m: 2e-5,
            keep_silhouette: true,
            compression: obv_mesh::Compression::Quantize,
            layout: obv_mesh::Layout::Assembly,
        }
    }

    /// Choix des pièces (D-38) : une seule pièce gardée, puis un filtre qui ne laisse rien.
    #[test]
    fn conversion_avec_choix_des_pieces() {
        let model = crate::demo::bracket();
        let dir = std::env::temp_dir().join(format!("obv-choix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("une.glb");
        let mut base = vec![false; model.parts.len()];
        base[0] = true;
        let filter = obv_mesh::PartFilter { min_size_m: 0.0, remove_hidden: true };
        let r = convert_job(&model, None, &base, filter, &path, opts(), &|_| {}).unwrap();
        assert_eq!(r.parts.len(), 1);
        assert!(r.bytes > 0 && path.exists());
        let filter = obv_mesh::PartFilter { min_size_m: 10.0, remove_hidden: false };
        assert!(convert_job(&model, None, &[], filter, &path, opts(), &|_| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fichier local (`OBV_SAMPLE`, STEP avec `OCCT_DIR`) : nouveau maillage, pièces cachées retirées.
    #[test]
    #[ignore]
    fn conversion_fichier_local() {
        let Some(src) = std::env::var_os("OBV_SAMPLE").map(PathBuf::from) else { return };
        let model = load(&src).unwrap();
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        let source = crate::convert_ui::is_cad(&name).then(|| (src.clone(), 20.0));
        let out = std::env::temp_dir().join("obv-local.glb");
        let filter = obv_mesh::PartFilter { min_size_m: 0.0, remove_hidden: true };
        let t = Instant::now();
        let r = convert_job(&model, source, &[], filter, &out, opts(), &|_| {}).unwrap();
        eprintln!(
            "{:.1} s : {} pièces sur {}, {} → {} triangles, {} octets",
            t.elapsed().as_secs_f64(),
            r.parts.len(),
            model.parts.len(),
            r.triangles_in(),
            r.triangles_out(),
            r.bytes
        );
    }
}
