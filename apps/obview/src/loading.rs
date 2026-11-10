//! Ouverture d'un fichier : état vide (aucun modèle), avancement d'une lecture longue (D-29).
//!
//! - Sans modèle, la vue montre une zone d'ouverture : icône de dossier en transparence dans un cadre en
//!   pointillés (état vide de la charte), cliquable, mise en avant quand un fichier est glissé sur la fenêtre.
//! - Pendant une lecture : ligne d'activité de 2 px en haut de la fenêtre (charte : activité longue), carte au
//!   centre de la vue avec le nom du fichier, l'étape, la progression, le temps écoulé et l'annulation.

use obv_io::Stage;
use obv_ui::{Anchor, Color, Edges, Id, Node, Rect, Size, TextStyle, Ui, Weight, i18n, tr, vec2, widgets};

use crate::convert_ui::Request;
use crate::shell::{AppState, spinner};

/// Lecture en cours, vue par l'interface.
#[derive(Clone, Debug)]
pub struct Loading {
    /// Nom du fichier.
    pub name: String,
    pub stage: Stage,
    /// Fraction de l'étape (`None` : indéterminée).
    pub fraction: Option<f32>,
    /// Secondes depuis le début de la lecture.
    pub elapsed: f64,
    /// Numéro de la lecture : chaque ouverture repart d'une barre vide.
    pub serial: u64,
}

impl Loading {
    pub fn new(name: String) -> Self {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Loading { name, stage: Stage::Read, fraction: None, elapsed: 0.0, serial }
    }

    /// Avancement global : part de chaque étape dans le temps d'une ouverture STEP type (relevé le 10/11/2026
    /// sur la montre d'essai, 285 pièces, 11,6 s en version optimisée : analyse 11 %, conversion 65 %,
    /// maillage 23 %). `None` tant que l'étape n'est pas mesurable.
    pub fn overall(&self) -> Option<f32> {
        let (start, len) = match self.stage {
            Stage::Read => (0.0, 0.11),
            Stage::Transfer => (0.11, 0.65),
            Stage::Mesh => (0.76, 0.22),
            Stage::Prepare => (0.98, 0.02),
        };
        match (self.stage, self.fraction) {
            (Stage::Prepare, _) => Some(0.99),
            (_, Some(f)) => Some(start + len * f),
            (_, None) => None,
        }
    }
}

/// Avis d'échec (ouverture, capture) : titre fixe (traduit à l'affichage), objet, détail technique.
#[derive(Clone, Debug)]
pub struct Notice {
    pub title: &'static str,
    /// Fichier concerné.
    pub subject: String,
    /// Message de la bibliothèque (début traduit à l'affichage).
    pub detail: String,
}

pub fn notice_close_id() -> Id {
    id("avis fermer")
}

/// Avis d'échec en haut de la vue : icône d'alerte, titre et fichier, détail, fermeture.
pub fn notice(ui: &mut Ui, s: &mut AppState) -> Option<Node> {
    if ui.clicked(notice_close_id()) {
        s.notice = None;
    }
    let n = s.notice.clone()?;
    let c = ui.theme.color.clone();
    let appear = ui.appear(id("avis"), 200.0);
    let detail = i18n::tr_message(&n.detail);
    let text = Node::column()
        .width(Size::Fill(1.0))
        .gap(2.0)
        .child(
            Node::row()
                .gap(6.0)
                .width(Size::Fill(1.0))
                .child(Node::text(tr(n.title), TextStyle::new(13.0, Weight::Medium), c.text))
                .child(
                    Node::text(n.subject, TextStyle::new(13.0, Weight::Regular), c.text_secondary)
                        .width(Size::Fill(1.0))
                        .truncate(),
                ),
        )
        .child(
            Node::text(detail.clone(), TextStyle::new(12.0, Weight::Regular), c.text_secondary)
                .width(Size::Fill(1.0))
                .truncate(),
        );
    Some(
        Node::row()
            .anchor(Anchor { center_x: true, top: Some(12.0 - (1.0 - appear) * 6.0), ..Default::default() })
            .opacity(appear)
            .width(Size::Px(440.0))
            .padding(Edges { left: 12.0, right: 8.0, top: 10.0, bottom: 10.0 })
            .gap(10.0)
            .radius(ui.theme.size.radius_panel)
            .bg(c.floating)
            .border(1.0, c.border_strong)
            .shadow(24.0, vec2(0.0, 10.0), c.shadow)
            .tooltip(detail)
            .child(Node::stack().size(Size::Px(18.0), Size::Px(34.0)).child(Node::icon("alert", 16.0, c.danger)))
            .child(text)
            .child(widgets::small_icon_button(ui, notice_close_id(), "close", tr("Fermer"), c.text_secondary)),
    )
}

/// Nom affiché d'une étape.
pub fn stage_label(s: Stage) -> &'static str {
    tr(match s {
        Stage::Read => "Lecture",
        Stage::Transfer => "Import",
        Stage::Mesh => "Maillage",
        Stage::Prepare => "Préparation",
    })
}

fn id(n: &str) -> Id {
    Id::new("ouverture").with(n)
}

