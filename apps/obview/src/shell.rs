//! Coque de l'application : barre du haut, arbre des pièces, vue 3D et ses outils, propriétés.

use std::{cell::Cell, rc::Rc};

use obv_io::Model;
use obv_render::Camera;
use obv_ui::{
    Align, Anchor, Color, Cursor, Edges, Id, Justify, Key, Node, Rect, Size, TextStyle, Theme, ThemeKind, Ui, Weight,
    controls::{self, TextInput},
    widgets::{self, ButtonKind, TreeRow},
};

use crate::{
    settings::{Settings, ThemeChoice, Units},
    settings_ui, viewcube,
};

/// Hauteur d'une ligne de l'arbre (liste virtuelle : hauteur fixe).
pub const TREE_ROW: f32 = 26.0;

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
    /// Rectangle de la vue 3D après la dernière mise en page (pixels logiques).
    pub viewport: Rc<Cell<Rect>>,
}

impl AppState {
    pub fn new(file_name: &str, model: &Model) -> Self {
        let parts = model
            .parts
            .iter()
            .map(|p| {
                let size =
                    p.mesh.bounds().map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]).unwrap_or([0.0; 3]);
                PartInfo {
                    name: p.name.clone(),
                    triangles: p.mesh.triangle_count(),
                    vertices: p.mesh.positions.len(),
                    size,
                    visible: true,
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
            viewport: Rc::new(Cell::new(Rect::ZERO)),
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

fn topbar(ui: &mut Ui, s: &AppState) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let theme_id = Id::new("thème");
    let logo = Node::stack()
        .size(Size::Px(24.0), Size::Px(24.0))
        .radius(7.0)
        .bg(obv_ui::Fill::Vertical(c.accent_hover, c.accent))
        .child(Node::icon("box", 15.0, Color::WHITE));
    let file = Node::row()
        .id(Id::new("fichier"))
        .tooltip("Ouvrir un fichier")
        .gap(6.0)
        .padding(Edges::xy(8.0, 0.0))
        .height(Size::Px(28.0))
        .radius(6.0)
        .bg(ui.state_color(Id::new("fichier"), Color::TRANSPARENT, c.raised_hover, c.raised_hover))
        .child(Node::icon("file-box", 15.0, c.text_secondary))
        .child(Node::text(&s.file_name, TextStyle::new(m.font_size, Weight::Medium), c.text))
        .child(Node::icon("chevron-down", 13.0, c.text_tertiary));
    let left = Node::row()
        .gap(10.0)
        .child(logo)
        .child(Node::text("Obview", TextStyle::new(13.5, Weight::SemiBold), c.text))
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
            Some("wand-sparkles"),
            ButtonKind::Primary,
            "Convertir en maillage optimisé (GLB, OBJ, FBX)",
        ));
    Node::row()
        .height(Size::Px(m.topbar_height))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(12.0, 0.0))
        .justify(Justify::SpaceBetween)
        .bg(c.panel)
        .border_bottom(c.border)
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
                    icon: "boxes",
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
                icon: "box",
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
    widgets::panel(ui, 268.0).border_right(c.border).child(header).child(list).child(widgets::divider(ui)).child(footer)
}

fn properties_panel(ui: &mut Ui, s: &AppState) -> Node {
    let units = s.settings.units;
    let c = ui.theme.color.clone();
    let mut col = widgets::panel(ui, 288.0).border_left(c.border).padding(Edges::xy(0.0, 6.0));
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
                            .child(Node::icon("box", 15.0, c.accent)),
                    )
                    .child(
                        Node::text(&p.name, TextStyle::new(13.5, Weight::SemiBold), c.text)
                            .width(Size::Fill(1.0))
                            .truncate(),
                    )
                    .child(widgets::small_icon_button(ui, Id::new("cadrer pièce"), "focus", "Cadrer la pièce", c.icon)),
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
            col.push(widgets::color_property(ui, "Couleur", Color::hex(0xc9ccd1), "#C9CCD1"));
            col.push(widgets::property_row(ui, "Matériau", "Métal satiné"));
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

fn viewport(ui: &mut Ui, s: &AppState) -> Node {
    let tool = |ui: &mut Ui, t: Tool, icon: &'static str, tip: &str, key: &str| {
        widgets::icon_button(ui, Id::new("outil").with(t as u8), icon, tip, Some(key), s.tool == t)
    };
    let items = vec![
        tool(ui, Tool::Select, "mouse-pointer-2", "Sélection", "V"),
        tool(ui, Tool::Measure, "ruler", "Mesurer", "M"),
        tool(ui, Tool::Section, "scissors", "Coupe", "C"),
        tool(ui, Tool::Explode, "ungroup", "Vue éclatée", "E"),
        widgets::toolbar_separator(ui),
        widgets::icon_button(ui, Id::new("rendu"), "blend", "Mode de rendu", Some("R"), false),
        widgets::icon_button(ui, Id::new("projection"), "view", "Perspective ou orthographique", Some("P"), false),
        widgets::icon_button(ui, Id::new("capture"), "camera", "Capture", None, false),
        widgets::toolbar_separator(ui),
        widgets::icon_button(ui, Id::new("cadrer"), "focus", "Cadrer tout", Some("F"), false),
    ];
    let toolbar = widgets::floating_toolbar(ui, items);
    let rect_cell = s.viewport.clone();
    let (yaw, pitch) = (s.camera.yaw as f32, s.camera.pitch as f32);
    let theme = ui.theme.clone();
    let mut root = Node::stack().fill().child(Node::custom(move |_, r| rect_cell.set(r)).fill()).child(toolbar);
    if s.settings.view_cube {
        root.push(
            Node::custom(move |p, r| viewcube::paint(p, r, yaw, pitch, &theme))
                .id(Id::new("cube de vue"))
                .cursor(Cursor::Pointer)
                .tooltip("Orienter la vue")
                .size(Size::Px(104.0), Size::Px(104.0))
                .anchor(Anchor { top: Some(12.0), right: Some(12.0), ..Default::default() }),
        );
    }
    root
}

/// Construit l'interface complète pour l'état courant (les commandes y appliquent leurs changements).
pub fn build(ui: &mut Ui, s: &mut AppState) -> Node {
    let body = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
        .align(Align::Stretch)
        .child(parts_panel(ui, s))
        .child(viewport(ui, s))
        .child(properties_panel(ui, s));
    let root = Node::column().fill().child(topbar(ui, s)).child(body);
    settings_ui::dialog(ui, s);
    root
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
    let ctrl = ui.input.modifiers.ctrl;
    if ctrl && ui.input.keys.contains(&Key::Char(',')) {
        s.settings_open = !s.settings_open;
    }
    if !s.settings_open && ctrl && ui.input.keys.contains(&Key::Char('f')) {
        ui.focus(Id::new("recherche"));
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

/// Thème correspondant à l'état.
pub fn theme(s: &AppState) -> Theme {
    s.settings.theme(s.system_dark)
}
