//! Atlas de couverture 8 bits (glyphes et icônes), rangé par étagères, envoyé au GPU par zones modifiées.

pub struct Atlas {
    pub size: u32,
    pub pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
    /// Zone modifiée depuis le dernier envoi (x0, y0, x1, y1).
    pub dirty: Option<[u32; 4]>,
}

struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

const PAD: u32 = 1;

impl Atlas {
    pub fn new(size: u32) -> Self {
        // Pixel (0,0) plein : sert de couverture totale si besoin.
        let mut pixels = vec![0u8; (size * size) as usize];
        pixels[0] = 255;
        Atlas { size, pixels, shelves: Vec::new(), next_y: 2, dirty: Some([0, 0, 1, 1]) }
    }

    /// Réserve une place et y copie `data` (largeur × hauteur, une valeur par pixel). Renvoie (x, y).
    pub fn insert(&mut self, w: u32, h: u32, data: &[u8]) -> Option<(u32, u32)> {
        if w == 0 || h == 0 {
            return Some((0, 0));
        }
        let (x, y) = self.allocate(w + PAD, h + PAD)?;
        for row in 0..h {
            let dst = ((y + row) * self.size + x) as usize;
            let src = (row * w) as usize;
            self.pixels[dst..dst + w as usize].copy_from_slice(&data[src..src + w as usize]);
        }
        let d = self.dirty.get_or_insert([x, y, x + w, y + h]);
        *d = [d[0].min(x), d[1].min(y), d[2].max(x + w), d[3].max(y + h)];
        Some((x, y))
    }

    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.size {
            return None;
        }
        // Étagère existante la mieux ajustée en hauteur.
        let mut best: Option<usize> = None;
        for (i, s) in self.shelves.iter().enumerate() {
            if s.height >= h
                && s.x + w <= self.size
                && s.height <= h + h / 2 + 2
                && best.is_none_or(|b| self.shelves[b].height > s.height)
            {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let s = &mut self.shelves[i];
            let x = s.x;
            s.x += w;
            return Some((x, s.y));
        }
        if self.next_y + h > self.size {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push(Shelf { y, height: h, x: w });
        Some((0, y))
    }

    pub fn uv(&self, x: u32, y: u32, w: u32, h: u32) -> [f32; 4] {
        let s = self.size as f32;
        [x as f32 / s, y as f32 / s, (x + w) as f32 / s, (y + h) as f32 / s]
    }
}
