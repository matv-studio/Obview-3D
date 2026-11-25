//! Coque de l'application : barre du haut, arbre des pièces, vue 3D et ses outils, propriétés.

use std::{cell::Cell, rc::Rc};

use obv_io::Model;
use obv_render::Camera;
use obv_ui::{
    Align, Anchor, Color, Cursor, Edges, Id, Justify, Key, Node, Rect, Size, TextStyle, Theme, ThemeKind, Ui, Weight,
    controls::{self, TextInput},
    i18n, tr, vec2,
    widgets::{self, ButtonKind, TreeRow},
};

use crate::{
    convert_ui::{self, ConvertState, Request},
    gizmo,
    inspector::{self, Inspector},
    settings::{GizmoStyle, PANEL_MAX, PANEL_MIN, Settings, ThemeChoice, ToolButton, Units},
    settings_ui, tools, views,
};

/// Hauteur d'une ligne de l'arbre (liste virtuelle : hauteur fixe).
pub const TREE_ROW: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Measure,
    Section,
    Explode,
}

/// Résumé d'une pièce pour l'interface.
#[derive(Clone, Debug)]
pub struct PartInfo {
    pub name: String,
    pub triangles: usize,
    pub vertices: usize,
    pub size: [f32; 3],
    pub visible: bool,
    /// Couleur affichée (matériau du fichier, sinon acier par défaut).
    pub color: [u8; 4],
    /// Matériau du fichier (aucun : couleur par défaut de la visionneuse).
    pub material: Option<String>,
    /// Centre de la boîte englobante (vue éclatée).
    pub center: [f32; 3],
}

pub struct AppState {
    pub settings: Settings,
    /// Le système est-il en thème sombre (fourni par la fenêtre) ?
    pub system_dark: bool,
    /// Réglages modifiés depuis le dernier enregistrement.
    pub settings_dirty: bool,
    pub settings_open: bool,
    pub settings_section: usize,
    /// Saisie en cours de la couleur d'accent.
    pub accent_hex: String,
    pub file_name: String,
    pub unit_m: f64,
    pub parts: Vec<PartInfo>,
    pub selected: Option<usize>,
    pub root_open: bool,
    pub tool: Tool,
    pub search: String,
    pub camera: Camera,
    /// Panneaux latéraux ouverts (masqués par défaut : la vue occupe tout l'écran).
    pub left_open: bool,
    /// Pièce à amener dans la liste (double-clic dans la vue) : défilement dès que la liste est mesurée.
    pub reveal: Option<usize>,
    /// Mises à jour (D-32) : état et demande de l'interface, traitée par l'application.
    pub update: crate::update::Status,
    /// Éclairage : environnement chargé, aperçus, demandes (D-33).
    pub lighting: crate::lighting::EnvState,
    /// Onglet du panneau droit : 0 propriétés, 1 éclairage.
    pub right_tab: usize,
    /// Fichiers récemment ouverts, le plus récent en tête (D-33).
    pub recent: Vec<crate::recent::Recent>,
    /// Version affichée dans les réglages (celle de la construction ; fixée par les essais de capture).
    pub app_version: &'static str,
    pub update_request: Option<crate::update::Action>,
    /// Appuis gauches reçus depuis l'image précédente (deux appuis dans une même image lente : double-clic).
    pub left_presses: u8,
    pub right_open: bool,
    /// Vues de rendu (D-15).
    pub inspector: Inspector,
    /// Limites animées entre la vue normale et les bandes des rendus (fractions de la largeur de la vue).
    pub inspector_cuts: [f32; inspector::CUTS],
    /// Fenêtre de conversion.
    pub convert: ConvertState,
    /// Demandes à traiter par la fenêtre de l'application (fichiers, fils de travail).
    pub requests: Vec<Request>,
    /// Lecture en cours (nom, étape, avancement).
    pub loading: Option<crate::loading::Loading>,
    /// Un fichier est glissé au-dessus de la fenêtre.
    pub drop_hover: bool,
    /// Avis d'échec affiché en haut de la vue.
    pub notice: Option<crate::loading::Notice>,
    /// Moteur de rendu actif et sa description (réglages).
    pub renderer: (crate::gfx::Backend, String),
    /// Boîte englobante du modèle (unités du fichier).
    pub scene_bounds: Option<([f32; 3], [f32; 3])>,
    /// Rectangle de la vue 3D après la dernière mise en page (pixels logiques).
    pub viewport: Rc<Cell<Rect>>,
    /// Rectangle du plateau (vue et panneaux) à la dernière image.
    pub stage: Rc<Cell<Rect>>,
    /// Poignée de panneau tenue : côté, abscisse de l'appui, déjà glissée.
    pub grip_press: Option<(u8, f32, bool)>,
    /// Outils de la vue (D-26).
    pub measure: tools::MeasureState,
    pub section: tools::SectionState,
    /// Vue éclatée : 0 (assemblé) à 1.
    pub explode: f32,
    pub capture: tools::CaptureState,
    /// Vues enregistrées du fichier ouvert.
    pub views: views::ViewsState,
    /// Caméra demandée par l'interface (vues standard ou enregistrées), animée par la fenêtre.
    pub camera_request: Option<Camera>,
    /// Onglets ouverts (D-41), l'actif (`tabs::HOME` : accueil), demandes de la barre, glisser en cours.
    pub tabs: Vec<crate::tabs::TabInfo>,
    pub active_tab: u64,
    pub tab_requests: Vec<crate::tabs::TabRequest>,
    pub tab_drag: Option<crate::tabs::TabDrag>,
    /// Accueil : recherche dans les récents, vignettes des fichiers (couches des photos de l'interface).
    pub home_search: String,
    /// Multivue (D-42), ses demandes, zone des vues à la dernière mise en page, dépôt visé par un onglet glissé
    /// (vue — aucune : vue unique —, zone, rectangle de l'aperçu).
    pub multi: Option<crate::multiview::Multi>,
    pub multi_requests: Vec<crate::multiview::MultiRequest>,
    pub multi_area: Rc<Cell<Rect>>,
    pub tab_drop: Option<((Option<u64>, crate::multiview::Zone), Rect)>,
    pub thumbs: std::collections::HashMap<std::path::PathBuf, obv_ui::PhotoId>,
}

