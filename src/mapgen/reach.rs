//! Who can get where: each kind of unit's movement class (on foot, by sea,
//! through the air), the hexes a class can reach from some start, and each
//! city's reach class: whether a unit on foot from the capitals' land gets to
//! it, or only a ship, or only a flyer.

use crate::state::{City, Hex, Map, MoveCost, TileType, Travel, UnitKind, step_cost};

use super::grid::{Grid, steps_from};

/// How a kind of unit goes, as the generator and its measures count it.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum MoveClass {
    Foot,
    Naval,
    Flyer,
}

/// A kind's movement class: what its `travels` says; for a kind that says
/// nothing (`land`, as in maps written before the key), what its prices say:
/// one that enters water and no land is naval, one that enters water,
/// mountains and land flies.
pub fn movement_class(kind: &UnitKind) -> MoveClass {
    match kind.travel {
        Travel::Sea => MoveClass::Naval,
        Travel::Air => MoveClass::Flyer,
        Travel::Land => {
            let c = &kind.cost;
            let land = [c.grass, c.forest, c.hill, c.swamp, c.desert].iter().any(|p| *p > 0);
            match (c.water > 0, c.mountain > 0, land) {
                (true, _, false) => MoveClass::Naval,
                (true, true, true) => MoveClass::Flyer,
                _ => MoveClass::Foot,
            }
        }
    }
}

/// The unit on foot that reach and travel costs are measured with: 10 moves;
/// grass 2, forest 4, hill 5, swamp 5, desert 3, a road (and a bridge, a
/// pass, along it) 1, a city 1. Mountains and water are walls.
pub fn reference_foot() -> MoveCost {
    MoveCost { grass: 2, forest: 4, hill: 5, swamp: 5, desert: 3, road: 1, bridge: 1, city: 1, ..MoveCost::default() }
}

/// A turn of the reference unit, in points.
pub const TURN: u32 = 10;

/// True if a city touches water: a ship can come to it.
pub fn by_water(map: &Map, hex: Hex) -> bool {
    map.neighbours(hex).any(|h| map.tile(h).is_some_and(|t| t.kind == TileType::Water))
}

/// The hexes a unit of `class` gets to from `starts`, over as many turns as
/// it takes: on foot, wherever the reference unit can step (mountains and
/// water are walls, but a pass and a bridge along their road); by sea, over
/// water (bridges too) and into the cities by it; through the air, anywhere.
pub fn reach(map: &Map, class: MoveClass, starts: &[Hex]) -> Grid<bool> {
    let (rows, cols) = (map.rows(), map.cols());
    let foot = reference_foot();
    let water = |h: Hex| map.tile(h).is_some_and(|t| t.kind == TileType::Water);
    let dist = match class {
        MoveClass::Flyer => steps_from(rows, cols, starts.iter().copied(), |_| true),
        MoveClass::Naval => steps_from(rows, cols, starts.iter().copied(), |h| {
            water(h) || (map.city_at(h).is_some() && by_water(map, h))
        }),
        MoveClass::Foot => {
            // A walk over the hexes a step can be made into, from where it is.
            let mut seen: Grid<bool> = Grid::new(rows, cols, false);
            let mut stack: Vec<Hex> = starts.iter().copied().filter(|h| map.contains(*h)).collect();
            for s in &stack {
                seen.set(*s, true);
            }
            while let Some(hex) = stack.pop() {
                for next in map.neighbours(hex) {
                    if !*seen.get(next) && step_cost(map, &foot, hex, next).is_some() {
                        seen.set(next, true);
                        stack.push(next);
                    }
                }
            }
            return seen;
        }
    };
    Grid::from_fn(rows, cols, |h| dist.get(h).is_some())
}

/// How a city can be got to.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ReachClass {
    /// On foot from the capitals' land.
    Foot,
    /// Only by ship, from a port on that land.
    Sea,
    /// Only through the air.
    Air,
}

impl ReachClass {
    pub fn word(self) -> &'static str {
        match self {
            ReachClass::Foot => "foot",
            ReachClass::Sea => "sea",
            ReachClass::Air => "air",
        }
    }
}

/// The capitals: the cities a side holds at the start (if none does, every
/// city: a map of nobody's).
pub fn capitals(map: &Map) -> Vec<&City> {
    let held: Vec<&City> = map.cities().iter().filter(|c| c.side != crate::state::Side::Grey).collect();
    if held.is_empty() { map.cities().iter().collect() } else { held }
}

/// Every city's reach class, in the cities' order: on foot if the reference
/// unit gets there from a capital; else by sea if units a ship carries from
/// a port on that land (a city by the water) can land on a coast it comes to
/// and walk there; else through the air.
pub fn city_reach(map: &Map) -> Vec<ReachClass> {
    let (foot, sea) = foot_and_sea(map);
    map.cities()
        .iter()
        .map(|c| {
            if *foot.get(c.pos) {
                ReachClass::Foot
            } else if *sea.get(c.pos) {
                ReachClass::Sea
            } else {
                ReachClass::Air
            }
        })
        .collect()
}

