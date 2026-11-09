//! Entrées d'une image : souris, molette, clavier, texte. Indépendant de la bibliothèque de fenêtres.

use crate::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false };
    pub fn none(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Touches utiles à l'application (les caractères saisis passent par `FrameInput::text`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    /// Lettre ou chiffre de la rangée principale, en minuscule.
    Char(char),
    /// Pavé numérique : chiffre.
    Numpad(u8),
    NumpadDecimal,
    NumpadAdd,
    NumpadSubtract,
    F(u8),
}

/// Entrées d'une image, en pixels logiques.
#[derive(Clone, Debug, Default)]
pub struct FrameInput {
    /// Taille de la fenêtre en pixels logiques.
    pub size: Vec2,
    pub scale: f32,
    /// Temps écoulé depuis le lancement, en secondes.
    pub time: f64,
    pub pointer: Option<Vec2>,
    /// Boutons enfoncés : gauche, milieu, droit.
    pub buttons: [bool; 3],
    pub modifiers: Modifiers,
    /// Défilement en pixels logiques (molette convertie, pavé tactile direct). Positif : vers le haut.
    pub scroll: Vec2,
    /// Caractères saisis pendant l'image.
    pub text: String,
    /// Touches pressées pendant l'image (répétition comprise).
    pub keys: Vec<Key>,
    /// Touches maintenues.
    pub held: Vec<Key>,
}

impl FrameInput {
    pub fn down(&self, b: MouseButton) -> bool {
        self.buttons[b as usize]
    }
    pub fn pressed_key(&self, k: Key) -> bool {
        self.keys.contains(&k)
    }
}
