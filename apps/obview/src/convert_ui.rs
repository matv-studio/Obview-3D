//! Fenêtre « Convertir » : format, écart maximal garanti, progression, bilan. Le travail lui-même tourne dans
//! un fil séparé lancé par la fenêtre de l'application (voir `app.rs`).

use obv_mesh::{Compression, Format};
use obv_ui::{
    Align, Edges, Id, Justify, Layer, Node, Size, TextStyle, Ui, Weight,
    controls::{self, TextInput},
    i18n, tr,
    widgets::{self, ButtonKind},
};

use crate::shell::{AppState, compact, group};

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
    /// Curseur fidélité (0, géométrie exacte) ↔ performance (1).
    pub density: f64,
    /// Écart maximal en millimètres (déduit du curseur, ou saisi).
    pub tolerance_mm: f64,
    pub tolerance_text: String,
    pub keep_silhouette: bool,
    /// GLB : compression des géométries (D-29).
    pub compression: Compression,
    /// Arbre d'assemblage ou pièces à plat (D-35).
    pub layout: obv_mesh::Layout,
    /// CAO : angle maximal entre facettes voisines du nouveau maillage, en degrés (D-38).
    pub max_angle_deg: f64,
    pub max_angle_text: String,
    /// Pièces exportées (D-38).
    pub scope: PartScope,
    /// Pièces plus petites (diagonale, millimètres) retirées ; 0 : toutes gardées.
    pub min_size_mm: f64,
    pub min_size_text: String,
    /// Pièces invisibles de l'extérieur retirées.
    pub remove_hidden: bool,
    /// L'écart a été initialisé pour le modèle ouvert.
    pub initialized: bool,
    pub status: Status,
}

impl Default for ConvertState {
    fn default() -> Self {
        ConvertState {
            open: false,
            format: Format::Glb,
            density: obv_mesh::DEFAULT_DENSITY,
            tolerance_mm: 0.0,
            tolerance_text: String::new(),
            keep_silhouette: true,
            compression: Compression::Quantize,
            layout: obv_mesh::Layout::Assembly,
            max_angle_deg: DEFAULT_ANGLE_DEG,
            max_angle_text: fmt_mm(DEFAULT_ANGLE_DEG),
            scope: PartScope::Visible,
            min_size_mm: 0.0,
            min_size_text: "0".into(),
            remove_hidden: true,
            initialized: false,
            status: Status::Idle,
        }
    }
}

/// Angle maximal par défaut du maillage de la CAO, en degrés (celui de la lecture, `obv_cad::Options`).
pub const DEFAULT_ANGLE_DEG: f64 = 20.0;

/// Pièces exportées (D-38).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartScope {
    All,
    Visible,
    Selection,
}

impl PartScope {
    pub const ALL: [PartScope; 3] = [PartScope::All, PartScope::Visible, PartScope::Selection];
}

impl ConvertState {
    /// Pièces de départ selon le choix « Pièces » (avant les filtres de taille et de visibilité).
    pub fn base_parts(&self, s_parts: &[crate::shell::PartInfo], selected: Option<usize>) -> Vec<bool> {
        match (self.scope, selected) {
            (PartScope::All, _) => vec![true; s_parts.len()],
            (PartScope::Selection, Some(k)) => (0..s_parts.len()).map(|i| i == k).collect(),
            _ => s_parts.iter().map(|p| p.visible).collect(),
        }
    }

    /// Filtres de taille et de visibilité.
    pub fn filter(&self) -> obv_mesh::PartFilter {
        obv_mesh::PartFilter { min_size_m: self.min_size_mm / 1000.0, remove_hidden: self.remove_hidden }
    }
}

