//! The generator's temples and ruins (`docs/mapgen.md`, milestone G5): as
//! many as asked; apart from the cities and from each other; where a walker
//! gets; and shared out evenly among the sides, each kind on its own, with
//! each side's nearest temple about as far from its capital (the owner:
//! "make sure they are distributed fairly (both) since temples are a
//! definite boon").

use hexgame::mapgen::cities::travel;
use hexgame::mapgen::config::Config;
use hexgame::mapgen::generate;
use hexgame::mapgen::sites::count;
use hexgame::state::{Hex, Side, TileType};

const UNITS: &str = include_str!("../assets/mapgen/units.txt");

fn config(seed: u64, size: &str) -> Config {
    let mut c = Config { seed, ..Config::default() };
    c.set("size", size).unwrap();
    c
}

#[test]
fn sites_are_as_many_as_asked_apart_and_reached() {
    for seed in 1..=20 {
        let cfg = config(seed, "medium");
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        let g = &made.state;
        let t = &g.terrain;
        let of = |kind: TileType| t.hexes().filter(|h| *t.get(*h) == kind).collect::<Vec<Hex>>();
        let (temples, ruins) = (of(TileType::Temple), of(TileType::Ruin));
        // As many as asked: a rate, rounded to so many for each side.
        assert_eq!(temples.len(), count(cfg.temples, &cfg, &made.plan), "seed {seed}");
        assert_eq!(ruins.len(), count(cfg.ruins, &cfg, &made.plan), "seed {seed}");
        assert_eq!(temples.len() % usize::from(cfg.sides), 0, "seed {seed}");
        assert!(!temples.is_empty() && !ruins.is_empty());
        let capitals: Vec<Hex> = g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| c.hex).collect();
        let (reached, _) = travel(g, &capitals);
        let sites: Vec<Hex> = temples.iter().chain(&ruins).copied().collect();
        for (i, s) in sites.iter().enumerate() {
            for c in &g.cities {
                assert!(s.distance(c.hex) >= cfg.site_city_gap, "seed {seed}: {s} near {}", c.name);
            }
            for o in &sites[i + 1..] {
                assert!(s.distance(*o) >= cfg.site_gap, "seed {seed}: {s} near {o}");
            }
            // Where a walker from the capitals gets (a river crossed, as
            // the roads will bridge it).
            assert!(reached.get(*s).is_some(), "seed {seed}: {s} out of reach");
            assert!(!*g.island.get(*s) && !t.on_edge(*s), "seed {seed}: {s}");
        }
    }
}

#[test]
fn every_side_has_as_many_temples_and_ruins_and_a_temple_near_home() {
    for size in ["medium", "huge"] {
        for seed in 1..=10 {
            let cfg = config(seed, size);
            let made = generate(&cfg, UNITS, "units.txt").unwrap();
            let g = &made.state;
            let t = &g.terrain;
            let capitals: Vec<Hex> = g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| c.hex).collect();
            let grids: Vec<_> = capitals.iter().map(|c| travel(g, &[*c]).0).collect();
            let owner = |h: Hex| (0..capitals.len()).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().unwrap().1;
            for kind in [TileType::Temple, TileType::Ruin] {
                let mut n = vec![0usize; capitals.len()];
                for h in t.hexes().filter(|h| *t.get(*h) == kind) {
                    n[owner(h)] += 1;
                }
                assert!(n.iter().all(|k| *k == n[0]), "{size} seed {seed}: {kind:?} in the cells {n:?}");
            }
            // Each side's nearest temple, in turns: within two of the others'.
            let temples: Vec<Hex> = t.hexes().filter(|h| *t.get(*h) == TileType::Temple).collect();
            let near: Vec<u32> =
                grids.iter().map(|grid| temples.iter().filter_map(|h| *grid.get(*h)).min().unwrap()).collect();
            let (most, least) = (near.iter().max().unwrap(), near.iter().min().unwrap());
            assert!(most - least <= 20, "{size} seed {seed}: nearest temples {near:?}");
            assert!(g.fairness.iter().all(|f| f.ok()), "{size} seed {seed}");
        }
    }
}

#[test]
fn a_count_is_kept_as_asked_and_the_game_reads_the_sites() {
    let mut cfg = config(3, "small");
    cfg.set("temples", "7").unwrap();
    cfg.set("ruins", "0").unwrap();
    let made = generate(&cfg, UNITS, "units.txt").unwrap();
    let map = made.map().unwrap();
    let all = hexgame::mapgen::grid::Grid::new(map.rows(), map.cols(), ());
    let kind = |k: TileType| all.hexes().filter(|h| map.tile(*h).unwrap().kind == k).count();
    assert_eq!((kind(TileType::Temple), kind(TileType::Ruin)), (7, 0));
    // A temple in the game blesses (it is the game's temple).
    let stats = hexgame::mapgen::stats::Stats::of(&map);
    assert_eq!((stats.sites.temples, stats.sites.ruins), (7, 0));
}
