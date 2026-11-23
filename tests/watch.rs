//! Battles watched: the game keeps each battle's fight (who fought with what,
//! each exchange) for a few rounds; the battle's window shows both stacks
//! with their bonuses and plays the exchanges out, a few a second; it opens
//! on the person's own attack, and from the log's Watch buttons for the
//! battles they were in, the computer's attacks and, in hotseat, another
//! person's.

mod common;

use common::{assets, choose, click, pixel_of, press, widget};
use hexlords::battle_window::{BATTLE_AGAIN_ID, BATTLE_SKIP_ID, points};
use hexlords::game::{Game, REPLAY_ROUNDS};
use hexlords::info;
use hexlords::orders::{Event, Order};
use hexlords::session::{BEGIN_TURN_ID, LOG_ID, MENU_VIEW_ID, Session};
use hexlords::setup::Setup;
use hexlords::state::combat::HITS;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);

/// Red's Redhold with `red` and Southwick with nobody; blue's Bluemoor, walls
/// 1, with `blue`, and Farwater with nobody. Red a person, blue passive, unless `setup`
/// says otherwise.
fn game(red: &str, blue: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[15] = "gggggcgggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nimage units/wardens.png\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nimage units/militia.png\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nimage units/captain.png\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\n{red}city 15 5\nname Southwick\nside red\n\
         city 15 15\nname Farwater\nside blue\n\
         city 5 9\nname Bluemoor\nside blue\nwalls 1\n{blue}"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup(&mut s);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

fn battle_record(game: &Game) -> usize {
    game.history().iter().rposition(|r| matches!(r.event, Event::Battle { .. })).expect("a battle")
}

#[test]
fn the_game_keeps_a_battles_fight_for_a_few_rounds() {
    let mut game = game("garrison soldiers 3\ngarrison captain\n", "garrison levy 2\n", |_| {});
    let red = at(&game, REDHOLD);
    game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    let record = battle_record(&game);
    let fight = game.fight(record).expect("kept").clone();
    assert_eq!((fight.attacker, fight.defender, fight.at, fight.walls), (Side::Red, Side::Blue, BLUEMOOR, 1));
    // Who fought with what: the soldiers 3 and the captain's 1, in quarter
    // points; the captain itself 2; the levies 1 and the walls' 1.
    let soldiers = &fight.attackers[0];
    assert_eq!((soldiers.strength, soldiers.led, soldiers.ground, soldiers.walls, soldiers.quarters), (3, 1, 0, 0, 16));
    let captain = fight.attackers.iter().find(|u| u.commander).unwrap();
    assert_eq!((captain.led, captain.quarters), (0, 8));
    let levy = &fight.defenders[0];
    assert_eq!((levy.strength, levy.walls, levy.quarters, levy.name.as_str()), (1, 4, 8, "Levy"));
    // Its trace kills the dead the battle reported.
    let Event::Battle { dead, attackers_lost, defenders_lost, .. } = &game.history()[record].event else { panic!() };
    let hits = |attacker: bool| fight.trace.iter().filter(|b| b.hit_attacker == attacker).count() as u32;
    assert!(hits(false) >= HITS * *defenders_lost as u32 && dead.len() == attackers_lost + defenders_lost);
    // After REPLAY_ROUNDS rounds it is gone.
    for _ in 0..REPLAY_ROUNDS * 2 {
        game.end_turn();
    }
    assert!(game.fight(record).is_none());
}

// The screen.

fn session(game: Game) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap()
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn says(s: &Session, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![widget(s, root)];
    while let Some(id) = stack.pop() {
        out.extend(s.screen.ui.text(id).map(str::to_string));
        stack.extend(s.screen.ui.children(id).iter().copied());
    }
    out
}

#[test]
fn an_attack_is_played_out_in_the_battles_window() {
    let mut s = session(game("garrison soldiers 3\ngarrison captain\n", "garrison levy 2\n", |s| s.combat.chance = 5));
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    let window = s.battle().expect("the battle's window");
    let n = window.progress().1;
    assert!(n >= 4, "two levies, two hits each, at least");
    // Both stacks, a card each, and where the bonuses come from.
    let text = says(&s, hexlords::battle_window::BATTLE_WINDOW_ID);
    for line in [
        "Red attacks blue at Bluemoor",
        "Red: 4 units, led by a Captain (+1 each)",
        "Blue: 2 units, behind walls 1 (+1)",
        "Ready...",
    ] {
        assert!(text.iter().any(|t| t == line), "{line} is not in {text:?}");
    }
    assert!(text.iter().filter(|t| *t == "Soldiers").count() == 3 && text.iter().filter(|t| *t == "Levy").count() == 2);
    assert!(text.iter().any(|t| t == "3 +1") && text.iter().any(|t| t == "1 +1"), "{text:?}");
    // The frames play it: nothing for a moment, then an exchange at a time.
    s.frame(0);
    s.frame(300);
    assert_eq!(s.battle().unwrap().progress().0, 0, "a moment first");
    s.frame(1000);
    assert_eq!(s.battle().unwrap().progress().0, 1);
    let [fought, fate] = s.battle().unwrap().status(&s.screen.ui);
    assert!(fought.starts_with("Red's ") && fought.contains(" against blue's "), "{fought}");
    assert!(fate.starts_with("A hit on ") || fate.ends_with(": out of the battle."), "{fate}");
    let hits: u32 = s.battle().unwrap().hits().iter().flatten().sum();
    assert_eq!(hits, 1, "one hit, on one card");
    // All of it, then how it went (at the pace a player gets at first,
    // half the fast one: an exchange in a second at most).
    let mut t = 1000;
    while !s.battle().unwrap().done() {
        t += 1000;
        s.frame(t);
    }
    assert!(t < 1000 + 1000 * n as u64 + 1000);
    let record = battle_record(&s.screen.game);
    let outcome = info::battle_outcome(&s.screen.game, record, Side::Red);
    assert_eq!(s.battle().unwrap().outcome(&s.screen.ui), outcome);
    assert!(s.battle().unwrap().status(&s.screen.ui)[1].ends_with("Red wins."));
    // Watch again: from the start; Skip: to the end.
    push(&mut s, BATTLE_AGAIN_ID);
    assert_eq!(s.battle().unwrap().progress().0, 0);
    assert!(s.battle().unwrap().outcome(&s.screen.ui).is_empty());
    push(&mut s, BATTLE_SKIP_ID);
    assert!(s.battle().unwrap().done());
    let dead: Vec<u32> = s.battle().unwrap().hits()[1].clone();
    assert!(dead.iter().all(|h| *h == HITS), "the levies, both down: {dead:?}");
    press(&mut s, SpecialKey::Esc);
    assert!(s.battle().is_none());
}

/// Enter, while a battle's window is open, is the window's: it ends a
/// battle still being played out and closes one that is over, whoever
/// ordered the attack. (For a battle of the person's own, or one watched
/// again from the log, it once went to the map as a click at the cursor;
/// only a battle the screen showed by itself took it.)
#[test]
fn enter_ends_and_then_closes_a_battle_of_ones_own() {
    let mut s = session(game("garrison soldiers 3\ngarrison captain\n", "garrison levy 2\n", |s| s.combat.chance = 5));
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    assert!(s.battle().is_some_and(|b| !b.done()), "being played out");
    let before = (s.screen.game.selection().to_vec(), s.screen.game.map.cursor, s.screen.game.history().len());
    press(&mut s, SpecialKey::Enter);
    assert!(s.battle().is_some_and(|b| b.done()), "Enter ends it");
    press(&mut s, SpecialKey::Enter);
    assert!(s.battle().is_none(), "and then closes it");
    let after = (s.screen.game.selection().to_vec(), s.screen.game.map.cursor, s.screen.game.history().len());
    assert_eq!(before, after, "and nothing else: no click on the map");
    // The same for one watched again from the log.
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    let watch = s.log_watchable();
    push(&mut s, &hexlords::log_window::watch_id(watch[0]));
    assert!(s.battle().is_some_and(|b| !b.done()));
    press(&mut s, SpecialKey::Enter);
    assert!(s.battle().is_some_and(|b| b.done()));
    press(&mut s, SpecialKey::Enter);
    assert!(s.battle().is_none());
    assert!(!s.log_lines().is_empty(), "the log under it is still open");
}

#[test]
fn the_computers_attacks_are_watched_from_the_log() {
    // Blue (the computer) attacks Redhold in its turn; red's log has the
    // battle, and a Watch button that plays it.
    let mut s = session(game("garrison levy 2\n", "garrison soldiers 4\n", |s| s.set_players("blue=simple").unwrap()));
    s.end_turn();
    s.wait_for_turn();
    let log = s.log_lines();
    assert!(log.iter().any(|l| l.starts_with("Blue attacked you at Redhold")), "{log:?}");
    let watch = s.log_watchable();
    assert_eq!(watch.len(), 1, "one battle");
    push(&mut s, &hexlords::log_window::watch_id(watch[0]));
    let window = s.battle().expect("the battle");
    assert_eq!(window.progress().0, 0);
    push(&mut s, BATTLE_SKIP_ID);
    let outcome = s.battle().unwrap().outcome(&s.screen.ui);
    assert!(outcome[0].starts_with("Blue attacked you at Redhold"), "{outcome:?}");
}

#[test]
fn in_hotseat_the_one_attacked_watches_it_from_their_log() {
    let mut s = session(game("garrison soldiers 3\n", "garrison levy\n", |s| s.set_players("blue=person").unwrap()));
    push(&mut s, BEGIN_TURN_ID);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    assert!(s.battle().is_some(), "red watches its own attack");
    s.end_turn();
    push(&mut s, BEGIN_TURN_ID);
    // Blue's log is open: the attack, with its button.
    assert!(s.log_lines().iter().any(|l| l.starts_with("Red attacked you at Bluemoor")));
    let watch = s.log_watchable();
    assert_eq!(watch.len(), 1);
    push(&mut s, &hexlords::log_window::watch_id(watch[0]));
    push(&mut s, BATTLE_SKIP_ID);
    let outcome = s.battle().unwrap().outcome(&s.screen.ui);
    assert!(outcome[0].starts_with("Red attacked you at Bluemoor"), "told as blue: {outcome:?}");
    // Closed, the log is still there; L closes it too.
    press(&mut s, SpecialKey::Esc);
    assert!(s.battle().is_none() && !s.log_lines().is_empty());
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
}

#[test]
fn strengths_read_in_quarters() {
    assert_eq!(
        [points(4), points(5), points(6), points(7), points(8), points(2)],
        ["1", "1\u{bc}", "1\u{bd}", "1\u{be}", "2", "\u{bd}"]
    );
}
