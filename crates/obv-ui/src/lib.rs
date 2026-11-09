//! Bibliothèque d'interface d'Obview (D-04) : dessinée par le moteur Vulkan, dans la même image que la 3D.
//!
//! - `geom`, `color`, `theme` : géométrie, couleurs linéaires, jetons de conception clair et sombre.
//! - `paint` : liste de primitives à distance signée (boîtes arrondies, ombres, traits, glyphes).
//! - `text`, `icons`, `atlas` : texte Inter et icônes Lucide rendus au pixel de l'écran.
//! - `layout` : arbre de nœuds, boîtes flexibles, superpositions, découpe.
//! - `ui` : entrées, survol, clics, info-bulles, animations.
//! - `widgets` : composants prêts à l'emploi.
//! - `gpu` : rendu Vulkan de la liste.

mod atlas;
mod color;
mod geom;
pub mod gpu;
mod icon_list;
mod icons;
pub mod layout;
mod paint;
mod text;
mod theme;
mod ui;
pub mod widgets;

pub use atlas::Atlas;
pub use color::Color;
pub use geom::{Edges, Rect, Vec2, vec2};
pub use icons::Icons;
pub use layout::{Align, Anchor, Content, Dir, Justify, Node, Size, Tooltip};
pub use paint::{Corners, DrawList, Fill, Prim};
pub use text::{Fonts, TextLayout, TextStyle, Weight};
pub use theme::{Metrics, Palette, Theme, ThemeKind};
pub use ui::{FrameInput, Id, IdPart, Painter, Resources, Ui};
