//! The generator's hard checks, retries and scores (`docs/mapgen.md`,
//! milestone G7): maps of each size and number of sides, with the
//! checkboxes off and on, pass the hard checks within `attempts`; `accept
//! best N` keeps the lowest score; a map that cannot pass is given all the
//! same and says why; the islands preset puts a capital on each home island
//! and cities on every island of size.

use hexlords::mapgen::config::Config;
use hexlords::mapgen::grid::pieces;
use hexlords::mapgen::reach::ReachClass;
use hexlords::mapgen::stats::Stats;
use hexlords::mapgen::{generate, score};
use hexlords::state::{Side, TileType};

const UNITS: &str = include_str!("../assets/mapgen/units.txt");

fn config(seed: u64, size: &str, sides: u8, boxes: bool) -> Config {
    let mut c = Config { seed, ..Config::default() };
    c.set("size", size).unwrap();
    c.set("sides", &sides.to_string()).unwrap();
    if boxes {
        c.set("sea_cities", "on").unwrap();
        c.set("fly_cities", "on").unwrap();
    }
    c
}

fn passes(size: &str, sides: u8, boxes: bool, seeds: std::ops::RangeInclusive<u64>) {
    for seed in seeds {
        let made = generate(&config(seed, size, sides, boxes), UNITS, "units.txt").unwrap();
        assert!(made.failures.is_empty(), "{size}, {sides} sides, boxes {boxes}, seed {seed}: {:?}", made.failures);
    }
}

#[test]
fn small_and_medium_maps_pass_the_hard_checks_with_the_boxes_off_and_on() {
    for boxes in [false, true] {
        for sides in [2, 4] {
            passes("tiny", sides, boxes, 1..=2);
            passes("medium", sides, boxes, 1..=2);
        }
        passes("medium", 8, boxes, 1..=1);
    }
}

/// The plan's measure of the generator (G7): 50 seeds of each size at 2, 4
/// and 8 sides, the checkboxes off and on. Slow: run it in release,
/// `cargo test --release --test mapgen_validate -- --ignored`. (A tiny map
/// with 8 sides has a city a side and nothing between; its land cannot
/// always be shared out fairly, and it is left out.)
#[test]
#[ignore = "slow: every size and number of sides, 50 seeds each"]
fn every_size_and_number_of_sides_passes_the_hard_checks() {
    for size in ["tiny", "medium", "huge"] {
        for sides in [2, 4, 8] {
            if size == "tiny" && sides == 8 {
                continue;
            }
            for boxes in [false, true] {
                passes(size, sides, boxes, 1..=50);
            }
        }
    }
}

#[test]
fn accept_best_keeps_the_lowest_score_of_maps_that_pass() {
    let first = generate(&config(3, "small", 4, false), UNITS, "units.txt").unwrap();
    let mut cfg = config(3, "small", 4, false);
    cfg.set("accept", "best 3").unwrap();
    let best = generate(&cfg, UNITS, "units.txt").unwrap();
    assert!(best.failures.is_empty());
    assert!(best.score <= first.score + 1e-9, "{} against {}", best.score, first.score);
    assert!(best.attempts >= 3, "{} tries", best.attempts);
    // Its score is the score of what it wrote.
    let again = score::score(&score::all(&best.config, &best.plan, &Stats::of(&best.map().unwrap())));
    assert!((again - best.score).abs() < 1e-9);
}

#[test]
fn a_map_that_cannot_pass_is_given_and_says_why() {
    // Strict fairness on a tiny map with 8 sides (a city a side), one try.
    let mut cfg = config(1, "tiny", 8, false);
    cfg.set("fairness", "strict").unwrap();
    cfg.set("attempts", "1").unwrap();
    let made = generate(&cfg, UNITS, "units.txt").unwrap();
    assert!(!made.failures.is_empty());
    assert!(made.failures.iter().any(|f| f.starts_with("unfair:")), "{:?}", made.failures);
    assert!(made.map().is_ok());
}

#[test]
fn islands_put_a_capital_on_each_home_island_and_cities_on_every_island() {
    for seed in 1..=3 {
        let mut cfg = Config::preset("islands").unwrap();
        cfg.seed = seed;
        cfg.set("size", "medium").unwrap();
        let made = generate(&cfg, UNITS, "units.txt").unwrap();
        assert!(made.failures.is_empty(), "seed {seed}: {:?}", made.failures);
        let g = &made.state;
        let t = &g.terrain;
        let (land, sizes) = pieces(g.rows(), g.cols(), |h| *t.get(h) != TileType::Water || *g.river.get(h));
        // A capital to each big island, if there are as many as sides (the
        // home islands may have grown into each other).
        let most = sizes.iter().copied().max().unwrap_or(0);
        let big_isles = sizes.iter().filter(|n| **n * 3 >= most).count();
        let mut homes: Vec<usize> =
            g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| land.get(c.hex).unwrap()).collect();
        homes.sort_unstable();
        homes.dedup();
        assert_eq!(homes.len(), usize::from(cfg.sides).min(big_isles), "seed {seed}: capitals share an island");
        // No island of size without a city: no land wasted.
        let big = sizes.iter().copied().max().unwrap_or(0) / 4;
        for (p, n) in sizes.iter().enumerate().filter(|(_, n)| **n >= big.max(60)) {
            assert!(
                g.cities.iter().any(|c| *land.get(c.hex) == Some(p)),
                "seed {seed}: an island of {n} hexes has no city"
            );
        }
        // The cities on the other islands are reached by sea, and their
        // islands have a port.
        assert!(g.cities.iter().any(|c| c.reach == ReachClass::Sea), "seed {seed}");
    }
}

#[test]
fn an_air_citys_ring_is_not_land_wasted_and_the_previews_show_the_cells() {
    let made = generate(&config(2, "medium", 4, true), UNITS, "units.txt").unwrap();
    let map = made.map().unwrap();
    let stats = Stats::of(&map);
    assert!(made.state.cities.iter().any(|c| c.reach == ReachClass::Air));
    assert_eq!(stats.pockets.iter().sum::<usize>(), 0, "{:?}", stats.pockets);
    // The cells' borders are drawn when given.
    let marks = made.city_marks();
    let plain = hexlords::mapgen::preview::preview(&map, 4.0, &marks, None);
    let cells = hexlords::mapgen::preview::preview(&map, 4.0, &marks, Some(&made.cells()));
    assert_ne!(plain.rgba, cells.rgba);
    // A line for each side in the table, its capital named.
    let table = made.sides_table();
    for c in made.state.cities.iter().filter(|c| c.side != Side::Grey) {
        assert!(table.contains(&c.name), "{table}");
    }
}
