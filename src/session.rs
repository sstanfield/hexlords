//! The game without its window: a map screen (the UI, the map and the views)
//! with the game's layout and what the player's input does to the game. It
//! needs no graphics context, so tests drive it the way the window does.
//!
//! Around the map are the panels: under it the line about the hex under the
//! cursor, then the *selected* hex (the one last clicked) with a button for
//! each unit that stands on it and all about the unit that is selected;
//! beside it, under the minimap, the selected hex's city. `info` has the
//! words; this module puts them into the widgets.
//!
//! C++: assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp
//! MapWidget::OnEvent (what input does to the map)

use std::path::Path;

use turbo_ferret::{LayoutParams, Modifiers, Size, TbId, Visibility, WidgetId};

use crate::game::Game;
use crate::gui::{MapCommand, MapKey, RootCommand};
use crate::info;
use crate::screen::{MapScreen, Scene};
use crate::state::{Hex, MAX_STACK, UnitId};

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/main.tb.txt";

/// The main map takes this share of the window's width and height; the minimap
/// gets the rest of each. C++: assets/layouts/main.lua
const MAP_SHARE: (f32, f32) = (0.8, 0.7);
/// The space between widgets in the layout, in dp (`spacing: 5` in the layout).
const SPACING_DP: i32 = 5;
/// The height of the line about the cursor, and of the panels' other lines, in dp.
const STATUS_DP: i32 = 24;
const LINE_DP: i32 = 20;
/// The width of a unit's button and the space beside it in the layout, in dp.
const UNIT_BUTTON_DP: i32 = 200;
const PANEL_SPACING_DP: i32 = 6;
/// The button that ends the turn, and the line about the cursor.
pub const END_TURN_ID: &str = "end_turn";
pub const STATUS_ID: &str = "status";
/// The selected hex's heading.
pub const HEX_TITLE_ID: &str = "hex_title";
/// The lines about the selected unit and about the selected city: `detail_0`
/// ..., `city_0` ...
pub const DETAIL_LINES: usize = 5;
pub const CITY_LINES: usize = 9;

/// The id of the button of the `i`th unit of the selected hex.
pub fn unit_button_id(i: usize) -> String {
    format!("unit_{i}")
}

/// A widget whose text the session writes, with what it shows now (so that it
/// is written only when it changes).
struct Line {
    widget: WidgetId,
    text: String,
}

impl Line {
    fn find(screen: &MapScreen, id: &str) -> Result<Line, String> {
        let widget =
            screen.ui.find(screen.ui.root(), TbId::new(id)).ok_or(format!("{LAYOUT}: no widget with id {id}"))?;
        Ok(Line { widget, text: String::new() })
    }

    fn set(&mut self, screen: &mut MapScreen, text: &str) {
        if self.text != text {
            screen.ui.set_text(self.widget, text);
            self.text = text.to_string();
        }
    }
}

pub struct Session {
    pub screen: MapScreen,
    /// The hex the panels are about: the one last chosen on the map.
    shown: Hex,
    status: Line,
    hex_title: Line,
    /// A button for each unit a hex can hold.
    unit_buttons: Vec<Line>,
    detail: Vec<Line>,
    city: Vec<Line>,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, and start the demo game.
    pub fn new(assets: &Path, viewport: Size, dpi: i32) -> Result<Session, String> {
        Session::with_game(assets, viewport, dpi, Game::demo())
    }