/// Le fichier ouvert est-il de la CAO (STEP, IGES), donc remaillable à l'export ?
pub fn is_cad(file_name: &str) -> bool {
    let ext = std::path::Path::new(file_name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    obv_cad::EXTENSIONS.contains(&ext.as_str())
}

/// Demandes adressées à la fenêtre de l'application (qui possède le modèle et les fils de travail).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Choisir un fichier à ouvrir.
    OpenFile,
    /// Choisir un HDRI d'éclairage (.hdr, .exr, D-33).
    LoadHdri,
    /// Ouvrir ce fichier (liste des récents).
    OpenPath(std::path::PathBuf),
    /// Abandonner la lecture en cours.
    CancelOpen,
    /// Choisir le fichier de sortie puis convertir.
    Convert,
    /// Ouvrir le fichier produit, pour le comparer.
    OpenResult(std::path::PathBuf),
    /// Montrer le fichier produit dans l'explorateur.
    Reveal(std::path::PathBuf),
    /// Enregistrer une image de la vue (D-26).
    Capture { size: crate::tools::CaptureSize, transparent: bool },
}

fn id(n: &str) -> Id {
    Id::new("convertir").with(n)
}

/// Millimètres avec trois chiffres significatifs, séparateur décimal de la langue.
fn fmt_mm(v: f64) -> String {
    if v <= 0.0 {
        return "0".into();
    }
    let digits = (2 - v.log10().floor() as i32).clamp(0, 6) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if i18n::is_fr() { s.replace('.', ",") } else { s }
}

/// Diagonale du modèle en millimètres.
fn diag_mm(s: &AppState) -> f64 {
    s.scene_bounds
        .map(|(lo, hi)| (0..3).map(|i| ((hi[i] - lo[i]) as f64).powi(2)).sum::<f64>().sqrt() * s.unit_m * 1000.0)
        .unwrap_or(0.0)
}

