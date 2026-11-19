//! The ten terrains, one function each: the color of a terrain at a point.
//!
//! A point is in screen space: x from -1 (the hex's left point) to 1, y from
//! -[`ASPECT`](crate::ASPECT) (its top edge) *down* to +ASPECT, one unit the
//! same length either way, so a circle here is a circle on the screen. The
//! functions answer for the whole square around the hex (and a little past
//! it), not only for the hex: the ground and what is strewn on it repeat on
//! the hex grid, so the corners of the square show the neighbouring hexes'
//! ground, and the seven terrains of open country run on into their
//! neighbours. City, temple and ruin are a picture in the middle of a grass
//! tile, so they join grass without a seam too.
//!
//! To change the art, change the colors and shapes here, and how many trees,
//! peaks and so on there are in `Features::new`.

use std::f32::consts::PI;

use crate::noise::{fractal, lattice, spread, unit};
use crate::scatter::{Features, pool};
use crate::shapes::{Rgb, ball, length, light, mix, octagon, rectangle, rotate, scale, segment};
use crate::sides;

/// A terrain of the game's maps.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Terrain {
    Grass,
    Forest,
    Hill,
    Swamp,
    Desert,
    Mountain,
    Water,
    City,
    Temple,
    Ruin,
}

impl Terrain {
    /// Every terrain, in the order of the letters' table in
    /// `docs/map-format.md`. An atlas has them in this order.
    pub const ALL: [Terrain; 10] = [
        Terrain::Grass,
        Terrain::Forest,
        Terrain::Hill,
        Terrain::Swamp,
        Terrain::Desert,
        Terrain::Mountain,
        Terrain::Water,
        Terrain::City,
        Terrain::Temple,
        Terrain::Ruin,
    ];

    /// The game's name for the terrain; its tile is `<name>_tile.png`.
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Grass => "grass",
            Terrain::Forest => "forest",
            Terrain::Hill => "hill",
            Terrain::Swamp => "swamp",
            Terrain::Desert => "desert",
            Terrain::Mountain => "mountain",
            Terrain::Water => "water",
            Terrain::City => "city",
            Terrain::Temple => "temple",
            Terrain::Ruin => "ruin",
        }
    }

    /// The terrain's letter in a map file.
    pub fn letter(self) -> char {
        match self {
            Terrain::Grass => 'g',
            Terrain::Forest => 'f',
            Terrain::Hill => 'h',
            Terrain::Swamp => 's',
            Terrain::Desert => 'd',
            Terrain::Mountain => 'm',
            Terrain::Water => 'w',
            Terrain::City => 'c',
            Terrain::Temple => 't',
            Terrain::Ruin => 'r',
        }
    }

    /// The terrain a map file's letter stands for.
    pub fn from_letter(letter: char) -> Option<Terrain> {
        Terrain::ALL.into_iter().find(|t| t.letter() == letter)
    }

    /// Its place in [`Terrain::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The terrain's color at a point in screen space.
    pub fn shade(self, x: f32, y: f32, features: &Features) -> Rgb {
        match self {
            Terrain::Grass => grass(x, y, features),
            Terrain::Forest => forest(x, y, features),
            Terrain::Hill => hill(x, y, features),
            Terrain::Swamp => swamp(x, y, features),
            Terrain::Desert => desert(x, y, features),
            Terrain::Mountain => mountain(x, y, features),
            Terrain::Water => water(x, y, features),
            Terrain::City => city(x, y, features, sides::NEUTRAL.color),
            Terrain::Temple => temple(x, y, features),
            Terrain::Ruin => ruin(x, y, features),
        }
    }
}

/// Light green, mottled, with tufts.
fn grass(x: f32, y: f32, features: &Features) -> Rgb {
    let n = spread(x, y, 3, 4, 11);
    let m = fractal(x, y, 11, 2, 12);
    let mut c = scale(mix([0.30, 0.50, 0.19], [0.43, 0.62, 0.25], n), 0.92 + 0.16 * m);
    for t in &features.tufts {
        if (x - t.x).abs() > 0.1 || (y - t.y).abs() > 0.12 {
            continue;
        }
        // Three blades from one root.
        let r = t.r;
        let d = segment(x, y, t.x, t.y, t.x - 0.55 * r, t.y - r)
            .min(segment(x, y, t.x, t.y, t.x, t.y - 1.25 * r))
            .min(segment(x, y, t.x, t.y, t.x + 0.55 * r, t.y - r));
        if d < 0.013 {
            c = scale(c, 0.74);
        }
    }
    c
}