pub fn cancel_id() -> Id {
    id("annuler")
}

pub fn empty_id() -> Id {
    id("zone")
}

/// Couleur de l'activité : l'accent s'il est choisi, sinon le texte (contraste inversé de la charte).
fn activity_color(ui: &Ui) -> Color {
    let c = &ui.theme.color;
    if c.has_accent { c.accent } else { c.text }
}

/// Barre de progression : `fraction` connue (plein jusqu'à elle) ou indéterminée (segment qui parcourt la piste,
/// 1,4 s par aller). `track` : couleur de la piste (transparente pour la ligne d'activité).
fn bar(ui: &mut Ui, key: Id, fraction: Option<f32>, height: f32, track: Color) -> Node {
    let color = activity_color(ui);
    let t = ui.input.time as f32;
    if fraction.is_none() {
        ui.keep_animating();
    }
    // Remplissage lissé : la barre ne saute pas d'une étape à l'autre.
    let shown = fraction.map(|f| ui.anim(key, f, 250.0));
    Node::custom(move |p, r| {
        let rad = height * 0.5;
        if track.a > 0.0 {
            p.dl.rect(r, rad, track, None);
        }
        match shown {
            Some(f) => {
                let w = (r.width() * f.clamp(0.0, 1.0)).max(height);
                p.dl.rect(Rect::from_xywh(r.min.x, r.min.y, w, r.height()), rad, color, None);
            }
            None => {
                // Segment de 30 % qui va et vient, vitesse douce aux extrémités.
                let w = r.width() * 0.3;
                let phase = (t / 1.4).fract();
                let x = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
                let x0 = r.min.x + (r.width() - w) * x;
                p.dl.push_clip(r);
                p.dl.rect(Rect::from_xywh(x0, r.min.y, w, r.height()), rad, color, None);
                p.dl.pop_clip();
            }
        }
    })
    .width(Size::Fill(1.0))
    .height(Size::Px(height))
}

/// Ligne d'activité de 2 px en haut de la fenêtre.
pub fn top_line(ui: &mut Ui, s: &AppState) -> Option<Node> {
    let l = s.loading.as_ref()?;
    let f = l.overall();
    Some(
        Node::stack()
            .anchor(Anchor { left: Some(0.0), right: Some(0.0), top: Some(0.0), ..Default::default() })
            .width(Size::Fill(1.0))
            .height(Size::Px(2.0))
            .child(bar(ui, id("ligne").with(l.serial), f, 2.0, Color::TRANSPARENT)),
    )
}

/// Temps écoulé, à la seconde (« 12 s », « 1 min 05 s »).
fn elapsed(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s < 60 {
        i18n::with_unit(&s.to_string(), "s")
    } else {
        format!("{} {}", i18n::with_unit(&(s / 60).to_string(), "min"), i18n::with_unit(&format!("{:02}", s % 60), "s"))
    }
}

/// Carte d'ouverture au centre de la vue.
pub fn card(ui: &mut Ui, s: &mut AppState) -> Option<Node> {
    let l = s.loading.clone()?;
    let c = ui.theme.color.clone();
    if ui.clicked(cancel_id()) {
        s.requests.push(Request::CancelOpen);
    }
    let appear = ui.appear(id("carte"), 200.0);
    let w = 320.0;
    let percent = l.overall().map(|f| i18n::with_unit(&format!("{:.0}", f * 100.0), "%"));
    let small = TextStyle::new(12.0, Weight::Regular);
    let head = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(28.0))
        .gap(10.0)
        .child(spinner(ui, 16.0, activity_color(ui)))
        .child(
            Node::text(l.name.clone(), TextStyle::new(13.0, Weight::Medium), c.text).width(Size::Fill(1.0)).truncate(),
        )
        .child(widgets::small_icon_button(ui, cancel_id(), "close", tr("Annuler l'ouverture"), c.text_secondary));
    let track = c.track;
    let foot = Node::row()
        .width(Size::Fill(1.0))
        .height(Size::Px(18.0))
        .gap(6.0)
        .child(Node::text(stage_label(l.stage), small, c.text_secondary))
        .children(percent.map(|p| Node::text(p, small, c.text_tertiary)))
        .child(Node::spacer())
        .child(Node::text(elapsed(l.elapsed), small, c.text_tertiary));
    Some(
        Node::column()
            .anchor(Anchor {
                center_x: true,
                top: Some((s.viewport.get().height() * 0.5 - 50.0 + (1.0 - appear) * 8.0).max(12.0)),
                ..Default::default()
            })
            .opacity(appear)
            .width(Size::Px(w))
            .padding(Edges { left: 14.0, right: 8.0, top: 10.0, bottom: 12.0 })
            .gap(8.0)
            .radius(ui.theme.size.radius_panel)
            .bg(c.floating)
            .border(1.0, c.border_strong)
            .shadow(24.0, vec2(0.0, 10.0), c.shadow)
            .child(head)
            .child(
                Node::row()
                    .width(Size::Fill(1.0))
                    .padding(Edges { left: 0.0, right: 6.0, top: 0.0, bottom: 0.0 })
                    .child(bar(ui, id("barre").with(l.serial), l.overall(), 4.0, track)),
            )
            .child(
                Node::row()
                    .width(Size::Fill(1.0))
                    .padding(Edges { left: 0.0, right: 6.0, top: 0.0, bottom: 0.0 })
                    .child(foot),
            ),
    )
}

