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

/// A side of a hex, and so the neighbour across it: clockwise from the top.
/// The hexes are flat-topped, so there is one straight up and one straight
/// down, and two on either hand.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum HexSide {
    North,
    NorthEast,
    SouthEast,
    South,
    SouthWest,
    NorthWest,
}

impl HexSide {
    /// Clockwise from the top.
    pub const ALL: [HexSide; 6] = [
        HexSide::North,
        HexSide::NorthEast,
        HexSide::SouthEast,
        HexSide::South,
        HexSide::SouthWest,
        HexSide::NorthWest,
    ];

    /// Its place in [`HexSide::ALL`], and its bit in a mask of sides.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The side a neighbour sees this one from.
    pub const fn opposite(self) -> HexSide {
        HexSide::ALL[(self.index() + 3) % 6]
    }
}

impl Hex {
    /// How many steps apart two hexes are, the fewest it takes to go from one
    /// to the other over the hexes in between (what is on them aside).
    pub fn distance(self, other: Hex) -> u32 {
        // As axial coordinates: the column, and the row moved up by half the
        // column (an odd column sits half a hex higher).
        let axial = |h: Hex| {
            let (r, c) = (i32::from(h.row), i32::from(h.col));
            (c, r - (c + (c & 1)) / 2)
        };
        let ((q1, r1), (q2, r2)) = (axial(self), axial(other));
        let (dq, dr) = (q1 - q2, r1 - r2);
        ((dq.abs() + dr.abs() + (dq + dr).abs()) / 2) as u32
    }

    /// The position across a side, as signed (row, col), on the map or not.
    /// An odd column sits half a hex higher, so its neighbours to the east
    /// and west are in its own row and the row above; an even column's are in
    /// its own row and the row below.
    pub fn toward(self, side: HexSide) -> (i32, i32) {
        let (r, c) = (i32::from(self.row), i32::from(self.col));
        // The row of the upper neighbour in the columns beside this one.
        let upper = if self.col % 2 == 1 { r - 1 } else { r };
        match side {
            HexSide::North => (r - 1, c),
            HexSide::NorthEast => (upper, c + 1),
            HexSide::SouthEast => (upper + 1, c + 1),
            HexSide::South => (r + 1, c),
            HexSide::SouthWest => (upper + 1, c - 1),
            HexSide::NorthWest => (upper, c - 1),
        }
    }
}

/// A step on the grid, as the arrow keys make them. Up and down change the row,
/// left and right the column (which, between an odd and an even column, also
/// moves half a hex up or down on screen).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
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

    #[test]
    fn the_six_sides_are_the_six_neighbours() {
        for hex in [Hex::new(5, 4), Hex::new(5, 5)] {
            let across: Vec<(i32, i32)> = HexSide::ALL.iter().map(|s| hex.toward(*s)).collect();
            let mut a = across.clone();
            let mut b = hex.neighbour_coords().to_vec();
            a.sort();
            b.sort();
            assert_eq!(a, b, "{hex}");
            // Going across a side and back across the opposite one is home.
            for side in HexSide::ALL {
                let (r, c) = hex.toward(side);
                let there = Hex::new(r as u16, c as u16);
                assert_eq!(there.toward(side.opposite()), (i32::from(hex.row), i32::from(hex.col)), "{hex} {side:?}");
            }
        }
        // An even column's eastern neighbours are in its row and the one below;
        // an odd column's in its row and the one above.
        assert_eq!(Hex::new(5, 4).toward(HexSide::NorthEast), (5, 5));
        assert_eq!(Hex::new(5, 4).toward(HexSide::SouthEast), (6, 5));
        assert_eq!(Hex::new(5, 5).toward(HexSide::NorthEast), (4, 6));
        assert_eq!(Hex::new(5, 5).toward(HexSide::SouthWest), (5, 4));
        assert_eq!(Hex::new(5, 5).toward(HexSide::North), (4, 5));
        assert_eq!(HexSide::NorthWest.opposite(), HexSide::SouthEast);
        assert_eq!(HexSide::ALL.map(HexSide::index), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_neighbour_is_one_step_and_steps_add_up() {
        for hex in [Hex::new(5, 4), Hex::new(5, 5), Hex::new(0, 0)] {
            assert_eq!(hex.distance(hex), 0);
            for side in HexSide::ALL {
                let (r, c) = hex.toward(side);
                if r < 0 || c < 0 {
                    continue;
                }
                assert_eq!(hex.distance(Hex::new(r as u16, c as u16)), 1, "{hex} {side:?}");
            }
        }
        // Straight down a column, and across columns, which zigzag.
        assert_eq!(Hex::new(2, 3).distance(Hex::new(9, 3)), 7);
        assert_eq!(Hex::new(5, 0).distance(Hex::new(5, 6)), 6);
        assert_eq!(Hex::new(5, 0).distance(Hex::new(2, 6)), 6);
        assert_eq!(Hex::new(5, 0).distance(Hex::new(1, 6)), 7);
        // Either way round.
        assert_eq!(Hex::new(11, 2).distance(Hex::new(3, 17)), Hex::new(3, 17).distance(Hex::new(11, 2)));
    }
}