/// Dark ground under round treetops, each with its shadow.
fn forest(x: f32, y: f32, features: &Features) -> Rgb {
    let n = fractal(x, y, 4, 3, 21);
    let mut c = scale([0.14, 0.27, 0.11], 0.85 + 0.3 * n);
    for t in &features.trees {
        let (dx, dy) = (x - t.x, y - t.y);
        if dx.abs() > t.r * 1.6 || dy.abs() > t.r * 1.6 {
            continue;
        }
        let (sx, sy) = (dx - 0.05, dy - 0.06);
        if sx * sx + sy * sy < t.r * t.r {
            c = scale(c, 0.6);
        }
        let d = length(dx, dy);
        let angle = dy.atan2(dx);
        // A wavy outline, different for every tree.
        let rim = t.r * (1.0 + 0.07 * (6.0 * angle + (t.key % 100) as f32).sin());
        if d < rim {
            let tone = unit(t.key as i32, 0, 5);
            let base = mix([0.17, 0.39, 0.14], [0.31, 0.53, 0.19], tone);
            let leaf = fractal(x, y, 28, 1, 22 + t.key % 7);
            let mut color = scale(base, 0.42 + 0.85 * light(dx / rim, dy / rim) + 0.25 * (leaf - 0.5));
            if d > rim - 0.02 {
                color = scale(base, 0.33);
            }
            c = color;
        }
    }
    c
}

/// Olive ground with low rounded hills, flat at the foot.
fn hill(x: f32, y: f32, features: &Features) -> Rgb {
    let n = spread(x, y, 4, 3, 61);
    let m = fractal(x, y, 12, 2, 62);
    let mut c = scale(mix([0.46, 0.51, 0.27], [0.57, 0.59, 0.33], n), 0.93 + 0.14 * m);
    for t in &features.mounds {
        let (rx, ry) = (t.r * 1.55, t.r * 0.95);
        let (dx, dy) = (x - t.x, y - t.y);
        if dx.abs() > rx + 0.12 || dy.abs() > ry + 0.12 {
            continue;
        }
        // The hill is an ellipse cut off a little below its middle.
        let foot = ry * 0.3;
        let (sx, sy) = ((dx - 0.08) / rx, (dy - 0.01) / ry);
        if sx * sx + sy * sy < 1.0 && dy < foot + 0.02 {
            c = scale(c, 0.72);
        }
        let (ex, ey) = (dx / rx, dy / ry);
        let e = ex * ex + ey * ey;
        if e < 1.0 && dy < foot {
            let base = mix([0.60, 0.60, 0.34], [0.66, 0.62, 0.36], unit(t.key as i32, 0, 9));
            let mut color = scale(base, 0.5 + 0.72 * light(ex, ey));
            if e > 0.88 || dy > foot - 0.02 {
                color = scale(base, 0.4);
            }
            c = color;
        }
    }
    c
}

