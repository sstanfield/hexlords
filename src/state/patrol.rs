//! A patrol (`Order::Patrol`): the places a stack goes round, turn after
//! turn, and whether it attacks what it meets on the way. Plain data: the
//! rules move it (`Game`, at the start of each of its side's turns).
//!
//! Not a port: the C++ had no orders.

use super::Hex;

/// A stack's patrol: where it goes, in order, and how far it has got.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Patrol {
    /// Its waypoints, in order: two at least, never the same twice in a row.
    pub stops: Vec<Hex>,
    /// After the last, on to the first (a loop); else back the way it came.
    pub round: bool,
    /// It attacks the other sides' stacks it sees and can reach that turn at
    /// these odds (in percent) or better; `None`: never.
    pub attack: Option<u32>,
    /// The waypoint it goes to next, and whether it goes back along them.
    pub next: usize,
    pub back: bool,
}

impl Patrol {
    /// The patrol round `waypoints`, in order, the first of them first: a
    /// loop if the last is the first again, else back and forth along them.
    /// Why not, if they make none: fewer than two places (the same place
    /// twice in a row counts once), or odds above 100.
    pub fn new(waypoints: &[Hex], attack: Option<u32>) -> Result<Patrol, String> {
        let mut stops = waypoints.to_vec();
        stops.dedup();
        let round = stops.len() > 2 && stops.first() == stops.last();
        if round {
            stops.pop();
        }
        if stops.len() < 2 {
            return Err("a patrol needs two places at least".to_string());
        }
        if attack.is_some_and(|odds| odds > 100) {
            return Err("the odds are 0 to 100 in a hundred".to_string());
        }
        Ok(Patrol { stops, round, attack, next: 0, back: false })
    }

    /// The waypoint it goes to now.
    pub fn target(&self) -> Hex {
        self.stops[self.next.min(self.stops.len() - 1)]
    }

    /// It got to its waypoint: on to the next (round to the first, or back
    /// the way it came at either end).
    pub fn advance(&mut self) {
        let last = self.stops.len() - 1;
        if self.round {
            self.next = if self.next >= last { 0 } else { self.next + 1 };
        } else if self.back {
            if self.next == 0 {
                (self.next, self.back) = (1, false);
            } else {
                self.next -= 1;
            }
        } else if self.next >= last {
            (self.next, self.back) = (last - 1, true);
        } else {
            self.next += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexes(n: u16) -> Vec<Hex> {
        (0..n).map(|i| Hex::new(i, i)).collect()
    }

    fn walk(patrol: &mut Patrol, steps: usize) -> Vec<u16> {
        (0..steps)
            .map(|_| {
                patrol.advance();
                patrol.target().row
            })
            .collect()
    }

    #[test]
    fn back_and_forth_or_round_and_round() {
        let mut there_and_back = Patrol::new(&hexes(3), None).unwrap();
        assert!(!there_and_back.round);
        assert_eq!(there_and_back.target().row, 0);
        assert_eq!(walk(&mut there_and_back, 6), [1, 2, 1, 0, 1, 2]);
        let mut looped = hexes(3);
        looped.push(Hex::new(0, 0));
        let mut round = Patrol::new(&looped, Some(65)).unwrap();
        assert_eq!((round.round, round.stops.len()), (true, 3));
        assert_eq!(walk(&mut round, 6), [1, 2, 0, 1, 2, 0]);
        let mut two = Patrol::new(&hexes(2), None).unwrap();
        assert_eq!(walk(&mut two, 4), [1, 0, 1, 0]);
    }

    #[test]
    fn a_patrol_needs_two_places() {
        let here = Hex::new(4, 4);
        assert!(Patrol::new(&[here], None).is_err());
        assert!(Patrol::new(&[here, here], None).is_err());
        assert!(Patrol::new(&[here, Hex::new(5, 5)], Some(101)).is_err());
        // The same place twice in a row counts once.
        let patrol = Patrol::new(&[here, here, Hex::new(5, 5), here], None).unwrap();
        assert_eq!(patrol.stops, [here, Hex::new(5, 5)]);
    }
}
