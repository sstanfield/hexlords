//! Where a battle's window goes: clear of the hex fought over, wherever on
//! the map that is.

mod common;

use common::{assets, click, pixel_of};
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::session::Session;
use hexlords::setup::{Fog, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side};
use turbo_ferret::Size;

/// 40 x 40 grass, red's and blue's cities in the middle; red's soldiers on
/// `from`, blue's levy on `at`.
fn game(from: Hex, at: Hex) -> Game {
    let mut rows = vec!["g".repeat(40); 40];
    rows[20] = format!("{}c{}c{}", "g".repeat(10), "g".repeat(18), "g".repeat(10));
    let terrain = format!("40\n40\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 20 10\nname Redhold\nside red\ncity 20 29\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    for (side, kind, hex) in [(Side::Red, "soldiers", from), (Side::Blue, "levy", at)] {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.fog, setup.combat.chance) = (Fog::Off, 0);
    game.with_setup(setup)
}

/// The owner's word, 2026-11-22: "a battle at the top of the map has the
/// battle hex covered by the combat dialog". Wherever the battle is (the
/// map's edges and corners, where the map cannot be moved under the
/// window), in a small window and a big one, zoomed in and out (a map all
/// in view does not move at all): the hex shows, whole, beside the window.
#[test]
fn the_battles_window_never_covers_the_hex_fought_over() {
    let fields = [(0, 4), (1, 20), (2, 38), (3, 1), (4, 12), (6, 20), (20, 0), (20, 39), (39, 21), (37, 3), (15, 15)];
    let mut covered = Vec::new();
    for (w, h, dpi) in [(1024, 768, 96), (800, 600, 96), (1600, 900, 96), (2048, 1536, 192), (2880, 1800, 192)] {
        for zoom in 0..4 {
            for at in fields.map(|(row, col)| Hex::new(row, col)) {
                let from = if at.row == 39 { Hex::new(38, at.col) } else { Hex::new(at.row + 1, at.col) };
                let options = Options { show_moves: true, ..Options::manual() };
                let mut s = Session::with_options(&assets(), Size::new(w, h), dpi, game(from, at), options).unwrap();
                for _ in 0..zoom {
                    s.screen.view.zoom_out();
                }
                s.screen.view.center_on(from);
                for hex in [from, at] {
                    let p = pixel_of(&s, hex);
                    click(&mut s, p);
                }
                let mut t = 0;
                while s.battle().is_none() && t < 20_000 {
                    t += 20;
                    s.frame(t);
                }
                let window = s.screen.ui.rect(s.battle().expect("the battle's window").window());
                let (p, map) = (pixel_of(&s, at), s.screen.map_rect());
                // (Half a hex's height, in pixels: the hex around its middle.)
                let half = (map.h as f32 * s.screen.view.to_ndc([0.0, 1.0])[1].abs()) as i32 / 2;
                let over = p.x + half > window.x
                    && p.x - half < window.x + window.w
                    && p.y + half > window.y
                    && p.y - half < window.y + window.h;
                if over || !s.screen.view.shows(at) {
                    covered.push(format!("{w} x {h}, zoom {zoom}, {at}: the hex at {p:?}, the window {window:?}"));
                }
                // The window is on the screen, under the menus.
                assert!(window.y >= map.y && window.x >= 0 && window.x + window.w <= w, "{window:?}");
            }
        }
    }
    assert!(covered.is_empty(), "{}", covered.join("\n"));
}
