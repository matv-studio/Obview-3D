//! Géométrie de l'interface, en pixels logiques (multipliés par l'échelle de l'écran au dessin).

use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        vec2(self.x * s, self.y * s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const ZERO: Rect = Rect { min: vec2(0.0, 0.0), max: vec2(0.0, 0.0) };
    pub const INFINITE: Rect = Rect { min: vec2(-1e9, -1e9), max: vec2(1e9, 1e9) };

    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { min: vec2(x, y), max: vec2(x + w, y + h) }
    }
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect {
            min: vec2(self.min.x.max(o.min.x), self.min.y.max(o.min.y)),
            max: vec2(self.max.x.min(o.max.x), self.max.y.min(o.max.y)),
        }
    }
    pub fn shrink(&self, e: Edges) -> Rect {
        Rect {
            min: vec2(self.min.x + e.left, self.min.y + e.top),
            max: vec2(self.max.x - e.right, self.max.y - e.bottom),
        }
    }
    pub fn expand(&self, d: f32) -> Rect {
        Rect { min: vec2(self.min.x - d, self.min.y - d), max: vec2(self.max.x + d, self.max.y + d) }
    }
    pub fn translate(&self, v: Vec2) -> Rect {
        Rect { min: self.min + v, max: self.max + v }
    }
    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }
}

/// Marges intérieures ou extérieures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Edges {
    pub const fn all(v: f32) -> Self {
        Edges { left: v, top: v, right: v, bottom: v }
    }
    pub const fn xy(x: f32, y: f32) -> Self {
        Edges { left: x, top: y, right: x, bottom: y }
    }
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}
