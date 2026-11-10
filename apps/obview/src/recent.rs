//! Fichiers récemment ouverts (D-33) : liste enregistrée à part des réglages (« Rétablir » ne la vide pas), menu
//! du bouton du document dans la barre du haut.

use std::path::{Path, PathBuf};

use obv_ui::{
    Anchor, Color, Edges, Id, Node, Size, TextStyle, Ui, Weight, tr,
    widgets::{self},
};

use crate::{convert_ui::Request, shell::AppState};

/// Nombre de fichiers gardés.
pub const MAX: usize = 12;

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Obview 3D").join("recents.json"))
}

pub fn load() -> Vec<PathBuf> {
    path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<PathBuf>>(&b).ok())
        .unwrap_or_default()
}

pub fn save(list: &[PathBuf]) -> std::io::Result<()> {
    let Some(p) = path() else { return Ok(()) };
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, p)
}

/// Met `file` en tête de la liste (sans doublon, au plus [`MAX`]).
pub fn push(list: &mut Vec<PathBuf>, file: &Path) {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    list.retain(|p| p != &file);
    list.insert(0, file);
    list.truncate(MAX);
}

/// Type affiché d'un fichier : extension en capitales, formes longues ramenées au nom du format.
pub fn kind(p: &Path) -> String {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "stp" | "step" => "STEP".into(),
        "igs" | "iges" => "IGES".into(),
        "gltf" => "glTF".into(),
        "" => "—".into(),
        e => e.to_ascii_uppercase(),
    }
}

/// Identifiant du bouton du document (il porte le menu).
pub fn button_id() -> Id {
    Id::new("fichier")
}

fn id(name: &str) -> Id {
    Id::new("récents").with(name)
}

/// Menu sous le bouton du document : « Ouvrir un fichier » en tête, puis les fichiers récents encore présents,
/// avec leur type ; le dossier en info-bulle.
pub fn popup(ui: &mut Ui, s: &mut AppState) {
    let bid = button_id();
    if ui.clicked(bid) {
        ui.toggle_popup(bid);
    }
    if !ui.popup_open(bid) {
        return;
    }
    let Some(r) = ui.rect_of(bid) else { return };
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let row = |ui: &mut Ui, rid: Id, icon: &'static str, label: Node, trailing: Option<Node>| {
        let hover = ui.anim(rid.with("survol"), if ui.hovered(rid) { 1.0 } else { 0.0 }, 120.0);
        Node::row()
            .id(rid)
            .width(Size::Fill(1.0))
            .height(Size::Px(30.0))
            .padding(Edges { left: 8.0, right: 8.0, top: 0.0, bottom: 0.0 })
            .gap(8.0)
            .radius(6.0)
            .bg(Color::TRANSPARENT.mix(c.raised_hover, hover))
            .child(Node::icon(icon, 15.0, c.text_secondary))
            .child(label.width(Size::Fill(1.0)))
            .children(trailing)
    };
    let text = TextStyle::new(m.font_size, Weight::Regular);
    let open_id = id("ouvrir");
    if ui.clicked(open_id) {
        s.requests.push(Request::OpenFile);
        ui.close_popup();
    }
    let hint = Node::text("Ctrl O", TextStyle::new(m.font_size_small, Weight::Medium), c.text_tertiary);
    let mut rows = vec![row(
        ui,
        open_id,
        "open",
        Node::text(tr("Ouvrir un fichier"), TextStyle::new(m.font_size, Weight::Medium), c.text),
        Some(hint),
    )];
    let files: Vec<PathBuf> = s.recent.iter().filter(|p| p.exists()).cloned().collect();
    if !files.is_empty() {
        rows.push(Node::new().width(Size::Fill(1.0)).height(Size::Px(1.0)).bg(c.divider));
        rows.push(widgets::section_header(ui, tr("Récents")));
    }
    for (i, p) in files.iter().enumerate() {
        let rid = id("fichier").with(i);
        if ui.clicked(rid) {
            s.requests.push(Request::OpenPath(p.clone()));
            ui.close_popup();
        }
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let badge = Node::stack()
            .height(Size::Px(18.0))
            .padding(Edges::xy(6.0, 0.0))
            .radius(4.0)
            .bg(c.input_bg)
            .border(1.0, c.border)
            .child(Node::text(kind(p), TextStyle::new(10.5, Weight::SemiBold), c.text_secondary));
        let folder = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
        rows.push(row(ui, rid, "file", Node::text(name, text, c.text).truncate(), Some(badge)).tooltip(&folder));
    }
    let w = 340.0;
    let appear = ui.appear(id("menu"), 120.0);
    let menu = Node::column()
        .anchor(Anchor { left: Some(r.min.x), top: Some(r.max.y + 6.0 - (1.0 - appear) * 4.0), ..Default::default() })
        .opacity(appear)
        .width(Size::Px(w))
        .padding(Edges::all(6.0))
        .gap(2.0)
        .radius(m.radius_panel)
        .bg(c.raised)
        .border(1.0, c.border_strong)
        .shadow(40.0, obv_ui::vec2(0.0, 16.0), c.shadow)
        .children(rows);
    ui.overlay(Node::stack().child(menu), obv_ui::Layer::Popup);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_sans_doublon() {
        let mut l = vec![PathBuf::from("/a.stl"), PathBuf::from("/b.step")];
        push(&mut l, Path::new("/b.step"));
        assert_eq!(l, vec![PathBuf::from("/b.step"), PathBuf::from("/a.stl")]);
        for i in 0..20 {
            push(&mut l, Path::new(&format!("/f{i}.obj")));
        }
        assert_eq!(l.len(), MAX);
        assert_eq!(kind(Path::new("x.STP")), "STEP");
        assert_eq!(kind(Path::new("x.glb")), "GLB");
    }
}
