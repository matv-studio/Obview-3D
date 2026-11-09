//! Composants d'Obview, d'après la charte (section 6). Chacun lit l'état d'interaction de l'image précédente et
//! renvoie un nœud. Aucune valeur en dur hors des jetons du thème.

use crate::{Align, Anchor, Color, Corners, Edges, Id, Justify, Node, Rect, Size, TextStyle, Ui, Weight, vec2};

pub fn text_style(ui: &Ui) -> TextStyle {
    TextStyle::new(ui.theme.size.font_size, Weight::Regular)
}

pub fn label(ui: &Ui, text: impl Into<String>) -> Node {
    Node::text(text, text_style(ui), ui.theme.color.text)
}

pub fn label_secondary(ui: &Ui, text: impl Into<String>) -> Node {
    Node::text(text, text_style(ui), ui.theme.color.text_secondary)
}

/// Bouton-icône 28 × 28 (`.icon-btn`) : sans bordure, gris atténué, survol plus clair. `active` : bascule ou
/// outil en cours, en bleu fonctionnel « mode actif ».
pub fn icon_button(
    ui: &mut Ui,
    id: Id,
    icon: &'static str,
    tooltip: &str,
    shortcut: Option<&str>,
    active: bool,
) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 120.0);
    let (bg, fg) = if active {
        (c.set_bg, c.set_text)
    } else {
        let bg = Color::TRANSPARENT.mix(c.raised_hover, hover);
        let bg = if ui.pressed(id) { c.surface_active } else { bg };
        (bg, c.icon.mix(c.text, hover))
    };
    let n = Node::stack()
        .id(id)
        .size(Size::Px(m.control_height), Size::Px(m.control_height))
        .radius(m.radius_control)
        .bg(bg)
        .child(Node::icon(icon, m.icon, fg));
    match shortcut {
        Some(s) => n.tooltip_shortcut(tooltip, s),
        None => n.tooltip(tooltip),
    }
}

/// Petit bouton-icône (24 px) pour les en-têtes de panneau et les lignes.
pub fn small_icon_button(ui: &mut Ui, id: Id, icon: &'static str, tooltip: &str, color: Color) -> Node {
    let c = ui.theme.color.clone();
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 120.0);
    let bg = if ui.pressed(id) { c.surface_active } else { Color::TRANSPARENT.mix(c.raised_hover, hover) };
    Node::stack()
        .id(id)
        .size(Size::Px(24.0), Size::Px(24.0))
        .radius(ui.theme.size.radius_control)
        .bg(bg)
        .tooltip(tooltip)
        .child(Node::icon(icon, ui.theme.size.icon, color.mix(c.text, hover)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// `.btn--primary` : une seule par zone.
    Primary,
    /// `.btn` : bordure, fond relevé.
    Secondary,
}

/// Bouton 28 px, `padding: 0 12px`, rayon 6, graisse moyenne, icône facultative à gauche.
pub fn button(ui: &mut Ui, id: Id, text: &str, icon: Option<&'static str>, kind: ButtonKind, tooltip: &str) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 120.0);
    let pressed = ui.pressed(id);
    let (bg, fg, border) = match kind {
        ButtonKind::Primary => {
            let bg = if pressed { c.accent_press } else { c.accent.mix(c.accent_hover, hover) };
            (bg, c.accent_text, bg)
        }
        ButtonKind::Secondary => {
            let bg = if pressed { c.surface_active } else { c.raised.mix(c.raised_hover, hover) };
            (bg, c.text, c.border_strong.mix(c.text_tertiary, hover))
        }
    };
    let mut n = Node::row()
        .id(id)
        .tooltip(tooltip)
        .height(Size::Px(m.control_height))
        .padding(Edges::xy(m.space_m, 0.0))
        .gap(6.0)
        .radius(m.radius_control)
        .bg(bg)
        .border(1.0, border);
    if let Some(i) = icon {
        n = n.child(Node::icon(i, m.icon, fg));
    }
    n.child(Node::text(text, TextStyle::new(m.font_size, Weight::Medium), fg))
}

/// Séparateur vertical d'une barre d'outils.
pub fn toolbar_separator(ui: &Ui) -> Node {
    Node::new().size(Size::Px(1.0), Size::Px(16.0)).bg(ui.theme.color.border_strong)
}

/// Choix exclusif (`.segmented`) : `padding: 2px; gap: 2px`, fond `--bg`, bordure, rayon 6 ; boutons de 22 px,
/// rayon 4 ; actif : `--surface-active`. Aucun segment actif si `selected` est hors limites.
pub fn tabs(ui: &mut Ui, base: Id, items: &[&str], selected: usize) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let mut row = Node::row()
        .gap(2.0)
        .height(Size::Px(m.control_height))
        .padding(Edges::all(2.0))
        .radius(m.radius_control)
        .bg(c.input_bg)
        .border(1.0, c.border);
    for (i, t) in items.iter().enumerate() {
        let id = base.with(i);
        let sel = i == selected;
        let hover = ui.anim(id.with("survol"), if ui.hovered(id) && !sel { 1.0 } else { 0.0 }, 120.0);
        let bg = if sel { c.segment_selected } else { Color::TRANSPARENT.mix(c.raised_hover, hover) };
        row.push(
            Node::stack()
                .id(id)
                .width(Size::Fill(1.0))
                .height(Size::Px(22.0))
                .padding(Edges::xy(m.space_m, 0.0))
                .radius(4.0)
                .bg(bg)
                .child(Node::text(
                    *t,
                    TextStyle::new(m.font_size_small, Weight::Medium),
                    if sel { c.text } else { c.text_secondary.mix(c.text, hover) },
                )),
        );
    }
    row
}

