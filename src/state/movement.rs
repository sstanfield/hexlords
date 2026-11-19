//! Where a unit can go: the cheapest cost to reach each hex.
//!
//! C++: src/state/mapmove.cpp (Map::selectUnit, Map::deSelectUnit and the flood
//! fill behind them). The C++ wrote the costs into the tiles and made a single
//! pass, so a cost could be higher than the cheapest path's. This is Dijkstra's
//! algorithm, which always finds the cheapest (deviation D3), and the result is a
//! value of its own instead of fields in the map.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::hex::Hex;
use super::map::Map;
use super::terrain::{MoveCost, TileType};
use super::units::Unit;

/// The cheapest movement cost from an origin to each hex within a limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveRange {
    origin: Hex,
    cols: u16,
    /// Row-major, like the map's tiles. `None`: out of reach (or the origin).
    cost: Vec<Option<u32>>,
}

impl MoveRange {
    /// Where the unit stood when the range was computed.
    pub fn origin(&self) -> Hex {
        self.origin
    }

    /// The movement points it takes to get to `hex`, if it can be reached. The
    /// origin itself has no cost: it is `None`, like every hex out of reach.
    pub fn cost(&self, hex: Hex) -> Option<u32> {
        if hex.col >= self.cols {
            return None;
        }
        *self.cost.get(usize::from(hex.row) * usize::from(self.cols) + usize::from(hex.col))?
    }

    /// Every hex in reach, with its cost, in row-major order.
    pub fn reachable(&self) -> impl Iterator<Item = (Hex, u32)> + '_ {
        let cols = usize::from(self.cols);
        self.cost
            .iter()
            .enumerate()
            .filter_map(move |(i, c)| c.map(|c| (Hex::new((i / cols) as u16, (i % cols) as u16), c)))
    }
}

/// The hexes `unit` can reach with the movement points it has left.
///
/// The unit may stand on terrain it could not enter (the demo's unit starts on a
/// mountain): only the hexes it moves into are paid for.
pub fn compute_range(map: &Map, unit: &Unit) -> MoveRange {
    match map.units().move_cost(unit.move_cost) {
        Some(costs) => reach(map, unit.pos(), costs, unit.movement),
        None => MoveRange {
            origin: unit.pos(),
            cols: map.cols(),
            cost: vec![None; usize::from(map.rows()) * usize::from(map.cols())],
        },
    }
}

/// What a step from `from` to the neighbouring hex `to` costs, or `None` if it
/// cannot be made.
///
/// A step is paid for by the terrain it enters. A step *along a road*, from a
/// hex that joins roads (it has a road, or it is a city) onto a hex with a
/// road, costs the unit's road price instead, or its bridge price over water,
/// if that is cheaper or the terrain cannot be entered at all. So a bridge
/// and a mountain pass can only be entered along the road, and a road is no
/// help to a unit that crosses it. A unit without a road price pays for the
/// terrain as if no road were there.
pub fn step_cost(map: &Map, costs: &MoveCost, from: Hex, to: Hex) -> Option<u16> {
    let tile = map.tile(to)?;
    let terrain = costs.entry(tile.kind);
    if tile.road && map.joins_roads(from) {
        let along = if tile.kind == TileType::Water { costs.bridge } else { costs.road };
        if along > 0 {
            return Some(terrain.map_or(along, |t| t.min(along)));
        }
    }
    terrain
}

