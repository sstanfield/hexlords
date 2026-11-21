//! What the second review found (2026-11-21), each held to what the docs
//! say: marauders and the word after a battle at the end of a turn, two
//! battles in one order, the patrol's guard and one's own stacks, units on
//! a standing order kept out of a stack's selection, what an end-of-turn
//! march sees in the fog, a queue edited, the selected stack clicked, a
//! city window that made way.

mod common;

use common::{assets, click, pixel_of};
use hexgame::game::{Game, StackState, Task};
use hexgame::orders::{Event, Order};
use hexgame::session::Session;
use hexgame::setup::{Fog, Setup};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const CAMP: Hex = Hex::new(12, 12);

fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[20] = format!("{}c{}", "g".repeat(15), "g".repeat(14));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 9\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 5 25\nname Bluemoor\nside blue\n\
                  city 20 15\nname Greyford\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex, n: usize) -> Vec<UnitId> {
    let kind = game.map.units().kind_named(kind).unwrap();
    (0..n)
        .map(|_| {
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        })
        .collect()
}

fn pos(game: &Game, id: UnitId) -> Hex {
    game.map.units().get(id).unwrap().pos()
}

fn unit(game: &Game, id: UnitId) -> &hexgame::state::Unit {
    game.map.units().get(id).unwrap()
}

fn stack_state(game: &Game, at: Hex) -> StackState {
    let task = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at: a, .. } if *a == at));
    match task {
        Some(Task::Stack { state, .. }) => state,
        other => panic!("no stack on {at}: {other:?}"),
    }
}

fn session(game: Game) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

/// A marauder that fights as the person ends the turn should wait for the
/// person's word in their next turn ("After a battle"), as a patrol does.
#[test]
fn a_marauder_that_fought_at_the_turns_end_waits_for_its_players_word() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    let mut s = session(game);
    // A blue levy turns up next to them (after the Maraud's immediate lead).
    let levy = put(&mut s.screen.game, Side::Blue, "levy", Hex::new(12, 14), 1);
    assert!(s.next_turn());
    let g = &s.screen.game;
    assert!(g.map.units().get(levy[0]).is_none(), "the levy was struck at the end of red's turn");
    let at = pos(g, red[0]);
    eprintln!("fought flags: {:?}", red.iter().map(|id| unit(g, *id).fought).collect::<Vec<_>>());
    eprintln!("state at {at}: {:?}", stack_state(g, at));
    let fought_battle = g.history().iter().any(|r| matches!(r.event, Event::Battle { .. }));
    assert!(fought_battle);
    assert_eq!(stack_state(g, at), StackState::Fought, "should wait for the player's word");
}

/// A marauder that fought (attacked in blue's turn) and is left alone should
/// go on at the end of red's turn ("left alone, it goes on at the end of the
/// turn"), as a patrol does.
#[test]
fn a_marauder_left_alone_after_a_battle_goes_on_at_the_turns_end() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    // red's End turn, then blue's levy attacks and loses.
    let mut s = session(game);
    let first = pos(&s.screen.game, red[0]);
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue);
    let at = pos(&s.screen.game, red[0]);
    eprintln!("after red's first end turn: {first} -> {at}");
    let n = Hex::new(at.row, at.col + 1);
    let levy = put(&mut s.screen.game, Side::Blue, "levy", n, 1);
    let events = s.screen.game.apply(Order::Move { units: levy, to: at }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { won: false, .. })));
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).fought));
    s.screen.game.apply(Order::EndTurn).unwrap();
    // (Blue is the computer's in this setup? it is passive: end turn by hand above.)
    s.wait_for_turn();
    s.start_turn();
    assert_eq!(s.screen.game.player(), Side::Red);
    assert_eq!(stack_state(&s.screen.game, at), StackState::Fought);
    // Left alone: End turn.
    s.end_turn();
    let after = pos(&s.screen.game, red[0]);
    eprintln!("after red's second end turn (left alone): {at} -> {after}");
    assert_ne!(after, at, "left alone, it should go on at the end of the turn");
}

