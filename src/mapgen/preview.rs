//! Pictures of maps, for looking a generated map over fast: the whole map in
//! flat colours (the minimap's, which are the tiles' from far away), roads as
//! dark lines (bridges brown, passes white), cities as white squares
//! (capitals in their side's colour, ringed), temples as gold dots and ruins
//! as brown ones; a stage's grid as a grey picture; a contact sheet of many
//! maps with their seeds.

use crate::render::map_renderer::side_color;
use crate::render::minimap_renderer::terrain_color;
use crate::state::{Hex, Map, Side, TileType};

use super::grid::Grid;

/// A picture: straight RGBA, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// What a city is marked with besides its square: its reach (by sea, by air)
/// and whether it builds ships.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CityMark {
    pub sea: bool,
    pub air: bool,
    pub port: bool,
}

type Rgb = [u8; 3];

const BACKGROUND: Rgb = [0, 0, 0];
const ROAD: Rgb = [60, 45, 30];
const BRIDGE: Rgb = [150, 95, 45];
const PASS: Rgb = [240, 240, 240];
const CITY: Rgb = [250, 250, 250];
const RING: Rgb = [20, 18, 16];
const TEMPLE: Rgb = [240, 200, 60];
const RUIN: Rgb = [120, 75, 40];
const PORT: Rgb = [20, 40, 120];

fn byte(c: [f32; 3]) -> Rgb {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Image {
    pub fn new(w: u32, h: u32, fill: Rgb) -> Image {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend([fill[0], fill[1], fill[2], 255]);
        }
        Image { w, h, rgba }
    }

    fn put(&mut self, x: i64, y: i64, c: Rgb) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            let i = ((y as u32 * self.w + x as u32) * 4) as usize;
            self.rgba[i..i + 3].copy_from_slice(&c);
        }
    }

    fn get(&self, x: u32, y: u32) -> Rgb {
        let i = ((y * self.w + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }

    fn square(&mut self, (cx, cy): (f64, f64), half: f64, c: Rgb) {
        let (x0, x1) = ((cx - half).round() as i64, (cx + half).round() as i64);
        let (y0, y1) = ((cy - half).round() as i64, (cy + half).round() as i64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.put(x, y, c);
            }
        }
    }

    fn dot(&mut self, (cx, cy): (f64, f64), r: f64, c: Rgb) {
        let reach = r.ceil() as i64 + 1;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (x, y) = (cx.round() as i64 + dx, cy.round() as i64 + dy);
                let (fx, fy) = (x as f64 - cx, y as f64 - cy);
                if fx * fx + fy * fy <= r * r {
                    self.put(x, y, c);
                }
            }
        }
    }

    fn line(&mut self, (ax, ay): (f64, f64), (bx, by): (f64, f64), width: f64, c: Rgb) {
        let steps = ((bx - ax).abs().max((by - ay).abs()) * 2.0).ceil().max(1.0) as u32;
        for i in 0..=steps {
            let t = f64::from(i) / f64::from(steps);
            self.dot((ax + (bx - ax) * t, ay + (by - ay) * t), width / 2.0, c);
        }
    }

    /// The picture as a PNG file.
    pub fn png(&self) -> Vec<u8> {
        crate::render::png_out::encode_rgb(self.w, self.h, &self.rgba).unwrap_or_default()
    }
}

/// Where hexes are on a picture: `s` pixels from a hex's middle to its
/// corners, the map's top left corner at the picture's.
struct Layout {
    s: f64,
    rows: u16,
    cols: u16,
}

impl Layout {
    fn size(&self) -> (u32, u32) {
        let w = 1.5 * self.s * f64::from(self.cols) + 0.5 * self.s;
        let h = 3f64.sqrt() * self.s * (f64::from(self.rows) + 0.5);
        (w.ceil() as u32, h.ceil() as u32)
    }

