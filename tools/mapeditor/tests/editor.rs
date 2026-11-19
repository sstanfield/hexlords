//! The editor, driven the way the window drives it: no window and no GPU, an
//! `Editor` on the repository's assets, fed pointer and key input.

use std::path::PathBuf;

use hexgame::screen::Scene;
use hexgame::script;
use hexgame::state::{Hex, Map, TileType};
use mapeditor::editor::{Editor, Tool};
use turbo_ferret::{Modifiers, Point, Rect, Size, SpecialKey, TbId, WidgetId};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// A 40 x 40 grass map in a 1280 x 800 window at 96 DPI.
fn editor() -> Editor {
    editor_on(Map::new(40, 40, TileType::Grass).unwrap())
}

fn editor_on(map: Map) -> Editor {
    Editor::new(&assets(), Size::new(1280, 800), 96, map, None).expect("the editor starts")
}

fn widget(e: &Editor, name: &str) -> WidgetId {
    e.screen.ui.find(e.screen.ui.root(), TbId::new(name)).unwrap_or_else(|| panic!("no widget {name}"))
}

fn rect(e: &Editor, name: &str) -> Rect {
    e.screen.area(widget(e, name))
}

fn middle(r: Rect) -> Point {
    Point::new(r.x + r.w / 2, r.y + r.h / 2)
}

fn pixel(e: &Editor, row: u16, col: u16) -> Point {
    e.screen.hex_pixel(Hex::new(row, col)).unwrap_or_else(|| panic!("hex {row},{col} is not in view"))
}

fn click(e: &mut Editor, p: Point) {
    e.screen.ui.pointer_move(p, Modifiers::NONE);
    e.screen.ui.pointer_down(p, 1, Modifiers::NONE);
    e.screen.ui.pointer_up(p, Modifiers::NONE);
    e.pump();
}

/// Click a button of the sidebar.
fn push(e: &mut Editor, name: &str) {
    let p = middle(rect(e, name));
    click(e, p);
}

fn key(e: &mut Editor, ch: char, mods: Modifiers) {
    e.screen.ui.key(Some(ch), SpecialKey::None, mods, true);
    e.screen.ui.key(Some(ch), SpecialKey::None, mods, false);
    e.pump();
}

fn type_char(e: &mut Editor, ch: char) {
    key(e, ch, Modifiers::NONE);
}

fn special(e: &mut Editor, k: SpecialKey) {
    e.screen.ui.key(None, k, Modifiers::NONE, true);
    e.screen.ui.key(None, k, Modifiers::NONE, false);
    e.pump();
}

fn kind(e: &Editor, row: u16, col: u16) -> TileType {
    e.map().tile(Hex::new(row, col)).unwrap().kind
}

fn count(e: &Editor, kind: TileType) -> usize {
    let map = e.map();
    let hexes = (0..map.rows()).flat_map(|r| (0..map.cols()).map(move |c| Hex::new(r, c)));
    hexes.filter(|h| map.tile(*h).unwrap().kind == kind).count()
}

/// True if the button shows as the chosen one of its group.
fn chosen(e: &Editor, name: &str) -> bool {
    e.screen.ui.value(widget(e, name)).as_int() == 1
}

fn set_file(e: &mut Editor, path: &std::path::Path) {
    let field = widget(e, "file_name");
    e.screen.ui.set_text(field, &path.display().to_string());
}

