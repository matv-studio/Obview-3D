//! Multivue (D-42) : plusieurs vues côte à côte, chacune montrant un onglet (le même fichier peut paraître
//! dans deux vues, dans deux rendus).
//!
//! - On l'ouvre en glissant un onglet de la barre dans la vue : la moitié (ou toute la vue) visée est
//!   contournée pendant le glisser, comme l'accroche des fenêtres de Windows ; au lâcher, la vue visée se
//!   partage. Les onglets de la multivue sont reliés dans la barre.
//! - La disposition est un arbre de partages (deux parts, rapport réglable par le séparateur). Le bouton
//!   « Disposition » de la barre du bas (présent seulement en multivue) propose les dispositions courantes et
//!   les liaisons.
//! - Une vue est active : elle reçoit la navigation, les outils et les panneaux ; ses réglages de caméra et de
//!   rendu vivent dans `AppState`, ceux des autres vues dans leur [`Pane`].
//! - Caméras liées (L) : les autres vues suivent l'orientation de la vue active (cadrage rapporté à la taille
//!   de chaque modèle).

use std::{cell::Cell, rc::Rc};

use obv_render::Camera;
use obv_ui::{Anchor, Color, Cursor, Edges, Id, Key, Node, Rect, Size, TextStyle, Ui, Weight, tr, vec2, widgets};

use crate::{inspector::Inspector, shell::AppState};

/// Écart entre deux vues (pixels logiques) : celui du plateau au bord de la fenêtre.
pub const GAP: f32 = 6.0;
/// Nombre de vues au plus.
pub const MAX_PANES: usize = 4;

/// Une vue de la multivue.
#[derive(Clone, Debug)]
pub struct Pane {
    pub id: u64,
    /// Onglet montré.
    pub tab: u64,
    /// Caméra et rendu de la vue quand elle n'est pas l'active (sinon : ceux de `AppState`).
    pub camera: Camera,
    pub inspector: Inspector,
}

/// Partage de la zone des vues : feuille (une vue) ou deux parts côte à côte (`vertical` : séparateur vertical)
/// ou l'une sur l'autre ; `ratio` : part de la première.
#[derive(Clone, Debug, PartialEq)]
pub enum Split {
    Leaf(u64),
    Node { vertical: bool, ratio: f32, a: Box<Split>, b: Box<Split> },
}

/// Zone visée par un onglet glissé dans une vue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    /// Toute la vue : l'onglet y remplace celui qu'elle montrait.
    Center,
}

/// Dispositions proposées par le bouton « Disposition ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Single,
    Columns,
    Rows,
    OnePlusTwo,
    Grid,
}

impl Preset {
    pub const ALL: [Preset; 5] = [Preset::Single, Preset::Columns, Preset::Rows, Preset::OnePlusTwo, Preset::Grid];

    pub fn panes(self) -> usize {
        match self {
            Preset::Single => 1,
            Preset::Columns | Preset::Rows => 2,
            Preset::OnePlusTwo => 3,
            Preset::Grid => 4,
        }
    }

    fn label(self) -> &'static str {
        tr(match self {
            Preset::Single => "Vue unique",
            Preset::Columns => "Deux colonnes",
            Preset::Rows => "Deux rangées",
            Preset::OnePlusTwo => "Une et deux",
            Preset::Grid => "Quatre vues",
        })
    }

    /// Arbre de la disposition pour les vues `ids` (autant que [`Preset::panes`]).
    pub fn tree(self, ids: &[u64]) -> Split {
        let l = |k: usize| Box::new(Split::Leaf(ids[k]));
        let node = |vertical, a, b| Split::Node { vertical, ratio: 0.5, a, b };
        match self {
            Preset::Single => Split::Leaf(ids[0]),
            Preset::Columns => node(true, l(0), l(1)),
            Preset::Rows => node(false, l(0), l(1)),
            Preset::OnePlusTwo => node(true, l(0), Box::new(node(false, l(1), l(2)))),
            Preset::Grid => node(false, Box::new(node(true, l(0), l(1))), Box::new(node(true, l(2), l(3)))),
        }
    }

    /// Dessin de l'icône : rectangles des vues dans un carré unité.
    fn cells(self) -> Vec<[f32; 4]> {
        match self {
            Preset::Single => vec![[0.0, 0.0, 1.0, 1.0]],
            Preset::Columns => vec![[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 1.0, 1.0]],
            Preset::Rows => vec![[0.0, 0.0, 1.0, 0.5], [0.0, 0.5, 1.0, 1.0]],
            Preset::OnePlusTwo => vec![[0.0, 0.0, 0.5, 1.0], [0.5, 0.0, 1.0, 0.5], [0.5, 0.5, 1.0, 1.0]],
            Preset::Grid => {
                vec![[0.0, 0.0, 0.5, 0.5], [0.5, 0.0, 1.0, 0.5], [0.0, 0.5, 0.5, 1.0], [0.5, 0.5, 1.0, 1.0]]
            }
        }
    }
}