/// Grey rock with peaks: a lit and a dark side, snow on top.
fn mountain(x: f32, y: f32, features: &Features) -> Rgb {
    let n = spread(x, y, 5, 3, 81);
    let m = fractal(x, y, 14, 2, 82);
    let mut c = scale(mix([0.36, 0.34, 0.31], [0.50, 0.47, 0.42], n), 0.9 + 0.2 * m);
    for p in &features.peaks {
        let (height, half_width) = (p.r * 1.7, p.r * 1.25);
        let (top, foot) = (p.y - height * 0.6, p.y + height * 0.4);
        let dx = x - p.x;
        if dx.abs() > half_width + 0.2 || y < top - 0.05 || y > foot + 0.05 {
            continue;
        }
        // 0 at the peak's top, 1 at its foot.
        let t = (y - top) / height;
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let jitter = (fractal(x, y, 18, 2, 83 + p.key % 5) - 0.5) * 0.18 * p.r;
        let half = half_width * t + jitter;
        // The shadow on the ground to the right of the peak.
        if dx > half && dx < half + 0.14 * t {
            c = scale(c, 0.7);
        }
        if dx.abs() < half {
            let ridge = p.x + half_width * 0.2 * t + (fractal(x, y, 9, 1, 84) - 0.5) * 0.06;
            let lit = x < ridge;
            let snow = t < 0.36 + (fractal(x, y, 20, 1, 85 + p.key % 3) - 0.5) * 0.3;
            let streaks = fractal(x, y, 22, 1, 86);
            let mut color = match (snow, lit) {
                (true, true) => [0.95, 0.96, 0.98],
                (true, false) => [0.66, 0.72, 0.84],
                (false, true) => scale([0.71, 0.66, 0.58], 0.88 + 0.24 * streaks),
                (false, false) => scale([0.40, 0.36, 0.34], 0.88 + 0.24 * streaks),
            };
            if half - dx.abs() < 0.024 {
                color = [0.17, 0.15, 0.14];
            }
            // The foot fades into the rock around it.
            if t > 0.82 {
                color = mix(color, c, (t - 0.82) / 0.18 * 0.7);
            }
            c = color;
        }
    }
    c
}

/// Dark olive mud with pools of dull water and reeds on the land between.
fn swamp(x: f32, y: f32, features: &Features) -> Rgb {
    let n = spread(x, y, 4, 3, 91);
    let m = fractal(x, y, 13, 2, 92);
    let mut c = scale(mix([0.35, 0.35, 0.19], [0.47, 0.45, 0.26], n), 0.92 + 0.16 * m);
    let p = pool(x, y);
    if p > 0.565 {
        c = if p < 0.585 {
            [0.23, 0.26, 0.16] // the pool's bank
        } else {
            let s = fractal(x, y, 9, 2, 93);
            mix([0.14, 0.22, 0.21], [0.22, 0.32, 0.30], s)
        };
    }
    for f in &features.reeds {
        let (dx, dy) = (x - f.x, y - f.y);
        if dx.abs() > 0.14 || dy.abs() > 0.22 {
            continue;
        }
        // Three stalks, the middle one with a brown head.
        let height = f.r * 2.1;
        let mut d = 1.0f32;
        for (lean, part) in [(-0.45f32, 0.72f32), (0.0, 1.0), (0.45, 0.8)] {
            d = d.min(segment(x, y, f.x + lean * f.r, f.y, f.x + lean * f.r * 1.8, f.y - height * part));
        }
        if d < 0.015 {
            c = [0.16, 0.24, 0.09];
        }
        let (ex, ey) = ((x - f.x) / 0.024, (y - (f.y - height * 0.86)) / 0.055);
        if ex * ex + ey * ey < 1.0 {
            c = [0.40, 0.25, 0.12];
        }
    }
    c
}

/// Sand in dunes, with a few pebbles.
fn desert(x: f32, y: f32, features: &Features) -> Rgb {
    let (u, v) = lattice(x, y);
    let w = fractal(x, y, 2, 2, 41);
    // The dunes: bands across the grid, bent by noise, each rising slowly to
    // a crest and falling off in a shadow.
    let f = (u + 3.0 * v + 1.6 * w).rem_euclid(1.0);
    let band = if f < 0.72 { 0.93 + 0.11 * (f / 0.72) } else { 0.86 };
    let grain = fractal(x, y, 40, 1, 42);
    let n = spread(x, y, 3, 3, 43);
    let mut c = scale(mix([0.80, 0.66, 0.41], [0.89, 0.78, 0.52], n), band + 0.08 * (grain - 0.5));
    for p in &features.pebbles {
        if let Some(color) = ball(x - p.x, y - p.y, p.r, [0.60, 0.46, 0.30], 0.0) {
            c = color;
        }
    }
    c
}