/// Ligne de l'arbre des pièces.
pub struct TreeRow<'a> {
    pub id: Id,
    pub depth: usize,
    /// `None` : feuille ; `Some(ouvert)` : nœud avec enfants.
    pub expanded: Option<bool>,
    pub icon: &'static str,
    pub label: &'a str,
    pub detail: Option<&'a str>,
    pub selected: bool,
    pub visible: bool,
}

pub fn tree_row(ui: &mut Ui, r: TreeRow<'_>) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let hovered = ui.hovered(r.id) || ui.hovered(r.id.with("œil")) || ui.hovered(r.id.with("flèche"));
    let hover = ui.anim(r.id.with("survol"), if hovered { 1.0 } else { 0.0 }, 120.0);
    let bg = if r.selected { c.accent_soft } else { Color::TRANSPARENT.mix(c.raised_hover, hover) };
    let dim = if r.visible { 1.0 } else { 0.45 };
    let text_color = if r.selected { c.selection_text } else { c.text };
    let mut row = Node::row()
        .id(r.id)
        .height(Size::Px(m.row_height))
        .width(Size::Fill(1.0))
        .padding(Edges { left: 6.0 + r.depth as f32 * 16.0, right: 4.0, top: 0.0, bottom: 0.0 })
        .radius(m.radius_control)
        .bg(bg)
        .gap(6.0);
    row = row.child(match r.expanded {
        Some(open) => Node::stack().id(r.id.with("flèche")).size(Size::Px(16.0), Size::Px(16.0)).child(Node::icon(
            if open { "chevron-down" } else { "chevron-right" },
            m.icon,
            c.text_tertiary,
        )),
        None => Node::new().size(Size::Px(16.0), Size::Px(16.0)),
    });
    row = row
        .child(Node::icon(r.icon, m.icon, if r.selected { c.text } else { c.text_secondary }).opacity(dim))
        .child(Node::text(r.label, text_style(ui), text_color).width(Size::Fill(1.0)).truncate().opacity(dim));
    if let Some(d) = r.detail {
        row = row.child(Node::text(d, TextStyle::new(m.font_size_label, Weight::Medium), c.text_tertiary).opacity(dim));
    }
    let eye_id = r.id.with("œil");
    if hovered || !r.visible {
        let ec = if ui.hovered(eye_id) { c.text } else { c.text_tertiary };
        row = row.child(
            Node::stack()
                .id(eye_id)
                .tooltip(if r.visible { "Masquer" } else { "Afficher" })
                .size(Size::Px(24.0), Size::Px(24.0))
                .radius(m.radius_control)
                .child(Node::icon(if r.visible { "eye" } else { "eye-off" }, m.icon, ec)),
        );
    } else {
        row = row.child(Node::new().size(Size::Px(24.0), Size::Px(24.0)));
    }
    row
}

/// Titre de partie d'un panneau (`h3` : 12 px, graisse moyenne, gris atténué).
pub fn section_header(ui: &Ui, title: &str) -> Node {
    let m = &ui.theme.size;
    Node::row()
        .height(Size::Px(32.0))
        .padding(Edges { left: m.space_m, right: m.space_m, top: 6.0, bottom: 0.0 })
        .child(Node::text(title, TextStyle::new(m.font_size_small, Weight::Medium), ui.theme.color.text_secondary))
}

/// Ligne « libellé : valeur » de l'inspecteur ; chiffres alignés (valeur à droite).
pub fn property_row(ui: &Ui, label: &str, value: impl Into<String>) -> Node {
    let c = &ui.theme.color;
    let m = &ui.theme.size;
    Node::row()
        .height(Size::Px(m.row_height))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(m.space_m, 0.0))
        .child(Node::text(label, text_style(ui), c.text_secondary).width(Size::Px(104.0)).truncate())
        .child(Node::text(value, text_style(ui), c.text).width(Size::Fill(1.0)).truncate())
}