/// Taille lisible : « 25 Ko », « 4,9 Mo » (« 25 KB », « 4.9 MB »).
pub fn file_size(bytes: usize) -> String {
    let b = bytes as f64;
    if b < 1e6 {
        format!("{:.0} {}", (b / 1e3).max(1.0), tr("Ko"))
    } else {
        format!("{} {}", i18n::decimal(b / 1e6, 1), tr("Mo"))
    }
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
        .radius(8.0)
        .bg(c.raised)
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
    let mut tabs_fmt = widgets::tabs(ui, fmt_base, &labels, fi).width(Size::Px(240.0));
    // Info-bulle de chaque choix, le choix conseillé entre parenthèses.
    let fmt_tip = |f: Format| match f {
        Format::Glb => {
            "Un seul fichier avec matériaux, textures et arborescence ; lu par Blender, les moteurs de jeu et le web (conseillé)"
        }
        Format::Obj => "Format ancien lu partout ; ni arborescence ni repères, matériaux simples dans un .mtl à côté",
        Format::Fbx => {
            "Pour Maya, 3ds Max, Cinema 4D, Unity, Unreal : arborescence et repères gardés, matériaux simples"
        }
    };
    for (child, f) in tabs_fmt.children.iter_mut().zip(Format::ALL.iter()) {
        child.tooltip = Some(obv_ui::Tooltip { text: tr(fmt_tip(*f)).into(), shortcut: None });
    }
    body.push(row(ui, tr("Format"), tabs_fmt));
    body.push(widgets::divider(ui));

    // Fidélité ↔ performance : curseur, écart en millimètres (lié au curseur), silhouette.
    let diag = diag_mm(s);
    if !s.convert.initialized {
        s.convert.tolerance_mm = obv_mesh::ConvertOptions::tolerance_for_density(s.convert.density, diag);
        s.convert.tolerance_text = fmt_mm(s.convert.tolerance_mm);
        s.convert.initialized = true;
    }
    let (slider, moved) = controls::slider(ui, id("densité"), s.convert.density as f32, 0.0, 1.0, 0.0, 240.0);
    if let Some(v) = moved
        && !running
    {
        s.convert.density = v as f64;
        s.convert.tolerance_mm = obv_mesh::ConvertOptions::tolerance_for_density(v as f64, diag);
        s.convert.tolerance_text = fmt_mm(s.convert.tolerance_mm);
        s.convert.status = Status::Idle;
    }
    let small = TextStyle::new(11.5, Weight::Medium);
    let density_ctl = Node::row()
        .gap(10.0)
        .child(
            Node::text(tr("Fidélité"), small, c.text_secondary)
                .id(id("fidélité"))
                .tooltip(tr("Forme exacte, fichier plus lourd")),
        )
        .child(slider.tooltip(tr(
            "Gauche : la forme d'abord. Droite : le moins de triangles possible (conseillé : position proposée)",
        )))
        .child(
            Node::text(tr("Performance"), small, c.text_secondary)
                .id(id("performance"))
                .tooltip(tr("Fichier léger, forme approchée")),
        );
    body.push(row(ui, tr("Densité"), density_ctl));
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
        s.convert.density = obv_mesh::ConvertOptions::density_for_tolerance(v, diag);
        s.convert.status = Status::Idle;
    }
    body.push(row(
        ui,
        tr("Écart maximal"),
        Node::row()
            .gap(8.0)
            .child(input.tooltip(tr(
                "Distance maximale garantie entre la forme d'origine et le résultat (conseillé : la valeur du curseur)",
            )))
            .child(Node::text("mm", TextStyle::new(13.0, Weight::Regular), c.text_secondary)),
    ));
    // CAO : nouveau maillage du modèle exact à l'export (D-38).
    let cad = is_cad(&s.file_name);
    if cad {
        let (angle, changed) = controls::text_input(
            ui,
            TextInput {
                id: id("angle"),
                placeholder: "20",
                width: Size::Px(96.0),
                icon: None,
                hint: None,
                clearable: false,
            },
            &mut s.convert.max_angle_text,
        );
        if changed
            && let Ok(v) = s.convert.max_angle_text.replace(',', ".").trim().parse::<f64>()
            && (1.0..=90.0).contains(&v)
        {
            s.convert.max_angle_deg = v;
            s.convert.status = Status::Idle;
        }
        body.push(row(
            ui,
            tr("Angle maximal"),
            Node::row()
                .gap(8.0)
                .child(angle.tooltip(tr(
                    "Angle maximal entre deux facettes voisines, le modèle exact étant remaillé : plus petit, courbes plus lisses et plus de triangles (conseillé : 20°)",
                )))
                .child(Node::text("°", TextStyle::new(13.0, Weight::Regular), c.text_secondary).width(Size::Px(22.0))),
        ));
    }
    let (sil, toggled) = controls::toggle(ui, id("silhouette"), s.convert.keep_silhouette);
    if let Some(v) = toggled
        && !running
    {
        s.convert.keep_silhouette = v;
        s.convert.status = Status::Idle;
    }
    body.push(row(
        ui,
        tr("Silhouette"),
        sil.tooltip(tr("Arêtes vives et contours plus fidèles, au prix de quelques triangles (conseillé : activé)")),
    ));
    body.push(widgets::divider(ui));

    // Structure et choix des pièces (D-35, D-38).
    let labels = [tr("Assemblage"), tr("À plat")];
    let layouts = [obv_mesh::Layout::Assembly, obv_mesh::Layout::Flat];
    let cur = layouts.iter().position(|l| *l == s.convert.layout).unwrap_or(0);
    let (mut structure, picked) = crate::tools::segmented(ui, id("structure"), &labels, cur);
    if let Some(i) = picked
        && !running
    {
        s.convert.layout = layouts[i];
        s.convert.status = Status::Idle;
    }
    let tips = [
        "Arborescence, repères et pièces répétées partagées gardés : fichier plus léger, ouvert par sous-ensembles (conseillé)",
        "Une pièce par objet, déjà placée, sans arborescence",
    ];
    for (child, t) in structure.children.iter_mut().zip(tips) {
        child.tooltip = Some(obv_ui::Tooltip { text: tr(t).into(), shortcut: None });
    }
    body.push(row(ui, tr("Structure"), structure.width(Size::Px(190.0))));
    let labels = [tr("Toutes"), tr("Visibles"), tr("Sélectionnée")];
    let cur = PartScope::ALL.iter().position(|p| *p == s.convert.scope).unwrap_or(1);
    let (mut scope, picked) = crate::tools::segmented(ui, id("pièces"), &labels, cur);
    if let Some(i) = picked
        && !running
        && (PartScope::ALL[i] != PartScope::Selection || s.selected.is_some())
    {
        s.convert.scope = PartScope::ALL[i];
        s.convert.status = Status::Idle;
    }
    if s.convert.scope == PartScope::Selection && s.selected.is_none() {
        s.convert.scope = PartScope::Visible;
    }
    let tips = [
        "Toutes les pièces, masquées comprises",
        "Pièces affichées seulement (conseillé)",
        "Pièce sélectionnée seulement",
    ];
    for (k, (child, t)) in scope.children.iter_mut().zip(tips).enumerate() {
        child.tooltip = Some(obv_ui::Tooltip { text: tr(t).into(), shortcut: None });
        if k == 2 && s.selected.is_none() {
            child.style.opacity = 0.4;
        }
    }
    body.push(row(ui, tr("Pièces"), scope.width(Size::Px(240.0))));
    let (min_input, changed) = controls::text_input(
        ui,
        TextInput {
            id: id("taille min"),
            placeholder: "0",
            width: Size::Px(96.0),
            icon: None,
            hint: None,
            clearable: false,
        },
        &mut s.convert.min_size_text,
    );
    if changed
        && let Ok(v) = s.convert.min_size_text.replace(',', ".").trim().parse::<f64>()
        && v >= 0.0
        && v.is_finite()
    {
        s.convert.min_size_mm = v;
        s.convert.status = Status::Idle;
    }
    body.push(row(
        ui,
        tr("Ignorer les pièces sous"),
        Node::row()
            .gap(8.0)
            .child(min_input.tooltip(tr(
                "Pièces plus petites que cette taille (diagonale) retirées : vis, ressorts, détails (conseillé : 0, tout garder)",
            )))
            .child(Node::text("mm", TextStyle::new(13.0, Weight::Regular), c.text_secondary)),
    ));
    let (hidden, toggled) = controls::toggle(ui, id("cachées"), s.convert.remove_hidden);
    if let Some(v) = toggled
        && !running
    {
        s.convert.remove_hidden = v;
        s.convert.status = Status::Idle;
    }
    body.push(row(
        ui,
        tr("Retirer les pièces cachées"),
        hidden.tooltip(tr(
            "Pièces invisibles de l'extérieur sous tous les angles retirées, mécanismes internes compris (conseillé : activé)",
        )),
    ));
    if s.convert.format == Format::Glb {
        // Choix proposés par cette construction (Draco seulement si la bibliothèque est liée).
        let options: Vec<Compression> = Compression::ALL.into_iter().filter(|c| c.available()).collect();
        let label = |c: Compression| match c {
            Compression::None => tr("Aucune"),
            Compression::Quantize => tr("Quantifiée"),
            Compression::Meshopt => "Meshopt",
            Compression::Draco => "Draco",
        };
        let tip = |c: Compression| match c {
            Compression::None => "Flottants : lisible partout, fichier le plus lourd",
            Compression::Quantize => {
                "KHR_mesh_quantization : environ deux fois plus léger ; Blender, Unity, Unreal, three.js, Babylon.js (conseillé)"
            }
            Compression::Meshopt => {
                "EXT_meshopt_compression : très léger, le plus rapide à décoder ; Blender ne le lit pas (conseillé pour le web)"
            }
            Compression::Draco => {
                "KHR_draco_mesh_compression : le plus compact ; Blender, three.js, Unity (glTFast), Babylon.js ; décodage plus lent"
            }
        };
        let labels: Vec<&str> = options.iter().map(|c| label(*c)).collect();
        let cur = options.iter().position(|c| *c == s.convert.compression).unwrap_or(1);
        let (mut comp, picked) = crate::tools::segmented(ui, id("compression"), &labels, cur);
        if let Some(i) = picked
            && !running
        {
            s.convert.compression = options[i];
            s.convert.status = Status::Idle;
        }
        for (child, c) in comp.children.iter_mut().zip(&options) {
            child.tooltip = Some(obv_ui::Tooltip { text: tr(tip(*c)).into(), shortcut: None });
        }
        body.push(row(ui, tr("Compression"), comp.width(Size::Px(84.0 * options.len() as f32))));
    }
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
                    .child(stat(ui, tr("Triangles"), compact(*triangles_out), true))
                    .child(stat(ui, tr("Réduction"), i18n::with_unit(&format!("{:.0}", 100.0 - ratio), "%"), false))
                    .child(stat(ui, tr("Écart mesuré"), format!("{} mm", fmt_mm(*deviation_mm)), false))
                    .child(stat(ui, tr("Taille"), file_size(*bytes), false)),
            );
        }
        Status::Failed(e) => {
            body.push(
                Node::row()
                    .width(Size::Fill(1.0))
                    .gap(8.0)
                    .padding(Edges::all(12.0))
                    .radius(8.0)
                    .bg(c.danger_bg)
                    .child(Node::icon("alert", 15.0, c.danger))
                    .child(
                        Node::text(i18n::tr_message(e), TextStyle::new(12.5, Weight::Medium), c.danger)
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
                    .child(stat(
                        ui,
                        tr("Pièces"),
                        {
                            // Pièces gardées par le choix et la taille ; les cachées sont comptées à la conversion.
                            let base = s.convert.base_parts(&s.parts, s.selected);
                            let min = s.convert.min_size_mm / 1000.0;
                            let kept = s
                                .parts
                                .iter()
                                .zip(&base)
                                .filter(|(p, k)| {
                                    let d = p.size.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt() * s.unit_m;
                                    **k && d >= min
                                })
                                .count();
                            if kept == s.parts.len() {
                                group(kept)
                            } else {
                                format!("{} / {}", group(kept), group(s.parts.len()))
                            }
                        },
                        false,
                    ))
                    .child(stat(ui, tr("Triangles"), compact(tri_in), false))
                    .child(stat(
                        ui,
                        tr("Écart garanti"),
                        if s.convert.tolerance_mm > 0.0 {
                            format!("{} mm", fmt_mm(s.convert.tolerance_mm))
                        } else {
                            tr("Exact").into()
                        },
                        true,
                    )),
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
            tr("Dossier"),
            Some("open"),
            ButtonKind::Secondary,
            tr("Montrer le fichier"),
        ));
        footer.push(Node::spacer());
        footer.push(widgets::button(
            ui,
            id("ouvrir"),
            tr("Ouvrir le résultat"),
            Some("file"),
            ButtonKind::Primary,
            tr("Ouvrir le fichier produit pour le comparer"),
        ));
    } else {
        footer.push(Node::spacer());
        if ui.clicked(id("annuler")) && !running {
            s.convert.open = false;
        }
        footer.push(widgets::button(ui, id("annuler"), tr("Annuler"), None, ButtonKind::Secondary, tr("Fermer")));
        if ui.clicked(id("lancer")) && !running {
            s.requests.push(Request::Convert);
        }
        let label = tr(if running { "Conversion…" } else { "Convertir" });
        let mut b = widgets::button(
            ui,
            id("lancer"),
            label,
            Some("convert"),
            ButtonKind::Primary,
            tr("Choisir le fichier de sortie et convertir"),
        );
        if running {
            b = b.opacity(0.6);
        }
        footer.push(b);
    }
    let body = Node::column().align(Align::Stretch).child(body);
    // Hauteur : en-tête, lignes de 52 px, séparateurs, bilan, pied.
    let rows = 8 + cad as usize + (s.convert.format == Format::Glb) as usize;
    let height = 482.0 + 52.0 * (rows as f32 - 5.0) + 1.0;
    let (node, close) = controls::modal(ui, Id::new("convertir"), tr("Convertir"), (640.0, height), body, footer);
    if close && !running {
        s.convert.open = false;
    }
    ui.overlay(node, Layer::Modal);
}
