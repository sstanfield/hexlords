//! The units' pictures: a plain sign for each kind (a sword, a spear, a
//! shield, a helmet, a bow, a horseshoe, a stirrup, a banner for a
//! commander; a longship and a cog for the ships, a feather and a pair of
//! wings for the flyers), light with a dark outline,
//! on nothing. The game draws it on a
//! disc of the unit's side's color, so the sign itself has no color of a side.
//!
//! A sign is a shape given by its distance: below zero inside. Points are in
//! screen space, like the terrains'; a sign keeps within 0.45 of the middle,
//! which is inside the game's disc.

use std::f32::consts::PI;

use crate::overlay::Ink;
use crate::shapes::{Rgb, length, rectangle, rotate, segment};

/// The signs there are, by the file they are written to: `<name>.png`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Icon {
    Sword,
    Spear,
    Shield,
    Helmet,
    Bow,
    Horseshoe,
    Stirrup,
    Banner,
    Longship,
    Cog,
    Feather,
    Wings,
}

/// The sign's body and its outline, which is also the color of the nothing
/// around it.
pub const BODY: Rgb = [0.96, 0.94, 0.86];
pub const OUTLINE: Rgb = [0.13, 0.11, 0.10];
/// How wide the outline is.
const LINE: f32 = 0.035;

impl Icon {
    pub const ALL: [Icon; 12] = [
        Icon::Sword,
        Icon::Spear,
        Icon::Shield,
        Icon::Helmet,
        Icon::Bow,
        Icon::Horseshoe,
        Icon::Stirrup,
        Icon::Banner,
        Icon::Longship,
        Icon::Cog,
        Icon::Feather,
        Icon::Wings,
    ];

    /// The file's name without `.png`, which is also what the example map
    /// calls the unit that carries the sign.
    pub fn name(self) -> &'static str {
        match self {
            Icon::Sword => "militia",
            Icon::Spear => "lightinfantry",
            Icon::Shield => "wardens",
            Icon::Helmet => "heavyinfantry",
            Icon::Bow => "archers",
            Icon::Horseshoe => "lightcavalry",
            Icon::Stirrup => "cavalry",
            Icon::Banner => "captain",
            Icon::Longship => "warship",
            Icon::Cog => "transport",
            Icon::Feather => "hawkriders",
            Icon::Wings => "wyverns",
        }
    }

    /// How far a point is from the sign's edge: below zero inside it.
    pub fn distance(self, x: f32, y: f32) -> f32 {
        match self {
            Icon::Sword => sword(x, y),
            Icon::Spear => spear(x, y),
            Icon::Shield => shield(x, y),
            Icon::Helmet => helmet(x, y),
            Icon::Bow => bow(x, y),
            Icon::Horseshoe => horseshoe(x, y),
            Icon::Stirrup => stirrup(x, y),
            Icon::Banner => banner(x, y),
            Icon::Longship => longship(x, y),
            Icon::Cog => cog(x, y),
            Icon::Feather => feather(x, y),
            Icon::Wings => wings(x, y),
        }
    }

    /// True where the sign has a dark mark inside its body.
    fn mark(self, x: f32, y: f32) -> bool {
        match self {
            // The shield's cross.
            Icon::Shield => (x.abs() < 0.028 || (y + 0.07).abs() < 0.028) && shield(x, y) < -0.07,
            // The horseshoe's nail holes.
            Icon::Horseshoe => (0..6).any(|i| {
                let angle = PI * (0.62 + 0.352 * i as f32);
                length(x - 0.27 * angle.cos(), y + 0.03 + 0.27 * angle.sin()) < 0.024
            }),
            // The helmet's eye slits, and the opening below its nose.
            Icon::Helmet => {
                let eyes = (y - 0.05).abs() < 0.03 && (0.05..0.24).contains(&x.abs());
                let mouth = x.abs() < 0.055 && (0.05..0.32).contains(&y);
                eyes || mouth
            }
            // The slot in the stirrup's strap.
            Icon::Stirrup => x.abs() < 0.04 && (y + 0.37).abs() < 0.022,
            // The banner's round device.
            Icon::Banner => length(x - 0.06, y + 0.15) < 0.055,
            // The shields along the longship's side.
            Icon::Longship => (-1..=1).any(|i| length(x - 0.17 * i as f32, y - 0.15) < 0.04),
            // A seam across the cog's sail.
            Icon::Cog => (y + 0.13).abs() < 0.02 && cog(x, y) < -0.04,
            // The feather's shaft.
            Icon::Feather => {
                let (u, v) = rotate(x, y, -PI / 4.0);
                u.abs() < 0.016 && v.abs() < 0.3
            }
            _ => false,
        }
    }

    /// The sign as a picture: its body, its outline around it, nothing else.
    pub fn ink(self, x: f32, y: f32) -> Ink {
        let d = self.distance(x, y);
        if d >= LINE {
            None
        } else if d > 0.0 || self.mark(x, y) {
            Some((OUTLINE, 1.0))
        } else {
            Some((BODY, 1.0))
        }
    }
}

