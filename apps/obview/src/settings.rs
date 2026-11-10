//! Réglages de l'application : thème, couleur d'accent, schéma de navigation, unités. Enregistrés en JSON dans le
//! dossier de configuration de l'utilisateur.

use std::path::PathBuf;

use obv_ui::{AccentScope, Color, Key, Modifiers, MouseButton, Theme, ThemeKind, tr};
use serde::{Deserialize, Serialize};

/// Moteur de rendu (D-30) : Vulkan d'abord, OpenGL en secours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RendererChoice {
    #[default]
    Auto,
    Vulkan,
    OpenGl,
}

impl RendererChoice {
    pub const ALL: [RendererChoice; 3] = [RendererChoice::Auto, RendererChoice::Vulkan, RendererChoice::OpenGl];
    pub fn label(self) -> &'static str {
        match self {
            RendererChoice::Auto => tr("Auto"),
            RendererChoice::Vulkan => "Vulkan",
            RendererChoice::OpenGl => "OpenGL",
        }
    }
}

/// Langue de l'interface (D-29) : anglais par défaut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    English,
    French,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::French];
    /// Nom de la langue dans cette langue (jamais traduit).
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
        }
    }
    pub fn lang(self) -> obv_ui::Lang {
        match self {
            Language::English => obv_ui::Lang::En,
            Language::French => obv_ui::Lang::Fr,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    /// Suit le thème du système (fourni par la fenêtre).
    System,
}

/// Schémas de navigation des logiciels courants (D-13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NavScheme {
    /// Clic gauche : panoramique ; clic droit : orbite ; molette ou Alt + clic droit : zoom (D-29).
    #[default]
    Obview,
    Blender,
    Maya,
    Cinema4D,
    Max,
    SolidWorks,
    Fusion,
    Rhino,
}

impl NavScheme {
    pub const ALL: [NavScheme; 8] = [
        NavScheme::Obview,
        NavScheme::Blender,
        NavScheme::Maya,
        NavScheme::Cinema4D,
        NavScheme::Max,
        NavScheme::SolidWorks,
        NavScheme::Fusion,
        NavScheme::Rhino,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NavScheme::Obview => "Obview",
            NavScheme::Blender => "Blender",
            NavScheme::Maya => "Maya",
            NavScheme::Cinema4D => "Cinema 4D",
            NavScheme::Max => "3ds Max",
            NavScheme::SolidWorks => "SolidWorks",
            NavScheme::Fusion => "Fusion",
            NavScheme::Rhino => "Rhino",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavAction {
    Orbit,
    Pan,
    Zoom,
}

impl NavAction {
    pub const ALL: [NavAction; 3] = [NavAction::Orbit, NavAction::Pan, NavAction::Zoom];
    pub fn label(self) -> &'static str {
        tr(match self {
            NavAction::Orbit => "Orbite",
            NavAction::Pan => "Panoramique",
            NavAction::Zoom => "Zoom",
        })
    }
}

/// Geste de souris : bouton tenu, modificateurs exigés, touche tenue éventuelle (Cinema 4D : 1, 2, 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gesture {
    pub button: MouseButton,
    pub mods: Modifiers,
    pub key: Option<char>,
}

const fn g(button: MouseButton, shift: bool, ctrl: bool, alt: bool) -> Gesture {
    Gesture { button, mods: Modifiers { shift, ctrl, alt }, key: None }
}
const fn gk(button: MouseButton, key: char) -> Gesture {
    Gesture { button, mods: Modifiers::NONE, key: Some(key) }
}

use MouseButton::{Left as L, Middle as M, Right as R};

type Table = &'static [(NavAction, Gesture)];
const OBVIEW: Table = &[
    (NavAction::Pan, g(L, false, false, false)),
    (NavAction::Orbit, g(R, false, false, false)),
    (NavAction::Zoom, g(R, false, false, true)),
];
const BLENDER: Table = &[
    (NavAction::Orbit, g(M, false, false, false)),
    (NavAction::Pan, g(M, true, false, false)),
    (NavAction::Zoom, g(M, false, true, false)),
];
const MAYA: Table = &[
    (NavAction::Orbit, g(L, false, false, true)),
    (NavAction::Pan, g(M, false, false, true)),
    (NavAction::Zoom, g(R, false, false, true)),
];
const C4D: Table = &[
    (NavAction::Orbit, g(L, false, false, true)),
    (NavAction::Pan, g(M, false, false, true)),
    (NavAction::Zoom, g(R, false, false, true)),
    (NavAction::Orbit, gk(L, '3')),
    (NavAction::Pan, gk(L, '1')),
    (NavAction::Zoom, gk(L, '2')),
];
const MAX: Table = &[
    (NavAction::Orbit, g(M, false, false, true)),
    (NavAction::Pan, g(M, false, false, false)),
    (NavAction::Zoom, g(M, false, true, true)),
];
const SOLIDWORKS: Table = &[
    (NavAction::Orbit, g(M, false, false, false)),
    (NavAction::Pan, g(M, false, true, false)),
    (NavAction::Zoom, g(M, true, false, false)),
];
const FUSION: Table = &[(NavAction::Orbit, g(M, true, false, false)), (NavAction::Pan, g(M, false, false, false))];
const RHINO: Table = &[
    (NavAction::Orbit, g(R, false, false, false)),
    (NavAction::Pan, g(R, true, false, false)),
    (NavAction::Zoom, g(R, false, true, false)),
];

