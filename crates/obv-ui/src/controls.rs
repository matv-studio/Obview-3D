//! Commandes maison : interrupteur, liste de choix, curseur, pastilles de couleur, champ de saisie, fenêtre
//! modale, liste virtuelle. Chaque commande lit l'état de l'image précédente et renvoie un nœud, plus la
//! nouvelle valeur quand l'utilisateur l'a changée.

use std::ops::Range;

use crate::{
    Align, Anchor, Color, Cursor, Edges, Id, Justify, Layer, Node, Rect, Size, TextStyle, Ui, Weight, vec2,
    widgets::text_style,
};

/// Interrupteur 32 × 18, bouton rond animé.
pub fn toggle(ui: &mut Ui, id: Id, on: bool) -> (Node, Option<bool>) {
    let c = ui.theme.color.clone();
    let clicked = ui.clicked(id);
    let value = if clicked { !on } else { on };
    let t = ui.anim(id.with("position"), if value { 1.0 } else { 0.0 }, 140.0);
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 90.0);
    let off = c.track.mix(c.border_strong, hover * 0.6);
    let on_c = c.accent.mix(c.accent_hover, hover);
    let track = off.mix(on_c, t);
    let knob = c.knob;
    let shadow = c.shadow;
    let node = Node::custom(move |p, r| {
        let tr = Rect::from_xywh(r.min.x, r.center().y - 9.0, 32.0, 18.0);
        p.dl.rect(tr, 9.0, track, None);
        let x = tr.min.x + 2.0 + t * 14.0;
        let k = Rect::from_xywh(x, tr.min.y + 2.0, 14.0, 14.0);
        p.dl.shadow(k, 7.0, 2.0, vec2(0.0, 1.0), shadow);
        p.dl.rect(k, 7.0, knob, None);
    })
    .id(id)
    .cursor(Cursor::Pointer)
    .size(Size::Px(32.0), Size::Px(20.0));
    (node, clicked.then_some(value))
}

/// Case à cocher 16 × 16.
pub fn checkbox(ui: &mut Ui, id: Id, on: bool) -> (Node, Option<bool>) {
    let c = ui.theme.color.clone();
    let clicked = ui.clicked(id);
    let value = if clicked { !on } else { on };
    let hover = ui.hovered(id);
    let mut n = Node::stack().id(id).cursor(Cursor::Pointer).size(Size::Px(16.0), Size::Px(16.0)).radius(4.0);
    if value {
        n = n.bg(if hover { c.accent_hover } else { c.accent }).child(Node::icon("check", 12.0, c.accent_text));
    } else {
        n = n.bg(c.input_bg).border(1.0, if hover { c.text_tertiary } else { c.border_strong });
    }
    (n, clicked.then_some(value))
}

/// Liste de choix : bouton qui affiche la valeur et ouvre un menu sous lui.
pub fn select(ui: &mut Ui, id: Id, options: &[&str], selected: usize, width: f32) -> (Node, Option<usize>) {
    let c = ui.theme.color.clone();
    let mut chosen = None;
    for i in 0..options.len() {
        if ui.clicked(id.with(i)) {
            chosen = Some(i);
            ui.close_popup();
        }
    }
    if ui.clicked(id) {
        ui.toggle_popup(id);
    }
    let open = ui.popup_open(id);
    let bg = if open { c.raised_hover } else { ui.state_color(id, c.raised, c.raised_hover, c.raised_hover) };
    let current = options.get(chosen.unwrap_or(selected)).copied().unwrap_or("");
    let trigger = Node::row()
        .id(id)
        .cursor(Cursor::Pointer)
        .width(Size::Px(width))
        .height(Size::Px(28.0))
        .padding(Edges { left: 10.0, right: 6.0, top: 0.0, bottom: 0.0 })
        .gap(6.0)
        .radius(ui.theme.size.radius_control)
        .bg(bg)
        .border(1.0, if open { c.accent } else { c.border })
        .child(Node::text(current, text_style(ui), c.text).width(Size::Fill(1.0)).truncate())
        .child(Node::icon("chevrons-up-down", 13.0, c.text_tertiary));

    if open && let Some(r) = ui.rect_of(id) {
        let item_h = 28.0;
        let h = options.len() as f32 * item_h + 8.0;
        let below = r.max.y + 4.0 + h <= ui.input.size.y - 8.0;
        let top = if below { r.max.y + 4.0 } else { r.min.y - 4.0 - h };
        let mut menu = Node::column()
            .anchor(Anchor { left: Some(r.min.x), top: Some(top), ..Default::default() })
            .width(Size::Px(r.width().max(160.0)))
            .padding(Edges::all(4.0))
            .radius(8.0)
            .bg(c.floating.with_alpha(1.0))
            .border(1.0, c.border)
            .shadow(18.0, vec2(0.0, 6.0), c.shadow)
            .shadow(2.0, vec2(0.0, 1.0), c.shadow.mul_alpha(0.5));
        let sel = chosen.unwrap_or(selected);
        for (i, o) in options.iter().enumerate() {
            let iid = id.with(i);
            let hover = ui.hovered(iid);
            let is_sel = i == sel;
            menu.push(
                Node::row()
                    .id(iid)
                    .cursor(Cursor::Pointer)
                    .height(Size::Px(item_h))
                    .padding(Edges::xy(8.0, 0.0))
                    .gap(8.0)
                    .radius(5.0)
                    .bg(if hover { c.accent } else { Color::TRANSPARENT })
                    .child(Node::stack().size(Size::Px(14.0), Size::Px(14.0)).children(
                        is_sel.then(|| Node::icon("check", 13.0, if hover { c.accent_text } else { c.text })),
                    ))
                    .child(
                        Node::text(*o, text_style(ui), if hover { c.accent_text } else { c.text })
                            .width(Size::Fill(1.0))
                            .truncate(),
                    ),
            );
        }
        ui.overlay(Node::stack().child(menu), Layer::Popup);
    }
    (trigger, chosen)
}