    fn center(&self, hex: Hex) -> (f64, f64) {
        let odd = f64::from(hex.col & 1);
        let x = self.s + 1.5 * self.s * f64::from(hex.col);
        let y = 3f64.sqrt() * self.s * (1.0 + f64::from(hex.row) - 0.5 * odd);
        (x, y)
    }

    /// The hex under a pixel's middle, if it is on the map.
    fn hex_at(&self, px: u32, py: u32) -> Option<Hex> {
        let x = f64::from(px) + 0.5 - self.s;
        let y = f64::from(py) + 0.5 - 3f64.sqrt() * self.s;
        // Axial, fractional, then rounded as cube coordinates.
        let q = (2.0 / 3.0 * x) / self.s;
        let r = (-x / 3.0 + 3f64.sqrt() / 3.0 * y) / self.s;
        let (cx, cz) = (q, r);
        let cy = -cx - cz;
        let (mut rx, ry, mut rz) = (cx.round(), cy.round(), cz.round());
        let (dx, dy, dz) = ((rx - cx).abs(), (ry - cy).abs(), (rz - cz).abs());
        if dx > dy && dx > dz {
            rx = -ry - rz;
        } else if dy <= dz {
            rz = -rx - ry;
        }
        let (q, r) = (rx as i64, rz as i64);
        let row = r + (q + (q & 1)) / 2;
        (q >= 0 && row >= 0 && q < i64::from(self.cols) && row < i64::from(self.rows))
            .then(|| Hex::new(row as u16, q as u16))
    }
}

/// The whole map in flat colours, a hex `2 s` pixels wide, its roads, cities
/// and sites on it; `marks` says more of the cities, in their order.
pub fn preview(map: &Map, s: f64, marks: &[CityMark]) -> Image {
    let layout = Layout { s, rows: map.rows(), cols: map.cols() };
    let (w, h) = layout.size();
    let mut img = Image::new(w, h, BACKGROUND);
    let kind = |hex: Hex| map.tile(hex).map_or(TileType::Water, |t| t.kind);
    for py in 0..h {
        for px in 0..w {
            if let Some(hex) = layout.hex_at(px, py) {
                let t = match kind(hex) {
                    TileType::City | TileType::Temple | TileType::Ruin => TileType::Grass,
                    t => t,
                };
                img.put(i64::from(px), i64::from(py), byte(terrain_color(t)));
            }
        }
    }
    // Roads: a line from each road hex to each road or city beside it.
    let road = |hex: Hex| map.tile(hex).is_some_and(|t| t.road);
    let all = Grid::new(map.rows(), map.cols(), ());
    for hex in all.hexes().filter(|h| road(*h)) {
        for next in map.neighbours(hex).filter(|n| road(*n) || kind(*n) == TileType::City) {
            let (a, b) = (layout.center(hex), layout.center(next));
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let colour = match kind(hex) {
                TileType::Water => BRIDGE,
                TileType::Mountain => PASS,
                _ => ROAD,
            };
            img.line(a, mid, (s / 2.5).max(1.0), colour);
        }
    }
    // Sites, then cities over everything.
    for hex in all.hexes() {
        match kind(hex) {
            TileType::Temple => img.dot(layout.center(hex), s * 0.45, TEMPLE),
            TileType::Ruin => img.dot(layout.center(hex), s * 0.45, RUIN),
            _ => {}
        }
    }
    for (i, city) in map.cities().iter().enumerate() {
        let at = layout.center(city.pos);
        let mark = marks.get(i).copied().unwrap_or_default();
        let half = (s * 0.6).max(1.5);
        if mark.port {
            img.square(at, half + 1.5, PORT);
        }
        if city.side == Side::Grey {
            img.square(at, half + 0.8, RING);
            img.square(at, half, CITY);
        } else {
            img.square(at, half + 2.0, RING);
            img.square(at, half + 1.0, byte(side_color(city.side)));
        }
        if mark.sea {
            img.dot(at, half * 0.5, byte(terrain_color(TileType::Water)));
        }
        if mark.air {
            img.dot(at, half * 0.5, RING);
        }
    }
    img
}