/// A sword, point up and to the right.
fn sword(x: f32, y: f32) -> f32 {
    // Stood upright, point up (y grows downward), then leaned over.
    let (x, y) = rotate(x, y, -PI / 4.0);
    let blade = rectangle(x, y + 0.10, 0.045, 0.27);
    // The point: the blade's width narrowing to nothing above it.
    let (px, py) = (x.abs(), y + 0.37);
    let point = if (-0.09..=0.0).contains(&py) { px - 0.045 * (1.0 + py / 0.09) } else { f32::MAX };
    let guard = rectangle(x, y - 0.19, 0.15, 0.035);
    let grip = rectangle(x, y - 0.29, 0.03, 0.08);
    let pommel = length(x, y - 0.39) - 0.05;
    blade.min(point).min(guard).min(grip).min(pommel)
}

/// A spear, point up and to the right, as the sword leans: a long shaft, a
/// leaf of a head, a bar under it.
fn spear(x: f32, y: f32) -> f32 {
    let (u, v) = rotate(x, y, -PI / 4.0);
    let shaft = rectangle(u, v - 0.13, 0.04, 0.31);
    // The head: where two circles cross, 0.34 long and 0.22 wide.
    let head = (length(u - 0.0764, v + 0.30) - 0.1864).max(length(u + 0.0764, v + 0.30) - 0.1864);
    let bar = rectangle(u, v + 0.13, 0.09, 0.025);
    let butt = length(u, v - 0.44) - 0.05;
    shaft.min(head).min(bar).min(butt)
}

/// A helmet from the front: a dome with a knob on top, its sides down to the
/// chin; its eye slits and the opening below the nose are marks.
fn helmet(x: f32, y: f32) -> f32 {
    let dome = (length(x, y) - 0.3).max(y);
    let sides = rectangle(x, y - 0.14, 0.3, 0.16);
    let knob = length(x, y + 0.31) - 0.05;
    dome.min(sides).min(knob)
}

/// A stirrup: a round arch on a flat tread, hung from a strap.
fn stirrup(x: f32, y: f32) -> f32 {
    let arch = if y < 0.12 { (length(x, y + 0.02) - 0.25).abs() - 0.055 } else { f32::MAX };
    let tread = rectangle(x, y - 0.17, 0.29, 0.05);
    let strap = rectangle(x, y + 0.37, 0.08, 0.06);
    arch.min(tread).min(strap)
}

/// A shield: straight at the top, round at the bottom.
fn shield(x: f32, y: f32) -> f32 {
    // Inside a wide box, and inside a circle that rounds its lower corners.
    let side = rectangle(x, y, 0.27, 0.36);
    let round = length(x, y + 0.08) - 0.41;
    side.max(round)
}

/// A bow, strung, with an arrow on the string.
fn bow(x: f32, y: f32) -> f32 {
    // The stave: a third of a ring, bulging to the left.
    let (cx, radius) = (0.17, 0.38);
    let (dx, dy) = (x - cx, y);
    let angle = dy.atan2(-dx).abs(); // 0 at the stave's middle
    let stave = if angle < 1.05 {
        (length(dx, dy) - radius).abs() - 0.035
    } else {
        // Past its ends: the distance to the nearer end.
        let (ex, ey) = (cx - radius * 1.05f32.cos(), radius * 1.05f32.sin());
        length(x - ex, y.abs() - ey) - 0.035
    };
    let (ex, ey) = (cx - radius * 1.05f32.cos(), radius * 1.05f32.sin());
    let string = segment(x, y, ex, -ey, ex, ey) - 0.012;
    // The arrow, pointing left.
    let shaft = segment(x, y, -0.36, 0.0, 0.36, 0.0) - 0.018;
    let head = {
        let along = x + 0.44; // 0 at the tip
        if (0.0..=0.13).contains(&along) { y.abs() - along * 0.6 } else { f32::MAX }
    };
    stave.min(string).min(shaft).min(head)
}

/// A horseshoe, open at the bottom.
fn horseshoe(x: f32, y: f32) -> f32 {
    let (dx, dy) = (x, y + 0.03);
    let ring = (length(dx, dy) - 0.27).abs() - 0.085;
    // The opening: 50 degrees either side of straight down.
    let from_down = dx.atan2(dy).abs();
    let gap = 50f32.to_radians();
    if from_down > gap {
        ring
    } else {
        // In the opening: the distance to the nearer end of the ring.
        let (ex, ey) = (0.27 * gap.sin(), 0.27 * gap.cos());
        length(dx.abs() - ex, dy - ey) - 0.085
    }
}

