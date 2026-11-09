//! Fenêtre des réglages : rubriques à gauche, réglages groupés en cartes à droite, effet immédiat.

use obv_ui::{
    ACCENT_PRESETS, Align, Color, Edges, Id, Justify, Layer, Node, Size, TextStyle, Ui, Weight,
    controls::{self, TextInput},
    widgets::{self, ButtonKind},
};

use crate::{
    settings::{NavAction, NavScheme, Scope, Settings, ThemeChoice, Units, gesture_keys, gestures},
    shell::AppState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Navigation,
    Display,
}

impl Section {
    const ALL: [Section; 3] = [Section::Appearance, Section::Navigation, Section::Display];
    fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Apparence",
            Section::Navigation => "Navigation",
            Section::Display => "Affichage",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Section::Appearance => "appearance",
            Section::Navigation => "mouse",
            Section::Display => "display",
        }
    }
}

const ROOT: &str = "réglages";

fn id(name: &str) -> Id {
    Id::new(ROOT).with(name)
}

fn card(ui: &Ui, rows: Vec<Node>) -> Node {
    let c = &ui.theme.color;
    let mut col = Node::column().width(Size::Fill(1.0)).radius(10.0).bg(c.raised.mul_alpha(0.55)).border(1.0, c.border);
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        col.push(r);
        if i + 1 < n {
            col.push(
                Node::row()
                    .height(Size::Px(1.0))
                    .padding(Edges::xy(14.0, 0.0))
                    .child(Node::new().height(Size::Px(1.0)).width(Size::Fill(1.0)).bg(c.divider)),
            );
        }
    }
    col
}

fn setting_row(ui: &Ui, label: &str, control: Node) -> Node {
    Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(48.0))
        .padding(Edges::xy(14.0, 0.0))
        .justify(Justify::SpaceBetween)
        .child(Node::text(label, TextStyle::new(13.0, Weight::Regular), ui.theme.color.text))
        .child(control)
}

fn section_title(ui: &Ui, title: &str) -> Node {
    Node::row()
        .height(Size::Px(28.0))
        .padding(Edges { left: 2.0, right: 0.0, top: 0.0, bottom: 2.0 })
        .child(Node::text(title, TextStyle::new(12.0, Weight::SemiBold), ui.theme.color.text_secondary))
}

/// Commande segmentée (thème).
fn segmented(ui: &mut Ui, base: Id, items: &[&str], selected: usize, width: f32) -> (Node, Option<usize>) {
    let mut chosen = None;
    for i in 0..items.len() {
        if ui.clicked(base.with(i)) {
            chosen = Some(i);
        }
    }
    let n = widgets::tabs(ui, base, items, chosen.unwrap_or(selected)).width(Size::Px(width));
    (n, chosen)
}

/// Pastille de touche : « Maj », « Milieu ».
fn key_chip(ui: &Ui, text: &str) -> Node {
    let c = &ui.theme.color;
    Node::stack()
        .height(Size::Px(22.0))
        .padding(Edges::xy(7.0, 0.0))
        .radius(5.0)
        .bg(c.panel)
        .border(1.0, c.border_strong)
        .shadow(0.5, obv_ui::vec2(0.0, 1.0), c.border_strong.mul_alpha(0.8))
        .child(Node::text(text, TextStyle::new(11.5, Weight::Medium), c.text_secondary))
}

fn gesture_cell(ui: &Ui, scheme: NavScheme, action: NavAction) -> Node {
    let c = &ui.theme.color;
    let mut row = Node::row().gap(6.0);
    let list: Vec<_> = gestures(scheme).iter().filter(|(a, _)| *a == action).collect();
    for (k, (_, gs)) in list.iter().enumerate() {
        if k > 0 {
            row.push(Node::text("ou", TextStyle::new(11.5, Weight::Regular), c.text_tertiary));
        }
        for (j, key) in gesture_keys(gs).into_iter().enumerate() {
            if j > 0 {
                row.push(Node::text("+", TextStyle::new(11.5, Weight::Medium), c.text_tertiary));
            }
            row.push(key_chip(ui, key));
        }
    }
    if action == NavAction::Zoom {
        if !list.is_empty() {
            row.push(Node::text("ou", TextStyle::new(11.5, Weight::Regular), c.text_tertiary));
        }
        row.push(key_chip(ui, "Molette"));
    }
    row
}

