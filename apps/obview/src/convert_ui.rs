//! Fenêtre « Convertir » : format, écart maximal garanti, progression, bilan. Le travail lui-même tourne dans
//! un fil séparé lancé par la fenêtre de l'application (voir `app.rs`).

use obv_mesh::Format;
use obv_ui::{
    Align, Edges, Id, Justify, Layer, Node, Size, TextStyle, Ui, Weight,
    controls::{self, TextInput},
    widgets::{self, ButtonKind},
};

use crate::shell::{AppState, compact, group};

/// Écarts proposés, en millimètres.
pub const PRESETS: [(f64, &str, &str); 4] = [
    (0.0, "Exact", "Géométrie gardée : soudure et réordonnancement seulement"),
    (0.005, "Fin", "Écart maximal 0,005 mm"),
    (0.02, "Standard", "Écart maximal 0,02 mm"),
    (0.1, "Léger", "Écart maximal 0,1 mm"),
];

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Running(f32),
    Done { triangles_in: usize, triangles_out: usize, deviation_mm: f64, bytes: usize, path: std::path::PathBuf },
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ConvertState {
    pub open: bool,
    pub format: Format,
    /// Écart maximal en millimètres.
    pub tolerance_mm: f64,
    pub tolerance_text: String,
    pub status: Status,
}

impl Default for ConvertState {
    fn default() -> Self {
        ConvertState {
            open: false,
            format: Format::Glb,
            tolerance_mm: 0.02,
            tolerance_text: "0,02".into(),
            status: Status::Idle,
        }
    }
}

/// Demandes adressées à la fenêtre de l'application (qui possède le modèle et les fils de travail).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Choisir un fichier à ouvrir.
    OpenFile,
    /// Choisir le fichier de sortie puis convertir.
    Convert,
    /// Ouvrir le fichier produit, pour le comparer.
    OpenResult(std::path::PathBuf),
    /// Montrer le fichier produit dans l'explorateur.
    Reveal(std::path::PathBuf),
}

fn id(n: &str) -> Id {
    Id::new("convertir").with(n)
}

fn fmt_mm(v: f64) -> String {
    let s = format!("{v}");
    s.replace('.', ",")
}

/// Taille lisible : « 25 Ko », « 4,9 Mo ».
pub fn file_size(bytes: usize) -> String {
    let b = bytes as f64;
    if b < 1e6 { format!("{:.0} Ko", (b / 1e3).max(1.0)) } else { format!("{:.1} Mo", b / 1e6).replace('.', ",") }
}

fn row(ui: &Ui, label: &str, control: Node) -> Node {
    Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(52.0))
        .justify(Justify::SpaceBetween)
        .child(Node::text(label, TextStyle::new(13.0, Weight::Regular), ui.theme.color.text))
        .child(control)
}

fn stat(ui: &Ui, label: &str, value: String, accent: bool) -> Node {
    let c = &ui.theme.color;
    Node::column()
        .width(Size::Fill(1.0))
        .gap(4.0)
        .padding(Edges::all(12.0))
        .radius(10.0)
        .bg(c.raised.mul_alpha(0.6))
        .border(1.0, c.border)
        .child(Node::text(label, TextStyle::new(11.5, Weight::Medium), c.text_secondary))
        .child(Node::text(value, TextStyle::new(17.0, Weight::SemiBold), if accent { c.accent } else { c.text }))
}

