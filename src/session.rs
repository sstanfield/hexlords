//! The game without its window: a map screen (the UI, the map and the views)
//! with the game's layout and what the player's input does to the game. It
//! needs no graphics context, so tests drive it the way the window does.
//!
//! Under the map is the panel: the line about the hex under the cursor, then
//! the *selected* hex (the one last clicked), a slot for each unit it can hold
//! (the unit's icon and its strength and moves; a click selects the unit, and
//! the pointer on it opens a box with all about it), and a line about the
//! selected unit. Beside the map, under the minimap, are the buttons for the
//! lists of cities and the selected hex's city, which opens in full in a
//! window of its own (`city_windows`). The map takes all the height the panel
//! leaves. `info` has the words; this module puts them into the widgets.
//!
//! C++: assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp
//! MapWidget::OnEvent (what input does to the map)

use std::collections::HashMap;
use std::path::Path;

use turbo_ferret::widgets::ImageWidget;
use turbo_ferret::{ImageHandle, LayoutParams, Modifiers, Point, Rect, Size, TbId, Visibility, WidgetId};

use crate::agents::{self, Agents};
use crate::city_windows::{self, CityList, CityOrder, CityWindow};
use crate::game::{Game, SelectOutcome};
use crate::gui::build::{self, WindowParts};
use crate::gui::{MapCommand, MapKey, RootCommand};
use crate::help::{HelpSection, command_key};
use crate::info;
use crate::log_window::{self, LogWindow};
use crate::orders::Order;
use crate::render::{AssetDirs, unit_icon};
use crate::screen::{MapScreen, Scene};
use crate::setup::Controller;
use crate::state::scenario::side_name;
use crate::state::{CityId, DEFAULT_UNIT_IMAGE, Hex, MAX_STACK, Side, UnitId};

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/main.tb.txt";

/// The main map takes this share of the window's width; the minimap gets the
/// rest of it, and this share of the height. C++: assets/layouts/main.lua
const MAP_WIDTH_SHARE: f32 = 0.8;
const MINIMAP_HEIGHT_SHARE: f32 = 0.3;
/// The space between widgets in the layout, in dp (`spacing: 5` in the layout).
const SPACING_DP: i32 = 5;
/// The height of the line about the cursor, and of the panels' other lines, in dp.
const STATUS_DP: i32 = 24;
const LINE_DP: i32 = 20;
/// A unit's slot in the stack: its width (less if nine do not fit under the
/// map), the space between two, and the size of the icon in it, in dp.
const SLOT_DP: i32 = 84;
const SLOT_SPACING_DP: i32 = 4;
const ICON_DP: i32 = 40;
/// How many slots the stack has: one for each unit a hex can hold, and one
/// more (for a leader, one day).
pub const STACK_SLOTS: usize = MAX_STACK + 1;
/// The button that ends the turn, and the line about the cursor.
pub const END_TURN_ID: &str = "end_turn";
/// The buttons for the lists of cities, and the one for the selected city.
pub const ALL_CITIES_ID: &str = "all_cities";
/// Take back the last order; go to the next stack that can move, the next
/// city.
pub const UNDO_ID: &str = "undo";
pub const NEXT_STACK_ID: &str = "next_stack";
pub const NEXT_CITY_ID: &str = "next_city";
/// The button that opens the log (`log_window`).
pub const LOG_ID: &str = "log";
pub const MY_CITIES_ID: &str = "my_cities";
pub const CITY_DETAILS_ID: &str = "city_details";
/// The cover between two people's turns, and its button.
pub const COVER_ID: &str = "turn_cover";
pub const BEGIN_TURN_ID: &str = "begin_turn";
/// The window that says who won, and its buttons.
pub const GAME_OVER_ID: &str = "game_over";
pub const NEW_GAME_ID: &str = "new_game";
/// The window of a few lines (a battle's report, the news), and its button.
pub const NOTICE_ID: &str = "notice";
pub const NOTICE_CLOSE_ID: &str = "notice_close";
pub const GAME_OVER_CLOSE_ID: &str = "game_over_close";
/// Where the windows go on the map: the list in its left part, a city in its
/// right, a third of the way down (see `gui::build::place`).
const LIST_ALIGN: (f32, f32) = (0.0, 0.33);
const CITY_ALIGN: (f32, f32) = (1.0, 0.33);
pub const STATUS_ID: &str = "status";
/// The selected hex's heading, and the line about the selected unit.
pub const HEX_TITLE_ID: &str = "hex_title";
pub const UNIT_LINE_ID: &str = "unit_line";
/// The panel under the map, and the row of slots in it.
const PANEL_ID: &str = "hex_panel";
const STACK_ID: &str = "stack";
/// The box about the unit under the pointer, and its lines (`tip_0` ...).
pub const UNIT_TIP_ID: &str = "unit_tip";
pub const TIP_LINES: usize = 5;
/// The lines about the selected city: `city_0` ...
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
        Ok(Line { widget: find(screen, id)?, text: String::new() })
    }

    fn set(&mut self, screen: &mut MapScreen, text: &str) {
        if self.text != text {
            screen.ui.set_text(self.widget, text);
            self.text = text.to_string();
        }
    }
}

