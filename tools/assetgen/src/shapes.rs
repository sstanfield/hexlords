//! Colors, distances to shapes, and light. Points are in screen space (see
//! `noise`); a distance below zero is inside the shape.

/// Red, green and blue, each 0 to 1.
pub type Rgb = [f32; 3];

/// `a` at `t` = 0, `b` at 1.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// A color made `k` times as bright.
pub fn scale(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// The length of a vector.
pub fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// The distance from a point to the line segment from `a` to `b`.
pub fn segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay, bax, bay) = (px - ax, py - ay, bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    ((pax - bax * h).powi(2) + (pay - bay * h).powi(2)).sqrt()
}

/// A point turned by an angle.
pub fn rotate(x: f32, y: f32, angle: f32) -> (f32, f32) {
    let (s, c) = angle.sin_cos();
    (x * c - y * s, x * s + y * c)
}

/// The signed distance to a box around the origin with these half sides.
pub fn rectangle(x: f32, y: f32, half_w: f32, half_h: f32) -> f32 {
    let (dx, dy) = (x.abs() - half_w, y.abs() - half_h);
    (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0)
}

/// The signed distance to a regular octagon around the origin whose flat
/// sides, `r` from the middle, face along the axes.
pub fn octagon(x: f32, y: f32, r: f32) -> f32 {
    // The directions of the two mirrors that fold an octagon onto one side, and
    // half a side's length for r = 1.
    #[allow(clippy::excessive_precision)]
    let (kx, ky, kz) = (-0.923_879_5_f32, 0.382_683_4_f32, 0.414_213_5_f32);
    let (mut px, mut py) = (x.abs(), y.abs());
    let d = 2.0 * (kx * px + ky * py).min(0.0);
    px -= d * kx;
    py -= d * ky;
    let d = 2.0 * (-kx * px + ky * py).min(0.0);
    px -= d * -kx;
    py -= d * ky;
    px -= px.max(-kz * r).min(kz * r);
    py -= r;
    (px * px + py * py).sqrt() * py.signum()
}

/// How bright a dome is at (`nx`, `ny`) from its middle (1 at its rim), lit
/// from the upper left: 0 in its shadow, about 1 where it faces the light.
pub fn light(nx: f32, ny: f32) -> f32 {
    let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
    (nx * -0.48 + ny * -0.62 + nz * 0.62).max(0.0)
}

/// A lit ball of radius `r` around the origin, with a dark rim `outline`
/// wide: its color at a point, or `None` outside it.
pub fn ball(x: f32, y: f32, r: f32, base: Rgb, outline: f32) -> Option<Rgb> {
    let d = length(x, y);
    if d >= r {
        return None;
    }
    if d > r - outline {
        return Some(scale(base, 0.35));
    }
    Some(scale(base, 0.45 + 0.75 * light(x / r, y / r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn colors() {
        assert_eq!(mix([0.0, 0.5, 1.0], [1.0, 0.5, 0.0], 0.25), [0.25, 0.5, 0.75]);
        assert_eq!(scale([0.1, 0.2, 0.4], 2.0), [0.2, 0.4, 0.8]);
    }

    #[test]
    fn distances() {
        assert!(close(length(3.0, 4.0), 5.0));
        // Beside the segment, and past its ends.
        assert!(close(segment(0.5, 1.0, 0.0, 0.0, 1.0, 0.0), 1.0));
        assert!(close(segment(-3.0, 4.0, 0.0, 0.0, 1.0, 0.0), 5.0));
        assert!(close(segment(2.0, 0.0, 0.0, 0.0, 1.0, 0.0), 1.0));
        // A box: inside, on a side, off a corner.
        assert!(close(rectangle(0.0, 0.0, 2.0, 1.0), -1.0));
        assert!(close(rectangle(3.0, 0.5, 2.0, 1.0), 1.0));
        assert!(close(rectangle(5.0, 5.0, 2.0, 1.0), 5.0));
        // An octagon: its flat sides are r away along the axes and the diagonals.
        assert!(close(octagon(0.0, 0.0, 1.0), -1.0));
        assert!(close(octagon(1.0, 0.0, 1.0), 0.0) && close(octagon(0.0, -1.0, 1.0), 0.0));
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert!(octagon(d, d, 1.0).abs() < 1e-4);
        assert!(octagon(0.9, 0.9, 1.0) > 0.0);
    }

    #[test]
    fn a_quarter_turn() {
        let (x, y) = rotate(1.0, 0.0, std::f32::consts::FRAC_PI_2);
        assert!(close(x, 0.0) && close(y, 1.0));
    }

    #[test]
    fn the_light_comes_from_the_upper_left() {
        assert!(light(-0.5, -0.5) > light(0.0, 0.0));
        assert!(light(0.0, 0.0) > light(0.5, 0.5));
        assert_eq!(light(0.7, 0.7), 0.0);
        // A ball: nothing outside, a dark rim, brighter toward the light.
        let base = [0.8, 0.8, 0.8];
        assert!(ball(1.0, 0.0, 1.0, base, 0.1).is_none());
        assert_eq!(ball(0.95, 0.0, 1.0, base, 0.1), Some(scale(base, 0.35)));
        let (lit, dark) = (ball(-0.4, -0.4, 1.0, base, 0.1).unwrap(), ball(0.4, 0.4, 1.0, base, 0.1).unwrap());
        assert!(lit[0] > dark[0]);
    }
}