impl AppState {
    pub fn new(file_name: &str, model: &Model) -> Self {
        let parts = model
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let material = p
                    .mesh
                    .submeshes
                    .iter()
                    .find_map(|g| g.material)
                    .and_then(|m| model.materials.get(m as usize))
                    .map(|m| m.name.clone());
                let b = p.mesh.bounds();
                let size = b.map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]).unwrap_or([0.0; 3]);
                let center = b.map(|(lo, hi)| std::array::from_fn(|k| (lo[k] + hi[k]) * 0.5)).unwrap_or([0.0; 3]);
                PartInfo {
                    name: p.name.clone(),
                    triangles: p.mesh.triangle_count(),
                    vertices: p.mesh.positions.len(),
                    size,
                    visible: true,
                    color: model.display_color(i).unwrap_or(crate::frame::STEEL),
                    material,
                    center,
                }
            })
            .collect();
        AppState {
            settings: Settings::default(),
            system_dark: true,
            settings_dirty: false,
            settings_open: false,
            settings_section: 0,
            accent_hex: String::new(),
            file_name: file_name.to_owned(),
            unit_m: model.unit_m,
            parts,
            selected: None,
            root_open: true,
            tool: Tool::Select,
            search: String::new(),
            camera: Camera::default(),
            inspector: Inspector::Off,
            inspector_cuts: inspector::target_cuts(Inspector::Off, &Default::default()),
            left_open: false,
            reveal: None,
            update: Default::default(),
            app_version: crate::update::CURRENT,
            recent: Vec::new(),
            lighting: Default::default(),
            right_tab: 0,
            update_request: None,
            left_presses: 0,
            right_open: false,
            scene_bounds: model.bounds(),
            convert: ConvertState::default(),
            requests: Vec::new(),
            loading: None,
            drop_hover: false,
            notice: None,
            renderer: (crate::gfx::Backend::Vulkan, String::new()),
            viewport: Rc::new(Cell::new(Rect::ZERO)),
            stage: Rc::new(Cell::new(Rect::ZERO)),
            grip_press: None,
            measure: tools::MeasureState::default(),
            section: tools::SectionState::default(),
            explode: 0.0,
            capture: tools::CaptureState::default(),
            views: views::ViewsState::default(),
            camera_request: None,
            tabs: Vec::new(),
            active_tab: crate::tabs::HOME,
            tab_requests: Vec::new(),
            tab_drag: None,
            home_search: String::new(),
            multi: None,
            multi_requests: Vec::new(),
            multi_area: Rc::new(Cell::new(Rect::ZERO)),
            tab_drop: None,
            thumbs: Default::default(),
        }
    }

    pub fn total_triangles(&self) -> usize {
        self.parts.iter().map(|p| p.triangles).sum()
    }

    pub fn theme_kind(&self) -> ThemeKind {
        self.settings.theme_kind(self.system_dark)
    }

    /// Indices des pièces qui passent le filtre de recherche.
    pub fn filtered(&self) -> Vec<usize> {
        let f = self.search.trim().to_lowercase();
        (0..self.parts.len()).filter(|&i| f.is_empty() || self.parts[i].name.to_lowercase().contains(&f)).collect()
    }
}

/// Nombre lisible : 453 278 → « 453 k ».
pub fn compact(n: usize) -> String {
    match n {
        0..=9_999 => group(n),
        10_000..=999_999 => i18n::with_unit(&format!("{}", (n as f64 / 1000.0).round()), "k"),
        _ => i18n::with_unit(&i18n::decimal(n as f64 / 1e6, 1), "M"),
    }
}

/// Entier groupé par milliers selon la langue.
pub fn group(n: usize) -> String {
    i18n::group(n as u64)
}

fn length(v: f32, unit_m: f64, units: Units) -> String {
    units.format(v as f64 * unit_m)
}

/// Indicateur d'attente : cercle ouvert qui tourne.
pub fn spinner(ui: &mut Ui, size: f32, color: Color) -> Node {
    let t = ui.input.time as f32;
    ui.keep_animating();
    Node::custom(move |p, r| {
        let c = r.center();
        p.dl.set_rotation(c, t * 6.0);
        p.res.icons.draw(p.dl, p.res.atlas, "spinner", r, size, color);
        p.dl.clear_rotation();
    })
    .size(Size::Px(size), Size::Px(size))
}

/// Avancement circulaire (0 à 1) : piste discrète, arc plein.
pub fn progress_ring(ui: &Ui, size: f32, fraction: f32, color: Color) -> Node {
    let track = ui.theme.color.track;
    Node::custom(move |p, r| {
        let c = r.center();
        let radius = size * 0.5 - 1.5;
        let seg = 48;
        let point = |k: f32| {
            let a = -std::f32::consts::FRAC_PI_2 + k * std::f32::consts::TAU;
            obv_ui::vec2(c.x + radius * a.cos(), c.y + radius * a.sin())
        };
        for i in 0..seg {
            let (a, b) = (i as f32 / seg as f32, (i + 1) as f32 / seg as f32);
            p.dl.line(point(a), point(b), 1.5, track);
        }
        let n = (fraction.clamp(0.0, 1.0) * seg as f32).ceil() as usize;
        for i in 0..n {
            let a = i as f32 / seg as f32;
            let b = ((i + 1) as f32 / seg as f32).min(fraction);
            p.dl.line(point(a), point(b), 1.5, color);
        }
    })
    .size(Size::Px(size), Size::Px(size))
}

