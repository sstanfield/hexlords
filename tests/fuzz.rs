//! A hunt by chance: games played by random orders (moves the range
//! offers, waypoints, marches by stops, patrols, marauders, undo, cities'
//! work, rally points), on the continent and on random maps, in turns and
//! in rounds, with and without fog; after every order the rules' promises
//! are held to:
//!
//! - nothing panics, and a round ends;
//! - a hex holds one side's units, a stack's worth and a commander at most;
//! - a hex the range offers (`Game::range`, also sent on from a plan's end,
//!   `Game::select_onward`) is taken, or refused only for room;
//! - units of one hex given one order to go are on one hex after it, and
//!   when their side's turn (or the round) is over, unless they fought (the
//!   owner's word, 2026-11-22: "a mixed stack should always stay and move
//!   together");
//! - the game saved (`save::text`) and loaded is the same game.
//!
//! A few games each time the tests run; many with `FUZZ_N=300 cargo test
//! --release --test fuzz` (`FUZZ_FROM`: the first seed).

use hexlords::agents::order_text;
use hexlords::game::Game;
use hexlords::orders::{Event, Order, RallyPoint, Step};
use std::sync::atomic::{AtomicUsize, Ordering};

static ONWARD: AtomicUsize = AtomicUsize::new(0);
static MIXED: AtomicUsize = AtomicUsize::new(0);
static BATTLES: AtomicUsize = AtomicUsize::new(0);
static GROUPS: AtomicUsize = AtomicUsize::new(0);

/// True if the units go at different tables (flyers with walkers...).
fn mixed(game: &Game, ids: &[UnitId]) -> bool {
    let tables: Vec<_> = ids.iter().filter_map(|id| game.map.units().get(*id)).map(|u| u.move_cost).collect();
    tables.windows(2).any(|w| w[0] != w[1])
}

/// The living of `ids` are on one hex.
fn together(game: &Game, ids: &[UnitId]) -> bool {
    let at: Vec<Hex> = ids.iter().filter_map(|id| game.map.units().get(*id)).map(|u| u.pos()).collect();
    at.windows(2).all(|w| w[0] == w[1])
}

fn fought(events: &[Event], side: Side) -> bool {
    events.iter().any(|e| match e {
        Event::Battle { attacker, defender, .. } => *attacker == side || *defender == side,
        Event::Surprise { side: s, .. } | Event::Drowned { side: s, .. } => *s == side,
        _ => false,
    })
}
use hexlords::save;
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::{Hex, MAX_STACK, Side, UnitId};
use std::path::Path;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

/// A random map (small), as the new game screen makes one.
fn random_map(seed: u64) -> Game {
    use hexlords::mapgen::config::{Config, Size};
    let units = std::fs::read_to_string("assets/mapgen/units.txt").unwrap();
    let config = Config { seed, size: Some(Size::Named("small")), ..Config::default() };
    let made = hexlords::mapgen::generate(&config, &units, "mapgen/units.txt").unwrap();
    let map = made.map().unwrap();
    let source = hexlords::state::MapSource {
        name: "random".to_string(),
        terrain: made.terrain_text(),
        units: Some(units.clone()),
        cities: Some(made.cities_text()),
    };
    Game::from_scenario(hexlords::state::Scenario { map, assets: None, warnings: Vec::new() }).with_source(source)
}

fn start(mode: Mode, fog: Fog, seed: u64, random: bool) -> Game {
    let game = if random { random_map(seed) } else { Game::load(Path::new("assets/maps/continent")).unwrap().0 };
    let mut setup = Setup::solo(&game.map, None, "passive");
    let sides: Vec<String> = Setup::sides_of(&game.map)
        .iter()
        .map(|s| format!("{}=passive", hexlords::state::scenario::side_name(*s)))
        .collect();
    setup.set_players(&sides.join(",")).unwrap();
    (setup.mode, setup.fog, setup.seed, setup.gold) = (mode, fog, seed, 400);
    setup.combat.wounds = seed % 2 == 0;
    let mut game = game.with_setup(setup);
    game.record(true);
    game
}

