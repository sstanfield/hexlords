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

    /// Where it is `elapsed` ms after it set off, its whole way taking
    /// `length` ms: the hex it last left (an index into the way) and how
    /// far it is on to the next (0 to 1).
    fn at(&self, elapsed: u64, length: u64) -> (usize, f32) {
        let last = self.way.len().saturating_sub(1);
        if last == 0 || elapsed >= length {
            return (last, 0.0);
        }
        // (In thousandths of a step.)
        let along = elapsed * self.steps() * 1000 / length.max(1);
        ((along / 1000) as usize, (along % 1000) as f32 / 1000.0)
    }
}

/// How long a thing shown takes, in ms: a stack, its way; stacks going
/// together, as long as the one with the longest way, and every one of
/// them takes that long (in rounds, a beat of the round: who goes faster
/// gets farther in it); a battle, as long as its window says.
fn length_ms(shown: &Shown) -> u64 {
    goings(shown).iter().map(Going::length_ms).max().unwrap_or(0)
}

/// What is shown, one after the other.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Going(Going),
    /// Stacks going at the same time (in rounds: a beat of the round that
    /// plays out): they all take as long as the one that goes farthest, so
    /// the faster ones are seen to be faster.
    Together(Vec<Going>),
    /// The battle of the history's record: played out in its window.
    Battle(usize),
}

/// The ways of stacks going together, a way each.
pub type Ways = Vec<Vec<Hex>>;

/// What the screen shows now (`Showing::tick`).
#[derive(Clone, Debug, PartialEq)]
pub enum Now {
    /// Nothing.
    Idle,
    /// A stack going; the way, if it has just set off (for the screen to
    /// bring it into view).
    Going(Option<Vec<Hex>>),
    /// Stacks going together; their ways, if they have just set off: the
    /// person's own stacks', and the others'.
    Together(Option<(Ways, Ways)>),
    /// A battle, until the screen says it is over (`Showing::battle_over`);
    /// true the first time (to open its window).
    Battle(usize, bool),
}

/// The stacks a thing shown has going: one, several together, or none (a
/// battle).
fn goings(shown: &Shown) -> &[Going] {
    match shown {
        Shown::Going(going) => std::slice::from_ref(going),
        Shown::Together(all) => all,
        Shown::Battle(_) => &[],
    }
}

