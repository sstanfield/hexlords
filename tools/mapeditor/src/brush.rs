//! Which hexes a stroke of the editor touches: the brush's footprint, and the
//! region a fill floods. Pure functions of the map.

use hexgame::state::{Hex, Map};

/// The largest brush: three hexes across is radius 1, five is radius 2.
pub const MAX_RADIUS: u8 = 2;

/// The hexes at most `radius` steps from `center` that are on the map, the
/// center first. Radius 0 is the center alone, 1 adds its six neighbours (7
/// hexes), 2 the ring around those (19).
pub fn footprint(map: &Map, center: Hex, radius: u8) -> Vec<Hex> {
    if !map.contains(center) {
        return Vec::new();
    }
    let mut hexes = vec![center];
    let mut ring_start = 0;
    for _ in 0..radius {
        let ring_end = hexes.len();
        for i in ring_start..ring_end {
            for next in map.neighbours(hexes[i]) {
                if !hexes.contains(&next) {
                    hexes.push(next);
                }
            }
        }
        ring_start = ring_end;
    }
    hexes
}

/// The hexes a fill at `start` changes: `start` and every hex of the same
/// terrain that can be reached from it through hexes of that terrain.
pub fn region(map: &Map, start: Hex) -> Vec<Hex> {
    let Some(kind) = map.tile(start).map(|t| t.kind) else { return Vec::new() };
    let cols = usize::from(map.cols());
    let index = |hex: Hex| usize::from(hex.row) * cols + usize::from(hex.col);
    let mut seen = vec![false; usize::from(map.rows()) * cols];
    seen[index(start)] = true;
    let mut hexes = vec![start];
    let mut next = 0;
    while next < hexes.len() {
        let hex = hexes[next];
        next += 1;
        for neighbour in map.neighbours(hex) {
            let same = map.tile(neighbour).is_some_and(|t| t.kind == kind);
            if same && !std::mem::replace(&mut seen[index(neighbour)], true) {
                hexes.push(neighbour);
            }
        }
    }
    hexes
}

#[cfg(test)]
mod tests {
    use super::*;
    use hexgame::state::TileType;

    fn map() -> Map {
        Map::new(20, 20, TileType::Grass).unwrap()
    }

    fn paint(map: &mut Map, hexes: &[(u16, u16)], kind: TileType) {
        for &(row, col) in hexes {
            map.tile_mut(Hex::new(row, col)).unwrap().kind = kind;
        }
    }

    #[test]
    fn the_footprint_grows_by_rings() {
        let m = map();
        let center = Hex::new(10, 10);
        assert_eq!(footprint(&m, center, 0), [center]);
        let seven = footprint(&m, center, 1);
        assert_eq!(seven.len(), 7);
        assert_eq!(seven[0], center);
        assert!(seven[1..].iter().all(|h| h.is_adjacent(center)));
        let nineteen = footprint(&m, center, 2);
        assert_eq!(nineteen.len(), 19);
        assert_eq!(nineteen[..7], seven[..]);
        // No hex twice.
        let mut sorted = nineteen.clone();
        sorted.sort_by_key(|h| (h.row, h.col));
        sorted.dedup();
        assert_eq!(sorted.len(), 19);
    }

    #[test]
    fn the_footprint_stops_at_the_maps_edge() {
        let m = map();
        let corner = footprint(&m, Hex::new(0, 0), 2);
        assert!(corner.iter().all(|h| m.contains(*h)));
        assert!(corner.len() < 19 && corner.len() > 3);
        assert!(footprint(&m, Hex::new(20, 0), 1).is_empty(), "a center off the map paints nothing");
    }

    #[test]
    fn a_fill_takes_the_connected_hexes_of_one_terrain() {
        let mut m = map();
        // A wall of water down column 5 cuts the map in two.
        for row in 0..20 {
            paint(&mut m, &[(row, 5)], TileType::Water);
        }
        let left = region(&m, Hex::new(3, 2));
        assert_eq!(left.len(), 20 * 5);
        assert!(left.iter().all(|h| h.col < 5));
        let wall = region(&m, Hex::new(0, 5));
        assert_eq!(wall.len(), 20);
        let right = region(&m, Hex::new(19, 19));
        assert_eq!(right.len(), 20 * 14);
        // A gap in the wall joins the two sides.
        paint(&mut m, &[(7, 5)], TileType::Grass);
        assert_eq!(region(&m, Hex::new(3, 2)).len(), 20 * 19 + 1);
        assert!(region(&m, Hex::new(20, 20)).is_empty());
    }
}