fn appearance(ui: &mut Ui, s: &mut AppState) -> Vec<Node> {
    let st = &mut s.settings;
    let theme_idx = match st.theme {
        ThemeChoice::Dark => 0,
        ThemeChoice::Light => 1,
        ThemeChoice::System => 2,
    };
    let (seg, picked) = segmented(ui, id("thème"), &["Sombre", "Clair", "Système"], theme_idx, 228.0);
    if let Some(i) = picked {
        st.theme = [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System][i];
        s.settings_dirty = true;
    }
    let mut swatches = Node::row().gap(2.0);
    let (none, clicked) = controls::swatch_none(ui, id("accent aucune"), s.settings.accent.is_none(), "Aucune");
    if clicked {
        s.settings.accent = None;
        s.accent_hex.clear();
        s.settings_dirty = true;
    }
    swatches.push(none);
    for (i, (hex, name)) in ACCENT_PRESETS.iter().enumerate() {
        let (n, clicked) =
            controls::swatch(ui, id("accent").with(i), Color::hex(*hex), s.settings.accent == Some(*hex), name);
        if clicked {
            s.settings.accent = Some(*hex);
            s.accent_hex = format!("{hex:06X}");
            s.settings_dirty = true;
        }
        swatches.push(n);
    }
    let hex_id = id("accent hex");
    if !ui.has_focus(hex_id) && !ui.clicked(hex_id) {
        s.accent_hex = s.settings.accent.map(|a| format!("{a:06X}")).unwrap_or_default();
    }
    let (input, changed) = controls::text_input(
        ui,
        TextInput {
            id: hex_id,
            placeholder: "RRGGBB",
            width: Size::Px(112.0),
            icon: None,
            hint: None,
            clearable: false,
        },
        &mut s.accent_hex,
    );
    if changed && let Some(col) = Color::parse_hex(&s.accent_hex) {
        s.settings.accent = Some(col.to_hex());
        s.settings_dirty = true;
    }
    let c = ui.theme.color.clone();
    let dot = match s.settings.accent {
        Some(a) => {
            Node::new().size(Size::Px(16.0), Size::Px(16.0)).radius(4.0).bg(Color::hex(a)).border(1.0, c.border_strong)
        }
        None => Node::new().size(Size::Px(16.0), Size::Px(16.0)).radius(4.0).border(1.0, c.border_strong),
    };
    let scope_idx = match s.settings.accent_scope {
        Scope::Minimal => 0,
        Scope::Standard => 1,
        Scope::Extended => 2,
    };
    let (scope, picked) = segmented(ui, id("étendue"), &["Minimale", "Standard", "Étendue"], scope_idx, 252.0);
    if let Some(i) = picked
        && s.settings.accent.is_some()
    {
        s.settings.accent_scope = [Scope::Minimal, Scope::Standard, Scope::Extended][i];
        s.settings_dirty = true;
    }
    let scope = if s.settings.accent.is_some() {
        scope.id(id("étendue info")).tooltip("Minimale : focus et contour de la vue. Standard : + action principale, interrupteurs, sélection. Étendue : + onglets, choix, lignes actives")
    } else {
        scope.opacity(0.45).id(id("étendue info")).tooltip("Choisir d'abord une couleur d'accent")
    };
    vec![
        section_title(ui, "Interface"),
        card(
            ui,
            vec![
                setting_row(ui, "Thème", seg),
                setting_row(ui, "Couleur d'accent", swatches),
                setting_row(ui, "Couleur personnalisée", Node::row().gap(8.0).child(dot).child(input)),
                setting_row(ui, "Étendue de l'accent", scope),
            ],
        ),
    ]
}

fn navigation(ui: &mut Ui, s: &mut AppState) -> Vec<Node> {
    let labels: Vec<&str> = NavScheme::ALL.iter().map(|n| n.label()).collect();
    let cur = NavScheme::ALL.iter().position(|n| *n == s.settings.navigation).unwrap_or(0);
    let (sel, picked) = controls::select(ui, id("schéma"), &labels, cur, 180.0);
    if let Some(i) = picked {
        s.settings.navigation = NavScheme::ALL[i];
        s.settings_dirty = true;
    }
    let scheme = s.settings.navigation;
    let mut map_rows = vec![setting_row(ui, "Schéma", sel)];
    for a in NavAction::ALL {
        map_rows.push(setting_row(ui, a.label(), gesture_cell(ui, scheme, a)));
    }
    let mut toggles = Vec::new();
    let mut tog = |ui: &mut Ui, name: &str, label: &str, v: &mut bool, dirty: &mut bool| {
        let (n, ch) = controls::toggle(ui, id(name), *v);
        if let Some(nv) = ch {
            *v = nv;
            *dirty = true;
        }
        toggles.push(setting_row(ui, label, n));
    };
    let mut dirty = false;
    tog(ui, "inverser", "Inverser le zoom", &mut s.settings.invert_zoom, &mut dirty);
    tog(ui, "vers pointeur", "Zoomer vers le pointeur", &mut s.settings.zoom_to_cursor, &mut dirty);
    tog(ui, "pivot", "Pivoter autour du point visé", &mut s.settings.orbit_around_cursor, &mut dirty);
    let (sl, v) = controls::slider(ui, id("vitesse"), s.settings.zoom_speed, 0.25, 4.0, 0.05, 160.0);
    if let Some(v) = v {
        s.settings.zoom_speed = v;
        dirty = true;
    }
    let speed = Node::row().gap(10.0).child(sl).child(
        Node::text(
            format!("{:.2}×", s.settings.zoom_speed).replace('.', ","),
            TextStyle::new(12.0, Weight::Medium),
            ui.theme.color.text_secondary,
        )
        .width(Size::Px(40.0)),
    );
    toggles.push(setting_row(ui, "Vitesse du zoom", speed));
    s.settings_dirty |= dirty;
    vec![
        section_title(ui, "Souris"),
        card(ui, map_rows),
        Node::new().height(Size::Px(14.0)),
        section_title(ui, "Comportement"),
        card(ui, toggles),
    ]
}