/// What the reference unit gets to on foot from the capitals; and what it
/// gets to landed from a ship that sails from the ports on that land.
pub fn foot_and_sea(map: &Map) -> (Grid<bool>, Grid<bool>) {
    let starts: Vec<Hex> = capitals(map).iter().map(|c| c.pos).collect();
    let foot = reach(map, MoveClass::Foot, &starts);
    let ports: Vec<Hex> =
        map.cities().iter().filter(|c| *foot.get(c.pos) && by_water(map, c.pos)).map(|c| c.pos).collect();
    let sailed = reach(map, MoveClass::Naval, &ports);
    let water = |h: Hex| map.tile(h).is_some_and(|t| t.kind == TileType::Water);
    let shore: Vec<Hex> = sailed
        .hexes()
        .filter(|h| *sailed.get(*h) && water(*h))
        .flat_map(|h| map.neighbours(h))
        .filter(|h| !water(*h) && map.tile(*h).is_some_and(|t| t.kind != TileType::Mountain))
        .collect();
    let mut sea = reach(map, MoveClass::Foot, &shore);
    for h in sea.hexes() {
        if *sailed.get(h) {
            sea.set(h, true);
        }
    }
    (foot, sea)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::scenario;

    const KINDS: &str = "\
unit foot
  moves 10
  enter grass 2, city 1, road 1, bridge 1
unit galley
  moves 12
  enter water 1, city 1, bridge 2
unit cog
  moves 12
  travels sea
  carries 4
  enter water 2, city 2
unit griffin
  moves 16
  enter grass 2, hill 2, mountain 3, water 2
unit balloon
  moves 8
  travels air
  enter grass 2
";

    fn map(terrain: &str, cities: &str) -> Map {
        scenario::parse(terrain, Some(KINDS), Some(cities)).unwrap().0
    }

    #[test]
    fn a_kinds_class_is_what_it_says_or_what_its_prices_say() {
        let kinds = scenario::parse_units(KINDS).unwrap();
        let classes: Vec<MoveClass> = kinds.iter().map(movement_class).collect();
        use MoveClass::*;
        assert_eq!(classes, [Foot, Naval, Naval, Flyer, Flyer]);
    }

    /// 10 x 12: grass west of column 6, a river of water at 6 and 7 (a bridge
    /// on row 2 if `bridge`), a range of mountains at column 9, a city on
    /// each side of the river and one beyond the range; an island at the top
    /// right in the water with a city.
    fn terrain(bridge: bool) -> String {
        let mut rows: Vec<String> = Vec::new();
        for r in 0..10 {
            let mut row: Vec<char> = "ggggggwwgmgg".chars().collect();
            if bridge && r == 2 {
                row[5] = 'G';
                row[6] = 'W';
                row[7] = 'W';
                row[8] = 'G';
            }
            rows.push(row.into_iter().collect::<String>());
        }
        rows[5].replace_range(2..3, "c");
        rows[5].replace_range(8..9, "c");
        rows[5].replace_range(11..12, "c");
        // Mountains around it, to the reader's smallest map.
        let drawing: Vec<&str> = rows.iter().map(String::as_str).collect();
        crate::mapgen::padded(&drawing, 'm')
    }

    const CITIES: &str = "city 5 2\nname Home\nside red\ncity 5 8\nname Shore\ncity 5 11\nname Beyond\n";

    #[test]
    fn on_foot_the_river_is_a_wall_unless_a_bridge_crosses_it() {
        let open = map(&terrain(false), CITIES);
        let starts = [Hex::new(5, 2)];
        let foot = reach(&open, MoveClass::Foot, &starts);
        assert!(*foot.get(Hex::new(0, 0)) && !*foot.get(Hex::new(5, 8)) && !*foot.get(Hex::new(5, 6)));
        let bridged = map(&terrain(true), CITIES);
        let foot = reach(&bridged, MoveClass::Foot, &starts);
        assert!(*foot.get(Hex::new(2, 7)) && *foot.get(Hex::new(5, 8)), "over the bridge");
        assert!(!*foot.get(Hex::new(5, 11)), "the range is a wall");
        // A ship goes on the water and into the cities by it; a flyer anywhere.
        let ship = reach(&open, MoveClass::Naval, &[Hex::new(0, 6)]);
        assert!(*ship.get(Hex::new(9, 7)) && *ship.get(Hex::new(5, 8)) && !*ship.get(Hex::new(5, 2)));
        assert!(reach(&open, MoveClass::Flyer, &starts).cells().iter().all(|b| *b));
    }

    #[test]
    fn a_city_is_reached_on_foot_by_sea_or_through_the_air() {
        // Home's land has no port: the city over the river is the air's.
        let open = map(&terrain(false), CITIES);
        use ReachClass::*;
        assert_eq!(city_reach(&open), [Foot, Air, Air]);
        // With a port on Home's side of the river, Shore is the sea's.
        let mut t = terrain(false);
        t = t.replacen("ggggggwwgmgg", "gggggcwwgmgg", 1);
        let cities = format!("{CITIES}city 0 5\nname Port\n");
        assert_eq!(city_reach(&map(&t, &cities)), [Foot, Sea, Air, Foot]);
        // Over a bridge, on foot.
        assert_eq!(city_reach(&map(&terrain(true), CITIES)), [Foot, Foot, Air]);
    }
}
