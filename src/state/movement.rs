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
use super::units::{Side, Unit};

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

    /// Where two units can both go (a stack): the hexes in both ranges, at the
    /// dearer of the two costs.
    pub fn both(&self, other: &MoveRange) -> MoveRange {
        let cost = self.cost.iter().zip(&other.cost).map(|(a, b)| Some((*a)?.max((*b)?))).collect();
        MoveRange { origin: self.origin, cols: self.cols, cost }
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

/// The hexes `unit` can reach with the movement points it has left, as a unit
/// of its side ([`passage`]: others' units are in its way, others' cities end
/// its move).
///
/// The unit may stand on terrain it could not enter (the demo's unit starts on a
/// mountain): only the hexes it moves into are paid for.
pub fn compute_range(map: &Map, unit: &Unit) -> MoveRange {
    match map.units().move_cost(unit.move_cost) {
        Some(costs) => {
            let side = unit.side();
            reach_with(map, unit.pos(), costs, unit.movement, &|hex| passage(map, side, hex))
        }
        None => MoveRange {
            origin: unit.pos(),
            cols: map.cols(),
            cost: vec![None; usize::from(map.rows()) * usize::from(map.cols())],
        },
    }
}

/// What a hex is to a unit that would move into it.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Passage {
    /// It goes in, and on if it has the points.
    Open,
    /// It goes in, and its move ends there.
    Stop,
    /// It does not go in.
    Blocked,
}

/// What a hex is to a unit of `side`. Another side's units there can be
/// attacked (going in is a battle), and not gone through; another side's
/// city, or nobody's, can be gone into, which takes it if nobody is in it:
/// either way the move ends there. (Nothing blocks a hex yet.)
pub fn passage(map: &Map, side: Side, hex: Hex) -> Passage {
    let others = map.units_at(hex).iter().filter_map(|id| map.units().get(*id)).any(|u| u.side() != side);
    if others || map.city_at(hex).is_some_and(|city| city.side != side) { Passage::Stop } else { Passage::Open }
}

/// True if `hex` has units of a side other than `side`: going in is a battle.
pub fn hostile(map: &Map, side: Side, hex: Hex) -> bool {
    map.units_at(hex).iter().filter_map(|id| map.units().get(*id)).any(|u| u.side() != side)
}

/// True if a unit that pays `costs` could get from `from` to `to` over the
/// terrain, in as many turns as it takes, whoever stands on the way (units
/// move; the terrain stays).
pub fn way_over_land(map: &Map, costs: &MoveCost, from: Hex, to: Hex) -> bool {
    if !map.contains(from) || !map.contains(to) {
        return false;
    }
    let cols = usize::from(map.cols());
    let index = |h: Hex| usize::from(h.row) * cols + usize::from(h.col);
    let mut seen = vec![false; usize::from(map.rows()) * cols];
    seen[index(from)] = true;
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(hex) = queue.pop_front() {
        if hex == to {
            return true;
        }
        for next in map.neighbours(hex) {
            if !seen[index(next)] && step_cost(map, costs, hex, next).is_some() {
                seen[index(next)] = true;
                queue.push_back(next);
            }
        }
    }
    false
}

/// Where `unit` gets to this turn on the cheapest way toward `target`, and
/// what that costs: the target itself if it is in reach, else the farthest
/// hex along that way that it can pay for (and that has room for it). `None`
/// if there is no way there, or it cannot take the first step.
pub fn step_toward(map: &Map, unit: &Unit, target: Hex) -> Option<(Hex, u32)> {
    step_toward_with(map, unit, target, &|hex| map.has_room(hex, unit.id()))
}

