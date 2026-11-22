//! A stack moved with moves left stays selected (the owner's word,
//! 2026-11-21: "if a stack is moved and it has more MP leave it selected,
//! in FOW games the user may have to creep into the fog so this will be
//! useful vs assuming the stack is done- they can hit D if they are really
//! done"). Once it has none left the screen goes on to the next stack that
//! waits for orders, as it did after every move (Options: the first).

mod common;

use common::{assets, click, pixel_of, push_widget, type_char};
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::Event;
use hexlords::screen::Scene;
use hexlords::session::Session;
use hexlords::setup::{Fog, Setup};
use hexlords::showing::WAY_MS;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use hexlords::temple_window::TEMPLE_BLESS_ID;
use turbo_ferret::{Size, SpecialKey};

/// Where the stack moved stands, and the other stack that waits for orders.
const A: Hex = Hex::new(10, 5);
const B: Hex = Hex::new(20, 20);
/// A temple four hexes south of `A`.
const TEMPLE: Hex = Hex::new(14, 5);

/// 30 x 30 grass: red's Redhold, a temple, blue's Bluemoor far off; red
/// soldiers on `B`, and what `place` puts. Soldiers go 6 hexes a turn,
/// plodders 3; levies are weak.
fn game(fog: Fog, place: impl FnOnce(&mut Game)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[14] = format!("{}t{}", "g".repeat(5), "g".repeat(24));
    rows[28] = format!("{}c{}", "g".repeat(28), "g");
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit plodders\nname Plodders\nstrength 3\nmoves 6\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 28 28\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    put(&mut game, Side::Red, "soldiers", B);
    place(&mut game);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.fog, setup.combat.chance) = (fog, 0);
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

/// The options as a game starts with them (the next stack selected after a
/// move), with moves not shown unless the test shows them.
fn options() -> Options {
    Options { show_moves: false, ..Options::default() }
}

/// A session on `game` with nothing selected yet (the screen selected the
/// stack nearest the cursor; Esc drops it).
fn session(game: Game, options: Options) -> Session {
    let mut s = Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).expect("the session starts");
    common::press(&mut s, SpecialKey::Esc);
    assert!(s.screen.game.selection().is_empty());
    s
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn unit(s: &Session, id: UnitId) -> &hexlords::state::Unit {
    s.screen.game.map.units().get(id).unwrap()
}

/// The units of red's stack on `B`, which waits for orders.
fn on_b(s: &Session) -> Vec<UnitId> {
    s.screen.game.map.units_at(B).to_vec()
}

#[test]
fn a_stack_moved_with_moves_left_stays_selected_to_creep_into_the_fog() {
    let mut a = None;
    let mut s = session(game(Fog::Total, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    let (near, beyond) = (Hex::new(10, 7), Hex::new(10, 9));
    assert!(!s.screen.game.sight(Side::Red).knows(beyond), "black: never seen");
    click_hex(&mut s, A);
    // Land never seen: refused, and it stays selected (as it did).
    click_hex(&mut s, Hex::new(10, 15));
    assert!(s.refusal().is_some(), "said why");
    assert_eq!(s.screen.game.selection(), [a]);
    // Two hexes: 8 of its 12 moves left, and it stays selected...
    click_hex(&mut s, near);
    assert_eq!((unit(&s, a).pos(), unit(&s, a).movement), (near, 8));
    assert_eq!(s.screen.game.selection(), [a], "kept, not on to the next stack");
    // ...where it sees now, and can go there.
    assert!(s.screen.game.sight(Side::Red).knows(beyond));
    assert!(s.screen.game.can_move_to(beyond), "its reach is from where it is, as far as it sees");
    click_hex(&mut s, beyond);
    assert_eq!((unit(&s, a).pos(), unit(&s, a).movement), (beyond, 4));
    assert_eq!(s.screen.game.selection(), [a], "and again");
}

#[test]
fn a_stack_with_no_moves_left_or_marching_on_goes_on_to_the_next() {
    // Six hexes: all its moves.
    let mut a = None;
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(10, 11));
    assert_eq!((unit(&s, a).pos(), unit(&s, a).movement), (Hex::new(10, 11), 0));
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");

    // Beyond its reach: sent there (Go to), as far as it goes this turn.
    let mut a = None;
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(10, 25));
    assert!(!unit(&s, a).route.is_empty(), "on its way");
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");
}

