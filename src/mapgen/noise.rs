//! Noise for the terrain: value noise on a square lattice, summed over
//! octaves (fBm), and its ridged form for mountain ranges. In `f64` and only
//! `+ - * /` and `floor`, so it is the same on every machine (no `sin`, `exp`
//! or `powf`: their last bits differ between platforms' libraries).
//!
//! It is sampled at the hexes' middles on the screen (`center`), never at
//! their rows and columns, or the terrain would stretch along the columns.

use crate::state::Hex;

use super::rng::mix;

/// A hex's middle, a hex being 1 from its middle to its corners: columns 1.5
/// apart, rows the square root of 3, an odd column half a row higher.
pub fn center(hex: Hex) -> (f64, f64) {
    let odd = f64::from(hex.col & 1);
    (1.5 * f64::from(hex.col), 3f64.sqrt() * (f64::from(hex.row) - 0.5 * odd))
}

/// A lattice point's value, 0 to 1.
fn lattice(seed: u64, x: i64, y: i64) -> f64 {
    let h = mix(
        seed ^ mix((x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f))
    );
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth from 0 to 1 between 0 and 1: `3t² - 2t³`.
fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise at a point: 0 to 1, smooth, one lattice cell per unit.
pub fn value(seed: u64, x: f64, y: f64) -> f64 {
    let (fx, fy) = (x.floor(), y.floor());
    let (ix, iy) = (fx as i64, fy as i64);
    let (tx, ty) = (smooth(x - fx), smooth(y - fy));
    let a = lattice(seed, ix, iy);
    let b = lattice(seed, ix + 1, iy);
    let c = lattice(seed, ix, iy + 1);
    let d = lattice(seed, ix + 1, iy + 1);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    top + (bottom - top) * ty
}

/// Octaves of value noise, each twice as fine and `gain` as strong as the
/// last, scaled back to 0 to 1.
pub fn fbm(seed: u64, x: f64, y: f64, octaves: u32, gain: f64) -> f64 {
    let (mut sum, mut weight, mut total, mut f) = (0.0, 1.0, 0.0, 1.0);
    for i in 0..octaves.max(1) {
        // Each octave shifted, so their lattices do not line up.
        let shift = f64::from(i) * 17.31;
        sum += weight * value(mix(seed.wrapping_add(u64::from(i))), x * f + shift, y * f - shift);
        total += weight;
        weight *= gain;
        f *= 2.0;
    }
    sum / total
}

/// Ridged noise: 1 where the noise crosses its middle, 0 at its extremes, so
/// its highs run in lines (ranges).
pub fn ridged(n: f64) -> f64 {
    1.0 - (2.0 * n - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_smooth_in_range_and_the_same_every_time() {
        let mut lo: f64 = 1.0;
        let mut hi: f64 = 0.0;
        for i in 0..2000 {
            let (x, y) = (f64::from(i) * 0.137, f64::from(i % 37) * 0.291);
            let v = fbm(5, x, y, 4, 0.5);
            assert!((0.0..=1.0).contains(&v));
            (lo, hi) = (lo.min(v), hi.max(v));
            // Near points, near values.
            assert!((value(5, x, y) - value(5, x + 0.001, y)).abs() < 0.01);
        }
        assert!(hi - lo > 0.5, "it varies: {lo} to {hi}");
        assert_eq!(fbm(9, 3.3, 4.4, 3, 0.5), fbm(9, 3.3, 4.4, 3, 0.5));
        assert_ne!(fbm(9, 3.3, 4.4, 3, 0.5), fbm(10, 3.3, 4.4, 3, 0.5));
        assert_eq!(ridged(0.5), 1.0);
        assert_eq!(ridged(0.0), 0.0);
    }

    #[test]
    fn a_hexs_middle_is_where_the_screen_has_it() {
        assert_eq!(center(Hex::new(0, 0)), (0.0, 0.0));
        let (x, y) = center(Hex::new(2, 1));
        assert_eq!(x, 1.5);
        assert!((y - 1.5 * 3f64.sqrt()).abs() < 1e-12, "an odd column half a row higher");
    }
}
