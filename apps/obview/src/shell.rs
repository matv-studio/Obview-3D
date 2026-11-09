//! Coque de l'application : barre du haut, arbre des pièces, vue 3D et ses outils, propriétés.

use std::{cell::Cell, rc::Rc};

use obv_io::Model;
use obv_render::Camera;
use obv_ui::{
    Align, Anchor, Color, Cursor, Edges, Id, Justify, Key, Node, Rect, Size, TextStyle, Theme, ThemeKind, Ui, Weight,
    controls::{self, TextInput},
    vec2,
    widgets::{self, ButtonKind, TreeRow},
};

use crate::{
    convert_ui::{self, ConvertState, Request},
    gizmo,
    inspector::{self, Inspector},
    settings::{GizmoStyle, PANEL_MAX, PANEL_MIN, Settings, ThemeChoice, ToolButton, Units},
    settings_ui,
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
    pub left_tab: usize,
    pub search: String,
    pub camera: Camera,
    /// Panneaux latéraux ouverts (masqués par défaut : la vue occupe tout l'écran).
    pub left_open: bool,
    pub right_open: bool,
    /// Vues de rendu (D-15).
    pub inspector: Inspector,
    /// Fenêtre de conversion.
    pub convert: ConvertState,
    /// Demandes à traiter par la fenêtre de l'application (fichiers, fils de travail).
    pub requests: Vec<Request>,
    /// Fichier en cours de lecture (nom affiché).
    pub loading: Option<String>,
    /// Boîte englobante du modèle (unités du fichier).
    pub scene_bounds: Option<([f32; 3], [f32; 3])>,
    /// Rectangle de la vue 3D après la dernière mise en page (pixels logiques).
    pub viewport: Rc<Cell<Rect>>,
    /// Rectangle du plateau (vue et panneaux) à la dernière image.
    pub stage: Rc<Cell<Rect>>,
    /// Poignée de panneau tenue : côté, abscisse de l'appui, déjà glissée.
    pub grip_press: Option<(u8, f32, bool)>,
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
                    .map(|m| if m.name.is_empty() { "Sans nom".to_owned() } else { m.name.clone() });
                let size =
                    p.mesh.bounds().map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]).unwrap_or([0.0; 3]);
                PartInfo {
                    name: p.name.clone(),
                    triangles: p.mesh.triangle_count(),
                    vertices: p.mesh.positions.len(),
                    size,
                    visible: true,
                    color: model.display_color(i).unwrap_or(crate::frame::STEEL),
                    material,
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
            left_tab: 0,
            search: String::new(),
            camera: Camera::default(),
            inspector: Inspector::Off,
            left_open: false,
            right_open: false,
            scene_bounds: model.bounds(),
            convert: ConvertState::default(),
            requests: Vec::new(),
            loading: None,
            viewport: Rc::new(Cell::new(Rect::ZERO)),
            stage: Rc::new(Cell::new(Rect::ZERO)),
            grip_press: None,
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
        10_000..=999_999 => format!("{} k", (n as f64 / 1000.0).round()),
        _ => format!("{:.1} M", n as f64 / 1e6).replace('.', ","),
    }
}

/// Séparateur des milliers à la française (espace fine insécable).
pub fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(ch);
    }
    out
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

