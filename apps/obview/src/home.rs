//! Page d'accueil (D-41) : fichiers récents en cartes (vignette, format, taille, date), recherche, ouverture.
//!
//! Épinglés en tête ; au survol d'une carte, épingler et retirer ; un fichier disparu reste grisé, avec son
//! chemin en info-bulle. Un fichier glissé sur la fenêtre s'ouvre comme ailleurs.

use std::path::{Path, PathBuf};

use obv_ui::{
    Anchor, Color, Edges, Id, Justify, Node, Size, TextStyle, Ui, Weight,
    controls::{self, TextInput},
    i18n, tr,
    widgets::{self, ButtonKind},
};

use crate::{convert_ui::Request, recent, shell::AppState};

fn id(name: &str) -> Id {
    Id::new("accueil").with(name)
}

pub fn card_id(p: &Path) -> Id {
    id("carte").with(p.to_string_lossy().as_ref())
}

/// Largeur la plus petite d'une carte, écart entre cartes, largeur maximale du contenu.
const CARD_MIN: f32 = 240.0;
const GAP: f32 = 16.0;
const CONTENT_MAX: f32 = 1240.0;
/// Rapport hauteur / largeur des vignettes (16:10, celui des captures de vignettes).
pub const THUMB_RATIO: f32 = 0.625;

/// Taille lisible d'un fichier (« 1,2 Mo » / « 1.2 MB »).
pub fn file_size(bytes: u64) -> String {
    let (v, unit) = match bytes {
        0..1_000 => return i18n::with_unit(&bytes.to_string(), i18n::pick("B", "o")),
        1_000..1_000_000 => (bytes as f64 / 1e3, i18n::pick("KB", "ko")),
        1_000_000..1_000_000_000 => (bytes as f64 / 1e6, i18n::pick("MB", "Mo")),
        _ => (bytes as f64 / 1e9, i18n::pick("GB", "Go")),
    };
    let digits = if v < 10.0 { 1 } else { 0 };
    i18n::with_unit(&i18n::decimal(v, digits), unit)
}

/// Date relative d'une ouverture (`then`, `now` : secondes depuis 1970).
pub fn when(then: u64, now: u64) -> String {
    if then == 0 {
        return "—".into();
    }
    let d = now.saturating_sub(then);
    let fr = i18n::is_fr();
    match d {
        0..60 => i18n::pick("Just now", "À l'instant").into(),
        60..3600 => {
            let n = d / 60;
            if fr { format!("Il y a {n} min") } else { format!("{n} min ago") }
        }
        3600..86_400 => {
            let n = d / 3600;
            if fr { format!("Il y a {n} h") } else { format!("{n} h ago") }
        }
        86_400..172_800 => i18n::pick("Yesterday", "Hier").into(),
        172_800..604_800 => {
            let n = d / 86_400;
            if fr { format!("Il y a {n} jours") } else { format!("{n} days ago") }
        }
        _ => date(then, fr),
    }
}