/// Blue, mottled, with small light arcs for waves.
fn water(x: f32, y: f32, features: &Features) -> Rgb {
    let n = spread(x, y, 3, 3, 51);
    let m = fractal(x, y, 10, 2, 52);
    let mut c = scale(mix([0.14, 0.31, 0.57], [0.22, 0.44, 0.71], n), 0.93 + 0.14 * m);
    for f in &features.waves {
        let (dx, dy) = (x - f.x, y - (f.y + f.r * 0.7));
        if dx.abs() > f.r * 1.2 || dy.abs() > f.r * 1.5 {
            continue;
        }
        if dy < 0.0 && dx.abs() < 0.85 * f.r && (length(dx, dy) - f.r).abs() < 0.018 {
            c = mix(c, [0.62, 0.80, 0.95], 0.85);
        }
    }
    c
}

/// How big the pictures of the city, the temple and the ruin are (1 is the
/// size their numbers are written for).
pub const ICON: f32 = 1.18;
const DARK: Rgb = [0.24, 0.22, 0.20];

/// A walled town on grass: an octagon of wall with a gate and four towers,
/// houses around a keep. The roofs of the houses, the towers and the keep are
/// in `roof`, the color of the side that holds the city (`sides`), and the
/// wall's stone is tinted with it, so that whose it is shows from afar.
pub fn city(x: f32, y: f32, features: &Features, roof: Rgb) -> Rgb {
    // The eaves and the ridges: the roof's color, much darker.
    let eaves = scale(roof, 0.35);
    // The wall's stone, and its battlements' shadows.
    let stone = mix([0.78, 0.75, 0.69], roof, 0.55);
    let shadow = mix([0.55, 0.52, 0.48], scale(roof, 0.7), 0.55);
    let mut c = grass(x, y, features);
    let (x, y) = (x / ICON, y / ICON);
    let (radius, wall) = (0.58f32, 0.11f32);
    if octagon(x - 0.05, y - 0.06, radius) < 0.0 {
        c = scale(c, 0.6);
    }
    let o = octagon(x, y, radius);
    if o < -wall {
        c = scale([0.58, 0.54, 0.47], 0.88 + 0.2 * fractal(x, y, 30, 1, 101));
    }
    // Houses with pitched red roofs: where, and turned how far.
    let houses = [
        (-0.24f32, -0.17f32, 0.3f32),
        (0.21, -0.22, -0.2),
        (-0.27, 0.17, -0.15),
        (0.24, 0.16, 0.25),
        (0.0, 0.31, 0.0),
        (-0.01, -0.35, 0.08),
    ];
    for (hx, hy, angle) in houses {
        let (sx, sy) = rotate(x - hx - 0.03, y - hy - 0.035, -angle);
        if rectangle(sx, sy, 0.095, 0.06) < 0.0 {
            c = scale(c, 0.65);
        }
        let (lx, ly) = rotate(x - hx, y - hy, -angle);
        let d = rectangle(lx, ly, 0.095, 0.06);
        if d < 0.0 {
            c = if d > -0.016 || ly.abs() < 0.008 {
                eaves
            } else if ly < 0.0 {
                scale(roof, 1.1)
            } else {
                scale(roof, 0.78)
            };
        }
    }
    // The keep, with a pyramid roof.
    if rectangle(x - 0.045, y - 0.055, 0.14, 0.14) < 0.0 {
        c = scale(c, 0.6);
    }
    let d = rectangle(x, y, 0.14, 0.14);
    if d < 0.0 {
        let face = if y.abs() > x.abs() {
            if y < 0.0 { 1.15 } else { 0.62 }
        } else if x < 0.0 {
            0.95
        } else {
            0.75
        };
        c = if d > -0.018 || (x.abs() - y.abs()).abs() < 0.007 { eaves } else { scale(roof, face) };
    }
    // The wall, with battlements.
    if o < 0.0 && o > -wall {
        let angle = y.atan2(x);
        let block = (((angle + PI) / (2.0 * PI) * 40.0).floor() as i32) % 2;
        let mut color = scale(stone, 0.95 + 0.1 * fractal(x, y, 24, 1, 102));
        if o > -0.042 && block == 0 {
            color = shadow;
        }
        if o > -0.016 || o < -wall + 0.016 {
            color = DARK;
        }
        c = color;
    }
    // The gate, at the bottom.
    let gate = rectangle(x, y - (radius - wall * 0.5), 0.075, wall * 0.62);
    if gate < 0.0 {
        c = if gate > -0.014 { DARK } else { [0.22, 0.15, 0.10] };
    }
    // Corner towers with pointed roofs.
    let p = (radius - wall * 0.5) / 2f32.sqrt();
    for (side_x, side_y) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (tx, ty) = (x - side_x * p, y - side_y * p);
        if length(tx - 0.035, ty - 0.045) < 0.125 {
            c = scale(c, 0.65);
        }
        let d = length(tx, ty);
        if d < 0.125 {
            c = if d > 0.125 - 0.018 {
                DARK
            } else if d > 0.095 {
                stone
            } else if d > 0.095 - 0.014 {
                eaves
            } else {
                let bright = (0.55 - 0.9 * (tx * 0.6 + ty * 0.8) / 0.095).clamp(0.0, 1.0);
                scale(roof, 0.6 + 0.6 * bright)
            };
        }
    }
    c
}