/// Bouton de mise à jour de la barre du haut (D-32, D-39) : seulement quand une version plus récente est publiée ;
/// « Version x.y.z », un clic propose d'installer et de redémarrer.
fn update_pill(ui: &mut Ui, s: &AppState) -> Option<Node> {
    use crate::update::Status;
    let release = s.update.release()?;
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let id = Id::new("mise à jour");
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) || ui.popup_open(id) { 1.0 } else { 0.0 }, 120.0);
    let fg = c.text_secondary.mix(c.text, hover);
    let version = format!("{} {}", tr("Version"), release.version);
    let tip = if release.prerelease { tr("Préversion disponible") } else { tr("Mise à jour disponible") };
    let (icon, label, tooltip) = match &s.update {
        Status::Downloading(_, f) => (
            progress_ring(ui, 14.0, *f, fg),
            i18n::with_unit(&format!("{}", (f * 100.0).floor() as u32), "%"),
            tip.to_string(),
        ),
        Status::Installing(_) => (spinner(ui, 14.0, fg), tr("Installation").to_string(), tip.to_string()),
        Status::Failed { error, .. } => (Node::icon("alert", m.icon, c.danger), version, format!("{tip} · {error}")),
        _ => (Node::icon("update", m.icon, fg), version, tip.to_string()),
    };
    let bg = if ui.pressed(id) { c.surface_active } else { c.raised.mul_alpha(hover) };
    Some(
        Node::row()
            .id(id)
            .tooltip(&tooltip)
            .height(Size::Px(m.control_height))
            .padding(Edges::xy(10.0, 0.0))
            .gap(7.0)
            .radius(m.radius_control)
            .bg(bg)
            .border(1.0, c.border.mix(c.border_strong, hover))
            .child(icon)
            .child(Node::text(label, TextStyle::new(m.font_size, Weight::Medium), fg)),
    )
}

/// Confirmation sous le bouton de mise à jour : « Installer Obview 3D x.y.z et redémarrer ? » (D-39).
fn update_popup(ui: &mut Ui, s: &mut AppState) {
    use crate::update::Status;
    let bid = Id::new("mise à jour");
    if ui.clicked(bid) && matches!(s.update, Status::Available(_) | Status::Failed { release: Some(_), .. }) {
        ui.toggle_popup(bid);
    }
    let Some(release) = s.update.release().cloned() else { return };
    if !ui.popup_open(bid) {
        return;
    }
    let pid = Id::new("mise à jour confirmation");
    if ui.clicked(pid.with("annuler")) {
        ui.close_popup();
        return;
    }
    if ui.clicked(pid.with("installer")) {
        s.update_request = Some(crate::update::Action::Install);
        ui.close_popup();
        ui.keep_animating();
        return;
    }
    let Some(r) = ui.rect_of(bid) else { return };
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let w = 360.0;
    let appear = ui.appear(pid, 120.0);
    let question = if i18n::is_fr() {
        format!("Installer Obview 3D {} et redémarrer ?", release.version)
    } else {
        format!("Install Obview 3D {} and restart?", release.version)
    };
    let card = Node::column()
        .anchor(Anchor {
            left: Some((r.max.x - w).max(8.0)),
            top: Some(r.max.y + 8.0 - (1.0 - appear) * 4.0),
            ..Default::default()
        })
        .opacity(appear)
        .width(Size::Px(w))
        .padding(Edges { left: 20.0, right: 16.0, top: 18.0, bottom: 16.0 })
        .gap(16.0)
        .radius(m.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, vec2(0.0, 16.0), c.shadow)
        .child(Node::text(question, TextStyle::new(13.5, Weight::Medium), c.text).width(Size::Fill(1.0)))
        .child(
            Node::row()
                .width(Size::Fill(1.0))
                .gap(8.0)
                .child(Node::spacer())
                .child(widgets::button(
                    ui,
                    pid.with("annuler"),
                    tr("Annuler"),
                    None,
                    ButtonKind::Secondary,
                    tr("Plus tard"),
                ))
                .child(widgets::button(
                    ui,
                    pid.with("installer"),
                    tr("Installer et redémarrer"),
                    None,
                    ButtonKind::Primary,
                    tr("Télécharger, installer puis relancer Obview 3D"),
                )),
        );
    ui.overlay(Node::stack().child(card), obv_ui::Layer::Popup);
}

/// Bouton des vues (standard et enregistrées), dans le coin haut droit de la vue (D-33).
fn views_button(ui: &mut Ui) -> Node {
    let id = Id::new("vues bouton");
    let c = ui.theme.color.clone();
    let open = ui.popup_open(id);
    let color = if open { c.text } else { c.icon };
    let mut b = widgets::small_icon_button(ui, id, "views", tr("Vues"), color).size(Size::Px(28.0), Size::Px(28.0));
    if open {
        b = b.bg(c.surface_active);
    }
    b
}

/// Menu des vues sous son bouton, aligné à droite : vues standard, vues enregistrées du fichier, enregistrer.
fn views_popup(ui: &mut Ui, s: &mut AppState) {
    let bid = Id::new("vues bouton");
    if ui.clicked(bid) {
        ui.toggle_popup(bid);
    }
    if !ui.popup_open(bid) || s.parts.is_empty() {
        return;
    }
    let Some(r) = ui.rect_of(bid) else { return };
    let c = ui.theme.color.clone();
    let rows = views::tab(ui, s);
    let w = 260.0;
    // Hauteur du contenu : en-têtes 32, lignes 28, bouton 28 et marges ; au plus 70 % de la fenêtre.
    let content = 2.0 * 32.0 + (7 + s.views.list.len()) as f32 * 28.0 + 8.0 + 4.0 + 28.0 + 12.0;
    let h = content.min(ui.input.size.y * 0.7);
    let appear = ui.appear(Id::new("vues menu"), 120.0);
    let menu = Node::column()
        .anchor(Anchor {
            left: Some((r.max.x - w).max(8.0)),
            top: Some(r.max.y + 6.0 - (1.0 - appear) * 4.0),
            ..Default::default()
        })
        .opacity(appear)
        .width(Size::Px(w))
        .height(Size::Px(h))
        .radius(ui.theme.size.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, vec2(0.0, 16.0), c.shadow)
        .clip()
        .child(
            Node::scroll(Id::new("vues défilement"))
                .width(Size::Fill(1.0))
                .height(Size::Fill(1.0))
                .padding(Edges::xy(6.0, 6.0))
                .children(rows),
        );
    ui.overlay(Node::stack().child(menu), obv_ui::Layer::Popup);
}