/// Date courte (« 12 nov. 2026 » / « Nov 12, 2026 »), calendrier civil UTC.
fn date(t: u64, fr: bool) -> String {
    // Jours depuis 1970 → année, mois, jour (algorithme de H. Hinnant).
    let z = (t / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    const FR: [&str; 12] =
        ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let m = (month - 1) as usize;
    if fr { format!("{day} {} {year}", FR[m]) } else { format!("{} {day}, {year}", EN[m]) }
}

/// Clics de la page (avant sa construction, comme le reste de la coque).
pub fn handle(ui: &mut Ui, s: &mut AppState) {
    if ui.clicked(id("ouvrir")) {
        s.requests.push(Request::OpenFile);
    }
    let paths: Vec<PathBuf> = s.recent.iter().map(|r| r.path.clone()).collect();
    let mut changed = false;
    for p in paths {
        let cid = card_id(&p);
        if ui.clicked(cid.with("épingle")) {
            recent::toggle_pin(&mut s.recent, &p);
            changed = true;
        } else if ui.clicked(cid.with("retirer")) {
            recent::remove(&mut s.recent, &p);
            changed = true;
        } else if ui.clicked(cid) && p.exists() {
            s.requests.push(Request::OpenPath(p));
        }
    }
    if changed && let Err(e) = recent::save(&s.recent) {
        eprintln!("Obview 3D : fichiers récents : {e}");
    }
}

/// Page entière, à la place de la vue 3D et des panneaux.
pub fn page(ui: &mut Ui, s: &mut AppState) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let stage = s.stage.get();
    let width = (stage.width() - 2.0 * 80.0).clamp(CARD_MIN, CONTENT_MAX);
    let cols = (((width + GAP) / (CARD_MIN + GAP)).floor() as usize).clamp(1, 6);
    let card_w = ((width - GAP * (cols - 1) as f32) / cols as f32).floor();

    let (search, _) = controls::text_input(
        ui,
        TextInput {
            id: Id::new("recherche"),
            placeholder: tr("Rechercher"),
            width: Size::Px(320.0),
            icon: Some("search"),
            hint: Some("Ctrl F"),
            clearable: true,
        },
        &mut s.home_search,
    );
    let open =
        widgets::button(ui, id("ouvrir"), tr("Ouvrir…"), Some("open"), ButtonKind::Primary, tr("Ouvrir un fichier"))
            .tooltip_shortcut(tr("Ouvrir un fichier"), "Ctrl O");
    let head = Node::row().width(Size::Fill(1.0)).justify(Justify::SpaceBetween).child(search).child(open);

    let filter = s.home_search.trim().to_lowercase();
    let now = recent::now();
    let items: Vec<recent::Recent> = recent::ordered(&s.recent)
        .into_iter()
        .filter(|r| filter.is_empty() || r.name().to_lowercase().contains(&filter))
        .cloned()
        .collect();
    let mut body = Node::column().width(Size::Px(width)).gap(20.0).child(head);
    if s.recent.is_empty() {
        // Aucun fichier encore ouvert : la zone d'ouverture (cliquer ou déposer) tient lieu de liste.
        body.push(Node::new().height(Size::Px(60.0)));
        body.push(
            Node::row().width(Size::Fill(1.0)).justify(Justify::Center).children(crate::loading::drop_zone(ui, s)),
        );
    } else {
        body.push(Node::text(tr("Récents"), TextStyle::new(m.font_size, Weight::Medium), c.text_secondary));
        let mut grid = Node::column().width(Size::Fill(1.0)).gap(GAP);
        for line in items.chunks(cols) {
            let mut row = Node::row().gap(GAP);
            for r in line {
                row.push(card(ui, s, r, card_w, now));
            }
            grid.push(row);
        }
        body.push(grid);
    }
    let scroll = Node::scroll(id("défilement"))
        .fill()
        .padding(Edges { left: 0.0, right: 0.0, top: 56.0, bottom: 56.0 })
        .child(Node::row().width(Size::Fill(1.0)).justify(Justify::Center).child(body));
    Node::stack().fill().child(scroll)
}