#[test]
fn done_puts_the_stack_kept_selected_aside_and_goes_on() {
    let mut a = None;
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(10, 7));
    assert_eq!(s.screen.game.selection(), [a]);
    type_char(&mut s, 'd');
    assert!(unit(&s, a).done, "put aside");
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");
    // (The Done button does as D: that stack aside too, and none waits now.)
    push_widget(&mut s, hexlords::session::DONE_ID);
    assert!(s.screen.game.selection().is_empty());
}

#[test]
fn of_a_stack_partly_spent_those_with_moves_left_stay_selected() {
    let (mut a, mut p) = (None, None);
    let mut s = session(
        game(Fog::Off, |g| {
            a = Some(put(g, Side::Red, "soldiers", A));
            p = Some(put(g, Side::Red, "plodders", A));
        }),
        options(),
    );
    let (a, p) = (a.unwrap(), p.unwrap());
    click_hex(&mut s, A);
    assert_eq!(s.screen.game.selection(), [a, p]);
    let to = Hex::new(10, 8);
    click_hex(&mut s, to);
    assert_eq!((unit(&s, a).pos(), unit(&s, p).pos()), (to, to), "they went together");
    assert_eq!((unit(&s, a).movement, unit(&s, p).movement), (6, 0));
    assert_eq!(s.screen.game.selection(), [a], "the soldiers, who can go on");
    // They go on alone.
    click_hex(&mut s, Hex::new(10, 9));
    assert_eq!((unit(&s, a).pos(), unit(&s, p).pos()), (Hex::new(10, 9), to));
}

#[test]
fn a_battle_spends_the_attackers_moves_so_the_screen_goes_on() {
    let battle = |s: &Session, before: usize| {
        s.screen.game.history()[before..].iter().any(|r| matches!(r.event, Event::Battle { .. }))
    };
    // Won: they move in, with no moves left (an attack uses them up).
    let mut red = Vec::new();
    let mut s = session(
        game(Fog::Off, |g| {
            red = (0..3).map(|_| put(g, Side::Red, "soldiers", A)).collect();
            put(g, Side::Blue, "levy", Hex::new(10, 6));
        }),
        options(),
    );
    let before = s.screen.game.history().len();
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(10, 6));
    assert!(battle(&s, before));
    let there: Vec<UnitId> = red.iter().copied().filter(|id| s.screen.game.map.units().get(*id).is_some()).collect();
    assert!(!there.is_empty() && there.iter().all(|id| unit(&s, *id).pos() == Hex::new(10, 6)), "won, moved in");
    assert!(there.iter().all(|id| unit(&s, *id).movement == 0));
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");

    // Lost: nothing of it to select.
    let mut levy = None;
    let mut s = session(
        game(Fog::Off, |g| {
            levy = Some(put(g, Side::Red, "levy", A));
            for _ in 0..3 {
                put(g, Side::Blue, "soldiers", Hex::new(10, 6));
            }
        }),
        options(),
    );
    let before = s.screen.game.history().len();
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(10, 6));
    assert!(battle(&s, before));
    assert!(s.screen.game.map.units().get(levy.unwrap()).is_none(), "lost");
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");
}

#[test]
fn a_stack_blessed_on_the_way_has_spent_its_moves() {
    // The temple asks: it stays selected while it does (moves left), and
    // the blessing stops it for the turn.
    let mut a = None;
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    click_hex(&mut s, A);
    click_hex(&mut s, TEMPLE);
    assert!(s.temple().is_some(), "it asks");
    assert_eq!(s.screen.game.selection(), [a], "4 moves left");
    push_widget(&mut s, TEMPLE_BLESS_ID);
    assert!(unit(&s, a).blessed_at(TEMPLE));
    assert_eq!(unit(&s, a).movement, 0);
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");

    // Blessed without asking (Options): on at once.
    let mut a = None;
    let options = Options { always_bless: true, ..options() };
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options);
    click_hex(&mut s, A);
    click_hex(&mut s, TEMPLE);
    assert!(unit(&s, a.unwrap()).blessed_at(TEMPLE));
    assert_eq!(s.screen.game.selection(), on_b(&s), "on to the next");
}

