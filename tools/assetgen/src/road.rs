//! Roads and bridges: the pictures laid over a hex that has a road.
//!
//! A road leaves a hex through the middle of each side whose neighbour has a
//! road too (or is a city), straight across the side, so that it goes on in
//! the neighbour without a step. Which sides those are is a mask of six bits,
//! one for each side clockwise from the top: 64 shapes. Turning a shape by a
//! sixth of a turn gives another, so there are only 14 to draw; the game
//! draws each turned as it is needed (`turn`).
//!
//! Over land the road is packed earth; over water it is a bridge of planks.

use crate::ASPECT;
use crate::noise::{fractal, unit};
use crate::overlay::Ink;
use crate::shapes::{Rgb, length, mix, scale};

/// The sides of a hex, clockwise from the top: north, north-east,
/// south-east, south, south-west, north-west. A mask has a bit for each.
pub const SIDES: usize = 6;

/// A mask turned clockwise by `turns` sixths of a turn: the road that left
/// through the top leaves through the upper right.
pub fn turned(mask: u8, turns: u8) -> u8 {
    let turns = turns % 6;
    let mask = mask & 0x3f;
    ((mask << turns) | (mask >> (6 - turns))) & 0x3f
}

/// The shape a mask is drawn with, and how far that shape is turned
/// clockwise to give the mask: `turned(shape, turns) == mask`. The shape is
/// the smallest mask among the six turns.
pub fn shape_of(mask: u8) -> (u8, u8) {
    let (shape, back) = (0..6).map(|k| (turned(mask, k), k)).min().unwrap_or((0, 0));
    (shape, (6 - back) % 6)
}

/// The 14 shapes there are pictures of, smallest first.
pub fn shapes() -> Vec<u8> {
    let mut shapes: Vec<u8> = (0..64).map(|mask| shape_of(mask).0).collect();
    shapes.sort_unstable();
    shapes.dedup();
    shapes
}

/// A point of the hex model (x right, y *up*, the hex 2 wide and 2 high)
/// turned counter-clockwise by `turns` sixths of a turn, as the hex turns
/// onto itself: where in a shape's picture the game finds what it draws at
/// the point of a hex whose road is that shape turned clockwise by `turns`.
pub fn turn(x: f32, y: f32, turns: u8) -> (f32, f32) {
    // The model's hex is a regular one stretched upward by 2 / √3.
    let h = 3f32.sqrt() / 2.0;
    let (sin, cos) = (f32::from(turns % 6) * std::f32::consts::FRAC_PI_3).sin_cos();
    (x * cos - y * h * sin, (x * sin + y * h * cos) / h)
}

/// The middle of a side, in screen space.
fn side_middle(side: usize) -> (f32, f32) {
    // In the model, y up: clockwise from the top.
    const MIDDLES: [(f32, f32); SIDES] =
        [(0.0, 1.0), (0.75, 0.5), (0.75, -0.5), (0.0, -1.0), (-0.75, -0.5), (-0.75, 0.5)];
    let (x, y) = MIDDLES[side];
    (x, -y * ASPECT)
}

/// How far past a side's middle a road is drawn, so that the picture has no
/// end where the hex ends.
const OVERSHOOT: f32 = 1.2;
/// The straight pieces a curve is made of.
const PIECES: usize = 24;

/// The lines a shape's road runs along, each a chain of points in screen
/// space. One side: from the side to the middle of the hex. Two: a curve from
/// one to the other that crosses both sides squarely (straight, if they are
/// opposite). More: from each side to the middle, where they meet. None: a
/// spot in the middle.
pub fn lines(mask: u8) -> Vec<Vec<(f32, f32)>> {
    let sides: Vec<usize> = (0..SIDES).filter(|side| mask & (1 << side) != 0).collect();
    let beyond = |side: usize| {
        let (x, y) = side_middle(side);
        (x * OVERSHOOT, y * OVERSHOOT)
    };
    match sides[..] {
        [] => vec![vec![(0.0, 0.0), (0.0, 0.0)]],
        [a, b] => {
            // A curve pulled toward the middle of the hex: it leaves each
            // side heading for the middle, which is squarely.
            let (pa, pb) = (side_middle(a), side_middle(b));
            let mut line = vec![beyond(a)];
            for i in 0..=PIECES {
                let t = i as f32 / PIECES as f32;
                let (wa, wb) = ((1.0 - t) * (1.0 - t), t * t);
                line.push((wa * pa.0 + wb * pb.0, wa * pa.1 + wb * pb.1));
            }
            line.push(beyond(b));
            vec![line]
        }
        _ => sides.iter().map(|side| vec![beyond(*side), (0.0, 0.0)]).collect(),
    }
}

