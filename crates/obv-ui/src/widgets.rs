//! Composants d'Obview. Chacun lit l'état d'interaction de l'image précédente et renvoie un nœud.

use crate::{Align, Anchor, Color, Edges, Id, Justify, Node, Size, TextStyle, Ui, Weight, vec2};

pub fn text_style(ui: &Ui) -> TextStyle {
    TextStyle::new(ui.theme.size.font_size, Weight::Regular)
}

pub fn label(ui: &Ui, text: impl Into<String>) -> Node {
    Node::text(text, text_style(ui), ui.theme.color.text)
}

pub fn label_secondary(ui: &Ui, text: impl Into<String>) -> Node {
    Node::text(text, TextStyle::new(ui.theme.size.font_size, Weight::Regular), ui.theme.color.text_secondary)
}

/// Bouton carré à icône, pour les barres d'outils. `active` : outil en cours.
pub fn icon_button(
    ui: &mut Ui,
    id: Id,
    icon: &'static str,
    tooltip: &str,
    shortcut: Option<&str>,
    active: bool,
) -> Node {
    let c = ui.theme.color.clone();
    let bg = if active {
        c.accent_soft
    } else {
        ui.state_color(id, Color::TRANSPARENT, c.raised_hover, c.raised_hover.mix(c.border_strong, 0.5))
    };
    let fg = if active { c.accent } else { c.icon };
    let mut n =
        Node::stack().id(id).size(Size::Px(32.0), Size::Px(32.0)).radius(8.0).bg(bg).child(Node::icon(icon, 18.0, fg));
    n = match shortcut {
        Some(s) => n.tooltip_shortcut(tooltip, s),
        None => n.tooltip(tooltip),
    };
    n
}

/// Petit bouton à icône (24 px) pour les en-têtes de panneau et les lignes.
pub fn small_icon_button(ui: &mut Ui, id: Id, icon: &'static str, tooltip: &str, color: Color) -> Node {
    let c = ui.theme.color.clone();
    let bg = ui.state_color(id, Color::TRANSPARENT, c.raised_hover, c.border_strong);
    Node::stack()
        .id(id)
        .size(Size::Px(24.0), Size::Px(24.0))
        .radius(6.0)
        .bg(bg)
        .tooltip(tooltip)
        .child(Node::icon(icon, 15.0, color))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
}

pub fn button(ui: &mut Ui, id: Id, text: &str, icon: Option<&'static str>, kind: ButtonKind, tooltip: &str) -> Node {
    let c = ui.theme.color.clone();
    let (bg, fg, border) = match kind {
        ButtonKind::Primary => (ui.state_color(id, c.accent, c.accent_hover, c.accent_hover), c.accent_text, None),
        ButtonKind::Secondary => (
            ui.state_color(id, c.raised, c.raised_hover, c.raised_hover.mix(c.border_strong, 0.5)),
            c.text,
            Some(c.border),
        ),
    };
    let mut n = Node::row()
        .id(id)
        .tooltip(tooltip)
        .height(Size::Px(ui.theme.size.control_height))
        .padding(Edges::xy(if icon.is_some() { 10.0 } else { 12.0 }, 0.0))
        .gap(6.0)
        .radius(ui.theme.size.radius_control)
        .bg(bg);
    if let Some(b) = border {
        n = n.border(1.0, b);
    }
    if let Some(i) = icon {
        n = n.child(Node::icon(i, 15.0, fg));
    }
    n.child(Node::text(text, TextStyle::new(ui.theme.size.font_size, Weight::Medium), fg))
}

/// Séparateur vertical d'une barre d'outils.
pub fn toolbar_separator(ui: &Ui) -> Node {
    Node::new().size(Size::Px(1.0), Size::Px(20.0)).bg(ui.theme.color.border)
}

/// Champ de recherche (affichage ; la saisie est gérée par l'application).
pub fn search_field(ui: &mut Ui, id: Id, placeholder: &str, value: &str, shortcut: &str) -> Node {
    let c = ui.theme.color.clone();
    let bg = ui.state_color(id, c.raised, c.raised_hover, c.raised_hover);
    let (text, color) = if value.is_empty() { (placeholder, c.text_tertiary) } else { (value, c.text) };
    Node::row()
        .id(id)
        .height(Size::Px(28.0))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(8.0, 0.0))
        .gap(6.0)
        .radius(ui.theme.size.radius_control)
        .bg(bg)
        .child(Node::icon("search", 14.0, c.text_tertiary))
        .child(Node::text(text, text_style(ui), color).width(Size::Fill(1.0)).truncate())
        .child(Node::stack().padding(Edges::xy(5.0, 1.0)).radius(4.0).border(1.0, c.border).child(Node::text(
            shortcut,
            TextStyle::new(10.5, Weight::Medium),
            c.text_tertiary,
        )))
}