/// Two patrol battles at the end of a turn: each battle's record should
/// replay its own fight.
#[test]
fn two_battles_in_one_order_keep_their_own_fights() {
    let mut game = game();
    let a = put(&mut game, Side::Red, "soldiers", Hex::new(10, 5), 2);
    let b = put(&mut game, Side::Red, "soldiers", Hex::new(20, 5), 2);
    // Patrols with nothing to attack yet; out of moves after their first leg.
    game.apply(Order::Patrol {
        units: a.clone(),
        waypoints: vec![Hex::new(10, 5), Hex::new(10, 11)],
        attack: Some(50),
    })
    .unwrap();
    game.apply(Order::Patrol {
        units: b.clone(),
        waypoints: vec![Hex::new(20, 5), Hex::new(20, 11)],
        attack: Some(50),
    })
    .unwrap();
    // Next round red's patrols go at the end of its turn: a levy near each.
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Red);
    let la = pos(&game, a[0]);
    let lb = pos(&game, b[0]);
    put(&mut game, Side::Blue, "levy", Hex::new(la.row + 1, la.col + 1), 1);
    put(&mut game, Side::Blue, "levy", Hex::new(lb.row + 1, lb.col + 1), 1);
    let before = game.history().len();
    game.apply(Order::EndTurn).unwrap();
    let records: Vec<usize> =
        (before..game.history().len()).filter(|i| matches!(game.history()[*i].event, Event::Battle { .. })).collect();
    assert_eq!(records.len(), 2, "two battles");
    for r in records {
        let Event::Battle { at, .. } = game.history()[r].event.clone() else { unreachable!() };
        let fight = game.fight(r).map(|f| f.at);
        eprintln!("record {r}: battle at {at}, fight kept: {fight:?}");
        assert_eq!(fight, Some(at), "the record's fight is its own");
    }
}

/// While a patrol is selected, a click on another of the player's stacks in
/// reach selects it (select_hex) — the guard should not ask for a second
/// click as if it gave an order.
#[test]
fn with_a_patrol_selected_a_click_on_another_own_stack_selects_it_at_once() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(10, 5), 2);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![Hex::new(10, 5), Hex::new(10, 11)], attack: None })
        .unwrap();
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
    let at = pos(&game, red[0]);
    let other_hex = Hex::new(at.row + 1, at.col);
    let other = put(&mut game, Side::Red, "soldiers", other_hex, 1);
    let mut s = session(game);
    click_hex(&mut s, at);
    assert_eq!(s.screen.game.selection().len(), 2);
    assert!(s.screen.game.can_move_to(other_hex));
    click_hex(&mut s, other_hex);
    eprintln!("line: {}", s.unit_line());
    eprintln!("selection: {:?}", s.screen.game.selection());
    assert_eq!(s.screen.game.selection(), other.as_slice(), "the other stack should be selected at once");
}

/// A unit sent somewhere (Go to, standing order) waits on a hex with an idle
/// unit: a click on the hex selects the idle one only, and its march goes
/// on.
#[test]
fn a_marching_unit_is_left_out_of_a_stacks_selection() {
    let mut game = game();
    let here = Hex::new(10, 5);
    let marcher = put(&mut game, Side::Red, "soldiers", here, 1);
    let idle = put(&mut game, Side::Red, "soldiers", here, 1);
    // Sent far (Go to), it goes this turn's part at once...
    game.apply(Order::GoTo { units: marcher.clone(), to: Hex::new(10, 25), via: vec![] }).unwrap();
    let m = pos(&game, marcher[0]);
    // ... and the idle one joins it there (a move of its own).
    game.apply(Order::Move { units: idle.clone(), to: m }).unwrap();
    game.apply(Order::EndTurn).unwrap(); // marches at the end
    game.apply(Order::EndTurn).unwrap();
    let m2 = pos(&game, marcher[0]);
    eprintln!("marcher {m} -> {m2}; idle at {}", pos(&game, idle[0]));
    // Put the idle one beside the marcher again, by hand (as if it walked).
    let idle2 = put(&mut game, Side::Red, "soldiers", m2, 1);
    let mut s = session(game);
    click_hex(&mut s, m2);
    eprintln!("selection {:?}, marcher {:?}, idle {:?}", s.screen.game.selection(), marcher, idle2);
    assert!(!s.screen.game.selection().contains(&marcher[0]), "a marching unit is not swept into the stack");
}