/// At sea the ship's whole stack stays selected, those it carries with no
/// moves of their own left too: the ship alone could not sail off and leave
/// them in the water.
#[test]
fn at_sea_a_ship_stays_selected_with_those_it_carries() {
    let kinds = "unit militia\nstrength 2\nmoves 10\nenter grass 2, city 1\n\
                 unit transport\nstrength 1\nmoves 12\ntravels sea\ncarries 6\nenter water 2, city 2\n";
    let row = format!("{}{}{}", "g".repeat(9), "w".repeat(12), "g".repeat(9));
    let mut rows = vec![row; 30];
    rows[5] = format!("{}c{}{}", "g".repeat(5), "g".repeat(3), &rows[5][9..]);
    rows[28] = format!("{}c{}", &rows[28][..28], "g");
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let cities = "city 5 5\nname Redhold\nside red\ncity 28 28\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(kinds), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let sea = Hex::new(10, 12);
    let ship = put(&mut game, Side::Red, "transport", sea);
    let militia = put(&mut game, Side::Red, "militia", sea);
    game.map.unit_mut(militia).unwrap().movement = 0;
    put(&mut game, Side::Red, "militia", Hex::new(20, 2));
    let setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    let mut s = session(game.with_setup(setup), options());
    click_hex(&mut s, sea);
    assert_eq!(s.screen.game.selection(), [ship, militia]);
    click_hex(&mut s, Hex::new(10, 14));
    assert_eq!((unit(&s, ship).movement, unit(&s, militia).movement), (8, 0));
    assert_eq!(s.screen.game.selection(), [ship, militia], "the ship and who it carries");
    click_hex(&mut s, Hex::new(10, 16));
    assert_eq!((unit(&s, ship).pos(), unit(&s, militia).pos()), (Hex::new(10, 16), Hex::new(10, 16)), "on together");
}

/// A stack put aside and then moved after all is the person's to go on with.
#[test]
fn a_stack_put_aside_then_moved_stays_selected_too() {
    let mut a = None;
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options());
    let a = a.unwrap();
    click_hex(&mut s, A);
    type_char(&mut s, 'd');
    assert!(unit(&s, a).done);
    common::press(&mut s, SpecialKey::Esc);
    click_hex(&mut s, A);
    assert_eq!(s.screen.game.selection(), [a]);
    click_hex(&mut s, Hex::new(10, 7));
    assert_eq!(s.screen.game.selection(), [a]);
}

/// Moves shown as they happen: the stack is selected at once (the game has
/// it there), its reach is marked once it is shown there, and a click on
/// where it went while it goes there sends nothing anywhere.
#[test]
fn a_stack_kept_selected_is_marked_once_it_is_shown_where_it_went() {
    let mut a = None;
    let options = Options { show_moves: true, ..options() };
    let mut s = session(game(Fog::Off, |g| a = Some(put(g, Side::Red, "soldiers", A))), options);
    let a = a.unwrap();
    let (to, next) = (Hex::new(10, 8), Hex::new(10, 9));
    click_hex(&mut s, A);
    click_hex(&mut s, to);
    assert_eq!(s.screen.game.selection(), [a]);
    s.frame(1000);
    assert!(Scene::moving(&s).iter().any(|m| m.units.contains(&a)), "shown going");
    assert!(s.screen.game.can_move_to(next));
    assert!(!Scene::marked(&s, next), "not marked from where it is not shown yet");
    // An impatient second click on where it went: it stays as it is.
    click_hex(&mut s, to);
    assert!(!s.asks_mission() && s.refusal().is_none());
    assert_eq!((unit(&s, a).pos(), unit(&s, a).movement), (to, 6));
    assert_eq!(s.screen.game.selection(), [a]);
    s.frame(1000 + WAY_MS + 1);
    assert!(Scene::moving(&s).is_empty(), "there");
    assert!(Scene::marked(&s, next), "its reach, from there");
}
