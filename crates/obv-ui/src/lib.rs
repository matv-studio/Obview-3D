//! Bibliothèque d'interface d'Obview (D-04) : dessinée par le moteur Vulkan, dans la même image que la 3D.
//!
//! - `geom`, `color`, `theme` : géométrie, couleurs linéaires, jetons de conception clair et sombre.
//! - `paint` : liste de primitives à distance signée (boîtes arrondies, ombres, traits, glyphes).
//! - `text`, `icons`, `atlas` : texte Geist et icônes maison rendus au pixel de l'écran.
//! - `layout` : arbre de nœuds, boîtes flexibles, superpositions, découpe.
//! - `input` : entrées d'une image (souris, clavier, texte).
//! - `ui` : survol, clics, glisser, défilement, focus et saisie, calques, info-bulles, animations.
//! - `widgets` : composants prêts à l'emploi.
//! - `gpu` : rendu Vulkan de la liste.

mod atlas;
mod color;
pub mod controls;
mod geom;
pub mod gpu;
pub mod i18n;
mod icon_list;
mod icons;
mod input;
pub mod layout;
mod paint;
mod text;
mod theme;
mod ui;
pub mod widgets;

pub use atlas::Atlas;
pub use color::Color;
pub use geom::{Edges, Rect, Vec2, vec2};
pub use i18n::{Lang, tr};
pub use icons::Icons;
pub use input::{FrameInput, Key, Modifiers, MouseButton};
pub use layout::{Align, Anchor, Content, Dir, Justify, Node, Size, Tooltip};
pub use paint::{Corners, DrawList, Fill, Prim};
pub use text::{Fonts, TextLayout, TextStyle, Weight};
pub use theme::ACCENT_PRESETS;
pub use theme::{AccentScope, Metrics, Palette, Theme, ThemeKind};
pub use ui::{Cursor, EditState, Hit, Id, IdPart, Layer, Painter, Resources, ScrollState, Ui};