fn find(screen: &MapScreen, id: &str) -> Result<WidgetId, String> {
    screen.ui.find(screen.ui.root(), TbId::new(id)).ok_or(format!("{LAYOUT}: no widget with id {id}"))
}

fn set_visibility(screen: &mut MapScreen, widget: WidgetId, visibility: Visibility) {
    if screen.ui.node(widget).is_some_and(|n| n.visibility() != visibility) {
        screen.ui.set_visibility(widget, visibility);
    }
}

/// The picture a unit is drawn with in a slot: its kind's image and its side.
type IconKey = (String, Side);

/// A unit's place in the stack: a button with the unit's icon and its numbers.
struct Slot {
    button: WidgetId,
    icon: WidgetId,
    numbers: Line,
    /// The icon it shows now.
    shows: Option<IconKey>,
}

impl Slot {
    /// The resource the slot is made from (`Session` makes them, so that their
    /// number is a constant, not a list in the layout).
    fn resource(i: usize) -> String {
        let id = unit_button_id(i);
        format!(
            "TBButton: id: {id}, skin: UnitSlot, is-focusable: 0\n\
             \tTBLayout: axis: y, spacing: 2\n\
             \t\tTBImageWidget: id: {id}_icon\n\
             \t\tTBTextField: id: {id}_text, text-align: center\n"
        )
    }
}

/// The game's keys and clicks, for the help window (F1).
fn help_section() -> HelpSection {
    let end_turn = format!("End turn,  or  {}+T", command_key());
    let undo = format!("{}+Z,  or  Undo", command_key());
    HelpSection::new(
        "The game",
        &[
            ("Click a hex,  or  Enter", "Choose it: select its stack, or send the selected units there"),
            ("Click a hex they cannot reach", "Drop the selection (and select the stack there, if it is yours)"),
            ("A hex with another side's units", "Sending units there attacks them: the line below says the odds"),
            ("Drag on the map", "Move it (a press that does not move is a click)"),
            ("Point at a unit below", "All about it"),
            ("Click a unit below", "Take it into the selection, or leave it out"),
            ("Esc", "Close the window on top; with none open, drop the selection"),
            (&undo, "Take back the last move or city order (not a battle)"),
            ("N,  or  Next stack", "Select the next stack that can still move (Shift+N: the one before)"),
            ("C,  or  Next city", "Go to your next city (Shift+C: the one before)"),
            ("L,  or  Log", "Your battles, with who won and who died, and the cities you took and lost"),
            (&end_turn, "End the turn: units get their moves back, cities work on what they build"),
            ("City details", "All about the selected city; set what it builds"),
            ("All cities, My cities", "A list of cities: see where one is, go to it, open it"),
        ],
    )
}

