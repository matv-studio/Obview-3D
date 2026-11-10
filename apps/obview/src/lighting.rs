//! Éclairage de la vue (D-33) : scénarios (HDRI livrés, environnements générés, HDRI de l'utilisateur), exposition,
//! rotation du dôme (aussi Maj + clic droit dans la vue), fond, teinte ; onglet « Éclairage » du panneau droit.

use std::{path::PathBuf, sync::Arc};

use obv_render::light::{Environment, LightBlock, Lighting, Preset};
use obv_ui::{Color, Edges, Id, Node, Size, TextStyle, Tooltip, Ui, Weight, controls, tr, widgets};
use serde::{Deserialize, Serialize};

use crate::{convert_ui::Request, shell::AppState};

/// Clé du HDRI choisi par l'utilisateur.
pub const FILE_KEY: &str = "file";

/// Réglages d'éclairage enregistrés.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingSettings {
    /// Clé du scénario (`Preset::key`) ou [`FILE_KEY`].
    pub preset: String,
    /// HDRI de l'utilisateur (.hdr, .exr).
    pub file: Option<PathBuf>,
    /// Exposition ajoutée, en IL (−3 à +3).
    pub exposure: f32,
    /// Rotation du dôme, en degrés (0 à 360).
    pub rotation: f32,
    /// Environnement visible en fond (sinon dégradé du thème).
    pub background: bool,
    /// Flou du fond (0 net à 1 très flou).
    pub blur: f32,
    /// Indice de la teinte dans [`TINTS`].
    pub tint: usize,
}

impl Default for LightingSettings {
    fn default() -> Self {
        LightingSettings {
            preset: Preset::Studio.key().into(),
            file: None,
            exposure: 0.0,
            rotation: 0.0,
            background: false,
            blur: 0.5,
            tint: 0,
        }
    }
}

/// Teintes proposées : nom, couleur linéaire appliquée à la lumière, pastille (sRGB).
pub const TINTS: [(&str, [f32; 3], u32); 7] = [
    ("Neutre", [1.0, 1.0, 1.0], 0xF2F2F2),
    ("Chaud", [1.0, 0.80, 0.60], 0xFFD2A8),
    ("Froid", [0.78, 0.88, 1.0], 0xC6DDFF),
    ("Ambre", [1.0, 0.62, 0.25], 0xFFB066),
    ("Bleu", [0.45, 0.62, 1.0], 0x7FA6FF),
    ("Rose", [1.0, 0.55, 0.78], 0xFF9CCB),
    ("Vert", [0.6, 1.0, 0.7], 0xA8FFC0),
];

/// État de l'éclairage pendant la session.
#[derive(Clone, Debug, Default)]
pub struct EnvState {
    /// Environnement affiché et sa clé.
    pub env: Option<Arc<Environment>>,
    pub key: String,
    /// Préparation en cours (clé demandée).
    pub loading: Option<String>,
    /// Demande de l'interface : clé à préparer.
    pub request: Option<String>,
    /// Aperçus des scénarios (clé, panorama sRGB de [`THUMB_W`] × [`THUMB_H`]).
    pub thumbs: Thumbs,
    /// Échec de la dernière préparation (HDRI illisible).
    pub error: Option<String>,
}

/// Aperçus des scénarios : clé et panorama sRGB.
pub type Thumbs = Vec<(String, Arc<Vec<[u8; 3]>>)>;

pub const THUMB_W: usize = 32;
pub const THUMB_H: usize = 16;

/// Paramètres d'image pour la vue : environnement chargé et réglages ; bloc neutre avant le premier.
pub fn block(s: &AppState) -> LightBlock {
    let Some(env) = &s.lighting.env else { return LightBlock::neutral() };
    let l = &s.settings.lighting;
    let lighting = Lighting {
        rotation: l.rotation.to_radians(),
        exposure: l.exposure,
        show_background: l.background,
        blur: l.blur,
        tint: TINTS.get(l.tint).map(|t| t.1).unwrap_or([1.0; 3]),
    };
    LightBlock::new(env, &lighting)
}

/// Prépare l'environnement d'une clé (fil de travail) : scénario livré ou fichier de l'utilisateur.
pub fn build(key: &str, file: Option<&std::path::Path>) -> Result<Environment, String> {
    if key == FILE_KEY {
        let f = file.ok_or("aucun fichier HDRI")?;
        return obv_render::light::build_file(f);
    }
    let p = Preset::from_key(key).unwrap_or(Preset::Studio);
    obv_render::light::build_preset(p)
}

/// Aperçus de tous les scénarios livrés (fil de travail).
pub fn thumbnails() -> Thumbs {
    use rayon::prelude::*;
    Preset::ALL
        .par_iter()
        .filter_map(|p| {
            let t = obv_render::light::preset_thumbnail(*p, THUMB_W, THUMB_H).ok()?;
            Some((p.key().to_string(), Arc::new(t)))
        })
        .collect()
}