/// Gestes de glisser de chaque schéma. La molette zoome dans tous les schémas.
pub fn gestures(s: NavScheme) -> Table {
    match s {
        NavScheme::Obview => OBVIEW,
        NavScheme::Blender => BLENDER,
        NavScheme::Maya => MAYA,
        NavScheme::Cinema4D => C4D,
        NavScheme::Max => MAX,
        NavScheme::SolidWorks => SOLIDWORKS,
        NavScheme::Fusion => FUSION,
        NavScheme::Rhino => RHINO,
    }
}

/// Action d'un glisser commencé avec `button`, les modificateurs `mods` et les touches tenues `held`.
/// Les modificateurs doivent correspondre exactement : Maj + milieu n'est pas milieu.
pub fn resolve(s: NavScheme, button: MouseButton, mods: Modifiers, held: &[Key]) -> Option<NavAction> {
    let keyed = gestures(s).iter().find(|(_, gs)| {
        gs.button == button && gs.key.is_some_and(|k| held.contains(&Key::Char(k)) || held.contains(&numpad(k)))
    });
    if let Some((a, _)) = keyed {
        return Some(*a);
    }
    gestures(s).iter().find(|(_, gs)| gs.key.is_none() && gs.button == button && gs.mods == mods).map(|(a, _)| *a)
}

fn numpad(c: char) -> Key {
    Key::Numpad(c.to_digit(10).unwrap_or(0) as u8)
}

/// Libellés courts d'un geste, une pastille par élément : « Maj », « Milieu ».
pub fn gesture_keys(gs: &Gesture) -> Vec<&'static str> {
    let mut v = Vec::new();
    if gs.mods.ctrl {
        v.push("Ctrl");
    }
    if gs.mods.alt {
        v.push("Alt");
    }
    if gs.mods.shift {
        v.push(tr("Maj"));
    }
    if let Some(k) = gs.key {
        v.push(match k {
            '1' => "1",
            '2' => "2",
            '3' => "3",
            _ => "?",
        });
    }
    v.push(tr(match gs.button {
        MouseButton::Left => "Gauche",
        MouseButton::Middle => "Milieu",
        MouseButton::Right => "Droit",
    }));
    v
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Units {
    #[default]
    Millimeters,
    Centimeters,
    Meters,
    Inches,
}

impl Units {
    pub const ALL: [Units; 4] = [Units::Millimeters, Units::Centimeters, Units::Meters, Units::Inches];
    pub fn label(self) -> &'static str {
        tr(match self {
            Units::Millimeters => "Millimètres",
            Units::Centimeters => "Centimètres",
            Units::Meters => "Mètres",
            Units::Inches => "Pouces",
        })
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Units::Millimeters => "mm",
            Units::Centimeters => "cm",
            Units::Meters => "m",
            Units::Inches => tr("po"),
        }
    }
    /// Longueur en mètres écrite dans l'unité, selon la langue (virgule ou point décimal).
    pub fn format(self, meters: f64) -> String {
        let (v, digits) = match self {
            Units::Millimeters => (meters * 1000.0, 2),
            Units::Centimeters => (meters * 100.0, 3),
            Units::Meters => (meters, 4),
            Units::Inches => (meters / 0.0254, 3),
        };
        format!("{} {}", obv_ui::i18n::decimal(v, digits), self.symbol())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    /// Moteur de rendu.
    pub renderer: RendererChoice,
    pub theme: ThemeChoice,
    /// Transitions animées (fenêtres, menus, panneaux, vues de rendu) ; coupées : changements instantanés.
    pub animations: bool,
    /// Couleur d'accent sRGB `0xRRGGBB` (D-14) ; aucune par défaut (charte : contraste inversé).
    pub accent: Option<u32>,
    /// Étendue de l'accent (charte).
    pub accent_scope: Scope,
    pub navigation: NavScheme,
    pub invert_zoom: bool,
    pub zoom_to_cursor: bool,
    /// Orbite autour du point de la surface sous le pointeur au début du geste.
    pub orbit_around_cursor: bool,
    /// Vitesse du zoom, 0,25 à 4.
    pub zoom_speed: f32,
    pub units: Units,
    /// Gizmo de navigation affiché en haut à droite de la vue (D-25).
    pub gizmo: GizmoStyle,
    /// Boutons retirés de la barre du bas (les nouveaux boutons apparaissent par défaut).
    pub toolbar_hidden: Vec<ToolButton>,
    /// Largeur des panneaux latéraux, 200 à 520 px (charte).
    pub left_width: f32,
    pub right_width: f32,
}