/// [`step_toward`], where `room` says which hexes have room (for a stack
/// that goes along with `unit`, say). Another side's units always have
/// room: going in is an attack.
pub fn step_toward_with(map: &Map, unit: &Unit, target: Hex, room: &dyn Fn(Hex) -> bool) -> Option<(Hex, u32)> {
    let costs = map.units().move_cost(unit.move_cost)?;
    let side = unit.side();
    let cols = usize::from(map.cols());
    let index = |h: Hex| usize::from(h.row) * cols + usize::from(h.col);
    let size = usize::from(map.rows()) * cols;
    let (origin, open) = (unit.pos(), |hex: Hex| passage(map, side, hex));
    if !map.contains(origin) || !map.contains(target) || target == origin {
        return None;
    }
    // The cheapest way to every hex, and the hex it came from.
    let mut best: Vec<Option<u32>> = vec![None; size];
    let mut from: Vec<Option<Hex>> = vec![None; size];
    best[index(origin)] = Some(0);
    let mut queue = BinaryHeap::from([Reverse((0u32, origin))]);
    while let Some(Reverse((spent, hex))) = queue.pop() {
        if best[index(hex)] != Some(spent) || hex == target {
            continue;
        }
        if hex != origin && open(hex) == Passage::Stop {
            continue; // a city on the way: the move would end there
        }
        for next in map.neighbours(hex) {
            if open(next) == Passage::Blocked {
                continue;
            }
            let Some(step) = step_cost(map, costs, hex, next) else { continue };
            let total = spent + u32::from(step);
            if best[index(next)].is_none_or(|old| total < old) {
                best[index(next)] = Some(total);
                from[index(next)] = Some(hex);
                queue.push(Reverse((total, next)));
            }
        }
    }
    best[index(target)]?;
    // Back from the target: the first hex that is paid for, and has room.
    let mut way = vec![target];
    while let Some(prev) = from[index(*way.last()?)] {
        if prev == origin {
            break;
        }
        way.push(prev);
    }
    way.into_iter().find_map(|hex| {
        let cost = best[index(hex)]?;
        let room = hostile(map, side, hex) || room(hex);
        (cost <= unit.movement && room).then_some((hex, cost))
    })
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
/// reach, at the prices in `costs` (see [`step_cost`]), whoever is on the way.
pub fn reach(map: &Map, origin: Hex, costs: &MoveCost, limit: u32) -> MoveRange {
    reach_with(map, origin, costs, limit, &|_| Passage::Open)
}

/// The same, with what each hex is to the one who moves (see [`passage`]).
pub fn reach_with(map: &Map, origin: Hex, costs: &MoveCost, limit: u32, passage: &dyn Fn(Hex) -> Passage) -> MoveRange {
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
        if hex != origin && passage(hex) == Passage::Stop {
            continue; // the move ends here
        }
        for next in map.neighbours(hex) {
            if passage(next) == Passage::Blocked {
                continue;
            }
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

    /// A unit of `side` with the cavalry's costs on `hex`.
    fn rider(map: &mut Map, side: Side, hex: Hex, movement: u32) -> crate::state::UnitId {
        let cost = map.acquire_move_cost(cav());
        let id = map.new_unit(side, "Rider", 0, 1, movement, cost);
        map.place_unit(id, hex).unwrap();
        id
    }

    #[test]
    fn others_units_are_attacked_and_others_cities_end_a_move() {
        let mut map = grass();
        let me = rider(&mut map, Side::Red, Hex::new(10, 5), 8);
        // A blue unit two hexes east, a red one north, nobody's city south.
        rider(&mut map, Side::Blue, Hex::new(10, 7), 8);
        rider(&mut map, Side::Red, Hex::new(8, 5), 8);
        let city = map.add_city("Tarn", Hex::new(12, 5), 0).unwrap();
        let unit = map.units().get(me).unwrap().clone();
        assert_eq!(passage(&map, Side::Red, Hex::new(10, 7)), Passage::Stop);
        assert!(hostile(&map, Side::Red, Hex::new(10, 7)) && !hostile(&map, Side::Red, Hex::new(8, 5)));
        assert_eq!(passage(&map, Side::Red, Hex::new(8, 5)), Passage::Open);
        assert_eq!(passage(&map, Side::Red, Hex::new(12, 5)), Passage::Stop);
        let range = compute_range(&map, &unit);
        // The blue unit's hex can be gone into (an attack), not through.
        assert_eq!(range.cost(Hex::new(10, 7)), Some(4));
        assert_eq!(range.cost(Hex::new(8, 5)), Some(4), "a red one's is open");
        assert_eq!(range.cost(Hex::new(6, 5)), Some(8), "and through it");
        // Into the city (grass, then the city's 1), but not through it: beyond
        // it is farther round.
        assert_eq!(range.cost(Hex::new(12, 5)), Some(3));
        assert!(range.cost(Hex::new(13, 5)).is_none_or(|c| c > 5), "{:?}", range.cost(Hex::new(13, 5)));
        // A city of its own side is open.
        map.city_mut(city).unwrap().side = Side::Red;
        assert_eq!(passage(&map, Side::Red, Hex::new(12, 5)), Passage::Open);
        let range = compute_range(&map, &unit);
        assert_eq!(range.cost(Hex::new(13, 5)), Some(5));
        // Someone in it of another side: an attack.
        rider(&mut map, Side::Blue, Hex::new(12, 5), 8);
        assert_eq!(passage(&map, Side::Red, Hex::new(12, 5)), Passage::Stop);
    }

    #[test]
    fn a_far_hex_is_gone_toward_as_far_as_the_points_go() {
        let mut map = grass();
        let me = rider(&mut map, Side::Red, Hex::new(10, 2), 6);
        let unit = map.units().get(me).unwrap().clone();
        // In reach: there.
        assert_eq!(step_toward(&map, &unit, Hex::new(10, 4)), Some((Hex::new(10, 4), 4)));
        // Far: three steps of grass along the way, closer than it was.
        let (hex, cost) = step_toward(&map, &unit, Hex::new(10, 15)).unwrap();
        assert_eq!(cost, 6);
        assert_eq!(hex.distance(Hex::new(10, 15)), Hex::new(10, 2).distance(Hex::new(10, 15)) - 3);
        // Round what is in the way.
        rider(&mut map, Side::Blue, Hex::new(10, 3), 6);
        let (hex, _) = step_toward(&map, &unit, Hex::new(10, 4)).unwrap();
        assert_ne!(hex, Hex::new(10, 3));
        // Toward another side's units in reach: into them, an attack.
        assert_eq!(step_toward(&map, &unit, Hex::new(10, 3)), Some((Hex::new(10, 3), 2)));
        // No way: a target beyond water it cannot cross.
        for row in 0..20 {
            map.tile_mut(Hex::new(row, 8)).unwrap().kind = TileType::Water;
        }
        assert_eq!(step_toward(&map, &unit, Hex::new(10, 15)), None);
        assert_eq!(step_toward(&map, &unit, Hex::new(10, 2)), None, "where it is");
        // Without points it goes nowhere.
        map.unit_mut(me).unwrap().movement = 1;
        let tired = map.units().get(me).unwrap().clone();
        assert_eq!(step_toward(&map, &tired, Hex::new(10, 6)), None);
    }
}