/// A banner on a staff: the staff upright on the left with a knob on top,
/// the cloth flying to the right from its top, cut in two tails at the end.
fn banner(x: f32, y: f32) -> f32 {
    let staff = rectangle(x + 0.18, y - 0.03, 0.03, 0.35);
    let knob = length(x + 0.18, y + 0.35) - 0.045;
    let cloth = rectangle(x - 0.075, y + 0.15, 0.225, 0.15);
    // The cut: a notch in the cloth's free end, deepest at its middle.
    let notch = (x - (0.17 + 0.13 * (y + 0.15).abs() / 0.15)) / 1.3;
    staff.min(knob).min(cloth.max(notch))
}

/// The signed distance to a triangle (its corners clockwise on the screen,
/// where y grows downward), near enough for a sign: the farthest of its
/// sides' lines.
fn triangle(x: f32, y: f32, corners: [(f32, f32); 3]) -> f32 {
    (0..3)
        .map(|i| {
            let ((ax, ay), (bx, by)) = (corners[i], corners[(i + 1) % 3]);
            let (dx, dy) = (bx - ax, by - ay);
            // The outward normal of a clockwise side (y down): (dy, -dx).
            ((x - ax) * dy - (y - ay) * dx) / length(dx, dy)
        })
        .fold(f32::MIN, f32::max)
}

/// A longship: a long low hull, shields along its side, a mast with a
/// three-cornered sail.
fn longship(x: f32, y: f32) -> f32 {
    // The hull: from 0.08 to 0.26 down, narrowing toward the keel.
    let hull = ((y - 0.17).abs() - 0.09).max(x.abs() - (0.40 - (y - 0.08) * 0.8));
    let mast = rectangle(x + 0.04, y + 0.12, 0.025, 0.22);
    let sail = triangle(x, y, [(0.0, -0.32), (0.30, 0.02), (0.0, 0.02)]);
    hull.min(mast).min(sail)
}

/// A cog: a deep round hull, a mast, a square sail.
fn cog(x: f32, y: f32) -> f32 {
    let hull = ((length(x / 0.38, (y - 0.04) / 0.28) - 1.0) * 0.28).max(0.04 - y);
    let mast = rectangle(x, y + 0.16, 0.025, 0.22);
    let sail = rectangle(x, y + 0.16, 0.22, 0.15);
    hull.min(mast).min(sail)
}

/// A feather, leaning to the right, its quill at the bottom.
fn feather(x: f32, y: f32) -> f32 {
    let (u, v) = rotate(x, y, -PI / 4.0);
    // A lens: where two circles cross.
    let vane = (length(u - 0.38, v + 0.04) - 0.5).max(length(u + 0.38, v + 0.04) - 0.5);
    let quill = segment(u, v, 0.0, 0.2, 0.0, 0.42) - 0.022;
    vane.min(quill)
}

