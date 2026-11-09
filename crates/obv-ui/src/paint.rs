//! Liste de dessin : primitives à distance signée envoyées telles quelles au GPU (un seul pipeline).

use crate::{Color, Rect, Vec2, vec2};

pub const KIND_BOX: f32 = 0.0;
pub const KIND_SHADOW: f32 = 1.0;
pub const KIND_COVERAGE: f32 = 2.0;
pub const KIND_LINE: f32 = 3.0;
pub const KIND_QUAD: f32 = 4.0;

/// Primitive telle que lue par `ui.vert` / `ui.frag` (8 × vec4, disposition scalaire).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prim {
    /// Rectangle en pixels de l'écran (x0, y0, x1, y1) ; pour une ligne : extrémités (x0, y0) → (x1, y1).
    pub rect: [f32; 4],
    pub clip: [f32; 4],
    /// Rayons des coins : haut-gauche, haut-droite, bas-droite, bas-gauche.
    pub radii: [f32; 4],
    /// Remplissage (haut du dégradé) ou teinte de la couverture.
    pub color0: [f32; 4],
    /// Bas du dégradé.
    pub color1: [f32; 4],
    pub border: [f32; 4],
    /// Coordonnées dans l'atlas (u0, v0, u1, v1).
    pub uv: [f32; 4],
    /// x : épaisseur de bordure ou de ligne ; y : écart type de l'ombre ; z : sorte ; w : gamma de couverture.
    pub params: [f32; 4],
}

/// Remplissage d'une boîte.
#[derive(Clone, Copy, Debug)]
pub enum Fill {
    Solid(Color),
    /// Dégradé vertical (haut, bas).
    Vertical(Color, Color),
}