/// Pastille de couleur avec sa valeur.
pub fn color_property(ui: &Ui, label: &str, color: Color, value: &str) -> Node {
    let c = &ui.theme.color;
    let m = &ui.theme.size;
    Node::row()
        .height(Size::Px(m.row_height))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(m.space_m, 0.0))
        .child(Node::text(label, text_style(ui), c.text_secondary).width(Size::Px(104.0)))
        .child(
            Node::row()
                .gap(8.0)
                .child(
                    Node::new().size(Size::Px(14.0), Size::Px(14.0)).radius(4.0).bg(color).border(1.0, c.border_strong),
                )
                .child(Node::text(value, text_style(ui), c.text)),
        )
}

/// Rayon du haut de l'onglet et des deux raccords incurvés de la barre du bas (charte : 8 px).
const TAB_RADIUS: f32 = 8.0;

/// Barre d'outils en onglet du cadre (D-25) : posée sur le bord bas du plateau, couleur du cadre, raccordée au
/// bord par deux coins incurvés et contournée par le trait du plateau, comme l'onglet de vue actif de la charte.
/// À placer au-dessus du contour du plateau (dessinée après lui) ; ancrée en bas au centre de son parent, dont
/// le bas doit coïncider avec celui du plateau.
pub fn docked_toolbar(ui: &Ui, children: Vec<Node>) -> Node {
    let c = &ui.theme.color;
    let (chrome, line) = (c.app_bg, c.stage_line);
    let shape = Node::custom(move |p, r| paint_tab(p.dl, r, chrome, line)).fill();
    let row = Node::row()
        .padding(Edges { left: 6.0, right: 6.0, top: 6.0, bottom: 2.0 })
        .gap(2.0)
        .align(Align::Center)
        .children(children);
    Node::stack().anchor(Anchor { bottom: Some(0.0), center_x: true, ..Default::default() }).child(shape).child(row)
}

/// Onglet `r` montant du bord bas du plateau (bas de `r`) : remplissage `chrome` qui efface le contour sous lui,
/// raccords concaves de chaque côté, puis contour d'un pixel côté plateau (`line`).
pub fn paint_tab(dl: &mut crate::DrawList, r: Rect, chrome: Color, line: Color) {
    let (x0, x1, y0, yb) = (r.min.x, r.max.x, r.min.y, r.max.y);
    let (rt, rf) = (TAB_RADIUS, TAB_RADIUS);
    let top = Corners { tl: rt, tr: rt, br: 0.0, bl: 0.0 };
    dl.rect(Rect::new(vec2(x0, y0), vec2(x1, yb)), top, chrome, None);
    // Raccords : hors du coin arrondi d'une grande boîte qui finit au pied de l'onglet, dans un carré de rf.
    let big = 4096.0;
    let t = rf + 4.0;
    for (square, boxr) in [
        (Rect::new(vec2(x0 - rf, yb - rf), vec2(x0, yb)), Rect::new(vec2(x0 - big, yb - big), vec2(x0, yb))),
        (Rect::new(vec2(x1, yb - rf), vec2(x1 + rf, yb)), Rect::new(vec2(x1, yb - big), vec2(x1 + big, yb))),
    ] {
        dl.push_clip(square);
        dl.rect(boxr.expand(t), rf + t, Color::TRANSPARENT, Some((t, chrome)));
        dl.rect(boxr, rf, Color::TRANSPARENT, Some((1.0, line)));
        dl.pop_clip();
    }
    // Contour des flancs et du haut, à l'extérieur de l'onglet, jusqu'au début des raccords.
    dl.push_clip(Rect::new(vec2(x0 - 1.0, y0 - 1.0), vec2(x1 + 1.0, yb - rf)));
    let ring = Corners { tl: rt + 1.0, tr: rt + 1.0, br: 0.0, bl: 0.0 };
    dl.rect(Rect::new(vec2(x0 - 1.0, y0 - 1.0), vec2(x1 + 1.0, yb + 1.0)), ring, Color::TRANSPARENT, Some((1.0, line)));
    dl.pop_clip();
}

/// Panneau latéral : colonne plate dans le plateau, séparée de la vue par un trait (D-24).
pub fn panel(ui: &Ui, width: f32) -> Node {
    let c = &ui.theme.color;
    Node::column().width(Size::Px(width)).height(Size::Fill(1.0)).bg(c.panel).clip()
}

pub fn divider(ui: &Ui) -> Node {
    Node::new().height(Size::Px(1.0)).width(Size::Fill(1.0)).bg(ui.theme.color.divider)
}

pub fn row_between(children: Vec<Node>) -> Node {
    Node::row().width(Size::Fill(1.0)).justify(Justify::SpaceBetween).align(Align::Center).children(children)
}