/// How far a point is from the nearest of the lines, and how far along that
/// line (from its start) the nearest place is.
fn nearest(lines: &[Vec<(f32, f32)>], x: f32, y: f32) -> (f32, f32) {
    let mut best = (f32::MAX, 0.0);
    for line in lines {
        let mut along = 0.0;
        for pair in line.windows(2) {
            let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
            let (vx, vy) = (bx - ax, by - ay);
            let len2 = vx * vx + vy * vy;
            let t = if len2 > 0.0 { (((x - ax) * vx + (y - ay) * vy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let d = length(x - (ax + vx * t), y - (ay + vy * t));
            if d < best.0 {
                best = (d, along + len2.sqrt() * t);
            }
            along += len2.sqrt();
        }
    }
    best
}

/// The color of a road's rim, and of the nothing around it.
pub const ROAD_RIM: Rgb = [0.38, 0.30, 0.19];
/// The color of a bridge's rails, and of the nothing around it.
pub const BRIDGE_RIM: Rgb = [0.24, 0.16, 0.09];
/// Half a road's width (a hex is 2 wide).
const ROAD_HALF: f32 = 0.115;
const BRIDGE_HALF: f32 = 0.15;

/// A road over land with this shape: packed earth, lighter than the ground
/// it crosses, with a dark rim.
pub fn road(mask: u8) -> impl Fn(f32, f32) -> Ink {
    let lines = lines(mask);
    // A road that goes nowhere is a round place a little wider than a road.
    let half = if mask & 0x3f == 0 { ROAD_HALF * 1.5 } else { ROAD_HALF };
    move |x, y| {
        let (d, _) = nearest(&lines, x, y);
        if d >= half {
            return None;
        }
        if d > half - 0.028 {
            return Some((ROAD_RIM, 1.0));
        }
        let earth = mix([0.74, 0.63, 0.44], [0.84, 0.74, 0.54], fractal(x, y, 9, 2, 141));
        // Two faint ruts.
        let rut = if (d - half * 0.42).abs() < 0.012 { 0.9 } else { 1.0 };
        Some((scale(earth, rut), 1.0))
    }
}

/// A bridge with this shape: planks across the way between two rails, and
/// its shadow on the water.
pub fn bridge(mask: u8) -> impl Fn(f32, f32) -> Ink {
    let lines = lines(mask);
    let half = if mask & 0x3f == 0 { BRIDGE_HALF * 1.3 } else { BRIDGE_HALF };
    move |x, y| {
        let (d, along) = nearest(&lines, x, y);
        if d >= half {
            // The shadow falls to the lower right.
            let (shadow, _) = nearest(&lines, x - 0.045, y - 0.055);
            return (shadow < half).then_some(([0.02, 0.05, 0.10], 0.4));
        }
        if d > half - 0.04 {
            return Some((BRIDGE_RIM, 1.0));
        }
        let plank = along / 0.075;
        if plank.fract() < 0.12 {
            return Some((scale(BRIDGE_RIM, 1.3), 1.0)); // the gap between two planks
        }
        let tone = unit(plank.floor() as i32, i32::from(mask), 151);
        Some((mix([0.52, 0.37, 0.21], [0.66, 0.49, 0.29], tone), 1.0))
    }
}

/// The file of a shape's picture: `road_05.png`, `bridge_05.png`.
pub fn file_name(over_water: bool, shape: u8) -> String {
    format!("{}_{shape:02}.png", if over_water { "bridge" } else { "road" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_four_masks_are_fourteen_shapes() {
        assert_eq!(shapes(), [0, 1, 3, 5, 7, 9, 11, 13, 15, 21, 23, 27, 31, 63]);
        for mask in 0..64u8 {
            let (shape, turns) = shape_of(mask);
            assert!(shapes().contains(&shape) && turns < 6);
            assert_eq!(turned(shape, turns), mask, "mask {mask}");
            assert_eq!(mask.count_ones(), shape.count_ones());
        }
        // North alone turned once is north-east alone; six turns are none.
        assert_eq!(turned(0b000001, 1), 0b000010);
        assert_eq!(turned(0b100000, 1), 0b000001);
        assert_eq!(turned(0b001011, 6), 0b001011);
        assert_eq!(shape_of(0b000010), (1, 1));
        // North-west to south-east is the straight road, turned (twice or five
        // times: it is the same either way).
        assert_eq!(shape_of(0b100100).0, 9);
    }

    #[test]
    fn turning_a_point_takes_a_side_to_the_one_before() {
        // The middles of the sides in the model, y up, clockwise from the top.
        let middles = [(0.0, 1.0), (0.75, 0.5), (0.75, -0.5), (0.0, -1.0), (-0.75, -0.5), (-0.75, 0.5)];
        for turns in 0..6u8 {
            for (side, (x, y)) in middles.into_iter().enumerate() {
                let (tx, ty) = turn(x, y, turns);
                let (wx, wy) = middles[(side + 6 - usize::from(turns)) % 6];
                assert!((tx - wx).abs() < 1e-5 && (ty - wy).abs() < 1e-5, "side {side}, {turns} turns: ({tx}, {ty})");
            }
        }
        // The corners go to corners too: the hex turns onto itself.
        let (x, y) = turn(1.0, 0.0, 1);
        assert!((x - 0.5).abs() < 1e-5 && (y - 1.0).abs() < 1e-5);
    }

    /// True if the road covers the point.
    fn on(ink: &impl Fn(f32, f32) -> Ink, x: f32, y: f32) -> bool {
        ink(x, y).is_some_and(|(_, alpha)| alpha == 1.0)
    }

    #[test]
    fn a_road_leaves_through_its_sides_and_no_others() {
        for mask in shapes() {
            let (road, bridge) = (road(mask), bridge(mask));
            for side in 0..SIDES {
                let (x, y) = side_middle(side);
                let leaves = mask & (1 << side) != 0;
                // On the side's middle, and a little outside the hex.
                for k in [1.0, 1.08] {
                    assert_eq!(on(&road, x * k, y * k), leaves, "road {mask}, side {side}");
                    assert_eq!(on(&bridge, x * k, y * k), leaves, "bridge {mask}, side {side}");
                }
                // It crosses the side squarely: as wide to the left as to the right.
                if leaves {
                    let (ex, ey) = (-y / length(x, y), x / length(x, y));
                    let width = |sign: f32| {
                        (0..40)
                            .filter(|i| on(&road, x + sign * ex * *i as f32 * 0.005, y + sign * ey * *i as f32 * 0.005))
                            .count()
                    };
                    assert!(
                        width(1.0).abs_diff(width(-1.0)) <= 1,
                        "road {mask}, side {side}: {} {}",
                        width(1.0),
                        width(-1.0)
                    );
                    assert!((20..28).contains(&width(1.0)), "road {mask}: half its width is {} steps", width(1.0));
                }
            }
            // The corners of the hex are never road.
            for (x, y) in [(1.0, 0.0), (-1.0, 0.0), (0.5, ASPECT), (-0.5, -ASPECT)] {
                assert!(!on(&road, x * 0.97, y * 0.97), "road {mask} at a corner");
            }
        }
    }

    #[test]
    fn the_shapes_of_a_road() {
        // Straight through, north to south: the middle and all between.
        let straight = road(0b001001);
        assert!((-8..=8).all(|i| on(&straight, 0.0, i as f32 * 0.1)));
        assert!(!on(&straight, 0.3, 0.0));
        // A bend between neighbouring sides keeps away from the middle.
        let bend = road(0b000011);
        assert!(!on(&bend, 0.0, 0.0) && !on(&bend, -0.2, 0.3));
        // A fork meets in the middle; a dead end stops there; a road to
        // nowhere is a place in the middle.
        assert!(on(&road(0b010101), 0.0, 0.0));
        let end = road(0b000001);
        assert!(on(&end, 0.0, -0.4) && on(&end, 0.0, 0.0) && !on(&end, 0.0, 0.3));
        let place = road(0);
        assert!(on(&place, 0.0, 0.0) && on(&place, 0.12, 0.0) && !on(&place, 0.3, 0.0));
    }

    #[test]
    fn a_bridge_has_rails_planks_and_a_shadow() {
        let bridge = bridge(0b001001);
        assert_eq!(bridge(0.13, 0.0), Some((BRIDGE_RIM, 1.0)), "the rail");
        let (deck, alpha) = bridge(0.03, 0.03).unwrap();
        assert!(alpha == 1.0 && deck[0] > deck[2], "wood: {deck:?}");
        // Beside it to the right, its shadow: dark and thin.
        let (shadow, alpha) = bridge(0.17, 0.0).unwrap();
        assert!(alpha < 0.5 && shadow[0] < 0.1);
        assert_eq!(bridge(-0.17, 0.0), None, "no shadow on the lit side");
        // It is wider than a road.
        assert!(road(0b001001)(0.13, 0.0).is_none());
        assert_eq!(file_name(false, 5), "road_05.png");
        assert_eq!(file_name(true, 63), "bridge_63.png");
    }
}