/// With fog of war, an enemy stack seen only along the way of an
/// end-of-turn march is remembered (its sightings are noted before the
/// trail is cleared).
/// A corridor: row 15 grass from col 0 to 29, all else mountain but a pocket
/// at (17, 14); red's city at (15, 0), blue's at (15, 29).
fn corridor(fog: Fog) -> Game {
    let mut rows = vec!["m".repeat(30); 30];
    rows[15] = format!("c{}c", "g".repeat(28));
    rows[17] = format!("{}g{}", "m".repeat(14), "m".repeat(15));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 15 0\nname Redhold\nside red\ncity 15 29\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    setup.fog = fog;
    game.with_setup(setup)
}

#[test]
fn what_an_end_of_turn_march_sees_in_the_fog_is_remembered() {
    for at_once in [true, false] {
        let mut game = corridor(Fog::On);
        let red = put(&mut game, Side::Red, "riders", Hex::new(15, 9), 1);
        let pocket = Hex::new(17, 14);
        put(&mut game, Side::Blue, "levy", pocket, 1);
        assert!(!game.sight(Side::Red).sees(pocket));
        // The rider goes 10 hexes: from col 9 to col 19, by col 14 (two
        // rows from the pocket), which neither end sees.
        if at_once {
            // A move of the player's, in the turn.
            game.apply(Order::Move { units: red.clone(), to: Hex::new(15, 19) }).unwrap();
        } else {
            // A march at the end of the turn: units built last turn, sent
            // there (as a city's new units are), or a stalled march.
            game.map.unit_mut(red[0]).unwrap().route = vec![Hex::new(15, 19)];
            game.apply(Order::EndTurn).unwrap();
        }
        assert_eq!(pos(&game, red[0]), Hex::new(15, 19));
        eprintln!("at_once {at_once}: ghosts {:?}", game.ghosts(Side::Red));
        assert!(game.ghost_at(Side::Red, pocket).is_some(), "at_once {at_once}: the stack it passed is remembered");
    }
}

mod queue_resume {
    use super::*;
    use hexgame::state::{QueueEnd, QueueItem, Sends};

