//! The generator's cities (`docs/mapgen.md`, milestone G4): as many as
//! asked, a capital a side; apart and off the edge; shared out fairly; named
//! each its own; cities at sea with a ship to leave by, in the air with a
//! flyer; and what it writes the game reads, garrisons and all.

use hexgame::mapgen::config::Config;
use hexgame::mapgen::generate;
use hexgame::mapgen::reach::{MoveClass, ReachClass, movement_class};
use hexgame::state::{Side, TileType};

const UNITS: &str = include_str!("../assets/mapgen/units.txt");

fn config(seed: u64, size: &str) -> Config {
    let mut c = Config { seed, ..Config::default() };
    c.set("size", size).unwrap();
    c
}

#[test]
fn the_cities_are_as_many_as_asked_apart_and_fairly_shared() {
    for seed in 1..=20 {
        let cfg = config(seed, "medium");
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let (g, plan) = (&made.state, &made.plan);
        // As many as asked; a capital for each side, each side once.
        assert_eq!(g.cities.len(), plan.cities as usize, "seed {seed}");
        let mut sides: Vec<Side> = g.cities.iter().map(|c| c.side).filter(|s| *s != Side::Grey).collect();
        sides.sort_by_key(|s| s.index());
        assert_eq!(sides.len(), usize::from(cfg.sides), "seed {seed}");
        sides.dedup();
        assert_eq!(sides.len(), usize::from(cfg.sides), "seed {seed}");
        // Apart, and off the map's edge.
        let gap = cfg.min_gap(plan);
        for (i, a) in g.cities.iter().enumerate() {
            assert!(
                u32::from(g.terrain.edge_distance(a.hex)) >= cfg.edge_margin,
                "seed {seed}: {} at the edge",
                a.name
            );
            assert_eq!(*g.terrain.get(a.hex), TileType::City);
            for b in &g.cities[i + 1..] {
                assert!(a.hex.distance(b.hex) >= gap, "seed {seed}: {} and {} too near", a.name, b.name);
            }
        }
        // Fair, at the default level.
        for f in &g.fairness {
            assert!(
                f.ok(),
                "seed {seed} (made from {}): {} is {} (at most {})",
                made.config.seed,
                f.name,
                f.value,
                f.limit
            );
        }
        // Named each its own.
        let mut names: Vec<&str> = g.cities.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), g.cities.len(), "seed {seed}");
        // With no checkbox on, every city is meant to be reached on foot.
        assert!(g.cities.iter().all(|c| c.reach == ReachClass::Foot), "seed {seed}");
    }
}

#[test]
fn what_it_writes_the_game_reads_with_its_cities() {
    let made = generate(&config(4, "small"), UNITS, "units.txt").unwrap();
    let map = made.map().unwrap();
    assert_eq!(map.cities().len(), made.state.cities.len());
    for (city, made) in map.cities().iter().zip(&made.state.cities) {
        assert_eq!(
            (city.pos, city.name.as_str(), city.side, city.walls),
            (made.hex, made.name.as_str(), made.side, made.walls)
        );
        assert!(!city.units.is_empty(), "{} builds something", city.name);
        assert!(!map.units_at(city.pos).is_empty(), "{} has a garrison", city.name);
    }
    // A capital: its side's highest walls, a commander and three more.
    let capital = made.state.cities.iter().find(|c| c.side != Side::Grey).unwrap();
    let units = map.units_at(capital.hex);
    assert_eq!(units.len(), 4);
    assert!(units.iter().any(|u| map.is_commander(*u)));
    // mapgen.txt makes the same cities again.
    let again = generate(&Config::default().with_text(&made.config_text()).unwrap(), UNITS, "units.txt").unwrap();
    assert_eq!(again.cities_text(), made.cities_text());
    assert_eq!(again.attempts, 1, "the seed that made it makes it at the first try");
}

#[test]
fn cities_at_sea_have_ships_and_cities_in_the_air_flyers() {
    let kinds = hexgame::state::scenario::parse_units(UNITS).unwrap();
    let class = |id: &str| kinds.iter().find(|k| k.id == id).map(movement_class);
    for seed in 1..=5 {
        let mut cfg = config(seed, "medium");
        cfg.set("sea_cities", "on").unwrap();
        cfg.set("fly_cities", "on").unwrap();
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let g = &made.state;
        let at_sea: Vec<_> = g.cities.iter().filter(|c| c.reach == ReachClass::Sea).collect();
        let in_air: Vec<_> = g.cities.iter().filter(|c| c.reach == ReachClass::Air).collect();
        assert!(
            !at_sea.is_empty() && !in_air.is_empty(),
            "seed {seed}: {} at sea, {} in the air",
            at_sea.len(),
            in_air.len()
        );
        for c in at_sea {
            assert!(*g.island.get(c.hex), "seed {seed}: {} is on an island", c.name);
            assert!(c.produce.iter().any(|(id, _)| class(id) == Some(MoveClass::Naval)), "{} builds a ship", c.name);
        }
        for c in in_air {
            assert_eq!(class(&c.produce[0].0), Some(MoveClass::Flyer), "{} builds a flyer first", c.name);
            // A ring of mountains two hexes out.
            let ring: Vec<_> = g.terrain.hexes().filter(|h| h.distance(c.hex) == 2).collect();
            assert!(ring.iter().all(|h| *g.terrain.get(*h) == TileType::Mountain), "{}: its ring", c.name);
        }
    }
}

#[test]
fn a_checkbox_needs_its_kind_of_unit_and_fairness_can_be_off() {
    let foot = "unit a\nmoves 10\nenter grass 2, city 1\nunit lord\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let mut cfg = config(2, "small");
    cfg.set("fly_cities", "on").unwrap();
    assert!(generate(&cfg, foot, "mine.txt").unwrap_err().contains("fly_cities is on"));
    // A template with no ship: no city builds one.
    let made = generate(&config(2, "small"), foot, "mine.txt").unwrap();
    assert!(made.state.cities.iter().all(|c| c.produce.iter().all(|(id, _)| id == "a" || id == "lord")));
    let mut off = config(2, "small");
    off.set("fairness", "off").unwrap();
    assert!(generate(&off, UNITS, "units.txt").unwrap().state.fairness.is_empty());
}
