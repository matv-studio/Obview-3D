//! Jetons de conception, repris de la charte des applications internes (Revel, `tokens.css`, 09/11/2026) :
//! gris neutres sans teinte, un seul accent choisi par l'utilisateur (« Aucune » par défaut : action principale en
//! contraste inversé), code couleur fonctionnel discret, grille de 4 px, contrôles de 28 px, texte de 13 px.
//! Un widget ne connaît que des rôles, jamais une valeur.

use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
}

/// Étendue de l'accent (charte, principe 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AccentScope {
    /// Focus et contour du plateau seulement.
    Minimal,
    /// + action principale, interrupteurs, sélection.
    #[default]
    Standard,
    /// + onglets, choix, lignes actives, glissières.
    Extended,
}

/// Couleurs sémantiques.
#[derive(Clone, Debug)]
pub struct Palette {
    /// Cadre de la fenêtre et barre supérieure (`--chrome`) ; aussi couleur d'effacement.
    pub app_bg: Color,
    /// Contour du plateau (la vue 3D) : accent, sinon gris (`--stage-line`).
    pub stage_line: Color,
    /// Panneaux, barres d'outils (`--surface`).
    pub panel: Color,
    /// Sections, champs, menus (`--surface-raised`).
    pub raised: Color,
    pub raised_hover: Color,
    /// Élément actif ou enfoncé (`--surface-active`).
    pub surface_active: Color,
    /// Barre flottante au-dessus de la vue 3D.
    pub floating: Color,
    pub border: Color,
    pub border_strong: Color,
    pub divider: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub icon: Color,
    /// Couleur d'accent effective : celle de l'utilisateur, ou le contraste inversé sans accent (`--primary`).
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_press: Color,
    /// Texte sur l'accent (`--primary-ink`).
    pub accent_text: Color,
    /// Un accent a été choisi (sinon `accent` est le contraste inversé).
    pub has_accent: bool,
    /// Segment choisi d'une commande segmentée.
    pub segment_selected: Color,
    /// Fond d'une ligne ou d'un élément sélectionné (`--selection`).
    pub accent_soft: Color,
    pub selection_text: Color,
    /// Code fonctionnel : bleu « réglé ici / mode actif », orange « hérité / avertissement », rouge, vert.
    pub set_text: Color,
    pub set_bg: Color,
    pub danger: Color,
    pub danger_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub focus_ring: Color,
    pub shadow: Color,
    /// Dégradé neutre du fond de la vue 3D (haut, bas).
    pub viewport_top: Color,
    pub viewport_bottom: Color,
    pub tooltip_bg: Color,
    pub tooltip_text: Color,
    pub axis_x: Color,
    pub axis_y: Color,
    pub axis_z: Color,
    /// Voile sous une fenêtre (`--overlay`).
    pub scrim: Color,
    /// Trait fin des glissières, interrupteurs, barres de défilement (`--range-track-bg`).
    pub track: Color,
    /// Pouce de la barre de défilement (`--text-faint`, survol `--text-muted`).
    pub thumb: Color,
    pub thumb_hover: Color,
    /// Poignée des glissières et des interrupteurs.
    pub knob: Color,
    pub knob_ring: Color,
    /// Champ de saisie (`--bg`).
    pub input_bg: Color,
}

/// Dimensions (pixels logiques), grille de 4 px.
#[derive(Clone, Debug)]
pub struct Metrics {
    pub radius_control: f32,
    pub radius_panel: f32,
    pub radius_floating: f32,
    pub radius_stage: f32,
    pub control_height: f32,
    pub row_height: f32,
    pub topbar_height: f32,
    pub icon: f32,
    pub font_size: f32,
    pub font_size_small: f32,
    pub font_size_label: f32,
    pub font_size_title: f32,
    pub space_xs: f32,
    pub space_s: f32,
    pub space_m: f32,
    pub space_l: f32,
    /// Marge du plateau dans le cadre.
    pub stage_margin: f32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub kind: ThemeKind,
    pub color: Palette,
    pub size: Metrics,
    pub scope: AccentScope,
}

fn metrics() -> Metrics {
    Metrics {
        radius_control: 6.0,
        radius_panel: 8.0,
        radius_floating: 10.0,
        radius_stage: 10.0,
        control_height: 28.0,
        row_height: 28.0,
        topbar_height: 44.0,
        icon: 16.0,
        font_size: 13.0,
        font_size_small: 12.0,
        font_size_label: 11.0,
        font_size_title: 15.0,
        space_xs: 4.0,
        space_s: 8.0,
        space_m: 12.0,
        space_l: 16.0,
        stage_margin: 6.0,
    }
}

impl Theme {
    /// Thème sans accent, étendue standard.
    pub fn new(kind: ThemeKind) -> Self {
        Self::build(kind, None, AccentScope::Standard)
    }
    pub fn dark() -> Self {
        Self::new(ThemeKind::Dark)
    }
    pub fn light() -> Self {
        Self::new(ThemeKind::Light)
    }