impl From<Color> for Fill {
    fn from(c: Color) -> Self {
        Fill::Solid(c)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Corners {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

impl Corners {
    pub const fn all(r: f32) -> Self {
        Corners { tl: r, tr: r, br: r, bl: r }
    }
}

impl From<f32> for Corners {
    fn from(r: f32) -> Self {
        Corners::all(r)
    }
}

/// Primitives d'une image, en pixels logiques ; l'échelle de l'écran est appliquée à l'ajout.
pub struct DrawList {
    pub prims: Vec<Prim>,
    clips: Vec<Rect>,
    pub scale: f32,
    /// Rotation appliquée aux glyphes et icônes : pivot (pixels de l'écran) et angle (radians).
    rotation: Option<([f32; 2], f32)>,
}

impl DrawList {
    pub fn new(scale: f32) -> Self {
        DrawList { prims: Vec::new(), clips: vec![Rect::INFINITE], scale, rotation: None }
    }

    pub fn clear(&mut self, scale: f32) {
        self.prims.clear();
        self.clips.truncate(1);
        self.scale = scale;
        self.rotation = None;
    }

    /// Fait tourner le texte et les icônes dessinés ensuite autour de `pivot` (pixels logiques), d'un angle en
    /// radians (sens horaire à l'écran). La découpe ne s'applique pas au texte tourné.
    pub fn set_rotation(&mut self, pivot: Vec2, angle: f32) {
        self.rotation = Some(([pivot.x * self.scale, pivot.y * self.scale], angle));
    }
    pub fn clear_rotation(&mut self) {
        self.rotation = None;
    }

    fn clip(&self) -> [f32; 4] {
        let c = self.clips.last().unwrap();
        let s = self.scale;
        [c.min.x * s, c.min.y * s, c.max.x * s, c.max.y * s]
    }

    fn r(&self, r: Rect) -> [f32; 4] {
        let s = self.scale;
        [r.min.x * s, r.min.y * s, r.max.x * s, r.max.y * s]
    }

    pub fn push_clip(&mut self, r: Rect) {
        let c = self.clips.last().unwrap().intersect(&r);
        self.clips.push(c);
    }

    pub fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    pub fn current_clip(&self) -> Rect {
        *self.clips.last().unwrap()
    }

    /// Boîte arrondie avec bordure intérieure facultative.
    pub fn rect(
        &mut self,
        rect: Rect,
        corners: impl Into<Corners>,
        fill: impl Into<Fill>,
        border: Option<(f32, Color)>,
    ) {
        let c = corners.into();
        let s = self.scale;
        let (c0, c1) = match fill.into() {
            Fill::Solid(c) => (c.premul(), c.premul()),
            Fill::Vertical(a, b) => (a.premul(), b.premul()),
        };
        let (bw, bc) = border.map(|(w, c)| (w * s, c.premul())).unwrap_or((0.0, [0.0; 4]));
        if c0[3] <= 0.0 && c1[3] <= 0.0 && (bw <= 0.0 || bc[3] <= 0.0) {
            return;
        }
        self.prims.push(Prim {
            rect: self.r(rect),
            clip: self.clip(),
            radii: [c.tl * s, c.tr * s, c.br * s, c.bl * s],
            color0: c0,
            color1: c1,
            border: bc,
            uv: [0.0; 4],
            params: [bw, 0.0, KIND_BOX, 1.0],
        });
    }

    /// Ombre portée floue d'une boîte arrondie (calcul exact par la fonction d'erreur, pas de texture).
    pub fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, offset: Vec2, color: Color) {
        let s = self.scale;
        self.prims.push(Prim {
            rect: self.r(rect.translate(offset)),
            clip: self.clip(),
            radii: [radius * s; 4],
            color0: color.premul(),
            color1: color.premul(),
            border: [0.0; 4],
            uv: [0.0; 4],
            params: [0.0, (blur * 0.5 * s).max(0.5), KIND_SHADOW, 1.0],
        });
    }

    /// Segment à bouts ronds.
    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: Color) {
        let s = self.scale;
        self.prims.push(Prim {
            rect: [a.x * s, a.y * s, b.x * s, b.y * s],
            clip: self.clip(),
            radii: [0.0; 4],
            color0: color.premul(),
            color1: color.premul(),
            border: [0.0; 4],
            uv: [0.0; 4],
            params: [width * s, 0.0, KIND_LINE, 1.0],
        });
    }

    /// Quadrilatère convexe plein, sommets dans l'ordre (sens quelconque), bord lissé ; bordure facultative.
    pub fn quad(&mut self, pts: [Vec2; 4], fill: Color, border: Option<(f32, Color)>) {
        let s = self.scale;
        let (bw, bc) = border.map(|(w, c)| (w * s, c.premul())).unwrap_or((0.0, [0.0; 4]));
        self.prims.push(Prim {
            rect: [pts[0].x * s, pts[0].y * s, pts[1].x * s, pts[1].y * s],
            clip: self.clip(),
            radii: [pts[2].x * s, pts[2].y * s, pts[3].x * s, pts[3].y * s],
            color0: fill.premul(),
            color1: fill.premul(),
            border: bc,
            uv: [0.0; 4],
            params: [bw, 0.0, KIND_QUAD, 1.0],
        });
    }

    /// Image de couverture (glyphe ou icône) déjà en pixels de l'écran.
    pub fn coverage(&mut self, device_rect: [f32; 4], uv: [f32; 4], color: Color, gamma: f32) {
        self.prims.push(Prim {
            rect: device_rect,
            clip: self.clip(),
            radii: [0.0; 4],
            color0: color.premul(),
            color1: color.premul(),
            border: match self.rotation {
                Some((p, a)) => [a, p[0], p[1], 1.0],
                None => [0.0; 4],
            },
            uv,
            params: [0.0, 0.0, KIND_COVERAGE, gamma],
        });
    }

    /// Point de départ commode pour placer du texte : pixel de l'écran le plus proche.
    pub fn snap(&self, p: Vec2) -> Vec2 {
        vec2((p.x * self.scale).round() / self.scale, (p.y * self.scale).round() / self.scale)
    }
}
