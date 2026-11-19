//! Battles: two stacks fight until one of them is gone. Our own rules, made
//! to be experimented with (`docs/gameplay.md`, "Fighting"): how much chance
//! there is (`Rules::chance`, from none at 0 to nothing but chance at 10) and
//! how a battle goes (`System`), both chosen when a game is set up.
//!
//! Pure: the fighters' strengths (every bonus already in them) and the game's
//! chance go in, who died comes out. Integers only, so that every machine
//! fights the same battle from the same seed.

use super::rng::Rng;
use super::terrain::TileType;
use super::units::UnitId;

/// How much chance there is unless the game says otherwise, and the most.
pub const DEFAULT_CHANCE: u8 = 5;
pub const MAX_CHANCE: u8 = 10;
/// How many hits a unit takes in a battle before it dies. (It is whole again
/// after the battle, if it lives.)
pub const HITS: u32 = 2;
/// A battle ends after this many exchanges whatever happens (it never needs
/// them: every exchange is a hit).
const MAX_EXCHANGES: u32 = 100_000;

/// How a battle goes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum System {
    /// Each stack stands in a line from its weakest to its strongest (its
    /// commander last); the two at the front fight; the loser of an exchange
    /// takes a hit; a unit out of hits dies and the next of its line steps up.
    Lines,
    /// Each exchange the two stacks' whole strengths are matched, and the
    /// loser takes the hit on one of its units drawn by chance: cheap units do
    /// not shield the dear ones behind them.
    Strength,
}

impl System {
    pub const ALL: [System; 2] = [System::Lines, System::Strength];

    /// Its name, in files and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            System::Lines => "lines",
            System::Strength => "strength",
        }
    }

    /// The system of a name.
    pub fn named(name: &str) -> Option<System> {
        System::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What a battle's strengths are counted in: quarters of a point, so that a
/// city's walls can count three quarters of a point a level.
pub const QUARTER: u32 = 4;
pub const WALL_QUARTERS: u32 = 3;

/// How much stronger the ground makes those who defend on it, in points:
/// hills and forests a little, mountains more. (A city's walls count
/// besides, [`WALL_QUARTERS`] a level.)
pub fn ground_bonus(kind: TileType) -> u32 {
    match kind {
        TileType::Forest | TileType::Hill => 1,
        TileType::Mountain => 2,
        _ => 0,
    }
}

/// What a game's battles are like.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Rules {
    /// 0: none, the stronger always wins; 10: strength counts for nothing.
    pub chance: u8,
    pub system: System,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules { chance: DEFAULT_CHANCE, system: System::Lines }
    }
}

/// A unit in a battle: who it is, how strong it fights here (with its bonuses
/// for the ground, the walls, its commander), and whether it is a commander
/// (which fights last).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Fighter {
    pub unit: UnitId,
    pub strength: u32,
    pub commander: bool,
}

/// How a battle went.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Battle {
    pub attackers_won: bool,
    /// The units that died, the attackers' and the defenders' together.
    pub dead: Vec<UnitId>,
    /// How many exchanges it took.
    pub exchanges: u32,
}

/// One exchange between strengths `a` (the attacker's) and `b`: true if `a`
/// wins it. `a` wins with the odds `a^k : b^k`, where `k` is `2 × (10 -
/// chance) / 5`: at 5, `k` is 2, and a strength twice another's wins four
/// times in five; the less chance, the more strength counts. At 0 the
/// stronger always wins (a tie is the defender's); at 10 it is even. Only
/// the ratio of the strengths counts, so they can be in any unit.
///
/// In integers, so that every machine fights the same battle: `k` is the
/// whole number below it or the one above, drawn in the share that makes
/// the odds come out as `k` says.
pub fn exchange(a: u32, b: u32, chance: u8, rng: &mut Rng) -> bool {
    let chance = chance.min(MAX_CHANCE);
    if chance == 0 {
        return a > b;
    }
    // k in fifths: 2 × (10 - chance).
    let fifths = 2 * u32::from(MAX_CHANCE - chance);
    let k = fifths / 5 + u32::from(rng.below(5) < u64::from(fifths % 5));
    // Small enough that the powers fit (the ratio is what counts).
    let (mut a, mut b) = (u64::from(a), u64::from(b));
    while a.max(b) > 1 << 15 {
        (a, b) = (a.div_ceil(2), b.div_ceil(2));
    }
    let (wa, wb) = (a.pow(k), b.pow(k));
    if wa + wb == 0 {
        return false;
    }
    rng.below(wa + wb) < wa
}

/// Fight a battle: `attackers` against `defenders`, until one side is gone.
/// A side with nobody in it has lost at once.
pub fn fight(attackers: &[Fighter], defenders: &[Fighter], rules: &Rules, rng: &mut Rng) -> Battle {
    if defenders.is_empty() || attackers.is_empty() {
        return Battle { attackers_won: defenders.is_empty() && !attackers.is_empty(), ..Battle::default() };
    }
    match rules.system {
        System::Lines => lines(attackers, defenders, rules.chance, rng),
        System::Strength => strength(attackers, defenders, rules.chance, rng),
    }
}

