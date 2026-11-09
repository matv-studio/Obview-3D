//! Couleurs en espace linéaire (le mélange se fait en linéaire, la cible est sRGB).

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

    /// Couleur écrite en sRGB hexadécimal (`0xRRGGBB`), comme dans un outil de conception.
    pub fn hex(rgb: u32) -> Self {
        Self::hexa(rgb, 1.0)
    }

    pub fn hexa(rgb: u32, alpha: f32) -> Self {
        let c = |s: u32| srgb_to_linear(((rgb >> s) & 0xff) as f32 / 255.0);
        Color { r: c(16), g: c(8), b: c(0), a: alpha }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Color { a, ..self }
    }

    pub fn mul_alpha(self, k: f32) -> Self {
        Color { a: self.a * k, ..self }
    }

    /// Interpolation linéaire.
    pub fn mix(self, o: Color, t: f32) -> Self {
        Color {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }

    /// Valeur envoyée au GPU : couleur prémultipliée par son alpha.
    pub fn premul(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}
