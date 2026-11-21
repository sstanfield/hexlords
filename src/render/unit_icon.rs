//! The panels' small pictures: a coin for the treasury ([`coin_icon`]), and
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
}