fn topbar(ui: &mut Ui, s: &AppState) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let theme_id = Id::new("thème");
    let logo = Node::stack().size(Size::Px(24.0), Size::Px(24.0)).radius(6.0).bg(c.accent).child(Node::icon(
        "part",
        15.0,
        c.accent_text,
    ));
    let file = Node::row()
        .id(Id::new("fichier"))
        .tooltip_shortcut("Ouvrir un fichier", "Ctrl O")
        .gap(6.0)
        .padding(Edges::xy(8.0, 0.0))
        .height(Size::Px(28.0))
        .radius(6.0)
        .bg(ui.state_color(Id::new("fichier"), Color::TRANSPARENT, c.raised_hover, c.raised_hover))
        .child(match &s.loading {
            Some(_) => spinner(ui, 15.0, c.accent),
            None => Node::icon("file", 15.0, c.text_secondary),
        })
        .child(Node::text(
            s.loading.as_deref().unwrap_or(&s.file_name),
            TextStyle::new(m.font_size, Weight::Medium),
            c.text,
        ))
        .child(Node::icon("chevron-down", 13.0, c.text_tertiary));
    let left = Node::row()
        .gap(10.0)
        .child(logo)
        .child(Node::text("Obview", TextStyle::new(m.font_size, Weight::SemiBold), c.text))
        .child(Node::new().size(Size::Px(1.0), Size::Px(18.0)).bg(c.border))
        .child(file);
    let dark = s.theme_kind() == ThemeKind::Dark;
    let theme_icon = if dark { "sun" } else { "moon" };
    let theme_tip = if dark { "Thème clair" } else { "Thème sombre" };
    let right = Node::row()
        .gap(6.0)
        .child(
            widgets::small_icon_button(ui, theme_id, theme_icon, theme_tip, c.icon)
                .size(Size::Px(28.0), Size::Px(28.0)),
        )
        .child(
            widgets::small_icon_button(ui, Id::new("réglages"), "settings", "Réglages", c.icon)
                .tooltip_shortcut("Réglages", "Ctrl ,")
                .size(Size::Px(28.0), Size::Px(28.0)),
        )
        .child(Node::new().size(Size::Px(4.0), Size::Px(1.0)))
        .child(widgets::button(
            ui,
            Id::new("convertir"),
            "Convertir",
            Some("convert"),
            ButtonKind::Primary,
            "Convertir en maillage optimisé (GLB, OBJ, FBX)",
        ));
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
        list.push(widgets::tree_row(
            ui,
            TreeRow {
                id: Id::new("pièce").with(i),
                depth: 1,
                expanded: None,
                icon: "part",
                label: &p.name,
                detail: Some(&detail),
                selected: s.selected == Some(i),
                visible: p.visible,
            },
        ));
    }
    list.push(Node::new().height(Size::Px(after)));
    let tabs = widgets::tabs(ui, Id::new("onglets"), &["Pièces", "Vues"], s.left_tab);
    let (search, changed) = controls::text_input(
        ui,
        TextInput {
            id: Id::new("recherche"),
            placeholder: "Rechercher",
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
    let header = Node::column()
        .padding(Edges { left: 8.0, right: 8.0, top: 10.0, bottom: 6.0 })
        .gap(8.0)
        .child(tabs)
        .child(search);
    let shown = if s.search.trim().is_empty() {
        format!("{} pièces · {} triangles", group(s.parts.len()), compact(s.total_triangles()))
    } else {
        format!("{} sur {} pièces", group(filtered.len()), group(s.parts.len()))
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

fn properties_panel(ui: &mut Ui, s: &AppState) -> Node {
    let units = s.settings.units;
    let c = ui.theme.color.clone();
    let mut col = widgets::panel(ui, s.settings.right_width).border_left(c.border).padding(Edges::xy(0.0, 6.0));
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
                        Node::text(&p.name, TextStyle::new(13.5, Weight::SemiBold), c.text)
                            .width(Size::Fill(1.0))
                            .truncate(),
                    )
                    .child(widgets::small_icon_button(ui, Id::new("cadrer pièce"), "frame", "Cadrer la pièce", c.icon)),
            );
            col.push(widgets::divider(ui));
            col.push(widgets::section_header(ui, "Géométrie"));
            col.push(widgets::property_row(ui, "Triangles", group(p.triangles)));
            col.push(widgets::property_row(ui, "Sommets", group(p.vertices)));
            col.push(widgets::property_row(ui, "Largeur", length(p.size[0], s.unit_m, units)));
            col.push(widgets::property_row(ui, "Hauteur", length(p.size[1], s.unit_m, units)));
            col.push(widgets::property_row(ui, "Profondeur", length(p.size[2], s.unit_m, units)));
            col.push(Node::new().height(Size::Px(8.0)));
            col.push(widgets::divider(ui));
            col.push(widgets::section_header(ui, "Apparence"));
            let [r, g, b, _] = p.color;
            let hex = format!("#{r:02X}{g:02X}{b:02X}");
            col.push(widgets::color_property(ui, "Couleur", Color::hex(u32::from_be_bytes([0, r, g, b])), &hex));
            col.push(widgets::property_row(ui, "Matériau", p.material.as_deref().unwrap_or("Par défaut")));
        }
        None => {
            col.push(widgets::section_header(ui, "Modèle"));
            col.push(widgets::property_row(ui, "Fichier", &s.file_name));
            col.push(widgets::property_row(ui, "Pièces", group(s.parts.len())));
            col.push(widgets::property_row(ui, "Triangles", group(s.total_triangles())));
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
    }
}

/// Barre du bas : boutons choisis dans les réglages, séparateur entre deux groupes (D-25).
fn toolbar(ui: &mut Ui, s: &AppState) -> Option<Node> {
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
            _ => false,
        };
        items.push(widgets::icon_button(ui, tool_button_id(b), b.icon(), b.label(), b.shortcut(), active));
    }
    (!items.is_empty()).then(|| widgets::docked_toolbar(ui, items))
}

