//! Jetons de conception : couleurs, rayons, espacements, typographie. Deux thèmes, mêmes noms.

use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
}

/// Couleurs sémantiques : un widget ne connaît que ces rôles, jamais une valeur.
#[derive(Clone, Debug)]
pub struct Palette {
    /// Fond de la fenêtre derrière les panneaux.
    pub app_bg: Color,
    /// Panneaux latéraux et barre du haut.
    pub panel: Color,
    /// Éléments posés sur un panneau (champ, survol de ligne).
    pub raised: Color,
    /// Survol d'un élément sur `raised`.
    pub raised_hover: Color,
    /// Barres et menus flottants au-dessus de la vue 3D.
    pub floating: Color,
    pub border: Color,
    pub border_strong: Color,
    pub divider: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub icon: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_text: Color,
    /// Segment choisi d'une commande segmentée (onglets).
    pub segment_selected: Color,
    /// Fond d'un élément actif ou sélectionné (accent atténué).
    pub accent_soft: Color,
    pub selection_text: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub focus_ring: Color,
    pub shadow: Color,
    /// Dégradé du fond de la vue 3D (haut, bas).
    pub viewport_top: Color,
    pub viewport_bottom: Color,
    pub tooltip_bg: Color,
    pub tooltip_text: Color,
    /// Axes X, Y, Z (cube de vue, repères).
    pub axis_x: Color,
    pub axis_y: Color,
    pub axis_z: Color,
}

/// Dimensions communes (pixels logiques).
#[derive(Clone, Debug)]
pub struct Metrics {
    pub radius_control: f32,
    pub radius_panel: f32,
    pub radius_floating: f32,
    pub control_height: f32,
    pub row_height: f32,
    pub topbar_height: f32,
    pub icon: f32,
    pub font_size: f32,
    pub font_size_small: f32,
    pub font_size_title: f32,
    pub space_xs: f32,
    pub space_s: f32,
    pub space_m: f32,
    pub space_l: f32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub kind: ThemeKind,
    pub color: Palette,
    pub size: Metrics,
}

impl Theme {
    pub fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Dark => Self::dark(),
            ThemeKind::Light => Self::light(),
        }
    }

    fn metrics() -> Metrics {
        Metrics {
            radius_control: 6.0,
            radius_panel: 10.0,
            radius_floating: 12.0,
            control_height: 28.0,
            row_height: 26.0,
            topbar_height: 44.0,
            icon: 16.0,
            font_size: 13.0,
            font_size_small: 11.5,
            font_size_title: 13.0,
            space_xs: 4.0,
            space_s: 8.0,
            space_m: 12.0,
            space_l: 16.0,
        }
    }

    pub fn dark() -> Self {
        let accent = Color::hex(0x4c8dff);
        Theme {
            kind: ThemeKind::Dark,
            color: Palette {
                app_bg: Color::hex(0x0e0f11),
                panel: Color::hex(0x17181b),
                raised: Color::hex(0x1f2024),
                raised_hover: Color::hex(0x27292e),
                floating: Color::hexa(0x1c1d21, 0.94),
                border: Color::hex(0x2a2c31),
                border_strong: Color::hex(0x3a3d44),
                divider: Color::hex(0x24262a),
                text: Color::hex(0xe9eaec),
                text_secondary: Color::hex(0xa0a4ac),
                text_tertiary: Color::hex(0x6c717a),
                icon: Color::hex(0xc4c7cd),
                accent,
                accent_hover: Color::hex(0x6aa0ff),
                accent_text: Color::WHITE,
                segment_selected: Color::hex(0x31343a),
                accent_soft: Color::hexa(0x4c8dff, 0.18),
                selection_text: Color::hex(0xdbe7ff),
                danger: Color::hex(0xf2645f),
                success: Color::hex(0x3fb27f),
                warning: Color::hex(0xe6a23c),
                focus_ring: Color::hexa(0x4c8dff, 0.55),
                shadow: Color::hexa(0x000000, 0.45),
                viewport_top: Color::hex(0x2a2d33),
                viewport_bottom: Color::hex(0x141518),
                tooltip_bg: Color::hex(0xf2f3f5),
                tooltip_text: Color::hex(0x16181c),
                axis_x: Color::hex(0xf0525a),
                axis_y: Color::hex(0x6cc644),
                axis_z: Color::hex(0x4c8dff),
            },
            size: Self::metrics(),
        }
    }

    pub fn light() -> Self {
        let accent = Color::hex(0x2f6fec);
        Theme {
            kind: ThemeKind::Light,
            color: Palette {
                app_bg: Color::hex(0xe7e9ed),
                panel: Color::hex(0xfbfbfc),
                raised: Color::hex(0xf0f1f4),
                raised_hover: Color::hex(0xe6e8ec),
                floating: Color::hexa(0xffffff, 0.95),
                border: Color::hex(0xdfe2e7),
                border_strong: Color::hex(0xc9cdd4),
                divider: Color::hex(0xeceef1),
                text: Color::hex(0x17191d),
                text_secondary: Color::hex(0x5b616b),
                text_tertiary: Color::hex(0x8d929b),
                icon: Color::hex(0x3c414a),
                accent,
                accent_hover: Color::hex(0x255fd6),
                accent_text: Color::WHITE,
                segment_selected: Color::WHITE,
                accent_soft: Color::hexa(0x2f6fec, 0.12),
                selection_text: Color::hex(0x1b4fbf),
                danger: Color::hex(0xd8403a),
                success: Color::hex(0x23905f),
                warning: Color::hex(0xc27c16),
                focus_ring: Color::hexa(0x2f6fec, 0.45),
                shadow: Color::hexa(0x1a2233, 0.16),
                viewport_top: Color::hex(0xf4f5f7),
                viewport_bottom: Color::hex(0xd8dce2),
                tooltip_bg: Color::hex(0x1d1f23),
                tooltip_text: Color::hex(0xf2f3f5),
                axis_x: Color::hex(0xe0434b),
                axis_y: Color::hex(0x4fa82e),
                axis_z: Color::hex(0x2f6fec),
            },
            size: Self::metrics(),
        }
    }
}
