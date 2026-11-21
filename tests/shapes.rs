//! Maps need not be square: wide ones, tall ones and odd sizes go through the
//! whole game, from the file to the screen, with rows and columns never taken
//! for each other. No window and no GPU.

mod common;

use common::{assets, click, press};
use hexgame::render::{CITY_SIDES, HexVariant, MapGeometry, Marks, RANGE_OVERLAY};
use hexgame::script;
use hexgame::session::Session;
use hexgame::state::{Hex, Map, TileType};
use turbo_ferret::{Point, Size, SpecialKey};

/// (rows, columns): wide, tall, odd, and the two extremes.
const SHAPES: [(u16, u16); 5] = [(20, 75), (75, 20), (23, 38), (20, 600), (600, 20)];

/// A terrain for every hex that tells a row from a column.
fn terrain_at(row: u16, col: u16) -> TileType {
    TileType::ALL[(usize::from(row) * 7 + usize::from(col) * 3) % TileType::ALL.len()]
}

/// A map with [`terrain_at`] everywhere.
fn patterned(rows: u16, cols: u16) -> Map {
    let mut map = Map::new(rows, cols, TileType::Grass).unwrap();
    for row in 0..rows {
        for col in 0..cols {
            map.tile_mut(Hex::new(row, col)).unwrap().kind = terrain_at(row, col);
        }
    }
    map
}

