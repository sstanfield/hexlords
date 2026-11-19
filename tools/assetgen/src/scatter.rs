//! The small things strewn over a tile: tufts of grass, trees, hills, peaks,
//! reeds, waves, pebbles. They are placed once for a hex and repeated on the
//! hex grid, so that a tree cut by a tile's edge has its other half on the
//! neighbouring tile.

use crate::ASPECT;
use crate::noise::{fractal, hash, unit};

/// One thing on a tile: where its middle is (screen space), how big it is,
/// and a number of its own for whatever else differs.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Feature {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub key: u32,
}

/// How features of one kind are strewn.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Strew {
    /// How many to place on one hex of ground.
    pub count: usize,
    /// Their sizes go from `r_min` to `r_max`.
    pub r_min: f32,
    pub r_max: f32,
    /// No two are nearer than this.
    pub apart: f32,
    pub seed: u32,
    /// How far past the tile a feature can be and still reach into it.
    pub margin: f32,
}

/// Place features with a least distance between them, the same on every hex,
/// and give back every copy that can touch the tile around the origin, the
/// highest first, so that the lower ones are drawn over them. `allowed` says
/// where a feature may stand.
pub fn scatter(strew: Strew, allowed: &dyn Fn(f32, f32) -> bool) -> Vec<Feature> {
    let Strew { count, r_min, r_max, apart, seed, margin } = strew;
    // The copy of a feature in the hex `a` steps to the lower right and `c` down.
    let copy = |f: &Feature, a: i32, c: i32| (f.x + a as f32 * 1.5, f.y + a as f32 * ASPECT + c as f32 * 2.0 * ASPECT);
    let mut base: Vec<Feature> = Vec::new();
    let mut i = 0;
    while base.len() < count && i < count * 400 {
        let n = i as i32;
        let (u, v) = (unit(n, 0, seed), unit(n, 1, seed));
        let r = r_min + (r_max - r_min) * unit(n, 2, seed);
        let key = hash(n, 3, seed);
        i += 1;
        let (x, y) = (u * 1.5, u * ASPECT + v * 2.0 * ASPECT);
        if !allowed(x, y) {
            continue;
        }
        // Too near to one that stands, or to its copy next door?
        let crowded = base.iter().any(|b| {
            (-1..=1).any(|a| {
                (-1..=1).any(|c| {
                    let (bx, by) = copy(b, a, c);
                    let (dx, dy) = (x - bx, y - by);
                    dx * dx + dy * dy < apart * apart
                })
            })
        });
        if !crowded {
            base.push(Feature { x, y, r, key });
        }
    }
    let mut all = Vec::new();
    for b in &base {
        for a in -2..=2 {
            for c in -2..=2 {
                let (x, y) = copy(b, a, c);
                if x.abs() < 1.0 + margin && y.abs() < ASPECT + margin {
                    all.push(Feature { x, y, ..*b });
                }
            }
        }
    }
    all.sort_by(|p, q| (p.y, p.x).partial_cmp(&(q.y, q.x)).unwrap_or(std::cmp::Ordering::Equal));
    all
}

/// Where a swamp's pools are: above about 0.57 is water. Reeds keep out of it.
pub fn pool(x: f32, y: f32) -> f32 {
    fractal(x, y, 5, 3, 31)
}

/// Everything strewn over the tiles, placed once and used for every pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Features {
    pub tufts: Vec<Feature>,
    pub trees: Vec<Feature>,
    pub mounds: Vec<Feature>,
    pub peaks: Vec<Feature>,
    pub reeds: Vec<Feature>,
    pub waves: Vec<Feature>,
    pub pebbles: Vec<Feature>,
}

impl Features {
    /// How many trees, peaks and so on a tile has, and how big: change them here.
    pub fn new() -> Features {
        let strew = |count, r_min, r_max, apart, seed, margin| Strew { count, r_min, r_max, apart, seed, margin };
        let anywhere = |_: f32, _: f32| true;
        Features {
            tufts: scatter(strew(16, 0.045, 0.065, 0.2, 1, 0.15), &anywhere),
            trees: scatter(strew(17, 0.15, 0.20, 0.215, 2, 0.4), &anywhere),
            mounds: scatter(strew(5, 0.19, 0.24, 0.48, 3, 0.5), &anywhere),
            peaks: scatter(strew(4, 0.30, 0.36, 0.62, 4, 0.7), &anywhere),
            reeds: scatter(strew(13, 0.06, 0.08, 0.17, 5, 0.25), &|x, y| pool(x, y) < 0.545),
            waves: scatter(strew(11, 0.075, 0.095, 0.27, 6, 0.2), &anywhere),
            pebbles: scatter(strew(7, 0.018, 0.03, 0.3, 7, 0.1), &anywhere),
        }
    }
}

impl Default for Features {
    fn default() -> Features {
        Features::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREES: Strew = Strew { count: 17, r_min: 0.15, r_max: 0.20, apart: 0.215, seed: 2, margin: 0.4 };

    #[test]
    fn features_keep_their_distance_and_their_sizes() {
        let trees = scatter(TREES, &|_, _| true);
        assert!(trees.len() >= 17, "{}", trees.len());
        for (i, a) in trees.iter().enumerate() {
            assert!((0.15..=0.20).contains(&a.r));
            assert!(a.x.abs() < 1.4 && a.y.abs() < ASPECT + 0.4);
            for b in &trees[i + 1..] {
                let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                assert!(d >= 0.215 - 1e-4, "{a:?} and {b:?} are {d} apart");
            }
        }
        // The highest first.
        assert!(trees.windows(2).all(|w| w[0].y <= w[1].y));
        // The same every time.
        assert_eq!(trees, scatter(TREES, &|_, _| true));
    }

    #[test]
    fn a_feature_near_an_edge_has_its_copy_across_it() {
        let trees = scatter(TREES, &|_, _| true);
        let steps =
            [(1.5, ASPECT), (1.5, -ASPECT), (0.0, 2.0 * ASPECT), (-1.5, ASPECT), (-1.5, -ASPECT), (0.0, -2.0 * ASPECT)];
        let mut pairs = 0;
        for t in &trees {
            for (dx, dy) in steps {
                let (x, y) = (t.x + dx, t.y + dy);
                // If the copy can reach into the tile, it is in the list.
                if x.abs() < 1.0 + 0.4 - 1e-3 && y.abs() < ASPECT + 0.4 - 1e-3 {
                    let found = trees
                        .iter()
                        .any(|o| (o.x - x).abs() < 1e-4 && (o.y - y).abs() < 1e-4 && o.r == t.r && o.key == t.key);
                    assert!(found, "{t:?} has no copy at ({x}, {y})");
                    pairs += 1;
                }
            }
        }
        assert!(pairs > 10, "{pairs}");
    }

    #[test]
    fn features_stand_only_where_they_may() {
        let reeds = Features::new().reeds;
        assert!(!reeds.is_empty());
        // The pools repeat from hex to hex, so the copies are on land too.
        assert!(reeds.iter().all(|f| pool(f.x, f.y) < 0.55), "a reed stands in a pool");
        // Nowhere to stand: nothing, and it gives up.
        assert!(scatter(TREES, &|_, _| false).is_empty());
    }
}