    /// Thème complet : `accent` facultatif (sRGB), étendue de l'accent.
    pub fn build(kind: ThemeKind, accent: Option<Color>, scope: AccentScope) -> Self {
        let dark = kind == ThemeKind::Dark;
        let h = Color::hex;
        let ha = Color::hexa;
        // Gris neutres de la charte.
        let (chrome, surface, raised, hover, active, line, line_strong) = if dark {
            (h(0x2a2a2a), h(0x1f1f1f), h(0x262626), h(0x2e2e2e), h(0x383838), h(0x2e2e2e), h(0x383838))
        } else {
            (h(0xe3e3e3), h(0xffffff), h(0xffffff), h(0xf7f7f7), h(0xe6e6e6), h(0xe6e6e6), h(0xdadada))
        };
        let (text, muted, faint) =
            if dark { (h(0xededed), h(0x9d9d9d), h(0x8a8a8a)) } else { (h(0x1a1a1a), h(0x626262), h(0x767676)) };
        let (inverse, inverse_ink) = if dark { (h(0xf2f2f2), h(0x191919)) } else { (h(0x1a1a1a), h(0xffffff)) };
        let bg = if dark { h(0x191919) } else { h(0xefefef) };
        let stage_base = if dark { h(0x3a3a3a) } else { h(0xcfcfcf) };

        // Accent effectif selon l'étendue.
        let accent = accent.map(|a| a.with_alpha(1.0));
        let primary_from_accent = matches!(scope, AccentScope::Standard | AccentScope::Extended);
        let primary = match accent {
            Some(a) if primary_from_accent => a,
            _ => inverse,
        };
        let ink_on = |c: Color| if Color::WHITE.contrast(c) >= 3.0 { Color::WHITE } else { h(0x111111) };
        let primary_ink = if accent.is_some() && primary_from_accent { ink_on(primary) } else { inverse_ink };
        let primary_hover = primary.mix_srgb(muted, 0.08);
        let primary_press = primary.mix_srgb(bg, 0.22);
        let selection_base = match accent {
            Some(a) if primary_from_accent => a,
            _ => text,
        };
        let selection = selection_base.with_alpha(0.14);
        let selection_text = match accent {
            Some(a) if scope == AccentScope::Extended => a.mix_srgb(text, 0.28),
            _ => text,
        };
        let segment = match accent {
            Some(a) if scope == AccentScope::Extended => a.with_alpha(0.18),
            _ => active,
        };
        Theme {
            kind,
            scope,
            size: metrics(),
            color: Palette {
                app_bg: chrome,
                stage_line: accent.unwrap_or(stage_base),
                panel: surface,
                raised,
                raised_hover: hover,
                surface_active: active,
                floating: surface.with_alpha(0.94),
                border: line,
                border_strong: line_strong,
                divider: line,
                text,
                text_secondary: muted,
                text_tertiary: faint,
                icon: muted,
                accent: primary,
                accent_hover: primary_hover,
                accent_press: primary_press,
                accent_text: primary_ink,
                has_accent: accent.is_some(),
                segment_selected: segment,
                accent_soft: selection,
                selection_text,
                set_text: if dark { h(0x7fb0ff) } else { h(0x1f5fd1) },
                set_bg: ha(0x4084ff, if dark { 0.2 } else { 0.14 }),
                danger: if dark { h(0xff9b92) } else { h(0xb3261e) },
                danger_bg: if dark { h(0x3b2120) } else { h(0xfdeceb) },
                success: if dark { h(0x7fd6a3) } else { h(0x1b7a4b) },
                warning: if dark { h(0xf3aa55) } else { h(0xb25b00) },
                focus_ring: accent.unwrap_or(muted),
                shadow: ha(0x000000, if dark { 0.45 } else { 0.18 }),
                viewport_top: if dark { h(0x262626) } else { h(0xf7f7f7) },
                viewport_bottom: if dark { h(0x151515) } else { h(0xdcdcdc) },
                tooltip_bg: inverse,
                tooltip_text: inverse_ink,
                axis_x: if dark { h(0xef5a5a) } else { h(0xd8433f) },
                axis_y: if dark { h(0x7cc35a) } else { h(0x4f9a2e) },
                axis_z: if dark { h(0x5b8fef) } else { h(0x2f6bd8) },
                scrim: ha(0x000000, if dark { 0.55 } else { 0.28 }),
                track: if dark { h(0x0c0c0c) } else { h(0xc9c9c9) },
                thumb: faint,
                thumb_hover: muted,
                knob: Color::WHITE,
                knob_ring: ha(0x000000, if dark { 0.55 } else { 0.28 }),
                input_bg: bg,
            },
        }
    }
}

/// Couleurs d'accent proposées (sRGB), en plus de « Aucune » et d'une valeur libre.
pub const ACCENT_PRESETS: [(u32, &str); 8] = [
    (0x4c8dff, "Bleu"),
    (0x7c5cff, "Violet"),
    (0xe0559b, "Rose"),
    (0xf0524f, "Rouge"),
    (0xf28c28, "Orange"),
    (0xe3b416, "Jaune"),
    (0x2fb67c, "Vert"),
    (0x1eb2c4, "Cyan"),
];
