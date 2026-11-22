//! The owner's eye, as tests: maps the generator made at milestone G2, judged
//! best and worst (`tests/mapgen/judged/README.md`), kept as they were made.
//! A measure the generator is held to must agree with the judgement; and
//! what was disliked, the generator no longer makes.

use std::path::PathBuf;

use hexlords::mapgen::config::Config;
use hexlords::mapgen::generate;
use hexlords::mapgen::stats::Stats;
use hexlords::state::scenario;

fn judged(name: &str) -> Stats {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mapgen/judged").join(name);
    Stats::of(&scenario::load(&dir).expect("a judged map reads").map)
}

#[test]
fn land_walled_in_by_mountains_is_what_the_worst_had() {
    let walled = |name: &str| judged(name).pockets.iter().sum::<usize>();
    // "The weird smallish enclosed space by mountains in 11 was bad"; 3 had
    // a valley shut in by two ranges.
    for bad in ["bad-11", "bad-3"] {
        assert!(walled(bad) >= 100, "{bad}: {}", walled(bad));
    }
    for good in ["good-1", "good-9", "good-12", "good-13", "good-16"] {
        assert!(walled(good) <= 30, "{good}: {}", walled(good));
    }
}

#[test]
fn the_generator_walls_no_land_in() {
    let units = include_str!("../assets/mapgen/units.txt");
    for seed in 1..=16 {
        let mut c = Config { seed, ..Config::default() };
        c.set("size", "medium").unwrap();
        let map = generate(&c, units, "units.txt").unwrap().map().unwrap();
        assert_eq!(Stats::of(&map).pockets, Vec::<usize>::new(), "seed {seed}");
    }
}
