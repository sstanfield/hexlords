//! The menu bar: menus that open under their titles, with each item's keys;
//! choosing one does what its key or button does; the treasury at the bar's
//! end; the Help menu's topics, written from the game in play; and, in
//! hotseat, each person's turn starting at their capital.

mod common;

use common::{assets, choose, click, pixel_of, press, push_widget, type_char, widget};
use hexgame::game::Game;
use hexgame::guide;
use hexgame::log_window::LOG_WINDOW_ID;
use hexgame::screen::{HELP_KEYS_ID, ZOOM_IN_ID};
use hexgame::session::{
    ALL_CITIES_ID, BEGIN_TURN_ID, CAPITAL_ID, DESELECT_ID, GO_TO_ID, LOG_ID, MENU_CITIES_ID, MENU_GAME_ID,
    MENU_HELP_ID, MENU_TITLES, MENU_UNITS_ID, MENU_VIEW_ID, Session, TASKS_ID, UNDO_ID,
};
use hexgame::setup::{Fog, Setup};
use hexgame::state::combat::System;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side};
use turbo_ferret::{Modifiers, Point, Size, SpecialKey, TbId};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(15, 15);

/// 20 x 20 grass: red's Redhold with soldiers, blue's Bluemoor far from it.
fn game(setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggggggggggggg".to_string();
    rows[15] = "gggggggggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers 2\n\
                  city 15 15\nname Bluemoor\nside blue\nproduce soldiers 1\ngarrison soldiers\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup(&mut s);
    game.with_setup(s)
}

fn session(setup: impl FnOnce(&mut Setup)) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game(setup)).unwrap()
}

fn open(s: &Session) -> Option<TbId> {
    s.open_menu_items().map(|(id, _)| id)
}

fn items(s: &Session) -> Vec<(String, String)> {
    s.open_menu_items().map_or(Vec::new(), |(_, items)| {
        items.into_iter().map(|(id, _, keys)| (id.to_string(), keys.to_string())).collect()
    })
}

fn disabled(s: &Session, name: &str) -> bool {
    s.screen.ui.node(widget(s, name)).is_some_and(|n| n.is_disabled())
}

#[test]
fn a_title_opens_its_menu_and_a_second_click_closes_it() {
    let mut s = session(|_| {});
    let titles: Vec<&str> = MENU_TITLES.iter().map(|(_, t)| *t).collect();
    assert_eq!(titles, ["Game", "Units", "Cities", "View", "Help"]);
    assert_eq!(open(&s), None);
    push_widget(&mut s, MENU_CITIES_ID);
    assert_eq!(open(&s), Some(TbId::new(MENU_CITIES_ID)));
    // Under its title, each item with its keys.
    let (title, drop) = (s.screen.area(widget(&s, MENU_CITIES_ID)), s.screen.area(widget(&s, "menu_drop")));
    assert!(drop.y >= title.bottom() - 2 && drop.x <= title.x + 8, "{title:?} {drop:?}");
    assert!(items(&s).contains(&(ALL_CITIES_ID.to_string(), "A".to_string())), "{:?}", items(&s));
    push_widget(&mut s, MENU_CITIES_ID);
    assert_eq!(open(&s), None);
    // The keyboard stays with the map all along.
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
}