/// A fighter with the hits it has left.
struct Standing {
    fighter: Fighter,
    hits: u32,
}

/// A side in its line: the weakest first, its commander last; a side's own
/// order where they are as strong.
fn line(side: &[Fighter]) -> Vec<Standing> {
    let mut line: Vec<Standing> = side.iter().map(|f| Standing { fighter: *f, hits: HITS }).collect();
    line.sort_by_key(|s| (s.fighter.commander, s.fighter.strength));
    line
}

fn lines(attackers: &[Fighter], defenders: &[Fighter], chance: u8, rng: &mut Rng) -> Battle {
    let (mut a, mut d) = (line(attackers), line(defenders));
    let (mut i, mut j) = (0, 0);
    let mut battle = Battle::default();
    while i < a.len() && j < d.len() && battle.exchanges < MAX_EXCHANGES {
        battle.exchanges += 1;
        let (front, other) = if exchange(a[i].fighter.strength, d[j].fighter.strength, chance, rng) {
            (&mut d[j], &mut j)
        } else {
            (&mut a[i], &mut i)
        };
        front.hits -= 1;
        if front.hits == 0 {
            battle.dead.push(front.fighter.unit);
            *other += 1;
        }
    }
    battle.attackers_won = j == d.len();
    battle
}

fn strength(attackers: &[Fighter], defenders: &[Fighter], chance: u8, rng: &mut Rng) -> Battle {
    let mut a: Vec<Standing> = attackers.iter().map(|f| Standing { fighter: *f, hits: HITS }).collect();
    let mut d: Vec<Standing> = defenders.iter().map(|f| Standing { fighter: *f, hits: HITS }).collect();
    let mut battle = Battle::default();
    let total = |side: &[Standing]| side.iter().map(|s| s.fighter.strength).sum::<u32>();
    while !a.is_empty() && !d.is_empty() && battle.exchanges < MAX_EXCHANGES {
        battle.exchanges += 1;
        let loser = if exchange(total(&a), total(&d), chance, rng) { &mut d } else { &mut a };
        let hit = rng.below(loser.len() as u64) as usize;
        loser[hit].hits -= 1;
        if loser[hit].hits == 0 {
            battle.dead.push(loser.remove(hit).fighter.unit);
        }
    }
    battle.attackers_won = d.is_empty();
    battle
}