/// Carte d'un fichier récent.
fn card(ui: &mut Ui, s: &AppState, r: &recent::Recent, w: f32, now: u64) -> Node {
    let c = ui.theme.color.clone();
    let m = ui.theme.size.clone();
    let cid = card_id(&r.path);
    let meta = std::fs::metadata(&r.path).ok();
    let exists = meta.as_ref().is_some_and(|m| m.is_file());
    let hot = ui.hovered(cid) || ui.hovered(cid.with("épingle")) || ui.hovered(cid.with("retirer"));
    let hover = ui.anim(cid.with("survol"), if hot { 1.0 } else { 0.0 }, 120.0);
    let tw = w - 16.0;
    let th = (tw * THUMB_RATIO).round();
    let photo = s.thumbs.get(&r.path).copied();
    // Fond de la vue du thème courant derrière la vignette (rendue sur fond transparent).
    let mut thumb = Node::stack()
        .size(Size::Px(tw), Size::Px(th))
        .radius(6.0)
        .bg(obv_ui::Fill::Vertical(c.viewport_top, c.viewport_bottom))
        .border(1.0, c.border);
    thumb.push(match (exists, photo) {
        (true, Some(ph)) => Node::photo(ph).fill().radius(6.0),
        (true, None) => Node::icon("file", 28.0, c.text_tertiary),
        (false, _) => Node::icon("alert", 22.0, c.text_tertiary),
    });
    // Épingle toujours visible sur un fichier épinglé ; épingler et retirer au survol.
    let pin_id = cid.with("épingle");
    let pin_tip = tr(if r.pinned { "Désépingler" } else { "Épingler" });
    let small = |ui: &mut Ui, bid: Id, icon: &'static str, tip: &str, on: bool| {
        let bh = ui.anim(bid.with("survol"), if ui.hovered(bid) { 1.0 } else { 0.0 }, 120.0);
        Node::stack()
            .id(bid)
            .size(Size::Px(24.0), Size::Px(24.0))
            .radius(6.0)
            .bg(c.floating.mix(c.raised_hover, bh))
            .border(1.0, c.border)
            .tooltip(tip)
            .child(Node::icon(icon, 13.0, if on { c.text } else { c.text_secondary.mix(c.text, bh) }))
    };
    if hover > 0.01 {
        let buttons = Node::row()
            .gap(4.0)
            .opacity(hover)
            .anchor(Anchor { top: Some(8.0), right: Some(8.0), ..Default::default() })
            .child(small(ui, pin_id, "pin", pin_tip, r.pinned))
            .child(small(ui, cid.with("retirer"), "close", tr("Retirer de la liste"), false));
        thumb.push(buttons);
    }
    if r.pinned && hover < 0.99 {
        thumb.push(
            Node::stack()
                .size(Size::Px(24.0), Size::Px(24.0))
                .opacity(1.0 - hover)
                .anchor(Anchor { top: Some(8.0), left: Some(8.0), ..Default::default() })
                .child(Node::icon("pin", 13.0, c.text_secondary)),
        );
    }
    let badge = Node::stack()
        .height(Size::Px(18.0))
        .padding(Edges::xy(5.0, 0.0))
        .radius(4.0)
        .border(1.0, c.border_strong)
        .child(Node::text(recent::kind(&r.path), TextStyle::new(10.5, Weight::SemiBold), c.text_secondary));
    let size = meta.filter(|_| exists).map(|m| file_size(m.len())).unwrap_or_else(|| "—".into());
    let info = Node::row().gap(6.0).child(badge).child(Node::text(
        format!("{size} · {}", when(r.opened, now)),
        TextStyle::new(12.0, Weight::Regular),
        c.text_secondary,
    ));
    Node::column()
        .id(cid)
        .width(Size::Px(w))
        .padding(Edges::all(8.0))
        .gap(8.0)
        .radius(10.0)
        .bg(Color::TRANSPARENT.mix(c.raised_hover, hover * 0.7))
        .opacity(if exists { 1.0 } else { 0.5 })
        .tooltip(r.path.to_string_lossy().into_owned())
        .child(thumb)
        .child(
            Node::column()
                .gap(4.0)
                .padding(Edges::xy(2.0, 0.0))
                .width(Size::Fill(1.0))
                .child(
                    Node::text(r.name(), TextStyle::new(m.font_size, Weight::Medium), c.text)
                        .width(Size::Fill(1.0))
                        .truncate(),
                )
                .child(info),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_et_tailles() {
        i18n::set_lang(i18n::Lang::Fr);
        assert_eq!(when(1000, 1030), "À l'instant");
        assert_eq!(when(1000, 1000 + 125), "Il y a 2 min");
        assert_eq!(when(1000, 1000 + 90_000), "Hier");
        // 12 novembre 2026 à midi UTC.
        assert_eq!(date(1_794_484_800, true), "12 nov. 2026");
        assert_eq!(date(1_794_484_800, false), "Nov 12, 2026");
        assert!(file_size(1_234_567).starts_with("1,2"));
        i18n::set_lang(i18n::Lang::En);
    }
}
