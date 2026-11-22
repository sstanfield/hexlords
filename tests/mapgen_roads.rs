//! The generator's roads (`docs/mapgen.md`, milestone G6): every bridge and
//! pass on a road that runs on over land at both ends, no longer than
//! allowed; `linked` puts every city on foot on one network, `sparse` about
//! three in five on a road; no road onto a sea city's island or into an air
//! city's ring; and what comes out passes `mapgen check` with every city
//! reached as planned, and every temple and ruin on foot (rivers are no
//! wall any more: bridges cross them).

use hexlords::mapgen::config::Config;
use hexlords::mapgen::grid::{Grid, pieces};
use hexlords::mapgen::reach::{MoveClass, ReachClass, reach};
use hexlords::mapgen::stats::Stats;
use hexlords::mapgen::{Generated, check, generate};
use hexlords::state::{Hex, Map, Side, TileType};

const UNITS: &str = include_str!("../assets/mapgen/units.txt");

fn config(seed: u64, size: &str) -> Config {
    let mut c = Config { seed, ..Config::default() };
    c.set("size", size).unwrap();
    c
}

/// The check passes (strict: every crossing on a road line with land road
/// at both ends), and every city is reached as it was planned to be.
fn checked(made: &Generated, seed: u64) -> Map {
    let map = made.map().unwrap();
    let c = check::check(&map, true);
    assert!(c.problems.is_empty(), "seed {seed}: {:?}", c.problems);
    for (name, class) in &c.reach {
        let planned = made.state.cities.iter().find(|x| x.name == *name).unwrap().reach;
        assert_eq!(*class, planned, "seed {seed}: {name}");
    }
    map
}

/// The road networks: pieces of hexes that join roads (a road, a city).
fn networks(map: &Map) -> (Grid<Option<usize>>, Vec<usize>) {
    pieces(map.rows(), map.cols(), |h| map.joins_roads(h))
}

#[test]
fn linked_roads_join_every_city_and_cross_rivers_and_ranges_on_land_to_land() {
    for seed in 1..=20 {
        let cfg = config(seed, "medium");
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let map = checked(&made, seed);
        // One network holds every city on foot (no checkbox: every city).
        let (net, _) = networks(&map);
        let cities: Vec<_> = map.cities().iter().collect();
        let first = *net.get(cities[0].pos);
        for c in cities {
            assert_eq!(*net.get(c.pos), first, "seed {seed}: {} is off the network", c.name);
            assert!(map.neighbours(c.pos).any(|n| map.tile(n).unwrap().road), "seed {seed}: {} has no road", c.name);
        }
        // Few tangles (in the game every two road hexes that touch are
        // joined): roads run in clean lines, and few cities have two roads
        // leaving by neighbouring sides (a triangle at the gate; where
        // water or mountains crowd a city with three roads it cannot
        // always be helped). Without the cost of touching, one city in
        // five had one.
        let s = Stats::of(&map);
        assert!(
            s.roads.lines * 100 >= s.roads.hexes * 93,
            "seed {seed}: {} of {} in lines",
            s.roads.lines,
            s.roads.hexes
        );
        let triangles = map
            .cities()
            .iter()
            .filter(|c| {
                let gate: Vec<Hex> = map.neighbours(c.pos).filter(|n| map.tile(*n).unwrap().road).collect();
                gate.iter().enumerate().any(|(i, a)| gate[i + 1..].iter().any(|b| a.is_adjacent(*b)))
            })
            .count();
        assert!(triangles * 10 <= map.cities().len(), "seed {seed}: {triangles} gates with a triangle");
        // Bridges and passes no longer than allowed; passes no more.
        assert!(s.roads.bridges.iter().all(|b| *b as u32 <= cfg.max_bridge), "seed {seed}: {:?}", s.roads.bridges);
        assert!(s.roads.passes.iter().all(|p| (1..=2).contains(p)), "seed {seed}: {:?}", s.roads.passes);
        let budget = cfg.passes.of(f64::from(made.plan.cities)) as usize;
        assert!(s.roads.passes.len() <= budget, "seed {seed}: {} passes", s.roads.passes.len());
        // Every temple and ruin is reached on foot from the capitals.
        let capitals: Vec<Hex> = map.cities().iter().filter(|c| c.side != Side::Grey).map(|c| c.pos).collect();
        let walked = reach(&map, MoveClass::Foot, &capitals);
        let all = Grid::new(map.rows(), map.cols(), ());
        for h in all.hexes().filter(|h| matches!(map.tile(*h).unwrap().kind, TileType::Temple | TileType::Ruin)) {
            assert!(*walked.get(h), "seed {seed}: the site at {h} is out of reach");
        }
    }
}

#[test]
fn no_road_reaches_a_city_at_sea_or_in_the_air() {
    for seed in 1..=6 {
        let mut cfg = config(seed, "medium");
        cfg.set("sea_cities", "on").unwrap();
        cfg.set("fly_cities", "on").unwrap();
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let map = checked(&made, seed);
        let g = &made.state;
        let air: Vec<Hex> = g.cities.iter().filter(|c| c.reach == ReachClass::Air).map(|c| c.hex).collect();
        for h in g.terrain.hexes().filter(|h| *g.road.get(*h)) {
            assert!(!*g.island.get(h), "seed {seed}: a road on an island at {h}");
            assert!(air.iter().all(|a| a.distance(h) > 2), "seed {seed}: a road into an air city's ring at {h}");
        }
        // The cities on foot are still one network.
        let (net, _) = networks(&map);
        let foot: Vec<Hex> = g.cities.iter().filter(|c| c.reach == ReachClass::Foot).map(|c| c.hex).collect();
        assert!(foot.iter().all(|c| *net.get(*c) == *net.get(foot[0])), "seed {seed}");
    }
}

#[test]
fn sparse_roads_put_about_three_cities_in_five_on_a_road() {
    let mut shares = Vec::new();
    for seed in 1..=10 {
        let mut cfg = config(seed, "medium");
        cfg.set("roads", "sparse").unwrap();
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let map = checked(&made, seed);
        let s = Stats::of(&map);
        let share = s.roads.cities_on_road as f64 / map.cities().len() as f64;
        assert!((0.45..=0.8).contains(&share), "seed {seed}: {share}");
        shares.push(share);
    }
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    assert!((0.55..=0.7).contains(&mean), "{shares:?}");
}

#[test]
fn without_roads_every_city_is_still_reached_by_fords() {
    for seed in 1..=4 {
        let mut cfg = config(seed, "medium");
        cfg.set("roads", "none").unwrap();
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        checked(&made, seed);
        assert!(made.state.road.cells().iter().all(|r| !r), "seed {seed}: a road");
    }
}
