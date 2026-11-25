//! The panels' small pictures: a coin for the treasury ([`coin_icon`]), a
//! city by the count of the side's cities ([`city_icon`]), and
//! a unit's icon: the disc of its side's color with the dark
//! ring around it and its kind's picture on it, as the map draws a unit, but
//! round (the map squeezes it with the hex) and filled to the edge. Drawn on
//! the CPU into the pixels of the size the panel shows, so the UI only copies
//! it (Turbo Ferret's images are not scaled smoothly).
//!
//! Not a port: the C++ had no panels.

use turbo_ferret::{Image, Size};

use super::map_renderer::{DISC_RADIUS, level_ring, side_color};
use crate::state::Side;

/// How far the drawn part of the picture reaches from the disc's middle
/// toward its ring, at most.
const PICTURE_REACH: f32 = 0.9;
/// A pixel of the picture with less alpha than this is not part of what is drawn.
const DRAWN: u8 = 8;
/// Samples taken across and down each pixel of the icon.
const SAMPLES: usize = 4;

/// A pixel of the icon, color premultiplied by alpha.
type Premul = [f32; 4];

/// The part of a picture that is drawn (its pixels that are not nearly
/// transparent): the middle of the box around them, and how far from that
/// middle the farthest of them reaches, in pixels. `None` for a picture that
/// is all transparent, or has fewer bytes than its size.
fn drawn_part(picture: &Image) -> Option<([f32; 2], f32)> {
    let (w, h) = (picture.size.w.max(0) as usize, picture.size.h.max(0) as usize);
    if picture.rgba.len() < w * h * 4 {
        return None;
    }
    let drawn = |x: usize, y: usize| picture.rgba[(y * w + x) * 4 + 3] >= DRAWN;
    let pixels = || (0..h).flat_map(move |y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| drawn(x, y));
    let [left, top, right, bottom] = pixels().fold(None, |b: Option<[usize; 4]>, (x, y)| {
        Some(b.map_or([x, y, x, y], |[l, t, r, b]| [l.min(x), t.min(y), r.max(x), b.max(y)]))
    })?;
    let middle = [(left + right + 1) as f32 / 2.0, (top + bottom + 1) as f32 / 2.0];
    // To the pixel's far corner.
    let reach = |(x, y): (usize, usize)| {
        let dx = (x as f32 + 0.5 - middle[0]).abs() + 0.5;
        let dy = (y as f32 + 0.5 - middle[1]).abs() + 0.5;
        (dx * dx + dy * dy).sqrt()
    };
    Some((middle, pixels().map(reach).fold(0.0, f32::max)))
}

/// The picture at a point (in pixels, from its top left corner), mixed from
/// the four pixels around it; transparent outside it.
fn sample(picture: &Image, x: f32, y: f32) -> Premul {
    let (w, h) = (picture.size.w, picture.size.h);
    let pixel = |px: i32, py: i32| -> Premul {
        if px < 0 || py < 0 || px >= w || py >= h {
            return [0.0; 4];
        }
        let i = (py as usize * w as usize + px as usize) * 4;
        let Some(p) = picture.rgba.get(i..i + 4) else { return [0.0; 4] };
        let a = f32::from(p[3]) / 255.0;
        [f32::from(p[0]) / 255.0 * a, f32::from(p[1]) / 255.0 * a, f32::from(p[2]) / 255.0 * a, a]
    };
    // Pixel centers are at half pixels.
    let (fx, fy) = (x - 0.5, y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);
    let mut out = [0.0; 4];
    for (dx, dy, weight) in
        [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)]
    {
        let p = pixel(x0 + dx, y0 + dy);
        for (o, v) in out.iter_mut().zip(p) {
            *o += v * weight;
        }
    }
    out
}

/// `top` laid over `bottom`, both premultiplied.
fn over(top: Premul, bottom: Premul) -> Premul {
    let keep = 1.0 - top[3];
    std::array::from_fn(|i| top[i] + bottom[i] * keep)
}