/// A round white terrace on grass with a ring of pillars and a golden dome.
fn temple(x: f32, y: f32, features: &Features) -> Rgb {
    let mut c = grass(x, y, features);
    let (x, y) = (x / ICON, y / ICON);
    let radius = 0.56;
    if length(x - 0.05, y - 0.06) < radius {
        c = scale(c, 0.6);
    }
    let d = length(x, y);
    if d < radius {
        let mut color = scale([0.87, 0.85, 0.79], 0.95 + 0.1 * fractal(x, y, 24, 1, 121));
        if d > radius - 0.02 {
            color = [0.35, 0.33, 0.30];
        } else if d > radius - 0.085 && d < radius - 0.07 {
            color = scale(color, 0.72); // the step's edge
        } else if d > radius - 0.07 {
            color = scale(color, 0.88);
        }
        c = color;
    }
    for i in 0..8 {
        let angle = i as f32 * PI / 4.0 + PI / 8.0;
        let (cx, cy) = (angle.cos() * 0.40, angle.sin() * 0.40);
        if length(x - cx - 0.025, y - cy - 0.03) < 0.06 {
            c = scale(c, 0.7);
        }
        if let Some(color) = ball(x - cx, y - cy, 0.06, [1.0, 0.99, 0.95], 0.014) {
            c = color;
        }
    }
    if length(x - 0.05, y - 0.06) < 0.27 {
        c = scale(c, 0.65);
    }
    let d = length(x, y);
    if d < 0.27 {
        c = if d > 0.27 - 0.02 {
            [0.42, 0.28, 0.08]
        } else {
            let bright = light(x / 0.27, y / 0.27);
            let shine = bright.powi(24) * 0.5;
            let base = scale([0.90, 0.70, 0.24], 0.45 + 0.75 * bright);
            [base[0] + shine, base[1] + shine, base[2] + shine]
        };
    }
    c
}