/// How likely the attackers are to win, in percent, from `runs` battles
/// fought with a chance of their own (seeded by `seed`): what a player is
/// told before an attack. The game's own chance is not touched.
pub fn odds(attackers: &[Fighter], defenders: &[Fighter], rules: &Rules, seed: u64, runs: u32) -> u32 {
    let runs = runs.max(1);
    let mut rng = Rng::new(seed);
    let won = (0..runs).filter(|_| fight(attackers, defenders, rules, &mut rng).attackers_won).count() as u32;
    (won * 100 + runs / 2) / runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Side;

    fn fighters(side: Side, strengths: &[u32]) -> Vec<Fighter> {
        strengths
            .iter()
            .enumerate()
            .map(|(i, s)| Fighter {
                unit: UnitId { side, index: i as u32, generation: 0 },
                strength: *s,
                commander: false,
            })
            .collect()
    }

    /// How often `a` beats `b` in one exchange, in percent, over many tries.
    fn share(a: u32, b: u32, chance: u8) -> u32 {
        let mut rng = Rng::new(9);
        let won = (0..4000).filter(|_| exchange(a, b, chance, &mut rng)).count() as u32;
        won / 40
    }

    #[test]
    fn without_chance_the_stronger_always_wins_and_with_all_of_it_either_may() {
        // None: strength decides, and a tie is the defender's.
        assert_eq!(share(4, 2, 0), 100);
        assert_eq!(share(2, 4, 0), 0);
        assert_eq!(share(3, 3, 0), 0);
        // All: about even, whatever the strengths.
        assert!((45..=55).contains(&share(8, 1, 10)), "{}", share(8, 1, 10));
        // The middle: the stronger mostly, not always; the more chance, the
        // less strength counts.
        let at = |chance| share(4, 2, chance);
        // At 5, twice as strong wins four times in five (16 : 4).
        assert!((76..=84).contains(&at(5)), "{}", at(5));
        assert!(
            at(3) >= at(5) && at(5) > at(7) && at(7) > at(9) && at(9) > 50,
            "{} {} {} {}",
            at(3),
            at(5),
            at(7),
            at(9)
        );
        // Two near strengths are near even (16 : 9), and the scale does not
        // matter: only the ratio does.
        assert!((60..=68).contains(&share(4, 3, 5)), "{}", share(4, 3, 5));
        assert!((60..=68).contains(&share(40, 30, 5)), "{}", share(40, 30, 5));
        // At chance 1, k is 3.6: 4 against 3 about 74 in 100; and strengths
        // too big for the powers are brought down, keeping the ratio.
        assert!((70..=78).contains(&share(4, 3, 1)), "{}", share(4, 3, 1));
        assert!((70..=78).contains(&share(4_000_000, 3_000_000, 1)), "{}", share(4_000_000, 3_000_000, 1));
    }

    #[test]
    fn a_seed_always_fights_the_same_battle() {
        let (a, d) = (fighters(Side::Red, &[2, 2, 4, 3]), fighters(Side::Blue, &[3, 3, 2]));
        for system in System::ALL {
            let rules = Rules { chance: 5, system };
            let one = fight(&a, &d, &rules, &mut Rng::new(42));
            assert_eq!(one, fight(&a, &d, &rules, &mut Rng::new(42)), "{system:?}");
            // One side is gone, the other has someone left.
            let dead = |side: Side| one.dead.iter().filter(|u| u.side == side).count();
            if one.attackers_won {
                assert!(dead(Side::Blue) == 3 && dead(Side::Red) < 4, "{one:?}");
            } else {
                assert!(dead(Side::Red) == 4 && dead(Side::Blue) < 3, "{one:?}");
            }
            assert!(one.exchanges >= 6, "every one of the losers' hits is an exchange");
        }
    }

    #[test]
    fn in_lines_the_weakest_go_first_and_the_commander_last() {
        // No chance: the attackers' 5 beats every defender in turn, so the
        // defenders die weakest first, and the attackers lose nobody.
        let a = fighters(Side::Red, &[5]);
        let mut d = fighters(Side::Blue, &[4, 1, 3]);
        d[0].commander = true;
        let battle = fight(&a, &d, &Rules { chance: 0, system: System::Lines }, &mut Rng::new(1));
        assert!(battle.attackers_won);
        let order: Vec<u32> = battle.dead.iter().map(|u| u.index).collect();
        assert_eq!(order, [1, 2, 0], "strength 1, then 3, then the commander");
        assert_eq!(battle.exchanges, 6, "two hits each");
        // A strong unit wears down: three of 3 against a 4 lose nobody but
        // the 4... at no chance, the 4 wins every exchange.
        let battle = fight(
            &fighters(Side::Red, &[3, 3, 3]),
            &fighters(Side::Blue, &[4]),
            &Rules { chance: 0, system: System::Lines },
            &mut Rng::new(1),
        );
        assert!(!battle.attackers_won && battle.dead.len() == 3);
    }

    #[test]
    fn by_strength_numbers_count_and_the_hits_fall_anywhere() {
        // Many weak against one strong: in lines the strong one wins at no
        // chance; by strength the many do, their total being greater.
        let many = fighters(Side::Red, &[1, 1, 1, 1, 1]);
        let one = fighters(Side::Blue, &[4]);
        let rules = |system| Rules { chance: 0, system };
        assert!(!fight(&many, &one, &rules(System::Lines), &mut Rng::new(1)).attackers_won);
        assert!(fight(&many, &one, &rules(System::Strength), &mut Rng::new(1)).attackers_won);
        // A dear unit behind cheap ones is hit as often as they are.
        let a = fighters(Side::Red, &[9]);
        let d = fighters(Side::Blue, &[1, 1, 1, 9]);
        let (mut hit_first, mut any) = (0, 0);
        for seed in 0..1000 {
            let battle = fight(&d, &a, &Rules { chance: 10, system: System::Strength }, &mut Rng::new(seed));
            if let Some(first) = battle.dead.iter().find(|u| u.side == Side::Blue) {
                any += 1;
                hit_first += u32::from(first.index == 3);
            }
        }
        // Of the four, it is the first to die about a quarter of the time.
        let share = hit_first * 100 / any.max(1);
        assert!(any > 100 && (15..=35).contains(&share), "the 9 died first {hit_first} times of {any}");
    }

    #[test]
    fn the_odds_follow_the_strengths_and_leave_the_game_alone() {
        let rules = Rules::default();
        let strong = fighters(Side::Red, &[4, 4, 4]);
        let weak = fighters(Side::Blue, &[2, 2]);
        let good = odds(&strong, &weak, &rules, 5, 200);
        let bad = odds(&weak, &strong, &rules, 5, 200);
        assert!(good > 90 && bad < 10, "{good} {bad}");
        assert_eq!(odds(&strong, &[], &rules, 5, 10), 100);
        assert_eq!(odds(&[], &weak, &rules, 5, 10), 0);
        // The same seed, the same answer.
        assert_eq!(odds(&strong, &weak, &rules, 5, 200), good);
    }

    #[test]
    fn systems_have_names() {
        for system in System::ALL {
            assert_eq!(System::named(system.name()), Some(system));
        }
        assert_eq!(System::named("dice"), None);
        assert_eq!(Rules::default(), Rules { chance: 5, system: System::Lines });
    }
}
