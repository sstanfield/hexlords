//! Pictures that are laid over a tile and let it show through: roads,
//! bridges, the units' icons. They lie in their image as a tile does (the hex
//! fills the square; see `tile`), so the game draws them with the same mesh.
//!
//! The game scales them smoothly, as it does the tiles, and that mixes each
//! pixel with its neighbours, the transparent ones too. So a transparent pixel
//! here is not black: it has the color of the picture's outline and no
//! opacity, and what is mixed in at an edge is more outline, not a dark rim.

use crate::ASPECT;
use crate::shapes::Rgb;

/// What a picture has at a point: a color and how much of it (1: nothing
/// shows through), or nothing.
pub type Ink = Option<(Rgb, f32)>;

/// Draw a picture as a `size` x `size` image: straight-alpha RGBA, the top
/// row first. `ink` is asked for points in screen space, as the terrains are
/// (x from -1 to 1 across the hex, y from -ASPECT *down* to ASPECT); where a
/// pixel has no ink at all it gets `empty`, with no opacity.
pub fn render(size: usize, empty: Rgb, ink: &dyn Fn(f32, f32) -> Ink) -> Vec<u8> {
    let samples = if size <= 64 { 6 } else { 4 };
    let pixel = 2.0 / size as f32;
    let mut out = vec![0u8; size * size * 4];
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for py in 0..size {
        for px in 0..size {
            // The colors weighted by their opacity, and the opacity.
            let (mut sum, mut opacity) = ([0f32; 3], 0f32);
            for sy in 0..samples {
                for sx in 0..samples {
                    let x = (px as f32 + (sx as f32 + 0.5) / samples as f32) * pixel - 1.0;
                    let y = (py as f32 + (sy as f32 + 0.5) / samples as f32) * pixel - 1.0;
                    if let Some((color, alpha)) = ink(x, y * ASPECT) {
                        for (total, part) in sum.iter_mut().zip(color) {
                            *total += part * alpha;
                        }
                        opacity += alpha;
                    }
                }
            }
            let o = (py * size + px) * 4;
            let color = if opacity > 0.0 { sum.map(|total| total / opacity) } else { empty };
            for (out, part) in out[o..o + 3].iter_mut().zip(color) {
                *out = byte(part);
            }
            out[o + 3] = byte(opacity / (samples * samples) as f32);
        }
    }
    out
}

/// Lay one straight-alpha pixel over an opaque one.
pub fn over(under: [u8; 3], top: &[u8]) -> [u8; 3] {
    let alpha = u32::from(top[3]);
    std::array::from_fn(|k| ((u32::from(top[k]) * alpha + u32::from(under[k]) * (255 - alpha) + 127) / 255) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_has_its_ink_and_the_rest_is_outline_colored_nothing() {
        // A red disc in the middle, half see-through on its right half.
        let empty = [0.2, 0.1, 0.0];
        let disc = |x: f32, y: f32| -> Ink {
            (x * x + y * y < 0.25).then_some(([1.0, 0.0, 0.0], if x > 0.0 { 0.5 } else { 1.0 }))
        };
        let size = 32;
        let image = render(size, empty, &disc);
        let at = |x: usize, y: usize| &image[(y * size + x) * 4..(y * size + x) * 4 + 4];
        assert_eq!(at(12, 16), [255, 0, 0, 255]);
        assert_eq!(at(20, 16), [255, 0, 0, 128]);
        // Outside: the outline's color, with no opacity.
        assert_eq!(at(1, 1), [51, 26, 0, 0]);
        // On the rim the color is the ink's, only thinner: no dark edge.
        let rim = (0..size).map(|x| at(x, 16)).find(|p| p[3] > 0 && p[3] < 255).expect("a pixel on the rim");
        assert_eq!(rim[..3], [255, 0, 0]);
        // The squeeze: the disc is round on the screen, so taller in the image.
        let wide = (0..size).filter(|x| at(*x, 16)[3] > 0).count();
        let high = (0..size).filter(|y| at(12, *y)[3] > 0).count();
        assert!(high > wide, "{wide} wide, {high} high");
    }

    #[test]
    fn laying_over() {
        assert_eq!(over([10, 20, 30], &[200, 100, 0, 255]), [200, 100, 0]);
        assert_eq!(over([10, 20, 30], &[200, 100, 0, 0]), [10, 20, 30]);
        assert_eq!(over([0, 0, 0], &[200, 100, 50, 128]), [100, 50, 25]);
    }
}