/// A file for this test under the build's temporary directory.
fn temp_file(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn the_map_fills_the_window_beside_the_sidebar_and_above_the_status_line() {
    let mut e = editor();
    for (w, h) in [(1280, 800), (2560, 1600), (900, 620)] {
        e.resize(Size::new(w, h));
        e.frame(16);
        let (map, mini, sidebar, status) =
            (e.screen.map_rect(), e.screen.minimap_rect(), rect(&e, "sidebar"), rect(&e, "status"));
        assert_eq!((map.x, map.y), (0, 0), "{w}x{h}");
        assert_eq!((sidebar.x, sidebar.y), (map.w + 5, 0), "{w}x{h}");
        assert_eq!(sidebar.x + sidebar.w + 5, w, "a spacing is left at the window's edge, {w}x{h}");
        assert_eq!((mini.x, mini.y, mini.w), (sidebar.x, 0, sidebar.w), "{w}x{h}");
        assert_eq!(status.y, map.h + 5, "{w}x{h}");
        assert_eq!(status.y + status.h, h, "{w}x{h}");
        assert_eq!(e.screen.view.size(), (map.w, map.h), "{w}x{h}");
        // Everything in the sidebar is inside it.
        for name in ["terrain_g", "terrain_r", "tool_fill", "brush_19", "redo", "file_name", "open", "new"] {
            let r = rect(&e, name);
            assert!(r.x >= sidebar.x && r.x + r.w <= sidebar.x + sidebar.w, "{name} at {w}x{h}: {r:?}");
            assert!(r.y >= mini.y + mini.h && r.y + r.h <= sidebar.y + sidebar.h, "{name} at {w}x{h}: {r:?}");
        }
    }
}

#[test]
fn the_minimap_takes_the_maps_shape() {
    let tall = editor_on(Map::new(30, 40, TileType::Grass).unwrap());
    let wide = editor_on(Map::new(20, 80, TileType::Grass).unwrap());
    let (tall, wide) = (tall.screen.minimap_rect(), wide.screen.minimap_rect());
    assert_eq!(tall.w, wide.w);
    assert!(wide.h * 3 < tall.h, "a wide map gets a low minimap: {wide:?} against {tall:?}");
    // The wide one fits exactly: no margins.
    let shape = wide.h as f32 / wide.w as f32;
    assert!((shape - 39.0 * hexgame::view::HEX_HEIGHT_ADJUST / 119.5).abs() < 0.01, "{shape}");
}

#[test]
fn a_terrain_button_chooses_the_brush_and_a_click_paints() {
    let mut e = editor();
    assert_eq!((e.brush(), e.tool(), e.radius()), (TileType::Grass, Tool::Paint, 0));
    assert!(chosen(&e, "terrain_g") && chosen(&e, "tool_paint") && chosen(&e, "brush_1"));
    assert!(!e.modified());

    push(&mut e, "terrain_w");
    assert_eq!(e.brush(), TileType::Water);
    assert!(chosen(&e, "terrain_w") && !chosen(&e, "terrain_g"));
    assert!(!e.modified(), "choosing a brush changes nothing");

    let p = pixel(&e, 18, 22);
    click(&mut e, p);
    assert_eq!(kind(&e, 18, 22), TileType::Water);
    assert_eq!(count(&e, TileType::Water), 1);
    assert_eq!(e.map().cursor, Hex::new(18, 22));
    assert!(e.modified());
    assert!(e.status().contains("row 18, column 22: water"), "{}", e.status());
    assert!(e.status().contains("Paint water, 1") && e.status().contains("no file (changed)"), "{}", e.status());
}

#[test]
fn a_terrains_letter_chooses_it_and_paints_at_the_cursor() {
    let mut e = editor();
    let p = pixel(&e, 20, 20);
    e.screen.ui.pointer_move(p, Modifiers::NONE);
    e.pump();
    for (ch, terrain, button) in [
        ('f', TileType::Forest, "terrain_f"),
        ('h', TileType::Hill, "terrain_h"),
        ('m', TileType::Mountain, "terrain_m"),
        ('w', TileType::Water, "terrain_w"),
        ('s', TileType::Swamp, "terrain_s"),
        ('d', TileType::Desert, "terrain_d"),
        ('c', TileType::City, "terrain_c"),
        ('t', TileType::Temple, "terrain_t"),
        ('r', TileType::Ruin, "terrain_r"),
        ('G', TileType::Grass, "terrain_g"),
    ] {
        type_char(&mut e, ch);
        assert_eq!(kind(&e, 20, 20), terrain, "{ch}");
        assert_eq!(e.brush(), terrain, "{ch}");
        assert!(chosen(&e, button), "{ch} shows in the sidebar");
    }
    // Keys that are no terrain do nothing, and Alt+letter is not a letter.
    type_char(&mut e, 'x');
    key(&mut e, 'w', Modifiers::ALT);
    assert_eq!((kind(&e, 20, 20), e.brush()), (TileType::Grass, TileType::Grass));
}

#[test]
fn enter_and_space_use_the_tool_at_the_cursor() {
    let mut e = editor();
    push(&mut e, "terrain_h");
    special(&mut e, SpecialKey::Right);
    special(&mut e, SpecialKey::Enter);
    assert_eq!(kind(&e, 20, 21), TileType::Hill);
    special(&mut e, SpecialKey::Down);
    type_char(&mut e, ' ');
    assert_eq!(kind(&e, 21, 21), TileType::Hill);
    assert_eq!(count(&e, TileType::Hill), 2);
}

#[test]
fn a_drag_paints_every_hex_on_its_way_as_one_step() {
    let mut e = editor();
    push(&mut e, "terrain_f");
    // One pointer move across twelve columns: nothing in between is skipped.
    let (from, to) = (pixel(&e, 20, 14), pixel(&e, 20, 26));
    e.screen.ui.pointer_move(from, Modifiers::NONE);
    e.screen.ui.pointer_down(from, 1, Modifiers::NONE);
    e.screen.ui.pointer_move(to, Modifiers::NONE);
    e.pump();
    for col in 14..=26 {
        // The straight line runs through the middle of row 20 in the even
        // columns, and in the odd ones (which sit half a hex higher) along the
        // edge between rows 20 and 21.
        let painted = kind(&e, 20, col) == TileType::Forest || kind(&e, 21, col) == TileType::Forest;
        assert!(painted, "column {col}");
    }
    let in_the_row = count(&e, TileType::Forest);
    assert!((13..=20).contains(&in_the_row), "{in_the_row}");
    // The drag goes on, down and out of the map: it paints its way to the
    // map's edge, and outside it nothing happens to the cursor.
    let outside = middle(rect(&e, "status"));
    e.screen.ui.pointer_move(outside, Modifiers::NONE);
    e.screen.ui.pointer_up(outside, Modifiers::NONE);
    e.pump();
    let painted = count(&e, TileType::Forest);
    assert!(painted > in_the_row, "{painted}");
    assert!(e.screen.hex_pixel(e.map().cursor).is_some(), "the cursor stayed in view");

    // A move without the button down paints nothing.
    let p = pixel(&e, 25, 20);
    e.screen.ui.pointer_move(p, Modifiers::NONE);
    e.pump();
    assert_eq!(kind(&e, 25, 20), TileType::Grass);

    // The whole drag is one step.
    push(&mut e, "undo");
    assert_eq!(count(&e, TileType::Forest), 0);
    push(&mut e, "redo");
    assert_eq!(count(&e, TileType::Forest), painted);
}

#[test]
fn the_brush_has_three_sizes() {
    let mut e = editor();
    push(&mut e, "terrain_m");
    push(&mut e, "brush_7");
    assert_eq!(e.radius(), 1);
    let p = pixel(&e, 20, 20);
    click(&mut e, p);
    assert_eq!(count(&e, TileType::Mountain), 7);
    assert!(e.map().neighbours(Hex::new(20, 20)).all(|h| e.map().tile(h).unwrap().kind == TileType::Mountain));
    // The preview marks what a press would paint.
    assert!(e.marked(Hex::new(20, 20)) && e.marked(Hex::new(19, 20)) && !e.marked(Hex::new(17, 20)));

    push(&mut e, "brush_19");
    click(&mut e, p);
    assert_eq!(count(&e, TileType::Mountain), 19);
    assert!(e.marked(Hex::new(18, 20)));
    assert!(e.status().contains("Paint mountain, 19"), "{}", e.status());

    // The bracket keys step through the sizes and stop at the ends.
    type_char(&mut e, '[');
    assert!(e.radius() == 1 && chosen(&e, "brush_7"));
    type_char(&mut e, '[');
    type_char(&mut e, '[');
    assert!(e.radius() == 0 && chosen(&e, "brush_1"));
    assert!(!e.marked(Hex::new(20, 20)), "one hex needs no preview: the cursor shows it");
    for _ in 0..4 {
        type_char(&mut e, ']');
    }
    assert!(e.radius() == 2 && chosen(&e, "brush_19"));
}

#[test]
fn fill_floods_the_connected_hexes_of_one_terrain() {
    let mut map = Map::new(40, 40, TileType::Grass).unwrap();
    // A lake, four by four, away from the edges.
    for row in 18..22 {
        for col in 18..22 {
            map.tile_mut(Hex::new(row, col)).unwrap().kind = TileType::Water;
        }
    }
    let mut e = editor_on(map);
    push(&mut e, "tool_fill");
    push(&mut e, "terrain_s");
    assert!(e.tool() == Tool::Fill && chosen(&e, "tool_fill"));
    let p = pixel(&e, 19, 19);
    click(&mut e, p);
    assert_eq!((count(&e, TileType::Swamp), count(&e, TileType::Water)), (16, 0));
    assert_eq!(count(&e, TileType::Grass), 1600 - 16);
    assert!(e.status().contains("Fill with swamp"), "{}", e.status());

    // The land around it, with Enter at the cursor. One undo each.
    special(&mut e, SpecialKey::Up);
    special(&mut e, SpecialKey::Up);
    special(&mut e, SpecialKey::Up);
    push(&mut e, "terrain_d");
    special(&mut e, SpecialKey::Enter);
    assert_eq!(count(&e, TileType::Desert), 1600 - 16);
    key(&mut e, 'z', Modifiers::CTRL);
    assert_eq!(count(&e, TileType::Desert), 0);
    key(&mut e, 'z', Modifiers::CTRL);
    assert_eq!(count(&e, TileType::Water), 16);
}

#[test]
fn undo_and_redo_by_key_and_by_button() {
    let mut e = editor();
    let enabled = |e: &Editor, name: &str| !e.screen.ui.node(widget(e, name)).unwrap().is_disabled();
    assert!(!enabled(&e, "undo") && !enabled(&e, "redo"));
    key(&mut e, 'z', Modifiers::CTRL);
    assert!(e.message().contains("Nothing to undo"));

    for (i, ch) in ['w', 'f', 'h'].into_iter().enumerate() {
        special(&mut e, SpecialKey::Right);
        type_char(&mut e, ch);
        assert_eq!(kind(&e, 20, 21 + i as u16), TileType::from_char(ch).unwrap());
    }
    assert!(enabled(&e, "undo") && !enabled(&e, "redo"));

    key(&mut e, 'z', Modifiers::CTRL);
    assert_eq!(kind(&e, 20, 23), TileType::Grass);
    key(&mut e, 'z', Modifiers::SUPER);
    assert_eq!(kind(&e, 20, 22), TileType::Grass);
    assert!(enabled(&e, "undo") && enabled(&e, "redo"));
    key(&mut e, 'Z', Modifiers::SUPER | Modifiers::SHIFT);
    assert_eq!(kind(&e, 20, 22), TileType::Forest);
    key(&mut e, 'y', Modifiers::CTRL);
    assert_eq!(kind(&e, 20, 23), TileType::Hill);
    assert!(!enabled(&e, "redo"));
    key(&mut e, 'y', Modifiers::CTRL);
    assert!(e.message().contains("Nothing to redo"));

    // Undo, then something new: nothing is left to redo.
    push(&mut e, "undo");
    type_char(&mut e, 'm');
    push(&mut e, "redo");
    assert_eq!(kind(&e, 20, 23), TileType::Mountain);
}

#[test]
fn typing_in_a_field_does_not_paint_and_enter_gives_the_keys_back_to_the_map() {
    let mut e = editor();
    let field = middle(rect(&e, "file_name"));
    click(&mut e, field);
    assert_eq!(e.screen.ui.focused(), Some(widget(&e, "file_name")));
    for ch in "wfm.map".chars() {
        type_char(&mut e, ch);
    }
    assert_eq!(e.screen.ui.text(widget(&e, "file_name")), Some("wfm.map"));
    assert_eq!(count(&e, TileType::Grass), 1600, "the letters went into the field");
    assert_eq!(e.brush(), TileType::Grass);

    special(&mut e, SpecialKey::Enter);
    assert_eq!(e.screen.ui.focused(), Some(e.screen.map_widget()));
    type_char(&mut e, 'w');
    assert_eq!(kind(&e, 20, 20), TileType::Water);

    // A click on the map takes the keyboard back as well, and the buttons
    // never take it.
    click(&mut e, field);
    push(&mut e, "terrain_h");
    assert_eq!(e.screen.ui.focused(), Some(widget(&e, "file_name")));
    let p = pixel(&e, 22, 22);
    click(&mut e, p);
    assert_eq!(e.screen.ui.focused(), Some(e.screen.map_widget()));
}

#[test]
fn save_writes_the_map_file_and_open_reads_it() {
    let path = temp_file("editor_save.map");
    let mut e = editor();
    push(&mut e, "save");
    assert!(e.message().contains("Type a file name first"), "{}", e.message());

    special(&mut e, SpecialKey::Left);
    type_char(&mut e, 'w');
    set_file(&mut e, &path);
    key(&mut e, 's', Modifiers::CTRL);
    assert!(e.message().starts_with("Saved "), "{}", e.message());
    assert!(!e.modified());
    assert_eq!(e.path(), Some(path.as_path()));
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, e.map().to_text());
    assert_eq!(text.lines().nth(22).unwrap().chars().nth(19), Some('w'), "row 20 is the file's line 23");

    // Another editor reads it back.
    let mut other = editor_on(Map::new(20, 20, TileType::Desert).unwrap());
    set_file(&mut other, &path);
    push(&mut other, "open");
    assert!(other.message().starts_with("Opened "), "{}", other.message());
    assert_eq!((other.map().rows(), other.map().cols()), (40, 40));
    assert_eq!(kind(&other, 20, 19), TileType::Water);
    assert_eq!(count(&other, TileType::Grass), 1599);
    assert!(!other.modified());
    assert_eq!(other.screen.ui.text(widget(&other, "new_rows")), Some("40"));
    // The views are the new map's.
    other.frame(16);
    assert!(other.screen.hex_pixel(Hex::new(20, 20)).is_some());
    assert!(other.screen.take_minimap_dirty());
    // What was done to the old map cannot be undone on the new one.
    key(&mut other, 'z', Modifiers::CTRL);
    assert!(other.message().contains("Nothing to undo"));
    assert_eq!(count(&other, TileType::Grass), 1599);

    // A file that cannot be written or read says why, and nothing changes.
    set_file(&mut e, &PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no/such/dir/x.map"));
    type_char(&mut e, 'f');
    push(&mut e, "save");
    assert!(e.message().starts_with("Not saved: "), "{}", e.message());
    assert!(e.modified());
    assert_eq!(e.path(), Some(path.as_path()));
    push(&mut e, "open");
    assert!(e.message().starts_with("Not opened: "), "{}", e.message());
    assert_eq!(kind(&e, 20, 19), TileType::Forest);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn new_makes_a_map_of_the_asked_size_in_the_chosen_terrain() {
    let mut e = editor();
    let (rows, cols) = (widget(&e, "new_rows"), widget(&e, "new_cols"));
    assert_eq!(e.screen.ui.text(rows), Some("40"));
    e.screen.ui.set_text(rows, "25");
    e.screen.ui.set_text(cols, "70");
    push(&mut e, "terrain_w");
    push(&mut e, "new");
    assert_eq!((e.map().rows(), e.map().cols()), (25, 70));
    assert_eq!(count(&e, TileType::Water), 25 * 70);
    assert!(!e.modified() && e.path().is_none());
    assert!(e.status().contains("25 x 70"), "{}", e.status());
    e.frame(16);
    assert_eq!(e.map().cursor, Hex::new(12, 35));
    assert!(e.screen.hex_pixel(Hex::new(12, 35)).is_some(), "the view is on the new map's middle");

    // Sizes the map cannot have are refused, with the reason.
    e.screen.ui.set_text(rows, "10");
    push(&mut e, "new");
    assert!(e.message().starts_with("New map: invalid map dimensions"), "{}", e.message());
    e.screen.ui.set_text(rows, "");
    push(&mut e, "new");
    assert!(e.message().contains("have to be numbers"), "{}", e.message());
    assert_eq!((e.map().rows(), e.map().cols()), (25, 70));
}

#[test]
fn wide_and_tall_maps_are_made_painted_saved_and_read_back() {
    for (rows, cols) in [(20u16, 90u16), (90, 20), (27, 41)] {
        let path = temp_file(&format!("editor_{rows}x{cols}.map"));
        let mut e = editor();
        let (rows_field, cols_field) = (widget(&e, "new_rows"), widget(&e, "new_cols"));
        e.screen.ui.set_text(rows_field, &rows.to_string());
        e.screen.ui.set_text(cols_field, &cols.to_string());
        push(&mut e, "new");
        assert_eq!((e.map().rows(), e.map().cols()), (rows, cols));
        e.frame(16);

        // Paint the four corners, each with another terrain: jump there (the
        // minimap gets to within a hex), point at the hex, press the letter.
        let corners = [
            (Hex::new(0, 0), 'w', TileType::Water),
            (Hex::new(0, cols - 1), 'f', TileType::Forest),
            (Hex::new(rows - 1, 0), 'd', TileType::Desert),
            (Hex::new(rows - 1, cols - 1), 'm', TileType::Mountain),
        ];
        for (corner, letter, _) in corners {
            let p = e.screen.minimap_pixel(corner);
            click(&mut e, p);
            assert!(e.map().cursor.row.abs_diff(corner.row) <= 1 && e.map().cursor.col.abs_diff(corner.col) <= 1);
            let p = pixel(&e, corner.row, corner.col);
            e.screen.ui.pointer_move(p, Modifiers::NONE);
            e.pump();
            assert_eq!(e.map().cursor, corner, "{rows} x {cols}");
            type_char(&mut e, letter);
        }
        for (corner, _, terrain) in corners {
            assert_eq!(kind(&e, corner.row, corner.col), terrain, "{rows} x {cols}: {corner}");
        }
        assert_eq!(count(&e, TileType::Grass), usize::from(rows) * usize::from(cols) - 4);

        // The file has the shape, the report on it agrees, and it reads back.
        set_file(&mut e, &path);
        push(&mut e, "save");
        assert!(e.message().starts_with("Saved "), "{}", e.message());
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            (lines[0], lines[1], lines.len()),
            (rows.to_string().as_str(), cols.to_string().as_str(), usize::from(rows) + 2)
        );
        assert!(lines[2].starts_with('w') && lines[2].ends_with('f') && lines[2].len() == usize::from(cols));
        assert!(lines[lines.len() - 1].starts_with('d') && lines[lines.len() - 1].ends_with('m'));
        let report = mapeditor::check::check(&text).unwrap();
        assert_eq!((report.rows, report.cols, report.warnings.len()), (rows, cols, 0));

        let mut other = editor();
        set_file(&mut other, &path);
        push(&mut other, "open");
        assert_eq!((other.map().rows(), other.map().cols()), (rows, cols));
        for (corner, _, terrain) in corners {
            assert_eq!(kind(&other, corner.row, corner.col), terrain, "{rows} x {cols}: {corner}");
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn unsaved_changes_are_not_thrown_away_at_the_first_asking() {
    let path = temp_file("editor_discard.map");
    let mut e = editor();
    assert!(e.confirm_quit(), "nothing to lose");
    type_char(&mut e, 'w');

    // New and Open have to be pressed twice in a row.
    push(&mut e, "new");
    assert!(e.message().contains("unsaved changes"), "{}", e.message());
    assert_eq!(kind(&e, 20, 20), TileType::Water);
    // A change in between means asking again.
    special(&mut e, SpecialKey::Right);
    type_char(&mut e, 'f');
    push(&mut e, "new");
    assert_eq!(count(&e, TileType::Forest), 1, "still the old map");
    push(&mut e, "new");
    assert_eq!((count(&e, TileType::Forest), count(&e, TileType::Water)), (40 * 40, 0), "a new map, all forest");

    // Quit: Ctrl+Q is refused once, then let through.
    type_char(&mut e, 'h');
    key(&mut e, 'q', Modifiers::CTRL);
    assert!(e.screen.quit_requested());
    assert!(!e.confirm_quit());
    assert!(e.status().contains("Quit again to lose them"), "{}", e.status());
    e.screen.cancel_quit();
    assert!(e.confirm_quit());

    // After a save there is nothing to ask about.
    type_char(&mut e, 'm');
    assert!(!e.confirm_quit());
    set_file(&mut e, &path);
    push(&mut e, "save");
    assert!(e.confirm_quit());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_minimap_and_the_keys_move_around_as_in_the_game() {
    let mut e = editor_on(Map::new(100, 100, TileType::Grass).unwrap());
    let p = e.screen.minimap_pixel(Hex::new(10, 90));
    click(&mut e, p);
    let cursor = e.map().cursor;
    assert!(cursor.row.abs_diff(10) <= 1 && cursor.col.abs_diff(90) <= 1, "{cursor}");
    // The view shows it, as near its middle as the map's edges let it.
    assert!(e.screen.view.shows(cursor));
    special(&mut e, SpecialKey::Left);
    assert_eq!(e.map().cursor.col, cursor.col - 1);
    let zoom = e.screen.view.zoom_level();
    type_char(&mut e, '1');
    assert_eq!(e.screen.view.zoom_level(), zoom + 1);
    assert_eq!(count(&e, TileType::Grass), 100 * 100, "moving around paints nothing");
    assert!(!e.modified());
}

#[test]
fn painting_asks_for_a_new_minimap_picture() {
    let mut e = editor();
    e.frame(16);
    assert!(e.screen.take_minimap_dirty(), "the first picture");
    e.frame(32);
    assert!(!e.screen.take_minimap_dirty());
    type_char(&mut e, 'w');
    assert!(e.screen.take_minimap_dirty());
    type_char(&mut e, 'w');
    assert!(!e.screen.take_minimap_dirty(), "painting water on water changes nothing");
}

#[test]
fn scripts_drive_the_editor() {
    let mut e = editor();
    let steps = "press:terrain_m,press:brush_7,click:20:20,key:w,drag:17:16:17:20,key:ctrl+z,right*3,space";
    script::run(&mut e, steps).unwrap();
    assert_eq!(e.brush(), TileType::Water);
    assert_eq!(kind(&e, 20, 20), TileType::Water);
    assert_eq!(kind(&e, 17, 17), TileType::Grass, "the drag was undone");
    assert_eq!(e.map().cursor, Hex::new(17, 23), "three steps right of where the drag ended");
    assert_eq!(kind(&e, 17, 23), TileType::Water, "space painted there");
    assert_eq!(count(&e, TileType::Water), 14);
    assert_eq!(script::run(&mut e, "press:nothing").unwrap_err(), "press:nothing: no widget with id nothing");
    assert_eq!(
        script::run(&mut e, "key:hyper+x").unwrap_err(),
        "key:hyper+x: \"hyper\" is not a modifier (ctrl, shift, alt, cmd)"
    );
}

#[test]
fn scrolling_and_pulling_the_map_move_it_and_paint_nothing() {
    let mut e = editor();
    let p = pixel(&e, 20, 20);
    e.screen.ui.pointer_move(p, Modifiers::NONE);
    e.pump();
    let start = e.screen.view.middle();

    // Two fingers, the wheel with Ctrl, and the right button: the window's calls.
    assert!(e.screen.scroll(p, -90.0, 60.0));
    e.pump();
    assert_ne!(e.screen.view.middle(), start);
    assert!(e.screen.zoom_at(p, false));
    assert!(e.screen.grab(p));
    e.screen.drag(Point::new(p.x + 40, p.y + 40));
    e.screen.let_go();
    e.pump();
    assert_eq!(count(&e, TileType::Grass), 40 * 40, "nothing was painted");
    assert!(!e.modified());
    // A wide brush's outline is around the hex under the pointer, the cursor.
    push(&mut e, "brush_7");
    assert!(e.screen.scroll(p, 30.0, 0.0));
    e.pump();
    let cursor = e.map().cursor;
    assert_eq!(e.screen.hex_pixel(cursor).map(|at| (at.x - p.x).abs() < 30 && (at.y - p.y).abs() < 30), Some(true));
    assert!(e.marked(cursor) && e.marked(Hex::new(cursor.row + 1, cursor.col)));
    push(&mut e, "brush_1");

    // On the sidebar's minimap the view goes the same way as on the map, and
    // the wheel is not the map's over the buttons.
    let mini = middle(e.screen.minimap_rect());
    let before = e.screen.view.middle();
    assert!(e.screen.scroll(mini, -15.0, 0.0));
    assert!(e.screen.view.middle()[0] > before[0]);
    assert!(!e.screen.scroll(middle(rect(&e, "save")), 15.0, 0.0));
    assert!(!e.screen.scroll(middle(rect(&e, "file_name")), 15.0, 0.0));

    // A click paints where the map now is.
    push(&mut e, "terrain_w");
    script::run(&mut e, "scroll:-300:0").unwrap();
    let p = middle(e.screen.map_rect());
    click(&mut e, p);
    assert_eq!(count(&e, TileType::Grass), 40 * 40 - 1);
    assert_eq!(kind(&e, e.map().cursor.row, e.map().cursor.col), TileType::Water);
}

#[test]
fn the_road_tool_lays_roads_along_a_drag_and_keeps_them_under_new_terrain() {
    let mut e = editor();
    push(&mut e, "tool_road");
    assert_eq!(e.tool(), Tool::Road);
    assert!(chosen(&e, "tool_road") && !chosen(&e, "tool_paint"));
    assert!(e.status().contains("Lay road"), "{}", e.status());

    // A drag along row 20 lays a road on every hex of the way, as one step.
    script::run(&mut e, "drag:20:16:20:22").unwrap();
    let road = |e: &Editor, row: u16, col: u16| e.map().tile(Hex::new(row, col)).unwrap().road;
    let laid: Vec<Hex> =
        (0..40).flat_map(|r| (0..40).map(move |c| Hex::new(r, c))).filter(|h| road(&e, h.row, h.col)).collect();
    assert!(laid.len() >= 7 && laid.contains(&Hex::new(20, 16)) && laid.contains(&Hex::new(20, 22)), "{laid:?}");
    // A road in one piece: every hex of it joins another.
    assert!(laid.iter().all(|h| e.map().road_links(*h) != 0));
    assert_eq!(count(&e, TileType::Grass), 40 * 40, "the terrain is what it was");
    assert!(e.modified());
    assert!(e.status().contains("row 20, column 22: grass, road"), "{}", e.status());
    // The brush's size is not the road's: a road is one hex wide.
    assert!(!e.marked(Hex::new(21, 22)));

    // Water painted under it makes a bridge; the road stays. (In column 19,
    // an odd one, the drag went through the hex of row 20 or of row 21.)
    let mid = *laid.iter().find(|h| h.col == 19).expect("a road hex in column 19");
    push(&mut e, "tool_paint");
    push(&mut e, "terrain_w");
    let p = pixel(&e, mid.row, mid.col);
    click(&mut e, p);
    assert_eq!(kind(&e, mid.row, mid.col), TileType::Water);
    assert!(road(&e, mid.row, mid.col));
    assert!(e.status().contains("water, bridge"), "{}", e.status());
    assert!(e.map().to_text().contains('W'), "a capital in the file");

    // The other tool takes it away again; undo brings it back, step by step.
    push(&mut e, "tool_unroad");
    assert!(e.status().contains("Remove road"), "{}", e.status());
    click(&mut e, p);
    assert!(!road(&e, mid.row, mid.col) && road(&e, 20, 18));
    key(&mut e, 'z', Modifiers::CTRL);
    assert!(road(&e, mid.row, mid.col));
    key(&mut e, 'z', Modifiers::CTRL);
    assert_eq!(kind(&e, mid.row, mid.col), TileType::Grass);
    key(&mut e, 'z', Modifiers::CTRL);
    assert!(laid.iter().all(|h| !road(&e, h.row, h.col)), "the whole drag is one step");
    // Enter lays one at the cursor.
    push(&mut e, "tool_road");
    special(&mut e, SpecialKey::Enter);
    assert!(road(&e, e.map().cursor.row, e.map().cursor.col));
}

#[test]
fn a_map_directory_is_opened_and_saved_by_its_terrain() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("editor_mapdir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut rows = vec!["g".repeat(20); 20];
    rows[4] = "ggggcGGggggggggggggg".to_string();
    std::fs::write(dir.join("terrain.map"), format!("20\n20\n{}\n", rows.join("\n"))).unwrap();
    let units = "unit riders\nmoves 9\nenter grass 2\n";
    let cities = "city 4 4\nname Wrenfold\ngarrison riders\n";
    std::fs::write(dir.join("units.txt"), units).unwrap();
    std::fs::write(dir.join("cities.txt"), cities).unwrap();

    let mut e = editor();
    set_file(&mut e, &dir);
    push(&mut e, "open");
    assert!(e.message().starts_with("Opened "), "{}", e.message());
    assert_eq!((e.map().rows(), e.map().cols()), (20, 20));
    assert!(e.map().tile(Hex::new(4, 5)).unwrap().road);
    // The editor has the terrain; the units are the game's business.
    assert_eq!(e.map().units().iter().count(), 0);

    // A change, and a save: the terrain file is written, the others are not touched.
    e.frame(16);
    push(&mut e, "terrain_f");
    let p = pixel(&e, 10, 10);
    click(&mut e, p);
    push(&mut e, "save");
    assert_eq!(e.message(), format!("Saved {}", dir.join("terrain.map").display()));
    assert!(!e.modified());
    assert_eq!(e.path(), Some(dir.as_path()));
    let text = std::fs::read_to_string(dir.join("terrain.map")).unwrap();
    assert_eq!(text.lines().nth(12).unwrap().chars().nth(10), Some('f'));
    assert_eq!(text.lines().nth(6), Some("ggggcGGggggggggggggg"));
    assert_eq!(std::fs::read_to_string(dir.join("units.txt")).unwrap(), units);
    assert_eq!(std::fs::read_to_string(dir.join("cities.txt")).unwrap(), cities);
    assert!(!dir.join("terrain.map").is_dir());

    // Painting the city away leaves its entry without a city: the saving says so.
    push(&mut e, "terrain_g");
    script::run(&mut e, "cursor:4:4,click:4:4").unwrap();
    push(&mut e, "save");
    assert!(e.message().starts_with("Saved "), "{}", e.message());
    assert!(
        e.message().contains("but: ") && e.message().contains("Wrenfold is at (4, 4), which is grass"),
        "{}",
        e.message()
    );
    // A new city tile without an entry is said too, more quietly.
    key(&mut e, 'z', Modifiers::CTRL);
    push(&mut e, "terrain_c");
    script::run(&mut e, "cursor:8:8,click:8:8").unwrap();
    push(&mut e, "save");
    assert!(e.message().ends_with("(the city tile at (8, 8) has no entry in cities.txt)"), "{}", e.message());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_maps_in_the_repository_are_good() {
    let dir = assets().join("maps");
    let (mut files, mut directories) = (0, 0);
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // A whole map: its files agree, and every unit has its picture.
            let report = mapeditor::check::check_path(&path, Some(&assets())).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(report.warnings, Vec::<String>::new(), "{}", path.display());
            assert!(!report.units.is_empty() && !report.cities.is_empty(), "{}", path.display());
            // And the game starts on it, with units to move.
            let (game, warnings) = hexgame::game::Game::load(&path).unwrap();
            assert!(warnings.is_empty());
            assert!(game.map.units().iter().count() > 0 && !game.map.units_at(game.map.cursor).is_empty());
            directories += 1;
        } else if path.extension().is_some_and(|e| e == "map") {
            let text = std::fs::read_to_string(&path).unwrap();
            let report = mapeditor::check::check(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(report.warnings, Vec::<String>::new(), "{}", path.display());
            files += 1;
        }
    }
    assert!(files >= 1 && directories >= 1, "{files} map files and {directories} map directories in {}", dir.display());
}

#[test]
fn the_format_pages_units_and_cities_are_read_as_it_says() {
    use hexgame::state::scenario::{parse_cities, parse_units};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let page = std::fs::read_to_string(root.join("docs/map-format.md")).unwrap();
    let (mut units, mut cities) = (0, 0);
    // Every other piece between the fences is a block of the page.
    for block in page.split("```").skip(1).step_by(2) {
        let block = block.trim_start_matches(|c: char| c != '\n');
        match block.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')) {
            Some(first) if first.starts_with("unit ") => {
                let kinds = parse_units(block).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!((kinds[0].id.as_str(), kinds[0].moves, kinds[0].cost.bridge), ("riders", 18, 1));
                units += 1;
            }
            Some(first) if first.starts_with("city ") => {
                let entries = parse_cities(block).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!((entries[0].name.as_str(), entries[0].hex), ("Brindlewick", Hex::new(13, 7)));
                assert_eq!(entries[0].garrison, [("militia".to_string(), 2), ("riders".to_string(), 1)]);
                let costs: Vec<Option<u32>> = entries[0].produce.iter().map(|p| p.cost).collect();
                assert_eq!(costs, [None, Some(12)]);
                cities += 1;
            }
            _ => {}
        }
    }
    assert_eq!((units, cities), (1, 1), "the page has an example of each");
}

#[test]
fn the_example_continent_is_a_whole_map() {
    let report = mapeditor::check::check_path(&assets().join("maps/continent"), Some(&assets())).unwrap();
    assert_eq!((report.rows, report.cols), (46, 72));
    assert_eq!((report.units.len(), report.cities.len()), (5, 9));
    assert!(report.units.iter().any(|u| u.contains("commander 1")), "a captain: {:?}", report.units);
    assert!(report.roads > 50 && report.bridges >= 1 && report.passes >= 1, "{report}");
    // Every city is on the road: a road or another city beside it.
    let (game, _) = hexgame::game::Game::load(&assets().join("maps/continent")).unwrap();
    for city in game.map.cities().iter() {
        assert!(game.map.road_links(city.pos) != 0, "{} has no road", city.name);
    }
    // Four sides hold a city each, and the first one's rider can ride out.
    let held = game.map.cities().iter().filter(|c| c.side != hexgame::state::Side::Grey).count();
    assert_eq!(held, 4);
}

#[test]
fn the_format_page_shows_the_example_map() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let page = std::fs::read_to_string(root.join("docs/map-format.md")).unwrap();
    let example = std::fs::read_to_string(assets().join("maps/example.map")).unwrap();
    assert!(page.contains(&format!("```\n{example}```")), "docs/map-format.md and assets/maps/example.map differ");
    // And what the page says about it is true: every terrain is on it.
    let report = mapeditor::check::check(&example).unwrap();
    assert_eq!((report.rows, report.cols), (20, 30));
    assert!(report.counts.iter().all(|n| *n > 0), "{:?}", report.counts);
}

#[test]
fn f1_opens_the_editors_help() {
    let mut e = editor();
    special(&mut e, SpecialKey::F1);
    assert!(e.screen.help_open());
    let help = e.screen.help();
    let titles: Vec<&str> = help.iter().map(|section| section.title.as_str()).collect();
    assert_eq!(titles, ["The map", "Editing", "The window"]);
    let editing = &help[1].rows;
    assert!(editing.iter().any(|(keys, what)| keys.ends_with("+S") && what == "Save"), "{editing:?}");
    // A letter is still a terrain while the help is open, and `?` is not one.
    type_char(&mut e, 'w');
    assert_eq!(e.map().tile(e.map().cursor).unwrap().kind, TileType::Water);
    type_char(&mut e, '?');
    assert!(!e.screen.help_open());
    // Esc closes it too.
    special(&mut e, SpecialKey::F1);
    special(&mut e, SpecialKey::Esc);
    assert!(!e.screen.help_open());
}