fn topbar(ui: &mut Ui, s: &AppState) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let theme_id = Id::new("thème");
    // Logo (D-36) : version claire sur le thème sombre, version sombre sur le thème clair.
    let logo = Node::image(if ui.theme.kind == ThemeKind::Dark { "obview-clair" } else { "obview-sombre" }, 24.0);
    let dark = s.theme_kind() == ThemeKind::Dark;
    let theme_icon = if dark { "sun" } else { "moon" };
    let theme_tip = tr(if dark { "Thème clair" } else { "Thème sombre" });
    let pill = update_pill(ui, s);
    // Onglets (D-41) : tout l'espace entre le logo et les commandes de droite.
    let right_w = 28.0 * 2.0 + 6.0 * 3.0 + 4.0 + 110.0 + if pill.is_some() { 150.0 } else { 0.0 };
    let avail = ui.input.size.x - 24.0 - 24.0 - 10.0 - right_w - 24.0;
    let left = Node::row().gap(10.0).child(logo).child(crate::tabs::bar(ui, s, avail));
    let right = Node::row()
        .gap(6.0)
        .children(pill.map(|p| [p, Node::new().size(Size::Px(4.0), Size::Px(1.0))]).into_iter().flatten())
        .child(
            widgets::small_icon_button(ui, theme_id, theme_icon, theme_tip, c.icon)
                .size(Size::Px(28.0), Size::Px(28.0)),
        )
        .child(
            widgets::small_icon_button(ui, Id::new("réglages"), "settings", tr("Réglages"), c.icon)
                .tooltip_shortcut(tr("Réglages"), "Ctrl ,")
                .size(Size::Px(28.0), Size::Px(28.0)),
        )
        .child(Node::new().size(Size::Px(4.0), Size::Px(1.0)))
        .child(
            widgets::button(
                ui,
                Id::new("convertir"),
                tr("Convertir"),
                Some("convert"),
                ButtonKind::Primary,
                tr("Convertir en maillage optimisé (GLB, OBJ, FBX)"),
            )
            // Rien à convertir sans modèle.
            .opacity(if s.parts.is_empty() { 0.4 } else { 1.0 }),
        );
    Node::row()
        .height(Size::Px(m.topbar_height))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(12.0, 0.0))
        .justify(Justify::SpaceBetween)
        .bg(c.app_bg)
        .child(left)
        .child(right)
}

fn parts_panel(ui: &mut Ui, s: &mut AppState) -> Node {
    let c = ui.theme.color.clone();
    let scroll_id = Id::new("arbre");
    let filtered = s.filtered();
    let count = 1 + if s.root_open { filtered.len() } else { 0 };
    // Pièce double-cliquée dans la vue : centrée dans la liste (défilement doux) dès que la liste a sa hauteur.
    if let Some(i) = s.reveal
        && let Some(st) = ui.scrolls.get_mut(&scroll_id)
        && st.view > 0.0
    {
        if let Some(k) = filtered.iter().position(|&f| f == i).filter(|_| s.root_open) {
            let content = count as f32 * TREE_ROW + 8.0;
            let top = (k + 1) as f32 * TREE_ROW + 4.0;
            st.target = (top - (st.view - TREE_ROW) * 0.5).clamp(0.0, (content - st.view).max(0.0));
        }
        s.reveal = None;
        // Rendu à la demande : l'image suivante doit venir sans attendre un évènement.
        ui.keep_animating();
    }
    let (range, before, after) = controls::virtual_range(ui, scroll_id, TREE_ROW, count);
    let mut list = Node::scroll(scroll_id).width(Size::Fill(1.0)).height(Size::Fill(1.0)).padding(Edges::xy(6.0, 4.0));
    list.push(Node::new().height(Size::Px(before)));
    for k in range {
        if k == 0 {
            list.push(widgets::tree_row(
                ui,
                TreeRow {
                    id: Id::new("racine"),
                    depth: 0,
                    expanded: Some(s.root_open),
                    icon: "assembly",
                    label: &s.file_name,
                    detail: Some(&group(filtered.len())),
                    selected: false,
                    visible: true,
                },
            ));
            continue;
        }
        let i = filtered[k - 1];
        let p = &s.parts[i];
        let detail = compact(p.triangles);
        let name = crate::lang::part_name(&p.name);
        list.push(widgets::tree_row(
            ui,
            TreeRow {
                id: Id::new("pièce").with(i),
                depth: 1,
                expanded: None,
                icon: "part",
                label: &name,
                detail: Some(&detail),
                selected: s.selected == Some(i),
                visible: p.visible,
            },
        ));
    }
    list.push(Node::new().height(Size::Px(after)));
    let (search, changed) = controls::text_input(
        ui,
        TextInput {
            id: Id::new("recherche"),
            placeholder: tr("Rechercher"),
            width: Size::Fill(1.0),
            icon: Some("search"),
            hint: Some("Ctrl F"),
            clearable: true,
        },
        &mut s.search,
    );
    if changed && let Some(st) = ui.scrolls.get_mut(&scroll_id) {
        st.target = 0.0;
        st.offset = 0.0;
    }
    let header = Node::column().padding(Edges { left: 8.0, right: 8.0, top: 10.0, bottom: 6.0 }).gap(8.0).child(search);
    let shown = if s.search.trim().is_empty() {
        i18n::pick(
            format!("{} parts · {} triangles", group(s.parts.len()), compact(s.total_triangles())),
            format!("{} pièces · {} triangles", group(s.parts.len()), compact(s.total_triangles())),
        )
    } else {
        i18n::pick(
            format!("{} of {} parts", group(filtered.len()), group(s.parts.len())),
            format!("{} sur {} pièces", group(filtered.len()), group(s.parts.len())),
        )
    };
    let footer = Node::row()
        .height(Size::Px(32.0))
        .padding(Edges::xy(14.0, 0.0))
        .gap(6.0)
        .child(Node::icon("layers", 13.0, c.text_tertiary))
        .child(Node::text(shown, TextStyle::new(11.5, Weight::Regular), c.text_tertiary));
    widgets::panel(ui, s.settings.left_width)
        .border_right(c.border)
        .child(header)
        .child(list)
        .child(widgets::divider(ui))
        .child(footer)
}