/// The game on an all-grass map of this shape (a rider in its middle), at
/// 1024 x 768.
fn session_on(rows: u16, cols: u16) -> Session {
    let game = common::rider_on(Map::new(rows, cols, TileType::Grass).unwrap());
    common::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

#[test]
fn a_map_has_the_rows_and_columns_it_was_given() {
    for (rows, cols) in SHAPES {
        let map = patterned(rows, cols);
        assert_eq!((map.rows(), map.cols()), (rows, cols));
        // The four corners are on it, and one step past each side is not.
        for hex in [Hex::new(0, 0), Hex::new(0, cols - 1), Hex::new(rows - 1, 0), Hex::new(rows - 1, cols - 1)] {
            assert!(map.contains(hex) && map.tile(hex).is_some(), "{rows} x {cols}: {hex}");
        }
        assert!(!map.contains(Hex::new(rows, 0)) && map.tile(Hex::new(rows, 0)).is_none());
        assert!(!map.contains(Hex::new(0, cols)) && map.tile(Hex::new(0, cols)).is_none());
        // The far corner the other way round is off the map (or it is square).
        assert!(!map.contains(Hex::new(cols - 1, rows - 1)), "{rows} x {cols}");
        // Every hex holds what was put there: no two share a tile.
        for row in 0..rows {
            for col in 0..cols {
                assert_eq!(map.tile(Hex::new(row, col)).unwrap().kind, terrain_at(row, col), "({row}, {col})");
            }
        }
    }
}

#[test]
fn the_map_file_keeps_the_shape() {
    for (rows, cols) in SHAPES {
        let map = patterned(rows, cols);
        let text = map.to_text();
        let lines: Vec<&str> = text.lines().collect();
        // The rows first, then the columns; a line for each row, a letter for each column.
        assert_eq!(lines[0], rows.to_string());
        assert_eq!(lines[1], cols.to_string());
        assert_eq!(lines.len(), 2 + usize::from(rows));
        assert!(lines[2..].iter().all(|line| line.len() == usize::from(cols)), "{rows} x {cols}");
        assert_eq!(lines[2].chars().nth(1), Some(terrain_at(0, 1).to_char()));
        assert_eq!(lines[3].chars().next(), Some(terrain_at(1, 0).to_char()));

        let back = Map::parse(&text).unwrap();
        assert_eq!((back.rows(), back.cols()), (rows, cols));
        assert_eq!(back.to_text(), text);
        // The same letters with the two numbers the other way round are not
        // this map: a wide map's rows are too long for it (an error), and a
        // tall map's are short (the reader fills them up with grass).
        let turned = format!("{cols}\n{rows}\n{}\n", lines[2..].join("\n"));
        match Map::parse(&turned) {
            Err(_) => assert!(cols > rows, "{rows} x {cols}"),
            Ok(other) => {
                assert!(rows > cols, "{rows} x {cols}");
                assert_eq!((other.rows(), other.cols()), (cols, rows));
                assert_ne!(other.to_text(), text);
            }
        }
    }
}

#[test]
fn neighbours_stay_on_the_map_and_know_each_other() {
    for (rows, cols) in SHAPES {
        let map = Map::new(rows, cols, TileType::Grass).unwrap();
        let edge = |row: u16, col: u16| row == 0 || col == 0 || row == rows - 1 || col == cols - 1;
        for row in (0..rows).filter(|r| *r < 3 || *r + 3 >= rows || r % 97 == 0) {
            for col in (0..cols).filter(|c| *c < 3 || *c + 3 >= cols || c % 97 == 0) {
                let hex = Hex::new(row, col);
                let around: Vec<Hex> = map.neighbours(hex).collect();
                assert!(around.iter().all(|n| map.contains(*n)), "{rows} x {cols}: {hex}");
                assert!(around.iter().all(|n| map.neighbours(*n).any(|back| back == hex)), "{hex}");
                assert_eq!(around.len() == 6, !edge(row, col), "{rows} x {cols}: {hex} has {}", around.len());
            }
        }
    }
}

#[test]
fn a_unit_moves_on_a_map_of_any_shape() {
    for (rows, cols) in SHAPES {
        let mut game = common::rider_on(Map::new(rows, cols, TileType::Grass).unwrap());
        let start = game.map.cursor;
        assert_eq!(start, Hex::new(rows / 2, cols / 2), "{rows} x {cols}: the unit starts in the middle");
        game.select_hex(start);
        assert!(game.selected().is_some());
        // Where it can go is on the map, around it, on both axes.
        let mut reach = 0;
        for row in 0..rows {
            for col in 0..cols {
                reach += usize::from(game.can_move_to(Hex::new(row, col)));
            }
        }
        assert!(reach > 100, "{rows} x {cols}: {reach}");
        // 20 points at 2 a hex: ten hexes along the map's long side, or to
        // its edge along the short one.
        let far = if cols > rows { Hex::new(start.row, start.col + 10) } else { Hex::new(start.row + 10, start.col) };
        assert!(game.can_move_to(far), "{rows} x {cols}: {far}");
        game.select_hex(far);
        assert_eq!(game.map.units_at(far).len(), 1, "{rows} x {cols}: the unit went to {far}");
        assert!(game.map.units_at(start).is_empty());
    }
}

#[test]
fn the_views_reach_every_corner() {
    for (rows, cols) in SHAPES {
        let mut s = session_on(rows, cols);
        let corners = [Hex::new(0, 0), Hex::new(0, cols - 1), Hex::new(rows - 1, 0), Hex::new(rows - 1, cols - 1)];
        // The minimap shows the whole map inside its widget, and a click on a
        // corner of it goes to that corner (to within a hex or two: on the
        // widest maps a hex is less than a pixel).
        let mini = s.screen.minimap_rect();
        for corner in corners {
            let p = s.screen.minimap_pixel(corner);
            assert!(mini.contains(p), "{rows} x {cols}: {corner} is at {p:?}, outside the minimap");
            click(&mut s, p);
            let at = s.screen.game.map.cursor;
            let slack = (rows.max(cols) / 100).max(1);
            assert!(
                at.row.abs_diff(corner.row) <= slack && at.col.abs_diff(corner.col) <= slack,
                "{rows} x {cols}: the minimap's {corner} went to {at}"
            );
            assert!(s.screen.view.shows(at));
        }
        // The cursor jumps to each corner, the main map shows and picks it,
        // and the arrows stop at the edges.
        for corner in corners {
            script::run(&mut s, &format!("cursor:{}:{}", corner.row, corner.col)).unwrap();
            let p = s.screen.hex_pixel(corner).unwrap_or_else(|| panic!("{rows} x {cols}: {corner} is not in view"));
            click(&mut s, p);
            assert_eq!(s.screen.game.map.cursor, corner, "{rows} x {cols}");
            for key in [SpecialKey::Left, SpecialKey::Right, SpecialKey::Up, SpecialKey::Down] {
                press(&mut s, key);
                assert!(s.screen.game.map.contains(s.screen.game.map.cursor), "{rows} x {cols}: {key:?}");
            }
        }
        // Scrolling far enough stops with the far corner in the widget's
        // corner: the widget's edges on the map's (or, along an axis where the
        // map is smaller than the widget, the map in its middle).
        let edges = |s: &Session| {
            let ([x, y], (hw, hh)) = (s.screen.view.middle(), s.screen.view.half_extent());
            [x - hw, x + hw, y + hh, y - hh]
        };
        // The widget from `a` to `b` over the map from `lo` to `hi`, at the
        // map's edge `edge` (one of the two).
        let stops = |a: f32, b: f32, lo: f32, hi: f32, edge: f32| {
            if b - a > hi - lo {
                ((a + b) / 2.0 - (lo + hi) / 2.0).abs() < 1e-2
            } else {
                (a - edge).abs() < 1e-2 || (b - edge).abs() < 1e-2
            }
        };
        let (map_right, map_bottom) = (1.5 * f32::from(cols - 1) + 0.5, -2.0 * f32::from(rows - 1));
        let p = Point::new(mini.x - 300, mini.y + 100);
        // In view, or on its edge: the view stops at the middle of the last
        // row's lower hexes, which a rounding puts a hair inside or outside.
        let near = |s: &Session, hex: Hex| {
            let ([x, y], (hw, hh)) = (s.screen.view.hex_center(hex), s.screen.view.half_extent());
            x.abs() <= hw + 1e-3 && y.abs() <= hh + 1e-3
        };
        assert!(s.screen.scroll(p, -1.0e6, -1.0e6), "{rows} x {cols}: the pointer is on the main map");
        let [left, right, top, bottom] = edges(&s);
        assert!(stops(left, right, -0.5, map_right, map_right), "{rows} x {cols}: {:?}", edges(&s));
        assert!(stops(bottom, top, map_bottom, 1.0, map_bottom), "{rows} x {cols}: {:?}", edges(&s));
        assert!(near(&s, Hex::new(rows - 1, cols - 2)), "{rows} x {cols}");
        assert!(s.screen.scroll(p, 1.0e6, 1.0e6));
        let [left, right, top, bottom] = edges(&s);
        assert!(stops(left, right, -0.5, map_right, -0.5), "{rows} x {cols}: {:?}", edges(&s));
        assert!(stops(bottom, top, map_bottom, 1.0, 1.0), "{rows} x {cols}: {:?}", edges(&s));
        assert!(s.screen.view.shows(Hex::new(0, 0)), "{rows} x {cols}");
        // What is in view is on the map, at every zoom.
        for _ in 0..5 {
            assert!(s.screen.view.visible_hexes().all(|v| s.screen.game.map.contains(v.hex)), "{rows} x {cols}");
            s.screen.view.zoom_out();
        }
    }
}

#[test]
fn the_maps_edges_are_cut_straight_on_all_four_sides() {
    for (rows, cols) in SHAPES {
        // The hexes of the edges are drawn cut: the corner hexes' models.
        let (last_row, last_col) = (rows - 1, cols - 1);
        assert_eq!(HexVariant::choose(0, 0, rows, cols), HexVariant::Left);
        assert_eq!(HexVariant::choose(5, 5, rows, cols), HexVariant::Full);
        assert_eq!(HexVariant::choose(last_row, 0, rows, cols), HexVariant::BottomLeft);
        // Both test shapes' last columns are odd (they sit higher: cut at the
        // top of the map, whole at its bottom) or even (the other way round).
        let (top_right, bottom_right) = if last_col % 2 == 1 {
            (HexVariant::TopRight, HexVariant::Right)
        } else {
            (HexVariant::Right, HexVariant::BottomRight)
        };
        assert_eq!(HexVariant::choose(0, last_col, rows, cols), top_right, "{rows} x {cols}");
        assert_eq!(HexVariant::choose(last_row, last_col, rows, cols), bottom_right, "{rows} x {cols}");

        // And the geometry of the far corner's view is inside the map's edges.
        let map = patterned(rows, cols);
        let mut view = hexgame::view::MapView::new(rows, cols, Hex::new(last_row, last_col));
        view.resize(819, 537);
        let mut geometry = MapGeometry::new();
        geometry.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));
        // Every hex once: its terrain's, or its city's.
        let layers: Vec<_> = TileType::ALL
            .into_iter()
            .map(|kind| geometry.terrain(kind))
            .chain(CITY_SIDES.into_iter().map(|side| geometry.cities(side)))
            .collect();
        let drawn: usize = layers.iter().map(|layer| layer.vertex_count()).sum();
        assert_eq!(drawn, view.visible_hexes().count() * 7, "{rows} x {cols}");
        // Nothing is drawn right of the last column's cut or below the last
        // row's: half a unit right of the last column's middle, and through the
        // middle of the last row's even columns.
        let right = view.hex_center(Hex::new(last_row, last_col))[0] + 0.5;
        let bottom = view.hex_center(Hex::new(last_row, 0))[1];
        let [right, bottom] = view.to_ndc([right, bottom]);
        for layer in &layers {
            for mesh in layer.meshes() {
                for v in &mesh.vertices {
                    assert!(v.pos[0] <= right + 1e-5 && v.pos[1] >= bottom - 1e-5, "{rows} x {cols}: {:?}", v.pos);
                }
            }
        }
    }
}