/// Séparateur : chemin du partage dans l'arbre, rectangle (l'écart), vertical.
pub type Divider = (u64, Rect, bool);

/// État de la multivue.
#[derive(Clone, Debug)]
pub struct Multi {
    pub root: Split,
    pub panes: Vec<Pane>,
    /// Vue active.
    pub active: u64,
    pub link_cameras: bool,
    /// Sélection liée : choisir une pièce la choisit (par son nom) dans les autres fichiers.
    pub link_selection: bool,
    pub next_id: u64,
}

impl Multi {
    pub fn pane(&self, id: u64) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }

    pub fn pane_mut(&mut self, id: u64) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    pub fn active_pane(&self) -> &Pane {
        self.pane(self.active).unwrap_or(&self.panes[0])
    }

    /// Onglets de la multivue, dans l'ordre des vues.
    pub fn tabs(&self) -> Vec<u64> {
        let mut v: Vec<u64> = Vec::new();
        for id in self.order() {
            if let Some(p) = self.pane(id)
                && !v.contains(&p.tab)
            {
                v.push(p.tab);
            }
        }
        v
    }

    /// Vues dans l'ordre de lecture de l'arbre.
    pub fn order(&self) -> Vec<u64> {
        fn walk(s: &Split, out: &mut Vec<u64>) {
            match s {
                Split::Leaf(id) => out.push(*id),
                Split::Node { a, b, .. } => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        let mut v = Vec::new();
        walk(&self.root, &mut v);
        v
    }

    /// Rectangles des vues dans `area`, et séparateurs (chemin dans l'arbre, rectangle, vertical).
    pub fn layout(&self, area: Rect) -> (Vec<(u64, Rect)>, Vec<Divider>) {
        fn walk(s: &Split, r: Rect, path: u64, panes: &mut Vec<(u64, Rect)>, divs: &mut Vec<Divider>) {
            match s {
                Split::Leaf(id) => panes.push((*id, r)),
                Split::Node { vertical, ratio, a, b } => {
                    let (ra, d, rb) = cut(r, *vertical, *ratio);
                    divs.push((path, d, *vertical));
                    walk(a, ra, path * 2, panes, divs);
                    walk(b, rb, path * 2 + 1, panes, divs);
                }
            }
        }
        let (mut panes, mut divs) = (Vec::new(), Vec::new());
        walk(&self.root, area, 1, &mut panes, &mut divs);
        (panes, divs)
    }

    pub fn rect_of(&self, area: Rect, pane: u64) -> Option<Rect> {
        self.layout(area).0.into_iter().find(|(id, _)| *id == pane).map(|(_, r)| r)
    }

    /// Partage la vue `target` : la nouvelle vue prend la moitié `zone` (ou remplace l'onglet au centre).
    /// Renvoie la vue qui montre l'onglet.
    pub fn split(&mut self, target: u64, zone: Zone, tab: u64, camera: Camera) -> u64 {
        if zone == Zone::Center || self.panes.len() >= MAX_PANES {
            if let Some(p) = self.pane_mut(target) {
                p.tab = tab;
                p.camera = camera;
                p.inspector = Inspector::Off;
            }
            return target;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.panes.push(Pane { id, tab, camera, inspector: Inspector::Off });
        fn put(s: &mut Split, target: u64, zone: Zone, id: u64) -> bool {
            match s {
                Split::Leaf(t) if *t == target => {
                    let (new, old) = (Box::new(Split::Leaf(id)), Box::new(Split::Leaf(target)));
                    let vertical = matches!(zone, Zone::Left | Zone::Right);
                    let first = matches!(zone, Zone::Left | Zone::Top);
                    let (a, b) = if first { (new, old) } else { (old, new) };
                    *s = Split::Node { vertical, ratio: 0.5, a, b };
                    true
                }
                Split::Leaf(_) => false,
                Split::Node { a, b, .. } => put(a, target, zone, id) || put(b, target, zone, id),
            }
        }
        put(&mut self.root, target, zone, id);
        id
    }

    /// Retire une vue (sa part revient à sa voisine).
    pub fn remove(&mut self, pane: u64) {
        fn cut_out(s: &mut Split, pane: u64) -> bool {
            if let Split::Node { a, b, .. } = s {
                if **a == Split::Leaf(pane) {
                    *s = (**b).clone();
                    return true;
                }
                if **b == Split::Leaf(pane) {
                    *s = (**a).clone();
                    return true;
                }
                return cut_out(a, pane) || cut_out(b, pane);
            }
            false
        }
        cut_out(&mut self.root, pane);
        self.panes.retain(|p| p.id != pane);
        if self.active == pane {
            self.active = self.order().first().copied().unwrap_or(0);
        }
    }

    /// Règle le rapport du partage au chemin `path`.
    pub fn set_ratio(&mut self, path: u64, ratio: f32) {
        fn find(s: &mut Split, at: u64, path: u64) -> Option<&mut f32> {
            match s {
                Split::Leaf(_) => None,
                Split::Node { ratio, a, b, .. } => {
                    if at == path {
                        return Some(ratio);
                    }
                    find(a, at * 2, path).or_else(|| find(b, at * 2 + 1, path))
                }
            }
        }
        if let Some(r) = find(&mut self.root, 1, path) {
            *r = ratio.clamp(0.15, 0.85);
        }
    }

    /// Disposition courante si c'est l'une des dispositions proposées.
    pub fn preset(&self) -> Option<Preset> {
        let ids = self.order();
        Preset::ALL.into_iter().find(|p| p.panes() == ids.len() && shape(&p.tree(&ids)) == shape(&self.root))
    }
}

/// Forme d'un arbre sans les rapports (comparaison des dispositions).
fn shape(s: &Split) -> String {
    match s {
        Split::Leaf(_) => "f".into(),
        Split::Node { vertical, a, b, .. } => {
            format!("({}{}{})", if *vertical { 'v' } else { 'h' }, shape(a), shape(b))
        }
    }
}

/// Coupe `r` en deux parts séparées d'un écart [`GAP`].
fn cut(r: Rect, vertical: bool, ratio: f32) -> (Rect, Rect, Rect) {
    if vertical {
        let w = (r.width() - GAP).max(0.0);
        let wa = (w * ratio).round();
        (
            Rect::from_xywh(r.min.x, r.min.y, wa, r.height()),
            Rect::from_xywh(r.min.x + wa, r.min.y, GAP, r.height()),
            Rect::new(vec2(r.min.x + wa + GAP, r.min.y), r.max),
        )
    } else {
        let h = (r.height() - GAP).max(0.0);
        let ha = (h * ratio).round();
        (
            Rect::from_xywh(r.min.x, r.min.y, r.width(), ha),
            Rect::from_xywh(r.min.x, r.min.y + ha, r.width(), GAP),
            Rect::new(vec2(r.min.x, r.min.y + ha + GAP), r.max),
        )
    }
}

/// Zone visée au point `p` d'une vue : bord le plus proche dans le tiers extérieur, sinon le centre.
pub fn zone_at(r: Rect, p: obv_ui::Vec2) -> Zone {
    let u = ((p.x - r.min.x) / r.width().max(1.0)).clamp(0.0, 1.0);
    let v = ((p.y - r.min.y) / r.height().max(1.0)).clamp(0.0, 1.0);
    let edges = [(u, Zone::Left), (1.0 - u, Zone::Right), (v, Zone::Top), (1.0 - v, Zone::Bottom)];
    let (d, z) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).expect("bords");
    if d < 0.3 { z } else { Zone::Center }
}

/// Rectangle de l'aperçu de dépôt : moitié visée, ou toute la vue.
pub fn zone_rect(r: Rect, z: Zone) -> Rect {
    let (w, h) = (r.width(), r.height());
    match z {
        Zone::Left => Rect::from_xywh(r.min.x, r.min.y, (w - GAP) * 0.5, h),
        Zone::Right => Rect::new(vec2(r.min.x + (w + GAP) * 0.5, r.min.y), r.max),
        Zone::Top => Rect::from_xywh(r.min.x, r.min.y, w, (h - GAP) * 0.5),
        Zone::Bottom => Rect::new(vec2(r.min.x, r.min.y + (h + GAP) * 0.5), r.max),
        Zone::Center => r,
    }
}

/// Caméra d'une vue liée : orientation et projection de la vue active ; même fichier, même caméra ; autre
/// fichier, cadrage et décalage rapportés à la taille des modèles (`r_from`, `r_to` : rayons englobants,
/// `c_from`, `c_to` : centres).
pub fn linked_camera(
    from: &Camera,
    to: &Camera,
    same: bool,
    r_from: f64,
    r_to: f64,
    c_from: [f64; 3],
    c_to: [f64; 3],
) -> Camera {
    if same {
        return *from;
    }
    let k = r_to / r_from.max(1e-12);
    Camera {
        yaw: from.yaw,
        pitch: from.pitch,
        ortho: from.ortho,
        fov_y: from.fov_y,
        distance: from.distance * k,
        target: std::array::from_fn(|i| c_to[i] + (from.target[i] - c_from[i]) * k),
        pivot: std::array::from_fn(|i| c_to[i] + (from.pivot[i] - c_from[i]) * k),
        radius: to.radius,
    }
}

/// Demandes de la multivue, traitées par la fenêtre au début de l'image suivante.
#[derive(Clone, Debug, PartialEq)]
pub enum MultiRequest {
    /// Onglet `tab` déposé dans la vue `pane` (aucune : vue unique), zone `zone`.
    Drop {
        tab: u64,
        pane: Option<u64>,
        zone: Zone,
    },
    Activate(u64),
    /// Montrer l'onglet `tab` dans la vue `pane`.
    Show {
        pane: u64,
        tab: u64,
    },
    Close(u64),
    Preset(Preset),
}

fn chip_id(pane: u64) -> Id {
    Id::new("volet puce").with(pane)
}

fn divider_id(path: u64) -> Id {
    Id::new("volet séparateur").with(path)
}

pub fn link_id() -> Id {
    Id::new("volet lier")
}

pub fn layout_id() -> Id {
    Id::new("disposition")
}

fn preset_id(p: Preset) -> Id {
    Id::new("disposition choix").with(p as u8)
}

/// La multivue est-elle affichée (l'onglet actif est celui de la vue active) ?
pub fn visible(s: &AppState) -> bool {
    s.multi.as_ref().is_some_and(|m| m.active_pane().tab == s.active_tab)
}

/// Clics, glisser des séparateurs, raccourcis.
pub fn handle(ui: &mut Ui, s: &mut AppState) {
    if !visible(s) {
        return;
    }
    let area = s.multi_area.get();
    let plain = !ui.input.modifiers.ctrl && !ui.text_focused() && !s.settings_open && !s.convert.open;
    let Some(m) = &mut s.multi else { return };
    if ui.clicked(link_id()) || ui.clicked(link_id().with("menu")) || (plain && ui.input.keys.contains(&Key::Char('l')))
    {
        m.link_cameras = !m.link_cameras;
    }
    if ui.clicked(Id::new("disposition lier sélection")) {
        m.link_selection = !m.link_selection;
    }
    let (panes, divs) = m.layout(area);
    for (path, r, vertical) in divs {
        let did = divider_id(path);
        if let Some(p) = ui.dragging(did) {
            // Rapport d'après la position du pointeur dans le rectangle du partage (parent du séparateur).
            let parent = parent_rect(m, area, path).unwrap_or(r);
            let ratio = if vertical {
                (p.x - parent.min.x - GAP * 0.5) / (parent.width() - GAP).max(1.0)
            } else {
                (p.y - parent.min.y - GAP * 0.5) / (parent.height() - GAP).max(1.0)
            };
            m.set_ratio(path, ratio);
        }
        if ui.double_clicked(did) {
            m.set_ratio(path, 0.5);
        }
    }
    for (id, _) in &panes {
        if ui.clicked(chip_id(*id)) {
            ui.toggle_popup(chip_id(*id));
        }
    }
    for p in Preset::ALL {
        if ui.clicked(preset_id(p)) {
            s.multi_requests.push(MultiRequest::Preset(p));
        }
    }
    if ui.clicked(layout_id()) {
        ui.toggle_popup(layout_id());
    }
}

/// Rectangle du partage dont le séparateur est au chemin `path`.
fn parent_rect(m: &Multi, area: Rect, path: u64) -> Option<Rect> {
    fn walk(s: &Split, r: Rect, at: u64, path: u64) -> Option<Rect> {
        match s {
            Split::Leaf(_) => None,
            Split::Node { vertical, ratio, a, b } => {
                if at == path {
                    return Some(r);
                }
                let (ra, _, rb) = cut(r, *vertical, *ratio);
                walk(a, ra, at * 2, path).or_else(|| walk(b, rb, at * 2 + 1, path))
            }
        }
    }
    walk(&m.root, area, 1, path)
}

/// Éléments de la multivue posés sur la zone des vues (positions relatives à `area`) : séparateurs, lien des
/// caméras, puces des vues et leurs menus, contours ; `active` : contenu de la vue active (outils, gizmo…).
pub fn overlay(ui: &mut Ui, s: &mut AppState, active: Node) -> Vec<Node> {
    let Some(m) = s.multi.clone() else { return vec![active] };
    let c = ui.theme.color.clone();
    let area = s.multi_area.get();
    let (panes, _) = m.layout(area);
    let rel = |r: Rect| (r.min.x - area.min.x, r.min.y - area.min.y);
    let mut out = Vec::new();
    // Vue active : contenu habituel de la vue, à sa place.
    if let Some((_, r)) = panes.iter().find(|(id, _)| *id == m.active) {
        let (x, y) = rel(*r);
        out.push(
            Node::stack()
                .anchor(Anchor { left: Some(x), top: Some(y), ..Default::default() })
                .size(Size::Px(r.width()), Size::Px(r.height()))
                .child(active),
        );
    }
    // Vues inactives : leur gizmo, en image (un appui dans la vue la rend active).
    let style = s.settings.gizmo;
    if style != crate::settings::GizmoStyle::None {
        for (id, r) in panes.iter().filter(|(id, _)| *id != m.active) {
            let Some(p) = m.pane(*id) else { continue };
            let (yaw, pitch) = (p.camera.yaw as f32, p.camera.pitch as f32);
            let theme = ui.theme.clone();
            let side = crate::gizmo::size(style);
            let (x, y) = rel(*r);
            out.push(
                Node::custom(move |pt, rr| crate::gizmo::paint(style, pt, rr, yaw, pitch, None, 0.0, &theme))
                    .size(Size::Px(side), Size::Px(side))
                    .anchor(Anchor {
                        left: Some(x + r.width() - 12.0 - side),
                        top: Some(y + 12.0),
                        ..Default::default()
                    }),
            );
        }
    }
    // Puces : fichier, rendu s'il n'est pas l'éclairé, choix de l'onglet montré.
    for (id, r) in &panes {
        let Some(p) = m.pane(*id) else { continue };
        let is_active = *id == m.active;
        let inspector = if is_active { s.inspector } else { p.inspector };
        let name = s.tabs.iter().find(|t| t.id == p.tab).map(|t| t.name.clone()).unwrap_or_default();
        let mode = match inspector {
            Inspector::Single(mode) => Some(tr(crate::inspector::label(mode))),
            _ => None,
        };
        let cid = chip_id(*id);
        let hover = ui.anim(cid.with("survol"), if ui.hovered(cid) || ui.popup_open(cid) { 1.0 } else { 0.0 }, 120.0);
        let (x, y) = rel(*r);
        let chip = Node::row()
            .id(cid)
            .anchor(Anchor { left: Some(x + 10.0), top: Some(y + 10.0), ..Default::default() })
            .height(Size::Px(28.0))
            .padding(Edges { left: 8.0, right: 6.0, top: 0.0, bottom: 0.0 })
            .gap(6.0)
            .radius(6.0)
            .bg(c.floating.mix(c.raised_hover, hover))
            .border(1.0, c.border)
            .tooltip(tr("Onglet montré"))
            .child(Node::icon("file", 13.0, c.text_secondary))
            .child(Node::text(name, TextStyle::new(12.5, Weight::Medium), c.text))
            .children(
                mode.map(|l| Node::text(format!("· {l}"), TextStyle::new(12.5, Weight::Regular), c.text_secondary)),
            )
            .child(Node::icon("chevron-down", 12.0, c.text_tertiary));
        out.push(chip);
        if ui.popup_open(cid)
            && let Some(cr) = ui.rect_of(cid)
        {
            chip_menu(ui, s, &m, *id, cr);
        }
    }
    out
}

/// Séparateurs (zone de prise sur l'écart) et lien des caméras au milieu du premier, posés au-dessus des
/// contours des vues ; positions relatives au plateau `stage`.
pub fn chrome(ui: &mut Ui, s: &AppState) -> Vec<Node> {
    let Some(m) = s.multi.clone().filter(|_| visible(s)) else { return Vec::new() };
    let c = ui.theme.color.clone();
    let area = s.multi_area.get();
    let stage = s.stage.get();
    let (_, divs) = m.layout(area);
    let mut out = Vec::new();
    // Séparateurs : zone de prise sur l'écart ; lien des caméras au milieu du premier.
    for (path, r, vertical) in &divs {
        let did = divider_id(*path);
        let (x, y) = (r.min.x - stage.min.x, r.min.y - stage.min.y);
        let hot = ui.anim(did.with("survol"), if ui.hovered(did) || ui.is_active(did) { 1.0 } else { 0.0 }, 120.0);
        let line = if c.has_accent { c.accent } else { c.text_tertiary };
        let v = *vertical;
        out.push(
            Node::custom(move |p, r| {
                if hot > 0.01 {
                    let bar = if v {
                        Rect::from_xywh(r.center().x - 1.0, r.min.y + 8.0, 2.0, r.height() - 16.0)
                    } else {
                        Rect::from_xywh(r.min.x + 8.0, r.center().y - 1.0, r.width() - 16.0, 2.0)
                    };
                    p.dl.rect(bar, 1.0, line.with_alpha(hot), None);
                }
            })
            .id(did)
            .cursor(if v { Cursor::ResizeHorizontal } else { Cursor::ResizeVertical })
            .anchor(Anchor { left: Some(x - 2.0), top: Some(y - 2.0), ..Default::default() })
            .size(Size::Px(r.width() + 4.0), Size::Px(r.height() + 4.0)),
        );
    }
    if let Some((_, r, _)) = divs.first() {
        let lid = link_id();
        let hover = ui.anim(lid.with("survol"), if ui.hovered(lid) { 1.0 } else { 0.0 }, 120.0);
        let on = m.link_cameras;
        let (bg, fg) = if on {
            (if c.has_accent { c.accent } else { c.text }, if c.has_accent { c.accent_text } else { c.app_bg })
        } else {
            (c.floating.mix(c.raised_hover, hover), c.text_secondary.mix(c.text, hover))
        };
        let size = 26.0;
        let (cx, cy) = (r.center().x - stage.min.x, r.center().y - stage.min.y);
        out.push(
            Node::stack()
                .id(lid)
                .anchor(Anchor { left: Some(cx - size * 0.5), top: Some(cy - size * 0.5), ..Default::default() })
                .size(Size::Px(size), Size::Px(size))
                .radius(size * 0.5)
                .bg(bg)
                .border(1.0, if on { bg } else { c.border_strong })
                .tooltip_shortcut(tr(if on { "Caméras liées" } else { "Lier les caméras" }), "L")
                .child(Node::icon("link", 13.0, fg)),
        );
    }
    out
}

/// Menu d'une puce : onglets à montrer dans la vue, fermeture de la vue.
fn chip_menu(ui: &mut Ui, s: &mut AppState, m: &Multi, pane: u64, r: Rect) {
    let c = ui.theme.color.clone();
    let mm = ui.theme.size.clone();
    let shown = m.pane(pane).map(|p| p.tab);
    let mut rows = Vec::new();
    for t in s.tabs.clone() {
        let rid = Id::new("volet onglet").with(pane).with(t.id);
        if ui.clicked(rid) {
            s.multi_requests.push(MultiRequest::Show { pane, tab: t.id });
            ui.close_popup();
        }
        let hover = ui.anim(rid.with("survol"), if ui.hovered(rid) { 1.0 } else { 0.0 }, 120.0);
        rows.push(
            Node::row()
                .id(rid)
                .width(Size::Fill(1.0))
                .height(Size::Px(30.0))
                .padding(Edges::xy(8.0, 0.0))
                .gap(8.0)
                .radius(6.0)
                .bg(Color::TRANSPARENT.mix(c.raised_hover, hover))
                .child(Node::icon("file", 14.0, c.text_secondary))
                .child(
                    Node::text(t.name.clone(), TextStyle::new(mm.font_size, Weight::Regular), c.text)
                        .width(Size::Fill(1.0))
                        .truncate(),
                )
                .children((shown == Some(t.id)).then(|| Node::icon("check", 14.0, c.text))),
        );
    }
    rows.push(Node::new().width(Size::Fill(1.0)).height(Size::Px(1.0)).bg(c.divider));
    let close = Id::new("volet fermer").with(pane);
    if ui.clicked(close) {
        s.multi_requests.push(MultiRequest::Close(pane));
        ui.close_popup();
    }
    let hover = ui.anim(close.with("survol"), if ui.hovered(close) { 1.0 } else { 0.0 }, 120.0);
    rows.push(
        Node::row()
            .id(close)
            .width(Size::Fill(1.0))
            .height(Size::Px(30.0))
            .padding(Edges::xy(8.0, 0.0))
            .gap(8.0)
            .radius(6.0)
            .bg(Color::TRANSPARENT.mix(c.raised_hover, hover))
            .child(Node::icon("close", 14.0, c.text_secondary))
            .child(Node::text(tr("Fermer la vue"), TextStyle::new(mm.font_size, Weight::Regular), c.text)),
    );
    let appear = ui.appear(Id::new("volet menu").with(pane), 120.0);
    let menu = Node::column()
        .anchor(Anchor { left: Some(r.min.x), top: Some(r.max.y + 6.0 - (1.0 - appear) * 4.0), ..Default::default() })
        .opacity(appear)
        .width(Size::Px(260.0))
        .padding(Edges::all(6.0))
        .gap(2.0)
        .radius(mm.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, vec2(0.0, 16.0), c.shadow)
        .children(rows);
    ui.overlay(Node::stack().child(menu), obv_ui::Layer::Popup);
}

/// Bouton « Disposition » de la barre du bas (seulement en multivue) et son menu.
pub fn layout_button(ui: &mut Ui, s: &AppState) -> Option<Node> {
    if !visible(s) {
        return None;
    }
    Some(widgets::icon_button(ui, layout_id(), "layout", tr("Disposition"), None, ui.popup_open(layout_id())))
}

/// Menu de la disposition au-dessus du bouton : dispositions, caméras liées, sélection liée.
pub fn layout_popup(ui: &mut Ui, s: &AppState) {
    let lid = layout_id();
    if !ui.popup_open(lid) || !visible(s) {
        return;
    }
    let (Some(m), Some(r)) = (&s.multi, ui.rect_of(lid)) else { return };
    let c = ui.theme.color.clone();
    let mm = ui.theme.size.clone();
    let current = m.preset();
    let mut presets = Node::row().gap(6.0);
    for p in Preset::ALL {
        let pid = preset_id(p);
        let on = current == Some(p);
        let hover = ui.anim(pid.with("survol"), if ui.hovered(pid) { 1.0 } else { 0.0 }, 120.0);
        let ink = if on { c.text } else { c.text_secondary.mix(c.text, hover) };
        let cells = p.cells();
        presets.push(
            Node::stack()
                .id(pid)
                .size(Size::Px(48.0), Size::Px(36.0))
                .radius(6.0)
                .bg(if on { c.surface_active } else { Color::TRANSPARENT.mix(c.raised_hover, hover) })
                .border(if on { 1.5 } else { 1.0 }, if on { ink } else { c.border })
                .tooltip(p.label())
                .child(
                    Node::custom(move |pt, r| {
                        let b = Rect::from_xywh(r.center().x - 13.0, r.center().y - 9.0, 26.0, 18.0);
                        for k in &cells {
                            let cell = Rect::new(
                                vec2(b.min.x + b.width() * k[0], b.min.y + b.height() * k[1]),
                                vec2(b.min.x + b.width() * k[2], b.min.y + b.height() * k[3]),
                            );
                            pt.dl.rect(cell, 2.0, Color::TRANSPARENT, Some((1.25, ink)));
                        }
                    })
                    .fill(),
                ),
        );
    }
    let toggle = |ui: &mut Ui, id: Id, label: &str, on: bool| {
        Node::row()
            .width(Size::Fill(1.0))
            .height(Size::Px(32.0))
            .padding(Edges::xy(4.0, 0.0))
            .child(Node::text(label, TextStyle::new(mm.font_size, Weight::Regular), c.text).width(Size::Fill(1.0)))
            .child(obv_ui::controls::toggle(ui, id, on).0)
    };
    let body = Node::column()
        .width(Size::Fill(1.0))
        .gap(2.0)
        .child(toggle(ui, link_id().with("menu"), tr("Caméras liées"), m.link_cameras))
        .child(toggle(ui, Id::new("disposition lier sélection"), tr("Sélection liée"), m.link_selection));
    let appear = ui.appear(lid.with("menu"), 120.0);
    let w = 6.0 * 48.0 + 4.0 * 6.0 - 10.0;
    let menu = Node::column()
        .anchor(Anchor {
            left: Some(r.center().x - w * 0.5 - 8.0),
            bottom: Some(ui.input.size.y - r.min.y + 8.0 - (1.0 - appear) * 4.0),
            ..Default::default()
        })
        .opacity(appear)
        .width(Size::Px(w + 16.0))
        .padding(Edges::all(8.0))
        .gap(8.0)
        .radius(mm.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, vec2(0.0, 16.0), c.shadow)
        .child(presets)
        .child(Node::new().width(Size::Fill(1.0)).height(Size::Px(1.0)).bg(c.divider))
        .child(body);
    ui.overlay(Node::stack().child(menu), obv_ui::Layer::Popup);
}

/// Contours des vues (dessinés par-dessus la 3D, comme celui du plateau) : anneau couleur du cadre qui
/// recouvre les écarts et arrondit les coins, trait du plateau ; trait plus marqué autour de la vue active.
pub fn frames(s: &AppState, theme: &obv_ui::Theme, radius: f32) -> Option<Node> {
    let m = s.multi.as_ref().filter(|_| visible(s))?.clone();
    let c = theme.color.clone();
    let area_cell: Rc<Cell<Rect>> = s.multi_area.clone();
    Some(
        Node::custom(move |p, _| {
            let area = area_cell.get();
            let (panes, _) = m.layout(area);
            for (id, r) in &panes {
                p.dl.rect(r.expand(GAP), radius + GAP, Color::TRANSPARENT, Some((GAP, c.app_bg)));
                let active = *id == m.active;
                let line = if active { c.text_tertiary } else { c.stage_line };
                p.dl.rect(*r, radius, Color::TRANSPARENT, Some((if active { 1.5 } else { 1.0 }, line)));
            }
        })
        .fill(),
    )
}

/// Aperçu du dépôt d'un onglet glissé dans la vue : contour d'accent de la moitié (ou de la vue) visée.
pub fn drop_preview(ui: &mut Ui, s: &AppState) -> Option<Node> {
    let (_, r) = s.tab_drop?;
    let c = ui.theme.color.clone();
    let line = if c.has_accent { c.accent } else { c.text };
    // Contour animé d'une zone à l'autre.
    let k = Id::new("dépôt onglet");
    let x0 = ui.anim(k.with(0u8), r.min.x, 90.0);
    let y0 = ui.anim(k.with(1u8), r.min.y, 90.0);
    let x1 = ui.anim(k.with(2u8), r.max.x, 90.0);
    let y1 = ui.anim(k.with(3u8), r.max.y, 90.0);
    let shown = Rect::new(vec2(x0, y0), vec2(x1, y1)).shrink(Edges::all(4.0));
    Some(
        Node::custom(move |p, _| {
            p.dl.rect(shown, 10.0, line.with_alpha(0.07), Some((2.0, line)));
        })
        .fill(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn multi() -> Multi {
        let cam = Camera::default();
        Multi {
            root: Split::Leaf(0),
            panes: vec![Pane { id: 0, tab: 1, camera: cam, inspector: Inspector::Off }],
            active: 0,
            link_cameras: false,
            link_selection: false,
            next_id: 1,
        }
    }

    #[test]
    fn partages_et_dispositions() {
        let mut m = multi();
        let b = m.split(0, Zone::Right, 2, Camera::default());
        assert_eq!(m.preset(), Some(Preset::Columns));
        let area = Rect::from_xywh(0.0, 0.0, 1006.0, 600.0);
        let (panes, divs) = m.layout(area);
        assert_eq!(panes[0].1.width(), 500.0);
        assert_eq!(panes[1].1.min.x, 506.0);
        assert_eq!(divs.len(), 1);
        let c = m.split(b, Zone::Bottom, 3, Camera::default());
        assert_eq!(m.preset(), Some(Preset::OnePlusTwo));
        assert_eq!(m.tabs(), vec![1, 2, 3]);
        m.set_ratio(1, 0.3);
        assert!((m.layout(area).0[0].1.width() - 300.0).abs() < 1.0);
        m.remove(c);
        assert_eq!(m.order(), vec![0, b]);
        assert_eq!(zone_at(area, vec2(20.0, 300.0)), Zone::Left);
        assert_eq!(zone_at(area, vec2(500.0, 300.0)), Zone::Center);
        assert_eq!(zone_at(area, vec2(500.0, 590.0)), Zone::Bottom);
    }

    #[test]
    fn camera_liee() {
        let from = Camera { yaw: 1.0, pitch: 0.2, distance: 10.0, target: [1.0, 0.0, 0.0], ..Default::default() };
        let to = Camera::default();
        let l = linked_camera(&from, &to, false, 5.0, 1.0, [0.0; 3], [10.0, 0.0, 0.0]);
        assert_eq!((l.yaw, l.pitch), (1.0, 0.2));
        assert!((l.distance - 2.0).abs() < 1e-9);
        assert!((l.target[0] - 10.2).abs() < 1e-9);
    }
}