/// A stage's grid as a grey picture: its least value black, its most white.
pub fn layer(grid: &Grid<f64>, s: f64) -> Image {
    let layout = Layout { s, rows: grid.rows(), cols: grid.cols() };
    let (w, h) = layout.size();
    let lo = grid.cells().iter().copied().fold(f64::INFINITY, f64::min);
    let hi = grid.cells().iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if hi > lo { hi - lo } else { 1.0 };
    let mut img = Image::new(w, h, BACKGROUND);
    for py in 0..h {
        for px in 0..w {
            if let Some(hex) = layout.hex_at(px, py) {
                let v = ((grid.get(hex) - lo) / span * 255.0).round() as u8;
                img.put(i64::from(px), i64::from(py), [v, v, v]);
            }
        }
    }
    img
}

/// The digits 0 to 9, 3 x 5, a row a number of 3 bits (the high one left).
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn number(img: &mut Image, n: u64, (x, y): (i64, i64), scale: i64) {
    for (i, ch) in n.to_string().chars().enumerate() {
        let d = DIGITS[ch.to_digit(10).unwrap_or(0) as usize];
        for (row, bits) in d.iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = x + (i as i64 * 4 + col) * scale + sx;
                            img.put(px, y + row as i64 * scale + sy, CITY);
                        }
                    }
                }
            }
        }
    }
}

/// Pictures side by side, `columns` to a row, each shrunk to fit a `cell`
/// pixels square, with its number at its top left.
pub fn sheet(pictures: &[(u64, Image)], columns: usize, cell: u32) -> Image {
    let columns = columns.max(1);
    let rows = pictures.len().div_ceil(columns).max(1);
    let gap = 6;
    let mut img = Image::new(columns as u32 * (cell + gap) + gap, rows as u32 * (cell + gap) + gap, [40, 40, 40]);
    for (i, (label, pic)) in pictures.iter().enumerate() {
        let (ox, oy) = ((i % columns) as u32 * (cell + gap) + gap, (i / columns) as u32 * (cell + gap) + gap);
        let scale = f64::from(pic.w.max(pic.h)) / f64::from(cell);
        let (w, h) = ((f64::from(pic.w) / scale) as u32, (f64::from(pic.h) / scale) as u32);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (((f64::from(x) + 0.5) * scale) as u32, ((f64::from(y) + 0.5) * scale) as u32);
                let c = pic.get(sx.min(pic.w - 1), sy.min(pic.h - 1));
                img.put(i64::from(ox + x), i64::from(oy + y), c);
            }
        }
        number(&mut img, *label, (i64::from(ox) + 3, i64::from(oy) + 3), 2);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_finds_its_hex() {
        let layout = Layout { s: 4.0, rows: 20, cols: 20 };
        for hex in [Hex::new(0, 0), Hex::new(0, 1), Hex::new(5, 6), Hex::new(19, 19), Hex::new(7, 3)] {
            let (x, y) = layout.center(hex);
            assert_eq!(layout.hex_at(x as u32, y as u32), Some(hex), "{hex:?}");
        }
        assert_eq!(layout.hex_at(0, 0), None, "the corner beside an odd column's first hex");
    }

    #[test]
    fn the_sheet_holds_every_picture() {
        let a = Image::new(100, 50, [255, 0, 0]);
        let b = Image::new(40, 80, [0, 255, 0]);
        let s = sheet(&[(1, a), (23, b)], 2, 60);
        assert_eq!((s.w, s.h), (2 * 66 + 6, 66 + 6));
        assert_eq!(s.get(6 + 30, 6 + 20), [255, 0, 0]);
        assert_eq!(s.get(72 + 10, 6 + 40), [0, 255, 0]);
        assert!(!s.png().is_empty());
    }
}