/// The icon of a unit of `side` whose kind is drawn with `picture` (`None`:
/// the disc alone), `size` pixels across: straight-alpha RGBA, top row first.
pub fn unit_icon(picture: Option<&Image>, side: Side, level: u8, size: i32) -> Image {
    let size = size.max(0);
    let n = size as usize;
    let outer = size as f32 / 2.0;
    let (ring, width) = level_ring(level);
    let inner = outer * DISC_RADIUS / (DISC_RADIUS + width);
    let [r, g, b] = side_color(side);
    // Where the picture goes: the middle of its drawn part on the middle of
    // the disc, as large as fits inside the ring.
    let placed = picture.and_then(|p| drawn_part(p).map(|(middle, reach)| (p, middle, inner * PICTURE_REACH / reach)));

    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    let d = ((px - outer).powi(2) + (py - outer).powi(2)).sqrt();
                    if d > outer {
                        continue;
                    }
                    let mut color = if d > inner { [ring[0], ring[1], ring[2], 1.0] } else { [r, g, b, 1.0] };
                    if let Some((picture, middle, scale)) = placed {
                        let at_x = middle[0] + (px - outer) / scale;
                        let at_y = middle[1] + (py - outer) / scale;
                        color = over(sample(picture, at_x, at_y), color);
                    }
                    for (s, c) in sum.iter_mut().zip(color) {
                        *s += c;
                    }
                }
            }
            let count = (SAMPLES * SAMPLES) as f32;
            let alpha = sum[3] / count;
            let straight = |c: f32| if alpha > 0.0 { (c / count / alpha).clamp(0.0, 1.0) } else { 0.0 };
            let byte = |v: f32| (v * 255.0).round() as u8;
            rgba.extend([byte(straight(sum[0])), byte(straight(sum[1])), byte(straight(sum[2])), byte(alpha)]);
        }
    }
    Image { size: Size::new(size, size), rgba }
}

/// `icon` with a small bronze shield in its lower right: the unit defends
/// (`Order::Defend`). The shield is flat on top and round below, outlined
/// dark, two fifths of the icon high.
pub fn defender_badge(mut icon: Image) -> Image {
    const FILL: [f32; 3] = [0.80, 0.64, 0.32];
    const LINE: [f32; 3] = [0.12, 0.10, 0.08];
    let n = icon.size.w.max(0) as usize;
    let s = n as f32;
    // In the badge's own units: its middle at (0, 0), 1 its half width.
    let (cx, cy, half) = (s * 0.78, s * 0.76, s * 0.19);
    let inside = |x: f32, y: f32, grow: f32| {
        let (u, v) = ((x - cx) / half, (y - cy) / half);
        let top = v >= -1.0 - grow && u.abs() <= 1.0 + grow && v <= 0.2;
        let round = u * u + (v - 0.2) * (v - 0.2) <= (1.0 + grow) * (1.0 + grow) && v > 0.2;
        top || round
    };
    for y in 0..n {
        for x in 0..n {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    if inside(px, py, 0.0) {
                        let edge = !inside(px, py, -0.28);
                        let c = if edge { LINE } else { FILL };
                        sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2], sum[3] + 1.0];
                    }
                }
            }
            let count = (SAMPLES * SAMPLES) as f32;
            let a = sum[3] / count;
            if a <= 0.0 {
                continue;
            }
            let o = (y * n + x) * 4;
            let under: [u8; 4] = [icon.rgba[o], icon.rgba[o + 1], icon.rgba[o + 2], icon.rgba[o + 3]];
            let ua = f32::from(under[3]) / 255.0;
            let out_a = a + ua * (1.0 - a);
            for i in 0..3 {
                let top = sum[i] / sum[3];
                let bottom = f32::from(under[i]) / 255.0;
                let v = (top * a + bottom * ua * (1.0 - a)) / out_a.max(1e-6);
                icon.rgba[o + i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            icon.rgba[o + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    icon
}

/// A gold coin, `size` pixels across, for the treasury: a disc of gold with a
/// darker rim and a lighter face, lit from the top left. Straight-alpha RGBA.
pub fn coin_icon(size: i32) -> Image {
    const RIM: [f32; 3] = [0.62, 0.45, 0.10];
    const FACE: [f32; 3] = [0.96, 0.78, 0.25];
    const SHINE: [f32; 3] = [1.0, 0.94, 0.62];
    let size = size.max(0);
    let n = size as usize;
    let outer = size as f32 / 2.0;
    let face = outer * 0.74;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (mut color, mut cover) = ([0.0f32; 3], 0.0f32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32 - outer;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32 - outer;
                    let d = (px * px + py * py).sqrt();
                    if d > outer {
                        continue;
                    }
                    // The face, lighter toward the top left.
                    let c = if d > face {
                        RIM
                    } else {
                        let light = ((-px - py) / (2.0 * face) + 0.5).clamp(0.0, 1.0) * 0.6;
                        std::array::from_fn(|i| FACE[i] + (SHINE[i] - FACE[i]) * light)
                    };
                    for (s, v) in color.iter_mut().zip(c) {
                        *s += v;
                    }
                    cover += 1.0;
                }
            }
            let alpha = cover / (SAMPLES * SAMPLES) as f32;
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let c = |i: usize| if cover > 0.0 { color[i] / cover } else { 0.0 };
            rgba.extend([byte(c(0)), byte(c(1)), byte(c(2)), byte(alpha)]);
        }
    }
    Image { size: Size::new(size, size), rgba }
}