/// Largeurs permises des panneaux latéraux (charte : poignée de largeur 200–520 px).
pub const PANEL_MIN: f32 = 200.0;
pub const PANEL_MAX: f32 = 520.0;

/// Gizmo de navigation (D-25).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GizmoStyle {
    /// Boules d'axe à la manière de Blender.
    #[default]
    Axes,
    /// Cube de vue opaque à faces nommées (ViewCube).
    Cube,
    /// Trièdre compact des logiciels de CAO.
    Triad,
    None,
}

impl GizmoStyle {
    pub const ALL: [GizmoStyle; 4] = [GizmoStyle::Axes, GizmoStyle::Cube, GizmoStyle::Triad, GizmoStyle::None];
    pub fn label(self) -> &'static str {
        tr(match self {
            GizmoStyle::Axes => "Axes",
            GizmoStyle::Cube => "Cube",
            GizmoStyle::Triad => "Trièdre",
            GizmoStyle::None => "Aucun",
        })
    }
    pub fn tooltip(self) -> &'static str {
        tr(match self {
            GizmoStyle::Axes => "Boules d'axe, comme Blender",
            GizmoStyle::Cube => "Cube à faces nommées : faces, arêtes et coins cliquables",
            GizmoStyle::Triad => "Trièdre compact, comme les logiciels de CAO",
            GizmoStyle::None => "Pas de gizmo",
        })
    }
}

/// Boutons de la barre du bas, dans l'ordre d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolButton {
    Select,
    Measure,
    Section,
    Explode,
    Render,
    Projection,
    Capture,
    Frame,
}

impl ToolButton {
    pub const ALL: [ToolButton; 8] = [
        ToolButton::Select,
        ToolButton::Measure,
        ToolButton::Section,
        ToolButton::Explode,
        ToolButton::Render,
        ToolButton::Projection,
        ToolButton::Capture,
        ToolButton::Frame,
    ];
    pub fn label(self) -> &'static str {
        tr(match self {
            ToolButton::Select => "Sélection",
            ToolButton::Measure => "Mesurer",
            ToolButton::Section => "Coupe",
            ToolButton::Explode => "Vue éclatée",
            ToolButton::Render => "Vues de rendu",
            ToolButton::Projection => "Perspective ou orthographique",
            ToolButton::Capture => "Capture",
            ToolButton::Frame => "Focus",
        })
    }
    pub fn icon(self) -> &'static str {
        match self {
            ToolButton::Select => "select",
            ToolButton::Measure => "measure",
            ToolButton::Section => "section",
            ToolButton::Explode => "explode",
            ToolButton::Render => "render",
            ToolButton::Projection => "projection",
            ToolButton::Capture => "capture",
            ToolButton::Frame => "frame",
        }
    }
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            ToolButton::Select => Some("V"),
            ToolButton::Measure => Some("M"),
            ToolButton::Section => Some("C"),
            ToolButton::Explode => Some("E"),
            ToolButton::Render => Some("R"),
            ToolButton::Projection => Some("P"),
            ToolButton::Capture => None,
            ToolButton::Frame => Some("F"),
        }
    }
    /// Groupe visuel (séparateur entre deux groupes).
    pub fn group(self) -> u8 {
        match self {
            ToolButton::Select | ToolButton::Measure | ToolButton::Section | ToolButton::Explode => 0,
            ToolButton::Render | ToolButton::Projection | ToolButton::Capture => 1,
            ToolButton::Frame => 2,
        }
    }
}

/// Étendue de l'accent, enregistrée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scope {
    Minimal,
    #[default]
    Standard,
    Extended,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::English,
            renderer: RendererChoice::Auto,
            theme: ThemeChoice::Dark,
            animations: true,
            accent: None,
            accent_scope: Scope::Standard,
            navigation: NavScheme::Obview,
            invert_zoom: false,
            zoom_to_cursor: true,
            orbit_around_cursor: true,
            zoom_speed: 1.0,
            units: Units::Millimeters,
            gizmo: GizmoStyle::Axes,
            toolbar_hidden: Vec::new(),
            left_width: 260.0,
            right_width: 280.0,
        }
    }
}

