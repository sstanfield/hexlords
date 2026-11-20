//! What `mapgen check` says of a map beyond what the game's reader refuses
//! (`state::scenario`: the sizes, the letters, a record for every city hex,
//! known units and sides, garrisons that fit, names used once): every bridge
//! and every pass is on a road line that starts and ends on a road on land;
//! and each city's reach class, with a warning for a
//! city only a ship or a flyer gets to when the map has no such unit, or no
//! way to leave it.

use crate::state::{Hex, Map, TileType};

use super::grid::{Grid, pieces};
use super::reach::{MoveClass, ReachClass, by_water, city_reach, movement_class};

/// What is wrong with a map (a generated map must have none), what may be,
/// and each city's reach class.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
    pub reach: Vec<(String, ReachClass)>,
}

impl Check {
    pub fn report(&self) -> String {
        let mut out = String::new();
        for p in &self.problems {
            out.push_str(&format!("problem: {p}\n"));
        }
        for w in &self.warnings {
            out.push_str(&format!("warning: {w}\n"));
        }
        let count = |r: ReachClass| self.reach.iter().filter(|(_, x)| *x == r).count();
        out.push_str(&format!(
            "reach: {} on foot, {} by sea, {} by air\n",
            count(ReachClass::Foot),
            count(ReachClass::Sea),
            count(ReachClass::Air)
        ));
        for (name, r) in self.reach.iter().filter(|(_, r)| *r != ReachClass::Foot) {
            out.push_str(&format!("  {name}: {}\n", r.word()));
        }
        out
    }
}

/// Check a map that the reader has read. A crossing whose road does not run
/// on at both ends is a warning, for a map drawn by hand (it may be meant);
/// `strict` (the generator's own maps) makes it a problem.
pub fn check(map: &Map, strict: bool) -> Check {
    let mut c = Check::default();
    // Bridges and passes: on a road line with land road at both ends.
    let (rows, cols) = (map.rows(), map.cols());
    let tile = |h: Hex| map.tile(h);
    let crossing = |h: Hex| tile(h).is_some_and(|t| t.road && matches!(t.kind, TileType::Water | TileType::Mountain));
    let land_road = |h: Hex| {
        tile(h).is_some_and(|t| {
            (t.road || t.kind == TileType::City) && !matches!(t.kind, TileType::Water | TileType::Mountain)
        })
    };
    let (piece, sizes) = pieces(rows, cols, crossing);
    let all = Grid::new(rows, cols, ());
    for (id, _) in sizes.iter().enumerate() {
        let run: Vec<Hex> = all.hexes().filter(|h| *piece.get(*h) == Some(id)).collect();
        let mut ends: Vec<Hex> = run.iter().flat_map(|h| map.neighbours(*h)).filter(|h| land_road(*h)).collect();
        ends.sort_unstable_by_key(|h| (h.row, h.col));
        ends.dedup();
        if ends.len() < 2 {
            let what = if tile(run[0]).is_some_and(|t| t.kind == TileType::Water) { "bridge" } else { "pass" };
            let says = format!("the {what} at {} does not run from a road on land to another", run[0]);
            if strict { c.problems.push(says) } else { c.warnings.push(says) }
        }
    }
    // Reach.
    let classes = city_reach(map);
    let units = map.units();
    let class_of = |name: &str| units.kind_named(name).and_then(|k| units.kind(k)).map(movement_class);
    let has = |class: MoveClass| units.kinds().iter().any(|k| movement_class(k) == class);
    for (city, r) in map.cities().iter().zip(&classes) {
        c.reach.push((city.name.clone(), *r));
        let builds = |class: MoveClass| city.units.iter().any(|p| class_of(&p.basename) == Some(class));
        match r {
            ReachClass::Foot => {}
            ReachClass::Sea => {
                if !has(MoveClass::Naval) && !has(MoveClass::Flyer) {
                    c.warnings.push(format!("{} is reached only by sea, and the map has no ship", city.name));
                } else if !builds(MoveClass::Naval) && !by_water(map, city.pos) {
                    c.warnings.push(format!("{} is reached only by sea, and builds no ship to leave by", city.name));
                }
            }
            ReachClass::Air => {
                if !has(MoveClass::Flyer) {
                    c.warnings.push(format!("{} is reached only through the air, and the map has no flyer", city.name));
                } else if !builds(MoveClass::Flyer) {
                    c.warnings.push(format!("{} is reached only through the air, and builds no flyer", city.name));
                }
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::scenario;

    #[test]
    fn crossings_and_reach_are_checked() {
        // A bridge on row 1 that ends in the water at its east end; a city
        // past the river with no bridge to it.
        let drawing = ["gggwwwgg", "ggGWWwgg", "cggwwwgc", "gggwwwgg", "gggwwwgg", "gggwwwgg"];
        let terrain = crate::mapgen::padded(&drawing, 'm');
        let units = "unit a\nmoves 10\nenter grass 2, city 1, road 1, bridge 1\n";
        let cities = "city 2 0\nname Home\nside red\ncity 2 7\nname Away\n";
        let (map, _) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let c = check(&map, true);
        assert_eq!(c.problems.len(), 1, "{:?}", c.problems);
        assert!(c.problems[0].contains("the bridge at (1, 3)"));
        assert_eq!(c.reach.iter().map(|(_, r)| *r).collect::<Vec<_>>(), [ReachClass::Foot, ReachClass::Air]);
        assert!(c.warnings[0].contains("the map has no flyer"), "{:?}", c.warnings);
        // Drawn by hand, the bridge to nowhere may be meant.
        let loose = check(&map, false);
        assert!(loose.problems.is_empty() && loose.warnings.iter().any(|w| w.contains("the bridge at (1, 3)")));
        assert!(c.report().contains("reach: 1 on foot, 0 by sea, 1 by air"));
    }
}
