//! Arbre de boîtes englobantes sur des triangles, pour la recherche du point le plus proche (écart de Hausdorff).
//! Construction par découpe SAH à casiers ; requête en profondeur, branches triées par distance à la boîte.

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[derive(Clone, Copy)]
struct Aabb {
    lo: V3,
    hi: V3,
}

impl Aabb {
    const EMPTY: Aabb = Aabb { lo: [f32::MAX; 3], hi: [f32::MIN; 3] };
    fn grow(&mut self, p: V3) {
        self.lo = std::array::from_fn(|i| self.lo[i].min(p[i]));
        self.hi = std::array::from_fn(|i| self.hi[i].max(p[i]));
    }
    fn merge(&mut self, o: &Aabb) {
        self.grow(o.lo);
        self.grow(o.hi);
    }
    fn area(&self) -> f32 {
        let d = sub(self.hi, self.lo);
        if d[0] < 0.0 { 0.0 } else { 2.0 * (d[0] * d[1] + d[1] * d[2] + d[2] * d[0]) }
    }
    fn dist2(&self, p: V3) -> f32 {
        (0..3)
            .map(|i| {
                let d = (self.lo[i] - p[i]).max(0.0).max(p[i] - self.hi[i]);
                d * d
            })
            .sum()
    }
}

struct Node {
    bounds: Aabb,
    /// Feuille : premier triangle et nombre ; nœud interne : indice de l'enfant gauche (le droit suit).
    first: u32,
    count: u32,
}

pub struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<[V3; 3]>,
}

const LEAF: usize = 4;
const BINS: usize = 12;

impl Bvh {
    pub fn new(positions: &[V3], indices: &[u32]) -> Bvh {
        let mut tris: Vec<[V3; 3]> = indices
            .chunks_exact(3)
            .map(|t| [positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]])
            .collect();
        let mut nodes = Vec::with_capacity(tris.len() * 2 / LEAF + 1);
        if tris.is_empty() {
            return Bvh { nodes, tris };
        }
        let mut centroids: Vec<V3> = tris.iter().map(|t| mul(add(add(t[0], t[1]), t[2]), 1.0 / 3.0)).collect();
        nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: tris.len() as u32 });
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let (first, count) = (nodes[ni].first as usize, nodes[ni].count as usize);
            let mut b = Aabb::EMPTY;
            let mut cb = Aabb::EMPTY;
            for i in first..first + count {
                for v in tris[i] {
                    b.grow(v);
                }
                cb.grow(centroids[i]);
            }
            nodes[ni].bounds = b;
            if count <= LEAF {
                continue;
            }
            // Découpe SAH sur l'axe le plus long des centres.
            let ext = sub(cb.hi, cb.lo);
            let axis = if ext[0] >= ext[1] && ext[0] >= ext[2] {
                0
            } else if ext[1] >= ext[2] {
                1
            } else {
                2
            };
            if ext[axis] <= 0.0 {
                continue;
            }
            let bin_of = |c: V3| (((c[axis] - cb.lo[axis]) / ext[axis] * BINS as f32) as usize).min(BINS - 1);
            let mut bins = [(Aabb::EMPTY, 0usize); BINS];
            for i in first..first + count {
                let k = bin_of(centroids[i]);
                for v in tris[i] {
                    bins[k].0.grow(v);
                }
                bins[k].1 += 1;
            }
            let mut best = (f32::MAX, 0usize);
            for split in 1..BINS {
                let (mut l, mut r) = (Aabb::EMPTY, Aabb::EMPTY);
                let (mut nl, mut nr) = (0, 0);
                for (k, (bb, n)) in bins.iter().enumerate() {
                    if *n == 0 {
                        continue;
                    }
                    if k < split {
                        l.merge(bb);
                        nl += n;
                    } else {
                        r.merge(bb);
                        nr += n;
                    }
                }
                if nl == 0 || nr == 0 {
                    continue;
                }
                let cost = l.area() * nl as f32 + r.area() * nr as f32;
                if cost < best.0 {
                    best = (cost, split);
                }
            }
            if best.1 == 0 {
                continue;
            }
            // Partition en place.
            let (mut i, mut j) = (first, first + count);
            while i < j {
                if bin_of(centroids[i]) < best.1 {
                    i += 1;
                } else {
                    j -= 1;
                    tris.swap(i, j);
                    centroids.swap(i, j);
                }
            }
            let left = nodes.len();
            nodes.push(Node { bounds: Aabb::EMPTY, first: first as u32, count: (i - first) as u32 });
            nodes.push(Node { bounds: Aabb::EMPTY, first: i as u32, count: (first + count - i) as u32 });
            nodes[ni].first = left as u32;
            nodes[ni].count = 0;
            stack.push(left);
            stack.push(left + 1);
        }
        Bvh { nodes, tris }
    }

    /// Distance du point `p` à la surface (infinie si l'arbre est vide).
    pub fn distance(&self, p: V3) -> f32 {
        if self.nodes.is_empty() {
            return f32::INFINITY;
        }
        let mut best = f32::MAX;
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(ni) = stack.pop() {
            let n = &self.nodes[ni as usize];
            if n.bounds.dist2(p) >= best {
                continue;
            }
            if n.count > 0 {
                for t in &self.tris[n.first as usize..(n.first + n.count) as usize] {
                    let q = closest_on_triangle(p, t);
                    let d = sub(p, q);
                    best = best.min(dot(d, d));
                }
            } else {
                let (l, r) = (n.first, n.first + 1);
                let (dl, dr) = (self.nodes[l as usize].bounds.dist2(p), self.nodes[r as usize].bounds.dist2(p));
                // La plus proche en dernier : visitée d'abord.
                if dl < dr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best.sqrt()
    }
}

/// Point du triangle le plus proche de `p` (Ericson, « Real-Time Collision Detection », 5.1.5).
fn closest_on_triangle(p: V3, t: &[V3; 3]) -> V3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return add(a, mul(ab, v));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return add(a, mul(ac, w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add(a, add(mul(ab, v), mul(ac, w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_a_un_plan_et_brute_force() {
        // Grille de 40 × 40 carrés dans le plan z = 0.
        let n = 40;
        let mut p = Vec::new();
        let mut idx = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                p.push([i as f32, j as f32, ((i * 7 + j * 3) % 5) as f32 * 0.01]);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = (j * (n + 1) + i) as u32;
                idx.extend([a, a + 1, a + n as u32 + 2, a, a + n as u32 + 2, a + n as u32 + 1]);
            }
        }
        let bvh = Bvh::new(&p, &idx);
        let mut seed = 1u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32
        };
        for _ in 0..200 {
            let q = [rnd() * 50.0 - 5.0, rnd() * 50.0 - 5.0, rnd() * 6.0 - 3.0];
            let brute = idx
                .chunks(3)
                .map(|t| {
                    let tri = [p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]];
                    let d = sub(q, closest_on_triangle(q, &tri));
                    dot(d, d).sqrt()
                })
                .fold(f32::MAX, f32::min);
            assert!((bvh.distance(q) - brute).abs() < 1e-4);
        }
    }
}