fn display(ui: &mut Ui, s: &mut AppState) -> Vec<Node> {
    let labels: Vec<&str> = Units::ALL.iter().map(|u| u.label()).collect();
    let cur = Units::ALL.iter().position(|u| *u == s.settings.units).unwrap_or(0);
    let (sel, picked) = controls::select(ui, id("unités"), &labels, cur, 180.0);
    if let Some(i) = picked {
        s.settings.units = Units::ALL[i];
        s.settings_dirty = true;
    }
    let (cube, ch) = controls::toggle(ui, id("cube"), s.settings.view_cube);
    if let Some(v) = ch {
        s.settings.view_cube = v;
        s.settings_dirty = true;
    }
    vec![
        section_title(ui, "Vue 3D"),
        card(ui, vec![setting_row(ui, "Unités", sel), setting_row(ui, "Cube de vue", cube)]),
    ]
}

/// Ajoute la fenêtre des réglages en calque modal si elle est ouverte.
pub fn dialog(ui: &mut Ui, s: &mut AppState) {
    if !s.settings_open {
        return;
    }
    let c = ui.theme.color.clone();
    let mut nav = Node::column()
        .width(Size::Px(196.0))
        .height(Size::Fill(1.0))
        .padding(Edges::all(10.0))
        .gap(2.0)
        .bg(c.raised.mul_alpha(0.35))
        .border_right(c.divider);
    for (i, sec) in Section::ALL.iter().enumerate() {
        let sid = id("rubrique").with(i);
        if ui.clicked(sid) {
            s.settings_section = i;
        }
        let sel = s.settings_section == i;
        let bg =
            if sel { c.accent_soft } else { ui.state_color(sid, Color::TRANSPARENT, c.raised_hover, c.raised_hover) };
        nav.push(
            Node::row()
                .id(sid)
                .height(Size::Px(32.0))
                .width(Size::Fill(1.0))
                .padding(Edges::xy(10.0, 0.0))
                .gap(10.0)
                .radius(7.0)
                .bg(bg)
                .child(Node::icon(sec.icon(), 15.0, if sel { c.accent } else { c.text_secondary }))
                .child(Node::text(
                    sec.label(),
                    TextStyle::new(13.0, Weight::Medium),
                    if sel { c.selection_text } else { c.text },
                )),
        );
    }
    let section = Section::ALL[s.settings_section.min(Section::ALL.len() - 1)];
    let rows = match section {
        Section::Appearance => appearance(ui, s),
        Section::Navigation => navigation(ui, s),
        Section::Display => display(ui, s),
    };
    let content = Node::scroll(id("défilement"))
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
        .padding(Edges { left: 24.0, right: 20.0, top: 18.0, bottom: 20.0 })
        .gap(4.0)
        .align(Align::Stretch)
        .children(rows);
    let body = Node::row().align(Align::Stretch).child(nav).child(content);
    let reset_id = id("rétablir");
    if ui.clicked(reset_id) {
        s.settings = Settings::default();
        s.settings_dirty = true;
    }
    let footer = vec![
        widgets::button(ui, reset_id, "Rétablir", Some("reset"), ButtonKind::Secondary, "Valeurs par défaut"),
        Node::spacer(),
        widgets::button(ui, id("terminé"), "Terminé", None, ButtonKind::Primary, "Fermer les réglages"),
    ];
    if ui.clicked(id("terminé")) {
        s.settings_open = false;
    }
    let (node, close) = controls::modal(ui, Id::new(ROOT), "Réglages", (760.0, 620.0), body, footer);
    if close {
        s.settings_open = false;
    }
    ui.overlay(node, Layer::Modal);
}