/// Ajoute la fenêtre en calque modal si elle est ouverte.
pub fn dialog(ui: &mut Ui, s: &mut AppState) {
    if !s.convert.open {
        return;
    }
    let c = ui.theme.color.clone();
    let running = matches!(s.convert.status, Status::Running(_));
    let mut body = Node::column().padding(Edges { left: 22.0, right: 22.0, top: 8.0, bottom: 16.0 }).gap(0.0);

    // Format.
    let fmt_base = id("format");
    for (i, f) in Format::ALL.iter().enumerate() {
        if !running && ui.clicked(fmt_base.with(i)) {
            s.convert.format = *f;
            s.convert.status = Status::Idle;
        }
    }
    let fi = Format::ALL.iter().position(|f| *f == s.convert.format).unwrap_or(0);
    let labels: Vec<&str> = Format::ALL.iter().map(|f| f.label()).collect();
    let tabs_fmt = widgets::tabs(ui, fmt_base, &labels, fi).width(Size::Px(240.0));
    body.push(row(ui, "Format", tabs_fmt));
    body.push(widgets::divider(ui));

    // Écart : préréglages et valeur libre.
    let pre_base = id("préréglage");
    for (i, (v, _, _)) in PRESETS.iter().enumerate() {
        if !running && ui.clicked(pre_base.with(i)) {
            s.convert.tolerance_mm = *v;
            s.convert.tolerance_text = fmt_mm(*v);
            s.convert.status = Status::Idle;
        }
    }
    let current = PRESETS.iter().position(|(v, _, _)| (*v - s.convert.tolerance_mm).abs() < 1e-12);
    let labels: Vec<&str> = PRESETS.iter().map(|p| p.1).collect();
    let mut tabs = widgets::tabs(ui, pre_base, &labels, current.unwrap_or(usize::MAX)).width(Size::Px(320.0));
    for (i, child) in tabs.children.iter_mut().enumerate() {
        child.tooltip = Some(obv_ui::Tooltip { text: PRESETS[i].2.into(), shortcut: None });
    }
    body.push(row(ui, "Précision", tabs));
    let (input, changed) = controls::text_input(
        ui,
        TextInput {
            id: id("écart"),
            placeholder: "0",
            width: Size::Px(96.0),
            icon: None,
            hint: None,
            clearable: false,
        },
        &mut s.convert.tolerance_text,
    );
    if changed
        && let Ok(v) = s.convert.tolerance_text.replace(',', ".").trim().parse::<f64>()
        && v >= 0.0
        && v.is_finite()
    {
        s.convert.tolerance_mm = v;
        s.convert.status = Status::Idle;
    }
    body.push(row(
        ui,
        "Écart maximal",
        Node::row().gap(8.0).child(input).child(Node::text(
            "mm",
            TextStyle::new(13.0, Weight::Regular),
            c.text_secondary,
        )),
    ));
    body.push(widgets::divider(ui));
    body.push(Node::new().height(Size::Px(14.0)));

    // Bilan : avant la conversion, le modèle d'entrée ; après, le résultat.
    let tri_in = s.total_triangles();
    match &s.convert.status {
        Status::Done { triangles_in, triangles_out, deviation_mm, bytes, .. } => {
            let ratio = *triangles_out as f64 * 100.0 / (*triangles_in).max(1) as f64;
            body.push(
                Node::row()
                    .width(Size::Fill(1.0))
                    .gap(10.0)
                    .child(stat(ui, "Triangles", compact(*triangles_out), true))
                    .child(stat(ui, "Réduction", format!("{:.0} %", 100.0 - ratio).replace('.', ","), false))
                    .child(stat(ui, "Écart mesuré", format!("{deviation_mm:.4} mm").replace('.', ","), false))
                    .child(stat(ui, "Taille", file_size(*bytes), false)),
            );
        }
        Status::Failed(e) => {
            body.push(
                Node::row()
                    .width(Size::Fill(1.0))
                    .gap(8.0)
                    .padding(Edges::all(12.0))
                    .radius(10.0)
                    .bg(c.danger.with_alpha(0.12))
                    .child(Node::icon("circle-alert", 15.0, c.danger))
                    .child(
                        Node::text(e.clone(), TextStyle::new(12.5, Weight::Medium), c.danger)
                            .width(Size::Fill(1.0))
                            .truncate(),
                    ),
            );
        }
        _ => {
            body.push(
                Node::row()
                    .width(Size::Fill(1.0))
                    .gap(10.0)
                    .child(stat(ui, "Pièces", group(s.parts.len()), false))
                    .child(stat(ui, "Triangles", compact(tri_in), false))
                    .child(stat(ui, "Écart garanti", format!("{} mm", fmt_mm(s.convert.tolerance_mm)), true)),
            );
        }
    }
    if let Status::Running(p) = s.convert.status {
        let accent = c.accent;
        let track = c.track;
        let shown = ui.anim(id("progression"), p, 120.0);
        body.push(Node::new().height(Size::Px(16.0)));
        body.push(
            Node::custom(move |pt, r| {
                pt.dl.rect(r, 3.0, track, None);
                let w = (r.width() * shown.clamp(0.0, 1.0)).max(6.0);
                pt.dl.rect(obv_ui::Rect::from_xywh(r.min.x, r.min.y, w, r.height()), 3.0, accent, None);
            })
            .width(Size::Fill(1.0))
            .height(Size::Px(6.0)),
        );
    }

    // Pied : actions.
    let mut footer = Vec::new();
    if let Status::Done { path, .. } = &s.convert.status {
        let path = path.clone();
        if ui.clicked(id("ouvrir")) {
            s.requests.push(Request::OpenResult(path.clone()));
            s.convert.open = false;
        }
        if ui.clicked(id("dossier")) {
            s.requests.push(Request::Reveal(path.clone()));
        }
        footer.push(widgets::button(
            ui,
            id("dossier"),
            "Dossier",
            Some("folder-open"),
            ButtonKind::Secondary,
            "Montrer le fichier",
        ));
        footer.push(Node::spacer());
        footer.push(widgets::button(
            ui,
            id("ouvrir"),
            "Ouvrir le résultat",
            Some("file-box"),
            ButtonKind::Primary,
            "Ouvrir le fichier produit pour le comparer",
        ));
    } else {
        footer.push(Node::spacer());
        if ui.clicked(id("annuler")) && !running {
            s.convert.open = false;
        }
        footer.push(widgets::button(ui, id("annuler"), "Annuler", None, ButtonKind::Secondary, "Fermer"));
        if ui.clicked(id("lancer")) && !running {
            s.requests.push(Request::Convert);
        }
        let label = if running { "Conversion…" } else { "Convertir" };
        let mut b = widgets::button(
            ui,
            id("lancer"),
            label,
            Some("wand-sparkles"),
            ButtonKind::Primary,
            "Choisir le fichier de sortie et convertir",
        );
        if running {
            b = b.opacity(0.6);
        }
        footer.push(b);
    }
    let body = Node::column().align(Align::Stretch).child(body);
    let (node, close) = controls::modal(ui, Id::new("convertir"), "Convertir", (620.0, 396.0), body, footer);
    if close && !running {
        s.convert.open = false;
    }
    ui.overlay(node, Layer::Modal);
}