/// Onglets texte d'un panneau.
pub fn tabs(ui: &mut Ui, base: Id, items: &[&str], selected: usize) -> Node {
    let c = ui.theme.color.clone();
    let mut row = Node::row().gap(2.0).height(Size::Px(28.0)).padding(Edges::all(2.0)).radius(8.0).bg(c.raised);
    for (i, t) in items.iter().enumerate() {
        let id = base.with(i);
        let sel = i == selected;
        let bg = if sel {
            c.segment_selected
        } else {
            ui.state_color(id, Color::TRANSPARENT, c.raised_hover, c.raised_hover)
        };
        let mut tab =
            Node::stack().id(id).width(Size::Fill(1.0)).height(Size::Fill(1.0)).radius(6.0).bg(bg).child(Node::text(
                *t,
                TextStyle::new(12.0, Weight::Medium),
                if sel { c.text } else { c.text_secondary },
            ));
        if sel {
            tab = tab.shadow(2.0, vec2(0.0, 1.0), c.shadow.mul_alpha(0.6)).border(1.0, c.border.mul_alpha(0.6));
        }
        row.push(tab);
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
    let hovered = ui.hovered(r.id) || ui.hovered(r.id.with("œil")) || ui.hovered(r.id.with("flèche"));
    let bg = if r.selected {
        c.accent_soft
    } else {
        ui.state_color(r.id, Color::TRANSPARENT, c.raised_hover.mul_alpha(0.7), c.raised_hover)
    };
    let dim = if r.visible { 1.0 } else { 0.45 };
    let text_color = if r.selected { c.selection_text } else { c.text };
    let mut row = Node::row()
        .id(r.id)
        .height(Size::Px(ui.theme.size.row_height))
        .width(Size::Fill(1.0))
        .padding(Edges { left: 6.0 + r.depth as f32 * 14.0, right: 4.0, top: 0.0, bottom: 0.0 })
        .radius(6.0)
        .bg(bg)
        .gap(4.0);
    row = row.child(match r.expanded {
        Some(open) => Node::stack().id(r.id.with("flèche")).size(Size::Px(16.0), Size::Px(16.0)).child(Node::icon(
            if open { "chevron-down" } else { "chevron-right" },
            13.0,
            c.text_tertiary,
        )),
        None => Node::new().size(Size::Px(16.0), Size::Px(16.0)),
    });
    row = row
        .child(Node::icon(r.icon, 14.0, if r.selected { c.accent } else { c.text_secondary }).opacity(dim))
        .child(Node::text(r.label, text_style(ui), text_color).width(Size::Fill(1.0)).truncate().opacity(dim));
    if let Some(d) = r.detail {
        row = row.child(Node::text(d, TextStyle::new(11.5, Weight::Regular), c.text_tertiary).opacity(dim));
    }
    let eye_id = r.id.with("œil");
    if hovered || !r.visible {
        let ec = if ui.hovered(eye_id) { c.text } else { c.text_tertiary };
        row = row.child(
            Node::stack()
                .id(eye_id)
                .tooltip(if r.visible { "Masquer" } else { "Afficher" })
                .size(Size::Px(22.0), Size::Px(22.0))
                .radius(5.0)
                .child(Node::icon(if r.visible { "eye" } else { "eye-off" }, 14.0, ec)),
        );
    } else {
        row = row.child(Node::new().size(Size::Px(22.0), Size::Px(22.0)));
    }
    row
}

/// En-tête de section d'un panneau.
pub fn section_header(ui: &Ui, title: &str) -> Node {
    Node::row().height(Size::Px(30.0)).padding(Edges::xy(12.0, 0.0)).child(Node::text(
        title,
        TextStyle::new(12.0, Weight::SemiBold),
        ui.theme.color.text,
    ))
}

/// Ligne « libellé : valeur » du panneau de propriétés.
pub fn property_row(ui: &Ui, label: &str, value: impl Into<String>) -> Node {
    let c = &ui.theme.color;
    Node::row()
        .height(Size::Px(24.0))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(12.0, 0.0))
        .child(Node::text(label, text_style(ui), c.text_secondary).width(Size::Px(96.0)).truncate())
        .child(Node::text(value, text_style(ui), c.text).width(Size::Fill(1.0)).truncate())
}

/// Pastille de couleur avec sa valeur.
pub fn color_property(ui: &Ui, label: &str, color: Color, value: &str) -> Node {
    let c = &ui.theme.color;
    Node::row()
        .height(Size::Px(24.0))
        .width(Size::Fill(1.0))
        .padding(Edges::xy(12.0, 0.0))
        .child(Node::text(label, text_style(ui), c.text_secondary).width(Size::Px(96.0)))
        .child(
            Node::row()
                .gap(8.0)
                .child(
                    Node::new().size(Size::Px(14.0), Size::Px(14.0)).radius(4.0).bg(color).border(1.0, c.border_strong),
                )
                .child(Node::text(value, text_style(ui), c.text)),
        )
}

/// Barre d'outils flottante (pilule) posée en bas au centre de la vue 3D.
pub fn floating_toolbar(ui: &Ui, children: Vec<Node>) -> Node {
    let c = &ui.theme.color;
    Node::row()
        .anchor(Anchor { bottom: Some(16.0), center_x: true, ..Default::default() })
        .padding(Edges::all(4.0))
        .gap(2.0)
        .radius(ui.theme.size.radius_floating)
        .bg(c.floating)
        .border(1.0, c.border)
        .shadow(24.0, vec2(0.0, 8.0), c.shadow)
        .shadow(3.0, vec2(0.0, 1.0), c.shadow.mul_alpha(0.5))
        .children(children)
}

/// Panneau latéral avec en-tête.
pub fn panel(ui: &Ui, width: f32) -> Node {
    Node::column().width(Size::Px(width)).height(Size::Fill(1.0)).bg(ui.theme.color.panel).clip()
}

pub fn divider(ui: &Ui) -> Node {
    Node::new().height(Size::Px(1.0)).width(Size::Fill(1.0)).bg(ui.theme.color.divider)
}

pub fn row_between(children: Vec<Node>) -> Node {
    Node::row().width(Size::Fill(1.0)).justify(Justify::SpaceBetween).align(Align::Center).children(children)
}