#[test]
fn the_pointer_on_another_title_opens_its_menu() {
    let mut s = session(|_| {});
    push_widget(&mut s, MENU_GAME_ID);
    let r = s.screen.area(widget(&s, MENU_VIEW_ID));
    common::hover(&mut s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    s.frame(16);
    assert_eq!(open(&s), Some(TbId::new(MENU_VIEW_ID)));
    assert!(items(&s).iter().any(|(id, keys)| id == TASKS_ID && keys == "T"));
}

#[test]
fn esc_or_a_press_on_the_map_closes_a_menu_and_does_nothing_else() {
    let mut s = session(|_| {});
    // Red's stack selected: Esc closes the menu, and the selection stays.
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    assert!(s.screen.game.selected().is_some());
    push_widget(&mut s, MENU_UNITS_ID);
    press(&mut s, SpecialKey::Esc);
    assert_eq!(open(&s), None);
    assert!(s.screen.game.selected().is_some(), "only the menu went");
    // A click on the map closes it too, and moves nothing.
    push_widget(&mut s, MENU_UNITS_ID);
    let far = pixel_of(&s, Hex::new(6, 6));
    click(&mut s, far);
    assert_eq!(open(&s), None);
    assert_eq!(s.screen.game.map.units_at(REDHOLD).len(), 2, "nothing moved");
    // Then a click is a click again.
    click(&mut s, far);
    assert_eq!(s.screen.game.map.units_at(Hex::new(6, 6)).len(), 2);
}

#[test]
fn an_item_does_what_its_key_does_and_is_greyed_when_it_can_do_nothing() {
    let mut s = session(|_| {});
    // Nothing selected: Go to and Drop the selection are greyed.
    push_widget(&mut s, MENU_UNITS_ID);
    assert!(disabled(&s, GO_TO_ID) && disabled(&s, DESELECT_ID));
    press(&mut s, SpecialKey::Esc);
    // Nothing to take back.
    push_widget(&mut s, MENU_GAME_ID);
    assert!(disabled(&s, UNDO_ID));
    press(&mut s, SpecialKey::Esc);
    // A stack selected: the menu drops it.
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    choose(&mut s, MENU_UNITS_ID, DESELECT_ID);
    assert!(s.screen.game.selected().is_none());
    // The view's: the tasks, the log, the zoom.
    choose(&mut s, MENU_VIEW_ID, TASKS_ID);
    assert!(s.tasks_window().is_some());
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    let log_open = |s: &Session| s.screen.ui.find(s.screen.ui.root(), TbId::new(LOG_WINDOW_ID)).is_some();
    assert!(log_open(&s));
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    assert!(!log_open(&s), "the same again closes it");
    let zoom = s.screen.view.zoom_level();
    choose(&mut s, MENU_VIEW_ID, ZOOM_IN_ID);
    assert!(s.screen.view.zoom_level() != zoom);
    // The cities': the list of all, as A.
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    assert!(s.city_list().is_some_and(|l| !l.own()));
    // The keys for them.
    type_char(&mut s, 'a');
    assert!(s.city_list().is_none());
    type_char(&mut s, 'm');
    assert!(s.city_list().is_some_and(|l| l.own()));
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_list().is_none());
    // O opens the selected hex's city.
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    type_char(&mut s, 'o');
    assert_eq!(s.city_window().map(|w| w.city()), s.screen.game.map.city_at(REDHOLD).map(|c| c.id));
}

#[test]
fn the_help_menu_has_the_keys_and_the_rules_of_this_game() {
    let mut s = session(|setup| {
        setup.combat.chance = 3;
        setup.combat.system = System::Strength;
        setup.fog = Fog::On;
    });
    choose(&mut s, MENU_HELP_ID, HELP_KEYS_ID);
    assert!(s.screen.help_open());
    assert_eq!(s.screen.help_title(), Some("Help: keys and mouse"));
    // A topic stands for what the help showed.
    choose(&mut s, MENU_HELP_ID, "help_battles");
    assert_eq!(s.screen.help_title(), Some("Help: battles"));
    let (_, battles) = guide::topic(&s.screen.game, "help_battles").unwrap();
    let words: Vec<&str> = battles.iter().flat_map(|sec| sec.rows.iter().map(|(_, w)| w.as_str())).collect();
    let all = words.join(" ");
    assert!(all.contains("Strength against strength"), "{all}");
    assert!(all.contains("3 of 10") && all.contains("87%"), "{all}");
    // Every topic says something, in lines that fit the window.
    for (id, _) in guide::TOPICS {
        let (_, sections) = guide::topic(&s.screen.game, id).unwrap();
        let rows: Vec<&(String, String)> = sections.iter().flat_map(|sec| &sec.rows).collect();
        assert!(!rows.is_empty() && rows.iter().all(|(_, w)| w.chars().count() <= 64), "{id}");
    }
    let (_, sight) = guide::topic(&s.screen.game, "help_fog").unwrap();
    assert!(sight.iter().flat_map(|sec| &sec.rows).any(|(_, w)| w.starts_with("On in this game")));
    let (_, movement) = guide::topic(&s.screen.game, "help_movement").unwrap();
    assert!(movement.iter().flat_map(|sec| &sec.rows).any(|(k, w)| k == "Soldiers" && w.contains("12 moves")));
    // Esc closes it.
    press(&mut s, SpecialKey::Esc);
    assert!(!s.screen.help_open());
}

#[test]
fn h_goes_to_the_capital_and_hotseat_turns_start_there() {
    let mut s = session(|_| {});
    s.screen.game.set_cursor(Hex::new(19, 0));
    type_char(&mut s, 'h');
    assert_eq!((s.shown(), s.screen.game.map.cursor), (REDHOLD, REDHOLD));
    choose(&mut s, MENU_VIEW_ID, CAPITAL_ID);
    assert_eq!(s.shown(), REDHOLD);

    // Two people at one screen: each turn starts at its side's capital.
    let mut s = session(|setup| setup.set_players("red=person,blue=person").unwrap());
    push_widget(&mut s, BEGIN_TURN_ID);
    assert_eq!(s.shown(), REDHOLD);
    s.screen.game.set_cursor(Hex::new(0, 19));
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue);
    assert_eq!((s.shown(), s.screen.game.map.cursor), (BLUEMOOR, BLUEMOOR));
    assert!(s.screen.view.shows(BLUEMOOR));
}

