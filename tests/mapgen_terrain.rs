//! The generator's skeleton and its terrain (`docs/mapgen.md`, milestones G1
//! and G2): the same seed makes the same files, and `mapgen.txt` makes the
//! map again; what it writes the game reads, and `mapgen check` passes; and
//! over seeds and sizes, the water and the land's mix are what the config
//! asks, and the terrain lies as the target map's does.

use hexgame::mapgen::config::Config;
use hexgame::mapgen::stats::Stats;
use hexgame::mapgen::{check, generate, score};

const UNITS: &str = include_str!("../assets/mapgen/units.txt");

fn config(seed: u64, size: &str) -> Config {
    let mut c = Config { seed, ..Config::default() };
    c.set("size", size).unwrap();
    c
}

#[test]
fn a_seed_makes_the_same_map_and_its_config_makes_it_again() {
    let a = generate(&config(11, "small"), UNITS, "units.txt").unwrap();
    let b = generate(&config(11, "small"), UNITS, "units.txt").unwrap();
    assert_eq!(a.terrain_text(), b.terrain_text());
    assert_eq!(a.config_text(), b.config_text());
    let other = generate(&config(12, "small"), UNITS, "units.txt").unwrap();
    assert_ne!(a.terrain_text(), other.terrain_text());
    // mapgen.txt, read back, makes it again.
    let again = Config::default().with_text(&a.config_text()).unwrap();
    assert_eq!(generate(&again, UNITS, "units.txt").unwrap().terrain_text(), a.terrain_text());
    // Its size is the plan's.
    assert_eq!((a.state.rows(), a.state.cols()), (54, 62));
}

#[test]
fn what_it_writes_the_game_reads_and_the_check_passes() {
    let made = generate(&config(3, "tiny"), UNITS, "units.txt").unwrap();
    let map = made.map().unwrap();
    assert_eq!((map.rows(), map.cols()), (38, 44));
    assert!(check::check(&map, true).problems.is_empty());
    // The units are the template's, word for word.
    assert_eq!(made.units, UNITS);
}

#[test]
fn a_checkbox_without_its_kind_of_unit_stops_the_generator() {
    let mut c = config(1, "tiny");
    c.sea_cities = true;
    let foot = "unit a\nmoves 10\nenter grass 2\n";
    let why = generate(&c, foot, "mine.txt").unwrap_err();
    assert_eq!(why, "sea_cities is on, and the units template (mine.txt) has no ship");
}

/// The measures of 20 seeds of a size: each metric's values.
fn measures(size: &str, seeds: u64) -> Vec<(String, f64, f64, Vec<f64>)> {
    let mut out: Vec<(String, f64, f64, Vec<f64>)> = Vec::new();
    for seed in 1..=seeds {
        let cfg = config(seed, size);
        let map = generate(&cfg, UNITS, "units.txt").unwrap().map().unwrap();
        for m in score::terrain(&cfg, &Stats::of(&map)) {
            match out.iter_mut().find(|(n, _, _, _)| *n == m.name) {
                Some((_, _, _, values)) => values.push(m.value),
                None => out.push((m.name.clone(), m.target, m.tolerance, vec![m.value])),
            }
        }
    }
    out
}

fn the_terrain_lies_as_asked(size: &str, seeds: u64, shape_slack: f64) {
    for (name, target, tolerance, values) in measures(size, seeds) {
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        if name.contains("share")
            || name.contains("% of land")
            || name.contains("edges on hill")
            || name.contains("walled")
        {
            // The water, the mix and the ring: on every map.
            for v in &values {
                assert!((v - target).abs() <= tolerance + 1e-9, "{size}: {name} {v} against {target}");
            }
        } else if !name.starts_with("water") {
            // The shapes, on the whole. (The water's waits for the rivers,
            // which narrow it: milestone G3.)
            assert!((mean - target).abs() <= tolerance * shape_slack, "{size}: {name}: mean {mean} against {target}");
        }
    }
}

#[test]
fn medium_maps_have_the_target_maps_terrain() {
    the_terrain_lies_as_asked("medium", 20, 1.0);
}

#[test]
fn huge_maps_have_the_target_maps_terrain() {
    the_terrain_lies_as_asked("huge", 20, 1.0);
}

#[test]
fn tiny_maps_come_near_it() {
    // A tiny map's patches are few and small, its edges near: its shapes
    // come within twice the tolerance, on the whole.
    the_terrain_lies_as_asked("tiny", 20, 2.0);
}

#[test]
fn every_layout_and_preset_makes_a_map() {
    for layout in ["coast", "continent", "inland", "twin", "archipelago"] {
        let mut c = config(5, "small");
        c.set("layout", layout).unwrap();
        let made = generate(&c, UNITS, "units.txt").unwrap();
        let s = Stats::of(&made.map().unwrap());
        assert!((s.water_share() - c.water).abs() <= 0.01, "{layout}: {}", s.water_share());
    }
    for preset in ["varied", "open", "rugged", "islands"] {
        let mut c = Config::preset(preset).unwrap();
        c.set("size", "small").unwrap();
        let s = Stats::of(&generate(&c, UNITS, "units.txt").unwrap().map().unwrap());
        assert!((s.water_share() - c.water).abs() <= 0.01, "{preset}: {}", s.water_share());
        for (i, t) in hexgame::mapgen::stats::LAND.iter().enumerate().skip(1) {
            assert!((s.land_share(*t) - c.mix[i]).abs() <= 1.0, "{preset}: {t:?}");
        }
    }
}
