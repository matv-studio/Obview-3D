//! Réglages de l'application : thème, couleur d'accent, schéma de navigation, unités. Enregistrés en JSON dans le
//! dossier de configuration de l'utilisateur.

use std::path::PathBuf;

use obv_ui::{AccentScope, Color, Key, Modifiers, MouseButton, Theme, ThemeKind};
use serde::{Deserialize, Serialize};

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
    #[default]
    Blender,
    Maya,
    Cinema4D,
    Max,
    SolidWorks,
    Fusion,
    Rhino,
}

impl NavScheme {
    pub const ALL: [NavScheme; 7] = [
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
        match self {
            NavAction::Orbit => "Orbite",
            NavAction::Pan => "Panoramique",
            NavAction::Zoom => "Zoom",
        }
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
        v.push("Maj");
    }
    if let Some(k) = gs.key {
        v.push(match k {
            '1' => "1",
            '2' => "2",
            '3' => "3",
            _ => "?",
        });
    }
    v.push(match gs.button {
        MouseButton::Left => "Gauche",
        MouseButton::Middle => "Milieu",
        MouseButton::Right => "Droit",
    });
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
        match self {
            Units::Millimeters => "Millimètres",
            Units::Centimeters => "Centimètres",
            Units::Meters => "Mètres",
            Units::Inches => "Pouces",
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Units::Millimeters => "mm",
            Units::Centimeters => "cm",
            Units::Meters => "m",
            Units::Inches => "po",
        }
    }
    /// Longueur en mètres écrite dans l'unité, à la française.
    pub fn format(self, meters: f64) -> String {
        let (v, digits) = match self {
            Units::Millimeters => (meters * 1000.0, 2),
            Units::Centimeters => (meters * 100.0, 3),
            Units::Meters => (meters, 4),
            Units::Inches => (meters / 0.0254, 3),
        };
        format!("{v:.digits$} {}", self.symbol()).replace('.', ",")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
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
    pub view_cube: bool,
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
            theme: ThemeChoice::Dark,
            accent: None,
            accent_scope: Scope::Standard,
            navigation: NavScheme::Blender,
            invert_zoom: false,
            zoom_to_cursor: true,
            orbit_around_cursor: true,
            zoom_speed: 1.0,
            units: Units::Millimeters,
            view_cube: true,
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
    fn blender_par_defaut() {
        let s = Settings::default();
        assert_eq!(s.navigation, NavScheme::Blender);
        let none = Modifiers::NONE;
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
        assert_eq!(Units::Millimeters.format(0.0125), "12,50 mm");
        assert_eq!(Units::Inches.format(0.0254), "1,000 po");
    }
}
