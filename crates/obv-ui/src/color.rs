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

    /// Rapport de contraste WCAG 2 entre deux couleurs opaques (1 à 21).
    pub fn contrast(self, o: Color) -> f32 {
        let (a, b) = (self.luminance(), o.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Valeur sRGB `0xRRGGBB` (alpha ignoré).
    pub fn to_hex(self) -> u32 {
        let c = |v: f32| {
            let v = v.clamp(0.0, 1.0);
            let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (s * 255.0).round() as u32
        };
        (c(self.r) << 16) | (c(self.g) << 8) | c(self.b)
    }

    /// Lit `#RRGGBB`, `RRGGBB` ou `#RGB`.
    pub fn parse_hex(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        let v = match s.len() {
            6 => u32::from_str_radix(s, 16).ok()?,
            3 => {
                let v = u32::from_str_radix(s, 16).ok()?;
                let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
                ((r * 17) << 16) | ((g * 17) << 8) | (b * 17)
            }
            _ => return None,
        };
        Some(Color::hex(v))
    }

    /// Mélange perceptif (en sRGB), pour dériver des teintes comme le ferait un outil de conception.
    pub fn mix_srgb(self, o: Color, t: f32) -> Color {
        let split = |c: Color| {
            let h = c.to_hex();
            [(h >> 16) as f32, ((h >> 8) & 0xff) as f32, (h & 0xff) as f32]
        };
        let (a, b) = (split(self), split(o));
        let m = |i: usize| (a[i] + (b[i] - a[i]) * t).round().clamp(0.0, 255.0) as u32;
        Color::hex((m(0) << 16) | (m(1) << 8) | m(2)).with_alpha(self.a + (o.a - self.a) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_aller_retour() {
        for v in [0x000000, 0xffffff, 0x4c8dff, 0x2f6fec, 0x123456] {
            assert_eq!(Color::hex(v).to_hex(), v);
        }
        assert_eq!(Color::parse_hex("#fa0").map(|c| c.to_hex()), Some(0xffaa00));
        assert!(Color::parse_hex("zz").is_none());
    }
}