/// A small city, `size` pixels across, for the menu bar's count of the
/// side's cities (the owner's word, 2026-11-24): two towers and a wall
/// between them, each with its battlements, a dark gate, in stone lit from
/// the top left inside a dark line. Straight-alpha RGBA. (Drawn here, not
/// taken from anywhere: rectangles and an arch.)
pub fn city_icon(size: i32) -> Image {
    const STONE: [f32; 3] = [0.78, 0.74, 0.66];
    const LIGHT: [f32; 3] = [0.95, 0.92, 0.85];
    const WALL: [f32; 3] = [0.62, 0.58, 0.51];
    const LINE: [f32; 3] = [0.18, 0.15, 0.12];
    const DARK: [f32; 3] = [0.14, 0.11, 0.09];
    type Part = [f32; 4];
    // In shares of the picture, x right and y down: [left, top, right, bottom].
    // A tower: its body and its two merlons; the wall between them, lower.
    const TOWERS: [[Part; 3]; 2] = [
        [[0.06, 0.26, 0.36, 0.93], [0.06, 0.10, 0.15, 0.27], [0.27, 0.10, 0.36, 0.27]],
        [[0.64, 0.26, 0.94, 0.93], [0.64, 0.10, 0.73, 0.27], [0.85, 0.10, 0.94, 0.27]],
    ];
    const WALL_PARTS: [Part; 3] = [[0.30, 0.50, 0.70, 0.93], [0.38, 0.38, 0.46, 0.51], [0.54, 0.38, 0.62, 0.51]];
    // A window in each tower, and the gate's doorway (an arch on it).
    const WINDOWS: [Part; 2] = [[0.18, 0.42, 0.24, 0.58], [0.76, 0.42, 0.82, 0.58]];
    let line = 0.045;
    let within = |parts: &[Part], x: f32, y: f32, grow: f32| {
        parts.iter().any(|[l, t, r, b]| x >= l - grow && x <= r + grow && y >= t - grow && y <= b + grow)
    };
    let gate = |x: f32, y: f32| {
        let (cx, top, half) = (0.5, 0.72, 0.075);
        (x - cx).abs() <= half && y <= 0.93 && (y >= top || (x - cx).powi(2) + (y - top).powi(2) <= half * half)
    };
    // Lit from the top left.
    let shade = |base: [f32; 3], x: f32, y: f32| -> [f32; 3] {
        let light = ((1.0 - x - y) * 0.45).clamp(0.0, 1.0);
        std::array::from_fn(|i| base[i] + (LIGHT[i] - base[i]) * light)
    };
    // What is at a point: the towers over the wall, each inside its line.
    let at = |x: f32, y: f32| -> Option<[f32; 3]> {
        for tower in &TOWERS {
            if within(tower, x, y, 0.0) {
                return Some(if within(&WINDOWS, x, y, 0.0) { DARK } else { shade(STONE, x, y) });
            }
            if within(tower, x, y, line) {
                return Some(LINE);
            }
        }
        if gate(x, y) {
            return Some(DARK);
        }
        if within(&WALL_PARTS, x, y, 0.0) {
            return Some(shade(WALL, x, y));
        }
        within(&WALL_PARTS, x, y, line).then_some(LINE)
    };
    let size = size.max(0);
    let n = size as usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for py in 0..n {
        for px in 0..n {
            let (mut color, mut cover) = ([0.0f32; 3], 0.0f32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let Some(c) = at(x, y) else { continue };
                    for (s, v) in color.iter_mut().zip(c) {
                        *s += v;
                    }
                    cover += 1.0;
                }
            }
            let alpha = cover / (SAMPLES * SAMPLES) as f32;
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let c = |i: usize| if cover > 0.0 { color[i] / cover } else { 0.0 };
            rgba.extend([byte(c(0)), byte(c(1)), byte(c(2)), byte(alpha)]);
        }
    }
    Image { size: Size::new(size, size), rgba }
}