/// Panneau droit : onglets Propriétés et Éclairage (D-33).
fn properties_panel(ui: &mut Ui, s: &mut AppState) -> Node {
    let c = ui.theme.color.clone();
    for i in 0..2 {
        if ui.clicked(Id::new("onglets droite").with(i)) {
            s.right_tab = i;
        }
    }
    let tabs = widgets::tabs(ui, Id::new("onglets droite"), &[tr("Propriétés"), tr("Éclairage")], s.right_tab);
    let header = Node::column().padding(Edges { left: 8.0, right: 8.0, top: 10.0, bottom: 4.0 }).child(tabs);
    let body = if s.right_tab == 1 {
        Node::scroll(Id::new("éclairage défilement"))
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0))
            .padding(Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 10.0 })
            .children(crate::lighting::tab(ui, s))
    } else {
        properties_content(ui, s)
    };
    widgets::panel(ui, s.settings.right_width).border_left(c.border).child(header).child(body)
}

fn properties_content(ui: &mut Ui, s: &AppState) -> Node {
    let units = s.settings.units;
    let c = ui.theme.color.clone();
    let mut col = Node::column().width(Size::Fill(1.0)).padding(Edges::xy(0.0, 6.0));
    match s.selected.and_then(|i| s.parts.get(i)) {
        Some(p) => {
            col.push(
                Node::row()
                    .padding(Edges { left: 12.0, right: 8.0, top: 6.0, bottom: 8.0 })
                    .gap(8.0)
                    .width(Size::Fill(1.0))
                    .child(
                        Node::stack()
                            .size(Size::Px(28.0), Size::Px(28.0))
                            .radius(7.0)
                            .bg(c.accent_soft)
                            .child(Node::icon("part", 15.0, c.accent)),
                    )
                    .child(
                        Node::text(crate::lang::part_name(&p.name), TextStyle::new(13.5, Weight::SemiBold), c.text)
                            .width(Size::Fill(1.0))
                            .truncate(),
                    )
                    .child(widgets::small_icon_button(ui, Id::new("cadrer pièce"), "frame", tr("Focus"), c.icon)),
            );
            col.push(widgets::divider(ui));
            col.push(widgets::section_header(ui, tr("Géométrie")));
            col.push(widgets::property_row(ui, tr("Triangles"), group(p.triangles)));
            col.push(widgets::property_row(ui, tr("Sommets"), group(p.vertices)));
            col.push(widgets::property_row(ui, tr("Largeur"), length(p.size[0], s.unit_m, units)));
            col.push(widgets::property_row(ui, tr("Hauteur"), length(p.size[1], s.unit_m, units)));
            col.push(widgets::property_row(ui, tr("Profondeur"), length(p.size[2], s.unit_m, units)));
            col.push(Node::new().height(Size::Px(8.0)));
            col.push(widgets::divider(ui));
            col.push(widgets::section_header(ui, tr("Apparence")));
            let [r, g, b, _] = p.color;
            let hex = format!("#{r:02X}{g:02X}{b:02X}");
            col.push(widgets::color_property(ui, tr("Couleur"), Color::hex(u32::from_be_bytes([0, r, g, b])), &hex));
            let material = match p.material.as_deref() {
                None => tr("Par défaut").into(),
                Some("") => tr("Sans nom").into(),
                Some(n) => crate::lang::part_name(n),
            };
            col.push(widgets::property_row(ui, tr("Matériau"), material));
        }
        None => {
            col.push(widgets::section_header(ui, tr("Modèle")));
            col.push(widgets::property_row(ui, tr("Fichier"), &s.file_name));
            col.push(widgets::property_row(ui, tr("Pièces"), group(s.parts.len())));
            col.push(widgets::property_row(ui, tr("Triangles"), group(s.total_triangles())));
        }
    }
    col
}

/// Identifiant d'un bouton de la barre du bas (les outils partagent « outil »).
pub fn tool_button_id(b: ToolButton) -> Id {
    match b {
        ToolButton::Select => Id::new("outil").with(Tool::Select as u8),
        ToolButton::Measure => Id::new("outil").with(Tool::Measure as u8),
        ToolButton::Section => Id::new("outil").with(Tool::Section as u8),
        ToolButton::Explode => Id::new("outil").with(Tool::Explode as u8),
        ToolButton::Render => Id::new("rendu"),
        ToolButton::Projection => Id::new("projection"),
        ToolButton::Capture => Id::new("capture"),
        ToolButton::Frame => Id::new("cadrer"),
        ToolButton::Panels => Id::new("panneaux"),
    }
}

/// Barre du bas : boutons choisis dans les réglages, séparateur entre deux groupes (D-25).
fn toolbar(ui: &mut Ui, s: &mut AppState) -> Option<Node> {
    // Sans modèle, les outils n'ont rien sur quoi agir.
    if s.parts.is_empty() {
        return None;
    }
    let mut items = Vec::new();
    let mut last_group = None;
    for b in ToolButton::ALL {
        if s.settings.toolbar_hidden.contains(&b) {
            continue;
        }
        if last_group.is_some_and(|g| g != b.group()) {
            items.push(widgets::toolbar_separator(ui));
        }
        last_group = Some(b.group());
        let active = match b {
            ToolButton::Select => s.tool == Tool::Select,
            ToolButton::Measure => s.tool == Tool::Measure,
            ToolButton::Section => s.tool == Tool::Section,
            ToolButton::Explode => s.tool == Tool::Explode,
            ToolButton::Render => s.inspector != Inspector::Off,
            ToolButton::Projection => s.camera.ortho,
            ToolButton::Panels => s.left_open || s.right_open,
            _ => false,
        };
        items.push(widgets::icon_button(ui, tool_button_id(b), b.icon(), b.label(), b.shortcut(), active));
    }
    // Multivue (D-42) : disposition et liaisons, seulement quand plusieurs vues sont ouvertes.
    if let Some(b) = crate::multiview::layout_button(ui, s) {
        items.push(widgets::toolbar_separator(ui));
        items.push(b);
    }
    tools::capture_popup(ui, s, None);
    crate::multiview::layout_popup(ui, s);
    (!items.is_empty()).then(|| widgets::docked_toolbar(ui, items))
}