/// Curseur horizontal. `step` : pas de la valeur (0 : continu).
pub fn slider(ui: &mut Ui, id: Id, value: f32, min: f32, max: f32, step: f32, width: f32) -> (Node, Option<f32>) {
    let c = ui.theme.color.clone();
    let mut v = value;
    let mut changed = None;
    if let (Some(p), Some(r)) = (ui.dragging(id), ui.rect_of(id)) {
        let t = ((p.x - r.min.x - 7.0) / (r.width() - 14.0)).clamp(0.0, 1.0);
        let mut nv = min + t * (max - min);
        if step > 0.0 {
            nv = (nv / step).round() * step;
        }
        if (nv - value).abs() > f32::EPSILON {
            changed = Some(nv);
        }
        v = nv;
    }
    let t = if max > min { ((v - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let hot = ui.hovered(id) || ui.is_active(id);
    let (track, fill, knob, ring, shadow) = (c.track, c.accent, c.knob, c.focus_ring, c.shadow);
    let node = Node::custom(move |p, r| {
        let y = r.center().y;
        let tr = Rect::from_xywh(r.min.x + 7.0, y - 2.0, r.width() - 14.0, 4.0);
        p.dl.rect(tr, 2.0, track, None);
        let x = tr.min.x + tr.width() * t;
        p.dl.rect(Rect::from_xywh(tr.min.x, tr.min.y, x - tr.min.x, 4.0), 2.0, fill, None);
        let k = Rect::from_xywh(x - 7.0, y - 7.0, 14.0, 14.0);
        if hot {
            p.dl.rect(k.expand(3.0), 10.0, ring.mul_alpha(0.6), None);
        }
        p.dl.shadow(k, 7.0, 3.0, vec2(0.0, 1.0), shadow);
        p.dl.rect(k, 7.0, knob, Some((1.0, fill.mul_alpha(0.0))));
    })
    .id(id)
    .cursor(Cursor::Pointer)
    .size(Size::Px(width), Size::Px(20.0));
    (node, changed)
}

/// Pastille de couleur ronde, anneau d'accent quand elle est choisie.
pub fn swatch(ui: &mut Ui, id: Id, color: Color, selected: bool, tooltip: &str) -> (Node, bool) {
    let c = ui.theme.color.clone();
    let clicked = ui.clicked(id);
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 90.0);
    let panel = c.panel;
    let edge = c.border_strong.mul_alpha(0.6);
    let node = Node::custom(move |p, r| {
        let cr = Rect::from_xywh(r.center().x - 11.0, r.center().y - 11.0, 22.0, 22.0);
        if selected {
            p.dl.rect(cr.expand(3.0), 14.0, color, None);
            p.dl.rect(cr.expand(1.0), 12.0, panel, None);
        }
        let s = 1.0 + hover * 0.08;
        let inner = Rect::from_xywh(cr.center().x - 9.0 * s, cr.center().y - 9.0 * s, 18.0 * s, 18.0 * s);
        p.dl.rect(inner, 9.0 * s, color, Some((1.0, edge)));
    })
    .id(id)
    .cursor(Cursor::Pointer)
    .tooltip(tooltip)
    .size(Size::Px(30.0), Size::Px(30.0));
    (node, clicked)
}

/// Champ de saisie d'une ligne : curseur clignotant, sélection, défilement horizontal du texte.
pub struct TextInput<'a> {
    pub id: Id,
    pub placeholder: &'a str,
    pub width: Size,
    pub icon: Option<&'static str>,
    /// Indication de raccourci affichée à droite quand le champ est vide.
    pub hint: Option<&'a str>,
    /// Bouton d'effacement quand le champ n'est pas vide.
    pub clearable: bool,
}

pub fn text_input(ui: &mut Ui, cfg: TextInput<'_>, text: &mut String) -> (Node, bool) {
    let id = cfg.id;
    // Clic : curseur au caractère le plus proche ; glisser : sélection ; double clic : tout.
    if let (Some(p), Some(r)) = (ui.dragging(id), ui.rect_of(id)) {
        let left = r.min.x + 8.0 + if cfg.icon.is_some() { 20.0 } else { 0.0 };
        let style = text_style(ui);
        let scale = ui.input.scale;
        let mut best = (f32::MAX, 0);
        let mut prefix = String::new();
        for (k, ch) in std::iter::once(None).chain(text.chars().map(Some)).enumerate() {
            if let Some(ch) = ch {
                prefix.push(ch);
            }
            let w = if prefix.is_empty() { 0.0 } else { ui.fonts.layout(&prefix, style, scale).width };
            let d = (left + w - p.x).abs();
            if d < best.0 {
                best = (d, k);
            }
        }
        let mut st = ui.edit_state(id);
        st.caret = best.1;
        if ui.press_started(id) {
            st.anchor = best.1;
        }
        ui.set_edit_state(id, st);
    }
    if ui.double_clicked(id) {
        ui.set_edit_state(id, crate::EditState { caret: text.chars().count(), anchor: 0 });
    }
    let changed = ui.text_edit(id, text);
    let c = ui.theme.color.clone();
    let focus = ui.has_focus(id);
    let hover = ui.anim(id.with("survol"), if ui.hovered(id) { 1.0 } else { 0.0 }, 90.0);
    let st = ui.edit_state(id);
    let caret_on = focus && ui.caret_visible();
    let style = text_style(ui);
    let value = text.clone();
    let empty = value.is_empty();
    let placeholder = cfg.placeholder.to_string();
    let (fg, ph, sel_bg, caret) = (c.text, c.text_tertiary, c.accent_soft.mul_alpha(1.6), c.accent);
    let field = Node::custom(move |p, r| {
        if empty {
            if !focus || !placeholder.is_empty() {
                let l = p.res.fonts.layout_truncated(&placeholder, style, p.dl.scale, r.width());
                let y = r.min.y + ((r.height() - l.height) * 0.5).round();
                p.res.fonts.draw(p.dl, p.res.atlas, &l, vec2(r.min.x, y), ph);
            }
            if caret_on {
                p.dl.rect(Rect::from_xywh(r.min.x, r.center().y - 7.5, 1.5, 15.0), 0.0, caret, None);
            }
            return;
        }
        let prefix = |p: &mut crate::Painter<'_>, n: usize| -> f32 {
            let s: String = value.chars().take(n).collect();
            p.res.fonts.layout(&s, style, p.dl.scale).width
        };
        let cx = prefix(p, st.caret);
        let shift = (cx - r.width() + 2.0).max(0.0);
        p.dl.push_clip(r);
        if focus && st.caret != st.anchor {
            let (a, b) = (st.caret.min(st.anchor), st.caret.max(st.anchor));
            let (xa, xb) = (prefix(p, a), prefix(p, b));
            p.dl.rect(Rect::from_xywh(r.min.x + xa - shift, r.center().y - 9.0, xb - xa, 18.0), 2.0, sel_bg, None);
        }
        let l = p.res.fonts.layout(&value, style, p.dl.scale);
        let y = r.min.y + ((r.height() - l.height) * 0.5).round();
        p.res.fonts.draw(p.dl, p.res.atlas, &l, vec2(r.min.x - shift, y), fg);
        if caret_on {
            p.dl.rect(Rect::from_xywh(r.min.x + cx - shift, r.center().y - 7.5, 1.5, 15.0), 0.0, caret, None);
        }
        p.dl.pop_clip();
    })
    .width(Size::Fill(1.0))
    .height(Size::Fill(1.0));
    let border = if focus { c.accent } else { c.border.mix(c.border_strong, hover) };
    let mut n = Node::row()
        .id(id)
        .cursor(Cursor::Text)
        .width(cfg.width)
        .height(Size::Px(28.0))
        .padding(Edges::xy(8.0, 0.0))
        .gap(6.0)
        .radius(ui.theme.size.radius_control)
        .bg(c.input_bg)
        .border(1.0, border);
    if focus {
        n = n.shadow(0.5, vec2(0.0, 0.0), c.focus_ring).shadow(3.0, vec2(0.0, 0.0), c.focus_ring.mul_alpha(0.5));
    }
    if let Some(i) = cfg.icon {
        n = n.child(Node::icon(i, 14.0, if focus { c.text_secondary } else { c.text_tertiary }));
    }
    n = n.child(field);
    if empty
        && !focus
        && let Some(h) = cfg.hint
    {
        n = n.child(Node::stack().padding(Edges::xy(5.0, 1.0)).radius(4.0).border(1.0, c.border).child(Node::text(
            h,
            TextStyle::new(10.5, Weight::Medium),
            c.text_tertiary,
        )));
    } else if !empty && cfg.clearable {
        let cid = id.with("effacer");
        if ui.clicked(cid) {
            text.clear();
            ui.focus(id);
        }
        let cc = if ui.hovered(cid) { c.text } else { c.text_tertiary };
        n = n.child(
            Node::stack()
                .id(cid)
                .cursor(Cursor::Pointer)
                .tooltip("Effacer")
                .size(Size::Px(18.0), Size::Px(18.0))
                .radius(9.0)
                .child(Node::icon("x", 12.0, cc)),
        );
    }
    let cleared = ui.clicked(id.with("effacer"));
    (n, changed || cleared)
}