impl Settings {
    /// Thème effectif ; `system_dark` vient de la fenêtre.
    pub fn theme_kind(&self, system_dark: bool) -> ThemeKind {
        match self.theme {
            ThemeChoice::Dark => ThemeKind::Dark,
            ThemeChoice::Light => ThemeKind::Light,
            ThemeChoice::System if system_dark => ThemeKind::Dark,
            ThemeChoice::System => ThemeKind::Light,
        }
    }

    /// Thème de l'interface (jetons de la charte, accent et étendue choisis).
    pub fn theme(&self, system_dark: bool) -> Theme {
        let scope = match self.accent_scope {
            Scope::Minimal => AccentScope::Minimal,
            Scope::Standard => AccentScope::Standard,
            Scope::Extended => AccentScope::Extended,
        };
        Theme::build(self.theme_kind(system_dark), self.accent.map(Color::hex), scope)
    }

    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("Obview 3D").join("settings.json"))
    }

    /// Lit les réglages ; un fichier absent ou illisible donne les valeurs par défaut.
    pub fn load() -> Settings {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .map(Settings::sanitized)
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

    fn sanitized(mut self) -> Settings {
        self.accent = self.accent.map(|a| a & 0xff_ffff);
        if !self.zoom_speed.is_finite() {
            self.zoom_speed = 1.0;
        }
        self.zoom_speed = self.zoom_speed.clamp(0.25, 4.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obview_par_defaut() {
        let s = Settings::default();
        assert_eq!(s.navigation, NavScheme::Obview);
        let none = Modifiers::NONE;
        let alt = Modifiers { alt: true, ..none };
        assert_eq!(resolve(NavScheme::Obview, L, none, &[]), Some(NavAction::Pan));
        assert_eq!(resolve(NavScheme::Obview, R, none, &[]), Some(NavAction::Orbit));
        assert_eq!(resolve(NavScheme::Obview, R, alt, &[]), Some(NavAction::Zoom));
        let shift = Modifiers { shift: true, ..none };
        let ctrl = Modifiers { ctrl: true, ..none };
        assert_eq!(resolve(NavScheme::Blender, M, none, &[]), Some(NavAction::Orbit));
        assert_eq!(resolve(NavScheme::Blender, M, shift, &[]), Some(NavAction::Pan));
        assert_eq!(resolve(NavScheme::Blender, M, ctrl, &[]), Some(NavAction::Zoom));
        assert_eq!(resolve(NavScheme::Blender, L, none, &[]), None);
    }

    #[test]
    fn schemas() {
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        assert_eq!(resolve(NavScheme::Maya, L, alt, &[]), Some(NavAction::Orbit));
        assert_eq!(resolve(NavScheme::Maya, R, alt, &[]), Some(NavAction::Zoom));
        assert_eq!(resolve(NavScheme::Cinema4D, L, Modifiers::NONE, &[Key::Char('1')]), Some(NavAction::Pan));
        assert_eq!(resolve(NavScheme::Max, M, Modifiers::NONE, &[]), Some(NavAction::Pan));
        assert_eq!(
            resolve(NavScheme::Fusion, M, Modifiers { shift: true, ..Modifiers::NONE }, &[]),
            Some(NavAction::Orbit)
        );
        assert_eq!(resolve(NavScheme::Rhino, R, Modifiers::NONE, &[]), Some(NavAction::Orbit));
        // Chaque schéma a une orbite et un panoramique.
        for s in NavScheme::ALL {
            for a in [NavAction::Orbit, NavAction::Pan] {
                assert!(gestures(s).iter().any(|(x, _)| *x == a), "{s:?} sans {a:?}");
            }
        }
    }

    #[test]
    fn json_aller_retour() {
        let s = Settings { accent: Some(0xe0559b), navigation: NavScheme::Maya, ..Default::default() };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&j).unwrap(), s);
        // Champs manquants : valeurs par défaut.
        let partial: Settings = serde_json::from_str(r#"{"navigation":"Rhino"}"#).unwrap();
        assert_eq!(partial.navigation, NavScheme::Rhino);
        assert!(partial.zoom_to_cursor);
    }

    #[test]
    fn unites() {
        crate::lang::init();
        obv_ui::i18n::set_lang(obv_ui::Lang::Fr);
        assert_eq!(Units::Millimeters.format(0.0125), "12,50 mm");
        assert_eq!(Units::Inches.format(0.0254), "1,000 po");
        obv_ui::i18n::set_lang(obv_ui::Lang::En);
        assert_eq!(Units::Millimeters.format(0.0125), "12.50 mm");
        assert_eq!(Units::Inches.format(0.0254), "1.000 in");
    }
}