/// Poignée d'un panneau latéral (0 : gauche, 1 : droite) : clic pour afficher ou masquer, glisser pour régler
/// la largeur (200 à 520 px, charte). Posée sur le bord du plateau ou du panneau ouvert.
/// `edge` : largeur affichée du panneau (animée à l'ouverture et à la fermeture).
fn grip(ui: &mut Ui, side: u8, edge: f32) -> Node {
    let id = grip_id(side);
    let c = ui.theme.color.clone();
    let open = edge > 0.5;
    let t = ui.anim(id.with("survol"), if ui.hovered(id) || ui.is_active(id) { 1.0 } else { 0.0 }, 120.0);
    // Zone sensible de 10 px : 3 px vers l'extérieur, 7 px vers la vue.
    let (hit_w, out) = (10.0, 3.0);
    let line = if c.has_accent { c.accent } else { c.text_tertiary };
    let pill_rest = c.text_tertiary.with_alpha(0.55);
    let pill_hover = c.text;
    let node = Node::custom(move |p, r| {
        // Bord du panneau (ou du plateau) dans le rectangle de la poignée.
        let bx = if side == 0 { r.min.x + out } else { r.max.x - out };
        if open && t > 0.01 {
            let x = bx - 1.0;
            p.dl.rect(Rect::new(vec2(x, r.min.y), vec2(x + 2.0, r.max.y)), 0.0, line.with_alpha(t), None);
        }
        // Pastille de prise au milieu : sur le trait si le panneau est ouvert, juste à l'intérieur sinon.
        let px = if open {
            bx - 2.0
        } else if side == 0 {
            bx + 4.0
        } else {
            bx - 8.0
        };
        let h = 40.0;
        let y = r.center().y - h * 0.5;
        p.dl.rect(Rect::from_xywh(px, y, 4.0, h), 2.0, pill_rest.mix(pill_hover, t), None);
    })
    .id(id)
    .cursor(Cursor::ResizeHorizontal)
    .tooltip_shortcut(tr(if side == 0 { "Pièces" } else { "Propriétés" }), if side == 0 { "Ctrl [" } else { "Ctrl ]" })
    .width(Size::Px(hit_w))
    .height(Size::Fill(1.0));
    let a = if side == 0 {
        Anchor { left: Some(edge - out), top: Some(0.0), bottom: Some(0.0), ..Default::default() }
    } else {
        Anchor { right: Some(edge - out), top: Some(0.0), bottom: Some(0.0), ..Default::default() }
    };
    node.anchor(a)
}

pub fn grip_id(side: u8) -> Id {
    Id::new("poignée").with(side)
}

fn viewport(ui: &mut Ui, s: &mut AppState) -> Node {
    // Multivue (D-42) : la zone des vues est partagée ; le contenu habituel de la vue va dans la vue active, dont
    // le rectangle devient celui de la vue (navigation, sélection, outils).
    if crate::multiview::visible(s) {
        let (area_cell, rect_cell) = (s.multi_area.clone(), s.viewport.clone());
        let m = s.multi.clone().expect("multivue");
        let content = view_content(ui, s, None);
        let mut root = Node::stack().fill().child(
            Node::custom(move |_, r| {
                area_cell.set(r);
                rect_cell.set(m.rect_of(r, m.active).unwrap_or(r));
            })
            .fill(),
        );
        for n in crate::multiview::overlay(ui, s, content) {
            root.push(n);
        }
        return root;
    }
    let rect_cell = s.viewport.clone();
    view_content(ui, s, Some(Node::custom(move |_, r| rect_cell.set(r)).fill()))
}

/// Contenu de la vue (rendus, outils, lecture, avis, gizmo, menu des vues) ; `setter` : nœud qui relève le
/// rectangle de la vue.
fn view_content(ui: &mut Ui, s: &mut AppState, setter: Option<Node>) -> Node {
    let mut root = Node::stack().fill().children(setter);
    if let Some(layer) = inspector::overlay(ui, s) {
        root.push(layer);
    }
    if let Some(layer) = inspector::stats(ui, s) {
        root.push(layer);
    }
    if let Some(layer) = tools::overlay(ui, s) {
        root.push(layer);
    }
    if let Some(p) = tools::tool_panel(ui, s) {
        root.push(p);
    }
    if let Some(card) = crate::loading::card(ui, s) {
        root.push(card);
    }
    if let Some(n) = crate::loading::notice(ui, s) {
        root.push(n);
    }
    let style = s.settings.gizmo;
    // Sans modèle : rien à orienter.
    if style != GizmoStyle::None && !s.parts.is_empty() {
        let (yaw, pitch) = (s.camera.yaw as f32, s.camera.pitch as f32);
        let theme = ui.theme.clone();
        let gid = Id::new("gizmo");
        let hover = match (ui.hovered(gid), ui.rect_of(gid), ui.input.pointer) {
            (true, Some(r), Some(p)) => gizmo::pick(style, r, yaw, pitch, p),
            _ => None,
        };
        let active = ui.anim(gid.with("disque"), if ui.hovered(gid) || ui.is_active(gid) { 1.0 } else { 0.0 }, 120.0);
        let side = gizmo::size(style);
        root.push(
            Node::custom(move |p, r| gizmo::paint(style, p, r, yaw, pitch, hover, active, &theme))
                .id(gid)
                .cursor(if hover.is_some() { Cursor::Default } else { Cursor::Grab })
                .tooltip(hover.map(gizmo::name).unwrap_or_else(|| tr("Orienter la vue").into()))
                .size(Size::Px(side), Size::Px(side))
                .anchor(Anchor { top: Some(12.0), right: Some(12.0), ..Default::default() }),
        );
    }
    // Menu des vues : coin haut droit de la vue, contre le gizmo (D-33), posé après lui pour recevoir le clic.
    if !s.parts.is_empty() {
        root.push(views_button(ui).anchor(Anchor { top: Some(8.0), right: Some(8.0), ..Default::default() }));
    }
    root
}

