//! The generator's measures against the target map: the map whose statistics
//! are the `classic` preset's targets. It is not in the repository and never
//! will be; this test reads it from the directory `MAPGEN_TARGET_DIR` names,
//! and is ignored unless asked for:
//!
//! ```text
//! MAPGEN_TARGET_DIR=<the map's directory> cargo test --test mapgen_target -- --ignored
//! ```
//!
//! It holds the ruler to the numbers the presets were tuned from
//! (`docs/mapgen.md`).

use std::path::PathBuf;

use hexlords::mapgen::check;
use hexlords::mapgen::reach::ReachClass;
use hexlords::mapgen::stats::Stats;
use hexlords::state::{TileType, scenario};

#[test]
#[ignore = "needs the target map: MAPGEN_TARGET_DIR"]
fn the_ruler_reads_the_target_map_as_its_targets_say() {
    let Some(dir) = std::env::var_os("MAPGEN_TARGET_DIR") else {
        eprintln!("MAPGEN_TARGET_DIR is not set: nothing to measure");
        return;
    };
    let map = scenario::load(&PathBuf::from(dir)).expect("the target map reads").map;
    let s = Stats::of(&map);
    let mix: Vec<(TileType, f64)> = [
        (TileType::Grass, 49.5),
        (TileType::Mountain, 17.1),
        (TileType::Hill, 15.5),
        (TileType::Forest, 13.9),
        (TileType::Swamp, 3.0),
        (TileType::Desert, 0.0),
    ]
    .to_vec();
    for (t, share) in mix {
        assert!((s.land_share(t) - share).abs() <= 0.1, "{t:?}: {} against {share}", s.land_share(t));
    }
    assert!((100.0 * s.places as f64 / s.land as f64 - 0.9).abs() <= 0.1);
    assert_eq!(s.cities.nearest.median, 8.0);
    assert!((s.mountains_on_hills() - 0.85).abs() < 0.01, "{}", s.mountains_on_hills());
    assert_eq!((s.roads.hexes, s.roads.bridges.len(), s.roads.passes.len()), (696, 17, 8));
    let c = check::check(&map, false);
    let count = |r: ReachClass| c.reach.iter().filter(|(_, x)| *x == r).count();
    assert_eq!((count(ReachClass::Foot), count(ReachClass::Sea), count(ReachClass::Air)), (77, 3, 0));
}
