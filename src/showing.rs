//! What the screen shows happening (the owner's word, 2026-11-21: "when
//! units move the player should see them moving and visibility opening up
//! ... not super slow"; the other sides' too, those the player can see,
//! with the map on the action; and the battles the player's side is in,
//! played out in their window). The game's state changes at once; this is
//! the screen's: the stacks that went, drawn going along their ways one
//! after the other, the battles in their turn among them, and, as the
//! person's own stacks go, the land they come to see.
//!
//! Pure: it is told the time, and says what is where. Not a port: the C++
//! had one unit and no turns.

use std::collections::VecDeque;

use crate::game::{Sight, UNIT_SIGHT};
use crate::render::Moving;
use crate::state::{Hex, Map, Side, UnitId};
use crate::view::MapView;

/// How long a stack takes from one hex to the next, in ms; and how long a
/// whole way takes at most (a long one goes faster).
pub const STEP_MS: u64 = 120;
pub const WAY_MS: u64 = 1500;

/// A stack that went: its units, its side, its way (the first hex where
/// it was, the last where it went), and whether it is the person's at the
/// screen (as it goes, they see what it sees).
#[derive(Clone, Debug, PartialEq)]
pub struct Going {
    pub units: Vec<UnitId>,
    pub side: Side,
    pub way: Vec<Hex>,
    pub own: bool,
}

impl Going {
    /// How long each step takes.
    fn step_ms(&self) -> u64 {
        let steps = self.steps().max(1);
        STEP_MS.min(WAY_MS / steps)
    }

    fn steps(&self) -> u64 {
        self.way.len().saturating_sub(1) as u64
    }

    fn length_ms(&self) -> u64 {
        self.steps() * self.step_ms()
    }

    /// Where it is `elapsed` ms after it set off: the hex it last left (an
    /// index into the way) and how far it is on to the next (0 to 1).
    fn at(&self, elapsed: u64) -> (usize, f32) {
        let step = self.step_ms().max(1);
        let i = (elapsed / step) as usize;
        if i >= self.way.len().saturating_sub(1) {
            return (self.way.len().saturating_sub(1), 0.0);
        }
        (i, (elapsed % step) as f32 / step as f32)
    }
}

/// What is shown, one after the other.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Going(Going),
    /// The battle of the history's record: played out in its window.
    Battle(usize),
}

/// What the screen shows now (`Showing::tick`).
#[derive(Clone, Debug, PartialEq)]
pub enum Now {
    /// Nothing.
    Idle,
    /// A stack going; the way, if it has just set off (for the screen to
    /// bring it into view).
    Going(Option<Vec<Hex>>),
    /// A battle, until the screen says it is over (`Showing::battle_over`);
    /// true the first time (to open its window).
    Battle(usize, bool),
}

/// The stacks and battles to be shown, and the one shown now.
#[derive(Clone, Debug, Default)]
pub struct Showing {
    queue: VecDeque<Shown>,
    /// What is shown now, and when it began.
    current: Option<(Shown, u64)>,
    /// What the person at the screen knew before what is shown happened;
    /// and the hexes their stacks shown so far have been to.
    before: Option<Sight>,
    reached: Vec<Hex>,
}

impl Showing {
    /// True with nothing to show.
    pub fn is_idle(&self) -> bool {
        self.current.is_none() && self.queue.is_empty()
    }

    /// Show `shown` after what is to be shown already. `before`: what the
    /// person at the screen knew before it happened (kept from the first of
    /// a run of things shown).
    pub fn push(&mut self, shown: Shown, before: &Sight) {
        if self.is_idle() {
            self.before = Some(before.clone());
            self.reached.clear();
        }
        self.queue.push_back(shown);
    }