/// Construit l'interface complète pour l'état courant (les commandes y appliquent leurs changements).
///
/// Charte des apps : cadre (`app_bg`) avec la barre du haut, puis un plateau unique aux coins arrondis
/// (marge 6 px, contour `stage_line`) qui contient les panneaux et la vue 3D.
pub fn build(ui: &mut Ui, s: &mut AppState) -> Node {
    // Langue et animations d'après les réglages, avant tout texte.
    crate::lang::init();
    i18n::set_lang(s.settings.language.lang());
    ui.reduce_motion = !s.settings.animations;
    let m = ui.theme.size.clone();
    let c = ui.theme.color.clone();
    // Vues de rendu : les bandes entrent depuis la gauche, un rendu choisi s'élargit à toute la vue (350 ms).
    let target = inspector::targets(s);
    for (k, t) in target.iter().enumerate() {
        s.inspector_cuts[k] = ui.tween(Id::new("inspecteur limite").with(k), *t, 350.0);
    }
    // Panneaux latéraux : ouverture et fermeture en glissement (200 ms), contenu à pleine largeur découpé.
    let lo = ui.tween(Id::new("panneau gauche"), if s.left_open { 1.0 } else { 0.0 }, 200.0);
    let ro = ui.tween(Id::new("panneau droit"), if s.right_open { 1.0 } else { 0.0 }, 200.0);
    let (lw, rw) = ((s.settings.left_width * lo).round(), (s.settings.right_width * ro).round());
    let slide = |panel: Node, w: f32, left: bool| {
        let a = if left {
            Anchor { right: Some(0.0), top: Some(0.0), ..Default::default() }
        } else {
            Anchor { left: Some(0.0), top: Some(0.0), ..Default::default() }
        };
        Node::stack().width(Size::Px(w)).height(Size::Fill(1.0)).clip().child(panel.anchor(a))
    };
    // Accueil (D-41) : la page des récents remplace la vue et les panneaux.
    let home = s.active_tab == crate::tabs::HOME;
    let (lw, rw) = if home { (0.0, 0.0) } else { (lw, rw) };
    let left = (lw >= 1.0).then(|| parts_panel(ui, s)).map(|p| slide(p, lw, true));
    let view = if home {
        let page = crate::home::page(ui, s);
        let rect_cell = s.viewport.clone();
        Node::stack()
            .fill()
            .child(Node::custom(move |_, r| rect_cell.set(r)).fill())
            .child(page)
            .children(crate::loading::notice(ui, s))
    } else {
        viewport(ui, s)
    };
    let right = (rw >= 1.0).then(|| properties_panel(ui, s)).map(|p| slide(p, rw, false));
    let body = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
        .align(Align::Stretch)
        .children(left)
        .child(view)
        .children(right);
    let (radius, margin) = (m.radius_stage, m.stage_margin);
    // La vue 3D est dessinée avant l'interface : un anneau couleur du cadre recouvre ce qui dépasse des
    // coins arrondis, puis le contour du plateau est tracé par-dessus.
    let stage_cell = s.stage.clone();
    let frame = Node::custom(move |p, r| {
        stage_cell.set(r);
        p.dl.rect(r.expand(margin), radius + margin, Color::TRANSPARENT, Some((margin, c.app_bg)));
        p.dl.rect(r, radius, Color::TRANSPARENT, Some((1.0, c.stage_line)));
    })
    .fill();
    // Au-dessus du contour : barre du bas en onglet (centrée sur la vue) et poignées des panneaux.
    let spacer = |w: f32| Node::new().width(Size::Px(w)).height(Size::Fill(1.0));
    // Multivue : la barre des outils se centre sous la vue active (ses outils agissent sur elle).
    let (lw_bar, rw_bar) = match s.multi.as_ref().filter(|_| crate::multiview::visible(s)) {
        Some(mv) => {
            let (st, area) = (s.stage.get(), s.multi_area.get());
            mv.rect_of(area, mv.active)
                .map(|r| ((r.min.x - st.min.x).max(0.0), (st.max.x - r.max.x).max(0.0)))
                .unwrap_or((lw, rw))
        }
        None => (lw, rw),
    };
    let lane = Node::row()
        .fill()
        .align(Align::Stretch)
        .children((lw_bar >= 1.0).then(|| spacer(lw_bar)))
        .child(Node::stack().fill().children((!home).then(|| toolbar(ui, s)).flatten()))
        .children((rw_bar >= 1.0).then(|| spacer(rw_bar)));
    let overlay = Node::stack()
        .fill()
        .child(lane)
        .children((!s.parts.is_empty()).then(|| [grip(ui, 0, lw), grip(ui, 1, rw)]).into_iter().flatten());
    // Multivue : contours de chaque vue, aperçu du dépôt d'un onglet glissé.
    let panes = if home { None } else { crate::multiview::frames(s, &ui.theme, radius) };
    let drop = crate::multiview::drop_preview(ui, s);
    let chrome = if home { Vec::new() } else { crate::multiview::chrome(ui, s) };
    let stage =
        Node::stack().fill().child(body).child(frame).children(panes).children(chrome).children(drop).child(overlay);
    let column = Node::column().fill().child(topbar(ui, s)).child(
        Node::stack().fill().padding(Edges { left: margin, right: margin, top: 0.0, bottom: margin }).child(stage),
    );
    // Activité longue : ligne de 2 px en haut de la fenêtre (charte).
    let root = Node::stack().fill().child(column).children(crate::loading::top_line(ui, s));
    views_popup(ui, s);
    update_popup(ui, s);
    settings_ui::dialog(ui, s);
    convert_ui::dialog(ui, s);
    root
}

/// Poignées des panneaux : un clic affiche ou masque, un glisser règle la largeur ; tiré sous la moitié de la
/// largeur minimale, le panneau se masque.
fn handle_grips(ui: &mut Ui, s: &mut AppState) {
    for side in [0u8, 1] {
        let id = grip_id(side);
        if ui.press_started(id)
            && let Some(p) = ui.input.pointer
        {
            s.grip_press = Some((side, p.x, false));
        }
        if let Some(p) = ui.dragging(id)
            && let Some((sd, x0, moved)) = s.grip_press
            && sd == side
        {
            if !moved && (p.x - x0).abs() < 3.0 {
                continue;
            }
            s.grip_press = Some((sd, x0, true));
            let st = s.stage.get();
            let w = if side == 0 { p.x - st.min.x } else { st.max.x - p.x };
            let open = w >= PANEL_MIN * 0.5;
            let w = w.clamp(PANEL_MIN, PANEL_MAX.min((st.width() - 240.0).max(PANEL_MIN)));
            if side == 0 {
                s.left_open = open;
                if open {
                    s.settings.left_width = w.round();
                }
            } else {
                s.right_open = open;
                if open {
                    s.settings.right_width = w.round();
                }
            }
        }
        if ui.clicked(id) {
            let dragged = s.grip_press.is_some_and(|(sd, _, moved)| sd == side && moved);
            s.grip_press = None;
            if dragged {
                s.settings_dirty = true;
            } else if side == 0 {
                s.left_open = !s.left_open;
            } else {
                s.right_open = !s.right_open;
            }
        }
    }
    if !ui.is_active_any() {
        s.grip_press = None;
    }
}