/// Zone d'ouverture de la vue vide : cadre en pointillés, dossier en transparence ; survol et fichier glissé
/// sur la fenêtre la mettent en avant.
pub fn empty(ui: &mut Ui, s: &mut AppState) -> Option<Node> {
    if !s.parts.is_empty() || s.loading.is_some() {
        return None;
    }
    let zid = empty_id();
    if ui.clicked(zid) {
        s.requests.push(Request::OpenFile);
    }
    let c = ui.theme.color.clone();
    let hot = ui.anim(zid.with("survol"), if ui.hovered(zid) || s.drop_hover { 1.0 } else { 0.0 }, 140.0);
    let drop = ui.anim(zid.with("dépôt"), if s.drop_hover { 1.0 } else { 0.0 }, 140.0);
    let appear = ui.appear(zid.with("apparition"), 240.0);
    // Couleurs opaques : les tirets, faits de segments à bouts ronds, se recouvrent dans les coins.
    let line = c.border_strong.mix(c.text_tertiary, hot).mix(activity_color(ui), drop);
    let icon = c.text_secondary.with_alpha(0.32 + 0.4 * hot).mix(activity_color(ui), drop);
    let fill = c.text.with_alpha(0.025 * hot + 0.03 * drop);
    let (w, h) = (300.0, 196.0);
    Some(
        Node::stack()
            .id(zid)
            .tooltip_shortcut(tr("Ouvrir un fichier"), "Ctrl O")
            .anchor(Anchor {
                center_x: true,
                top: Some(((s.viewport.get().height() - h) * 0.5 + (1.0 - appear) * 6.0).round().max(12.0)),
                ..Default::default()
            })
            .size(Size::Px(w), Size::Px(h))
            .opacity(appear)
            .child(
                Node::custom(move |p, r| {
                    p.dl.rect(r, 12.0, fill, None);
                    dashed_round_rect(p.dl, r.shrink(Edges::all(0.75)), 12.0, 1.5, 5.0, 5.0, line);
                })
                .fill(),
            )
            .child(Node::stack().fill().child(Node::icon("open", 56.0, icon).anchor(Anchor {
                center_x: true,
                top: Some((h - 56.0) * 0.5),
                ..Default::default()
            }))),
    )
}

/// Contour en pointillés d'un rectangle arrondi : tirets de `dash`, intervalles de `gap` (pixels logiques),
/// répartis pour boucler juste.
fn dashed_round_rect(dl: &mut obv_ui::DrawList, r: Rect, radius: f32, width: f32, dash: f32, gap: f32, color: Color) {
    let rad = radius.min(r.width() * 0.5).min(r.height() * 0.5);
    let (sw, sh) = (r.width() - 2.0 * rad, r.height() - 2.0 * rad);
    let arc = std::f32::consts::FRAC_PI_2 * rad;
    let perimeter = 2.0 * (sw + sh) + 4.0 * arc;
    let count = (perimeter / (dash + gap)).round().max(4.0);
    let period = perimeter / count;
    let on = period * dash / (dash + gap);
    // Point du contour à l'abscisse curviligne `d`, en partant du haut gauche (après le coin), sens horaire.
    let at = |d: f32| {
        let mut d = d.rem_euclid(perimeter);
        let corner = |cx: f32, cy: f32, a0: f32, d: f32| {
            let a = a0 + d / rad.max(1e-3);
            vec2(cx + rad * a.cos(), cy + rad * a.sin())
        };
        let half_pi = std::f32::consts::FRAC_PI_2;
        let pi = std::f32::consts::PI;
        if d < sw {
            return vec2(r.min.x + rad + d, r.min.y);
        }
        d -= sw;
        if d < arc {
            return corner(r.max.x - rad, r.min.y + rad, -half_pi, d);
        }
        d -= arc;
        if d < sh {
            return vec2(r.max.x, r.min.y + rad + d);
        }
        d -= sh;
        if d < arc {
            return corner(r.max.x - rad, r.max.y - rad, 0.0, d);
        }
        d -= arc;
        if d < sw {
            return vec2(r.max.x - rad - d, r.max.y);
        }
        d -= sw;
        if d < arc {
            return corner(r.min.x + rad, r.max.y - rad, half_pi, d);
        }
        d -= arc;
        if d < sh {
            return vec2(r.min.x, r.max.y - rad - d);
        }
        d -= sh;
        corner(r.min.x + rad, r.min.y + rad, pi, d)
    };
    for i in 0..count as usize {
        let d0 = i as f32 * period;
        // Tiret en petits segments pour suivre les coins.
        let steps = 3;
        let mut prev = at(d0);
        for k in 1..=steps {
            let p = at(d0 + on * k as f32 / steps as f32);
            dl.line(prev, p, width, color);
            prev = p;
        }
    }
}