    fn qgame() -> Game {
        let mut rows = vec!["g".repeat(30); 30];
        rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
        let terrain = format!("30\n30\n{}\n", rows.join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                     unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
        let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\nproduce riders 1\n\
                      city 5 25\nname Bluemoor\nside blue\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
        setup.gold = 500;
        game.with_setup(setup)
    }

    /// [Riders x1, Soldiers x3 (defend), Soldiers x3 (stay)], building item 1
    /// with 2 of 3 out; the player takes out item 0 (the riders, done). The
    /// queue should go on with the item it was building (now item 0), one
    /// soldier left of it, then the 3 of the last item.
    #[test]
    fn taking_out_an_item_before_goes_on_with_the_current_one() {
        let mut game = qgame();
        let city = game.map.city_at(Hex::new(5, 5)).unwrap().id;
        let items = vec![
            QueueItem { what: 1, count: 1, sends: Sends::Stay },
            QueueItem { what: 0, count: 3, sends: Sends::Defend },
            QueueItem { what: 0, count: 3, sends: Sends::Stay },
        ];
        game.apply(Order::Queue { city, items: items.clone(), end: QueueEnd::Stop }).unwrap();
        for _ in 0..3 {
            game.apply(Order::EndTurn).unwrap();
            game.apply(Order::EndTurn).unwrap();
        }
        let q = game.map.cities().get(city).unwrap().queue.clone();
        eprintln!("before: at {} made {}", q.at, q.made);
        assert_eq!((q.at, q.made), (1, 2));
        // The session's QueueRemove(0): the same items without the first.
        let new: Vec<QueueItem> = items[1..].to_vec();
        game.apply(Order::Queue { city, items: new, end: QueueEnd::Stop }).unwrap();
        let q = game.map.cities().get(city).unwrap().queue.clone();
        eprintln!("after: at {} made {} item {:?}", q.at, q.made, q.current());
        assert_eq!(q.at, 0, "it goes on with the item it was building (Soldiers x3, defend)");
    }

    /// Repeat once: on the last pass the line says it is the last time (it said
    /// "0 times more").
    #[test]
    fn the_last_pass_of_a_repeat_says_so() {
        let mut game = qgame();
        let city = game.map.city_at(Hex::new(5, 5)).unwrap().id;
        let items = vec![QueueItem { what: 0, count: 1, sends: Sends::Stay }];
        game.apply(Order::Queue { city, items, end: QueueEnd::Repeat { from: 0, times: Some(1) } }).unwrap();
        let c = game.map.cities().get(city).unwrap();
        eprintln!("first pass: {}", hexgame::info::queue_end_line(c));
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        let c = game.map.cities().get(city).unwrap();
        let line = hexgame::info::queue_end_line(c);
        eprintln!("last pass: {line} (rounds {})", c.queue.rounds);
        assert!(!line.contains("0 times more"), "{line}");
    }
}

/// A click on the selected stack's hex "keeps it" (the doc): the units the
/// player took into the selection by their boxes stay selected.
#[test]
fn a_click_on_the_selected_stacks_hex_keeps_the_selection() {
    let mut game = game();
    let here = Hex::new(10, 5);
    let soldiers = put(&mut game, Side::Red, "soldiers", here, 2);
    let guard = put(&mut game, Side::Red, "soldiers", here, 1);
    game.apply(Order::Defend { units: guard.clone(), on: true }).unwrap();
    // The player selects the guard alone (its box), to send it out.
    assert!(game.select_unit(guard[0]));
    assert_eq!(game.selection(), guard.as_slice());
    let outcome = game.select_hex(here);
    eprintln!("outcome {outcome:?}, selection {:?}, soldiers {soldiers:?}", game.selection());
    assert_eq!(game.selection(), guard.as_slice(), "a click on the selected stack keeps it");
}

/// A city's window that made way for a pick, then a task's row chosen
/// (the pick is over): the window should not come back when a later pick
/// from the sidebar is let go.
#[test]
fn a_task_row_ends_a_pick_and_its_window_does_not_come_back() {
    use hexgame::city_windows::{CITY_RALLY_ID, PANEL_PREFIX};
    use turbo_ferret::SpecialKey;
    let mut game = game();
    put(&mut game, Side::Red, "soldiers", Hex::new(10, 5), 1);
    let mut s = session(game);
    click_hex(&mut s, Hex::new(5, 5));
    common::type_char(&mut s, 'o');
    s.frame(0);
    common::push_widget(&mut s, CITY_RALLY_ID);
    s.frame(0);
    assert!(s.city_window().is_none(), "out of the way");
    assert!(s.picking().is_some());
    // The tasks: a stack's row.
    common::type_char(&mut s, 't');
    s.frame(0);
    common::push_widget(&mut s, &hexgame::tasks_window::row_id(0));
    s.frame(0);
    assert!(s.picking().is_none(), "the pick is over");
    // Later: Redhold in the sidebar, its Send to..., let go with Esc.
    click_hex(&mut s, Hex::new(5, 5));
    common::press(&mut s, SpecialKey::Esc);
    s.frame(0);
    common::push_widget(&mut s, &format!("{PANEL_PREFIX}{CITY_RALLY_ID}"));
    s.frame(0);
    assert!(s.picking().is_some());
    common::press(&mut s, SpecialKey::Esc);
    s.frame(0);
    assert!(s.city_window().is_none(), "the window from long ago does not come back");
}

/// A city's new unit sent to a rally point waits in the city through the
/// turn it is built; a garrison there waits for orders. Next stack selects
/// the city's stack: the garrison, not the new unit, whose march goes on.
#[test]
fn a_new_unit_on_its_way_is_left_out_of_the_garrisons_selection() {
    use hexgame::orders::RallyPoint;
    use hexgame::state::Sends;
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ncity 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    let mut game = game.with_setup(setup);
    let redhold = game.map.city_at(Hex::new(5, 5)).unwrap().id;
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: Hex::new(5, 15), defend: false }) }).unwrap();
    game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 0, bless: false } }).unwrap();
    let garrison = put(&mut game, Side::Red, "soldiers", Hex::new(5, 5), 1);
    let mut s = session(game);
    assert!(s.next_turn());
    let g = &s.screen.game;
    let new: Vec<UnitId> = g.map.units_at(Hex::new(5, 5)).iter().copied().filter(|id| !garrison.contains(id)).collect();
    assert_eq!(new.len(), 1, "a new soldier");
    assert_eq!(unit(g, new[0]).route, vec![Hex::new(5, 15)], "sent to the point, waiting for the turn's end");
    // N: the next stack that waits.
    common::type_char(&mut s, 'n');
    eprintln!("selection {:?}; garrison {:?}; new {:?}", s.screen.game.selection(), garrison, new);
    assert!(!s.screen.game.selection().contains(&new[0]), "the new unit on its way is not taken along");
}