pub struct Session {
    pub screen: MapScreen,
    /// The hex the panels are about: the one last chosen on the map.
    shown: Hex,
    /// Where the units' pictures come from (the map's own assets first).
    assets: AssetDirs,
    /// The units' icons drawn so far, for the UI.
    icons: HashMap<IconKey, ImageHandle>,
    panel: WidgetId,
    stack_row: WidgetId,
    status: Line,
    hex_title: Line,
    slots: Vec<Slot>,
    unit_line: Line,
    tip: WidgetId,
    tip_lines: Vec<Line>,
    city: Vec<Line>,
    /// The player's gold, and its taxes and upkeep.
    gold: Vec<Line>,
    city_details: WidgetId,
    sidebar: WidgetId,
    city_lists: WidgetId,
    turn_row: WidgetId,
    next_row: WidgetId,
    undo: WidgetId,
    /// The log, while it is open.
    log: Option<LogWindow>,
    /// How far each person at this screen has read the game's history: their
    /// news, at the start of their turn, is what came after; and those told
    /// they are out.
    seen: Vec<(Side, usize)>,
    told_out: Vec<Side>,
    /// The hex of the stack Next stack went to last.
    cycled: Option<Hex>,
    /// The windows about cities, while they are open.
    city_window: Option<CityWindow>,
    city_list: Option<CityList>,
    end_turn: Line,
    /// The computer's players.
    agents: Agents,
    /// What covers the map between two people's turns, while it does.
    cover: Option<WindowParts>,
    /// The side of the person who played last.
    last_person: Option<Side>,
    /// The window that says who won, once it is open.
    game_over: Option<WindowParts>,
    /// A new game was asked for (the game over window's button).
    wants_new_game: bool,
    /// The window of a few lines (a battle's report, the news), while it is
    /// open.
    notice: Option<WindowParts>,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, playing `game`.
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        let dirs = AssetDirs::new(assets, game.assets.as_deref());
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, game)?;
        screen.set_help(vec![help_section()]);
        screen.set_drag_pans(true);
        let stack_row = find(&screen, STACK_ID)?;
        let mut slots = Vec::with_capacity(STACK_SLOTS);
        for i in 0..STACK_SLOTS {
            screen.ui.inflate(stack_row, &Slot::resource(i)).map_err(|e| format!("a slot of the stack: {e}"))?;
            let id = unit_button_id(i);
            let (button, icon) = (find(&screen, &id)?, find(&screen, &format!("{id}_icon"))?);
            slots.push(Slot { button, icon, numbers: Line::find(&screen, &format!("{id}_text"))?, shows: None });
        }
        let lines = |screen: &MapScreen, name: &str, n: usize| -> Result<Vec<Line>, String> {
            (0..n).map(|i| Line::find(screen, &format!("{name}_{i}"))).collect()
        };
        let agents = Agents::for_game(&screen.game)?;
        let tip = find(&screen, UNIT_TIP_ID)?;
        // The box is over the map, and the pointer goes through it.
        screen.ui.set_ignore_input(tip, true);
        let mut session = Session {
            shown: screen.game.map.cursor,
            assets: dirs,
            icons: HashMap::new(),
            panel: find(&screen, PANEL_ID)?,
            stack_row,
            status: Line::find(&screen, STATUS_ID)?,
            hex_title: Line::find(&screen, HEX_TITLE_ID)?,
            slots,
            unit_line: Line::find(&screen, UNIT_LINE_ID)?,
            tip,
            tip_lines: lines(&screen, "tip", TIP_LINES)?,
            city: lines(&screen, "city", CITY_LINES)?,
            gold: lines(&screen, "gold", 2)?,
            city_details: find(&screen, CITY_DETAILS_ID)?,
            sidebar: find(&screen, "main_sidebar")?,
            city_lists: find(&screen, "city_lists")?,
            turn_row: find(&screen, "turn_row")?,
            next_row: find(&screen, "next_row")?,
            undo: find(&screen, UNDO_ID)?,
            log: None,
            seen: Vec::new(),
            told_out: Vec::new(),
            cycled: None,
            city_window: None,
            city_list: None,
            end_turn: Line::find(&screen, END_TURN_ID)?,
            agents,
            cover: None,
            last_person: None,
            game_over: None,
            wants_new_game: false,
            notice: None,
            screen,
        };
        session.apply_layout_sizes();
        // The computer's turns that come before the first person's.
        session.turn_passed();
        session.refresh();
        session.screen.settle();
        Ok(session)
    }

    /// The person whose turn it is has done: the next side's turn, then the
    /// computer's, until a person's.
    pub fn end_turn(&mut self) {
        let game = &self.screen.game;
        // Only a person's turn is the screen's to end (once nobody here is in
        // the game, the computer plays on by itself).
        let person = game.controller(game.player()) == Some(&Controller::Person);
        if self.cover.is_some() || game.winner().is_some() || !person {
            return;
        }
        self.screen.game.end_turn();
        self.turn_passed();
    }

    /// A turn has passed: the computer plays its sides' turns, as far as the
    /// next person's (a round at most, if nobody is a person); and when that
    /// person is not the one who played last, the map is covered until they
    /// say they are there.
    fn turn_passed(&mut self) {
        // The computer's turns, side by side, and what they did.
        loop {
            let game = &self.screen.game;
            let side = game.player();
            let computer = matches!(game.controller(side), Some(Controller::Computer(_)));
            if !computer || game.winner().is_some() || self.agents.get_mut(side).is_none() {
                break;
            }
            let Some(agent) = self.agents.get_mut(side) else { break };
            agents::play_turn(&mut self.screen.game, agent);
            if !self.people_in_play() {
                break; // nobody here plays any more: a turn a frame
            }
        }
        // The last person here went out in the computer's turns: they hear
        // of it, once.
        if !self.people_in_play() {
            if let Some(side) = self.last_person.filter(|s| !self.told_out.contains(s)) {
                self.told_out.push(side);
                self.show_news_to(side);
            }
        }
        let game = &self.screen.game;
        let side = game.player();
        let person = game.controller(side) == Some(&Controller::Person);
        if person && game.winner().is_none() && game.setup().people() > 1 && self.last_person != Some(side) {
            self.open_cover(side);
        }
        if person {
            self.last_person = Some(side);
            if self.cover.is_none() {
                self.show_news();
            }
        }
        // The panels are about the hex the cursor is on.
        self.shown = self.screen.game.map.cursor;
    }

    /// Cover the map until `side`'s person says they are there.
    fn open_cover(&mut self, side: Side) {
        // Nothing of the last person's stays open for the next to see.
        while self.close_top_window() {}
        let root = self.screen.root_window();
        let ui = &mut self.screen.ui;
        let parts = build::cover(ui, root, COVER_ID);
        let name = side_name(side);
        let mut chars = name.chars();
        let title = chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect::<String>());
        build::heading(ui, parts.content, &format!("{title}'s turn"));
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        build::button(ui, parts.content, BEGIN_TURN_ID, &format!("Begin {name}'s turn"));
        self.cover = Some(parts);
        self.screen.settle();
    }

    /// True while the map is covered between two people's turns.
    pub fn covered(&self) -> bool {
        self.cover.is_some()
    }

    /// True once a new game was asked for (the shell starts the new game
    /// screen).
    pub fn wants_new_game(&self) -> bool {
        self.wants_new_game
    }

    /// The game is over: a window says who won.
    fn open_game_over(&mut self, winner: Side) {
        let root = self.screen.root_window();
        let area = self.window_area();
        let ui = &mut self.screen.ui;
        let parts = build::window(ui, root, GAME_OVER_ID, "Game over");
        let round = self.screen.game.turn();
        build::heading(ui, parts.content, &format!("{} wins, in round {round}.", side_name(winner)));
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        let buttons = build::row(ui, parts.content, space);
        build::button(ui, buttons, NEW_GAME_ID, "New game");
        build::button(ui, buttons, GAME_OVER_CLOSE_ID, "Look at the map");
        build::place(ui, parts, area, (0.5, 0.4));
        self.game_over = Some(parts);
    }

    /// Size everything from the window's size: the map is 80% of the width and
    /// all the height the panel under it leaves; the minimap is the rest of
    /// the width and 30% of the height; the lines get the widths that leaves
    /// them. Called again whenever the window's size changes.
    /// C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let dp = |screen: &MapScreen, dp: i32| screen.ui.dimension_converter().dp_to_px(dp);
        let spacing = dp(&self.screen, SPACING_DP);
        let map_w = (w as f32 * MAP_WIDTH_SHARE).floor() as i32;
        let side_w = (w - map_w - spacing).max(0);

        let (status_h, line_h) = (dp(&self.screen, STATUS_DP), dp(&self.screen, LINE_DP));
        let gap = dp(&self.screen, SLOT_SPACING_DP);
        let fit = (map_w - gap * (STACK_SLOTS as i32 - 1)) / STACK_SLOTS as i32;
        let (slot_w, icon) = (dp(&self.screen, SLOT_DP).min(fit).max(0), dp(&self.screen, ICON_DP));
        let ui = &mut self.screen.ui;
        ui.set_layout_params(self.status.widget, Some(LayoutParams::fixed(map_w, status_h)));
        ui.set_layout_params(self.hex_title.widget, Some(LayoutParams::fixed(map_w, line_h)));
        ui.set_layout_params(self.unit_line.widget, Some(LayoutParams::fixed(map_w, line_h)));
        for slot in &self.slots {
            let mut lp = LayoutParams::default();
            lp.set_width(slot_w);
            ui.set_layout_params(slot.button, Some(lp));
            ui.set_layout_params(slot.icon, Some(LayoutParams::fixed(icon, icon)));
        }
        // The slots do not push the sidebar off a narrow window.
        ui.set_layout_params(self.stack_row, Some(LayoutParams { max_w: Some(map_w), ..LayoutParams::default() }));
        for line in self.city.iter().chain(&self.gold) {
            ui.set_layout_params(line.widget, Some(LayoutParams::fixed(side_w, line_h)));
        }
        // Nor do the sidebar's buttons push it wider than its share (their
        // text is cut short instead).
        let narrow = LayoutParams { min_w: Some(0), max_w: Some(side_w), ..LayoutParams::default() };
        // The sidebar is as tall as the window (its rows of buttons alone
        // would make it only as tall as they are).
        ui.set_layout_params(self.sidebar, Some(LayoutParams { max_h: Some(h), ..narrow }));
        ui.set_layout_params(self.city_lists, Some(narrow));
        ui.set_layout_params(self.turn_row, Some(narrow));
        ui.set_layout_params(self.next_row, Some(narrow));
        // The panel is as tall as its lines and its slots (an empty slot keeps
        // its room), and the map has the rest.
        let panel_h = ui.preferred_size_free(self.panel).pref.h;
        let map_h = (h - spacing - panel_h).max(0);
        let minimap_h = h - (h as f32 * (1.0 - MINIMAP_HEIGHT_SHARE)).floor() as i32;
        self.screen.set_map_sizes(Size::new(map_w, map_h), Size::new(side_w, minimap_h.min(h)));
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

    /// The units of the selected hex, in the stack's order: the order of their
    /// slots, the first the one the map shows.
    fn stack_units(&self) -> Vec<UnitId> {
        let map = &self.screen.game.map;
        map.units_at(self.shown).iter().copied().filter(|id| map.units().get(*id).is_some()).collect()
    }

    /// The numbers under the icons of the selected hex's units, the first unit
    /// first.
    pub fn stack(&self) -> Vec<&str> {
        let units = self.stack_units().len();
        self.slots.iter().take(units).map(|slot| slot.numbers.text.as_str()).collect()
    }

    /// The picture the `i`th slot shows: the image of the unit's kind and its
    /// side (`None` for an empty slot).
    pub fn slot_icon(&self, i: usize) -> Option<(&str, Side)> {
        self.slots.get(i)?.shows.as_ref().map(|(image, side)| (image.as_str(), *side))
    }

    /// The line about the selected unit.
    pub fn unit_line(&self) -> &str {
        &self.unit_line.text
    }

    /// The lines of the box the pointer opens over a unit's slot (none while
    /// it is closed).
    pub fn tip(&self) -> Vec<&str> {
        let open = self.screen.ui.node(self.tip).is_some_and(|n| n.visibility() == Visibility::Visible);
        let lines = self.tip_lines.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty());
        lines.filter(|_| open).collect()
    }

    /// The lines about the selected hex's city (none if it has no city).
    pub fn city(&self) -> Vec<&str> {
        self.city.iter().map(|line| line.text.as_str()).filter(|text| !text.is_empty()).collect()
    }

    /// The icon of a unit whose kind is drawn with `image`, of `side`, for the
    /// UI: drawn the first time it is asked for. A picture that cannot be read
    /// is the default unit's; if that cannot be read either, the disc alone.
    fn icon(&mut self, key: &IconKey) -> ImageHandle {
        if let Some(handle) = self.icons.get(key) {
            return handle.clone();
        }
        let (image, side) = key;
        let picture = self.assets.image(image).or_else(|_| self.assets.image(DEFAULT_UNIT_IMAGE)).ok();
        let size = self.screen.ui.dimension_converter().dp_to_px(ICON_DP);
        let icon = unit_icon(picture.as_ref(), *side, size);
        let name = format!("unit icon {image} {side:?} {size}");
        let handle = self.screen.ui.image_from_pixels(&name, icon.size, &icon.rgba);
        self.icons.insert(key.clone(), handle.clone());
        handle
    }

    /// Bring the panels up to date with the game.
    fn refresh(&mut self) {
        let game = &self.screen.game;
        let map = &game.map;
        let status = info::cursor_line(game);
        let title = info::hex_title(map, self.shown);
        // The units of the selected hex: each one's icon and numbers, and
        // which of them is the selected one.
        let ids = self.stack_units();
        let units: Vec<(IconKey, String)> = ids
            .iter()
            .filter_map(|id| map.units().get(*id))
            .map(|unit| {
                let image = map.units().kind_of(unit).map_or(DEFAULT_UNIT_IMAGE, |kind| kind.image.as_str());
                ((image.to_string(), unit.side()), info::unit_numbers(unit))
            })
            .collect();
        let chosen: Vec<bool> = ids.iter().map(|id| game.selection().contains(id)).collect();
        // The selected unit in a line, wherever it stands; a stack in a word.
        let line = match game.selected().and_then(|id| map.units().get(id)) {
            Some(_) if game.selection().len() > 1 => format!(
                "{} units selected: they move together (click one to leave it behind, or to take it along).",
                game.selection().len()
            ),
            Some(unit) => info::unit_line(map, unit),
            None if ids.is_empty() => String::new(),
            None => "Click your units to select them, then a hex to send them there.".to_string(),
        };
        // All about the unit whose slot the pointer is on.
        let hovered = self.screen.ui.hovered();
        let tipped = self.slots.iter().position(|slot| Some(slot.button) == hovered).filter(|i| *i < units.len());
        let mut tip =
            tipped.and_then(|i| map.units().get(ids[i])).map_or(Vec::new(), |unit| info::unit_details(map, unit));
        tip.truncate(TIP_LINES);
        let city = map.city_at(self.shown).map_or(Vec::new(), |city| info::city_lines(map, city, CITY_LINES));

        self.status.set(&mut self.screen, &status);
        self.hex_title.set(&mut self.screen, &title);
        self.unit_line.set(&mut self.screen, &line);
        for i in 0..self.slots.len() {
            let unit = units.get(i);
            let key = unit.map(|(key, _)| key.clone());
            if self.slots[i].shows != key {
                let handle = key.as_ref().map_or(ImageHandle::default(), |key| self.icon(key));
                let icon = self.slots[i].icon;
                self.screen.ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                self.slots[i].shows = key;
            }
            let slot = &mut self.slots[i];
            slot.numbers.set(&mut self.screen, unit.map_or("", |(_, numbers)| numbers.as_str()));
            // An empty slot keeps its room.
            let visibility = if unit.is_some() { Visibility::Visible } else { Visibility::Invisible };
            set_visibility(&mut self.screen, slot.button, visibility);
            // The selected unit's slot is the one that is pressed in.
            let pressed = i32::from(chosen.get(i).copied().unwrap_or(false));
            if self.screen.ui.value(slot.button).as_int() != pressed {
                self.screen.ui.set_value(slot.button, pressed);
            }
        }
        for (i, line) in self.tip_lines.iter_mut().enumerate() {
            line.set(&mut self.screen, tip.get(i).map_or("", String::as_str));
            // The box is as tall as what it says.
            let visibility = if i < tip.len() { Visibility::Visible } else { Visibility::Gone };
            set_visibility(&mut self.screen, line.widget, visibility);
        }
        match tipped.filter(|_| !tip.is_empty()) {
            Some(i) => self.open_tip(self.slots[i].button),
            None => set_visibility(&mut self.screen, self.tip, Visibility::Gone),
        }
        for (i, line) in self.city.iter_mut().enumerate() {
            line.set(&mut self.screen, city.get(i).map_or("", String::as_str));
        }
        let details = if city.is_empty() { Visibility::Gone } else { Visibility::Visible };
        set_visibility(&mut self.screen, self.city_details, details);
        // The button says whose turn it ends (on a map without sides, nobody's).
        let label = match self.screen.game.player() {
            Side::Grey => "End turn".to_string(),
            side => format!("End {}'s turn", side_name(side)),
        };
        self.end_turn.set(&mut self.screen, &label);
        // It is a person's to press, on their turn.
        let game = &self.screen.game;
        let theirs = game.controller(game.player()) == Some(&Controller::Person) && game.winner().is_none();
        self.screen.ui.set_disabled(self.end_turn.widget, !theirs);
        // The treasury of the person at the screen (none on a map without
        // sides).
        let viewer = self.viewer();
        let gold =
            if viewer == Side::Grey { Default::default() } else { info::treasury_lines(&self.screen.game, viewer) };
        for (line, text) in self.gold.iter_mut().zip(&gold) {
            line.set(&mut self.screen, text);
        }
        // Undo is there while there is something to take back.
        let undo = self.screen.game.can_undo();
        self.screen.ui.set_disabled(self.undo, !undo);
        if let (Some(winner), None) = (self.screen.game.winner(), self.game_over) {
            self.open_game_over(winner);
        }
        self.refresh_windows();
    }

    /// The windows about cities: forget those that were closed, and bring the
    /// others up to date.
    fn refresh_windows(&mut self) {
        // The log says what the history says now, to the player of now.
        let viewer = self.viewer();
        match &self.log {
            Some(log) if !log.is_open(&self.screen.ui) => self.log = None,
            Some(log) if log.stale(&self.screen.game, viewer) => self.open_log(),
            _ => {}
        }
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        if let Some(window) = &mut self.city_window {
            if !window.is_open(ui) || !window.refresh(ui, &game.map, game.player()) {
                window.close(ui);
                self.city_window = None;
            }
        }
        if let Some(list) = &mut self.city_list {
            if list.is_open(ui) {
                list.refresh(ui, &game.map);
            } else {
                self.city_list = None;
            }
        }
    }

    /// The window of a city: its own, replacing another city's.
    fn open_city(&mut self, id: CityId) {
        if let Some(window) = self.city_window.take() {
            window.close(&mut self.screen.ui);
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        self.city_window = CityWindow::open(ui, root, &game.map, id, game.player(), (area, CITY_ALIGN));
    }

    /// The list of cities, every one or the player's (`own`); the same list
    /// again closes it.
    fn toggle_list(&mut self, own: bool) {
        if let Some(list) = self.city_list.take() {
            list.close(&mut self.screen.ui);
            if list.own() == own {
                return;
            }
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        let mut list = CityList::open(ui, root, &game.map, own, game.player(), (area, LIST_ALIGN));
        // The selected hex's city, if it is in the list, is the chosen one.
        if let Some(city) = game.map.city_at(self.shown) {
            list.choose(ui, &game.map, city.id);
        }
        self.city_list = Some(list);
    }

    /// Where the windows go: over the map, a little in from its edges.
    fn window_area(&self) -> Rect {
        let margin = self.screen.ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        let map = self.screen.map_rect();
        Rect::new(map.x + margin, map.y + margin, (map.w - 2 * margin).max(0), (map.h - 2 * margin).max(0))
    }

    /// The window of a city, if one is open.
    pub fn city_window(&self) -> Option<&CityWindow> {
        self.city_window.as_ref()
    }

    /// The list of cities, if one is open.
    pub fn city_list(&self) -> Option<&CityList> {
        self.city_list.as_ref()
    }

    /// Close the window on top (a notice, a city's, the list). False if none
    /// was open.
    fn close_top_window(&mut self) -> bool {
        let ui = &mut self.screen.ui;
        if let Some(notice) = self.notice.take() {
            if build::is_open(ui, notice.window) {
                build::close(ui, notice.window);
                return true;
            }
        }
        if let Some(log) = self.log.take() {
            if log.is_open(ui) {
                log.close(ui);
                return true;
            }
        }
        if let Some(window) = self.city_window.take() {
            window.close(ui);
            return true;
        }
        if let Some(list) = self.city_list.take() {
            list.close(ui);
            return true;
        }
        false
    }

    /// Do what a window asked.
    fn obey(&mut self, order: CityOrder) {
        match order {
            CityOrder::SetProduction(id, what) => {
                // An order: only the city's holder, if that is the player.
                let _ = self.screen.game.apply(Order::Build { city: id, what });
            }
            CityOrder::Details(id) => self.open_city(id),
            CityOrder::GoTo(id) => {
                if let Some(pos) = self.screen.game.map.cities().get(id).map(|city| city.pos) {
                    self.screen.game.set_cursor(pos);
                    self.screen.view.center_on(pos);
                    self.shown = pos;
                }
            }
        }
    }

    /// A click on a button of a window about cities, or on one that opens
    /// one. False if it was none of those.
    fn window_click(&mut self, id: TbId) -> bool {
        if id == TbId::new(ALL_CITIES_ID) || id == TbId::new(MY_CITIES_ID) {
            self.toggle_list(id == TbId::new(MY_CITIES_ID));
            return true;
        }
        if id == TbId::new(CITY_DETAILS_ID) {
            if let Some(city) = self.screen.game.map.city_at(self.shown).map(|c| c.id) {
                self.open_city(city);
            }
            return true;
        }
        if id == TbId::new(city_windows::CITY_CLOSE_ID) {
            if let Some(window) = self.city_window.take() {
                window.close(&mut self.screen.ui);
            }
            return true;
        }
        if id == TbId::new(city_windows::LIST_CLOSE_ID) {
            if let Some(list) = self.city_list.take() {
                list.close(&mut self.screen.ui);
            }
            return true;
        }
        if let Some(order) = self.city_window.as_ref().and_then(|w| w.click(id)) {
            self.obey(order);
            return true;
        }
        let (ui, map) = (&mut self.screen.ui, &self.screen.game.map);
        let mut handled = false;
        let mut order = None;
        if let Some(list) = &mut self.city_list {
            let rows = list.cities().len();
            let is_list_button = (0..rows).any(|i| id == TbId::new(&city_windows::row_id(i)))
                || id == TbId::new(city_windows::LIST_DETAILS_ID)
                || id == TbId::new(city_windows::LIST_GOTO_ID);
            if is_list_button {
                order = list.click(ui, map, id);
                handled = true;
            }
        }
        if let Some(order) = order {
            self.obey(order);
        }
        handled
    }

    /// Show the box about a unit over its slot (under it if there is no room
    /// above, and wherever it fits in a window too small for either), inside
    /// the window.
    fn open_tip(&mut self, slot: WidgetId) {
        set_visibility(&mut self.screen, self.tip, Visibility::Visible);
        let ui = &mut self.screen.ui;
        let size = ui.preferred_size_free(self.tip).pref;
        let window = ui.viewport();
        let spacing = ui.dimension_converter().dp_to_px(SPACING_DP);
        let at = Rect::from_origin_size(ui.to_root(slot, Point::ZERO), ui.rect(slot).size());
        let x = at.x.min(window.w - size.w).max(0);
        let (above, below) = (at.y - spacing - size.h, at.bottom() + spacing);
        let y = if above >= 0 {
            above
        } else if below + size.h <= window.h {
            below
        } else {
            (window.h - size.h).max(0)
        };
        ui.set_rect(self.tip, Rect::new(x, y, size.w, size.h));
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
        if let Some(cover) = &mut self.cover {
            build::stretch_cover(&mut self.screen.ui, cover);
        }
    }

    /// One frame of host work before drawing (see `MapScreen::frame`). In a
    /// game the computer plays alone, a round of it a frame.
    pub fn frame(&mut self, now_ms: u64) {
        let game = &self.screen.game;
        let nobody = game.controller(game.player()) != Some(&Controller::Person);
        if nobody && game.winner().is_none() && self.cover.is_none() {
            self.turn_passed();
        }
        self.screen.frame(now_ms);
        self.refresh();
    }

    /// The player chose a hex of the map (a click, or Enter at the cursor):
    /// the game selects or moves a unit, and the panels are about that hex
    /// from now on.
    fn choose(&mut self, hex: Hex) {
        let side = self.screen.game.player();
        let before = self.screen.game.history().len();
        if let SelectOutcome::Fought(_) = self.screen.game.select_hex(hex) {
            // The report: what the history says of it to the attacker.
            let game = &self.screen.game;
            let records = game.history().get(before..).unwrap_or_default();
            let lines: Vec<String> = records.iter().flat_map(|r| info::record_lines(&game.map, r, side)).collect();
            self.open_notice("Battle", &lines);
        }
        self.shown = hex;
    }

    /// Open the log (the Log button, L), or close it if it is open.
    pub fn toggle_log(&mut self) {
        if let Some(log) = self.log.take() {
            if log.is_open(&self.screen.ui) {
                log.close(&mut self.screen.ui);
                return;
            }
        }
        self.open_log();
    }

    fn open_log(&mut self) {
        if self.covered() {
            return;
        }
        if let Some(old) = self.log.take() {
            old.close(&mut self.screen.ui);
        }
        let root = self.screen.root_window();
        let area = self.window_area();
        let side = self.viewer();
        self.log = Some(LogWindow::open(&mut self.screen.ui, root, &self.screen.game, side, (area, (0.5, 0.3))));
        self.screen.settle();
    }

    /// Whose log the screen shows: the person whose turn it is, or, in the
    /// computer's turns, the person who played last.
    fn viewer(&self) -> Side {
        let game = &self.screen.game;
        let side = game.player();
        if game.controller(side) == Some(&Controller::Person) { side } else { self.last_person.unwrap_or(side) }
    }

    /// What the log says, while it is open (none while it is closed).
    pub fn log_lines(&self) -> Vec<String> {
        self.log
            .as_ref()
            .filter(|log| log.is_open(&self.screen.ui))
            .map_or(Vec::new(), |log| log.lines(&self.screen.ui))
    }

    /// Take back the last order (the Undo button, Ctrl+Z): the units go back
    /// where they were, selected, and the panels are about them.
    pub fn undo(&mut self) {
        if self.covered() || self.screen.game.apply(Order::Undo).is_err() {
            return;
        }
        let game = &mut self.screen.game;
        if let Some(pos) = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos()) {
            game.set_cursor(pos);
            self.shown = pos;
        }
    }

    /// Select the next stack of the player's that can still move (or the one
    /// before, `back`), and look at it: the one after the selected stack, or
    /// else after the last one this went to (at first, the first).
    pub fn next_stack(&mut self, back: bool) {
        let game = &self.screen.game;
        let selected = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        let Some(hex) = game.next_stack(selected.or(self.cycled), back) else { return };
        let game = &mut self.screen.game;
        game.deselect();
        game.select_hex(hex);
        self.cycled = Some(hex);
        self.look_at(hex);
    }

    /// Go to the next city of the player's (or the one before), and look at
    /// it; nothing stays selected.
    pub fn next_city(&mut self, back: bool) {
        let Some(hex) = self.screen.game.next_city(self.shown, back) else { return };
        self.screen.game.deselect();
        self.look_at(hex);
    }

    /// The cursor goes to a hex, the view to the middle of it, and the panels
    /// are about it.
    fn look_at(&mut self, hex: Hex) {
        self.screen.game.set_cursor(hex);
        self.screen.view.center_on(hex);
        self.shown = hex;
    }

    /// A person's turn begins (they are at the screen): what happened to
    /// them in the others' turns since they last read the history, in a
    /// window, if anything did. (All of it stays in the log.)
    fn show_news(&mut self) {
        self.show_news_to(self.screen.game.player());
    }

    /// True while a side that a person plays at this screen is still in the
    /// game.
    fn people_in_play(&self) -> bool {
        let game = &self.screen.game;
        game.order().iter().any(|s| !game.out().contains(s) && game.controller(*s) == Some(&Controller::Person))
    }

    fn show_news_to(&mut self, side: Side) {
        let length = self.screen.game.history().len();
        let from = match self.seen.iter_mut().find(|(s, _)| *s == side) {
            Some((_, seen)) => std::mem::replace(seen, length),
            None => {
                self.seen.push((side, length));
                0
            }
        };
        let lines = info::news(&self.screen.game, side, from);
        if !lines.is_empty() {
            self.open_notice("While you waited", &lines);
        }
    }

    /// A window of a few lines, with a button that closes it (as Esc does).
    fn open_notice(&mut self, title: &str, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        if let Some(old) = self.notice.take() {
            build::close(&mut self.screen.ui, old.window);
        }
        let root = self.screen.root_window();
        let area = self.window_area();
        let ui = &mut self.screen.ui;
        let parts = build::window(ui, root, NOTICE_ID, title);
        for line in lines {
            build::text(ui, parts.content, line);
        }
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        build::button(ui, parts.content, NOTICE_CLOSE_ID, "Close");
        build::place(ui, parts, area, (0.5, 0.15));
        self.notice = Some(parts);
        self.screen.settle();
    }

    /// What the window of a few lines says, while it is open.
    pub fn notice(&self) -> Vec<String> {
        let Some(parts) = self.notice.filter(|p| build::is_open(&self.screen.ui, p.window)) else { return Vec::new() };
        let ui = &self.screen.ui;
        ui.children(parts.content)
            .iter()
            .filter_map(|id| ui.get::<turbo_ferret::widgets::TextField>(*id))
            .map(|t| t.text.clone())
            .collect()
    }

    /// Apply what the widgets recorded since the last call. The host calls this
    /// after every input event it forwards to the UI.
    pub fn pump(&mut self) {
        // While the map is covered, only the cover's button counts.
        if self.cover.is_some() {
            self.screen.take_map_commands();
            for command in self.screen.pump_shared() {
                if command == RootCommand::Click(TbId::new(BEGIN_TURN_ID)) {
                    if let Some(cover) = self.cover.take() {
                        build::close(&mut self.screen.ui, cover.window);
                    }
                    self.show_news();
                }
            }
            self.refresh();
            return;
        }
        for command in self.screen.take_map_commands() {
            // Esc closes the help if it is open, else the window on top, and
            // with none open it drops the selection.
            if command == MapCommand::Key(MapKey::Esc) && !self.screen.help_open() && !self.close_top_window() {
                self.screen.game.deselect();
            }
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
                RootCommand::Click(id) if id == TbId::new(END_TURN_ID) => self.end_turn(),
                // Ctrl+T (Cmd+T) ends the turn too. C++: main.cpp (the
                // commented-out Ctrl+T)
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'t') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.end_turn();
                }
                RootCommand::Click(id) if id == TbId::new(UNDO_ID) => self.undo(),
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'z') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.undo();
                }
                RootCommand::Click(id) if id == TbId::new(NEXT_STACK_ID) => self.next_stack(false),
                RootCommand::Click(id) if id == TbId::new(NEXT_CITY_ID) => self.next_city(false),
                RootCommand::Click(id) if id == TbId::new(LOG_ID) => self.toggle_log(),
                RootCommand::Click(id) if id == TbId::new(log_window::LOG_CLOSE_ID) => {
                    if let Some(log) = self.log.take() {
                        log.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Key(ch, mods) if !mods.intersects(Modifiers::CTRL | Modifiers::SUPER | Modifiers::ALT) => {
                    match ch {
                        'n' => self.next_stack(false),
                        'N' => self.next_stack(true),
                        'c' => self.next_city(false),
                        'C' => self.next_city(true),
                        'l' | 'L' => self.toggle_log(),
                        _ => {}
                    }
                }
                RootCommand::Click(id) if id == TbId::new(NEW_GAME_ID) => self.wants_new_game = true,
                RootCommand::Click(id) if id == TbId::new(NOTICE_CLOSE_ID) => {
                    if let Some(notice) = self.notice.take() {
                        build::close(&mut self.screen.ui, notice.window);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(GAME_OVER_CLOSE_ID) => {
                    if let Some(window) = self.game_over {
                        build::close(&mut self.screen.ui, window.window);
                    }
                }
                RootCommand::Click(id) if self.window_click(id) => {}
                // A unit's slot: that unit is the selected one.
                RootCommand::Click(id) => {
                    let unit = (0..STACK_SLOTS).find(|i| id == TbId::new(&unit_button_id(*i)));
                    let unit = unit.and_then(|i| self.stack_units().get(i).copied());
                    if let Some(unit) = unit {
                        self.screen.game.toggle_unit(unit);
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

    /// The selected unit shows on its hex, over the others of its stack (in a
    /// city, its pip is ringed).
    fn front_unit(&self) -> Option<UnitId> {
        self.screen.game.selected()
    }
}
