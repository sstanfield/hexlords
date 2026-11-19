//! Numbers that look random but are not: a hash of a grid point, value noise
//! that wraps, and fractal noise that repeats on the hex grid, which is what
//! makes a tile run on into its neighbours without a seam.
//!
//! Points are in screen space: x from -1 to 1 across the hex, y from
//! -[`ASPECT`] to [`ASPECT`] down it, one unit the same length either way.

use crate::ASPECT;

/// A hash of a grid point and a seed.
pub fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h =
        (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27d4_eb2d);
    h ^= h >> 16;
    h
}

/// The hash as a number from 0 up to (not including) 1.
pub fn unit(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// 0 to 1 with no slope at either end.
pub fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on an `n` x `n` grid that wraps: it repeats when `u` or `v`
/// grows by 1.
pub fn value_noise(u: f32, v: f32, n: i32, seed: u32) -> f32 {
    let (fx, fy) = (u * n as f32, v * n as f32);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (smooth(fx - ix), smooth(fy - iy));
    let (ix, iy) = (ix as i32, iy as i32);
    let g = |a: i32, b: i32| unit(a.rem_euclid(n), b.rem_euclid(n), seed);
    let top = g(ix, iy) + (g(ix + 1, iy) - g(ix, iy)) * tx;
    let bottom = g(ix, iy + 1) + (g(ix + 1, iy + 1) - g(ix, iy + 1)) * tx;
    top + (bottom - top) * ty
}

/// A point in the hex grid's own coordinates: `u` grows by 1 from a hex to its
/// lower right neighbour (1.5 right and [`ASPECT`] down), `v` by 1 to the hex
/// below (2 x [`ASPECT`] down). Whatever repeats in `u` and `v` repeats from
/// hex to hex.
pub fn lattice(x: f32, y: f32) -> (f32, f32) {
    let u = x / 1.5;
    (u, (y - u * ASPECT) / (2.0 * ASPECT))
}

/// Fractal noise that repeats on the hex grid: `octaves` layers of value
/// noise, the first with `n` cells a hex, each next one twice as fine and half
/// as strong. About 0.5 on average, seldom near 0 or 1.
pub fn fractal(x: f32, y: f32, n: i32, octaves: u32, seed: u32) -> f32 {
    let (u, v) = lattice(x, y);
    let (mut sum, mut amplitude, mut total, mut cells) = (0.0, 1.0, 0.0, n);
    for octave in 0..octaves {
        sum += amplitude * value_noise(u, v, cells, seed.wrapping_add(octave * 7919));
        total += amplitude;
        amplitude *= 0.5;
        cells *= 2;
    }
    sum / total
}

/// [`fractal`], stretched to use more of 0 to 1.
pub fn spread(x: f32, y: f32, n: i32, octaves: u32, seed: u32) -> f32 {
    ((fractal(x, y, n, octaves, seed) - 0.5) * 2.0 + 0.5).clamp(0.0, 1.0)
}

/// Fractal noise over a plane that does not repeat (it does, every 4096
/// units): for the continent of the far preview.
pub fn plain(x: f32, y: f32, seed: u32) -> f32 {
    let (mut sum, mut amplitude, mut total, mut frequency) = (0.0, 1.0, 0.0, 1.0);
    for octave in 0..5 {
        sum += amplitude * value_noise(x * frequency / 4096.0, y * frequency / 4096.0, 4096, seed + octave);
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_spread_and_steady() {
        assert_eq!(hash(3, 4, 5), hash(3, 4, 5));
        assert_ne!(hash(3, 4, 5), hash(4, 3, 5));
        assert_ne!(hash(3, 4, 5), hash(3, 4, 6));
        let mean: f32 = (0..4000).map(|i| unit(i, i / 7, 9)).sum::<f32>() / 4000.0;
        assert!((mean - 0.5).abs() < 0.03, "{mean}");
        assert!((0..4000).all(|i| (0.0..1.0).contains(&unit(i, -i, 1))));
    }

    #[test]
    fn value_noise_wraps_and_meets_the_grid() {
        for (u, v) in [(0.13, 0.7), (0.5, 0.5), (0.99, 0.01)] {
            let here = value_noise(u, v, 5, 3);
            assert!((here - value_noise(u + 1.0, v, 5, 3)).abs() < 1e-4);
            assert!((here - value_noise(u, v - 2.0, 5, 3)).abs() < 1e-4);
            assert!((0.0..=1.0).contains(&here));
        }
        // On a grid point it is that point's number.
        assert_eq!(value_noise(0.4, 0.6, 5, 3), unit(2, 3, 3));
    }

    #[test]
    fn fractal_noise_repeats_from_hex_to_hex() {
        // The six neighbours of a hex, in screen space.
        let steps =
            [(1.5, ASPECT), (1.5, -ASPECT), (0.0, 2.0 * ASPECT), (-1.5, ASPECT), (-1.5, -ASPECT), (0.0, -2.0 * ASPECT)];
        for i in 0..200 {
            let (x, y) = (unit(i, 0, 77) * 2.0 - 1.0, unit(i, 1, 77) * 1.7 - 0.85);
            let here = fractal(x, y, 4, 3, 21);
            assert!((0.0..=1.0).contains(&here));
            for (dx, dy) in steps {
                let there = fractal(x + dx, y + dy, 4, 3, 21);
                assert!((here - there).abs() < 2e-3, "({x}, {y}) + ({dx}, {dy}): {here} {there}");
            }
            assert!((0.0..=1.0).contains(&spread(x, y, 3, 4, 11)));
        }
    }

    #[test]
    fn the_lattice_counts_hexes() {
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(close(lattice(0.0, 0.0), (0.0, 0.0)));
        assert!(close(lattice(1.5, ASPECT), (1.0, 0.0)));
        assert!(close(lattice(0.0, 2.0 * ASPECT), (0.0, 1.0)));
        assert!(close(lattice(1.5, -ASPECT), (1.0, -1.0)));
    }
}