fn check(game: &Game, log: &[String], what: &str) {
    let map = &game.map;
    for unit in map.units().iter() {
        if !map.contains(unit.pos()) {
            continue;
        }
        let here = map.units_at(unit.pos());
        assert!(here.contains(&unit.id()), "{what}: a unit not on its hex's list\n{}", log.join("\n"));
        let sides: Vec<Side> = here.iter().filter_map(|id| map.units().get(*id)).map(|u| u.side()).collect();
        assert!(sides.iter().all(|s| *s == sides[0]), "{what}: two sides on {:?}\n{}", unit.pos(), log.join("\n"));
        let leaders = here.iter().filter(|id| map.is_commander(**id)).count();
        assert!(
            leaders <= 1 && here.len() - leaders <= MAX_STACK,
            "{what}: too many on {:?}: {} with {leaders} commanders\n{}",
            unit.pos(),
            here.len(),
            log.join("\n")
        );
    }
}

/// Refusals that are fine for a hex the range offers.
fn allowed(why: &str) -> bool {
    ["no room", "attack together", "in the water", "not aboard", "the ships carry"].iter().any(|w| why.contains(w))
}

fn random_hex(rng: &mut Rng, game: &Game, near: Hex, far: usize) -> Hex {
    let (rows, cols) = (game.map.rows() as i64, game.map.cols() as i64);
    let d = far as i64;
    let row = (near.row as i64 + rng.below(2 * far + 1) as i64 - d).clamp(0, rows - 1);
    let col = (near.col as i64 + rng.below(2 * far + 1) as i64 - d).clamp(0, cols - 1);
    Hex::new(row as u16, col as u16)
}