#[test]
fn the_menus_items_do_not_need_the_mouse() {
    // Every item but the help's topics has its keys, and they are the ones
    // that do it (spot checks: Ctrl+Z is Undo, F1 the help).
    let mut s = session(|_| {});
    for (menu, _) in MENU_TITLES {
        push_widget(&mut s, menu);
        for (id, keys) in items(&s) {
            assert!(!keys.is_empty() || id.starts_with("help_"), "{menu}: {id} has no keys");
        }
        press(&mut s, SpecialKey::Esc);
    }
    let mods = if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CTRL };
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let to = pixel_of(&s, Hex::new(6, 6));
    click(&mut s, to);
    s.screen.ui.key(Some('z'), SpecialKey::None, mods, true);
    s.screen.ui.key(Some('z'), SpecialKey::None, mods, false);
    s.pump();
    assert_eq!(s.screen.game.map.units_at(REDHOLD).len(), 2, "undone");
}

#[test]
fn keys_and_mouse_shows_the_keys_over_a_topic() {
    let mut s = session(|_| {});
    choose(&mut s, MENU_HELP_ID, "help_cities");
    assert_eq!(s.screen.help_title(), Some("Help: cities and gold"));
    choose(&mut s, MENU_HELP_ID, HELP_KEYS_ID);
    assert_eq!(s.screen.help_title(), Some(hexgame::screen::KEYS_TITLE));
    choose(&mut s, MENU_HELP_ID, HELP_KEYS_ID);
    assert!(!s.screen.help_open(), "the keys again close them");
}

#[test]
fn esc_closes_the_window_on_top_first() {
    let mut s = session(|_| {});
    let log_open = |s: &Session| s.screen.ui.find(s.screen.ui.root(), TbId::new(LOG_WINDOW_ID)).is_some();
    type_char(&mut s, 'l');
    type_char(&mut s, 'a');
    assert!(log_open(&s) && s.city_list().is_some());
    // The list came last: it goes first.
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_list().is_none() && log_open(&s));
    press(&mut s, SpecialKey::Esc);
    assert!(!log_open(&s));
}

#[test]
fn new_game_asks_while_the_game_goes_on_and_not_once_it_is_over() {
    let mut s = session(|_| {});
    choose(&mut s, MENU_GAME_ID, hexgame::session::NEW_GAME_ASK_ID);
    assert!(s.asks_new_game() && !s.wants_new_game(), "it asks first");
    push_widget(&mut s, hexgame::session::KEEP_PLAYING_ID);
    assert!(!s.asks_new_game() && !s.wants_new_game());
    // Ctrl+N (Cmd+N) too; Esc says no.
    let mods = if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CTRL };
    s.screen.ui.key(Some('n'), SpecialKey::None, mods, true);
    s.screen.ui.key(Some('n'), SpecialKey::None, mods, false);
    s.pump();
    assert!(s.asks_new_game());
    press(&mut s, SpecialKey::Esc);
    assert!(!s.asks_new_game() && !s.wants_new_game());
    choose(&mut s, MENU_GAME_ID, hexgame::session::NEW_GAME_ASK_ID);
    push_widget(&mut s, hexgame::session::NEW_GAME_ID);
    assert!(s.wants_new_game(), "leaving it");

    // Over: straight to a new game. Red takes blue's only city.
    let mut s = session(|_| {});
    let game = &mut s.screen.game;
    let red = game.map.units_at(REDHOLD)[0];
    let blue = game.map.units_at(BLUEMOOR)[0];
    game.map.kill_unit(blue);
    game.map.move_unit(red, Hex::new(15, 14)).unwrap();
    game.apply(hexgame::orders::Order::Move { units: vec![red], to: BLUEMOOR }).unwrap();
    assert_eq!(s.screen.game.winner(), Some(Side::Red));
    s.frame(16);
    choose(&mut s, MENU_GAME_ID, hexgame::session::NEW_GAME_ASK_ID);
    assert!(s.wants_new_game() && !s.asks_new_game());
}

#[test]
fn the_new_game_question_goes_the_way_of_the_other_windows() {
    let mut s = session(|_| {});
    let log_open = |s: &Session| s.screen.ui.find(s.screen.ui.root(), TbId::new(LOG_WINDOW_ID)).is_some();
    s.ask_new_game();
    type_char(&mut s, 'l');
    assert!(s.asks_new_game() && log_open(&s));
    // The log came last: Esc closes it first.
    press(&mut s, SpecialKey::Esc);
    assert!(s.asks_new_game() && !log_open(&s));
    // When the game ends, the question goes.
    let game = &mut s.screen.game;
    let red = game.map.units_at(REDHOLD)[0];
    let blue = game.map.units_at(BLUEMOOR)[0];
    game.map.kill_unit(blue);
    game.map.move_unit(red, Hex::new(15, 14)).unwrap();
    game.apply(hexgame::orders::Order::Move { units: vec![red], to: BLUEMOOR }).unwrap();
    s.frame(16);
    assert!(!s.asks_new_game());
}