    /// The same, playing `game`.
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        let screen = MapScreen::new(assets, viewport, dpi, LAYOUT, game)?;
        let lines = |screen: &MapScreen, name: &str, n: usize| -> Result<Vec<Line>, String> {
            (0..n).map(|i| Line::find(screen, &format!("{name}_{i}"))).collect()
        };
        let mut session = Session {
            shown: screen.game.map.cursor,
            status: Line::find(&screen, STATUS_ID)?,
            hex_title: Line::find(&screen, HEX_TITLE_ID)?,
            unit_buttons: lines(&screen, "unit", MAX_STACK)?,
            detail: lines(&screen, "detail", DETAIL_LINES)?,
            city: lines(&screen, "city", CITY_LINES)?,
            screen,
        };
        session.apply_layout_sizes();
        session.refresh();
        session.screen.settle();
        Ok(session)
    }

    /// Size the map and the minimap from the window's size: the map is 80% of the
    /// width and 70% of the height, the minimap what is left of each; and give
    /// the panels' lines the widths that leaves them. Called again whenever the
    /// window's size changes. C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let dp = |screen: &MapScreen, dp: i32| screen.ui.dimension_converter().dp_to_px(dp);
        let spacing = dp(&self.screen, SPACING_DP);
        let map_w = (w as f32 * MAP_SHARE.0).floor() as i32;
        let map_h = (h as f32 * MAP_SHARE.1).floor() as i32;
        let side_w = w - map_w - spacing;
        self.screen.set_map_sizes(Size::new(map_w, map_h), Size::new(side_w, h - map_h));

        let (status_h, line_h) = (dp(&self.screen, STATUS_DP), dp(&self.screen, LINE_DP));
        let ui = &mut self.screen.ui;
        ui.set_layout_params(self.status.widget, Some(LayoutParams::fixed(map_w, status_h)));
        ui.set_layout_params(self.hex_title.widget, Some(LayoutParams::fixed(map_w, line_h)));
        for line in &self.city {
            ui.set_layout_params(line.widget, Some(LayoutParams::fixed(side_w.max(0), line_h)));
        }
        // Beside the two columns of unit buttons.
        let columns = 2 * (dp(&self.screen, UNIT_BUTTON_DP) + dp(&self.screen, PANEL_SPACING_DP));
        let detail_w = (map_w - columns).max(0);
        for line in &self.detail {
            self.screen.ui.set_layout_params(line.widget, Some(LayoutParams::fixed(detail_w, line_h)));
        }
    }

    /// The line about the hex under the cursor.
    pub fn status(&self) -> &str {
        &self.status.text
    }

    /// The hex the panels are about: the one last chosen on the map (at the
    /// start, the one the cursor starts on).
    pub fn shown(&self) -> Hex {
        self.shown
    }

    /// The selected hex's heading.
    pub fn hex_title(&self) -> &str {
        &self.hex_title.text
    }

    /// What the buttons of the selected hex's units say, the first unit first.
    pub fn stack(&self) -> Vec<&str> {
        let units = self.screen.game.map.units_at(self.shown).len();
        self.unit_buttons.iter().take(units).map(|line| line.text.as_str()).collect()
    }

    /// The lines about the selected unit (none if no unit is selected).
    pub fn unit_details(&self) -> Vec<&str> {
        self.detail.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty()).collect()
    }

    /// The lines about the selected hex's city (none if it has no city).
    pub fn city(&self) -> Vec<&str> {
        self.city.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty()).collect()
    }

    /// Bring the panels up to date with the game.
    fn refresh(&mut self) {
        let game = &self.screen.game;
        let map = &game.map;
        let status = info::cursor_line(game);
        let title = info::hex_title(map, self.shown);
        // The units of the selected hex, and which of them is the selected one.
        let ids: Vec<UnitId> = map.units_at(self.shown).to_vec();
        let briefs: Vec<String> = ids.iter().filter_map(|id| map.units().get(*id)).map(info::unit_brief).collect();
        let chosen = game.selected().and_then(|id| ids.iter().position(|unit| *unit == id));
        // All about the selected unit, wherever it stands.
        let mut detail = match game.selected().and_then(|id| map.units().get(id)) {
            Some(unit) => info::unit_details(map, unit),
            None if ids.is_empty() => Vec::new(),
            None => vec!["Click a unit to select it, then a hex to send it there.".to_string()],
        };
        detail.truncate(DETAIL_LINES);
        let city = map.city_at(self.shown).map_or(Vec::new(), |city| info::city_lines(map, city, CITY_LINES));

        let screen = &mut self.screen;
        self.status.set(screen, &status);
        self.hex_title.set(screen, &title);
        for (i, button) in self.unit_buttons.iter_mut().enumerate() {
            let here = i < briefs.len();
            button.set(screen, briefs.get(i).map_or("", String::as_str));
            let visibility = if here { Visibility::Visible } else { Visibility::Gone };
            if screen.ui.node(button.widget).is_some_and(|n| n.visibility() != visibility) {
                screen.ui.set_visibility(button.widget, visibility);
            }
            // The selected unit's button is the one that is pressed in.
            let pressed = i32::from(chosen == Some(i));
            if screen.ui.value(button.widget).as_int() != pressed {
                screen.ui.set_value(button.widget, pressed);
            }
        }
        for (i, line) in self.detail.iter_mut().enumerate() {
            line.set(screen, detail.get(i).map_or("", String::as_str));
        }
        for (i, line) in self.city.iter_mut().enumerate() {
            line.set(screen, city.get(i).map_or("", String::as_str));
        }
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
    }

    /// One frame of host work before drawing (see `MapScreen::frame`).
    pub fn frame(&mut self, now_ms: u64) {
        self.screen.frame(now_ms);
        self.refresh();
    }

    /// The player chose a hex of the map (a click, or Enter at the cursor):
    /// the game selects or moves a unit, and the panels are about that hex
    /// from now on.
    fn choose(&mut self, hex: Hex) {
        self.screen.game.select_hex(hex);
        self.shown = hex;
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        for command in self.screen.take_map_commands() {
            self.screen.navigate(command);
            // A click, or Enter at the cursor, chooses a hex.
            // C++: MapWidget::OnEvent
            match command {
                MapCommand::Click(p, size) => {
                    if let Some(hex) = self.screen.pick(p, size) {
                        self.choose(hex);
                    }
                }
                MapCommand::Key(MapKey::Enter) => self.choose(self.screen.game.map.cursor),
                _ => {}
            }
        }
        for command in self.screen.pump_shared() {
            match command {
                RootCommand::Click(id) if id == TbId::new(END_TURN_ID) => self.screen.game.end_turn(),
                // Ctrl+T (Cmd+T) ends the turn too. C++: main.cpp (the
                // commented-out Ctrl+T)
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'t') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.screen.game.end_turn();
                }
                // A unit's button: that unit is the selected one.
                RootCommand::Click(id) => {
                    let unit = (0..MAX_STACK).find(|i| id == TbId::new(&unit_button_id(*i)));
                    let unit = unit.and_then(|i| self.screen.game.map.units_at(self.shown).get(i).copied());
                    if let Some(unit) = unit {
                        self.screen.game.select_unit(unit);
                    }
                }
                _ => {}
            }
        }
        self.refresh();
    }
}

impl Scene for Session {
    fn screen(&self) -> &MapScreen {
        &self.screen
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        &mut self.screen
    }

    fn resize(&mut self, viewport: Size) {
        Session::resize(self, viewport);
    }

    fn pump(&mut self) {
        Session::pump(self);
    }

    fn frame(&mut self, now_ms: u64) {
        Session::frame(self, now_ms);
    }

    /// Where the selected unit can go.
    fn marked(&self, hex: Hex) -> bool {
        self.screen.game.can_move_to(hex)
    }

    /// The selected unit shows on its hex, over the others of its stack.
    fn front_unit(&self) -> Option<UnitId> {
        self.screen.game.selected()
    }
}
