//! The game without its window: a map screen (the UI, the map and the views)
//! with the game's layout and what the player's input does to the game. It
//! needs no graphics context, so tests drive it the way the window does.
//!
//! Along the top is the menu bar: the game's menus (`gui::menu`; each item
//! has its key) and, at its right end, the player's treasury. Under the map
//! is the panel: the line about the hex under the cursor, then the *selected*
//! hex (the one last clicked), the stack: a box for its leader and one for
//! each of the eight units a hex holds (the unit's icon and its strength and
//! moves; a click selects the unit, and the pointer on it opens a box with
//! all about it; empty boxes stay, dimmed), with Go to, Done and Next beside
//! them, and a line about the selected unit. Beside the map, under the
//! minimap, are End turn and Undo, and the selected hex's city: one of the
//! player's in full (what it builds, where its new units go), another's as it
//! shows from outside; either opens in a window of its own (`city_windows`).
//! The map takes all the height the menu bar and the panel leave. `info` has
//! the words; this module puts them into the widgets.
//!
//! C++: assets/layouts/main.lua (the layout), src/gui/mapwidget.cpp
//! MapWidget::OnEvent (what input does to the map)

use std::collections::HashMap;
use std::path::Path;

use turbo_ferret::widgets::ImageWidget;
use turbo_ferret::{ImageHandle, LayoutParams, Modifiers, Point, Rect, Size, TbId, Visibility, WidgetId};

use crate::agents::{self, Agents};
use crate::battle_window::{self, BattleWindow};
use crate::city_windows::{self, CityList, CityOrder, CityPanel, CityWindow};
use crate::game::{Game, SelectOutcome, Sight, Task};
use crate::gui::build::{self, WindowParts};
use crate::gui::menu::{ItemWords, Menu, MenuBar};
use crate::gui::{MapCommand, MapKey, RootCommand};
use crate::guide;
use crate::help::{HelpSection, command_key, fullscreen_keys};
use crate::info;
use crate::log_window::{self, LogWindow};
use crate::orders::{Event, Order};
use crate::render::{AssetDirs, coin_icon, unit_icon};
use crate::screen::{FULLSCREEN_ID, HELP_KEYS_ID, MapScreen, QUIT_ID, Scene, ZOOM_IN_ID, ZOOM_OUT_ID};
use crate::setup::Controller;
use crate::state::scenario::side_name;
use crate::state::{CityId, DEFAULT_UNIT_IMAGE, Hex, MAX_STACK, Side, UnitId};
use crate::tasks_window::{self, TasksWindow};

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
/// A unit's box in the stack: its width (less if they do not fit under the
/// map beside the buttons), the space between two, and the size of the icon
/// in it, in dp; and the space between the boxes and the buttons.
const SLOT_DP: i32 = 84;
const SLOT_SPACING_DP: i32 = 4;
const ICON_DP: i32 = 40;
const STACK_BAR_SPACING_DP: i32 = 8;
/// The space between the menu bar and what is under it (`spacing: 4`), and
/// the coin's size, in dp.
const MENU_SPACING_DP: i32 = 4;
const COIN_DP: i32 = 18;
/// How many boxes the stack has for units: one for each a hex can hold. Its
/// leader (its commander) has one more, on the left: [`LEADER_ID`].
pub const STACK_SLOTS: usize = MAX_STACK;
pub const LEADER_ID: &str = "unit_leader";
/// The menus' titles (their ids are the menus'), and the items only the menus
/// have; the others share their ids with a button or with what a key does.
pub const MENU_GAME_ID: &str = "menu_game";
pub const MENU_UNITS_ID: &str = "menu_units";
pub const MENU_CITIES_ID: &str = "menu_cities";
pub const MENU_VIEW_ID: &str = "menu_view";
pub const MENU_HELP_ID: &str = "menu_help";
pub const MENU_TITLES: [(&str, &str); 5] = [
    (MENU_GAME_ID, "Game"),
    (MENU_UNITS_ID, "Units"),
    (MENU_CITIES_ID, "Cities"),
    (MENU_VIEW_ID, "View"),
    (MENU_HELP_ID, "Help"),
];
pub const PREV_STACK_ID: &str = "prev_stack";
pub const PREV_CITY_ID: &str = "prev_city";
pub const DESELECT_ID: &str = "deselect";
pub const CAPITAL_ID: &str = "capital";
/// The treasury in the menu bar: the gold, what comes in a turn, what goes out.
pub const GOLD_ID: &str = "gold";
pub const GOLD_IN_ID: &str = "gold_in";
pub const GOLD_OUT_ID: &str = "gold_out";
/// The button that ends the turn, and the line about the cursor.
pub const END_TURN_ID: &str = "end_turn";
/// The buttons for the lists of cities, and the one for the selected city.
pub const ALL_CITIES_ID: &str = "all_cities";
/// Take back the last order; go to the next stack that can move, the next
/// city.
pub const UNDO_ID: &str = "undo";
pub const NEXT_STACK_ID: &str = "next_stack";
pub const NEXT_CITY_ID: &str = "next_city";
/// How long the computer takes over a turn at least, in ms, so that the log
/// can be followed while it plays.
pub const COMPUTER_TURN_MS: u64 = 400;
/// The button that opens the log (`log_window`).
pub const LOG_ID: &str = "log";
pub const GO_TO_ID: &str = "go_to";
pub const DONE_ID: &str = "done";
pub const TASKS_ID: &str = "tasks";
pub const MY_CITIES_ID: &str = "my_cities";
pub const CITY_DETAILS_ID: &str = "city_details";
/// The cover between two people's turns, and its button.
pub const COVER_ID: &str = "turn_cover";
pub const BEGIN_TURN_ID: &str = "begin_turn";
/// The window that says who won, and its buttons.
pub const GAME_OVER_ID: &str = "game_over";
pub const NEW_GAME_ID: &str = "new_game";
/// The Game menu's New game: straight to the new game screen once the game
/// is over, and else a window that asks first (its buttons: `NEW_GAME_ID`,
/// and this one).
pub const NEW_GAME_ASK_ID: &str = "new_game_ask";
pub const KEEP_PLAYING_ID: &str = "keep_playing";
pub const ASK_NEW_GAME_ID: &str = "ask_new_game";
/// The window of a few lines (a battle's report, the news), and its button.
pub const GAME_OVER_CLOSE_ID: &str = "game_over_close";
/// Where the windows go on the map: the list in its left part, a city in its
/// right, a third of the way down (see `gui::build::place`).
const LIST_ALIGN: (f32, f32) = (0.0, 0.33);
const CITY_ALIGN: (f32, f32) = (1.0, 0.33);
const TASKS_ALIGN: (f32, f32) = (1.0, 0.0);

/// What a click on the map picks, when it does not choose (`Session::picking`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Where the selected units go.
    GoTo,
    /// Where the city sends its new units.
    Rally(CityId),
}
pub const STATUS_ID: &str = "status";
/// The selected hex's heading, and the line about the selected unit.
pub const HEX_TITLE_ID: &str = "hex_title";
pub const UNIT_LINE_ID: &str = "unit_line";
/// The panel under the map, and the row of slots in it.
const PANEL_ID: &str = "hex_panel";
const STACK_ID: &str = "stack";
/// The box about the unit under the pointer, and its lines (`tip_0` ...).
pub const UNIT_TIP_ID: &str = "unit_tip";
pub const TIP_LINES: usize = 7;
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
/// A unit's icon: its kind's picture, its side, its level (the ring).
type IconKey = (String, Side, u8);

/// A unit's place in the stack: a button with the unit's icon and its numbers.
struct Slot {
    button: WidgetId,
    icon: WidgetId,
    numbers: Line,
    /// The icon it shows now.
    shows: Option<IconKey>,
}

impl Slot {
    /// The resource the slot with the id `id` is made from (`Session` makes
    /// them, so that their number is a constant, not a list in the layout).
    fn resource(id: &str) -> String {
        format!(
            "TBButton: id: {id}, skin: UnitSlot, is-focusable: 0\n\
             \tTBLayout: axis: y, spacing: 2\n\
             \t\tTBImageWidget: id: {id}_icon\n\
             \t\tTBTextField: id: {id}_text, text-align: center\n"
        )
    }

    /// Make the slot `id` in `row`.
    fn make(screen: &mut MapScreen, row: WidgetId, id: &str) -> Result<Slot, String> {
        screen.ui.inflate(row, &Slot::resource(id)).map_err(|e| format!("a box of the stack: {e}"))?;
        let (button, icon) = (find(screen, id)?, find(screen, &format!("{id}_icon"))?);
        Ok(Slot { button, icon, numbers: Line::find(screen, &format!("{id}_text"))?, shows: None })
    }
}

