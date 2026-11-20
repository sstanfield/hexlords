//! The hex grid as the generator and its measures see it: a value for each
//! hex, row-major; the hexes around one; flood fills, shortest ways, the
//! pieces a set of hexes falls into, and the quantile pick.
//!
//! Everything here breaks ties by the hex's index, so a seed always gives the
//! same map.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::state::Hex;

/// A value for every hex of a `rows` x `cols` map, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    rows: u16,
    cols: u16,
    cells: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn new(rows: u16, cols: u16, fill: T) -> Grid<T> {
        Grid { rows, cols, cells: vec![fill; usize::from(rows) * usize::from(cols)] }
    }

    /// A grid of what `f` says of each hex.
    pub fn from_fn(rows: u16, cols: u16, f: impl Fn(Hex) -> T) -> Grid<T> {
        let cells = (0..usize::from(rows) * usize::from(cols)).map(|i| f(hex_of(cols, i))).collect();
        Grid { rows, cols, cells }
    }
}

impl<T> Grid<T> {
    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// A hex's place in the cells.
    pub fn index(&self, hex: Hex) -> usize {
        usize::from(hex.row) * usize::from(self.cols) + usize::from(hex.col)
    }

    /// The hex at a place in the cells.
    pub fn hex(&self, i: usize) -> Hex {
        hex_of(self.cols, i)
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.row < self.rows && hex.col < self.cols
    }

    pub fn get(&self, hex: Hex) -> &T {
        &self.cells[self.index(hex)]
    }

    pub fn set(&mut self, hex: Hex, value: T) {
        let i = self.index(hex);
        self.cells[i] = value;
    }

    pub fn cells(&self) -> &[T] {
        &self.cells
    }

    pub fn cells_mut(&mut self) -> &mut [T] {
        &mut self.cells
    }

    /// Every hex, in reading order.
    pub fn hexes(&self) -> impl Iterator<Item = Hex> + use<T> {
        let cols = self.cols;
        (0..self.cells.len()).map(move |i| hex_of(cols, i))
    }

    /// The hexes around `hex` on the map (fewer at its edges).
    pub fn neighbours(&self, hex: Hex) -> impl Iterator<Item = Hex> {
        hex.neighbours(self.rows, self.cols)
    }

    /// True if `hex` is in the outermost ring of the map.
    pub fn on_edge(&self, hex: Hex) -> bool {
        hex.row == 0 || hex.col == 0 || hex.row + 1 == self.rows || hex.col + 1 == self.cols
    }

    /// How far `hex` is from the nearest edge of the map, in rows or columns.
    pub fn edge_distance(&self, hex: Hex) -> u16 {
        hex.row.min(hex.col).min(self.rows - 1 - hex.row).min(self.cols - 1 - hex.col)
    }
}

fn hex_of(cols: u16, i: usize) -> Hex {
    Hex::new((i / usize::from(cols)) as u16, (i % usize::from(cols)) as u16)
}

/// Steps from the nearest of `sources` to every hex that `open` lets a walk
/// go over (a source is always reached); `None` where none gets.
pub fn steps_from(
    rows: u16,
    cols: u16,
    sources: impl IntoIterator<Item = Hex>,
    open: impl Fn(Hex) -> bool,
) -> Grid<Option<u32>> {
    let mut dist: Grid<Option<u32>> = Grid::new(rows, cols, None);
    let mut queue = VecDeque::new();
    for s in sources {
        if dist.contains(s) && dist.get(s).is_none() {
            dist.set(s, Some(0));
            queue.push_back(s);
        }
    }
    while let Some(hex) = queue.pop_front() {
        let d = dist.get(hex).unwrap_or(0);
        for next in hex.neighbours(rows, cols) {
            if dist.get(next).is_none() && open(next) {
                dist.set(next, Some(d + 1));
                queue.push_back(next);
            }
        }
    }
    dist
}

/// The cheapest cost from the nearest of `sources` (each with a cost to
/// start at) to every hex, at the price `step` says a step from a hex to its
/// neighbour costs (`None`: it cannot be made); and which source that is.
pub fn cheapest_from(
    rows: u16,
    cols: u16,
    sources: &[(Hex, u32)],
    step: impl Fn(Hex, Hex) -> Option<u32>,
) -> (Grid<Option<u32>>, Grid<Option<usize>>) {
    let mut cost: Grid<Option<u32>> = Grid::new(rows, cols, None);
    let mut owner: Grid<Option<usize>> = Grid::new(rows, cols, None);
    let mut queue = BinaryHeap::new();
    for (i, (hex, start)) in sources.iter().enumerate() {
        if cost.contains(*hex) && cost.get(*hex).is_none_or(|c| *start < c) {
            cost.set(*hex, Some(*start));
            owner.set(*hex, Some(i));
            queue.push(Reverse((*start, cost.index(*hex), i)));
        }
    }
    while let Some(Reverse((spent, at, from))) = queue.pop() {
        let hex = cost.hex(at);
        if *cost.get(hex) != Some(spent) || *owner.get(hex) != Some(from) {
            continue;
        }
        for next in hex.neighbours(rows, cols) {
            let Some(price) = step(hex, next) else { continue };
            let total = spent + price;
            let better = match *cost.get(next) {
                None => true,
                Some(old) => total < old || (total == old && owner.get(next).is_some_and(|o| from < o)),
            };
            if better {
                cost.set(next, Some(total));
                owner.set(next, Some(from));
                queue.push(Reverse((total, cost.index(next), from)));
            }
        }
    }
    (cost, owner)
}