/// What happened in a beat of a round (rounds), stacks going and battles
/// in their order, as it is shown: the stacks that went between two
/// battles go at the same time, each all the steps it took (its way goes
/// on); units that took a step with others and then more by themselves
/// (two stacks that left a hex by the same step, one of them faster) are a
/// stack of their own from the start; and who went on in another stack
/// altogether goes after.
pub fn beats(shown: Vec<Shown>) -> Vec<Shown> {
    let mut beats: Vec<Shown> = Vec::new();
    for next in shown {
        let going = match next {
            Shown::Going(going) => going,
            other => {
                beats.push(other);
                continue;
            }
        };
        let Some(Shown::Together(all)) = beats.last_mut() else {
            beats.push(Shown::Together(vec![going]));
            continue;
        };
        let from = going.way.first().copied();
        let within = |g: &Going| g.way.last().copied() == from && going.units.iter().all(|u| g.units.contains(u));
        match all.iter().position(within) {
            Some(i) if all[i].units.len() == going.units.len() => {
                all[i].way.extend(going.way.iter().skip(1).copied());
            }
            Some(i) => {
                all[i].units.retain(|u| !going.units.contains(u));
                let way = all[i].way.iter().copied().chain(going.way.iter().skip(1).copied()).collect();
                all.push(Going { way, ..going });
            }
            None if all.iter().any(|g| g.units.iter().any(|u| going.units.contains(u))) => {
                beats.push(Shown::Together(vec![going]));
            }
            None => all.push(going),
        }
    }
    beats
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
    /// a run of moves shown).
    pub fn push(&mut self, shown: Shown, before: &Sight) {
        if self.before.is_none() {
            self.before = Some(before.clone());
            self.reached.clear();
        }
        self.queue.push_back(shown);
    }

    /// The moves still to be shown are where they went (the person gave an
    /// order); the battles to be shown are still to be.
    pub fn finish_moves(&mut self) {
        self.queue.retain(|shown| matches!(shown, Shown::Battle(_)));
        if matches!(self.current, Some((Shown::Going(_) | Shown::Together(_), _))) {
            self.current = None;
        }
        self.before = None;
        self.reached.clear();
    }

    /// True if `unit` is shown going, or is still to be.
    pub fn goes(&self, unit: UnitId) -> bool {
        let current = self.current.iter().map(|(shown, _)| shown);
        current.chain(&self.queue).flat_map(goings).any(|going| going.units.contains(&unit))
    }

    /// True while what the person is shown to know is not what they know
    /// (`sight`).
    pub fn has_sight(&self) -> bool {
        self.before.is_some()
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
                Some((Shown::Together(all), since)) => {
                    let length = all.iter().map(Going::length_ms).max().unwrap_or(0);
                    if now_ms.saturating_sub(*since) < length {
                        let ways = |own: bool| all.iter().filter(|g| g.own == own).map(|g| g.way.clone()).collect();
                        return Now::Together(started.then(|| (ways(true), ways(false))));
                    }
                    let own = all.iter().filter(|g| g.own).flat_map(|g| g.way.iter().copied());
                    self.reached.extend(own);
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
        let all = current.chain(self.queue.iter().map(|shown| (shown, None)));
        let each = all.flat_map(|(shown, since)| goings(shown).iter().map(move |g| (g, since, length_ms(shown))));
        for (going, since, length) in each {
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
            let (i, along) = since.map_or((0, 0.0), |since| going.at(now_ms.saturating_sub(since), length));
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
        if let Some((shown, since)) = &self.current {
            for going in goings(shown).iter().filter(|g| g.own) {
                let (i, _) = going.at(now_ms.saturating_sub(*since), length_ms(shown));
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

    /// Where the person's own stacks shown going are at `now_ms`: the one
    /// going, the hex it is on; those still to go, where they set off from.
    fn stands(&self, now_ms: u64) -> Vec<Hex> {
        let mut stands = Vec::new();
        let current = self.current.iter().map(|(shown, since)| (shown, Some(*since)));
        let all = current.chain(self.queue.iter().map(|shown| (shown, None)));
        let each = all.flat_map(|(shown, since)| goings(shown).iter().map(move |g| (g, since, length_ms(shown))));
        for (going, since, length) in each {
            if !going.own || going.way.is_empty() {
                continue;
            }
            let (i, along) = since.map_or((0, 0.0), |since| going.at(now_ms.saturating_sub(since), length));
            stands.push(going.way[i]);
            // (Between two hexes it sees from both.)
            if along > 0.0 {
                stands.extend(going.way.get(i + 1).copied());
            }
        }
        stands
    }

    /// What the person at the screen is shown to know at `now_ms`, `after`
    /// being what they know now: the land their stacks have come to see so
    /// far opens, and what a stack sees goes with it (the owner's word,
    /// 2026-11-22: "only see what the unit can see at the time and leave
    /// ghosts"): what it has passed is seen no longer. (`None`: as `after`,
    /// nothing being shown.)
    pub fn sight(&self, after: &Sight, now_ms: u64) -> Option<Sight> {
        let before = self.before.as_ref()?;
        let (opened, stands) = (self.opened(now_ms), self.stands(now_ms));
        let within = |hexes: &[Hex], hex: Hex| hexes.iter().any(|o| o.distance(hex) <= UNIT_SIGHT);
        Some(before.opening(after, &|hex| within(&opened, hex), &|hex| within(&stands, hex)))
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
    fn a_unit_goes_until_it_is_shown_there() {
        let mut showing = Showing::default();
        let g = going(&[(5, 5), (5, 6)]);
        let (unit, other) = (g.units[0], UnitId { side: Side::Red, index: 1, generation: 0 });
        assert!(!showing.goes(unit));
        showing.push(Shown::Going(g), &Sight::all());
        assert!(showing.goes(unit), "still to go");
        assert!(!showing.goes(other));
        showing.tick(1000);
        assert!(showing.goes(unit), "on its way");
        showing.tick(1000 + STEP_MS);
        assert!(!showing.goes(unit), "there");
    }

    /// In rounds, a tick: its stacks go at the same moment, the person's
    /// own and the others', and the land opens as the person's go.
    #[test]
    fn stacks_shown_together_go_at_the_same_moment() {
        let mut showing = Showing::default();
        let own = going(&[(5, 5), (5, 6)]);
        let other = Going {
            units: vec![UnitId { side: Side::Blue, index: 0, generation: 0 }],
            side: Side::Blue,
            way: vec![Hex::new(9, 9), Hex::new(9, 10)],
            own: false,
        };
        let (mine, theirs) = (own.units[0], other.units[0]);
        showing.push(Shown::Together(vec![other.clone(), own.clone()]), &Sight::all());
        showing.push(Shown::Battle(3), &Sight::all());
        assert!(showing.goes(mine) && showing.goes(theirs));
        // It sets off: the ways, the person's own apart.
        let ways = Some((vec![own.way.clone()], vec![other.way.clone()]));
        assert_eq!(showing.tick(1000), Now::Together(ways));
        assert_eq!(showing.tick(1000 + STEP_MS / 2), Now::Together(None), "both on their way");
        assert_eq!(showing.opened_count(1000 + STEP_MS / 2), 1);
        // Both there at once: the next thing.
        assert_eq!(showing.tick(1000 + STEP_MS), Now::Battle(3, true));
        assert!(!showing.goes(mine) && !showing.goes(theirs));
        assert_eq!(showing.opened_count(1000 + STEP_MS), 2, "the person's own way, opened");
        // An order of the person's cuts a tick short, as a move.
        showing.push(Shown::Together(vec![own]), &Sight::all());
        showing.finish_moves();
        assert!(!showing.goes(mine));
    }

    /// In rounds, a beat: the stacks that went in it all take as long as
    /// the one that went farthest, so the one that went three hexes is
    /// seen to go three times as fast as the one that went one.
    #[test]
    fn stacks_shown_together_take_the_same_time_over_their_ways() {
        let mut showing = Showing::default();
        let fast = going(&[(5, 5), (5, 6), (5, 7), (5, 8)]);
        let slow =
            Going { units: vec![UnitId { side: Side::Red, index: 1, generation: 0 }], ..going(&[(9, 5), (9, 6)]) };
        showing.push(Shown::Together(vec![fast.clone(), slow.clone()]), &Sight::all());
        showing.tick(1000);
        let length = 3 * STEP_MS;
        // A third of the way through: the fast one a hex on, the slow one a
        // third of its only step.
        assert_eq!(fast.at(length / 3, length), (1, 0.0));
        let (i, along) = slow.at(length / 3, length);
        assert!(i == 0 && (along - 1.0 / 3.0).abs() < 0.01, "{i} {along}");
        assert_eq!(showing.tick(1000 + length - 1), Now::Together(None), "both on their way to the end");
        assert_eq!(showing.opened_count(1000 + length - 1), 3 + 1, "the hexes they have left behind them");
        assert_eq!(showing.tick(1000 + length), Now::Idle);
    }

    /// Two stacks left a hex by the same step in a beat, and the faster
    /// took another: shown as one stack for the first step, the faster
    /// one's second step came after the beat, by itself (seen on the
    /// continent, 2026-11-22: riders and the garrison leaving a city by its
    /// road).
    #[test]
    fn a_beat_shows_each_stack_all_its_steps_at_the_same_time() {
        let unit = |index: u32| UnitId { side: Side::Red, index, generation: 0 };
        let hex = |(row, col): (u16, u16)| Hex::new(row, col);
        let going = |units: &[u32], way: &[(u16, u16)]| {
            Shown::Going(Going {
                units: units.iter().map(|i| unit(*i)).collect(),
                side: Side::Red,
                way: way.iter().copied().map(hex).collect(),
                own: true,
            })
        };
        // The rider (3) and the garrison (0, 1) take the same step, shown
        // as one; then the rider another; then a battle; then the garrison
        // a step, and the rider joins a stack (7) that goes on with it.
        let happened = vec![
            going(&[3, 0, 1], &[(13, 7), (13, 8)]),
            going(&[3], &[(13, 8), (14, 9)]),
            going(&[3], &[(14, 9), (15, 10)]),
            Shown::Battle(4),
            going(&[0, 1], &[(13, 8), (14, 9)]),
            going(&[3], &[(15, 10), (16, 11)]),
            going(&[3, 7], &[(16, 11), (17, 11)]),
        ];
        let shown = beats(happened);
        let together = |goings: Vec<Shown>| Shown::Together(goings.into_iter().flat_map(goings_of).collect());
        assert_eq!(
            shown,
            [
                together(vec![
                    going(&[0, 1], &[(13, 7), (13, 8)]),
                    going(&[3], &[(13, 7), (13, 8), (14, 9), (15, 10)])
                ]),
                Shown::Battle(4),
                together(vec![going(&[0, 1], &[(13, 8), (14, 9)]), going(&[3], &[(15, 10), (16, 11)])]),
                together(vec![going(&[3, 7], &[(16, 11), (17, 11)])]),
            ]
        );
    }

    fn goings_of(shown: Shown) -> Vec<Going> {
        goings(&shown).to_vec()
    }

    #[test]
    fn a_long_way_goes_faster() {
        let way: Vec<(u16, u16)> = (0..40).map(|c| (5, c)).collect();
        let g = going(&way);
        assert!(g.length_ms() <= WAY_MS);
        assert_eq!(going(&[(5, 5), (5, 6)]).length_ms(), STEP_MS);
    }
}