fn id(name: &str) -> Id {
    Id::new("éclairage").with(name)
}

/// Panorama d'aperçu dessiné en cellules (l'interface n'a pas d'images en couleur).
fn panorama(thumb: Option<Arc<Vec<[u8; 3]>>>, w: f32, h: f32, fallback: Color) -> Node {
    Node::custom(move |p, r| {
        let Some(t) = &thumb else {
            p.dl.rect(r, 0.0, fallback, None);
            return;
        };
        let (cw, ch) = (r.width() / THUMB_W as f32, r.height() / THUMB_H as f32);
        for y in 0..THUMB_H {
            for x in 0..THUMB_W {
                let [cr, cg, cb] = t[y * THUMB_W + x];
                let cell =
                    obv_ui::Rect::from_xywh(r.min.x + x as f32 * cw, r.min.y + y as f32 * ch, cw + 1.5, ch + 1.5);
                p.dl.rect(cell, 0.0, Color::hex(u32::from_be_bytes([0, cr, cg, cb])), None);
            }
        }
    })
    .size(Size::Px(w), Size::Px(h))
}

/// Carte d'un scénario : aperçu, nom ; contour d'accent si choisi.
fn card(ui: &mut Ui, key: &str, label: &str, thumb: Option<Arc<Vec<[u8; 3]>>>, selected: bool, busy: bool) -> Node {
    let c = ui.theme.color.clone();
    let cid = id("carte").with(key);
    let hover = ui.anim(cid.with("survol"), if ui.hovered(cid) { 1.0 } else { 0.0 }, 120.0);
    let ring =
        if selected { if c.has_accent { c.accent } else { c.text } } else { c.border.mix(c.border_strong, hover) };
    let w = 116.0;
    let img = Node::stack()
        .width(Size::Px(w - 8.0))
        .height(Size::Px((w - 8.0) * 0.5))
        .radius(5.0)
        .clip()
        .child(panorama(thumb, w - 8.0, (w - 8.0) * 0.5, c.raised_hover))
        .children(busy.then(|| {
            Node::stack().fill().bg(Color::hex(0x000000).with_alpha(0.35)).child(crate::shell::spinner(
                ui,
                16.0,
                Color::WHITE,
            ))
        }));
    Node::column()
        .id(cid)
        .width(Size::Px(w))
        .padding(Edges::all(4.0))
        .gap(5.0)
        .radius(8.0)
        .bg(Color::TRANSPARENT.mix(c.raised_hover, hover * 0.6))
        .border(if selected { 1.5 } else { 1.0 }, ring)
        .child(img)
        .child(
            Node::text(label, TextStyle::new(12.0, if selected { Weight::SemiBold } else { Weight::Regular }), c.text)
                .truncate(),
        )
}

fn row(ui: &Ui, label: &str, control: Node) -> Node {
    Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(36.0))
        .padding(Edges::xy(12.0, 0.0))
        .gap(10.0)
        .child(
            Node::text(label, TextStyle::new(12.5, Weight::Regular), ui.theme.color.text_secondary)
                .width(Size::Px(78.0)),
        )
        .child(control)
}