/// Broken walls on a patch of bare ground in the grass, a fallen column, the
/// stump of another, rubble.
fn ruin(x: f32, y: f32, features: &Features) -> Rgb {
    let mut c = grass(x, y, features);
    let (x, y) = (x / ICON, y / ICON);
    let n = fractal(x, y, 5, 2, 111);
    if length(x, y) < 0.5 + 0.3 * (n - 0.5) * 2.0 {
        c = scale([0.50, 0.44, 0.34], 0.88 + 0.24 * fractal(x, y, 16, 2, 112));
        if fractal(x, y, 7, 2, 113) > 0.58 {
            c = mix(c, [0.36, 0.46, 0.22], 0.5);
        }
    }
    let stone = |x: f32, y: f32| -> Rgb { scale([0.64, 0.62, 0.57], 0.9 + 0.2 * fractal(x, y, 20, 1, 115)) };
    // What is left of a square foundation, a little askew.
    let wall = |x: f32, y: f32| -> f32 {
        let (lx, ly) = rotate(x, y, -0.18);
        rectangle(lx, ly, 0.32, 0.32).abs() - 0.042
    };
    let standing = |x: f32, y: f32| -> bool {
        let (lx, ly) = rotate(x, y, -0.18);
        let angle = ly.atan2(lx);
        // Gaps at fixed angles, so that it looks broken and not noisy.
        let gaps = [-2.6f32, -0.9, 0.5, 1.9];
        gaps.iter().all(|gap| ((angle - gap + PI).rem_euclid(2.0 * PI) - PI).abs() > 0.26)
    };
    if wall(x - 0.035, y - 0.045) < 0.0 && standing(x - 0.035, y - 0.045) {
        c = scale(c, 0.62);
    }
    let d = wall(x, y);
    if d < 0.0 && standing(x, y) {
        c = if d > -0.015 { DARK } else { stone(x, y) };
        if d < -0.015 && fractal(x, y, 9, 2, 116) > 0.62 {
            c = mix(c, [0.34, 0.48, 0.20], 0.55); // moss
        }
    }
    // The fallen column.
    let (ax, ay, bx, by) = (-0.25f32, 0.12f32, 0.08f32, 0.25f32);
    if segment(x - 0.03, y - 0.04, ax, ay, bx, by) < 0.055 {
        c = scale(c, 0.65);
    }
    let d = segment(x, y, ax, ay, bx, by);
    if d < 0.055 {
        let (vx, vy) = (bx - ax, by - ay);
        let l = length(vx, vy);
        let along = ((x - ax) * vx + (y - ay) * vy) / (l * l);
        let side = ((x - ax) * -vy + (y - ay) * vx) / l; // below zero is the upper side
        let mut color = scale([0.82, 0.80, 0.74], 1.0 - 0.35 * ((side + 0.055) / 0.11).clamp(0.0, 1.0));
        if d > 0.055 - 0.014 || (along * 6.0).fract() < 0.08 {
            color = DARK; // its outline, and the joints of its drums
        }
        c = color;
    }
    // The stump.
    if let Some(color) = ball(x - 0.14, y + 0.05, 0.075, [0.85, 0.83, 0.77], 0.015) {
        c = color;
    }
    // Rubble.
    for i in 0..16 {
        let angle = unit(i, 0, 120) * 2.0 * PI;
        let away = 0.12 + 0.34 * unit(i, 1, 120);
        let r = 0.022 + 0.022 * unit(i, 2, 120);
        if let Some(color) = ball(x - angle.cos() * away, y - angle.sin() * away * 0.9, r, [0.66, 0.63, 0.58], 0.0) {
            c = color;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ASPECT;

    /// Points all over a hex's square, in screen space.
    fn points(n: i32) -> impl Iterator<Item = (f32, f32)> {
        (0..n).map(|i| (unit(i, 0, 900) * 2.0 - 1.0, (unit(i, 1, 900) * 2.0 - 1.0) * ASPECT))
    }

    fn apart(a: Rgb, b: Rgb) -> f32 {
        (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn names_and_letters() {
        let names: Vec<&str> = Terrain::ALL.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            ["grass", "forest", "hill", "swamp", "desert", "mountain", "water", "city", "temple", "ruin"]
        );
        let letters: String = Terrain::ALL.iter().map(|t| t.letter()).collect();
        assert_eq!(letters, "gfhsdmwctr");
        for (i, t) in Terrain::ALL.into_iter().enumerate() {
            assert_eq!(t.index(), i);
            assert_eq!(Terrain::from_letter(t.letter()), Some(t));
        }
        assert_eq!(Terrain::from_letter('x'), None);
    }

    #[test]
    fn every_color_is_a_color() {
        let features = Features::new();
        for terrain in Terrain::ALL {
            for (x, y) in points(400) {
                let c = terrain.shade(x, y, &features);
                // The temple's dome shines a little past white; the tile clamps it.
                assert!(c.iter().all(|v| (0.0..1.3).contains(v)), "{terrain:?} at ({x}, {y}): {c:?}");
            }
        }
    }

    #[test]
    fn open_country_repeats_from_hex_to_hex() {
        // The color at a point is the color one hex further: no seam between
        // two tiles of a terrain. A tile knows its own square, so the pairs
        // of points are taken inside it: a corner of the square (which is the
        // neighbouring hex's ground) and the same place a hex step away,
        // inside the hex; and a strip along the top edge with the strip along
        // the bottom. (A point on the very edge of a tree or a wave can fall
        // either way after the step; those are a handful in thousands.)
        let features = Features::new();
        let open = [
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Hill,
            Terrain::Swamp,
            Terrain::Desert,
            Terrain::Mountain,
            Terrain::Water,
        ];
        for terrain in open {
            let (mut off, mut all) = (0, 0);
            let mut pair = |x: f32, y: f32, dx: f32, dy: f32| {
                all += 1;
                if apart(terrain.shade(x, y, &features), terrain.shade(x + dx, y + dy, &features)) > 0.03 {
                    off += 1;
                }
            };
            for i in 0..1500 {
                let (a, b) = (unit(i, 0, 901), unit(i, 1, 901));
                // The upper left corner and the lower right of the hex; the
                // lower left corner and the upper right.
                pair(-1.0 + 0.5 * a, -ASPECT * b, 1.5, ASPECT);
                pair(-1.0 + 0.5 * a, ASPECT * b, 1.5, -ASPECT);
                // Along the top edge, and the same along the bottom.
                pair(-0.5 + a, -ASPECT + 0.08 * (b - 0.5), 0.0, 2.0 * ASPECT);
            }
            assert!(off * 100 <= all, "{terrain:?}: {off} of {all} points differ a hex away");
        }
    }

    #[test]
    fn the_places_are_grass_at_their_edges() {
        // City, temple and ruin are a picture in the middle of grass: along
        // the hex's rim they are grass, so they join it.
        let features = Features::new();
        for terrain in [Terrain::City, Terrain::Temple, Terrain::Ruin] {
            let mut middle_differs = false;
            for i in 0..360 {
                let angle = i as f32 * PI / 180.0;
                // The rim of the hex in screen space, a little inside it.
                let (mx, my) = (angle.cos(), angle.sin());
                let rim = 0.97 / my.abs().max(mx.abs() + my.abs() * 0.5);
                let (x, y) = (mx * rim, my * rim * ASPECT);
                assert_eq!(terrain.shade(x, y, &features), grass(x, y, &features), "{terrain:?} at {i} degrees");
                let (x, y) = (mx * 0.3, my * 0.3 * ASPECT);
                middle_differs |= terrain.shade(x, y, &features) != grass(x, y, &features);
            }
            assert!(middle_differs, "{terrain:?} has no picture");
        }
    }

    #[test]
    fn the_terrains_can_be_told_apart_by_their_color() {
        // From far away a tile is one color. Those of open country differ.
        let features = Features::new();
        let mean = |terrain: Terrain| -> Rgb {
            let mut sum = [0.0f32; 3];
            for (x, y) in points(2000) {
                let c = terrain.shade(x, y, &features);
                (0..3).for_each(|k| sum[k] += c[k]);
            }
            scale(sum, 1.0 / 2000.0)
        };
        let open = [
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Hill,
            Terrain::Swamp,
            Terrain::Desert,
            Terrain::Mountain,
            Terrain::Water,
        ];
        let means: Vec<Rgb> = open.iter().map(|t| mean(*t)).collect();
        for i in 0..open.len() {
            for j in i + 1..open.len() {
                assert!(
                    apart(means[i], means[j]) > 0.06,
                    "{:?} and {:?}: {:?} {:?}",
                    open[i],
                    open[j],
                    means[i],
                    means[j]
                );
            }
        }
        // Water is the blue one, grass the green one, desert the bright one.
        let (grass, water, desert) = (means[0], means[6], means[4]);
        assert!(water[2] > water[0] && water[2] > water[1]);
        assert!(grass[1] > grass[0] && grass[1] > grass[2]);
        assert!(desert.iter().sum::<f32>() > grass.iter().sum::<f32>());
    }
}