#[cfg(test)]
mod tests {
    use super::super::map_renderer::{DISC_RING, RING_COLOR};
    use super::*;
    use crate::render::AssetDirs;
    use std::path::Path;

    fn pixel(icon: &Image, x: i32, y: i32) -> [u8; 4] {
        let i = (y * icon.size.w + x) as usize * 4;
        icon.rgba[i..i + 4].try_into().unwrap()
    }

    fn byte(c: f32) -> u8 {
        (c * 255.0).round() as u8
    }

    fn assets() -> AssetDirs {
        AssetDirs::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"), None)
    }

    #[test]
    fn a_disc_of_the_sides_color_in_a_dark_ring() {
        let icon = unit_icon(None, Side::Blue, 1, 40);
        assert_eq!(icon.size, Size::new(40, 40));
        assert_eq!(icon.rgba.len(), 40 * 40 * 4);
        let [r, g, b] = side_color(Side::Blue);
        assert_eq!(pixel(&icon, 20, 20), [byte(r), byte(g), byte(b), 255]);
        // The ring, just inside the edge; outside the disc nothing.
        let [rr, rg, rb, _] = RING_COLOR;
        assert_eq!(pixel(&icon, 20, 0), [byte(rr), byte(rg), byte(rb), 255]);
        assert_eq!(pixel(&icon, 0, 0)[3], 0);
        assert_eq!(pixel(&icon, 39, 39)[3], 0);
        // The edge is smoothed: some pixels are partly covered.
        assert!(icon.rgba.chunks(4).any(|p| p[3] > 0 && p[3] < 255));
    }

    #[test]
    fn a_defenders_badge_is_a_bronze_shield_in_the_lower_right() {
        let plain = unit_icon(None, Side::Blue, 1, 40);
        let marked = defender_badge(plain.clone());
        assert_eq!(marked.size, plain.size);
        // In the badge's middle: bronze; in the upper left: the icon as it was.
        let at = |icon: &Image, x: i32, y: i32| pixel(icon, x, y);
        let bronze = at(&marked, 31, 31);
        assert!(bronze[0] > 180 && bronze[1] > 140 && bronze[2] < 120 && bronze[3] == 255, "{bronze:?}");
        assert_eq!(at(&marked, 10, 10), at(&plain, 10, 10));
        assert_ne!(marked.rgba, plain.rgba);
    }

