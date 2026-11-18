//! Hex coordinates and neighbours.
//!
//! The map is made of flat-topped hexes in offset columns ("odd-q"): odd columns
//! sit half a hex higher on screen than even ones, and rows grow downward. So the
//! hex above (row, col) is (row - 1, col), and the two hexes beside it in a
//! neighbouring column are at `row` and at `row - 1` (odd columns) or `row + 1`
//! (even columns).
//!
//! C++: src/state/mapmove.cpp (addCloseToStack, genEntryCost: the neighbour offsets)

/// A hex on the map.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Hex {
    pub row: u16,
    pub col: u16,
}

impl Hex {
    pub const fn new(row: u16, col: u16) -> Hex {
        Hex { row, col }
    }

    /// The hex at signed coordinates, if it is on a `rows` x `cols` map.
    pub fn on_map(row: i32, col: i32, rows: u16, cols: u16) -> Option<Hex> {
        if row >= 0 && row < i32::from(rows) && col >= 0 && col < i32::from(cols) {
            Some(Hex::new(row as u16, col as u16))
        } else {
            None
        }
    }

    /// The six neighbour positions as signed (row, col), on the map or not, in the
    /// order the C++ visits them: up, down, left, right, then the two diagonals.
    pub fn neighbour_coords(self) -> [(i32, i32); 6] {
        let (r, c) = (i32::from(self.row), i32::from(self.col));
        let d = if self.col % 2 == 1 { -1 } else { 1 };
        [(r - 1, c), (r + 1, c), (r, c - 1), (r, c + 1), (r + d, c - 1), (r + d, c + 1)]
    }

    /// The neighbours that are on a `rows` x `cols` map.
    pub fn neighbours(self, rows: u16, cols: u16) -> impl Iterator<Item = Hex> {
        self.neighbour_coords().into_iter().filter_map(move |(r, c)| Hex::on_map(r, c, rows, cols))
    }

    /// True if `other` is one of this hex's six neighbours.
    pub fn is_adjacent(self, other: Hex) -> bool {
        self.neighbour_coords().contains(&(i32::from(other.row), i32::from(other.col)))
    }
}

impl core::fmt::Display for Hex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.row, self.col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<Hex>) -> Vec<Hex> {
        v.sort();
        v
    }

    fn hexes(list: &[(u16, u16)]) -> Vec<Hex> {
        sorted(list.iter().map(|&(r, c)| Hex::new(r, c)).collect())
    }

    #[test]
    fn neighbours_of_an_even_column() {
        // Even columns sit lower: their side neighbours are at `row` and `row + 1`.
        let got = sorted(Hex::new(5, 4).neighbours(20, 20).collect());
        assert_eq!(got, hexes(&[(4, 4), (6, 4), (5, 3), (5, 5), (6, 3), (6, 5)]));
    }

    #[test]
    fn neighbours_of_an_odd_column() {
        // Odd columns sit higher: their side neighbours are at `row` and `row - 1`.
        let got = sorted(Hex::new(5, 3).neighbours(20, 20).collect());
        assert_eq!(got, hexes(&[(4, 3), (6, 3), (5, 2), (5, 4), (4, 2), (4, 4)]));
    }

    #[test]
    fn neighbours_at_the_edges() {
        // Top left corner (an even column).
        assert_eq!(sorted(Hex::new(0, 0).neighbours(20, 20).collect()), hexes(&[(1, 0), (0, 1), (1, 1)]));
        // Top row, odd column: nothing above, and the diagonals would be at row -1.
        assert_eq!(sorted(Hex::new(0, 1).neighbours(20, 20).collect()), hexes(&[(1, 1), (0, 0), (0, 2)]));
        // Bottom right corner of a 20 x 20 map (column 19 is odd).
        assert_eq!(sorted(Hex::new(19, 19).neighbours(20, 20).collect()), hexes(&[(18, 19), (19, 18), (18, 18)]));
        // Bottom row, even column: the diagonals would be at row 20.
        assert_eq!(sorted(Hex::new(19, 2).neighbours(20, 20).collect()), hexes(&[(18, 2), (19, 1), (19, 3)]));
    }

    #[test]
    fn adjacency_is_symmetric() {
        for row in 0..6u16 {
            for col in 0..6u16 {
                let h = Hex::new(row, col);
                for n in h.neighbours(6, 6) {
                    assert!(n.is_adjacent(h), "{h} -> {n} but not back");
                    assert!(h.is_adjacent(n));
                }
                assert!(!h.is_adjacent(h));
            }
        }
    }

    #[test]
    fn on_map_checks_both_axes() {
        assert_eq!(Hex::on_map(0, 0, 20, 30), Some(Hex::new(0, 0)));
        assert_eq!(Hex::on_map(19, 29, 20, 30), Some(Hex::new(19, 29)));
        assert_eq!(Hex::on_map(20, 0, 20, 30), None);
        assert_eq!(Hex::on_map(0, 30, 20, 30), None);
        assert_eq!(Hex::on_map(-1, 0, 20, 30), None);
        assert_eq!(Hex::on_map(0, -1, 20, 30), None);
    }
}