/// A pair of wings, spread up and out from a small body.
fn wings(x: f32, y: f32) -> f32 {
    let wing = |x: f32| {
        let (u, v) = rotate(x - 0.2, y + 0.06, -0.85);
        (length(u - 0.28, v) - 0.4).max(length(u + 0.28, v) - 0.4)
    };
    let body = length(x / 0.7, y - 0.08) - 0.1;
    wing(x).min(wing(-x)).min(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ASPECT;
    use crate::noise::unit;

    #[test]
    fn every_sign_has_a_body_an_outline_and_room_around_it() {
        for icon in Icon::ALL {
            let (mut body, mut outline, mut far) = (0, 0, 0.0f32);
            for i in 0..20000 {
                let (x, y) = (unit(i, 0, 300) * 2.0 - 1.0, (unit(i, 1, 300) * 2.0 - 1.0) * ASPECT);
                match icon.ink(x, y) {
                    Some((color, alpha)) => {
                        assert_eq!(alpha, 1.0);
                        if color == BODY {
                            body += 1
                        } else {
                            outline += 1
                        };
                        far = far.max(length(x, y));
                    }
                    None => assert!(icon.distance(x, y) >= LINE),
                }
            }
            // A good part of the disc, more body than line, and inside the disc.
            assert!((500..4500).contains(&body), "{icon:?}: {body} points of body");
            assert!(outline > 150 && outline < body * 3, "{icon:?}: {outline} of outline, {body} of body");
            assert!(far < 0.56, "{icon:?} reaches {far} from the middle");
        }
    }

    #[test]
    fn the_signs_are_what_they_are_called() {
        // The sword's blade runs up to the right, its pommel is down to the left.
        assert!(Icon::Sword.distance(0.15, -0.15) < 0.0 && Icon::Sword.distance(-0.27, 0.27) < 0.0);
        assert!(Icon::Sword.distance(-0.2, -0.2) > LINE && Icon::Sword.distance(0.2, 0.2) > LINE);
        // The shield is whole, wider at the top than at the bottom, with a cross.
        assert!(Icon::Shield.distance(0.0, 0.0) < 0.0);
        assert!(Icon::Shield.distance(0.24, -0.3) < 0.0 && Icon::Shield.distance(0.24, 0.3) > 0.0);
        assert_eq!(Icon::Shield.ink(0.0, 0.1), Some((OUTLINE, 1.0)));
        assert_eq!(Icon::Shield.ink(0.12, 0.1), Some((BODY, 1.0)));
        // The bow's stave is left of its string, and the arrow crosses both.
        assert!(Icon::Bow.distance(-0.21, 0.0) < 0.0 && Icon::Bow.distance(-0.02, 0.2) < 0.0);
        assert!(Icon::Bow.distance(-0.1, 0.2) > 0.0);
        assert!(Icon::Bow.distance(0.3, 0.0) < 0.0 && Icon::Bow.distance(-0.4, 0.0) < 0.0);
        // The horseshoe is a ring with its middle and its bottom open.
        assert!(Icon::Horseshoe.distance(0.0, -0.3) < 0.0 && Icon::Horseshoe.distance(0.27, -0.03) < 0.0);
        assert!(Icon::Horseshoe.distance(0.0, 0.0) > LINE && Icon::Horseshoe.distance(0.0, 0.25) > LINE);
        // The banner: the staff, the cloth to its right, the cut at its end.
        assert!(Icon::Banner.distance(-0.18, 0.3) < 0.0 && Icon::Banner.distance(0.0, -0.25) < 0.0);
        assert!(Icon::Banner.distance(0.1, 0.2) > LINE, "under the cloth");
        assert!(Icon::Banner.distance(0.27, -0.15) > 0.0 && Icon::Banner.distance(0.27, -0.28) < 0.0, "the tails");
        assert_eq!(Icon::Banner.ink(0.06, -0.15), Some((OUTLINE, 1.0)));
        // The longship: its hull, its sail right of the mast, shields on its side.
        assert!(Icon::Longship.distance(-0.3, 0.12) < 0.0 && Icon::Longship.distance(0.1, -0.1) < 0.0);
        assert!(Icon::Longship.distance(-0.2, -0.2) > LINE, "nothing left of the mast, up high");
        assert_eq!(Icon::Longship.ink(0.0, 0.15), Some((OUTLINE, 1.0)));
        // The cog: a round hull under a square sail.
        assert!(Icon::Cog.distance(0.0, 0.25) < 0.0 && Icon::Cog.distance(0.18, -0.25) < 0.0);
        assert!(Icon::Cog.distance(0.35, 0.28) > 0.0, "the hull is round below");
        // The feather leans to the right; the wings are a pair, apart above.
        assert!(Icon::Feather.distance(0.12, -0.12) < 0.0 && Icon::Feather.distance(-0.12, -0.12) > LINE);
        assert!(Icon::Wings.distance(0.3, -0.2) < 0.0 && Icon::Wings.distance(-0.3, -0.2) < 0.0);
        assert!(Icon::Wings.distance(0.0, -0.3) > LINE);
        // The spear leans as the sword does, its head up to the right; the
        // helmet is whole above its slits, open below its nose; the stirrup
        // is an arch, open in the middle, on a tread.
        assert!(Icon::Spear.distance(0.2, -0.2) < 0.0 && Icon::Spear.distance(-0.25, 0.25) < 0.0);
        assert!(Icon::Spear.distance(-0.2, -0.2) > LINE && Icon::Spear.distance(0.2, 0.2) > LINE);
        assert_eq!(Icon::Helmet.ink(0.0, -0.15), Some((BODY, 1.0)));
        assert_eq!(Icon::Helmet.ink(0.15, 0.05), Some((OUTLINE, 1.0)));
        assert_eq!(Icon::Helmet.ink(0.0, 0.2), Some((OUTLINE, 1.0)));
        assert_eq!(Icon::Helmet.ink(0.2, 0.2), Some((BODY, 1.0)));
        assert!(Icon::Stirrup.distance(0.0, -0.27) < 0.0 && Icon::Stirrup.distance(0.0, 0.17) < 0.0);
        assert!(Icon::Stirrup.distance(0.0, 0.0) > LINE, "open in the middle");
        // Twelve signs, twelve names.
        let mut names: Vec<&str> = Icon::ALL.iter().map(|i| i.name()).collect();
        names.dedup();
        assert_eq!(
            names,
            [
                "militia",
                "lightinfantry",
                "wardens",
                "heavyinfantry",
                "archers",
                "lightcavalry",
                "cavalry",
                "captain",
                "warship",
                "transport",
                "hawkriders",
                "wyverns"
            ]
        );
    }
}