    /// Show nothing more: everything is where it is.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.current = None;
        self.before = None;
        self.reached.clear();
    }

    /// What is shown at `now_ms`: a stack that has got where it went makes
    /// way for the next thing; a battle waits for `battle_over`.
    pub fn tick(&mut self, now_ms: u64) -> Now {
        let mut started = false;
        loop {
            match &self.current {
                None => match self.queue.pop_front() {
                    None => {
                        self.clear();
                        return Now::Idle;
                    }
                    Some(shown) => {
                        self.current = Some((shown, now_ms));
                        started = true;
                    }
                },
                Some((Shown::Going(going), since)) => {
                    if now_ms.saturating_sub(*since) < going.length_ms() {
                        return Now::Going(started.then(|| going.way.clone()));
                    }
                    if going.own {
                        self.reached.extend(going.way.iter().copied());
                    }
                    self.current = None;
                }
                Some((Shown::Battle(record), _)) => return Now::Battle(*record, started),
            }
        }
    }

    /// The battle shown now has been played out: on to the next thing.
    pub fn battle_over(&mut self) {
        if matches!(self.current, Some((Shown::Battle(_), _))) {
            self.current = None;
        }
    }

    /// The stacks shown going, at `now_ms`, as `view` shows them: the one
    /// going, where it is on its way; and those still to go, where they set
    /// off from. (A unit goes with the first of them it is in.)
    pub fn moving(&self, map: &Map, view: &MapView, now_ms: u64) -> Vec<Moving> {
        let mut drawn: Vec<UnitId> = Vec::new();
        let mut out = Vec::new();
        let current = self.current.iter().map(|(shown, since)| (shown, Some(*since)));
        for (shown, since) in current.chain(self.queue.iter().map(|shown| (shown, None))) {
            let Shown::Going(going) = shown else { continue };
            let units: Vec<UnitId> = going
                .units
                .iter()
                .copied()
                .filter(|id| !drawn.contains(id) && map.units().get(*id).is_some())
                .collect();
            if units.is_empty() || going.way.is_empty() {
                continue;
            }
            drawn.extend(units.iter().copied());
            let (i, along) = since.map_or((0, 0.0), |since| going.at(now_ms.saturating_sub(since)));
            let from = view.hex_center(going.way[i]);
            let next = going.way.get(i + 1).copied().unwrap_or(going.way[i]);
            let to = view.hex_center(next);
            let at = [from[0] + (to[0] - from[0]) * along, from[1] + (to[1] - from[1]) * along];
            let hex = if along < 0.5 { going.way[i] } else { next };
            out.push(Moving { units, at, hex });
        }
        out
    }

    /// The hexes the person's own stacks have been to so far, at `now_ms`.
    fn opened(&self, now_ms: u64) -> Vec<Hex> {
        let mut opened = self.reached.clone();
        if let Some((Shown::Going(going), since)) = &self.current {
            if going.own {
                let (i, _) = going.at(now_ms.saturating_sub(*since));
                opened.extend(going.way.iter().take(i + 1).copied());
            }
        }
        opened
    }

    /// How many hexes the person's stacks have been to (what is seen changes
    /// with it).
    pub fn opened_count(&self, now_ms: u64) -> usize {
        self.opened(now_ms).len()
    }

    /// What the person at the screen is shown to know at `now_ms`, `after`
    /// being what they know now: the land their stacks have come to see so
    /// far opens (`None`: as `after`, nothing being shown).
    pub fn sight(&self, after: &Sight, now_ms: u64) -> Option<Sight> {
        let before = self.before.as_ref()?;
        let opened = self.opened(now_ms);
        Some(before.opening(after, &|hex| opened.iter().any(|o| o.distance(hex) <= UNIT_SIGHT)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn going(way: &[(u16, u16)]) -> Going {
        let units = vec![UnitId { side: Side::Red, index: 0, generation: 0 }];
        Going { units, side: Side::Red, way: way.iter().map(|(r, c)| Hex::new(*r, *c)).collect(), own: true }
    }

    #[test]
    fn a_stack_goes_a_hex_at_a_time_then_the_next_thing_and_a_battle_waits() {
        let mut showing = Showing::default();
        let way = [(5, 5), (5, 6), (5, 7)];
        showing.push(Shown::Going(going(&way)), &Sight::all());
        showing.push(Shown::Battle(3), &Sight::all());
        let set_off = showing.tick(1000);
        assert_eq!(set_off, Now::Going(Some(going(&way).way)));
        assert_eq!(showing.tick(1000 + STEP_MS), Now::Going(None));
        assert_eq!(showing.opened_count(1000 + STEP_MS), 2, "two hexes reached");
        assert_eq!(showing.tick(1000 + 2 * STEP_MS), Now::Battle(3, true));
        assert_eq!(showing.tick(9000), Now::Battle(3, false), "until it is over");
        showing.battle_over();
        assert_eq!(showing.tick(9000), Now::Idle);
        assert!(showing.is_idle());
    }

    #[test]
    fn a_long_way_goes_faster() {
        let way: Vec<(u16, u16)> = (0..40).map(|c| (5, c)).collect();
        let g = going(&way);
        assert!(g.length_ms() <= WAY_MS);
        assert_eq!(going(&[(5, 5), (5, 6)]).length_ms(), STEP_MS);
    }
}