/// Poignée d'un panneau latéral (0 : gauche, 1 : droite) : clic pour afficher ou masquer, glisser pour régler
/// la largeur (200 à 520 px, charte). Posée sur le bord du plateau ou du panneau ouvert.
fn grip(ui: &mut Ui, s: &AppState, side: u8) -> Node {
    let id = grip_id(side);
    let c = ui.theme.color.clone();
    let open = if side == 0 { s.left_open } else { s.right_open };
    let width = if side == 0 { s.settings.left_width } else { s.settings.right_width };
    let edge = if open { width } else { 0.0 };
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
    .tooltip_shortcut(if side == 0 { "Pièces" } else { "Propriétés" }, if side == 0 { "Ctrl [" } else { "Ctrl ]" })
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

fn viewport(ui: &mut Ui, s: &AppState) -> Node {
    let rect_cell = s.viewport.clone();
    let mut root = Node::stack().fill().child(Node::custom(move |_, r| rect_cell.set(r)).fill());
    if let Some(layer) = inspector::overlay(ui, s) {
        root.push(layer);
    }
    let style = s.settings.gizmo;
    if style != GizmoStyle::None {
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
                .tooltip(hover.map(gizmo::name).unwrap_or_else(|| "Orienter la vue".into()))
                .size(Size::Px(side), Size::Px(side))
                .anchor(Anchor { top: Some(12.0), right: Some(12.0), ..Default::default() }),
        );
    }
    root
}

/// Construit l'interface complète pour l'état courant (les commandes y appliquent leurs changements).
///
/// Charte des apps : cadre (`app_bg`) avec la barre du haut, puis un plateau unique aux coins arrondis
/// (marge 6 px, contour `stage_line`) qui contient les panneaux et la vue 3D.
pub fn build(ui: &mut Ui, s: &mut AppState) -> Node {
    let m = ui.theme.size.clone();
    let c = ui.theme.color.clone();
    let (lw, rw) = (s.settings.left_width, s.settings.right_width);
    let body = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
        .align(Align::Stretch)
        .children(s.left_open.then(|| parts_panel(ui, s)))
        .child(viewport(ui, s))
        .children(s.right_open.then(|| properties_panel(ui, s)));
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
    let lane = Node::row()
        .fill()
        .align(Align::Stretch)
        .children(s.left_open.then(|| spacer(lw)))
        .child(Node::stack().fill().children(toolbar(ui, s)))
        .children(s.right_open.then(|| spacer(rw)));
    let overlay = Node::stack().fill().child(lane).child(grip(ui, s, 0)).child(grip(ui, s, 1));
    let stage = Node::stack().fill().child(body).child(frame).child(overlay);
    let root = Node::column().fill().child(topbar(ui, s)).child(
        Node::stack().fill().padding(Edges { left: margin, right: margin, top: 0.0, bottom: margin }).child(stage),
    );
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
    if ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char('\\')) {
        let open = !(s.left_open || s.right_open);
        s.left_open = open;
        s.right_open = open;
    }
    if ui.clicked(Id::new("convertir")) {
        s.convert.open = true;
        if !matches!(s.convert.status, convert_ui::Status::Running(_)) {
            s.convert.status = convert_ui::Status::Idle;
        }
    }
    if ui.clicked(Id::new("fichier")) || (ui.input.modifiers.ctrl && ui.input.keys.contains(&Key::Char('o'))) {
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
    if ui.clicked(Id::new("rendu")) || (plain && ui.input.keys.contains(&Key::Char('r'))) {
        s.inspector = if s.inspector == Inspector::Off { Inspector::Bands } else { Inspector::Off };
    }
    if ui.clicked(Id::new("inspecteur retour")) {
        s.inspector = Inspector::Bands;
    } else if !s.settings_open && ui.escape_pressed() {
        s.inspector = match s.inspector {
            Inspector::Single(_) => Inspector::Bands,
            _ => Inspector::Off,
        };
    }
    if plain {
        for (k, t) in [('v', Tool::Select), ('m', Tool::Measure), ('c', Tool::Section), ('e', Tool::Explode)] {
            if ui.input.keys.contains(&Key::Char(k)) {
                s.tool = t;
            }
        }
    }
    for t in [Tool::Select, Tool::Measure, Tool::Section, Tool::Explode] {
        if ui.clicked(Id::new("outil").with(t as u8)) {
            s.tool = t;
        }
    }
    if ui.clicked(Id::new("racine")) || ui.clicked(Id::new("racine").with("flèche")) {
        s.root_open = !s.root_open;
    }
    for i in 0..2 {
        if ui.clicked(Id::new("onglets").with(i)) {
            s.left_tab = i;
        }
    }
    for i in 0..s.parts.len() {
        if ui.clicked(Id::new("pièce").with(i).with("œil")) {
            s.parts[i].visible = !s.parts[i].visible;
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