/// The cheapest cost from `origin` to every hex that costs at most `limit` to
/// reach, at the prices in `costs` (see [`step_cost`]).
pub fn reach(map: &Map, origin: Hex, costs: &MoveCost, limit: u32) -> MoveRange {
    let cols = usize::from(map.cols());
    let index = |h: Hex| usize::from(h.row) * cols + usize::from(h.col);
    let mut best: Vec<Option<u32>> = vec![None; usize::from(map.rows()) * cols];
    let mut range = MoveRange { origin, cols: map.cols(), cost: Vec::new() };
    if !map.contains(origin) {
        range.cost = best;
        return range;
    }
    best[index(origin)] = Some(0);
    let mut queue = BinaryHeap::from([Reverse((0u32, origin))]);
    while let Some(Reverse((spent, hex))) = queue.pop() {
        if best[index(hex)] != Some(spent) {
            continue; // a cheaper way here was found after this entry was queued
        }
        for next in map.neighbours(hex) {
            let Some(step) = step_cost(map, costs, hex, next) else { continue };
            let total = spent + u32::from(step);
            if total <= limit && best[index(next)].is_none_or(|old| total < old) {
                best[index(next)] = Some(total);
                queue.push(Reverse((total, next)));
            }
        }
    }
    best[index(origin)] = None;
    range.cost = best;
    range
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::units::Side;

    fn cav() -> MoveCost {
        MoveCost {
            grass: 2,
            forest: 4,
            swamp: 5,
            desert: 3,
            hill: 4,
            bridge: 1,
            road: 2,
            city: 1,
            ..MoveCost::default()
        }
    }

    fn grass() -> Map {
        Map::new(20, 20, TileType::Grass).unwrap()
    }

    fn set(map: &mut Map, row: u16, col: u16, kind: TileType) {
        map.tile_mut(Hex::new(row, col)).unwrap().kind = kind;
    }

    /// The same costs by repeated relaxation until nothing changes (Bellman-Ford):
    /// slow, and written without a queue so that it shares nothing with `reach`.
    fn brute_force(map: &Map, origin: Hex, costs: &MoveCost, limit: u32) -> Vec<Option<u32>> {
        let (rows, cols) = (map.rows(), map.cols());
        let at = |h: Hex| usize::from(h.row) * usize::from(cols) + usize::from(h.col);
        let mut best: Vec<Option<u32>> = vec![None; usize::from(rows) * usize::from(cols)];
        best[at(origin)] = Some(0);
        loop {
            let mut changed = false;
            for row in 0..rows {
                for col in 0..cols {
                    let hex = Hex::new(row, col);
                    for from in hex.neighbours(rows, cols) {
                        let Some(step) = step_cost(map, costs, from, hex) else { continue };
                        let Some(spent) = best[at(from)] else { continue };
                        let total = spent + u32::from(step);
                        if total <= limit && best[at(hex)].is_none_or(|old| total < old) {
                            best[at(hex)] = Some(total);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        best[at(origin)] = None;
        best
    }

    #[test]
    fn open_grass_costs_two_a_step() {
        let map = grass();
        let origin = Hex::new(10, 10);
        let range = reach(&map, origin, &cav(), 6);
        assert_eq!(range.origin(), origin);
        assert_eq!(range.cost(origin), None);
        for n in map.neighbours(origin) {
            assert_eq!(range.cost(n), Some(2), "{n}");
        }
        // Straight up and down the column, and along the row.
        assert_eq!(range.cost(Hex::new(8, 10)), Some(4));
        assert_eq!(range.cost(Hex::new(7, 10)), Some(6));
        assert_eq!(range.cost(Hex::new(6, 10)), None);
        assert_eq!(range.cost(Hex::new(10, 13)), Some(6));
        assert_eq!(range.cost(Hex::new(10, 14)), None);
        // Three steps reach 1 + 6 + 12 + 18 hexes, less the origin.
        assert_eq!(range.reachable().count(), 36);
        assert!(range.reachable().all(|(_, c)| c == 2 || c == 4 || c == 6));
        // Off the map is out of reach.
        assert_eq!(range.cost(Hex::new(10, 20)), None);
        assert_eq!(range.cost(Hex::new(20, 10)), None);
    }

    #[test]
    fn the_limit_is_inclusive_and_zero_reaches_nothing() {
        let map = grass();
        let origin = Hex::new(10, 10);
        assert_eq!(reach(&map, origin, &cav(), 0).reachable().count(), 0);
        assert_eq!(reach(&map, origin, &cav(), 1).reachable().count(), 0);
        assert_eq!(reach(&map, origin, &cav(), 2).reachable().count(), 6);
        assert_eq!(reach(&map, origin, &cav(), 3).reachable().count(), 6);
    }

    #[test]
    fn terrain_costs_are_paid_on_entry() {
        let mut map = grass();
        set(&mut map, 10, 11, TileType::Forest);
        set(&mut map, 10, 12, TileType::Swamp);
        let range = reach(&map, Hex::new(10, 10), &cav(), 20);
        assert_eq!(range.cost(Hex::new(10, 11)), Some(4));
        // Into the swamp from the grass at (11, 11): 2 + 5. Through the forest it
        // would be 4 + 5.
        assert_eq!(range.cost(Hex::new(10, 12)), Some(7));
        // Past both: forest, then two grass steps (4 + 2 + 2), which beats going
        // through the swamp (2 + 5 + 2). Over open grass it would be 6.
        assert_eq!(range.cost(Hex::new(10, 13)), Some(8));
        let want = brute_force(&map, Hex::new(10, 10), &cav(), 20);
        assert_eq!(range.reachable().count(), want.iter().flatten().count());
    }

    #[test]
    fn a_mountain_wall_must_be_walked_around() {
        let mut map = grass();
        // A wall down column 12 from row 5 to row 15, with the unit to its left.
        for row in 5..=15 {
            set(&mut map, row, 12, TileType::Mountain);
        }
        let origin = Hex::new(10, 10);
        let range = reach(&map, origin, &cav(), 40);
        for row in 5..=15 {
            assert_eq!(range.cost(Hex::new(row, 12)), None, "the wall, row {row}");
        }
        // Just behind the wall: 3 columns away, but the way round is much longer.
        let behind = range.cost(Hex::new(10, 13)).unwrap();
        let open = reach(&grass(), origin, &cav(), 40).cost(Hex::new(10, 13)).unwrap();
        assert_eq!(open, 6);
        assert!(behind > open + 6, "{behind}");
        assert_eq!(Some(behind), brute_force(&map, origin, &cav(), 40)[10 * 20 + 13]);
        // With too few points the far side is out of reach.
        assert_eq!(reach(&map, origin, &cav(), 10).cost(Hex::new(10, 13)), None);
    }

    #[test]
    fn a_walled_in_pocket_is_out_of_reach() {
        let mut map = grass();
        let pocket = Hex::new(5, 5);
        for n in map.neighbours(pocket).collect::<Vec<_>>() {
            map.tile_mut(n).unwrap().kind = TileType::Water;
        }
        let range = reach(&map, Hex::new(10, 10), &cav(), 1000);
        assert_eq!(range.cost(pocket), None);
        // Everything else that is not water is reached.
        let grass_tiles = 20 * 20 - 6 - 1;
        assert_eq!(range.reachable().count(), grass_tiles - 1);
        // And from inside the pocket there is no way out.
        assert_eq!(reach(&map, pocket, &cav(), 1000).reachable().count(), 0);
    }

    #[test]
    fn a_unit_on_terrain_it_cannot_enter_can_still_leave() {
        // The demo's unit starts on a mountain.
        let mut map = grass();
        let origin = Hex::new(10, 10);
        set(&mut map, 10, 10, TileType::Mountain);
        let range = reach(&map, origin, &cav(), 4);
        assert_eq!(range.cost(origin), None);
        assert_eq!(range.reachable().count(), 18);
    }

    #[test]
    fn compute_range_uses_the_units_costs_and_movement() {
        let mut map = grass();
        let mc = map.acquire_move_cost(cav());
        let id = map.new_unit(Side::Black, "Cav", 3, 5, 4, mc);
        let origin = Hex::new(10, 10);
        map.place_unit(id, origin).unwrap();
        let range = compute_range(&map, map.units().get(id).unwrap());
        assert_eq!(range, reach(&map, origin, &cav(), 4));
        assert_eq!(range.reachable().count(), 18);
        // Selecting does not change the map: the costs are in the range.
        assert_eq!(map.to_text(), grass().to_text());
    }

    #[test]
    fn agrees_with_brute_force_on_random_maps() {
        // A small fixed-seed generator (no `rand`): the maps are the same every run.
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for case in 0..40 {
            let mut map = grass();
            for row in 0..20 {
                for col in 0..20 {
                    // Mostly passable, with enough mountains and water to make walls.
                    let kind = match next(10) {
                        0..=3 => TileType::Grass,
                        4 => TileType::Forest,
                        5 => TileType::Swamp,
                        6 => TileType::Hill,
                        7 => TileType::Desert,
                        8 => TileType::Mountain,
                        _ => TileType::Water,
                    };
                    set(&mut map, row, col, kind);
                }
            }
            let origin = Hex::new(next(20) as u16, next(20) as u16);
            let limit = next(40) as u32;
            let got = reach(&map, origin, &cav(), limit);
            let want = brute_force(&map, origin, &cav(), limit);
            for row in 0..20 {
                for col in 0..20 {
                    let h = Hex::new(row, col);
                    assert_eq!(got.cost(h), want[usize::from(row) * 20 + usize::from(col)], "case {case}: {h}");
                }
            }
        }
    }

    /// A unit that walks: roads are cheap, and it can cross bridges.
    fn foot() -> MoveCost {
        MoveCost { grass: 3, forest: 5, hill: 6, city: 2, road: 1, bridge: 2, ..MoveCost::default() }
    }

    fn road(map: &mut Map, row: u16, col: u16) {
        map.tile_mut(Hex::new(row, col)).unwrap().road = true;
    }

    #[test]
    fn a_step_along_a_road_costs_the_roads_price() {
        let mut map = grass();
        // A road along row 10 from column 5 to 9, through a forest at column 7.
        for col in 5..=9 {
            road(&mut map, 10, col);
        }
        set(&mut map, 10, 7, TileType::Forest);
        let (a, b, forest) = (Hex::new(10, 5), Hex::new(10, 6), Hex::new(10, 7));
        assert_eq!(step_cost(&map, &foot(), a, b), Some(1));
        assert_eq!(step_cost(&map, &foot(), b, forest), Some(1), "the road goes through the forest");
        // Onto the road from beside it, and off it again: the terrain's price.
        let beside = Hex::new(11, 6);
        assert!(beside.is_adjacent(b) && !map.tile(beside).unwrap().road);
        assert_eq!(step_cost(&map, &foot(), beside, b), Some(3));
        assert_eq!(step_cost(&map, &foot(), b, beside), Some(3));
        assert_eq!(step_cost(&map, &foot(), Hex::new(9, 7), forest), Some(5), "into the forest from beside the road");
        // A unit with no price for roads walks on them as on the ground under them.
        let no_roads = MoveCost { road: 0, ..foot() };
        assert_eq!(step_cost(&map, &no_roads, a, b), Some(3));
        // And a road is never dearer than the ground.
        let slow_roads = MoveCost { road: 9, ..foot() };
        assert_eq!(step_cost(&map, &slow_roads, a, b), Some(3));
        // Off the map is no step.
        assert_eq!(step_cost(&map, &foot(), a, Hex::new(40, 40)), None);
    }

    #[test]
    fn a_bridge_and_a_pass_are_entered_only_along_the_road() {
        let mut map = grass();
        // Column 10 is a river with a bridge at row 10, and column 14 a
        // mountain range with a pass at row 10; a road runs along row 10.
        for row in 0..20 {
            set(&mut map, row, 10, TileType::Water);
            set(&mut map, row, 14, TileType::Mountain);
        }
        for col in 8..=16 {
            road(&mut map, 10, col);
        }
        let (bridge, pass) = (Hex::new(10, 10), Hex::new(10, 14));
        assert_eq!(step_cost(&map, &foot(), Hex::new(10, 9), bridge), Some(2));
        assert_eq!(step_cost(&map, &foot(), Hex::new(10, 13), pass), Some(1));
        // From the bank beside the road's end, the water is water.
        let bank = Hex::new(11, 9);
        assert!(bank.is_adjacent(bridge));
        assert_eq!(step_cost(&map, &foot(), bank, bridge), None);
        assert_eq!(step_cost(&map, &foot(), Hex::new(9, 10), bridge), None, "nor from the water beside it");
        // Without a price for bridges there is no crossing.
        assert_eq!(step_cost(&map, &MoveCost { bridge: 0, ..foot() }, Hex::new(10, 9), bridge), None);

        // The range of a unit west of the river goes over the bridge and on.
        let range = reach(&map, Hex::new(10, 8), &foot(), 12);
        assert_eq!(range.cost(Hex::new(10, 9)), Some(1));
        assert_eq!(range.cost(bridge), Some(3));
        assert_eq!(range.cost(Hex::new(10, 11)), Some(4));
        assert_eq!(range.cost(pass), Some(7));
        assert_eq!(range.cost(Hex::new(10, 16)), Some(9));
        assert_eq!(range.cost(Hex::new(9, 10)), None, "the river beside the bridge");
        assert_eq!(range.cost(Hex::new(9, 14)), None, "the mountains beside the pass");
        assert_eq!(range.cost, brute_force(&map, Hex::new(10, 8), &foot(), 12));
        // Take the bridge away and the far bank is out of reach.
        map.tile_mut(bridge).unwrap().road = false;
        assert_eq!(reach(&map, Hex::new(10, 8), &foot(), 12).cost(Hex::new(10, 11)), None);
    }

    #[test]
    fn roads_run_up_to_a_city() {
        let mut map = grass();
        let city = Hex::new(10, 10);
        set(&mut map, 10, 10, TileType::City);
        road(&mut map, 10, 11);
        road(&mut map, 10, 12);
        // Out of the gate onto the road is a step along the road, though the
        // city has no road of its own; into the city costs what a city costs.
        assert!(map.joins_roads(city) && !map.tile(city).unwrap().road);
        assert_eq!(step_cost(&map, &foot(), city, Hex::new(10, 11)), Some(1));
        assert_eq!(step_cost(&map, &foot(), Hex::new(10, 11), city), Some(2));
    }
}