/// Onglet « Éclairage » du panneau droit.
pub fn tab(ui: &mut Ui, s: &mut AppState) -> Vec<Node> {
    let c = ui.theme.color.clone();
    let mut out = vec![widgets::section_header(ui, tr("Ambiance"))];
    // Cartes des scénarios, deux par ligne ; le HDRI de l'utilisateur en dernier.
    let mut cards: Vec<(String, String)> =
        Preset::ALL.iter().map(|p| (p.key().to_string(), tr(p.label()).to_string())).collect();
    if let Some(f) = &s.settings.lighting.file {
        let name = f.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        cards.push((FILE_KEY.into(), name));
    }
    let current = s.settings.lighting.preset.clone();
    let mut grid = Node::column().padding(Edges::xy(8.0, 2.0)).gap(6.0);
    for pair in cards.chunks(2) {
        let mut line = Node::row().gap(6.0);
        for (key, label) in pair {
            let cid = id("carte").with(key.as_str());
            if ui.clicked(cid) && *key != current {
                s.settings.lighting.preset = key.clone();
                s.settings_dirty = true;
                s.lighting.request = Some(key.clone());
            }
            let thumb = s.lighting.thumbs.iter().find(|(k, _)| k == key).map(|(_, t)| t.clone());
            let busy = s.lighting.loading.as_deref() == Some(key.as_str());
            let selected = *key == s.settings.lighting.preset;
            line.push(card(ui, key, label, thumb, selected, busy));
        }
        grid.push(line);
    }
    out.push(grid);
    let load = id("charger");
    if ui.clicked(load) {
        s.requests.push(Request::LoadHdri);
    }
    out.push(
        Node::row().padding(Edges::xy(12.0, 4.0)).child(
            widgets::button(
                ui,
                load,
                tr("Charger un HDRI"),
                Some("open"),
                widgets::ButtonKind::Secondary,
                ".hdr, .exr",
            )
            .width(Size::Fill(1.0)),
        ),
    );
    if let Some(e) = &s.lighting.error {
        out.push(
            Node::row()
                .padding(Edges::xy(12.0, 2.0))
                .gap(6.0)
                .child(Node::icon("alert", 13.0, c.danger))
                .child(Node::text(tr("HDRI illisible"), TextStyle::new(12.0, Weight::Medium), c.danger))
                .tooltip(e.as_str()),
        );
    }
    out.push(Node::new().height(Size::Px(6.0)));
    out.push(widgets::divider(ui));
    out.push(widgets::section_header(ui, tr("Réglages")));
    let l = s.settings.lighting.clone();
    let value = |t: String| Node::text(t, TextStyle::new(12.0, Weight::Medium), c.text_secondary).width(Size::Px(44.0));
    let (sl, v) = controls::slider_with_default(ui, id("exposition"), l.exposure, -3.0, 3.0, 0.1, 110.0, Some(0.0));
    if let Some(v) = v {
        s.settings.lighting.exposure = v;
        s.settings_dirty = true;
    }
    let ev = obv_ui::i18n::decimal(s.settings.lighting.exposure as f64, 1);
    out.push(row(
        ui,
        tr("Exposition"),
        Node::row().gap(8.0).child(sl).child(value(obv_ui::i18n::with_unit(&ev, tr("IL")))),
    ));
    let (sl, v) = controls::slider_with_default(ui, id("rotation"), l.rotation, 0.0, 360.0, 1.0, 110.0, Some(0.0));
    if let Some(v) = v {
        s.settings.lighting.rotation = v;
        s.settings_dirty = true;
    }
    out.push(
        row(
            ui,
            tr("Rotation"),
            Node::row().gap(8.0).child(sl).child(value(format!("{:.0}°", s.settings.lighting.rotation))),
        )
        .tooltip_shortcut(tr("Tourner le dôme"), tr("Maj + clic droit")),
    );
    // Fond : dégradé du thème ou environnement (avec son flou).
    let base = id("fond");
    let mut picked = None;
    for i in 0..2 {
        if ui.clicked(base.with(i)) {
            picked = Some(i);
        }
    }
    if let Some(i) = picked {
        s.settings.lighting.background = i == 1;
        s.settings_dirty = true;
    }
    let seg = widgets::tabs(ui, base, &[tr("Dégradé"), tr("Environnement")], s.settings.lighting.background as usize)
        .width(Size::Px(162.0));
    out.push(row(ui, tr("Arrière-plan"), seg));
    if s.settings.lighting.background {
        let (sl, v) = controls::slider(ui, id("flou"), l.blur, 0.0, 1.0, 0.01, 110.0);
        if let Some(v) = v {
            s.settings.lighting.blur = v;
            s.settings_dirty = true;
        }
        out.push(row(
            ui,
            tr("Flou"),
            Node::row().gap(8.0).child(sl).child(value(format!("{:.0} %", s.settings.lighting.blur * 100.0))),
        ));
    }
    // Teinte de la lumière : pastilles.
    let mut swatches = Node::row().gap(6.0);
    for (i, (name, _, hex)) in TINTS.iter().enumerate() {
        let sid = id("teinte").with(i);
        if ui.clicked(sid) {
            s.settings.lighting.tint = i;
            s.settings_dirty = true;
        }
        let sel = s.settings.lighting.tint == i;
        let hover = ui.anim(sid.with("survol"), if ui.hovered(sid) { 1.0 } else { 0.0 }, 120.0);
        let ring = if sel { c.text } else { c.border_strong.mix(c.text_tertiary, hover) };
        let mut sw = Node::stack()
            .id(sid)
            .size(Size::Px(18.0), Size::Px(18.0))
            .radius(9.0)
            .bg(Color::hex(*hex))
            .border(if sel { 2.0 } else { 1.0 }, ring);
        sw.tooltip = Some(Tooltip { text: tr(name).into(), shortcut: None });
        swatches.push(sw);
    }
    out.push(row(ui, tr("Teinte"), swatches));
    out
}

/// Choix d'un HDRI par l'utilisateur : réglage enregistré et préparation demandée.
pub fn set_file(s: &mut AppState, path: PathBuf) {
    s.settings.lighting.file = Some(path);
    s.settings.lighting.preset = FILE_KEY.into();
    s.settings_dirty = true;
    s.lighting.request = Some(FILE_KEY.into());
}