    #[test]
    fn the_kinds_picture_is_in_the_middle_of_the_disc() {
        let sign = assets().image("units/lightcavalry.png").unwrap();
        let icon = unit_icon(Some(&sign), Side::Red, 1, 48);
        // The sign's light body shows on the disc, and the disc around it.
        let light = |p: &[u8]| p[0] > 200 && p[1] > 200 && p[2] > 180 && p[3] == 255;
        let lights = icon.rgba.chunks(4).filter(|p| light(p)).count();
        assert!(lights > 48 * 48 / 20, "{lights} light pixels");
        let [r, g, b] = side_color(Side::Red);
        assert!(icon.rgba.chunks(4).any(|p| p == [byte(r), byte(g), byte(b), 255]));
        // At any size.
        for size in [1, 2, 17, 80] {
            let icon = unit_icon(Some(&sign), Side::Red, 1, size);
            assert_eq!(icon.rgba.len(), (size * size * 4) as usize);
        }
        assert!(unit_icon(Some(&sign), Side::Red, 1, 0).rgba.is_empty());
    }

    #[test]
    fn a_picture_that_fills_its_frame_is_kept_inside_the_disc() {
        // The default unit's picture is a frame to its edges: none of it is
        // drawn over the ring.
        let picture = assets().image(crate::state::DEFAULT_UNIT_IMAGE).unwrap();
        let icon = unit_icon(Some(&picture), Side::Black, 1, 40);
        let inner = 20.0 * DISC_RADIUS / (DISC_RADIUS + DISC_RING);
        let [rr, rg, rb, _] = RING_COLOR;
        let mut ring = 0;
        for y in 0..40 {
            for x in 0..40 {
                let d = ((x as f32 + 0.5 - 20.0).powi(2) + (y as f32 + 0.5 - 20.0).powi(2)).sqrt();
                if d > inner + 0.75 && d < 20.0 - 0.75 {
                    assert_eq!(pixel(&icon, x, y), [byte(rr), byte(rg), byte(rb), 255], "{x}, {y}");
                    ring += 1;
                }
            }
        }
        assert!(ring > 0);
        // And it is drawn: black lines on the disc.
        assert!(icon.rgba.chunks(4).any(|p| p[0] < 10 && p[3] == 255));
        // A picture with nothing drawn on it is the disc alone.
        let empty = Image { size: Size::new(8, 8), rgba: vec![0; 8 * 8 * 4] };
        assert_eq!(unit_icon(Some(&empty), Side::Black, 1, 30).rgba, unit_icon(None, Side::Black, 1, 30).rgba);
        // So is one whose bytes run short.
        let short = Image { size: Size::new(8, 8), rgba: vec![255; 10] };
        assert_eq!(unit_icon(Some(&short), Side::Black, 1, 30).rgba, unit_icon(None, Side::Black, 1, 30).rgba);
    }

    /// The menu bar's city (the owner's word, 2026-11-24: "add a small city
    /// icon and have my city count/total cities on the menu bar").
    #[test]
    fn the_city_is_stone_with_a_dark_gate_and_clear_around() {
        let icon = city_icon(32);
        assert_eq!((icon.size, icon.rgba.len()), (Size::new(32, 32), 32 * 32 * 4));
        // The corners are clear; the gate is dark; a tower is stone.
        assert_eq!(pixel(&icon, 0, 0)[3], 0);
        assert_eq!(pixel(&icon, 31, 0)[3], 0);
        let gate = pixel(&icon, 16, 28);
        assert!(gate[3] == 255 && gate[0] < 60, "{gate:?}");
        let tower = pixel(&icon, 4, 26);
        assert!(tower[3] == 255 && tower[0] > 170, "{tower:?}");
        // Over the wall between the towers, the sky; a tower's window, dark.
        assert_eq!(pixel(&icon, 16, 6)[3], 0);
        let window = pixel(&icon, 6, 16);
        assert!(window[3] == 255 && window[0] < 60, "{window:?}");
        // No size, no pixels; and it is the same each time.
        assert!(city_icon(0).rgba.is_empty());
        assert_eq!(city_icon(16).rgba, city_icon(16).rgba);
    }
}