/// Fenêtre modale centrée : titre, bouton de fermeture, corps, pied facultatif. Renvoie aussi « fermer ».
pub fn modal(ui: &mut Ui, id: Id, title: &str, size: (f32, f32), body: Node, footer: Vec<Node>) -> (Node, bool) {
    let c = ui.theme.color.clone();
    let close_id = id.with("fermer");
    let close = ui.clicked(close_id) || ui.escape_pressed();
    let appear = ui.anim(id.with("apparition"), 1.0, 160.0);
    let cc = if ui.hovered(close_id) { c.text } else { c.text_secondary };
    let close_bg = ui.state_color(close_id, Color::TRANSPARENT, c.raised_hover, c.border_strong);
    let header = Node::row()
        .height(Size::Px(48.0))
        .width(Size::Fill(1.0))
        .padding(Edges { left: 18.0, right: 10.0, top: 0.0, bottom: 0.0 })
        .border_bottom(c.divider)
        .child(Node::text(title, TextStyle::new(14.0, Weight::SemiBold), c.text).width(Size::Fill(1.0)))
        .child(
            Node::stack()
                .id(close_id)
                .cursor(Cursor::Pointer)
                .tooltip_shortcut("Fermer", "Échap")
                .size(Size::Px(28.0), Size::Px(28.0))
                .radius(6.0)
                .bg(close_bg)
                .child(Node::icon("x", 16.0, cc)),
        );
    let mut card = Node::column()
        .anchor(Anchor {
            center_x: true,
            top: Some(((ui.input.size.y - size.1) * 0.5).round().max(16.0) + (1.0 - appear) * 10.0),
            ..Default::default()
        })
        .size(Size::Px(size.0), Size::Px(size.1.min(ui.input.size.y - 32.0)))
        .radius(12.0)
        .bg(c.panel)
        .border(1.0, c.border)
        .shadow(48.0, vec2(0.0, 18.0), c.shadow.mul_alpha(1.2))
        .shadow(4.0, vec2(0.0, 1.0), c.shadow.mul_alpha(0.6))
        .opacity(appear)
        .clip()
        .child(header)
        .child(body.width(Size::Fill(1.0)).height(Size::Fill(1.0)));
    if !footer.is_empty() {
        card = card.child(
            Node::row()
                .height(Size::Px(56.0))
                .width(Size::Fill(1.0))
                .padding(Edges::xy(16.0, 0.0))
                .gap(8.0)
                .justify(Justify::End)
                .align(Align::Center)
                .border_top(c.divider)
                .children(footer),
        );
    }
    (Node::stack().id(id).child(card), close)
}

/// Lignes visibles d'une liste virtuelle de `count` lignes de `row_h` dans la zone défilante `scroll`.
/// Renvoie la plage à construire ; les espaces avant et après sont des nœuds vides de la bonne hauteur.
pub fn virtual_range(ui: &Ui, scroll: Id, row_h: f32, count: usize) -> (Range<usize>, f32, f32) {
    let st = ui.scrolls.get(&scroll).copied().unwrap_or_default();
    let view = if st.view > 0.0 { st.view } else { 2000.0 };
    let overscan = 6;
    let first = ((st.offset / row_h).floor() as usize).saturating_sub(overscan).min(count);
    let last = (((st.offset + view) / row_h).ceil() as usize + overscan).min(count);
    (first..last, first as f32 * row_h, (count - last) as f32 * row_h)
}

/// Fait défiler `scroll` pour que la ligne `index` soit visible.
pub fn scroll_into_view(ui: &mut Ui, scroll: Id, row_h: f32, index: usize) {
    if let Some(st) = ui.scrolls.get_mut(&scroll) {
        let top = index as f32 * row_h;
        if top < st.target {
            st.target = top;
        } else if top + row_h > st.target + st.view {
            st.target = top + row_h - st.view;
        }
    }
}