/// Applique les clics et raccourcis de l'image.
pub fn handle(ui: &mut Ui, s: &mut AppState) {
    crate::lang::init();
    i18n::set_lang(s.settings.language.lang());
    // Mise à jour (D-39) : le clic sur le bouton ouvre la confirmation (`update_popup`) ; sans fichier à
    // installer, après un échec de la recherche, il relance la recherche.
    if ui.clicked(Id::new("mise à jour")) && matches!(s.update, crate::update::Status::Failed { release: None, .. }) {
        s.update_request = Some(crate::update::Action::Check);
        ui.keep_animating();
    }
    if ui.clicked(Id::new("thème")) {
        s.settings.theme = if s.theme_kind() == ThemeKind::Dark { ThemeChoice::Light } else { ThemeChoice::Dark };
        s.settings_dirty = true;
    }
    if ui.clicked(Id::new("réglages")) {
        s.settings_open = true;
    }
    if ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char('[')) {
        s.left_open = !s.left_open;
    }
    if ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char(']')) {
        s.right_open = !s.right_open;
    }
    handle_grips(ui, s);
    // Les deux panneaux ensemble : bouton de la barre du bas ou Ctrl \ (fermés si l'un est ouvert).
    if ui.clicked(Id::new("panneaux")) || (ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char('\\'))) {
        let open = !(s.left_open || s.right_open);
        s.left_open = open;
        s.right_open = open;
    }
    if ui.clicked(Id::new("convertir")) && !s.parts.is_empty() {
        s.convert.open = true;
        if !matches!(s.convert.status, convert_ui::Status::Running(_)) {
            s.convert.status = convert_ui::Status::Idle;
        }
    }
    crate::tabs::handle(ui, s);
    crate::multiview::handle(ui, s);
    if s.active_tab == crate::tabs::HOME {
        crate::home::handle(ui, s);
    }
    // Ctrl O (comme Ctrl T) : choisir un fichier, ouvert dans un nouvel onglet (D-41).
    if ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char('o')) {
        s.requests.push(Request::OpenFile);
    }
    let ctrl = ui.input.modifiers.ctrl;
    if ctrl && ui.input.keys.contains(&Key::Char(',')) {
        s.settings_open = !s.settings_open;
    }
    if !s.settings_open && ctrl && ui.input.keys.contains(&Key::Char('f')) {
        ui.focus(Id::new("recherche"));
    }
    let plain = !ctrl && !ui.text_focused() && !s.settings_open && !s.convert.open;
    // Vues de rendu : R (ou le bouton) affiche toutes les vues ; depuis une vue isolée, il réaffiche le menu ;
    // depuis le menu, il le referme.
    if ui.clicked(Id::new("rendu")) || (plain && ui.input.keys.contains(&Key::Char('r'))) {
        s.inspector = match s.inspector {
            Inspector::Bands => Inspector::Off,
            _ => Inspector::Bands,
        };
    }
    // Échap pendant une lecture l'annule, avant tout autre usage.
    let cancel_open = s.loading.is_some() && !s.settings_open && !s.convert.open && ui.escape_pressed();
    if cancel_open {
        s.requests.push(Request::CancelOpen);
    }
    if ui.clicked(Id::new("inspecteur retour")) {
        s.inspector = Inspector::Bands;
    } else if !cancel_open && !s.settings_open && ui.escape_pressed() {
        s.inspector = match s.inspector {
            Inspector::Single(_) => Inspector::Bands,
            _ => Inspector::Off,
        };
    }
    tools::handle(ui, s);
    if ui.clicked(Id::new("projection")) || (plain && ui.input.keys.contains(&Key::Char('p'))) {
        s.camera.ortho = !s.camera.ortho;
    }
    if plain && ui.input.keys.contains(&Key::Escape) && s.measure.pending.is_some() {
        s.measure.pending = None;
    } else if plain && !cancel_open && ui.escape_pressed() && s.tool != Tool::Select && s.inspector == Inspector::Off {
        s.tool = Tool::Select;
    }
    // Un outil déjà actif, recliqué (ou sa touche), rend la main à la sélection.
    let mut pick_tool = |t: Tool| s.tool = if s.tool == t { Tool::Select } else { t };
    if plain {
        for (k, t) in [('v', Tool::Select), ('m', Tool::Measure), ('c', Tool::Section), ('e', Tool::Explode)] {
            if ui.input.keys.contains(&Key::Char(k)) {
                pick_tool(t);
            }
        }
    }
    for t in [Tool::Select, Tool::Measure, Tool::Section, Tool::Explode] {
        if ui.clicked(Id::new("outil").with(t as u8)) {
            pick_tool(t);
        }
    }
    if ui.clicked(Id::new("racine")) || ui.clicked(Id::new("racine").with("flèche")) {
        s.root_open = !s.root_open;
    }
    for i in 0..s.parts.len() {
        if ui.clicked(Id::new("pièce").with(i).with("œil")) {
            s.parts[i].visible = !s.parts[i].visible;
        } else if ui.double_clicked(Id::new("pièce").with(i))
            || (ui.clicked(Id::new("pièce").with(i)) && s.left_presses >= 2)
        {
            // Double-clic dans la liste : pièce sélectionnée et panneau des propriétés ouvert.
            s.selected = Some(i);
            s.right_open = true;
        } else if ui.clicked(Id::new("pièce").with(i)) {
            s.selected = if s.selected == Some(i) { None } else { Some(i) };
        }
    }
}

/// Teinte des pièces sélectionnées dans la vue 3D : l'accent s'il y en a un, sinon le bleu fonctionnel
/// « mode actif » de la charte (le contraste inversé ne se lit pas sur une pièce grise).
pub fn selection_color(theme: &Theme) -> Color {
    if theme.color.has_accent { theme.color.stage_line } else { theme.color.set_text }
}

/// Thème correspondant à l'état.
pub fn theme(s: &AppState) -> Theme {
    s.settings.theme(s.system_dark)
}