fn fuzz(seed: u64, mode: Mode, random: bool, rounds: u32) {
    let fog = [Fog::Off, Fog::On, Fog::Total][(seed % 3) as usize];
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut game = start(mode, fog, seed, random);
    let mut steps: Vec<Step> = Vec::new();
    let mut log: Vec<String> = Vec::new();
    let note = |log: &mut Vec<String>, line: String| {
        log.push(line);
        if log.len() > 40 {
            log.remove(0);
        }
    };
    let mut turns = 0;
    let mut groups: Vec<(Side, Vec<UnitId>)> = Vec::new();
    while game.winner().is_none() && game.turn() <= rounds {
        let side = game.player();
        let orders = 4 + rng.below(10);
        for _ in 0..orders {
            let stacks = game.stacks_of(side);
            if stacks.is_empty() {
                break;
            }
            let at = stacks[rng.below(stacks.len())];
            let mut ids: Vec<UnitId> = game.map.units_at(at).to_vec();
            if rng.chance(30) && ids.len() > 1 {
                let keep = 1 + rng.below(ids.len() - 1);
                while ids.len() > keep {
                    ids.remove(rng.below(ids.len()));
                }
            }
            let pick = rng.below(100);
            let order = if pick < 35 {
                // A move to a hex the range offers: it has to be taken.
                game.deselect();
                game.select_unit(ids[0]);
                for id in &ids[1..] {
                    game.toggle_unit(*id);
                }
                let ids = game.selection().to_vec();
                let reach: Vec<Hex> = game.range().map_or(Vec::new(), |r| r.reachable().map(|(h, _)| h).collect());
                game.deselect();
                if reach.is_empty() || ids.is_empty() {
                    continue;
                }
                let to = reach[rng.below(reach.len())];
                let order = Order::Move { units: ids.clone(), to };
                let mut trial = game.clone();
                if let Err(why) = trial.apply(order.clone()) {
                    assert!(
                        allowed(&why),
                        "seed {seed} {mode:?}: the range offers {to:?} to {ids:?} at {at:?}, refused: {why}\n{}",
                        log.join("\n")
                    );
                }
                order
            } else if pick < 47 {
                // Onward, from a plan's end: what its range offers has to be taken.
                if game.select_onward(&ids) {
                    let reach: Vec<Hex> = game.range().map_or(Vec::new(), |r| r.reachable().map(|(h, _)| h).collect());
                    game.deselect();
                    let to = reach[rng.below(reach.len())];
                    let order = Order::Onward { units: ids.clone(), to };
                    let mut trial = game.clone();
                    if let Err(why) = trial.apply(order.clone()) {
                        assert!(
                            allowed(&why),
                            "seed {seed} {mode:?}: onward offers {to:?} to {ids:?} at {at:?}, refused: {why}\n{}",
                            log.join("\n")
                        );
                    }
                    order
                } else {
                    Order::Onward { units: ids.clone(), to: random_hex(&mut rng, &game, at, 4) }
                }
            } else if pick < 57 {
                Order::Toward { units: ids.clone(), to: random_hex(&mut rng, &game, at, 12) }
            } else if pick < 68 {
                let via = (0..rng.below(3)).map(|_| random_hex(&mut rng, &game, at, 8)).collect();
                Order::GoTo { units: ids.clone(), to: random_hex(&mut rng, &game, at, 20), via }
            } else if pick < 73 {
                let waypoints = (0..1 + rng.below(3)).map(|_| random_hex(&mut rng, &game, at, 6)).collect();
                Order::Patrol { units: ids.clone(), waypoints, attack: rng.chance(60).then_some(40) }
            } else if pick < 77 {
                Order::Defend { units: ids.clone(), on: rng.chance(60) }
            } else if pick < 80 {
                Order::Done { units: ids.clone() }
            } else if pick < 83 {
                Order::Maraud { units: ids.clone(), on: rng.chance(70) }
            } else if pick < 87 {
                Order::Undo
            } else if pick < 89 {
                Order::Bless { units: ids.clone() }
            } else if pick < 90 {
                Order::Disband { units: vec![ids[0]] }
            } else if pick < 93 {
                let n = rng.below(10) as u8;
                let point = rng
                    .chance(80)
                    .then(|| RallyPoint { at: random_hex(&mut rng, &game, at, 6), defend: rng.chance(50) });
                Order::RallyPoint { n, point }
            } else {
                let cities: Vec<_> =
                    game.map.cities().iter().filter(|c| c.side == side).map(|c| (c.id, c.units.len())).collect();
                if cities.is_empty() {
                    continue;
                }
                let (city, kinds) = cities[rng.below(cities.len())];
                Order::Build { city, what: (kinds > 0).then(|| rng.below(kinds)) }
            };
            let text = order_text(&game, &order);
            let moves = match &order {
                Order::Move { units, .. }
                | Order::Onward { units, .. }
                | Order::Toward { units, .. }
                | Order::GoTo { units, .. } => Some(units.clone()),
                _ => None,
            };
            let was_mixed = moves.as_ref().is_some_and(|ids| mixed(&game, ids));
            let onward = matches!(order, Order::Onward { .. });
            let done = game.apply(order);
            note(
                &mut log,
                format!("r{} {side:?}: {text} -> {}", game.turn(), done.as_ref().err().cloned().unwrap_or_default()),
            );
            check(&game, &log, &format!("seed {seed} {mode:?} after {text}"));
            if let (Ok(events), Some(ids)) = (&done, &moves) {
                if onward {
                    ONWARD.fetch_add(1, Ordering::Relaxed);
                }
                if was_mixed {
                    MIXED.fetch_add(1, Ordering::Relaxed);
                }
                BATTLES
                    .fetch_add(events.iter().filter(|e| matches!(e, Event::Battle { .. })).count(), Ordering::Relaxed);
                // One order from one hex: whoever of them lives is on one hex
                // still, unless they fought.
                assert!(
                    fought(events, side) || together(&game, ids),
                    "seed {seed} {mode:?}: {text} split the stack\n{}",
                    log.join("\n")
                );
                // The others' orders for these units are theirs no more.
                for (_, group) in groups.iter_mut() {
                    group.retain(|id| !ids.contains(id));
                }
                groups.push((side, ids.clone()));
            } else if done.is_ok() {
                // (Any other order may end theirs: not followed.)
                groups.retain(|(s, _)| *s != side);
            }
        }
        note(&mut log, format!("r{} {side:?}: end", game.turn()));
        let mut happened = game.apply(Order::EndTurn).unwrap();
        let mut beats = 0;
        let played = game.playing_out();
        while game.playing_out() {
            happened.extend(game.play_out());
            beats += 1;
            assert!(beats < 100_000, "seed {seed}: the round never ends\n{}", log.join("\n"));
            check(&game, &log, &format!("seed {seed} {mode:?} in a beat"));
        }
        check(&game, &log, &format!("seed {seed} {mode:?} after the turn"));
        // The stacks ordered as one that nobody fought: still one (those
        // of them on the same order still).
        if mode == Mode::Turns || played {
            for (whose, ids) in &groups {
                if mode == Mode::Turns && *whose != side {
                    continue;
                }
                if fought(&happened, *whose)
                    || happened.iter().any(|e| matches!(e, Event::Stalled { .. } | Event::Blessed { .. }))
                {
                    continue;
                }
                GROUPS.fetch_add(1, Ordering::Relaxed);
                assert!(
                    together(&game, ids),
                    "seed {seed} {mode:?} r{}: a stack of {whose:?} ordered as one came apart: {:?}\n{}",
                    game.turn(),
                    ids.iter()
                        .filter_map(|id| game.map.units().get(*id))
                        .map(|u| (u.name.clone(), u.pos(), u.route.clone()))
                        .collect::<Vec<_>>(),
                    log.join("\n")
                );
            }
            if mode == Mode::Turns {
                groups.retain(|(whose, _)| *whose != side);
            } else {
                groups.clear();
            }
        }
        steps.extend(game.take_steps());
        turns += 1;
        if turns % 23 == 0 || game.winner().is_some() {
            let text = save::text(&game, &steps, &[], 0).expect("a text");
            match save::load(&text) {
                Ok(loaded) => assert_eq!(loaded.game.digest(), game.digest(), "seed {seed} {mode:?}: loaded otherwise"),
                Err(why) => panic!("seed {seed} {mode:?} turn {turns}: not loaded: {why}\n{}", log.join("\n")),
            }
        }
    }
}

