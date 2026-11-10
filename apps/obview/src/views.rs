//! Onglet « Vues » (D-26) : vues standard et vues enregistrées par fichier (nom, cadrage, projection), gardées
//! dans le dossier de configuration (`vues.json`).

use std::{collections::BTreeMap, path::PathBuf};

use obv_render::Camera;
use obv_ui::{Color, Edges, Id, Node, Size, TextStyle, Ui, Weight, controls, tr, widgets};
use serde::{Deserialize, Serialize};

use crate::{navigation, shell::AppState};

/// Vue enregistrée.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub name: String,
    pub target: [f64; 3],
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub ortho: bool,
}

impl SavedView {
    pub fn from_camera(name: String, c: &Camera) -> Self {
        SavedView { name, target: c.target, distance: c.distance, yaw: c.yaw, pitch: c.pitch, ortho: c.ortho }
    }
    pub fn apply(&self, c: &Camera) -> Camera {
        Camera {
            target: self.target,
            distance: self.distance,
            yaw: self.yaw,
            pitch: self.pitch,
            ortho: self.ortho,
            ..*c
        }
    }
}

/// Toutes les vues enregistrées, par fichier.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ViewStore(pub BTreeMap<String, Vec<SavedView>>);

impl ViewStore {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("Obview 3D").join("vues.json"))
    }
    pub fn load() -> ViewStore {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> std::io::Result<()> {
        let Some(p) = Self::path() else { return Ok(()) };
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, p)
    }
}

/// État de l'onglet : vues du fichier ouvert, renommage en cours.
#[derive(Clone, Debug, Default)]
pub struct ViewsState {
    pub list: Vec<SavedView>,
    /// Vue en cours de renommage et texte saisi.
    pub renaming: Option<(usize, String)>,
    /// Liste modifiée depuis le dernier enregistrement.
    pub dirty: bool,
}

const STANDARD: [(&str, navigation::View); 7] = [
    ("Avant", navigation::View::Front),
    ("Arrière", navigation::View::Back),
    ("Gauche", navigation::View::Left),
    ("Droite", navigation::View::Right),
    ("Dessus", navigation::View::Top),
    ("Dessous", navigation::View::Bottom),
    ("Isométrique", navigation::View::Iso),
];

fn id(name: &str) -> Id {
    Id::new("vues").with(name)
}

fn row(ui: &mut Ui, rid: Id, icon: &'static str, label: Node, trailing: Option<Node>) -> Node {
    let c = ui.theme.color.clone();
    let hover = ui.anim(rid.with("survol"), if ui.hovered(rid) { 1.0 } else { 0.0 }, 120.0);
    Node::row()
        .id(rid)
        .width(Size::Fill(1.0))
        .height(Size::Px(28.0))
        .padding(Edges { left: 8.0, right: 4.0, top: 0.0, bottom: 0.0 })
        .gap(8.0)
        .radius(6.0)
        .bg(Color::TRANSPARENT.mix(c.raised_hover, hover))
        .child(Node::icon(icon, 15.0, c.text_secondary))
        .child(label.width(Size::Fill(1.0)))
        .children(trailing)
}

/// Contenu de l'onglet « Vues ».
pub fn tab(ui: &mut Ui, s: &mut AppState) -> Vec<Node> {
    let c = ui.theme.color.clone();
    let text = TextStyle::new(ui.theme.size.font_size, Weight::Regular);
    let mut out = vec![widgets::section_header(ui, tr("Standard"))];
    for (i, (name, v)) in STANDARD.iter().enumerate() {
        let rid = id("standard").with(i);
        if ui.clicked(rid) {
            let (yaw, pitch) = v.angles();
            s.camera_request = Some(Camera { yaw, pitch, ..s.camera });
        }
        let label = Node::text(tr(name), text, c.text);
        out.push(row(ui, rid, "projection", label, None));
    }
    out.push(Node::new().height(Size::Px(8.0)));
    out.push(widgets::section_header(ui, tr("Enregistrées")));

    let mut remove = None;
    for i in 0..s.views.list.len() {
        let rid = id("vue").with(i);
        let del = rid.with("supprimer");
        if ui.clicked(del) {
            remove = Some(i);
            continue;
        }
        if ui.double_clicked(rid) {
            s.views.renaming = Some((i, s.views.list[i].name.clone()));
            ui.focus(id("nom"));
        } else if ui.clicked(rid) && s.views.renaming.as_ref().is_none_or(|(k, _)| *k != i) {
            s.camera_request = Some(s.views.list[i].apply(&s.camera));
        }
        let editing = s.views.renaming.as_ref().is_some_and(|(k, _)| *k == i);
        let label = if editing {
            let mut t = s.views.renaming.as_ref().map(|(_, t)| t.clone()).unwrap_or_default();
            let (input, _) = controls::text_input(
                ui,
                controls::TextInput {
                    id: id("nom"),
                    placeholder: tr("Nom"),
                    width: Size::Fill(1.0),
                    icon: None,
                    hint: None,
                    clearable: false,
                },
                &mut t,
            );
            if let Some((_, cur)) = &mut s.views.renaming {
                *cur = t;
            }
            input
        } else {
            Node::text(s.views.list[i].name.clone(), text, c.text).truncate()
        };
        let hovered = ui.hovered(rid) || ui.hovered(del);
        let trailing = (!editing).then(|| {
            widgets::small_icon_button(ui, del, "close", tr("Supprimer la vue"), c.icon).opacity(if hovered {
                1.0
            } else {
                0.0
            })
        });
        out.push(row(ui, rid, "bookmark", label, trailing).tooltip(tr("Clic : afficher · double-clic : renommer")));
    }
    // Fin du renommage : Entrée, Échap ou clic ailleurs.
    if let Some((k, t)) = s.views.renaming.clone() {
        let enter = ui.input.keys.contains(&obv_ui::Key::Enter);
        let lost = !ui.text_focused();
        if enter || lost {
            if let Some(v) = s.views.list.get_mut(k)
                && !t.trim().is_empty()
            {
                v.name = t.trim().to_owned();
                s.views.dirty = true;
            }
            s.views.renaming = None;
            ui.blur();
        }
    }
    if let Some(i) = remove {
        s.views.list.remove(i);
        s.views.renaming = None;
        s.views.dirty = true;
    }
    let add = id("ajouter");
    if ui.clicked(add) {
        let base = tr("Vue");
        let n = (1..).find(|n| !s.views.list.iter().any(|v| v.name == format!("{base} {n}"))).unwrap_or(1);
        s.views.list.push(SavedView::from_camera(format!("{base} {n}"), &s.camera));
        s.views.dirty = true;
    }
    out.push(Node::new().height(Size::Px(4.0)));
    out.push(
        widgets::button(
            ui,
            add,
            tr("Enregistrer la vue"),
            Some("plus"),
            widgets::ButtonKind::Secondary,
            tr("Vue actuelle"),
        )
        .width(Size::Fill(1.0)),
    );
    out
}
