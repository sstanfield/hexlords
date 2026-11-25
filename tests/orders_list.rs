//! The sidebar's list of what needs orders, and the side's army under it
//! (the owner's word, 2026-11-24: "Lets try putting the sides stack list
//! over there. Can select a stack to go to it. Color can indicate if it is
//! a garrison and if it has orders for the turn yet"; T steps through what
//! needs orders, the Tasks window went).

mod common;

use common::{assets, push_widget, type_char};
use hexlords::game::{Game, StackState, Task};
use hexlords::orders::Order;
use hexlords::orders_list::{DONE_SKIN, GARRISON_SKIN, NEEDS_SKIN, row_id};
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Size, TbId};

const REDHOLD: Hex = Hex::new(5, 5);
const FIELD: Hex = Hex::new(8, 8);

/// Red's Redhold (two soldiers in it, building nothing) and two soldiers in
/// the field; blue's Bluemoor far off.
fn session() -> (Session, Vec<UnitId>, Vec<UnitId>) {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggggggggggggg".to_string();
    rows[15] = "gggggggggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers 2\n\
                  city 15 15\nname Bluemoor\nside blue\nproduce soldiers 1\ngarrison soldiers\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let field: Vec<UnitId> = (0..2)
        .map(|_| {
            let id = game.map.new_unit_of(Side::Red, kind).unwrap();
            game.map.place_unit(id, FIELD).unwrap();
            id
        })
        .collect();
    let city: Vec<UnitId> = game.map.units_at(REDHOLD).to_vec();
    let setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    let game = game.with_setup(setup);
    let s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    (s, city, field)
}

/// The rows: what each is about, and its skin.
fn rows(s: &mut Session) -> Vec<(Task, &'static str)> {
    s.frame(0);
    s.orders_rows().iter().map(|r| (r.task, r.skin)).collect()
}

fn skin_of(s: &Session, i: usize) -> TbId {
    s.screen.ui.skin(common::widget(s, &row_id(i)))
}

#[test]
fn what_needs_orders_comes_first_in_gold_and_the_army_stays() {
    let (mut s, city, _) = session();
    let redhold = s.screen.game.map.city_at(REDHOLD).unwrap().id;
    // Both stacks wait, and the city builds nothing: all three need orders.
    let rows0 = rows(&mut s);
    assert_eq!(rows0.len(), 3, "{rows0:?}");
    assert!(rows0.iter().all(|(_, skin)| *skin == NEEDS_SKIN));
    assert!(rows0.iter().any(|(t, _)| matches!(t, Task::City { city, .. } if *city == redhold)));
    assert_eq!(skin_of(&s, 0), TbId::new(NEEDS_SKIN));
    let says = |s: &Session| s.screen.ui.text(common::widget(s, "orders_heading")).unwrap_or("").to_string();
    assert_eq!(says(&s), "3 need orders");
    // The city given work leaves the list; the garrison set to defend stays,
    // in its own colour, after what needs orders.
    s.screen.game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    s.screen.game.apply(Order::Defend { units: city.clone(), on: true }).unwrap();
    let rows1 = rows(&mut s);
    assert_eq!(rows1.len(), 2, "{rows1:?}");
    assert!(matches!(rows1[0], (Task::Stack { at: FIELD, state: StackState::Waiting, .. }, NEEDS_SKIN)), "{rows1:?}");
    assert!(matches!(rows1[1], (Task::Stack { at: REDHOLD, state: StackState::Defending, .. }, GARRISON_SKIN)));
    assert_eq!(says(&s), "1 needs orders");
    // The field's stack put aside (D): dimmed; nothing needs orders.
    let i = s.orders_rows().iter().position(|r| matches!(r.task, Task::Stack { at: FIELD, .. })).unwrap();
    push_widget(&mut s, &row_id(i));
    type_char(&mut s, 'd');
    let rows2 = rows(&mut s);
    assert!(
        rows2.iter().any(|(t, skin)| matches!(t, Task::Stack { at: FIELD, .. }) && *skin == DONE_SKIN),
        "{rows2:?}"
    );
    assert_eq!(says(&s), "All have orders");
}

#[test]
fn a_row_selects_its_stack_and_t_steps_through_what_needs_orders() {
    let (mut s, city, field) = session();
    let redhold = s.screen.game.map.city_at(REDHOLD).unwrap().id;
    s.frame(0);
    // A stack's row: those units, selected and shown.
    let i = s.orders_rows().iter().position(|r| matches!(r.task, Task::Stack { at: FIELD, .. })).unwrap();
    push_widget(&mut s, &row_id(i));
    let mut chosen = s.screen.game.selection().to_vec();
    chosen.sort_by_key(|id| (id.index, id.generation));
    let mut want = field.clone();
    want.sort_by_key(|id| (id.index, id.generation));
    assert_eq!((chosen, s.shown()), (want, FIELD));
    // A city's row: production, on it.
    let i = s.orders_rows().iter().position(|r| matches!(r.task, Task::City { .. })).unwrap();
    push_widget(&mut s, &row_id(i));
    assert_eq!(s.production().and_then(|p| p.chosen_city()), Some(redhold));
    type_char(&mut s, 'e');
    // T: from Redhold (shown: its stack), on to the field's; Shift+T, back
    // to Redhold's; T twice more, the field's and then the city, in
    // production.
    let sorted = |mut ids: Vec<UnitId>| {
        ids.sort_by_key(|id| (id.index, id.generation));
        ids
    };
    type_char(&mut s, 't');
    assert_eq!(s.shown(), FIELD);
    assert_eq!(sorted(s.screen.game.selection().to_vec()), sorted(field.clone()));
    type_char(&mut s, 'T');
    assert_eq!(sorted(s.screen.game.selection().to_vec()), sorted(city.clone()), "Redhold's stack");
    type_char(&mut s, 't');
    type_char(&mut s, 't');
    assert_eq!(s.production().and_then(|p| p.chosen_city()), Some(redhold));
    assert!(s.screen.ui.find(s.screen.ui.root(), TbId::new("tasks_window")).is_none(), "no Tasks window");
}

/// The owner saw the menu bar go off the top of the window (and the
/// production panel with it): the list's box kept its height as the city's
/// lines above it grew, the sidebar ran past the window's bottom, and the
/// whole layout moved. The list has what the window leaves, whatever the
/// sidebar shows.
#[test]
fn the_sidebar_stays_in_the_window_as_the_citys_lines_come_and_go() {
    let (game, _) = Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    let rect = |s: &Session, name: &str| {
        let id = common::widget(s, name);
        turbo_ferret::Rect::from_origin_size(
            s.screen.ui.to_root(id, turbo_ferret::Point::ZERO),
            s.screen.ui.rect(id).size(),
        )
    };
    // Land, a city (its lines), land again, and the city again.
    for hex in [Hex::new(14, 9), Hex::new(13, 7), Hex::new(14, 9), Hex::new(13, 7)] {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(&s, hex);
        common::click(&mut s, p);
        for t in 0..3 {
            s.frame(16 * t);
        }
        let (menu, sidebar) = (rect(&s, "menu_bar"), rect(&s, "main_sidebar"));
        assert_eq!(menu.y, 0, "at {hex}");
        assert!(sidebar.bottom() <= 768, "at {hex}: {sidebar:?}");
        assert!(rect(&s, "orders_scroll").bottom() <= 768, "at {hex}");
    }
}