fn seeds() -> std::ops::Range<u64> {
    let from: u64 = std::env::var("FUZZ_FROM").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let n: u64 = std::env::var("FUZZ_N").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
    from..from + n
}

/// What the games came to (with `--nocapture`): a hunt that orders nothing
/// finds nothing.
fn stats(what: &str) {
    let n = |a: &AtomicUsize| a.load(Ordering::Relaxed);
    eprintln!(
        "{what}: onward {}, mixed {}, battles {}, stacks held {}",
        n(&ONWARD),
        n(&MIXED),
        n(&BATTLES),
        n(&GROUPS)
    );
}

#[test]
fn random_orders_in_turns_keep_the_rules_promises() {
    for seed in seeds() {
        fuzz(seed, Mode::Turns, false, 40);
        fuzz(seed, Mode::Turns, true, 30);
    }
    stats("turns");
    assert!(ONWARD.load(Ordering::Relaxed) > 0 && GROUPS.load(Ordering::Relaxed) > 0);
}

#[test]
fn random_orders_in_rounds_keep_the_rules_promises() {
    for seed in seeds() {
        fuzz(seed, Mode::Rounds, false, 40);
        fuzz(seed, Mode::Rounds, true, 30);
    }
    stats("rounds");
    assert!(ONWARD.load(Ordering::Relaxed) > 0 && GROUPS.load(Ordering::Relaxed) > 0);
}
