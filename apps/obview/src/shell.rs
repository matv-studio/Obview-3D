//! Coque de l'application : barre du haut, arbre des pièces, vue 3D et ses outils, propriétés.

use std::{cell::Cell, rc::Rc};

use obv_io::Model;
use obv_render::Camera;
use obv_ui::{
    Align, Anchor, Color, Edges, Id, Justify, Node, Rect, Size, TextStyle, Theme, ThemeKind, Ui, Weight,
    widgets::{self, ButtonKind, TreeRow},
};

use crate::viewcube;

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
    pub theme: ThemeKind,
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
            theme: ThemeKind::Dark,
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

fn length(v: f32, unit_m: f64) -> String {
    let mm = v as f64 * unit_m * 1000.0;
    if mm >= 1000.0 {
        format!("{:.3} m", mm / 1000.0).replace('.', ",")
    } else {
        format!("{mm:.2} mm").replace('.', ",")
    }
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
    let theme_icon = if s.theme == ThemeKind::Dark { "sun" } else { "moon" };
    let theme_tip = if s.theme == ThemeKind::Dark { "Thème clair" } else { "Thème sombre" };
    let right = Node::row()
        .gap(6.0)
        .child(
            widgets::small_icon_button(ui, theme_id, theme_icon, theme_tip, c.icon)
                .size(Size::Px(28.0), Size::Px(28.0)),
        )
        .child(
            widgets::small_icon_button(ui, Id::new("réglages"), "settings", "Réglages", c.icon)
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

fn parts_panel(ui: &mut Ui, s: &AppState) -> Node {
    let c = ui.theme.color.clone();
    let mut list =
        Node::column().width(Size::Fill(1.0)).height(Size::Fill(1.0)).gap(1.0).padding(Edges::xy(6.0, 4.0)).clip();
    let root_id = Id::new("racine");
    list.push(widgets::tree_row(
        ui,
        TreeRow {
            id: root_id,
            depth: 0,
            expanded: Some(s.root_open),
            icon: "boxes",
            label: &s.file_name,
            detail: Some(&s.parts.len().to_string()),
            selected: false,
            visible: true,
        },
    ));
    if s.root_open {
        let filter = s.search.to_lowercase();
        for (i, p) in s.parts.iter().enumerate() {
            if !filter.is_empty() && !p.name.to_lowercase().contains(&filter) {
                continue;
            }
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
    }
    let header = Node::column()
        .padding(Edges { left: 8.0, right: 8.0, top: 10.0, bottom: 6.0 })
        .gap(8.0)
        .child(widgets::tabs(ui, Id::new("onglets"), &["Pièces", "Vues"], s.left_tab))
        .child(widgets::search_field(ui, Id::new("recherche"), "Rechercher", &s.search, "Ctrl F"));
    let footer = Node::row()
        .height(Size::Px(32.0))
        .padding(Edges::xy(14.0, 0.0))
        .border_bottom(Color::TRANSPARENT)
        .gap(6.0)
        .child(Node::icon("layers", 13.0, c.text_tertiary))
        .child(Node::text(
            format!("{} pièces · {} triangles", group(s.parts.len()), compact(s.total_triangles())),
            TextStyle::new(11.5, Weight::Regular),
            c.text_tertiary,
        ));
    widgets::panel(ui, 268.0).border_right(c.border).child(header).child(list).child(widgets::divider(ui)).child(footer)
}

fn properties_panel(ui: &mut Ui, s: &AppState) -> Node {
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
            col.push(widgets::property_row(ui, "Largeur", length(p.size[0], s.unit_m)));
            col.push(widgets::property_row(ui, "Hauteur", length(p.size[1], s.unit_m)));
            col.push(widgets::property_row(ui, "Profondeur", length(p.size[2], s.unit_m)));
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
    let cube = Node::custom(move |p, r| viewcube::paint(p, r, yaw, pitch, &theme))
        .id(Id::new("cube de vue"))
        .tooltip("Orienter la vue")
        .size(Size::Px(104.0), Size::Px(104.0))
        .anchor(Anchor { top: Some(12.0), right: Some(12.0), ..Default::default() });
    Node::stack().fill().child(Node::custom(move |_, r| rect_cell.set(r)).fill()).child(toolbar).child(cube)
}

/// Construit l'interface complète pour l'état courant.
pub fn build(ui: &mut Ui, s: &AppState) -> Node {
    let body = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
        .align(Align::Stretch)
        .child(parts_panel(ui, s))
        .child(viewport(ui, s))
        .child(properties_panel(ui, s));
    Node::column().fill().child(topbar(ui, s)).child(body)
}

/// Applique les clics de l'image.
pub fn handle(ui: &Ui, s: &mut AppState) {
    if ui.clicked(Id::new("thème")) {
        s.theme = if s.theme == ThemeKind::Dark { ThemeKind::Light } else { ThemeKind::Dark };
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
    Theme::new(s.theme)
}
