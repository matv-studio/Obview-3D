//! Onglets (D-41) : plusieurs fichiers ouverts dans une même fenêtre, précédés de l'onglet d'accueil.
//!
//! L'état d'un document (pièces, caméra, outils, vues de rendu…) vit dans `AppState` pour l'onglet actif ;
//! les autres onglets le gardent dans un [`DocState`], échangé champ par champ au changement d'onglet. Les
//! réglages, l'éclairage, les panneaux et les fenêtres restent communs à tous les onglets.

use std::path::PathBuf;

use obv_io::Model;
use obv_render::Camera;
use obv_ui::{Color, Edges, Id, Key, Node, Size, TextStyle, Ui, Weight, tr, widgets};

use crate::{
    inspector::Inspector,
    loading::Loading,
    shell::{AppState, PartInfo, Tool, progress_ring, spinner},
    tools, views,
};

/// Déclare les champs propres à un document : la structure qui les garde hors de `AppState`, sa création et
/// l'échange avec `AppState`. Un champ ajouté ici suit l'onglet ; ailleurs, il est commun à tous.
macro_rules! doc_fields {
    ($($(#[$doc:meta])* $f:ident: $t:ty),* $(,)?) => {
        /// État d'un document hors de l'onglet actif.
        pub struct DocState {
            $($(#[$doc])* pub $f: $t,)*
        }

        impl DocState {
            /// État de départ d'un document lu (`model`) ou vide (accueil, lecture en cours).
            pub fn new(name: &str, model: &Model) -> Self {
                let s = AppState::new(name, model);
                DocState { $($f: s.$f,)* }
            }

            /// Échange ces champs avec ceux de l'onglet actif.
            pub fn swap(&mut self, s: &mut AppState) {
                $(std::mem::swap(&mut self.$f, &mut s.$f);)*
            }
        }
    };
}

doc_fields!(
    file_name: String,
    unit_m: f64,
    parts: Vec<PartInfo>,
    selected: Option<usize>,
    root_open: bool,
    tool: Tool,
    search: String,
    camera: Camera,
    reveal: Option<usize>,
    inspector: Inspector,
    measure: tools::MeasureState,
    section: tools::SectionState,
    explode: f32,
    views: views::ViewsState,
    camera_request: Option<Camera>,
    scene_bounds: Option<([f32; 3], [f32; 3])>,
    /// Lecture en cours ou en attente du fichier de l'onglet.
    loading: Option<Loading>,
);

/// Identifiant de l'onglet d'accueil (aucun document).
pub const HOME: u64 = 0;

/// Onglet vu par l'interface.
#[derive(Clone, Debug)]
pub struct TabInfo {
    pub id: u64,
    pub name: String,
    pub path: Option<PathBuf>,
    /// Lecture en cours : avancement global s'il est mesurable.
    pub loading: Option<Option<f32>>,
    /// Vignette du fichier (aperçu au survol).
    pub thumb: Option<obv_ui::PhotoId>,
}

/// Demandes de la barre d'onglets, traitées par la fenêtre au début de l'image suivante.
#[derive(Clone, Debug, PartialEq)]
pub enum TabRequest {
    Select(u64),
    Close(u64),
    /// Déplace l'onglet à cet indice (glisser dans la barre).
    Move(u64, usize),
    /// Onglet suivant (+1) ou précédent (−1), en boucle, accueil compris.
    Cycle(i32),
    /// N-ième onglet de fichier (Ctrl 1 à 8), le dernier (Ctrl 9).
    Nth(usize),
    /// Rouvre le dernier onglet fermé.
    Reopen,
}

/// Glisser d'un onglet : identifiant, décalage du pointeur dans l'onglet à l'appui, déplacé au-delà du seuil.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabDrag {
    pub id: u64,
    pub grab: f32,
    pub start: f32,
    pub moved: bool,
    /// Glissé sous la barre, au-dessus de la vue.
    pub outside: bool,
}

pub fn tab_id(id: u64) -> Id {
    Id::new("onglet").with(id)
}

fn close_id(id: u64) -> Id {
    Id::new("onglet fermer").with(id)
}

pub fn new_tab_id() -> Id {
    Id::new("onglet nouveau")
}

/// Largeur naturelle d'un onglet de fichier, bornes comprises.
const TAB_MIN: f32 = 84.0;
const TAB_MAX: f32 = 220.0;

/// Vue et zone visées par l'onglet `tab` glissé au point `p`, avec le rectangle de l'aperçu.
fn drop_target(
    s: &AppState,
    tab: u64,
    p: obv_ui::Vec2,
) -> Option<((Option<u64>, crate::multiview::Zone), obv_ui::Rect)> {
    use crate::multiview::{self, Zone};
    if s.active_tab == HOME {
        return None;
    }
    if multiview::visible(s) {
        let m = s.multi.as_ref()?;
        let (panes, _) = m.layout(s.multi_area.get());
        let (id, r) = panes.into_iter().find(|(_, r)| r.contains(p))?;
        let mut z = multiview::zone_at(r, p);
        // Quatre vues au plus : au-delà, l'onglet ne peut que remplacer.
        if m.panes.len() >= multiview::MAX_PANES {
            z = Zone::Center;
        }
        if z == Zone::Center && m.pane(id).is_some_and(|pn| pn.tab == tab) {
            return None;
        }
        return Some(((Some(id), z), multiview::zone_rect(r, z)));
    }
    let r = s.viewport.get();
    if !r.contains(p) {
        return None;
    }
    let z = multiview::zone_at(r, p);
    if z == Zone::Center && tab == s.active_tab {
        return None;
    }
    Some(((None, z), multiview::zone_rect(r, z)))
}

/// Clics, glisser et raccourcis de la barre d'onglets.
pub fn handle(ui: &mut Ui, s: &mut AppState) {
    if ui.clicked(tab_id(HOME)) {
        s.tab_requests.push(TabRequest::Select(HOME));
    }
    if ui.clicked(new_tab_id()) {
        s.requests.push(crate::convert_ui::Request::OpenFile);
    }
    let ids: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();
    for &id in &ids {
        let tid = tab_id(id);
        if ui.clicked(close_id(id)) || ui.middle_clicked(tid) || ui.middle_clicked(close_id(id)) {
            s.tab_requests.push(TabRequest::Close(id));
            continue;
        }
        if ui.clicked(tid) && !s.tab_drag.is_some_and(|d| d.id == id && d.moved) {
            s.tab_requests.push(TabRequest::Select(id));
        }
        if ui.press_started(tid)
            && let (Some(p), Some(r)) = (ui.input.pointer, ui.rect_of(tid))
        {
            // L'onglet s'active au clic (relâché sans glisser) : glissé dans la vue, il rejoint l'onglet affiché.
            s.tab_drag = Some(TabDrag { id, grab: p.x - r.min.x, start: p.x, moved: false, outside: false });
        }
        if let Some(p) = ui.dragging(tid)
            && let Some(d) = &mut s.tab_drag
            && d.id == id
        {
            if !d.moved && (p.x - d.start).abs() > 4.0 {
                d.moved = true;
            }
            // Sous la barre : dépôt dans la vue (multivue, D-42) au lieu d'un déplacement dans la barre.
            let below = ui.rect_of(tid).is_some_and(|r| p.y > r.max.y + 12.0);
            if below {
                d.moved = true;
                d.outside = true;
            } else if d.outside && ui.rect_of(tid).is_some_and(|r| p.y <= r.max.y) {
                d.outside = false;
            }
            s.tab_drop = if d.outside { drop_target(s, id, p) } else { None };
            if s.tab_drop.is_some() || below {
                ui.keep_animating();
            }
            let d = s.tab_drag.as_mut().expect("glisser");
            if d.moved && !d.outside {
                ui.keep_animating();
                // Indice d'insertion : premier onglet dont le milieu est à droite du bord gauche glissé.
                let left = p.x - d.grab;
                let w = ui.rect_of(tid).map(|r| r.width()).unwrap_or(TAB_MIN);
                let centre = left + w * 0.5;
                let others: Vec<(u64, f32)> = ids
                    .iter()
                    .filter(|&&o| o != id)
                    .filter_map(|&o| ui.rect_of(tab_id(o)).map(|r| (o, r.center().x)))
                    .collect();
                let to = others.iter().take_while(|(_, x)| *x < centre).count();
                let from = ids.iter().position(|&o| o == id).unwrap_or(0);
                if to != from {
                    s.tab_requests.push(TabRequest::Move(id, to));
                }
            }
        }
    }
    if !ui.is_active_any() {
        // Onglet lâché sur la vue : partage (multivue).
        if let (Some(d), Some(((pane, zone), _))) = (s.tab_drag.take(), s.tab_drop.take())
            && d.outside
        {
            s.multi_requests.push(crate::multiview::MultiRequest::Drop { tab: d.id, pane, zone });
        }
        s.tab_drag = None;
        s.tab_drop = None;
    }
    // Raccourcis : Ctrl T ouvrir, Ctrl W fermer, Ctrl Tab / Ctrl Maj Tab, Ctrl 1 à 9, Ctrl Maj T.
    let m = ui.input.modifiers;
    if !m.ctrl {
        return;
    }
    let keys = ui.input.keys.clone();
    for k in keys {
        match k {
            Key::Char('t') if m.shift => s.tab_requests.push(TabRequest::Reopen),
            Key::Char('t') => s.requests.push(crate::convert_ui::Request::OpenFile),
            Key::Char('w') if s.active_tab != HOME => s.tab_requests.push(TabRequest::Close(s.active_tab)),
            Key::Tab => s.tab_requests.push(TabRequest::Cycle(if m.shift { -1 } else { 1 })),
            Key::Char(c @ '1'..='9') => s.tab_requests.push(TabRequest::Nth(c as usize - '1' as usize)),
            _ => {}
        }
    }
}

/// Barre d'onglets : accueil, fichiers ouverts, nouvel onglet. `avail` : largeur disponible.
pub fn bar(ui: &mut Ui, s: &AppState, avail: f32) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let style = TextStyle::new(m.font_size, Weight::Medium);
    let scale = ui.input.scale;
    // Largeur commune quand la place manque ; sinon chaque onglet prend la largeur de son nom.
    let natural: Vec<f32> = s
        .tabs
        .iter()
        .map(|t| {
            (ui.fonts.layout(&t.name, style, scale).width.ceil() + 8.0 + 15.0 + 6.0 + 6.0 + 18.0 + 4.0)
                .clamp(TAB_MIN, TAB_MAX)
        })
        .collect();
    let n = s.tabs.len().max(1) as f32;
    let room = (avail - 32.0 - 28.0 - 8.0 - 2.0 * n).max(TAB_MIN * n);
    let total: f32 = natural.iter().sum();
    let squeeze = if total > room { Some((room / n).max(TAB_MIN)) } else { None };

    let home_active = s.active_tab == HOME;
    let hid = tab_id(HOME);
    let home = Node::stack()
        .id(hid)
        .size(Size::Px(32.0), Size::Px(28.0))
        .radius(6.0)
        .bg(if home_active {
            c.raised
        } else {
            ui.state_color(hid, Color::TRANSPARENT, c.raised_hover, c.surface_active)
        })
        .border(1.0, if home_active { c.border } else { Color::TRANSPARENT })
        .tooltip(tr("Accueil"))
        .child(Node::icon("home", 15.0, if home_active { c.text } else { c.icon }));
    let mut row = Node::row().gap(2.0).child(home);
    let dragging = s.tab_drag.filter(|d| d.moved).map(|d| d.id);
    // Onglets de la multivue (D-42) : reliés dans un même cadre ; les suites contiguës forment un groupe.
    let grouped: Vec<u64> = s.multi.as_ref().map(|m| m.tabs()).unwrap_or_default();
    let mut group: Option<Node> = None;
    let flush = |row: &mut Node, group: &mut Option<Node>| {
        if let Some(g) = group.take() {
            row.push(g);
        }
    };
    for (k, t) in s.tabs.iter().enumerate() {
        if !grouped.contains(&t.id) {
            flush(&mut row, &mut group);
        }
        let active = t.id == s.active_tab;
        let tid = tab_id(t.id);
        let hover =
            ui.anim(tid.with("survol"), if ui.hovered(tid) || ui.hovered(close_id(t.id)) { 1.0 } else { 0.0 }, 120.0);
        let lead = match t.loading {
            Some(Some(f)) => progress_ring(ui, 14.0, f, c.accent),
            Some(None) => spinner(ui, 14.0, c.accent),
            None => Node::icon("file", 14.0, if active { c.text_secondary } else { c.text_tertiary }),
        };
        let show_close = active || hover > 0.01;
        let close = widgets::small_icon_button(ui, close_id(t.id), "close", tr("Fermer l'onglet"), c.text_tertiary)
            .size(Size::Px(18.0), Size::Px(18.0))
            .opacity(if active { 1.0 } else { hover });
        let bg = if active { c.raised } else { Color::TRANSPARENT.mix(c.raised_hover, hover) };
        let mut tab = Node::row()
            .id(tid)
            .width(Size::Px(squeeze.unwrap_or(natural[k])))
            .height(Size::Px(28.0))
            .padding(Edges { left: 8.0, right: 4.0, top: 0.0, bottom: 0.0 })
            .gap(6.0)
            .radius(6.0)
            .bg(bg)
            .border(1.0, if active { c.border } else { Color::TRANSPARENT })
            .child(lead)
            .child(
                Node::text(t.name.as_str(), style, if active { c.text } else { c.text_secondary })
                    .width(Size::Fill(1.0))
                    .truncate(),
            );
        tab = tab.child(if show_close { close } else { Node::new().size(Size::Px(18.0), Size::Px(18.0)) });
        // Chemin en info-bulle, sauf quand l'aperçu (qui le montre) le remplace.
        if active || t.thumb.is_none() {
            tab = tab
                .tooltip(t.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| t.name.clone()));
        }
        if dragging == Some(t.id) {
            tab = tab.opacity(if s.tab_drag.is_some_and(|d| d.outside) { 0.4 } else { 0.85 });
        }
        if grouped.contains(&t.id) {
            let g = group.get_or_insert_with(|| {
                Node::row()
                    .gap(2.0)
                    .padding(Edges::all(2.0))
                    .radius(8.0)
                    .bg(c.text.with_alpha(0.035))
                    .border(1.0, c.border_strong)
                    .tooltip(tr("Onglets en multivue"))
            });
            g.push(tab);
        } else {
            row.push(tab);
        }
        // Aperçu au survol d'un onglet inactif (après le délai des info-bulles) : vignette, nom, dossier.
        if !active
            && dragging.is_none()
            && ui.hovered(tid)
            && let (Some(ph), Some(r)) = (t.thumb, ui.rect_of(tid))
        {
            let shown = ui.hover_duration() > 0.45;
            if !shown {
                ui.keep_animating();
            }
            let appear = ui.anim(tid.with("aperçu"), if shown { 1.0 } else { 0.0 }, 120.0);
            if appear > 0.01 {
                preview(ui, t, ph, r, appear);
            }
        }
    }
    flush(&mut row, &mut group);
    // Onglet glissé au-dessus de la vue : son nom suit le pointeur.
    if let (Some(d), Some(p)) = (s.tab_drag.filter(|d| d.outside), ui.input.pointer)
        && let Some(t) = s.tabs.iter().find(|t| t.id == d.id)
    {
        let ghost = Node::row()
            .anchor(obv_ui::Anchor { left: Some(p.x + 12.0), top: Some(p.y + 10.0), ..Default::default() })
            .height(Size::Px(28.0))
            .padding(Edges::xy(10.0, 0.0))
            .gap(6.0)
            .radius(6.0)
            .bg(c.floating)
            .border(1.0, c.border_strong)
            .shadow(24.0, obv_ui::vec2(0.0, 8.0), c.shadow)
            .child(Node::icon("file", 14.0, c.text_secondary))
            .child(Node::text(t.name.as_str(), style, c.text));
        ui.overlay(Node::stack().child(ghost), obv_ui::Layer::Popup);
    }
    let nid = new_tab_id();
    row.push(Node::new().size(Size::Px(4.0), Size::Px(1.0)));
    row.push(
        widgets::small_icon_button(ui, nid, "plus", tr("Ouvrir"), c.icon)
            .tooltip_shortcut(tr("Ouvrir"), "Ctrl T")
            .size(Size::Px(28.0), Size::Px(28.0)),
    );
    row
}

/// Carte d'aperçu sous l'onglet `r` : vignette 240 × 150 sur le fond de la vue, nom, dossier.
fn preview(ui: &mut Ui, t: &TabInfo, photo: obv_ui::PhotoId, r: obv_ui::Rect, appear: f32) {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let (w, h) = (240.0, 150.0);
    let folder = t.path.as_ref().and_then(|p| p.parent()).map(|d| d.display().to_string()).unwrap_or_default();
    let card = Node::column()
        .anchor(obv_ui::Anchor {
            left: Some(r.min.x),
            top: Some(r.max.y + 6.0 - (1.0 - appear) * 4.0),
            ..Default::default()
        })
        .opacity(appear)
        .width(Size::Px(w + 12.0))
        .padding(Edges::all(6.0))
        .gap(6.0)
        .radius(m.radius_panel)
        .bg(c.floating)
        .border(1.0, c.border_strong)
        .shadow(32.0, obv_ui::vec2(0.0, 12.0), c.shadow)
        .child(
            Node::stack()
                .size(Size::Px(w), Size::Px(h))
                .radius(6.0)
                .bg(obv_ui::Fill::Vertical(c.viewport_top, c.viewport_bottom))
                .child(Node::photo(photo).fill().radius(6.0)),
        )
        .child(
            Node::column()
                .padding(Edges { left: 4.0, right: 4.0, top: 0.0, bottom: 2.0 })
                .gap(2.0)
                .width(Size::Fill(1.0))
                .child(
                    Node::text(t.name.as_str(), TextStyle::new(m.font_size, Weight::Medium), c.text)
                        .width(Size::Fill(1.0))
                        .truncate(),
                )
                .child(
                    Node::text(folder, TextStyle::new(11.5, Weight::Regular), c.text_tertiary)
                        .width(Size::Fill(1.0))
                        .truncate(),
                ),
        );
    ui.overlay(Node::stack().child(card), obv_ui::Layer::Popup);
}