/// The game's keys and clicks, for the help window (F1).
fn help_section() -> Vec<HelpSection> {
    let end_turn = format!("{}+T,  or  End turn", command_key());
    let undo = format!("{}+Z,  or  Undo", command_key());
    let new_game = format!("{}+N", command_key());
    vec![
        HelpSection::new(
            "The game",
            &[
                ("The menus along the top", "Everything there is to do; each item says its keys"),
                ("Click a hex,  or  Enter", "Choose it: select its stack, or send the selected units there"),
                ("Click a hex they cannot reach", "Drop the selection (and select the stack there, if it is yours)"),
                ("A hex with another side's units", "Sending units there attacks them: the line below says the odds"),
                ("Drag on the map", "Move it (a press that does not move is a click)"),
                ("Point at a unit below", "All about it"),
                (
                    "Click a unit below",
                    "Take it into the selection, or leave it out (the leader's box is the left one)",
                ),
                ("Esc", "Close the menu or the window on top; with none open, drop the selection"),
                (&undo, "Take back the last move or city order (not a battle)"),
                (&end_turn, "End the turn: your log shows what the others do; then Start turn"),
                (&new_game, "A new game (once this one is over at once; before, it asks)"),
            ],
        ),
        HelpSection::new(
            "Units",
            &[
                ("N,  or  Next", "Select the next stack that can still move (Shift+N: the one before)"),
                ("G,  or  Go to", "Then click a hex: the selected units go there, over as many turns as it takes"),
                ("D,  or  Done", "Put the selected units aside for this turn (Next stack passes them by)"),
            ],
        ),
        HelpSection::new(
            "Cities",
            &[
                ("C", "Go to your next city (Shift+C: the one before)"),
                ("Your city, in the sidebar", "Set what it builds (point at one: all about it), where its units go"),
                ("O,  or  City details", "The selected city's window, with more room"),
                ("M,  A", "Your cities, all cities: see where one is, go to it, open it"),
            ],
        ),
        HelpSection::new(
            "View",
            &[
                ("T", "Tasks: what you have left to do, stacks with no orders and idle cities first"),
                ("L", "Your log: your battles, who won and who died, the cities you took and lost"),
                ("Watch, in the log", "A battle you fought, played out again: both stacks, a hit at a time"),
                ("H", "Your capital (in hotseat, each turn starts there)"),
            ],
        ),
    ]
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
    /// The stack's boxes: its leader's, and its units'.
    leader: Slot,
    slots: Vec<Slot>,
    stack_actions: WidgetId,
    unit_line: Line,
    tip: WidgetId,
    tip_lines: Vec<Line>,
    /// Another side's city, as it shows from outside, a line at a time (in
    /// `city_text`); one of the player's own in full (in `city_box`).
    city: Vec<Line>,
    city_text: WidgetId,
    city_box: WidgetId,
    city_scroll: WidgetId,
    city_panel: Option<CityPanel>,
    /// The city, and its holder, the panel was made for.
    panel_of: Option<(CityId, Side)>,
    /// A press on the map closed a menu: the click it makes does nothing.
    swallow_click: bool,
    city_details: WidgetId,
    /// The menu bar, its menus, and the treasury at its end: the gold, what
    /// comes in a turn, what goes out.
    menu_bar: WidgetId,
    menu_room: WidgetId,
    menu_titles: WidgetId,
    treasury_box: WidgetId,
    menus: MenuBar,
    treasury: [Line; 3],
    sidebar: WidgetId,
    turn_row: WidgetId,
    undo: WidgetId,
    go_to: WidgetId,
    done: WidgetId,
    /// What the next click on the map (or Enter) picks, instead of choosing
    /// a hex: where the selected units go, or where a city sends its new
    /// ones. Esc lets it go.
    picking: Option<Pick>,
    /// Why the last pick was refused, said until the next one.
    refused: Option<String>,

    /// The list of tasks, while it is open; and the map's revision and the
    /// side it was last brought up to date for.
    tasks: Option<TasksWindow>,
    tasks_of: Option<(u64, Side)>,
    /// The log, while it is open.
    log: Option<LogWindow>,
    /// How long the game's history was when each person at this screen last
    /// ended a turn: what came after is new to them (the log marks it).
    seen: Vec<(Side, usize)>,
    /// Between a person's turns: from End turn (or, in hotseat, the next
    /// person's Begin) to Start turn, while the log shows what the others do.
    between: bool,
    /// What the person at the screen sees (fog of war), as of the map's
    /// revision and the person it was worked out for; and which cities they
    /// saw then (the minimap's picture is drawn again when that changes).
    sight: Sight,
    sight_of: Option<(u64, Side)>,
    cities_seen: Vec<bool>,
    /// When the computer last played a turn, in ms (it plays one at a time,
    /// [`COMPUTER_TURN_MS`] apart, so that the log can be followed).
    last_play: Option<u64>,
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
    /// A new game was asked for (the game over window's button, the menu).
    wants_new_game: bool,
    /// The window that asks whether to leave the game for a new one, while
    /// it is open.
    ask_new: Option<WindowParts>,
    /// A battle being watched, while its window is open.
    battle: Option<BattleWindow>,
}

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, playing `game`.
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        let dirs = AssetDirs::new(assets, game.assets.as_deref());
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, game)?;
        screen.set_help(help_section());
        screen.set_drag_pans(true);
        let stack_row = find(&screen, STACK_ID)?;
        let leader = Slot::make(&mut screen, stack_row, LEADER_ID)?;
        let slots = (0..STACK_SLOTS)
            .map(|i| Slot::make(&mut screen, stack_row, &unit_button_id(i)))
            .collect::<Result<_, _>>()?;
        let next = find(&screen, NEXT_STACK_ID)?;
        screen.ui.set_text(next, "Next \u{bb}");
        // The menus' titles, and the coin by the treasury.
        let titles = find(&screen, "menu_titles")?;
        let menus = MenuBar::new(&mut screen.ui, titles, &MENU_TITLES);
        let coin_px = screen.ui.dimension_converter().dp_to_px(COIN_DP);
        let coin = coin_icon(coin_px);
        let coin_image = screen.ui.image_from_pixels("coin", coin.size, &coin.rgba);
        let coin_widget = find(&screen, "coin")?;
        screen.ui.set_layout_params(coin_widget, Some(LayoutParams::fixed(coin_px, coin_px)));
        screen.ui.with_widget::<ImageWidget, _>(coin_widget, |w, cx| w.set_image(cx, coin_image));
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
            leader,
            slots,
            stack_actions: find(&screen, "stack_actions")?,
            unit_line: Line::find(&screen, UNIT_LINE_ID)?,
            tip,
            tip_lines: lines(&screen, "tip", TIP_LINES)?,
            city: lines(&screen, "city", CITY_LINES)?,
            city_text: find(&screen, "city_panel")?,
            city_box: find(&screen, "city_box")?,
            city_scroll: find(&screen, "city_scroll")?,
            city_panel: None,
            panel_of: None,
            swallow_click: false,
            city_details: find(&screen, CITY_DETAILS_ID)?,
            menu_bar: find(&screen, "menu_bar")?,
            menu_room: find(&screen, "menu_room")?,
            menu_titles: titles,
            treasury_box: find(&screen, "treasury")?,
            menus,
            treasury: [
                Line::find(&screen, GOLD_ID)?,
                Line::find(&screen, GOLD_IN_ID)?,
                Line::find(&screen, GOLD_OUT_ID)?,
            ],
            sidebar: find(&screen, "main_sidebar")?,
            turn_row: find(&screen, "turn_row")?,
            undo: find(&screen, UNDO_ID)?,
            go_to: find(&screen, GO_TO_ID)?,
            done: find(&screen, DONE_ID)?,
            picking: None,
            refused: None,

            tasks: None,
            tasks_of: None,
            log: None,
            seen: Vec::new(),
            between: false,
            last_play: None,
            sight: Sight::all(),
            sight_of: None,
            cities_seen: Vec::new(),
            cycled: None,
            city_window: None,
            city_list: None,
            end_turn: Line::find(&screen, END_TURN_ID)?,
            agents,
            cover: None,
            last_person: None,
            game_over: None,
            wants_new_game: false,
            ask_new: None,
            battle: None,
            screen,
        };
        session.apply_layout_sizes();
        // The computer's turns that come before the first person's are
        // played by the frames, the log open; or the first person is handed
        // the game.
        session.between = !session.persons_turn();
        session.turn_passed();
        if session.between {
            session.open_log();
        }
        session.refresh();
        session.screen.settle();
        Ok(session)
    }

    /// True if it is the turn of a person at this screen.
    fn persons_turn(&self) -> bool {
        let game = &self.screen.game;
        game.controller(game.player()) == Some(&Controller::Person)
    }

    /// The person whose turn it is has done (End turn): the next side's
    /// turn. Their log opens, and shows what the others do, a turn at a time
    /// (the frames play the computer's); when it is a person's turn again,
    /// Start turn begins it.
    pub fn end_turn(&mut self) {
        // Only a person's turn is the screen's to end (once nobody here is in
        // the game, the computer plays on by itself).
        if self.cover.is_some() || self.screen.game.winner().is_some() || !self.persons_turn() {
            return;
        }
        (self.picking, self.refused) = (None, None);
        let side = self.screen.game.player();
        let length = self.screen.game.history().len();
        match self.seen.iter_mut().find(|(s, _)| *s == side) {
            Some((_, seen)) => *seen = length,
            None => self.seen.push((side, length)),
        }
        self.screen.game.end_turn();
        // Nobody else plays before this person's next turn (a side alone):
        // nothing to wait for.
        self.between = !(self.persons_turn() && self.screen.game.player() == side);
        if self.between {
            self.open_log();
        }
        self.turn_passed();
    }

    /// Start the turn of the person whose turn it is (Start turn): the log
    /// closes, and the map is theirs.
    pub fn start_turn(&mut self) {
        if !self.between || !self.persons_turn() || self.cover.is_some() {
            return;
        }
        self.between = false;
        if let Some(log) = self.log.take() {
            log.close(&mut self.screen.ui);
        }
    }

    /// The button under the minimap, or Ctrl+T: Start turn between turns,
    /// else End turn.
    fn turn_button(&mut self) {
        if self.between { self.start_turn() } else { self.end_turn() }
    }

    /// The computer plays one turn, that of the side whose turn it is, if the
    /// computer plays it.
    fn play_one(&mut self) {
        let game = &self.screen.game;
        let side = game.player();
        if game.winner().is_some() || !matches!(game.controller(side), Some(Controller::Computer(_))) {
            return;
        }
        let Some(agent) = self.agents.get_mut(side) else { return };
        agents::play_turn(&mut self.screen.game, agent);
        self.turn_passed();
    }

    /// Play the computer's turns now, up to a person's (a turn at most, once
    /// nobody here is in the game): what the frames do a turn at a time. For
    /// tests and scripts.
    pub fn wait_for_turn(&mut self) {
        for _ in 0..self.screen.game.order().len() + 1 {
            let before = (self.screen.game.player(), self.screen.game.turn());
            if self.cover.is_some() || self.persons_turn() {
                break;
            }
            self.play_one();
            if (self.screen.game.player(), self.screen.game.turn()) == before || !self.people_in_play() {
                break;
            }
        }
        self.refresh();
    }

    /// End the turn, play the others', and start the next: a round in one
    /// step, for scripts. False if it was not a person's turn to end.
    pub fn next_turn(&mut self) -> bool {
        if !self.persons_turn() || self.between {
            self.start_turn();
        }
        if !self.persons_turn() {
            return false;
        }
        self.end_turn();
        self.wait_for_turn();
        if let Some(cover) = self.cover.take() {
            build::close(&mut self.screen.ui, cover.window);
        }
        self.start_turn();
        self.refresh();
        true
    }

    /// A turn has passed: if it is a person's turn now, and not the person
    /// who played last (hotseat), the map is covered until they say they are
    /// there.
    fn turn_passed(&mut self) {
        // Nothing picked, nothing said, carries over to another turn.
        (self.picking, self.refused) = (None, None);
        let game = &self.screen.game;
        let side = game.player();
        let person = game.controller(side) == Some(&Controller::Person);
        let hotseat = game.setup().people() > 1;
        if person && game.winner().is_none() && hotseat && self.last_person != Some(side) {
            self.open_cover(side);
        }
        // In hotseat, a person's turn starts at their capital.
        if person && hotseat && self.last_person != Some(side) {
            if let Some(capital) = self.screen.game.capital(side) {
                self.screen.game.deselect();
                self.look_at(capital);
            }
        }
        if person {
            self.last_person = Some(side);
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
        // It is over: nothing asks whether to leave it any more.
        if let Some(ask) = self.ask_new.take() {
            build::close(&mut self.screen.ui, ask.window);
        }
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

    /// New game (the Game menu, Ctrl+N): once the game is over, straight to
    /// the new game screen; while it goes on, a window asks first (the game
    /// is not saved: it would be lost).
    pub fn ask_new_game(&mut self) {
        if self.screen.game.winner().is_some() {
            self.wants_new_game = true;
            return;
        }
        if self.ask_new.as_ref().is_some_and(|ask| build::is_open(&self.screen.ui, ask.window)) {
            return;
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let ui = &mut self.screen.ui;
        let parts = build::window(ui, root, ASK_NEW_GAME_ID, "New game");
        build::heading(ui, parts.content, "Leave this game for a new one?");
        build::text(ui, parts.content, "It is not over, and it is not saved: it will be lost.");
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        let buttons = build::row(ui, parts.content, space);
        build::button(ui, buttons, NEW_GAME_ID, "Leave it: new game");
        build::button(ui, buttons, KEEP_PLAYING_ID, "Keep playing");
        build::place(ui, parts, area, (0.5, 0.4));
        self.ask_new = Some(parts);
    }

    /// True while the window that asks about a new game is open.
    pub fn asks_new_game(&self) -> bool {
        self.ask_new.as_ref().is_some_and(|ask| build::is_open(&self.screen.ui, ask.window))
    }

    /// Size everything from the window's size: the map is 80% of the width and
    /// all the height the panel under it leaves; the minimap is the rest of
    /// the width and 30% of the height; the lines get the widths that leaves
    /// them. Called again whenever the window's size changes.
    /// C++: assets/layouts/main.lua
    fn apply_layout_sizes(&mut self) {
        let Size { w, h: window_h } = self.screen.ui.viewport();
        let dp = |screen: &MapScreen, dp: i32| screen.ui.dimension_converter().dp_to_px(dp);
        let spacing = dp(&self.screen, SPACING_DP);
        let map_w = (w as f32 * MAP_WIDTH_SHARE).floor() as i32;
        let side_w = (w - map_w - spacing).max(0);
        // The menu bar across the top; everything else under it.
        let ui = &mut self.screen.ui;
        let bar_h = ui.preferred_size_free(self.menu_bar).pref.h;
        ui.set_layout_params(self.menu_bar, Some(LayoutParams::fixed(w.max(0), bar_h)));
        // The treasury, if there is room for it beside the menus.
        let (titles, treasury) =
            (ui.preferred_size_free(self.menu_titles).pref.w, ui.preferred_size_free(self.treasury_box).pref.w);
        let fits = titles + treasury + 4 * spacing <= w;
        let visibility = if fits { Visibility::Visible } else { Visibility::Gone };
        if ui.node(self.treasury_box).is_some_and(|n| n.visibility() != visibility) {
            ui.set_visibility(self.treasury_box, visibility);
        }
        // The room between the menus and the treasury takes what is left.
        ui.set_layout_params(
            self.menu_room,
            Some(LayoutParams { min_w: Some(0), max_w: Some(w.max(0)), ..LayoutParams::default() }),
        );
        let h = (window_h - bar_h - dp(&self.screen, MENU_SPACING_DP)).max(0);

        let (status_h, line_h) = (dp(&self.screen, STATUS_DP), dp(&self.screen, LINE_DP));
        let gap = dp(&self.screen, SLOT_SPACING_DP);
        // The leader's box and the units', beside the buttons.
        let boxes = STACK_SLOTS as i32 + 1;
        let actions = self.screen.ui.preferred_size_free(self.stack_actions).pref.w;
        let room = map_w - actions - dp(&self.screen, STACK_BAR_SPACING_DP);
        let fit = (room - gap * (boxes - 1)) / boxes;
        let slot_w = dp(&self.screen, SLOT_DP).min(fit).max(0);
        // (Inside the box's padding and its layout's: 8 dp each side.)
        let inside = (slot_w - dp(&self.screen, 16)).max(0);
        let icon = dp(&self.screen, ICON_DP).min(inside);
        let ui = &mut self.screen.ui;
        ui.set_layout_params(self.status.widget, Some(LayoutParams::fixed(map_w, status_h)));
        ui.set_layout_params(self.hex_title.widget, Some(LayoutParams::fixed(map_w, line_h)));
        ui.set_layout_params(self.unit_line.widget, Some(LayoutParams::fixed(map_w, line_h)));
        for slot in std::iter::once(&self.leader).chain(&self.slots) {
            let mut lp = LayoutParams::default();
            lp.set_width(slot_w);
            ui.set_layout_params(slot.button, Some(lp));
            ui.set_layout_params(slot.icon, Some(LayoutParams::fixed(icon, icon)));
            // The numbers are cut short, not the box made wider, when it is
            // narrow.
            let numbers = LayoutParams { min_w: Some(0), max_w: Some(inside), ..LayoutParams::default() };
            ui.set_layout_params(slot.numbers.widget, Some(numbers));
        }
        // The slots do not push the sidebar off a narrow window.
        ui.set_layout_params(self.stack_row, Some(LayoutParams { max_w: Some(map_w), ..LayoutParams::default() }));
        for line in &self.city {
            ui.set_layout_params(line.widget, Some(LayoutParams::fixed(side_w, line_h)));
        }
        // Nor do the sidebar's buttons push it wider than its share (their
        // text is cut short instead).
        let narrow = LayoutParams { min_w: Some(0), max_w: Some(side_w), ..LayoutParams::default() };
        // The sidebar is as tall as the window (its rows of buttons alone
        // would make it only as tall as they are).
        ui.set_layout_params(self.sidebar, Some(LayoutParams { max_h: Some(h), ..narrow }));
        ui.set_layout_params(self.turn_row, Some(narrow));
        ui.set_layout_params(self.city_box, Some(narrow));
        // The panel is as tall as its lines and its slots (an empty slot keeps
        // its room), and the map has the rest.
        let panel_h = ui.preferred_size_free(self.panel).pref.h;
        let map_h = (h - spacing - panel_h).max(0);
        let minimap_h = h - (h as f32 * (1.0 - MINIMAP_HEIGHT_SHARE)).floor() as i32;
        // The city's box has what the minimap and the turn's buttons leave
        // of the sidebar (and scrolls if the city needs more).
        let turn_h = ui.preferred_size_free(self.turn_row).pref.h;
        let city_h = (h - minimap_h - turn_h - 2 * spacing).max(0);
        ui.set_layout_params(self.city_scroll, Some(LayoutParams::fixed(side_w, city_h)));
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
    /// Only the person at the screen's own units are shown in full: of
    /// another side's stack, how many there are (the hex's heading) and no
    /// more.
    fn stack_units(&self) -> Vec<UnitId> {
        let map = &self.screen.game.map;
        let viewer = self.seer();
        let mine = |id: &UnitId| map.units().get(*id).is_some_and(|u| viewer == Side::Grey || u.side() == viewer);
        map.units_at(self.shown).iter().copied().filter(mine).collect()
    }

    /// Work out what the person at the screen sees, if the map or the
    /// person changed; and draw the minimap again if what it shows of the
    /// cities changed.
    fn see(&mut self) {
        let (revision, viewer) = (self.screen.game.map.revision(), self.seer());
        if self.sight_of == Some((revision, viewer)) {
            return;
        }
        self.sight_of = Some((revision, viewer));
        self.sight = self.screen.game.sight(viewer);
        let seen: Vec<bool> = self.screen.game.map.cities().iter().map(|c| self.sight.sees(c.pos)).collect();
        if seen != self.cities_seen {
            self.cities_seen = seen;
            self.screen.redraw_minimap();
        }
    }

    /// What the person at the screen sees.
    pub fn sight(&self) -> &Sight {
        &self.sight
    }

    /// The numbers under the icons of the selected hex's units, the first unit
    /// first.
    pub fn stack(&self) -> Vec<&str> {
        let boxes = self.boxes();
        std::iter::once(&self.leader)
            .chain(&self.slots)
            .zip(boxes)
            .filter(|(_, unit)| unit.is_some())
            .map(|(slot, _)| slot.numbers.text.as_str())
            .collect()
    }

    /// The picture the `i`th unit's box shows: the image of the unit's kind
    /// and its side (`None` for an empty box). The leader's: [`Session::leader_icon`].
    pub fn slot_icon(&self, i: usize) -> Option<(&str, Side)> {
        self.slots.get(i)?.shows.as_ref().map(|(image, side, _)| (image.as_str(), *side))
    }

    /// The picture the leader's box shows, if the stack has one.
    pub fn leader_icon(&self) -> Option<(&str, Side)> {
        self.leader.shows.as_ref().map(|(image, side, _)| (image.as_str(), *side))
    }

    /// The units in the stack's boxes: its leader's first (`None` if it has
    /// none), then a box for each of the eight a hex holds, its units in their
    /// order (`None` for an empty box).
    fn boxes(&self) -> Vec<Option<UnitId>> {
        let map = &self.screen.game.map;
        let ids = self.stack_units();
        let leader = ids.iter().copied().find(|id| map.is_commander(*id));
        let mut boxes = vec![leader];
        boxes.extend(ids.iter().copied().filter(|id| Some(*id) != leader).map(Some));
        boxes.resize(STACK_SLOTS + 1, None);
        boxes
    }

    /// The `i`th box: the leader's, then the units'.
    fn slot(&self, i: usize) -> &Slot {
        if i == 0 { &self.leader } else { &self.slots[i - 1] }
    }

    fn slot_mut(&mut self, i: usize) -> &mut Slot {
        if i == 0 { &mut self.leader } else { &mut self.slots[i - 1] }
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
        let (image, side, level) = key;
        let picture = self.assets.image(image).or_else(|_| self.assets.image(DEFAULT_UNIT_IMAGE)).ok();
        let size = self.screen.ui.dimension_converter().dp_to_px(ICON_DP);
        let icon = unit_icon(picture.as_ref(), *side, *level, size);
        let name = format!("unit icon {image} {side:?} {level} {size}");
        let handle = self.screen.ui.image_from_pixels(&name, icon.size, &icon.rgba);
        self.icons.insert(key.clone(), handle.clone());
        handle
    }

    /// Bring the panels up to date with the game.
    fn refresh(&mut self) {
        self.see();
        let game = &self.screen.game;
        let map = &game.map;
        let cursor = game.map.cursor;
        let remembered = game.ghost_at(self.seer(), cursor).filter(|_| !self.sight.sees(cursor));
        let status = info::cursor_line_for(game, &self.sight, remembered);
        let ghost = game.ghost_at(self.seer(), self.shown).filter(|_| !self.sight.sees(self.shown));
        let title = info::hex_title_for(map, self.shown, self.sight.sees(self.shown), ghost);
        // The units of the selected hex in their boxes: each one's icon and
        // numbers, and which of them are selected.
        let ids = self.boxes();
        let units: Vec<Option<(IconKey, String)>> = ids
            .iter()
            .map(|id| {
                let unit = map.units().get((*id)?)?;
                let image = map.units().kind_of(unit).map_or(DEFAULT_UNIT_IMAGE, |kind| kind.image.as_str());
                Some(((image.to_string(), unit.side(), unit.level), info::unit_numbers(unit)))
            })
            .collect();
        let chosen: Vec<bool> = ids.iter().map(|id| id.is_some_and(|id| game.selection().contains(&id))).collect();
        // The empty leader's box says whose it is (the map's first kind of
        // commander).
        let leader_word =
            map.units().kinds().iter().find(|k| k.commander > 0).map_or("Leader", |k| k.name.as_str()).to_string();
        // The selected unit in a line, wherever it stands; a stack in a word.
        let line = match game.selected().and_then(|id| map.units().get(id)) {
            _ if self.picking == Some(Pick::GoTo) => match &self.refused {
                Some(why) => format!("Go to: {why}. Click another hex (Esc: never mind)."),
                None => {
                    "Go to: click the hex they go to, over as many turns as it takes (Esc: never mind).".to_string()
                }
            },
            _ if matches!(self.picking, Some(Pick::Rally(_))) => {
                "Click the hex the city sends its new units to (Esc: never mind).".to_string()
            }
            Some(unit) if unit.goto.is_some() && game.selection().len() == 1 => format!(
                "{} (on the way to {})",
                info::unit_line(map, unit),
                info::place(map, unit.goto.unwrap_or_default())
            ),
            Some(_) if game.selection().len() > 1 => format!(
                "{} units selected: they move together (click one to leave it behind, or to take it along).",
                game.selection().len()
            ),
            Some(unit) => info::unit_line(map, unit),
            None if ids.iter().all(Option::is_none) => String::new(),
            None => "Click your units to select them, then a hex to send them there.".to_string(),
        };
        // All about the unit whose box the pointer is on; or what the city in
        // the sidebar can build, whose button it is on.
        let hovered = self.screen.ui.hovered();
        let tipped = (0..=STACK_SLOTS).find(|i| Some(self.slot(*i).button) == hovered);
        let unit_tip = tipped.and_then(|i| map.units().get(ids[i]?)).map(|unit| info::unit_details(map, unit));
        let city_tip = self.city_panel.as_ref().and_then(|panel| {
            let (button, what) = panel.hovered(&self.screen.ui)?;
            let city = map.cities().get(panel.city())?;
            Some((button, info::production_details(map, city.units.get(what)?, city.trains)))
        });
        let (mut tip, tip_at) = match (unit_tip, city_tip) {
            (Some(lines), _) => (lines, tipped.map(|i| (self.slot(i).button, false))),
            (None, Some((button, lines))) => (lines, Some((button, true))),
            (None, None) => (Vec::new(), None),
        };
        tip.truncate(TIP_LINES);
        let city = map.city_at(self.shown).map_or(Vec::new(), |city| {
            let known = info::Known::of(city, self.seer(), &self.sight);
            info::city_lines_for(map, city, CITY_LINES, known)
        });

        self.status.set(&mut self.screen, &status);
        self.hex_title.set(&mut self.screen, &title);
        self.unit_line.set(&mut self.screen, &line);
        for (i, unit) in units.iter().enumerate() {
            let key = unit.as_ref().map(|(key, _)| key.clone());
            if self.slot(i).shows != key {
                let handle = key.as_ref().map_or(ImageHandle::default(), |key| self.icon(key));
                let icon = self.slot(i).icon;
                self.screen.ui.with_widget::<ImageWidget, _>(icon, |w, cx| w.set_image(cx, handle));
                self.slot_mut(i).shows = key;
            }
            let empty_word = if i == 0 { leader_word.clone() } else { String::new() };
            let numbers = unit.as_ref().map_or(empty_word.as_str(), |(_, numbers)| numbers.as_str()).to_string();
            let button = self.slot(i).button;
            let screen = &mut self.screen;
            let slot = if i == 0 { &mut self.leader } else { &mut self.slots[i - 1] };
            slot.numbers.set(screen, &numbers);
            // An empty box stays, dimmed.
            if screen.ui.node(button).is_some_and(|n| n.is_disabled() != unit.is_none()) {
                screen.ui.set_disabled(button, unit.is_none());
            }
            // The selected units' boxes are pressed in.
            let pressed = i32::from(chosen.get(i).copied().unwrap_or(false));
            if screen.ui.value(button).as_int() != pressed {
                screen.ui.set_value(button, pressed);
            }
        }
        for (i, line) in self.tip_lines.iter_mut().enumerate() {
            line.set(&mut self.screen, tip.get(i).map_or("", String::as_str));
            // The box is as tall as what it says.
            let visibility = if i < tip.len() { Visibility::Visible } else { Visibility::Gone };
            set_visibility(&mut self.screen, line.widget, visibility);
        }
        match tip_at.filter(|_| !tip.is_empty()) {
            Some((button, beside)) => self.open_tip(button, beside),
            None => set_visibility(&mut self.screen, self.tip, Visibility::Gone),
        }
        // The city: one of the player's in full in its panel; another's in
        // lines.
        let own = self.refresh_city_panel();
        let city = if own { Vec::new() } else { city };
        for (i, line) in self.city.iter_mut().enumerate() {
            line.set(&mut self.screen, city.get(i).map_or("", String::as_str));
        }
        let (text, panel) =
            if own { (Visibility::Gone, Visibility::Visible) } else { (Visibility::Visible, Visibility::Gone) };
        set_visibility(&mut self.screen, self.city_text, text);
        set_visibility(&mut self.screen, self.city_scroll, panel);
        let details = if city.is_empty() { Visibility::Gone } else { Visibility::Visible };
        set_visibility(&mut self.screen, self.city_details, details);
        // The button says whose turn it ends (on a map without sides,
        // nobody's), or, between turns, that the next can start; while the
        // computer plays, whose turn it waits for.
        let game = &self.screen.game;
        let (person, side) = (self.persons_turn(), game.player());
        let theirs = person && game.winner().is_none();
        let label = match side {
            _ if game.winner().is_some() => "Game over".to_string(),
            Side::Grey => "End turn".to_string(),
            _ if !person => format!("Waiting for {}", side_name(side)),
            _ if self.between => "Start turn".to_string(),
            side => format!("End {}'s turn", side_name(side)),
        };
        self.end_turn.set(&mut self.screen, &label);
        // It is a person's to press, on their turn.
        self.screen.ui.set_disabled(self.end_turn.widget, !theirs);
        // The treasury of the person at the screen (none on a map without
        // sides): "34", "+40", "-32".
        let viewer = self.viewer();
        let gold = if viewer == Side::Grey {
            [String::new(), String::new(), String::new()]
        } else {
            let game = &self.screen.game;
            [game.gold(viewer).to_string(), format!("+{}", game.income(viewer)), format!("-{}", game.upkeep(viewer))]
        };
        for (line, text) in self.treasury.iter_mut().zip(&gold) {
            line.set(&mut self.screen, text);
        }
        // While a menu is open, the pointer on another title opens that one.
        if let Some(title) = self.menus.hovered_title(&self.screen.ui) {
            if self.menus.open_menu(&self.screen.ui).is_some_and(|open| open != title) {
                self.open_menu(title);
            }
        }
        // Undo is there while there is something to take back.
        let undo = self.screen.game.can_undo();
        self.screen.ui.set_disabled(self.undo, !undo);
        // Go to and Done are about the person's selected units.
        let game = &self.screen.game;
        let own = self.persons_turn()
            && game.selected().and_then(|id| game.map.units().get(id)).is_some_and(|u| u.side() == game.player());
        for button in [self.go_to, self.done] {
            if self.screen.ui.node(button).is_some_and(|n| n.is_disabled() == own) {
                self.screen.ui.set_disabled(button, !own);
            }
        }
        if let (Some(winner), None) = (self.screen.game.winner(), self.game_over) {
            self.open_game_over(winner);
        }
        self.refresh_windows();
    }

    /// The windows about cities: forget those that were closed, and bring the
    /// others up to date.
    fn refresh_windows(&mut self) {
        // The log says what the history says now, to the player of now, and
        // whose turn it is while the others play.
        let viewer = self.viewer();
        let new_from = self.seen_by(viewer);
        let status = self.log_status();
        if self.log.as_ref().is_some_and(|log| !log.is_open(&self.screen.ui)) {
            self.log = None;
        }
        if self.battle.as_ref().is_some_and(|battle| !battle.is_open(&self.screen.ui)) {
            self.battle = None;
        }
        // The tasks: the person's, as the game is now (a refill is worked
        // out only when the map changed).
        if self.tasks.as_ref().is_some_and(|tasks| !tasks.is_open(&self.screen.ui)) {
            self.tasks = None;
        }
        let revision = self.screen.game.map.revision();
        if let Some(tasks) = &mut self.tasks {
            if self.tasks_of != Some((revision, viewer))
                && tasks.refresh(&mut self.screen.ui, &self.screen.game, viewer)
            {
                self.screen.settle();
            }
        }
        self.tasks_of = Some((revision, viewer));
        if let Some(log) = &mut self.log {
            let refilled = log.refresh(&mut self.screen.ui, &self.screen.game, viewer, new_from);
            log.set_status(&mut self.screen.ui, &status);
            if refilled {
                self.screen.settle();
            }
        }
        let (seer, actor) = (self.seer(), self.actor());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        if let Some(window) = &mut self.city_window {
            let known = game
                .map
                .cities()
                .get(window.city())
                .map_or(info::Known::All, |c| info::Known::of(c, seer, &self.sight));
            if !window.is_open(ui) || !window.refresh(ui, &game.map, (actor, known)) {
                window.close(ui);
                self.city_window = None;
            }
        }
        if let Some(list) = &mut self.city_list {
            if list.is_open(ui) {
                let sight = &self.sight;
                list.refresh(ui, &game.map, &|hex| sight.sees(hex));
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
        let icons = self.production_icons(id);
        let (seer, actor) = (self.seer(), self.actor());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        let known = game.map.cities().get(id).map_or(info::Known::All, |c| info::Known::of(c, seer, &self.sight));
        self.city_window = CityWindow::open(ui, root, &game.map, id, (actor, known), &icons, (area, CITY_ALIGN));
    }

    /// The icons of what a city can build, in its holder's colors.
    fn production_icons(&mut self, id: CityId) -> Vec<ImageHandle> {
        let keys: Vec<IconKey> = self.screen.game.map.cities().get(id).map_or(Vec::new(), |city| {
            let units = self.screen.game.map.units();
            city.units
                .iter()
                .map(|p| {
                    let kind = units.kind_named(&p.basename).and_then(|k| units.kind(k));
                    (kind.map_or(DEFAULT_UNIT_IMAGE, |k| k.image.as_str()).to_string(), city.side, city.trains)
                })
                .collect()
        });
        keys.iter().map(|key| self.icon(key)).collect()
    }

    /// The selected hex's city in the sidebar, if it is one the person at
    /// the screen knows in full (their own): made anew for another city, or
    /// when its holder changed; brought up to date. True if it shows.
    fn refresh_city_panel(&mut self) -> bool {
        let seer = self.seer();
        let wanted = self.screen.game.map.city_at(self.shown).and_then(|city| {
            (info::Known::of(city, seer, &self.sight) == info::Known::All).then_some((city.id, city.side))
        });
        // Made anew for another city, or for the same one under another
        // holder (its icons are in its holder's colors).
        if self.panel_of != wanted || self.city_panel.is_none() != wanted.is_none() {
            if let Some(panel) = self.city_panel.take() {
                panel.close(&mut self.screen.ui);
            }
            if let Some((id, _)) = wanted {
                let (icons, actor) = (self.production_icons(id), self.actor());
                let (ui, game) = (&mut self.screen.ui, &self.screen.game);
                self.city_panel = CityPanel::open(ui, self.city_box, &game.map, id, (actor, info::Known::All), &icons);
            }
            self.panel_of = wanted;
        }
        let actor = self.actor();
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        if let Some(panel) = &mut self.city_panel {
            if !panel.refresh(ui, &game.map, (actor, info::Known::All)) {
                if let Some(panel) = self.city_panel.take() {
                    panel.close(ui);
                }
            }
        }
        self.city_panel.is_some()
    }

    /// The selected hex's city in the sidebar, while it shows there.
    pub fn city_panel(&self) -> Option<&CityPanel> {
        self.city_panel.as_ref()
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
        let viewer = self.viewer();
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        let sight = &self.sight;
        let seen = |hex| sight.sees(hex);
        let mut list = CityList::open(ui, root, &game.map, own, viewer, (area, LIST_ALIGN), &seen);
        // The selected hex's city, if it is in the list, is the chosen one.
        if let Some(city) = game.map.city_at(self.shown) {
            list.choose(ui, &game.map, city.id, &seen);
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

    /// Close the menu, or else the window on top (the one opened last of a
    /// battle's, the log, a city's, the list, the tasks). False if none was
    /// open.
    fn close_top_window(&mut self) -> bool {
        let root = self.screen.root_window();
        let ui = &mut self.screen.ui;
        if self.menus.close(ui) {
            return true;
        }
        let open = |w: WidgetId| build::is_open(ui, w).then_some(w);
        let windows = [
            self.ask_new.as_ref().and_then(|w| open(w.window)),
            self.battle.as_ref().and_then(|w| open(w.window())),
            self.log.as_ref().and_then(|w| open(w.window())),
            self.city_window.as_ref().and_then(|w| open(w.window())),
            self.city_list.as_ref().and_then(|w| open(w.window())),
            self.tasks.as_ref().and_then(|w| open(w.window())),
        ];
        // The one on top: the last of them among the root window's children.
        let order = ui.children(root).to_vec();
        let top = (0..windows.len())
            .filter_map(|i| Some((order.iter().position(|c| Some(*c) == windows[i])?, i)))
            .max()
            .map(|(_, i)| i);
        match top {
            Some(0) => self.ask_new.take().map(|w| build::close(ui, w.window)).is_some(),
            Some(1) => self.battle.take().map(|w| w.close(ui)).is_some(),
            Some(2) => self.log.take().map(|w| w.close(ui)).is_some(),
            Some(3) => self.city_window.take().map(|w| w.close(ui)).is_some(),
            Some(4) => self.city_list.take().map(|w| w.close(ui)).is_some(),
            Some(5) => self.tasks.take().map(|w| w.close(ui)).is_some(),
            _ => false,
        }
    }

    /// Do what a window asked.
    fn obey(&mut self, order: CityOrder) {
        match order {
            CityOrder::SetProduction(id, what) => {
                // An order: only the city's holder, if that is the player.
                let _ = self.screen.game.apply(Order::Build { city: id, what });
            }
            CityOrder::Details(id) => self.open_city(id),
            CityOrder::PickRally(id) => self.picking = Some(Pick::Rally(id)),
            CityOrder::NoRally(id) => {
                let _ = self.screen.game.apply(Order::Rally { city: id, to: None });
            }
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
            self.open_shown_city();
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
        if let Some(order) = self.city_panel.as_ref().and_then(|p| p.click(id)) {
            self.obey(order);
            return true;
        }
        let (ui, map, sight) = (&mut self.screen.ui, &self.screen.game.map, &self.sight);
        let mut handled = false;
        let mut order = None;
        if let Some(list) = &mut self.city_list {
            let rows = list.cities().len();
            let is_list_button = (0..rows).any(|i| id == TbId::new(&city_windows::row_id(i)))
                || id == TbId::new(city_windows::LIST_DETAILS_ID)
                || id == TbId::new(city_windows::LIST_GOTO_ID);
            if is_list_button {
                order = list.click(ui, map, id, &|hex| sight.sees(hex));
                handled = true;
            }
        }
        if let Some(order) = order {
            self.obey(order);
        }
        handled
    }

    /// Show the box about a unit over its box (under it if there is no room
    /// above, and wherever it fits in a window too small for either), or
    /// `beside` a button of the sidebar (to its left), inside the window.
    fn open_tip(&mut self, slot: WidgetId, beside: bool) {
        set_visibility(&mut self.screen, self.tip, Visibility::Visible);
        let ui = &mut self.screen.ui;
        let size = ui.preferred_size_free(self.tip).pref;
        let window = ui.viewport();
        let spacing = ui.dimension_converter().dp_to_px(SPACING_DP);
        let at = Rect::from_origin_size(ui.to_root(slot, Point::ZERO), ui.rect(slot).size());
        if beside {
            let x = (at.x - spacing - size.w).max(0);
            let y = at.y.min(window.h - size.h).max(0);
            ui.set_rect(self.tip, Rect::new(x, y, size.w, size.h));
            return;
        }
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

    /// One frame of host work before drawing (see `MapScreen::frame`). On the
    /// computer's turns, it plays one, [`COMPUTER_TURN_MS`] after the last.
    pub fn frame(&mut self, now_ms: u64) {
        let due = self.last_play.is_none_or(|last| now_ms >= last.saturating_add(COMPUTER_TURN_MS));
        if !self.persons_turn() && self.cover.is_none() && due {
            let before = (self.screen.game.player(), self.screen.game.turn());
            self.play_one();
            if (self.screen.game.player(), self.screen.game.turn()) != before {
                self.last_play = Some(now_ms);
            }
        }
        if let Some(battle) = &mut self.battle {
            battle.tick(&mut self.screen.ui, now_ms);
        }
        self.screen.frame(now_ms);
        self.refresh();
    }

    /// The player chose a hex of the map (a click, or Enter at the cursor):
    /// the game selects or moves a unit, and the panels are about that hex
    /// from now on.
    fn choose(&mut self, hex: Hex) {
        // While another side plays, a click only looks.
        if !self.persons_turn() {
            self.screen.game.deselect();
            self.shown = hex;
            return;
        }
        // An order starts the turn, if it has not been started.
        if self.between {
            self.start_turn();
        }
        let before = self.screen.game.history().len();
        if let SelectOutcome::Fought(_) = self.screen.game.select_hex(hex) {
            // The battle, played out.
            let history = self.screen.game.history();
            let battle = (before..history.len()).find(|i| matches!(history[*i].event, Event::Battle { .. }));
            if let Some(record) = battle {
                self.open_battle(record);
            }
        }
        self.shown = hex;
    }

    /// Watch the battle of the history's record `record`, from the start, if
    /// the game still keeps it.
    pub fn open_battle(&mut self, record: usize) {
        let Some(fight) = self.screen.game.fight(record).cloned() else { return };
        if let Some(old) = self.battle.take() {
            old.close(&mut self.screen.ui);
        }
        let icons = [&fight.attackers, &fight.defenders]
            .map(|units| units.iter().map(|u| self.icon(&(u.image.clone(), u.side, u.level))).collect::<Vec<_>>());
        let game = &self.screen.game;
        let place = game
            .map
            .city_at(fight.at)
            .map_or(format!("row {}, column {}", fight.at.row, fight.at.col), |c| c.name.clone());
        // How it went, to the one watching (who fought it).
        let viewer =
            if [fight.attacker, fight.defender].contains(&self.viewer()) { self.viewer() } else { fight.attacker };
        let outcome = info::battle_outcome(game, record, viewer);
        let (root, area) = (self.screen.root_window(), self.window_area());
        let window =
            BattleWindow::open(&mut self.screen.ui, root, &fight, icons, (&place, outcome), (area, (0.5, 0.2)));
        self.battle = Some(window);
        self.screen.settle();
    }

    /// The battle being watched, while its window is open.
    pub fn battle(&self) -> Option<&BattleWindow> {
        self.battle.as_ref().filter(|b| b.is_open(&self.screen.ui))
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
        let whose = (side, self.seen_by(side));
        let status = self.log_status();
        let mut log = LogWindow::open(&mut self.screen.ui, root, &self.screen.game, whose, (area, (0.5, 0.3)));
        log.set_status(&mut self.screen.ui, &status);
        self.log = Some(log);
        self.screen.settle();
    }

    /// True if something happened to `side` since it last ended a turn.
    fn has_news(&self, side: Side) -> bool {
        let game = &self.screen.game;
        let records = game.history().get(self.seen_by(side)..).unwrap_or_default();
        records.iter().any(|r| !info::record_lines(&game.map, r, side).is_empty())
    }

    /// How long the history was when `side` last ended a turn (0: never).
    fn seen_by(&self, side: Side) -> usize {
        self.seen.iter().find(|(s, _)| *s == side).map_or(0, |(_, seen)| *seen)
    }

    /// The log's line on top: whose turn it is while the others play, and
    /// then that the person's can start.
    fn log_status(&self) -> String {
        let game = &self.screen.game;
        let side = game.player();
        if let Some(winner) = game.winner() {
            return format!("{} has won.", title(side_name(winner)));
        }
        if !self.persons_turn() && side != Side::Grey {
            return format!("{} is playing...", title(side_name(side)));
        }
        if self.between { "Your turn: press Start turn when you are ready.".to_string() } else { String::new() }
    }

    /// What the log's line on top says, while it is open.
    pub fn log_status_line(&self) -> String {
        self.log.as_ref().filter(|log| log.is_open(&self.screen.ui)).map_or(String::new(), |log| log.status().into())
    }

    /// The records of the battles the log's Watch buttons play, while it is
    /// open.
    pub fn log_watchable(&self) -> Vec<usize> {
        self.log.as_ref().filter(|log| log.is_open(&self.screen.ui)).map_or(Vec::new(), |l| l.watchable())
    }

    /// The new lines of the log, while it is open.
    pub fn log_new_lines(&self) -> Vec<String> {
        self.log
            .as_ref()
            .filter(|log| log.is_open(&self.screen.ui))
            .map_or(Vec::new(), |l| l.new_lines(&self.screen.ui))
    }

    /// Whose log the screen shows: the person whose turn it is, or, in the
    /// computer's turns, the person who played last.
    fn viewer(&self) -> Side {
        let game = &self.screen.game;
        let side = game.player();
        if game.controller(side) == Some(&Controller::Person) { side } else { self.last_person.unwrap_or(side) }
    }

    /// Who gives the orders at this screen now: the side whose turn it is, if
    /// a person plays it here; else nobody (`Side::Grey`: the buttons that
    /// give orders are greyed).
    fn actor(&self) -> Side {
        if self.persons_turn() && !self.covered() { self.screen.game.player() } else { Side::Grey }
    }

    /// Whom the map and the panels show what they know: the person at the
    /// screen (fog of war, the other sides' details hidden). Nobody
    /// (`Side::Grey`, all shown) while no person plays here, or none is left
    /// in the game.
    pub fn seer(&self) -> Side {
        if self.people_in_play() { self.viewer() } else { Side::Grey }
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
        let (side, mine) = (self.viewer(), self.persons_turn());
        let game = &self.screen.game;
        let selected = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        let Some(hex) = game.next_stack_of(side, selected.or(self.cycled), back) else { return };
        let game = &mut self.screen.game;
        game.deselect();
        // (On another side's turn, only looked at.)
        if mine {
            game.select_hex(hex);
        }
        self.cycled = Some(hex);
        self.look_at(hex);
    }

    /// Go to the next city of the player's (or the one before), and look at
    /// it; nothing stays selected.
    pub fn next_city(&mut self, back: bool) {
        let side = self.viewer();
        let Some(hex) = self.screen.game.next_city_of(side, self.shown, back) else { return };
        self.screen.game.deselect();
        self.look_at(hex);
    }

    /// Go to (G, or the Go to button): the next click on the map (or Enter)
    /// picks where the selected units go, over as many turns as it takes.
    pub fn start_go_to(&mut self) {
        let game = &self.screen.game;
        let own = game.selected().and_then(|id| game.map.units().get(id)).is_some_and(|u| u.side() == game.player());
        if own && self.persons_turn() && !self.covered() {
            (self.picking, self.refused) = (Some(Pick::GoTo), None);
        }
    }

    /// Done (D, or the Done button): the selected units are put aside for
    /// this turn (Next stack and the tasks pass them by), and the next
    /// stack that waits is selected.
    pub fn done_with_stack(&mut self) {
        if self.covered() || !self.persons_turn() {
            return;
        }
        let units = self.screen.game.selection().to_vec();
        if units.is_empty() || self.screen.game.apply(Order::Done { units }).is_err() {
            return;
        }
        self.picking = None;
        let game = &self.screen.game;
        let here = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        self.screen.game.deselect();
        // On to the next, if one waits for orders.
        let waits = self
            .screen
            .game
            .tasks(self.screen.game.player())
            .iter()
            .any(|t| t.tier() == 1 && matches!(t, Task::Stack { .. }));
        if waits {
            self.cycled = here.or(self.cycled);
            self.next_stack(false);
        }
    }

    /// What a pick on the map does: the selected units go there, or the
    /// city sends its new units there.
    fn picked(&mut self, pick: Pick, hex: Hex) {
        if self.between && self.persons_turn() {
            self.start_turn();
        }
        match pick {
            Pick::GoTo => {
                let units = self.screen.game.selection().to_vec();
                match self.screen.game.apply(Order::GoTo { units: units.clone(), to: hex }) {
                    Ok(_) => {
                        // The cursor follows them; nothing stays selected.
                        let game = &self.screen.game;
                        let at = units.iter().find_map(|id| game.map.units().get(*id)).map(|u| u.pos());
                        self.screen.game.deselect();
                        if let Some(at) = at {
                            self.screen.game.set_cursor(at);
                            self.shown = at;
                        }
                    }
                    // Refused: say why, and pick again.
                    Err(why) => {
                        self.refused = Some(why);
                        self.picking = Some(Pick::GoTo);
                    }
                }
            }
            Pick::Rally(city) => {
                let _ = self.screen.game.apply(Order::Rally { city, to: Some(hex) });
            }
        }
    }

    /// See to a task of the list: select its stack and look at it, or open
    /// its city.
    fn see_to(&mut self, task: Task) {
        self.picking = None;
        let mine = self.persons_turn();
        match task {
            Task::Stack { at, .. } => {
                let game = &mut self.screen.game;
                game.deselect();
                if !mine {
                    self.look_at(at);
                    return;
                }
                if game
                    .map
                    .units_at(at)
                    .iter()
                    .filter_map(|id| game.map.units().get(*id))
                    .any(|u| u.side() == game.player())
                {
                    game.select_hex(at);
                }
                self.cycled = Some(at);
                self.look_at(at);
            }
            Task::City { city, .. } => {
                if let Some(pos) = self.screen.game.map.cities().get(city).map(|c| c.pos) {
                    self.screen.game.deselect();
                    self.look_at(pos);
                    self.open_city(city);
                }
            }
        }
    }

    /// Open the list of tasks (T, or the Tasks button), or close it.
    pub fn toggle_tasks(&mut self) {
        if let Some(tasks) = self.tasks.take() {
            if tasks.is_open(&self.screen.ui) {
                tasks.close(&mut self.screen.ui);
                return;
            }
        }
        if self.covered() {
            return;
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let side = self.viewer();
        let window = TasksWindow::open(&mut self.screen.ui, root, &self.screen.game, side, (area, TASKS_ALIGN));
        self.tasks = Some(window);
        self.screen.settle();
    }

    /// The list of tasks, while it is open.
    pub fn tasks_window(&self) -> Option<&TasksWindow> {
        self.tasks.as_ref().filter(|t| t.is_open(&self.screen.ui))
    }

    /// What the next click on the map picks, if anything.
    pub fn picking(&self) -> Option<Pick> {
        self.picking
    }

    /// The window of the selected hex's city (O, City details), if it is one.
    pub fn open_shown_city(&mut self) {
        if let Some(city) = self.screen.game.map.city_at(self.shown).map(|c| c.id) {
            self.open_city(city);
        }
    }

    /// Look at the person's capital (H; in hotseat each turn starts there).
    pub fn go_to_capital(&mut self) {
        if let Some(capital) = self.screen.game.capital(self.viewer()) {
            self.look_at(capital);
        }
    }

    /// Open the menu with the title `id`, as it is now.
    fn open_menu(&mut self, id: TbId) {
        let Some(menu) = self.menu(id) else { return };
        let root = self.screen.root_window();
        self.menus.open(&mut self.screen.ui, root, &menu);
    }

    /// The menu with the title `id`: its items, each with its keys, greyed
    /// where it can do nothing now.
    fn menu(&self, id: TbId) -> Option<Menu> {
        let game = &self.screen.game;
        let command = |key: char| format!("{}+{key}", command_key());
        let turn = self.persons_turn() && game.winner().is_none() && !self.covered();
        let units = game.selected().and_then(|id| game.map.units().get(id));
        let own = turn && units.is_some_and(|u| u.side() == game.player());
        let city = game.map.city_at(self.shown).is_some();
        let people = self.people_in_play();
        let menu = if id == TbId::new(MENU_GAME_ID) {
            Menu::new(MENU_GAME_ID)
                .item(END_TURN_ID, if self.between { "Start turn" } else { "End turn" }, &command('T'), turn)
                .item(UNDO_ID, "Undo", &command('Z'), turn && game.can_undo())
                .separator()
                .item(NEW_GAME_ASK_ID, "New game...", &command('N'), true)
                .item(QUIT_ID, "Quit", &command('Q'), true)
        } else if id == TbId::new(MENU_UNITS_ID) {
            Menu::new(MENU_UNITS_ID)
                .item(NEXT_STACK_ID, "Next stack", "N", people)
                .item(PREV_STACK_ID, "Previous stack", "Shift+N", people)
                .separator()
                .item(GO_TO_ID, "Go to...", "G", own)
                .item(DONE_ID, "Done for this turn", "D", own)
                .item(DESELECT_ID, "Drop the selection", "Esc", units.is_some())
        } else if id == TbId::new(MENU_CITIES_ID) {
            Menu::new(MENU_CITIES_ID)
                .item(NEXT_CITY_ID, "Next city", "C", people)
                .item(PREV_CITY_ID, "Previous city", "Shift+C", people)
                .item(CITY_DETAILS_ID, "This city's window", "O", city)
                .separator()
                .item(MY_CITIES_ID, "My cities", "M", people)
                .item(ALL_CITIES_ID, "All cities", "A", true)
        } else if id == TbId::new(MENU_VIEW_ID) {
            Menu::new(MENU_VIEW_ID)
                .item(TASKS_ID, "Tasks", "T", people)
                .item(LOG_ID, "Log", "L", true)
                .separator()
                .item(CAPITAL_ID, "Your capital", "H", people)
                .item(ZOOM_IN_ID, "Zoom in", "2", true)
                .item(ZOOM_OUT_ID, "Zoom out", "1", true)
                .item(FULLSCREEN_ID, "Fullscreen", fullscreen_keys(), true)
        } else if id == TbId::new(MENU_HELP_ID) {
            let mut menu = Menu::new(MENU_HELP_ID).item(HELP_KEYS_ID, "Keys and mouse", "F1  or  ?", true).separator();
            for (topic, title) in guide::TOPICS {
                menu = menu.item(topic, title, "", true);
            }
            menu
        } else {
            return None;
        };
        Some(menu)
    }

    /// The menu open, if one is, and what its items are: the id, the words
    /// and the keys of each.
    pub fn open_menu_items(&self) -> Option<(TbId, Vec<ItemWords<'_>>)> {
        Some((self.menus.open_menu(&self.screen.ui)?, self.menus.items()))
    }

    /// The cursor goes to a hex, the view to the middle of it, and the panels
    /// are about it.
    fn look_at(&mut self, hex: Hex) {
        self.screen.game.set_cursor(hex);
        self.screen.view.center_on(hex);
        self.shown = hex;
    }

    /// True while a side that a person plays at this screen is still in the
    /// game.
    fn people_in_play(&self) -> bool {
        let game = &self.screen.game;
        game.order().iter().any(|s| !game.out().contains(s) && game.controller(*s) == Some(&Controller::Person))
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
                    // Their log, if anything happened to them since they
                    // last played; then Start turn.
                    let side = self.screen.game.player();
                    self.between = self.has_news(side);
                    if self.between {
                        self.open_log();
                    }
                }
            }
            self.refresh();
            return;
        }
        for command in self.screen.take_map_commands() {
            // A press on the map, or Esc, closes an open menu, and does
            // nothing else (nor does the click the press makes).
            let menu_open = self.menus.open_menu(&self.screen.ui).is_some();
            match command {
                MapCommand::Press(..) | MapCommand::Key(MapKey::Esc) if menu_open => {
                    self.menus.close(&mut self.screen.ui);
                    self.swallow_click = matches!(command, MapCommand::Press(..));
                    continue;
                }
                MapCommand::Press(..) => self.swallow_click = false,
                MapCommand::Click(..) if self.swallow_click => {
                    self.swallow_click = false;
                    continue;
                }
                _ => {}
            }
            // Esc closes the help if it is open, else the window on top, and
            // with none open it drops the selection.
            if command == MapCommand::Key(MapKey::Esc)
                && !self.screen.help_open()
                && self.picking.take().is_none()
                && !self.close_top_window()
            {
                self.screen.game.deselect();
            }
            self.screen.navigate(command);
            // A click, or Enter at the cursor, chooses a hex (or picks it).
            // C++: MapWidget::OnEvent
            let hex = match command {
                MapCommand::Click(p, size) => self.screen.pick(p, size),
                MapCommand::Key(MapKey::Enter) => Some(self.screen.game.map.cursor),
                _ => None,
            };
            match (hex, self.picking.take()) {
                (Some(hex), Some(pick)) => {
                    self.refused = None;
                    self.picked(pick, hex);
                }
                (Some(hex), None) => self.choose(hex),
                (None, pick) => self.picking = pick,
            }
        }
        for command in self.screen.pump_shared() {
            // A title opens its menu, or closes it if it is open; anything
            // else closes the menu open (a click on an item then does what
            // it says).
            if let RootCommand::Click(id) = command {
                if self.menus.is_title(id) {
                    if self.menus.open_menu(&self.screen.ui) == Some(id) {
                        self.menus.close(&mut self.screen.ui);
                    } else {
                        self.open_menu(id);
                    }
                    continue;
                }
            }
            self.menus.close(&mut self.screen.ui);
            match command {
                RootCommand::Click(id) if id == TbId::new(END_TURN_ID) => self.turn_button(),
                RootCommand::Click(id) if id == TbId::new(PREV_STACK_ID) => self.next_stack(true),
                RootCommand::Click(id) if id == TbId::new(PREV_CITY_ID) => self.next_city(true),
                RootCommand::Click(id) if id == TbId::new(DESELECT_ID) => {
                    self.picking = None;
                    self.screen.game.deselect();
                }
                RootCommand::Click(id) if id == TbId::new(CAPITAL_ID) => self.go_to_capital(),
                RootCommand::Click(id) if guide::TOPICS.iter().any(|(topic, _)| id == TbId::new(topic)) => {
                    let topic = guide::TOPICS.iter().find(|(topic, _)| id == TbId::new(topic)).map(|(t, _)| *t);
                    if let Some((title, sections)) = topic.and_then(|t| guide::topic(&self.screen.game, t)) {
                        self.screen.show_topic(&format!("Help: {}", title.to_lowercase()), &sections);
                    }
                }
                // Ctrl+T (Cmd+T) ends the turn too, or starts it. C++:
                // main.cpp (the commented-out Ctrl+T)
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'t') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.turn_button();
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
                RootCommand::Click(id) if id == TbId::new(GO_TO_ID) => self.start_go_to(),
                RootCommand::Click(id) if id == TbId::new(DONE_ID) => self.done_with_stack(),
                RootCommand::Click(id) if id == TbId::new(TASKS_ID) => self.toggle_tasks(),
                RootCommand::Click(id) if id == TbId::new(tasks_window::TASKS_CLOSE_ID) => {
                    if let Some(tasks) = self.tasks.take() {
                        tasks.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id) if self.tasks.as_ref().and_then(|t| t.click(id)).is_some() => {
                    if let Some(task) = self.tasks.as_ref().and_then(|t| t.click(id)).cloned() {
                        self.see_to(task);
                    }
                }
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
                        'g' | 'G' => self.start_go_to(),
                        'd' | 'D' => self.done_with_stack(),
                        't' | 'T' => self.toggle_tasks(),
                        'm' | 'M' => self.toggle_list(true),
                        'a' | 'A' => self.toggle_list(false),
                        'o' | 'O' => self.open_shown_city(),
                        'h' | 'H' => self.go_to_capital(),
                        _ => {}
                    }
                }
                RootCommand::Click(id) if id == TbId::new(NEW_GAME_ID) => self.wants_new_game = true,
                RootCommand::Click(id) if id == TbId::new(NEW_GAME_ASK_ID) => self.ask_new_game(),
                RootCommand::Key(ch, mods)
                    if ch.eq_ignore_ascii_case(&'n') && mods.intersects(Modifiers::CTRL | Modifiers::SUPER) =>
                {
                    self.ask_new_game();
                }
                RootCommand::Click(id) if id == TbId::new(KEEP_PLAYING_ID) => {
                    if let Some(ask) = self.ask_new.take() {
                        build::close(&mut self.screen.ui, ask.window);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(battle_window::BATTLE_CLOSE_ID) => {
                    if let Some(battle) = self.battle.take() {
                        battle.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(battle_window::BATTLE_SKIP_ID) => {
                    if let Some(battle) = &mut self.battle {
                        battle.skip(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(battle_window::BATTLE_AGAIN_ID) => {
                    if let Some(battle) = &mut self.battle {
                        battle.again(&mut self.screen.ui);
                    }
                }
                // A battle's Watch button in the log.
                RootCommand::Click(id) if self.log.as_ref().and_then(|l| l.watch(id)).is_some() => {
                    if let Some(record) = self.log.as_ref().and_then(|l| l.watch(id)) {
                        self.open_battle(record);
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
                    // A box of the stack: the leader's, or the `i`th unit's.
                    let i = if id == TbId::new(LEADER_ID) {
                        Some(0)
                    } else {
                        (0..STACK_SLOTS).find(|i| id == TbId::new(&unit_button_id(*i))).map(|i| i + 1)
                    };
                    let unit = i.and_then(|i| self.boxes().get(i).copied().flatten());
                    if let (Some(unit), true) = (unit, self.persons_turn()) {
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

    fn next_turn(&mut self) -> bool {
        Session::next_turn(self)
    }

    /// Where the selected unit can go.
    fn marked(&self, hex: Hex) -> bool {
        self.screen.game.can_move_to(hex)
    }

    /// The other sides' stack the person remembers on a hex in the fog: its
    /// side and how many (a ghost of it, on the map).
    fn ghost(&self, hex: Hex) -> Option<(Side, usize)> {
        let seer = self.seer();
        (!self.sight.sees(hex)).then(|| self.screen.game.ghost_at(seer, hex).map(|g| (g.side, g.count))).flatten()
    }

    /// The selected unit shows on its hex, over the others of its stack (in a
    /// city, its pip is ringed).
    fn front_unit(&self) -> Option<UnitId> {
        self.screen.game.selected()
    }

    /// The person at the screen: their units show what they are; the
    /// others', on the map, are banners.
    fn viewer(&self) -> Option<Side> {
        Some(self.seer()).filter(|side| *side != Side::Grey)
    }

    fn sees(&self, hex: Hex) -> bool {
        self.sight.sees(hex)
    }
}

/// A word with a capital: "Red".
fn title(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}