/// The cheapest way from any of `sources` to a hex `goal` says is one, at
/// the price `step` says a step into a hex costs (`None`: it cannot be
/// entered): the hexes after the source, the goal last. `None` if there is
/// none.
pub fn cheapest_path(
    rows: u16,
    cols: u16,
    sources: &[Hex],
    step: impl Fn(Hex) -> Option<u32>,
    goal: impl Fn(Hex) -> bool,
) -> Option<Vec<Hex>> {
    let mut cost: Grid<Option<u32>> = Grid::new(rows, cols, None);
    let mut from: Grid<Option<Hex>> = Grid::new(rows, cols, None);
    let mut queue = BinaryHeap::new();
    for s in sources {
        cost.set(*s, Some(0));
        queue.push(Reverse((0, cost.index(*s))));
    }
    while let Some(Reverse((spent, at))) = queue.pop() {
        let hex = cost.hex(at);
        if *cost.get(hex) != Some(spent) {
            continue;
        }
        if goal(hex) && !sources.contains(&hex) {
            let mut way = vec![hex];
            while let Some(prev) = *from.get(*way.last()?) {
                way.push(prev);
            }
            way.pop(); // the source
            way.reverse();
            return Some(way);
        }
        for next in hex.neighbours(rows, cols) {
            let Some(price) = step(next) else { continue };
            let total = spent + price;
            if cost.get(next).is_none_or(|old| total < old) {
                cost.set(next, Some(total));
                from.set(next, Some(hex));
                queue.push(Reverse((total, cost.index(next))));
            }
        }
    }
    None
}

/// The pieces the hexes `member` says are in fall into (six-way joined): each
/// hex's piece, and each piece's size, the pieces in the order of their first
/// hex.
pub fn pieces(rows: u16, cols: u16, member: impl Fn(Hex) -> bool) -> (Grid<Option<usize>>, Vec<usize>) {
    let mut piece: Grid<Option<usize>> = Grid::new(rows, cols, None);
    let mut sizes = Vec::new();
    for start in piece.hexes() {
        if piece.get(start).is_some() || !member(start) {
            continue;
        }
        let id = sizes.len();
        let mut size = 0;
        piece.set(start, Some(id));
        let mut stack = vec![start];
        while let Some(hex) = stack.pop() {
            size += 1;
            for next in hex.neighbours(rows, cols) {
                if piece.get(next).is_none() && member(next) {
                    piece.set(next, Some(id));
                    stack.push(next);
                }
            }
        }
        sizes.push(size);
    }
    (piece, sizes)
}

/// The `n` best of `scores` (each a place and its score), the highest first,
/// ties to the lower place: how every share of the map is hit exactly.
pub fn top(scores: &[(usize, f64)], n: usize) -> Vec<usize> {
    let mut sorted: Vec<(usize, f64)> = scores.to_vec();
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted.into_iter().take(n).map(|(i, _)| i).collect()
}

/// The middle value of a list of numbers (of the two in the middle, their
/// mean); 0 for none.
pub fn median(values: &[u32]) -> f64 {
    let mut v = values.to_vec();
    v.sort_unstable();
    match v.len() {
        0 => 0.0,
        n if n % 2 == 1 => f64::from(v[n / 2]),
        n => (f64::from(v[n / 2 - 1]) + f64::from(v[n / 2])) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_round_what_is_shut() {
        // A wall down column 5 but at row 0.
        let dist = steps_from(10, 10, [Hex::new(5, 2)], |h| h.col != 5 || h.row == 0);
        assert_eq!(*dist.get(Hex::new(5, 2)), Some(0));
        assert_eq!(*dist.get(Hex::new(5, 3)), Some(1));
        assert!(dist.get(Hex::new(5, 8)).unwrap() > Hex::new(5, 2).distance(Hex::new(5, 8)));
        assert_eq!(*dist.get(Hex::new(5, 5)), None);
    }

    #[test]
    fn the_cheapest_way_and_whose_it_is() {
        let (cost, owner) = cheapest_from(5, 9, &[(Hex::new(2, 0), 0), (Hex::new(2, 8), 0)], |_, to| {
            Some(if to.col == 4 { 10 } else { 1 })
        });
        assert_eq!(*cost.get(Hex::new(2, 3)), Some(3));
        assert_eq!((*owner.get(Hex::new(2, 1)), *owner.get(Hex::new(2, 7))), (Some(0), Some(1)));
        // Halfway, as dear from both: the first source's.
        assert_eq!(*owner.get(Hex::new(2, 4)), Some(0));
        assert_eq!(*cost.get(Hex::new(2, 4)), Some(13));
    }

    #[test]
    fn the_cheapest_path_goes_round_the_dear() {
        // From column 0 to column 4, dear on column 2 but at row 7.
        let way = cheapest_path(
            10,
            6,
            &[Hex::new(2, 0)],
            |h| Some(if h.col == 2 && h.row != 7 { 50 } else { 1 }),
            |h| h.col == 4,
        )
        .unwrap();
        assert!(way.contains(&Hex::new(7, 2)) && way.last().unwrap().col == 4);
        assert!(cheapest_path(10, 6, &[Hex::new(2, 0)], |h| (h.col != 2).then_some(1), |h| h.col == 4).is_none());
    }

    #[test]
    fn pieces_are_counted_and_sized() {
        let (piece, sizes) = pieces(6, 6, |h| h.col != 3);
        assert_eq!(sizes, [18, 12]);
        assert_ne!(piece.get(Hex::new(0, 0)), piece.get(Hex::new(0, 5)));
        assert_eq!(*piece.get(Hex::new(4, 3)), None);
    }

    #[test]
    fn the_top_scores_ties_to_the_lower_place() {
        assert_eq!(top(&[(4, 1.0), (2, 3.0), (1, 1.0), (7, 2.0)], 3), [2, 7, 1]);
        assert_eq!(median(&[5, 1, 3]), 3.0);
        assert_eq!(median(&[4, 1, 3, 2]), 2.5);
    }
}
