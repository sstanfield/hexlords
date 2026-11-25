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
use std::path::{Path, PathBuf};

use turbo_ferret::widgets::{ImageWidget, Slider};
use turbo_ferret::{ImageHandle, LayoutParams, Modifiers, Point, Rect, Size, TbId, Visibility, WidgetId};

use crate::agents::{self, Agents};
use crate::battle_window::{self, BattleWindow};
use crate::city_windows::{self, CityList, CityOrder, CityWindow};
use crate::game::{Game, SelectOutcome, Sight, Task};
use crate::gui::build::{self, WindowParts};
use crate::gui::menu::{ItemWords, Menu, MenuBar};
use crate::gui::{MapCommand, MapKey, RootCommand};
use crate::help::{HelpSection, command_key, fullscreen_keys};
use crate::info;
use crate::log_window::{self, LogWindow};
use crate::options_window::{self, Options, OptionsWindow};
use crate::orders::{Event, Order, RallyPoint};
use crate::orders_list::{self, OrdersList};
use crate::patrol_window::{self, PatrolClick, PatrolWindow};
use crate::platform::WEB;
use crate::production_window::{self, ProductionClick, ProductionWindow};
use crate::rally_window::{self, RallyClick, RallyWindow};
use crate::render::{
    AssetDirs, BATTLE_BORDER, BATTLE_FLASH, OWN_WAY_BORDER, SELECTED_BORDER, city_icon, coin_icon, unit_icon,
};
use crate::replay::{Replay, Step};
use crate::screen::{FULLSCREEN_ID, HELP_KEYS_ID, MapScreen, QUIT_ID, Scene, ZOOM_IN_ID, ZOOM_OUT_ID};
use crate::setup::Controller;
use crate::showing::{Going, Now, Showing, Shown};
use crate::state::scenario::side_name;
use crate::state::{
    CityId, DEFAULT_UNIT_IMAGE, Hex, MAX_QUEUE, MAX_STACK, Patrol, QueueEnd, QueueItem, Sends, Side, UnitId,
};
use crate::temple_window::{self, TempleWindow};
use crate::{about, guide};

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
/// Beside it, the side's cities of all the map's ("9/24", by a city's
/// picture).
pub const CITIES_HELD_ID: &str = "cities_held";
/// The button that ends the turn, and the line about the cursor.
pub const END_TURN_ID: &str = "end_turn";
/// The buttons for the lists of cities, and the one for the selected city.
pub const ALL_CITIES_ID: &str = "all_cities";
/// Take back the last order; go to the next stack that can move, the next
/// city.
pub const UNDO_ID: &str = "undo";
/// The sidebar's Replay button and the slider beside it (`replay::Replay`).
pub const REPLAY_ID: &str = "replay";
pub const REPLAY_SLIDER_ID: &str = "replay_slider";
pub const NEXT_STACK_ID: &str = "next_stack";
pub const NEXT_CITY_ID: &str = "next_city";
/// How long the computer takes over a turn at least, in ms, so that the log
/// can be followed while it plays.
pub const COMPUTER_TURN_MS: u64 = 400;
/// The button that opens the log (`log_window`).
pub const LOG_ID: &str = "log";
pub const GO_TO_ID: &str = "go_to";
pub const BLESS_ID: &str = "seek_blessing";
pub const DISBAND_ID: &str = "disband";
pub const DEFEND_ID: &str = "defend";
pub const DONE_ID: &str = "done";
pub const TASKS_ID: &str = "tasks";
/// The View menu's item for which orders the map outlines (rounds): all
/// the person's stacks', or the selected stack's only.
pub const ORDERS_SHOWN_ID: &str = "orders_shown";
pub const MY_CITIES_ID: &str = "my_cities";
/// The Cities menu's Production... (`production_window`).
pub const PRODUCTION_ID: &str = "production";
pub const CITY_DETAILS_ID: &str = "city_details";
/// The Game menu's Options (`options_window`).
pub const OPTIONS_ID: &str = "options";
/// The window that asks before a click gives new orders to a stack that
/// has orders of its own, and its buttons.
/// It asks too when a click on another of the person's stacks could join
/// it or take it instead (Take: that stack is selected).
pub const MISSION_ASK_ID: &str = "mission_ask";
pub const MISSION_GO_ID: &str = "mission_go";
pub const MISSION_TAKE_ID: &str = "mission_take";
pub const MISSION_KEEP_ID: &str = "mission_keep";
/// The window that says an order a click gave was refused, and why; its
/// button.
pub const REFUSED_ID: &str = "refused";
pub const REFUSED_OK_ID: &str = "refused_ok";
/// The Units menu's Rally points (`rally_window`).
pub const RALLY_POINTS_ID: &str = "rally_points";
/// The Units menu's Patrol (`patrol_window`) and Maraud.
pub const PATROL_ID: &str = "patrol";
pub const MARAUD_ID: &str = "maraud";
/// The window End turn opens first while something waits for orders
/// (Options), and its buttons.
pub const END_ASK_ID: &str = "end_turn_ask";
pub const END_ANYWAY_ID: &str = "end_turn_anyway";
pub const END_TASKS_ID: &str = "end_turn_tasks";
pub const END_KEEP_ID: &str = "end_turn_keep";
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

/// What a click on the map picks, when it does not choose (`Session::picking`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Where the selected units go.
    GoTo,
    /// Where the city sends its new units.
    Rally(CityId),
    /// Where the side's rally point goes (0 to 9).
    RallyPoint(u8),
    /// The waypoints of the selected units' patrol.
    Patrol,
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

/// A unit's icon, by its kind's picture, its side, its level (the ring) and
/// whether it defends (a badge).
type IconKey = (String, Side, u8, bool);

/// A tick of a round that played out: what happened in it, and what each
/// person who did not watch saw after it.
type RoundTick = (Vec<(Option<usize>, Event)>, Vec<(Side, Sight)>);

/// The game as it is, put by while the screen shows how it was
/// (`Session::go_back`): whose record is shown (a person's since their
/// last turn; `None`: the round that played out, everyone's), how many of
/// its steps have been done, and whether it is being played on.
struct Past {
    present: Game,
    whose: Option<Side>,
    at: usize,
    playing: bool,
}

/// A unit's place in the stack: a button with the unit's icon and its numbers.
struct Slot {
    button: WidgetId,
    icon: WidgetId,
    numbers: Line,
    /// The icon it shows now.
    shows: Option<IconKey>,
    /// Its unit is not on the hex yet (in rounds: on its way to join the
    /// stack), and the box is in the color of those (`COMING_SKIN`).
    coming: bool,
}

/// The look of a unit's box in the stack: of one on the hex, and of one on
/// its way there (the owner's word, 2026-11-22: "use some color coding to
/// make it clearer what is happening (there now vs will be there soon)").
const SLOT_SKIN: &str = "UnitSlot";
const COMING_SKIN: &str = "UnitComing";

impl Slot {
    /// The resource the slot with the id `id` is made from (`Session` makes
    /// them, so that their number is a constant, not a list in the layout).
    fn resource(id: &str) -> String {
        format!(
            "TBButton: id: {id}, skin: {SLOT_SKIN}, is-focusable: 0\n\
             \tTBLayout: axis: y, spacing: 2\n\
             \t\tTBImageWidget: id: {id}_icon\n\
             \t\tTBTextField: id: {id}_text, text-align: center\n"
        )
    }

    /// Make the slot `id` in `row`.
    fn make(screen: &mut MapScreen, row: WidgetId, id: &str) -> Result<Slot, String> {
        screen.ui.inflate(row, &Slot::resource(id)).map_err(|e| format!("a box of the stack: {e}"))?;
        let (button, icon) = (find(screen, id)?, find(screen, &format!("{id}_icon"))?);
        Ok(Slot { button, icon, numbers: Line::find(screen, &format!("{id}_text"))?, shows: None, coming: false })
    }
}

/// The game's keys and clicks, for the help window (F1).
fn help_section() -> Vec<HelpSection> {
    let end_turn = format!("{}+T,  or  End turn", command_key());
    let undo = format!("{}+Z,  or  Undo", command_key());
    let new_game = format!("{}+N", command_key());
    let options = format!("{}+,", command_key());
    vec![
        HelpSection::new(
            "The game",
            &[
                ("The menus along the top", "Everything there is to do; each item says its keys"),
                (
                    "Click a hex,  or  Enter",
                    "Choose it: select its stack, or send the selected units there (still selected while they can go on)",
                ),
                (
                    "Click one of your stacks",
                    "Select it; one the selected units can reach: asks, join it or take it (Enter: the first)",
                ),
                (
                    "Click for a stack with orders",
                    "Asks first, saying what it does (defends, marches...): Enter sends it, Esc keeps its orders",
                ),
                (
                    "An order refused",
                    "A window says why (OK, Enter, Esc or a click closes it); the stack stays selected",
                ),
                (
                    "Click a hex beyond their reach",
                    "Send them there, over as many turns as it takes (Go to); your stack there: select it",
                ),
                ("A hex with another side's units", "Sending units there attacks them: the line below says the odds"),
                ("Drag on the map", "Move it (a press that does not move is a click)"),
                ("Point at a unit below", "All about it"),
                (
                    "Click a unit below",
                    "Take it into the selection, or leave it out (the leader's box is the left one)",
                ),
                ("Esc", "Close the menu or the window on top; with none open, drop the selection"),
                (
                    &undo,
                    "Take back the last move, a city's work or its walls (not a battle); in rounds: the last plan or leg",
                ),
                (
                    &end_turn,
                    "End the turn; End turn (3): three stacks or cities still wait, and it asks first (Options)",
                ),
                (
                    "Start turn",
                    "The same button and key, when your turn comes with news in your log: close it and play",
                ),
                ("Begin red's turn", "In hotseat the map is covered between two people's turns: the next one's button"),
                ("In rounds", "A click is a plan (its way outlined): nothing moves until every side's orders are done"),
                (
                    "In rounds, a click after a plan",
                    "A waypoint: they go on from where it ends (a click on the stack: afresh)",
                ),
                ("In rounds, a click on the black", "Total fog: the stack sets off toward land not seen, to look"),
                (
                    "Orders done (in rounds)",
                    "End turn's button: the round then plays out; a stack attacks who it finds on its way",
                ),
                (&new_game, "A new game (once this one is over at once; before, it asks)"),
                ("Leaving a game", "It is kept as it goes: the new game screen plays it on (not in a browser)"),
                (&options, "Options: what the game does for you as a turn goes (selects, asks, tells)"),
            ],
        ),
        HelpSection::new(
            "Units",
            &[
                ("N,  or  Next", "Select the nearest stack waiting for orders (Shift+N: back); moved ones are done"),
                ("G,  or  Go to", "Then click a hex: the selected units go there, over as many turns as it takes"),
                (
                    "Shift+click,  or  Shift+Enter",
                    "After G (or a city's Send to): a stop on the way; at a temple, they seek its blessing",
                ),
                (
                    "D,  or  Done;  F,  B,  X",
                    "Aside this turn, on to the next; F: defend, or stand down; B: a temple's blessing; X: disband",
                ),
                ("R", "Rally points: place them; Go to (or a city's Send to) onto one's flag sends units there"),
                ("0 to 9", "Go to that rally point (the view and the cursor; its flag is on the minimap too)"),
                (
                    "P",
                    "Patrol: Shift+click waypoints, click the last (its own: a loop), say if it attacks (Enter: Patrol)",
                ),
                ("U", "Maraud: the computer leads the stack, hunts and takes cities; U again calls it back"),
            ],
        ),
        HelpSection::new(
            "Cities",
            &[
                ("C", "Go to your next city (Shift+C: the one before)"),
                ("Your city, in the sidebar", "Set what it builds (point at one: all about it), where its units go"),
                ("O,  or  Open the city's window", "Your selected city, in Production (another side's: the sidebar)"),
                (
                    "A unit's box, in Production",
                    "One more in the city's queue (the first starts one that repeats; its units stay)",
                ),
                ("Tab, in Production", "The cities, the queue, the units (Shift+Tab: back); the arrows move in them"),
                ("Enter,  -  +,  Delete", "In Production: add the unit; one fewer or more of the item; take it out"),
                ("0 to 9,  K,  F", "In Production: the item's units to that rally point; keep them; defend"),
                ("Raise the walls, in Production", "A level, paid at once: once a turn"),
                ("M,  A", "Your cities, all cities: see where one is, go to it, open it"),
                ("E", "Production, over the map: your cities, what the next 5 rounds bring, your stacks"),
                (
                    "A city's row, or Up and Down",
                    "In Production: choose it; it is in full beside, to set what it builds and where its units go",
                ),
                ("A,  I,  W,  B, in Production", "The cities: all, idle, waiting, building"),
                ("Stop all production...", "In Production: every city stops building (it asks first)"),
            ],
        ),
        HelpSection::new(
            "View",
            &[
                (
                    "T  (Shift+T: back)",
                    "The next stack or city that needs orders: the stack selected, the city in Production",
                ),
                (
                    "The list in the sidebar",
                    "Needs orders (gold), then your army (garrisons blue-grey, those done dim): a click goes to it",
                ),
                ("L", "Your log: your battles, who won and who died, the cities you took and lost"),
                ("Watch, in the log", "A battle you fought, played out again: both stacks, a hit at a time"),
                (
                    "Enter, in a battle's window",
                    "End the battle, then close it (Close and Esc close it too); one shown as it happens holds the game",
                ),
                ("H", "Your capital (in hotseat, each turn starts there)"),
                ("V", "In rounds: outline all your stacks' orders, or the selected stack's only"),
                (
                    "W,  or  Replay",
                    "What happened since your last turn, again (W: stop; its slider: back into it; Esc: now)",
                ),
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
    /// The selected hex's city, a line at a time: what shows of it (one's
    /// own in full in the production panel, O).
    city: Vec<Line>,
    /// The sidebar's list of what needs orders, and the army (`orders_list`),
    /// in a box that scrolls, as tall as the sidebar leaves it (the height
    /// it was given last).
    orders: OrdersList,
    orders_scroll: WidgetId,
    orders_h: i32,
    /// A press on the map closed a menu: the click it makes does nothing.
    swallow_click: bool,
    city_details: WidgetId,
    /// The menu bar, its menus, and the treasury at its end: the gold, what
    /// comes in a turn, what goes out; and the side's cities of all.
    /// (`treasury` is the three lines of gold, then the cities' line.)
    menu_bar: WidgetId,
    menu_room: WidgetId,
    menu_titles: WidgetId,
    treasury_box: WidgetId,
    menus: MenuBar,
    treasury: [Line; 4],
    sidebar: WidgetId,
    turn_row: WidgetId,
    undo: WidgetId,
    replay_row: WidgetId,
    replay_button: Line,
    replay_slider: WidgetId,
    go_to: WidgetId,
    done: WidgetId,
    /// What the next click on the map (or Enter) picks, instead of choosing
    /// a hex: where the selected units go, or where a city sends its new
    /// ones. Esc lets it go.
    picking: Option<Pick>,
    /// The stops picked so far on the way (Shift+click), in order.
    stops: Vec<Hex>,
    /// The selection the screen made by itself (the stack that waits for
    /// orders next), while it is that: a click on another of the person's
    /// stacks takes that one instead of sending this one onto it.
    auto_selection: Vec<UnitId>,
    /// The window that asks before a click gives new orders to a stack with
    /// orders of its own, or joins another (`ask_mission`): where the click
    /// sends them, and the units; and the click it lets through.
    mission_ask: Option<(WindowParts, Hex, Vec<UnitId>)>,
    mission_takes: bool,
    /// The windows open when it asked (`open_windows`): one opened after it
    /// ends the question.
    mission_windows: u32,
    overriding: bool,
    /// (A stop on the way refused, for the words of `refuse_pick`.)
    refused_stop: bool,
    /// The window that says an order was refused, and why, while it is open
    /// (the owner's word, 2026-11-21: a click that leads to a refused order
    /// says so in a window).
    refusal: Option<WindowParts>,
    /// The item of a city's queue whose units the pick sends, if it is one;
    /// and the city's window that made way for the pick, with its item.
    rally_item: Option<usize>,
    reopen: Option<(CityId, Option<usize>)>,
    /// Why the last pick was refused, said until the next one.
    refused: Option<String>,

    /// The production window, while it is open.
    production: Option<ProductionWindow>,
    /// A temple's window: whether the stack there seeks its blessing, and
    /// what came of it.
    temple: Option<TempleWindow>,
    /// The window that asks before the selected units are disbanded.
    disband: Option<crate::disband_window::DisbandWindow>,
    /// The log, while it is open.
    log: Option<LogWindow>,
    /// How long the game's history was when each person at this screen last
    /// ended a turn: what came after is new to them (the log marks it).
    seen: Vec<(Side, usize)>,
    /// Between a person's turns: from End turn (or, in hotseat, the next
    /// person's Begin) to Start turn, while the log shows what the others do.
    between: bool,
    /// What the person at the screen knows (fog of war), as of the map's
    /// revision, the round and the person it was worked out for; and what the
    /// minimap showed of it then: who holds each city it knows of, how much
    /// land it knows (its picture is drawn again when that changes).
    sight: Sight,
    sight_of: Option<(u64, u32, Side, Side)>,
    minimap_of: (Vec<Option<Side>>, Option<usize>),
    /// When the computer last played a turn, in ms (it plays one at a time,
    /// [`COMPUTER_TURN_MS`] apart, so that the log can be followed).
    last_play: Option<u64>,
    /// The hex of the stack Next stack went to last.
    cycled: Option<Hex>,
    /// The stacks Next stack has gone to since it last went round (it goes
    /// to the nearest it has not), and the way it came (Shift+N goes back).
    visited: Vec<Hex>,
    trail: Vec<Hex>,
    /// What the person at the screen has chosen, and its window while it is
    /// open.
    options: Options,
    options_window: Option<OptionsWindow>,
    /// What End turn asks first, while it is open.
    end_ask: Option<WindowParts>,
    /// The rally points' window, while it is open; and the flags of the
    /// person's rally points on the map.
    rally_window: Option<RallyWindow>,
    flags: Vec<(Hex, u8)>,
    /// How many stacks and cities wait for orders (tier 1 tasks), as of the
    /// map's revision and the side it was counted for.
    waiting: (usize, usize),
    waiting_of: Option<(u64, Side)>,
    /// A line said under the stack in place of the selected unit's, until
    /// the next click or key: how a battle went (Options: told in a line),
    /// what a temple gave.
    notice: Option<String>,
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
    /// All that was done to the game from its start (`Game::record`): what
    /// a saved game is made of (`save`). The file the game is kept in, if
    /// it is; and the turn it was last written in.
    journal: Vec<Step>,
    save_to: Option<PathBuf>,
    saved_in: Option<(u32, Side)>,
    /// The last person here is out, and their log said so.
    told_out: bool,
    /// The window that asks whether to leave the game for a new one, while
    /// it is open.
    ask_new: Option<WindowParts>,
    /// A battle being watched, while its window is open.
    battle: Option<BattleWindow>,
    /// The window that asks how a patrol fights, while it is open; and what
    /// was chosen there last.
    patrol_window: Option<PatrolWindow>,
    patrol_odds: Option<u32>,
    /// The turn (round and side) whose marauders the computer has led.
    marauded: Option<(u32, Side)>,
    /// What the screen shows happening (Options: moves shown), the time of
    /// the last frame, and what the person is shown to know meanwhile (how
    /// many hexes their stacks had been to when it was worked out).
    showing: Showing,
    now: u64,
    shown_sight: Option<(Sight, usize)>,
    /// How many orders the computer has given in the turn it is playing an
    /// order at a time; and the turn it ended, still being shown, before
    /// the next is handed over (`turn_passed`).
    given: usize,
    pass_due: bool,
    /// The battles told by the order that fought them (`tell_battle`), not
    /// to be told again.
    told: Vec<usize>,
    /// In rounds: the ways the person's stacks take as the round plays out,
    /// as things stand (outlined on the map), and what they were worked out
    /// for (the map's revision, whose they are).
    /// (Apart: the way of the selected stack, ringed in a color of its
    /// own; the others' are not outlined at all if the person said so.)
    plans: Vec<Hex>,
    own_way: Vec<Hex>,
    plans_of: Option<(u64, Side, Vec<UnitId>, bool)>,
    /// Where a battle being shown is fought, since when, and until when
    /// (`None`: while its window is open): the map rings the hex, and it
    /// flashes.
    battle_mark: Option<(Hex, u64, Option<u64>)>,
    /// In rounds: what happened as the last round played out, for the
    /// people who did not watch it (hotseat: it is shown to each as their
    /// orders begin), and who has seen it.
    round_shown: Vec<RoundTick>,
    round_seen: Vec<Side>,
    round_live: bool,
    /// What each person in the game saw as that round began; and, while a
    /// tick of it is shown again, what the person saw after it.
    round_before: Vec<(Side, Sight)>,
    replay_after: Option<Sight>,
    /// What is to be shown now is a tick of a round: its stacks go together.
    together: bool,
    /// In rounds: where the person was looking when their orders were done
    /// (the map goes back there once the round has played out).
    looked: Option<[f32; 2]>,
    /// What happened since each person here last gave orders, to be looked
    /// at again (`replay`): theirs (in turns: from the end of their turn
    /// to the start of their next), or everyone's (`None`; in rounds: the
    /// round that played out). And those still being kept.
    replays: Vec<(Option<Side>, Replay)>,
    recording: Vec<(Option<Side>, Replay)>,
    /// While the screen shows how the game was: the game as it is.
    past: Option<Box<Past>>,
    /// Where the replay's slider was put last (anywhere else: the person
    /// moved it).
    slider_at: usize,
}

/// How long the hex a battle is fought on takes to flash on and off, in ms.
const BATTLE_FLASH_MS: u64 = 700;

/// Where a window of `size` can go on a map screen, the best first: at the
/// top of the map in its middle, at its bottom, in the map's corners (the
/// left ones first), and, last, at the screen's right edge over the sidebar
/// (top and bottom). Never above the map's top, where the menus are; a
/// window taller than the map reaches below it. `edge`: how far in from an
/// edge it stays.
fn window_places(map: Rect, screen: Rect, (w, h): (i32, i32), edge: i32) -> [Rect; 8] {
    let (top, left) = (map.y + edge, map.x + edge);
    let bottom = (map.y + map.h - h - edge).max(top);
    let (middle, right) = ((map.x + (map.w - w) / 2).max(left), (map.x + map.w - w - edge).max(left));
    let far = (screen.x + screen.w - w - edge).max(left);
    [
        (middle, top),
        (middle, bottom),
        (left, top),
        (right, top),
        (left, bottom),
        (right, bottom),
        (far, top),
        (far, bottom),
    ]
    .map(|(x, y)| Rect::new(x, y, w, h))
}

/// How far a pixel is from a rectangle, along the axis it is farther along:
/// negative inside it, by how deep.
fn apart(rect: Rect, p: Point) -> i32 {
    let x = (rect.x - p.x).max(p.x - (rect.x + rect.w));
    let y = (rect.y - p.y).max(p.y - (rect.y + rect.h));
    x.max(y)
}
/// How many orders the computer gives in a frame, at most, looking for one
/// to show.
const STEPS_PER_FRAME: usize = 25;

impl Session {
    /// Set up the UI for a window of `viewport` pixels at `dpi` with the skin and
    /// the layout from the asset directory, playing `game`, with the options
    /// the game starts with (`Options::default`).
    pub fn with_game(assets: &Path, viewport: Size, dpi: i32, game: Game) -> Result<Session, String> {
        Session::with_options(assets, viewport, dpi, game, Options::default())
    }

    /// The same, with the person's `options`.
    pub fn with_options(
        assets: &Path,
        viewport: Size,
        dpi: i32,
        game: Game,
        options: Options,
    ) -> Result<Session, String> {
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
        let city = city_icon(coin_px);
        let city_image = screen.ui.image_from_pixels("city_icon", city.size, &city.rgba);
        let city_widget = find(&screen, "city_icon")?;
        screen.ui.set_layout_params(city_widget, Some(LayoutParams::fixed(coin_px, coin_px)));
        screen.ui.with_widget::<ImageWidget, _>(city_widget, |w, cx| w.set_image(cx, city_image));
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
            orders: OrdersList::new(find(&screen, "orders_heading")?, find(&screen, "orders_list")?),
            orders_scroll: find(&screen, "orders_scroll")?,
            orders_h: -1,
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
                Line::find(&screen, CITIES_HELD_ID)?,
            ],
            sidebar: find(&screen, "main_sidebar")?,
            turn_row: find(&screen, "turn_row")?,
            undo: find(&screen, UNDO_ID)?,
            replay_row: find(&screen, "replay_row")?,
            replay_button: Line::find(&screen, REPLAY_ID)?,
            replay_slider: find(&screen, REPLAY_SLIDER_ID)?,
            go_to: find(&screen, GO_TO_ID)?,
            done: find(&screen, DONE_ID)?,
            picking: None,
            stops: Vec::new(),
            rally_item: None,
            auto_selection: Vec::new(),
            mission_windows: 0,
            mission_ask: None,
            mission_takes: false,
            overriding: false,
            refused_stop: false,
            refusal: None,
            reopen: None,
            refused: None,

            production: None,
            temple: None,
            disband: None,
            log: None,
            seen: Vec::new(),
            between: false,
            last_play: None,
            sight: Sight::all(),
            sight_of: None,
            minimap_of: (Vec::new(), None),
            cycled: None,
            visited: Vec::new(),
            trail: Vec::new(),
            options,
            options_window: None,
            end_ask: None,
            rally_window: None,
            flags: Vec::new(),
            waiting: (0, 0),
            waiting_of: None,
            notice: None,
            city_window: None,
            city_list: None,
            end_turn: Line::find(&screen, END_TURN_ID)?,
            agents,
            cover: None,
            last_person: None,
            game_over: None,
            wants_new_game: false,
            journal: Vec::new(),
            save_to: None,
            saved_in: None,
            told_out: false,
            ask_new: None,
            battle: None,
            patrol_window: None,
            patrol_odds: None,
            marauded: None,
            showing: Showing::default(),
            now: 0,
            shown_sight: None,
            given: 0,
            pass_due: false,
            told: Vec::new(),
            plans: Vec::new(),
            own_way: Vec::new(),
            plans_of: None,
            battle_mark: None,
            round_shown: Vec::new(),
            round_seen: Vec::new(),
            round_live: false,
            round_before: Vec::new(),
            replay_after: None,
            together: false,
            looked: None,
            replays: Vec::new(),
            recording: Vec::new(),
            past: None,
            slider_at: 0,
            screen,
        };
        // What happens is kept for the screen to show.
        session.screen.game.watch(true);
        session.screen.game.record(true);
        session.apply_layout_sizes();
        // The computer's turns that come before the first person's are
        // played by the frames; or the first person is handed the game.
        session.between = !session.persons_turn();
        session.turn_passed();
        session.refresh();
        session.screen.settle();
        Ok(session)
    }

    /// Keep the game in this file from now on (`save`): it is written when
    /// a turn has passed, when the window is closed and when the game is
    /// left for a new one; and taken away once the game is over.
    pub fn keep_in(&mut self, path: PathBuf) {
        self.save_to = Some(path);
        self.saved_in = None;
        self.keep();
    }

    /// The game is one that was saved (`save::load`): `steps` is all that
    /// was done to it from its start, `seen` how much of its history each
    /// person here had seen.
    pub fn restore(&mut self, steps: Vec<Step>, seen: Vec<(Side, usize)>) {
        (self.journal, self.seen) = (steps, seen);
    }

    /// The file the game is kept in, if it is.
    pub fn kept_in(&self) -> Option<&Path> {
        self.save_to.as_deref()
    }

    /// The game as it is (not as it was, while that is looked at).
    fn present(&self) -> &Game {
        self.past.as_ref().map_or(&self.screen.game, |past| &past.present)
    }

    /// Write the game to its file now, if it has one (`keep_in`): as it
    /// is, whatever the screen shows of how it was. Once it is over, the
    /// file is taken away instead: a game that is over is not kept.
    pub fn save_now(&mut self) {
        let Some(path) = self.save_to.clone() else { return };
        let game = self.present();
        if game.winner().is_some() {
            if crate::files::is_file(&path) {
                if let Err(why) = crate::save::delete(&path) {
                    crate::platform::complain(&format!("hexlords: {why}"));
                }
            }
            self.save_to = None;
            return;
        }
        // (With what was done since the screen last took it.)
        let steps: Vec<Step> = self.journal.iter().chain(game.journal.iter().flatten()).cloned().collect();
        let stamp = (crate::platform::clock_nanos() / 1_000_000_000) as u64;
        let Some(text) = crate::save::text(game, &steps, &self.seen, stamp) else {
            // A map made by hand has no texts to keep.
            self.save_to = None;
            return;
        };
        self.saved_in = Some((game.turn(), game.player()));
        if let Err(why) = crate::save::write(&path, &text) {
            crate::platform::complain(&format!("hexlords: the game was not saved: {why}"));
        }
    }

    /// Write the game to its file if a turn has passed since it last was
    /// (or the game is over).
    fn keep(&mut self) {
        if self.save_to.is_none() {
            return;
        }
        let game = self.present();
        if game.winner().is_some() || self.saved_in != Some((game.turn(), game.player())) {
            self.save_now();
        }
    }

    /// True if it is the turn of a person at this screen: in the game, and
    /// handed over to them (not while the turn before is still being shown,
    /// `pass_due`: the screen is still the last person's).
    fn persons_turn(&self) -> bool {
        let game = &self.screen.game;
        // (The past is nobody's turn: it is looked at.)
        game.controller(game.player()) == Some(&Controller::Person) && !self.pass_due && self.past.is_none()
    }

    /// True if the person at the screen can give orders now: their turn
    /// (handed over), no cover over it, and the game not over.
    fn can_act(&self) -> bool {
        self.persons_turn() && !self.covered() && self.screen.game.winner().is_none() && !self.pass_due
    }

    /// The person whose turn it is has done (End turn): the next side's
    /// turn (the frames play the computer's, a turn at a time). When it is a
    /// person's turn again, their log opens if anything happened to them,
    /// and Start turn begins it; else it begins at once.
    pub fn end_turn(&mut self) {
        // Only a person's turn is the screen's to end (once nobody here is in
        // the game, the computer plays on by itself).
        if self.cover.is_some() || self.screen.game.winner().is_some() || !self.persons_turn() {
            return;
        }
        (self.picking, self.refused) = (None, None);
        let side = self.screen.game.player();
        // In turns: what happens from here to their next turn is kept, to
        // be looked at again then. (In rounds: the round, as it plays out.)
        self.take_orders();
        self.recording.retain(|(whose, _)| *whose != Some(side));
        if !self.screen.game.rounds() {
            self.recording.push((Some(side), Replay::begin(&self.screen.game)));
        }
        let length = self.screen.game.history().len();
        match self.seen.iter_mut().find(|(s, _)| *s == side) {
            Some((_, seen)) => *seen = length,
            None => self.seen.push((side, length)),
        }
        // At the end of the turn the computer leads the person's marauders
        // (and the rules march the marches and the patrols): what comes of
        // it is news for their next turn.
        self.lead_marauders();
        self.looked = self.screen.game.rounds().then(|| self.screen.view.middle());
        self.screen.game.end_turn();
        // Nobody else plays before this person's next turn (a side alone):
        // nothing to wait for.
        self.between = !(self.persons_turn() && self.screen.game.player() == side);
        self.turn_passed();
    }

    /// Start the turn of the person whose turn it is (Start turn): the log
    /// closes, and the map is theirs.
    pub fn start_turn(&mut self) {
        self.start_turn_with(true);
    }

    /// Start the turn; `select`: the first stack that waits is selected (as
    /// the person said, in Options), unless an order of theirs starts it.
    fn start_turn_with(&mut self, select: bool) {
        if !self.between || !self.persons_turn() || self.cover.is_some() {
            return;
        }
        self.between = false;
        if let Some(log) = self.log.take() {
            log.close(&mut self.screen.ui);
        }
        if select {
            self.begin_orders();
        } else {
            (self.cycled, self.visited, self.trail) = (None, Vec::new(), Vec::new());
        }
        // (In rounds and hotseat: the round they did not watch, now that
        // the log is out of the way.)
        self.show_last_round();
    }

    /// The person's turn begins at the screen (no cover over it, no log in
    /// the way): Next stack goes round afresh, from the first stack that
    /// waits for orders, which is selected (if the person said so: Options).
    fn begin_orders(&mut self) {
        (self.cycled, self.visited, self.trail) = (None, Vec::new(), Vec::new());
        let busy = self.between || self.screen.game.selected().is_some();
        if !self.options.next_after_move || !self.can_act() || busy {
            return;
        }
        if !self.screen.game.waiting_stacks(self.screen.game.player()).is_empty() {
            self.next_stack(false);
            self.auto_selection = self.screen.game.selection().to_vec();
        }
    }

    /// The button under the minimap, or Ctrl+T: Start turn between turns,
    /// else End turn; but first, if the person said so (Options), a window
    /// asks while stacks or cities wait for orders or the gold will not pay
    /// the upkeep.
    fn turn_button(&mut self) {
        // (In the past the button goes back to now.)
        if self.past.is_some() {
            self.back_to_now();
            return;
        }
        if self.between {
            self.start_turn();
            return;
        }
        if self.options.ask_end_turn && self.can_act() && !self.asks_end_turn() {
            let lines = self.end_turn_worries();
            if !lines.is_empty() {
                self.ask_end_turn(&lines);
                return;
            }
        }
        self.close_end_ask();
        self.end_turn();
    }

    /// How many of the player's stacks and cities wait for orders (the
    /// tasks that need them), counted again only when the map changed.
    fn waiting_now(&mut self) -> (usize, usize) {
        let game = &self.screen.game;
        let key = (game.map.revision(), game.player());
        if self.waiting_of != Some(key) {
            let tasks = game.tasks(key.1);
            let needs = tasks.iter().filter(|t| t.tier() == 1);
            let stacks = needs.clone().filter(|t| matches!(t, Task::Stack { .. })).count();
            self.waiting = (stacks, needs.count() - stacks);
            self.waiting_of = Some(key);
        }
        if self.persons_turn() { self.waiting } else { (0, 0) }
    }

    /// What End turn would leave undone, in lines: the stacks and cities
    /// that wait for orders, and upkeep the gold will not pay (units leave).
    fn end_turn_worries(&mut self) -> Vec<String> {
        let (stacks, cities) = self.waiting_now();
        let mut lines = Vec::new();
        match stacks {
            0 => {}
            1 => lines.push("A stack waits for orders.".to_string()),
            n => lines.push(format!("{n} stacks wait for orders.")),
        }
        match cities {
            0 => {}
            1 => lines.push("A city builds nothing.".to_string()),
            n => lines.push(format!("{n} cities build nothing.")),
        }
        let game = &self.screen.game;
        let side = game.player();
        let (have, owed) = (game.gold(side).saturating_add(game.income(side)), game.upkeep(side));
        if owed > have {
            lines.push(format!(
                "Next turn your gold and taxes ({have}) will not pay the upkeep ({owed}): units will leave you."
            ));
        }
        lines
    }

    /// The window that asks before the turn ends, saying what waits.
    fn ask_end_turn(&mut self, lines: &[String]) {
        let (root, area) = (self.screen.root_window(), self.window_area());
        let rounds = self.screen.game.rounds();
        let ui = &mut self.screen.ui;
        let parts = build::window(ui, root, END_ASK_ID, if rounds { "Orders done" } else { "End turn" });
        build::heading(ui, parts.content, if rounds { "Are your orders done?" } else { "End your turn?" });
        for line in lines {
            build::text(ui, parts.content, line);
        }
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        let buttons = build::row(ui, parts.content, space);
        build::primary_button(ui, buttons, END_ANYWAY_ID, if rounds { "They are done" } else { "End the turn" });
        build::button(ui, buttons, END_TASKS_ID, "Show the tasks");
        build::button(ui, buttons, END_KEEP_ID, "Keep playing");
        build::text(ui, parts.content, "(Options: whether End turn asks.)");
        build::place(ui, parts, area, (0.5, 0.4));
        self.end_ask = Some(parts);
        self.screen.settle();
    }

    /// Close the window End turn opened, if it is open.
    fn close_end_ask(&mut self) {
        if let Some(ask) = self.end_ask.take() {
            build::close(&mut self.screen.ui, ask.window);
        }
    }

    /// True while End turn asks first.
    pub fn asks_end_turn(&self) -> bool {
        self.end_ask.as_ref().is_some_and(|ask| build::is_open(&self.screen.ui, ask.window))
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
        self.given = 0;
        self.turn_passed();
    }

    /// The computer plays the turn of the side whose turn it is an order at
    /// a time (Options: moves shown), until one of them is to be shown, or
    /// the turn is over (handed over once what it did has been shown).
    fn play_steps(&mut self, now_ms: u64) {
        let side = self.screen.game.player();
        for _ in 0..STEPS_PER_FRAME {
            let game = &self.screen.game;
            if game.winner().is_some() || !matches!(game.controller(side), Some(Controller::Computer(_))) {
                return;
            }
            let Some(agent) = self.agents.get_mut(side) else { return };
            let (_, over) = agents::play_step(&mut self.screen.game, agent, self.given);
            self.given += 1;
            // (What the person sees is worked out once a frame: the
            // computer's moves do not open it up.)
            let happened = self.screen.game.take_happened();
            self.show_happened(happened, None);
            if over {
                self.given = 0;
                self.last_play = Some(now_ms);
                if self.showing.is_idle() {
                    self.turn_passed();
                } else {
                    self.pass_due = true;
                }
                return;
            }
            if !self.showing.is_idle() {
                return;
            }
        }
    }

    /// In rounds, the round that plays out goes on (`Game::play_out`): its
    /// ticks, until one of them has something to show (the stacks the
    /// person sees going, their battles: the next tick waits for them), or
    /// the round is over: the next one's orders are handed over once what
    /// it did has been shown. What happened is kept, to be shown to the
    /// people who did not watch (hotseat).
    fn play_round(&mut self, now_ms: u64) {
        if !std::mem::replace(&mut self.round_live, true) {
            self.round_shown.clear();
            self.round_seen = self.person_here().into_iter().collect();
            self.round_before = self.others_sights();
            // The round is kept, a beat at a time, to be looked at again.
            self.take_orders();
            self.recording.retain(|(whose, _)| whose.is_some());
            self.recording.push((None, Replay::begin(&self.screen.game)));
        }
        while self.screen.game.playing_out() {
            self.screen.game.play_out();
            if let Some((_, replay)) = self.recording.iter_mut().find(|(whose, _)| whose.is_none()) {
                replay.push(Step::Beat);
            }
            if !self.screen.game.playing_out() {
                self.keep_replay(None);
            }
            let happened = self.screen.game.take_happened();
            let sights = self.others_sights();
            self.round_shown.push((happened.clone(), sights));
            self.together = true;
            self.see_what(happened);
            self.together = false;
            if !self.screen.game.playing_out() {
                self.round_live = false;
                self.last_play = Some(now_ms);
                if self.showing.is_idle() {
                    self.turn_passed();
                } else {
                    self.pass_due = true;
                }
                return;
            }
            if !self.showing.is_idle() {
                return;
            }
        }
    }

    /// In rounds and hotseat: the person whose orders begin is shown what
    /// happened as the last round played out, if they did not watch it
    /// (the stacks they see that are still there, going; their battles).
    fn show_last_round(&mut self) {
        let side = self.screen.game.player();
        if !self.persons_turn() || self.round_seen.contains(&side) || self.round_shown.is_empty() {
            return;
        }
        self.round_seen.push(side);
        self.see();
        self.showing.finish_moves();
        // Each tick as they saw it then: a stack of another side's shows
        // only where it was in their sight before the tick or after it
        // (not where they came to see later in the round).
        let theirs = |sights: &[(Side, Sight)]| sights.iter().find(|(s, _)| *s == side).map(|(_, sight)| sight.clone());
        let mut before = theirs(&self.round_before);
        self.together = true;
        for (tick, sights) in self.round_shown.clone() {
            let after = theirs(&sights);
            self.replay_after = after.clone();
            self.show_happened(tick, before.clone());
            before = after;
        }
        (self.together, self.replay_after) = (false, None);
    }

    /// What each person in the game who is not at the screen sees now
    /// (rounds and hotseat: for the round shown to them later).
    fn others_sights(&self) -> Vec<(Side, Sight)> {
        let game = &self.screen.game;
        let people =
            game.setup().seats.iter().filter(|seat| seat.controller == Controller::Person).map(|seat| seat.side);
        let here = self.person_here();
        let others = people.filter(|side| Some(*side) != here && !game.out().contains(side));
        others.map(|side| (side, game.sight(side))).collect()
    }

    /// Play the computer's turns now, up to a person's (a turn at most, once
    /// nobody here is in the game): what the frames do a turn at a time. For
    /// tests and scripts.
    pub fn wait_for_turn(&mut self) {
        // (Nothing is shown: what was being shown is where it went. Nor is
        // a round played so kept to be looked at again.)
        self.back_to_now();
        self.stop_showing();
        self.recording.retain(|(whose, _)| whose.is_some());
        for _ in 0..self.screen.game.order().len() + 1 {
            // (In rounds: the round plays out, all of it.)
            if self.screen.game.playing_out() {
                while self.screen.game.playing_out() {
                    self.screen.game.play_out();
                }
                self.round_live = false;
                self.turn_passed();
            }
            let before = (self.screen.game.player(), self.screen.game.turn());
            if self.cover.is_some() || self.persons_turn() {
                break;
            }
            self.play_one();
            if self.screen.game.playing_out() {
                continue;
            }
            if (self.screen.game.player(), self.screen.game.turn()) == before || !self.people_in_play() {
                break;
            }
        }
        self.screen.game.take_happened();
        self.refresh();
    }

    /// Show nothing more: what was being shown is where it went, and the
    /// turn that was being shown is handed over.
    fn stop_showing(&mut self) {
        self.see();
        self.showing.clear();
        if self.shown_sight.take().is_some() {
            self.screen.redraw_minimap();
        }
        if std::mem::take(&mut self.pass_due) {
            self.turn_passed();
        }
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
        // Nothing picked, nothing said, carries over to another turn; nor a
        // question about units (they may be gone, or another side's turn).
        (self.picking, self.refused, self.reopen, self.notice) = (None, None, None, None);
        self.close_mission_ask();
        self.close_refusal();
        if let Some(window) = self.disband.take() {
            window.close(&mut self.screen.ui);
        }
        if let Some(window) = self.patrol_window.take() {
            window.close(&mut self.screen.ui);
        }
        if self.temple.as_ref().is_some_and(|t| t.asks()) {
            if let Some(window) = self.temple.take() {
                window.close(&mut self.screen.ui);
            }
        }
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
        // In rounds, the same person's next orders: the map is where they
        // left it (the round that played out took it to what happened).
        if person {
            if let Some([x, y]) = self.looked.take().filter(|_| self.last_person == Some(side)) {
                self.screen.view.set_middle(x, y);
            }
        }
        if person {
            self.last_person = Some(side);
            // What happened since their last turn is theirs to look at
            // again (turns).
            self.keep_replay(Some(side));
        }
        // The panels are about the hex the cursor is on.
        self.shown = self.screen.game.map.cursor;
        // The last person here is out: their log tells them (the computer
        // plays on).
        let out = self.last_person.is_some_and(|p| self.screen.game.out().contains(&p));
        if out && !self.people_in_play() && !self.told_out {
            self.told_out = true;
            self.open_log();
        }
        // A person's turn has come (with no cover over it): their log, if
        // anything happened to them; else it begins.
        if person && self.screen.game.winner().is_none() && self.cover.is_none() {
            if self.between && self.has_news(side) {
                self.open_log();
            } else {
                self.between = false;
                self.begin_orders();
            }
        }
    }

    /// Cover the map until `side`'s person says they are there.
    fn open_cover(&mut self, side: Side) {
        // Nothing of the last person's stays open for the next to see, nor
        // goes on being shown (the review, 2026-11-22: a round shown again
        // to one person went on under the next one's cover, its battles in
        // their window over it, its stacks going before the next's eyes).
        self.showing.clear();
        self.shown_sight = None;
        self.screen.redraw_minimap();
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
        build::primary_button(ui, buttons, NEW_GAME_ID, "New game");
        build::button(ui, buttons, GAME_OVER_CLOSE_ID, "Look at the map");
        build::place(ui, parts, area, (0.5, 0.4));
        self.game_over = Some(parts);
    }

    /// New game (the Game menu, Ctrl+N): once the game is over, straight to
    /// the new game screen; while it goes on, a window asks first (and says
    /// whether the game is kept, `keep_in`, or would be lost).
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
        let kept = if self.save_to.is_some() {
            "It is not over: it is kept, to be played on from the new game screen."
        } else {
            "It is not over, and it is not saved: it will be lost."
        };
        build::text(ui, parts.content, kept);
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
        self.screen.set_clear_top(bar_h);
        let ui = &mut self.screen.ui;
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
        ui.set_layout_params(self.replay_row, Some(narrow));
        // (The replay's slider has what its button leaves of the row.)
        let button_w = ui.preferred_size_free(self.replay_button.widget).pref.w;
        let slider_h = ui.preferred_size_free(self.replay_slider).pref.h;
        let slider_w = (side_w - button_w - 3 * spacing).max(4 * spacing);
        ui.set_layout_params(self.replay_slider, Some(LayoutParams::fixed(slider_w, slider_h)));

        // The panel is as tall as its lines and its slots (an empty slot keeps
        // its room), and the map has the rest.
        let panel_h = ui.preferred_size_free(self.panel).pref.h;
        let map_h = (h - spacing - panel_h).max(0);
        let minimap_h = h - (h as f32 * (1.0 - MINIMAP_HEIGHT_SHARE)).floor() as i32;
        // (The list under the city has what the rest leaves: it is sized
        // as it fills, `refresh_orders`.)
        self.orders_h = -1;
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
    /// In rounds, while the person gives their orders: after them, the
    /// units sent to join them there (`Game::joining`), to be sent on with
    /// them.
    fn stack_units(&self) -> Vec<UnitId> {
        let game = &self.screen.game;
        let map = &game.map;
        let viewer = self.seer();
        let mine = |id: &UnitId| map.units().get(*id).is_some_and(|u| viewer == Side::Grey || u.side() == viewer);
        let mut units: Vec<UnitId> = map.units_at(self.shown).iter().copied().filter(mine).collect();
        if !units.is_empty() {
            units.extend(self.joining_here());
        }
        units
    }

    /// In rounds, while the person gives their orders: the units of theirs
    /// sent to the hex the panels are about, on their way there.
    fn joining_here(&self) -> Vec<UnitId> {
        let game = &self.screen.game;
        if !game.rounds() || game.playing_out() || !self.can_act() {
            return Vec::new();
        }
        game.joining(game.player(), self.shown)
    }

    /// Work out what the person at the screen knows, if the map, the round
    /// or the person changed; and draw the minimap again if what it shows of
    /// it changed (the cities' holders, the land seen).
    fn see(&mut self) {
        self.take_orders();
        self.keep();
        let happened = self.screen.game.take_happened();
        self.see_what(happened);
    }

    /// The orders carried out since this was last done (`Game::record`) go
    /// into what is being kept of each person's time between two turns.
    fn take_orders(&mut self) {
        let steps = self.screen.game.take_steps();
        if self.past.is_some() {
            return;
        }
        for (_, replay) in self.recording.iter_mut().filter(|(whose, _)| whose.is_some()) {
            // (A round's beats are kept as it plays, `play_round`.)
            for step in steps.iter().filter(|step| **step != Step::Beat) {
                replay.push(step.clone());
            }
        }
        // All of it, from the game's start: what a saved game is made of.
        self.journal.extend(steps);
    }

    /// What was being kept for `whose` is whole: it is what they can look
    /// at again, in place of what they could before.
    fn keep_replay(&mut self, whose: Option<Side>) {
        self.take_orders();
        let Some(i) = self.recording.iter().position(|(of, _)| *of == whose) else { return };
        let kept = self.recording.remove(i);
        self.replays.retain(|(of, _)| *of != whose);
        if !kept.1.is_empty() {
            self.replays.push(kept);
        }
    }

    /// What the person at the screen can look at again: what happened
    /// since their last turn (in rounds: the round that played out); or,
    /// in the past, what is being shown.
    fn replay(&self) -> Option<&(Option<Side>, Replay)> {
        let whose = match &self.past {
            Some(past) => past.whose,
            None if self.screen.game.rounds() => None,
            None => Some(self.viewer()),
        };
        self.replays.iter().find(|(of, _)| *of == whose)
    }

    /// True if the person at the screen has something to look at again, and
    /// may: in the past already, or on their turn with nothing in the way.
    fn can_replay(&self) -> bool {
        self.replay().is_some() && (self.past.is_some() || (self.can_act() && !self.between))
    }

    /// True while the screen shows how the game was, not how it is.
    pub fn in_the_past(&self) -> bool {
        self.past.is_some()
    }

    /// How many steps of what can be looked at again have been done where
    /// the screen is (all of them: now), and how many there are.
    pub fn replay_at(&self) -> Option<(usize, usize)> {
        let (_, replay) = self.replay()?;
        Some((self.past.as_ref().map_or(replay.len(), |past| past.at), replay.len()))
    }

    /// The screen shows another state of the game than it did (the past, or
    /// the present again): what it worked out for the old one goes.
    fn another_state(&mut self) {
        self.showing.clear();
        if let Some(battle) = self.battle.take() {
            battle.close(&mut self.screen.ui);
        }
        (self.sight_of, self.shown_sight, self.waiting_of, self.plans_of, self.battle_mark) =
            (None, None, None, None, None);
        (self.picking, self.refused, self.notice) = (None, None, None);
        self.plans.clear();
        self.own_way.clear();
        self.close_mission_ask();
        self.screen.game.deselect();
        self.screen.redraw_minimap();
        self.see();
        self.shown = self.screen.game.map.cursor;
        self.refresh();
    }

    /// Go back: the screen shows the game as it was after `at` steps of
    /// what the person can look at again (the slider; all of them: now).
    /// The game as it is is put by; in the past everything can be looked
    /// at, and nothing ordered.
    pub fn go_back(&mut self, at: usize) {
        if !self.can_replay() {
            return;
        }
        let Some((whose, replay)) = self.replay() else { return };
        if at >= replay.len() {
            self.back_to_now();
            return;
        }
        let (whose, mut state) = (*whose, replay.state(at));
        state.map.cursor = self.screen.game.map.cursor;
        match &mut self.past {
            Some(past) => {
                (past.at, past.playing) = (at, false);
                self.screen.game = state;
            }
            None => {
                let present = std::mem::replace(&mut self.screen.game, state);
                self.past = Some(Box::new(Past { present, whose, at, playing: false }));
            }
        }
        self.another_state();
    }

    /// Back to the game as it is.
    pub fn back_to_now(&mut self) {
        let Some(past) = self.past.take() else { return };
        let cursor = self.screen.game.map.cursor;
        self.screen.game = past.present;
        self.screen.game.set_cursor(cursor);
        self.another_state();
    }

    /// Replay (the button, W): what the person can look at again is played
    /// from where the screen is in it (from its start, from now), every
    /// move and battle shown as it was; pressed while it plays, it stops
    /// there.
    pub fn play_replay(&mut self) {
        if !self.can_replay() {
            return;
        }
        match &mut self.past {
            Some(past) if past.playing => past.playing = false,
            Some(past) => past.playing = true,
            None => {
                self.go_back(0);
                if let Some(past) = &mut self.past {
                    past.playing = true;
                }
            }
        }
        self.refresh();
    }

    /// The replay being played goes on a step: what happened then is shown
    /// (the next step waits for it); after the last, back to now.
    fn replay_step(&mut self) {
        let Some(past) = &self.past else { return };
        let (whose, at) = (past.whose, past.at);
        let Some((_, replay)) = self.replays.iter().find(|(of, _)| *of == whose) else { return };
        if at >= replay.len() {
            self.back_to_now();
            return;
        }
        let beat = replay.step(at) == Some(&Step::Beat);
        let happened = replay.advance(&mut self.screen.game, at);
        // (An undo done again selects the units it gave back: nothing is
        // selected in the past, least of all a stack the person looking
        // does not see.)
        self.screen.game.deselect();
        if let Some(past) = &mut self.past {
            past.at = at + 1;
        }
        self.together = beat;
        self.see_what(happened);
        self.together = false;
    }

    /// What the line under the stack says in the past: where in what
    /// happened the screen is, and how to get back.
    fn past_line(&self) -> String {
        let (at, steps) = self.replay_at().unwrap_or((0, 0));
        let round = self.past.as_ref().is_some_and(|past| past.whose.is_none());
        let what = if round { "The round as it went" } else { "The others' turns as they went" };
        let playing = self.past.as_ref().is_some_and(|past| past.playing);
        let then = if playing { "W stops it" } else { "W plays it on" };
        format!("{what}, step {at} of {steps}: look, but give no orders. {then}; Esc: back to now.")
    }

    /// The replay's row in the sidebar: there when the person has something
    /// to look at again; its button says what it does, and its slider is
    /// where the screen is in it (moved by the person: the screen goes
    /// there).
    fn refresh_replay(&mut self) {
        let shows = self.can_replay() && self.cover.is_none();
        set_visibility(
            &mut self.screen,
            self.replay_row,
            if shows { Visibility::Visible } else { Visibility::Invisible },
        );
        let Some((at, steps)) = self.replay_at().filter(|_| shows) else { return };
        let label = if self.past.as_ref().is_some_and(|past| past.playing) { "Stop" } else { "Replay" };
        self.replay_button.set(&mut self.screen, label);
        // The slider: moved by the person since it was last put somewhere?
        let slider = self.replay_slider;
        let value = self.screen.ui.value(slider).as_float().round().max(0.0) as usize;
        let limits = self.screen.ui.with_widget::<Slider, _>(slider, |w, _| w.limits());
        let moved = value != self.slider_at && limits.is_some_and(|(_, max)| max as usize == steps);
        if moved {
            self.slider_at = value.min(steps);
            self.go_back(self.slider_at);
            return;
        }
        self.slider_at = at;
        self.screen.ui.with_widget::<Slider, _>(slider, |w, cx| {
            w.set_limits(cx, 0.0, steps as f64);
            w.set_value_f(cx, at as f64);
        });
    }

    /// As `see`, with what happened (`Game::take_happened`) already taken.
    fn see_what(&mut self, happened: Vec<(Option<usize>, Event)>) {
        let game = &self.screen.game;
        let key = (game.map.revision(), game.turn(), self.seer(), game.player());
        if self.sight_of == Some(key) {
            self.show_happened(happened, None);
            return;
        }
        self.sight_of = Some(key);
        let now = game.sight(key.2);
        let before = std::mem::replace(&mut self.sight, now);
        self.show_happened(happened, Some(before));
        let game = &self.screen.game;
        let sight = &self.sight;
        let holders = game.map.cities().iter().map(|c| sight.knows(c.pos).then(|| sight.holder(c))).collect();
        let shown = (holders, sight.known_count());
        if shown != self.minimap_of {
            self.minimap_of = shown;
            self.screen.redraw_minimap();
        }
    }

    /// What the person at the screen sees.
    pub fn sight(&self) -> &Sight {
        &self.sight
    }

    /// What happened (`Game::take_happened`) to be shown: the stacks that
    /// went, the person's own and those of the others' they see go (Options:
    /// moves shown), and the battles their side is in that no order of theirs
    /// told (unless battles are told in a line). `before`: what the person
    /// knew before it, if it has changed.
    fn show_happened(&mut self, happened: Vec<(Option<usize>, Event)>, before: Option<Sight>) {
        let told = std::mem::take(&mut self.told);
        if happened.is_empty() || self.cover.is_some() {
            return;
        }
        let before = before.as_ref().unwrap_or(&self.sight);
        // An order of the person's own, in their turn: what was being shown
        // of the moves before is where it went (the battles still play).
        // (Not between the ticks of a round shown again: `show_last_round`.)
        if self.can_act() && !self.together {
            self.showing.finish_moves();
        }
        let seer = self.seer();
        // Whose battles are played out: the person at the screen's, or, in
        // the computer's turns, the last person's (even once they are out:
        // the battle that put them out).
        let watcher = if self.persons_turn() { Some(self.screen.game.player()) } else { self.person_here() };
        let mut shown: Vec<Shown> = Vec::new();
        for (record, event) in happened {
            match event {
                Event::Moved { unit, way, .. } if self.options.show_moves && way.len() > 1 => {
                    let Some(side) = self.screen.game.map.units().get(unit).map(|u| u.side()) else { continue };
                    // The units of a stack went together, one event each.
                    if let Some(Shown::Going(going)) = shown.last_mut() {
                        if going.way == way && going.side == side {
                            going.units.push(unit);
                            continue;
                        }
                    }
                    let own = side == seer;
                    let after = self.replay_after.as_ref().unwrap_or(&self.sight);
                    let seen = |h: &Hex| before.sees(*h) || after.sees(*h);
                    if own || seer == Side::Grey || way.iter().any(seen) {
                        shown.push(Shown::Going(Going { units: vec![unit], side, way, own }));
                    }
                }
                // (Told in a line, with Options: the map shows where all
                // the same.)
                Event::Battle { attacker, defender, .. } => {
                    let theirs = watcher.is_some_and(|w| [attacker, defender].contains(&w));
                    if let Some(record) = record.filter(|r| theirs && !told.contains(r)) {
                        shown.push(Shown::Battle(record));
                    }
                }
                _ => {}
            }
        }
        // (What the person is shown to know is worked out again.)
        if shown.iter().any(|s| matches!(s, Shown::Going(_))) {
            self.shown_sight = None;
        }
        // In rounds, a beat of the round that plays out: its stacks go at
        // the same time (those between two battles, together).
        if self.together {
            shown = crate::showing::beats(shown);
        }
        for shown in shown {
            self.showing.push(shown, before);
        }
    }

    /// Show what is to be shown at `now_ms` (Options: moves shown): the
    /// stacks going (the map brought to them, when the computer plays), the
    /// land opening up as the person's go, a battle in its window.
    fn show(&mut self, now_ms: u64) {
        loop {
            match self.showing.tick(now_ms) {
                Now::Idle | Now::Going(None) | Now::Together(None) => break,
                Now::Going(Some(way)) => {
                    if !self.can_act() {
                        self.bring_into_view(&way);
                    }
                    break;
                }
                // Stacks going together: the map goes to the person's own,
                // if none of them is in view; with none of theirs going, to
                // another's they see, if none of those is in view.
                Now::Together(Some((own, others))) => {
                    let sight = self.shown_sight.as_ref().map_or(&self.sight, |(s, _)| s);
                    let view = &self.screen.view;
                    let in_view = |way: &Vec<Hex>| way.iter().any(|h| sight.sees(*h) && view.shows(*h));
                    let ways = if own.is_empty() { &others } else { &own };
                    let lead = ways.iter().find(|way| way.iter().any(|h| sight.sees(*h))).cloned();
                    if let (false, false, Some(way)) = (self.can_act(), ways.iter().any(in_view), lead) {
                        self.bring_into_view(&way);
                    }
                    break;
                }
                Now::Battle(record, first) => {
                    if first && self.options.quick_battles {
                        // Told in a line: the map goes there all the same,
                        // and the hex flashes a moment.
                        self.flash_battle(record);
                        break;
                    }
                    if first {
                        self.open_battle(record);
                        break;
                    }
                    if self.battle_holds() || self.battle_marked().is_some_and(|_| self.options.quick_battles) {
                        break;
                    }
                    self.showing.battle_over();
                }
            }
        }
        // The land the person's stacks have come to see so far, worked out
        // again when they have been to more of it.
        let opened = self.showing.opened_count(now_ms);
        if self.showing.has_sight() {
            if self.shown_sight.as_ref().is_some_and(|(_, n)| *n == opened) {
                return;
            }
            self.shown_sight = self.showing.sight(&self.sight, now_ms).map(|sight| (sight, opened));
        } else if self.shown_sight.take().is_none() {
            return;
        }
        self.screen.redraw_minimap();
    }

    /// True while the battle shown is in its window: being played out, or
    /// over and still open. What happens next waits until the person
    /// closes it (the owner's word, 2026-11-22: "the advancement should
    /// stop, resolve the combat, and let the user dismiss the box ... before
    /// moving on. The user can miss details otherwise"): Close, Enter or
    /// Esc.
    fn battle_holds(&self) -> bool {
        self.battle.as_ref().is_some_and(|b| b.is_open(&self.screen.ui))
    }

    /// Bring the hexes of `way` the person sees into view, if they are not
    /// (the middle of them in the middle of the map).
    fn bring_into_view(&mut self, way: &[Hex]) {
        let sight = self.shown_sight.as_ref().map_or(&self.sight, |(s, _)| s);
        let seen: Vec<Hex> = way.iter().copied().filter(|h| sight.sees(*h)).collect();
        let hexes = if seen.is_empty() { way.to_vec() } else { seen };
        if hexes.is_empty() || hexes.iter().all(|h| self.screen.view.shows(*h)) {
            return;
        }
        self.screen.view.center_on(hexes[hexes.len() / 2]);
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

    /// Of each unit [`Session::stack`] lists: whether its box is in the
    /// color of those on their way to the hex, and whether it is selected
    /// (pressed in: lighter, in a green line).
    pub fn stack_looks(&self) -> Vec<(bool, bool)> {
        let boxes = self.boxes();
        std::iter::once(&self.leader)
            .chain(&self.slots)
            .zip(boxes)
            .filter(|(_, unit)| unit.is_some())
            .map(|(slot, _)| (slot.coming, self.screen.ui.value(slot.button).as_int() != 0))
            .collect()
    }

    /// The picture the `i`th unit's box shows: the image of the unit's kind
    /// and its side (`None` for an empty box). The leader's: [`Session::leader_icon`].
    pub fn slot_icon(&self, i: usize) -> Option<(&str, Side)> {
        self.slots.get(i)?.shows.as_ref().map(|(image, side, _, _)| (image.as_str(), *side))
    }

    /// The picture the leader's box shows, if the stack has one.
    pub fn leader_icon(&self) -> Option<(&str, Side)> {
        self.leader.shows.as_ref().map(|(image, side, _, _)| (image.as_str(), *side))
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

    /// The icons of these units, as the UI shows them (those that are gone
    /// are left out).
    fn unit_icons(&mut self, ids: &[UnitId]) -> Vec<ImageHandle> {
        let keys: Vec<IconKey> = {
            let units = self.screen.game.map.units();
            ids.iter()
                .filter_map(|id| units.get(*id))
                .map(|u| {
                    let image = units.kind_of(u).map_or(DEFAULT_UNIT_IMAGE, |k| k.image.as_str()).to_string();
                    (image, u.side(), u.level, false)
                })
                .collect()
        };
        keys.iter().map(|k| self.icon(k)).collect()
    }

    /// The icon of a unit whose kind is drawn with `image`, of `side`, for the
    /// UI: drawn the first time it is asked for. A picture that cannot be read
    /// is the default unit's; if that cannot be read either, the disc alone.
    fn icon(&mut self, key: &IconKey) -> ImageHandle {
        if let Some(handle) = self.icons.get(key) {
            return handle.clone();
        }
        let (image, side, level, defending) = key;
        let picture = self.assets.image(image).or_else(|_| self.assets.image(DEFAULT_UNIT_IMAGE)).ok();
        let size = self.screen.ui.dimension_converter().dp_to_px(ICON_DP);
        let mut icon = unit_icon(picture.as_ref(), *side, *level, size);
        if *defending {
            icon = crate::render::unit_icon::defender_badge(icon);
        }
        let name = format!("unit icon {image} {side:?} {level} {defending} {size}");
        let handle = self.screen.ui.image_from_pixels(&name, icon.size, &icon.rgba);
        self.icons.insert(key.clone(), handle.clone());
        handle
    }

    /// Bring the panels up to date with the game.
    fn refresh(&mut self) {
        self.see();
        self.forget_stale_questions();
        self.plan_ways();
        let game = &self.screen.game;
        let map = &game.map;
        let status = info::cursor_line_for(game, &self.sight);
        let title = info::hex_title_for(map, self.shown, &self.sight);
        // The units of the selected hex in their boxes: each one's icon and
        // numbers, and which of them are selected.
        let ids = self.boxes();
        // (One on its way to join the stack: marked.)
        let joining = self.joining_here();
        let units: Vec<Option<(IconKey, String)>> = ids
            .iter()
            .map(|id| {
                let unit = map.units().get((*id)?)?;
                let image = map.units().kind_of(unit).map_or(DEFAULT_UNIT_IMAGE, |kind| kind.image.as_str());
                let numbers = info::unit_numbers(unit);
                // (Shorter, to fit its box with the mark.)
                let numbers = match joining.contains(&unit.id()) {
                    true => format!("+ {}", numbers.split('/').next().unwrap_or(&numbers)),
                    false => numbers,
                };
                Some(((image.to_string(), unit.side(), unit.level, unit.defending), numbers))
            })
            .collect();
        let chosen: Vec<bool> = ids.iter().map(|id| id.is_some_and(|id| game.selection().contains(&id))).collect();
        let coming: Vec<bool> = ids.iter().map(|id| id.is_some_and(|id| joining.contains(&id))).collect();
        // The empty leader's box says whose it is (the map's first kind of
        // commander).
        let leader_word =
            map.units().kinds().iter().find(|k| k.commander > 0).map_or("Leader", |k| k.name.as_str()).to_string();
        // The selected unit in a line, wherever it stands; a stack in a word.
        let line = match game.selected().and_then(|id| map.units().get(id)) {
            _ if self.past.is_some() && self.notice.is_none() => self.past_line(),
            _ if matches!(self.picking, Some(Pick::RallyPoint(_))) => {
                let n = match self.picking {
                    Some(Pick::RallyPoint(n)) => n,
                    _ => 0,
                };
                match &self.refused {
                    Some(why) => format!("Rally point {n}: {why}. Click another hex (Esc: never mind)."),
                    None => format!("Rally point {n}: click the hex where it goes (Esc: never mind)."),
                }
            }
            // (The map outlines the waypoints picked.)
            _ if self.picking == Some(Pick::Patrol) => match (&self.refused, self.stops.len()) {
                (Some(why), _) => format!("Patrol: {why}. Click another hex (Esc: never mind)."),
                (None, 0) => "Patrol from here: click where it goes and back, or Shift+click waypoints and click \
                              the last (Esc: never mind)."
                    .to_string(),
                (None, n) => format!(
                    "Patrol from here, {} so far: click the last (the stack's own hex: round and round; \
                     Shift+click: another).",
                    if n == 1 { "1 waypoint".to_string() } else { format!("{n} waypoints") }
                ),
            },
            _ if self.picking.is_some() => {
                let (what, first) = match self.picking {
                    Some(Pick::GoTo) => ("Go to", "click the hex they go to, over as many turns as it takes"),
                    _ => ("New units go to", "click a hex, or a rally point's flag"),
                };
                let by = match self.stops.is_empty() {
                    true => String::new(),
                    false => format!(" (by way of {})", info::route_words(map, &self.stops)),
                };
                match (&self.refused, self.stops.is_empty()) {
                    (Some(why), _) => format!("{what}{by}: {why}. Click another hex (Esc: never mind)."),
                    (None, true) => format!("{what}: {first} (Shift+click: a stop on the way; Esc: never mind)."),
                    (None, false) => {
                        format!("{what}{by}: click where they end (Shift+click: another stop).")
                    }
                }
            }
            _ if self.notice.is_some() => self.notice.clone().unwrap_or_default(),
            // In rounds: with units on their way to join them.
            Some(_) if game.selection().iter().any(|id| joining.contains(id)) => {
                let coming = game.selection().iter().filter(|id| joining.contains(id)).count();
                format!(
                    "{} units selected, {coming} of them (+, blue) on their way here: sent on, they go as one \
                     stack (click one to leave it out).",
                    game.selection().len()
                )
            }
            Some(unit) if unit.on_duty() => info::duty_line(map, unit, game.selection().len()),
            // (To be sent on from where the plan ends: `Game::select_onward`.)
            Some(unit) if !unit.route.is_empty() && unit.planned && game.onward_from().is_some() => {
                format!(
                    "Planned: to {}. Another click: on from there (the stack itself: afresh).",
                    info::route_words_to(map, &unit.route, None)
                )
            }
            Some(unit) if !unit.route.is_empty() && unit.planned => {
                format!(
                    "Planned: to {} as the round plays out (another click: another plan; F: defend here).",
                    info::route_words_to(map, &unit.route, None)
                )
            }
            Some(unit) if !unit.route.is_empty() && game.selection().len() == 1 => {
                format!(
                    "{} (on the way to {})",
                    info::unit_line(map, unit),
                    info::route_words_to(map, &unit.route, unit.bound)
                )
            }
            Some(_) if game.selection_problem().is_some() => {
                format!("They cannot go together: {}.", game.selection_problem().unwrap_or_default())
            }
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
        let unit_tip = tipped.and_then(|i| map.units().get(ids[i]?)).map(|unit| {
            let mut lines = info::unit_details(map, unit);
            if joining.contains(&unit.id()) {
                let at = unit.pos();
                lines.push(format!("On its way here from row {}, column {}: it joins this stack.", at.row, at.col));
            }
            lines
        });
        let (mut tip, tip_at) = match unit_tip {
            Some(lines) => (lines, tipped.map(|i| (self.slot(i).button, false))),
            None => (Vec::new(), None),
        };
        tip.truncate(TIP_LINES);
        // (A city on land never seen is not known to be there.)
        // (Brief: what it can build is production's, O.)
        let city = map.city_at(self.shown).filter(|c| self.sight.knows(c.pos)).map_or(Vec::new(), |city| {
            let known = info::Known::of(city, self.seer(), &self.sight);
            let lines = info::city_lines_for(map, city, CITY_LINES, known);
            lines.into_iter().take_while(|l| !l.starts_with("Can build")).collect()
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
            // One on its way to the hex has a box of another color.
            let on_its_way = coming.get(i).copied().unwrap_or(false);
            if slot.coming != on_its_way {
                slot.coming = on_its_way;
                screen.ui.set_skin(button, TbId::new(if on_its_way { COMING_SKIN } else { SLOT_SKIN }));
            }
            // The selected units' boxes are pressed in: lighter, in a green
            // line.
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
        // The city, in lines (the owner's word, 2026-11-24: the sidebar
        // "Does not need to display city production info. Can keep basic
        // city info when one is selected"); one's own has a button to its
        // production (O). Another side's is all here.
        for (i, line) in self.city.iter_mut().enumerate() {
            let text = city.get(i).map_or("", String::as_str);
            line.set(&mut self.screen, text);
            // (An empty line takes no room: the list under it moves up.)
            let shown = if text.is_empty() { Visibility::Gone } else { Visibility::Visible };
            set_visibility(&mut self.screen, line.widget, shown);
        }
        let viewer = self.viewer();
        let own = self.people_in_play()
            && self.screen.game.map.city_at(self.shown).is_some_and(|c| c.side == viewer && viewer != Side::Grey);
        set_visibility(&mut self.screen, self.city_details, if own { Visibility::Visible } else { Visibility::Gone });
        self.refresh_orders();
        // The button says whose turn it ends (on a map without sides,
        // nobody's), or, between turns, that the next can start; while the
        // computer plays, whose turn it waits for.
        let (stacks, cities) = self.waiting_now();
        let game = &self.screen.game;
        let (person, side) = (self.persons_turn(), game.player());
        let theirs = person && game.winner().is_none();
        let label = match side {
            _ if self.past.is_some() => "Back to now".to_string(),
            _ if game.winner().is_some() => "Game over".to_string(),
            Side::Grey if game.playing_out() => "Playing the round".to_string(),
            Side::Grey => "End turn".to_string(),
            _ if !person => format!("Waiting for {}", side_name(side)),
            _ if self.between => "Start turn".to_string(),
            // (In rounds a side says its orders are done: the round plays
            // out once every side has.)
            _ if game.rounds() && stacks + cities > 0 => format!("Orders done ({})", stacks + cities),
            _ if game.rounds() => "Orders done".to_string(),
            // What still waits for orders, counted.
            _ if stacks + cities > 0 => format!("End turn ({})", stacks + cities),
            side => format!("End {}'s turn", side_name(side)),
        };
        self.end_turn.set(&mut self.screen, &label);
        // It is a person's to press, on their turn (or to come back from
        // the past).
        self.screen.ui.set_disabled(self.end_turn.widget, !theirs && self.past.is_none());
        self.refresh_replay();
        // The treasury of the person at the screen (none on a map without
        // sides): "34", "+40", "-32"; and its cities of all the map's,
        // "9/24" (the owner's word, 2026-11-24: "my city count/total cities
        // on the menu bar with the gold info"; how many there are is no
        // secret, only who holds those in the fog).
        let viewer = self.viewer();
        let gold = if viewer == Side::Grey {
            [String::new(), String::new(), String::new(), String::new()]
        } else {
            let game = &self.screen.game;
            [
                game.gold(viewer).to_string(),
                format!("+{}", game.income(viewer)),
                format!("-{}", game.upkeep(viewer)),
                info::cities_held(&game.map, viewer),
            ]
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
        // Undo is there while there is something of the person's to take
        // back (in the computer's turn, the last order is the computer's).
        let undo = self.can_act() && self.screen.game.can_undo();
        self.screen.ui.set_disabled(self.undo, !undo);
        // Go to and Done are about the person's selected units.
        let game = &self.screen.game;
        let own = self.can_act()
            && game.selected().and_then(|id| game.map.units().get(id)).is_some_and(|u| u.side() == game.player());
        for button in [self.go_to, self.done] {
            if self.screen.ui.node(button).is_some_and(|n| n.is_disabled() == own) {
                self.screen.ui.set_disabled(button, !own);
            }
        }
        // (Once the battle that ended it has been played out.)
        let playing =
            self.battle.as_ref().is_some_and(|b| b.is_open(&self.screen.ui) && !b.done()) || !self.showing.is_idle();
        if let (Some(winner), None, false) = (self.screen.game.winner(), self.game_over, playing) {
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
        if let Some(log) = &mut self.log {
            let refilled = log.refresh(&mut self.screen.ui, &self.screen.game, viewer, new_from);
            log.set_status(&mut self.screen.ui, &status);
            if refilled {
                self.screen.settle();
            }
        }
        if self.rally_window.as_ref().is_some_and(|w| !w.is_open(&self.screen.ui)) {
            self.rally_window = None;
        }
        // The production window: the viewer's, as the game is now (it works
        // out again only what changed).
        if self.production.as_ref().is_some_and(|w| !w.is_open(&self.screen.ui)) {
            self.production = None;
        }
        let acting = self.can_act();
        let area = self.screen.map_rect();
        if let Some(window) = &mut self.production {
            window.place(&mut self.screen.ui, area);
            if window.refresh(&mut self.screen.ui, &self.screen.game, (viewer, acting)) {
                self.screen.settle();
            }
        }
        self.show_chosen_city();
        // The disband window asks about units that are all there still, and
        // the player's (one fell in a battle: the question is over).
        let gone = self.disband.as_ref().is_some_and(|w| {
            let game = &self.screen.game;
            w.units.iter().any(|id| game.map.units().get(*id).is_none_or(|u| u.side() != game.player()))
        });
        if gone {
            if let Some(window) = self.disband.take() {
                window.close(&mut self.screen.ui);
            }
        }
        // The patrol's window shut by its title bar's button: let go, as
        // Never mind does (the stack is selected again).
        if self.patrol_window.as_ref().is_some_and(|w| !w.is_open(&self.screen.ui)) {
            self.let_patrol_go();
        }
        if let Some(window) = &mut self.rally_window {
            window.refresh(&mut self.screen.ui, &self.screen.game, viewer);
        }
        // The person's rally points' flags.
        let points = self.screen.game.rally_points(self.seer());
        self.flags = (0..points.len()).filter_map(|n| Some((points[n]?.at, n as u8))).collect();
        let (seer, actor) = (self.seer(), self.actor());
        let points = self.screen.game.rally_points(viewer).map(|p| p.is_some());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        if let Some(window) = &mut self.city_window {
            window.set_points(points);
            let known = game
                .map
                .cities()
                .get(window.city())
                .map_or(info::Known::All, |c| info::Known::of(c, seer, &self.sight));
            if !window.is_open(ui) || !window.refresh(ui, &game.map, (actor, known)) {
                window.close(ui);
                self.city_window = None;
            } else {
                window.set_walls(ui, &game.map, actor, walls_price(game, actor, window.city()));
            }
        }
        if let Some(list) = &mut self.city_list {
            if list.is_open(ui) {
                list.refresh(ui, &game.map, &self.sight);
            } else {
                self.city_list = None;
            }
        }
    }

    /// A city in full: the person's own in the production panel, chosen
    /// there (the owner's word, 2026-11-24: "The O button will open the
    /// merged prod dialog on the selected city"); another side's (or
    /// nobody's) shows only in the sidebar, as it is seen from outside.
    fn open_city(&mut self, id: CityId) {
        let viewer = self.viewer();
        if self.screen.game.map.cities().get(id).is_some_and(|c| c.side == viewer && viewer != Side::Grey) {
            self.open_production(Some(id));
        }
    }

    /// The icons of what a city can build, in its holder's colors.
    fn production_icons(&mut self, id: CityId) -> Vec<ImageHandle> {
        let keys: Vec<IconKey> = self.screen.game.map.cities().get(id).map_or(Vec::new(), |city| {
            let units = self.screen.game.map.units();
            city.units
                .iter()
                .map(|p| {
                    let kind = units.kind_named(&p.basename).and_then(|k| units.kind(k));
                    (kind.map_or(DEFAULT_UNIT_IMAGE, |k| k.image.as_str()).to_string(), city.side, city.trains, false)
                })
                .collect()
        });
        keys.iter().map(|key| self.icon(key)).collect()
    }

    /// The sidebar's list of what needs orders, and the army, as the game
    /// is (made again only when it changed), as tall as the sidebar leaves
    /// it.
    fn refresh_orders(&mut self) {
        let side = if self.people_in_play() && !self.covered() { self.viewer() } else { Side::Grey };
        let rows = orders_list::rows(&self.screen.game, side);
        let map = &self.screen.game.map;
        let keys: Vec<Option<IconKey>> = rows
            .iter()
            .map(|row| match row.task {
                crate::game::Task::Stack { at, .. } => {
                    let about = self.screen.game.task_units(side, at);
                    let unit = about.first().or(map.units_at(at).first()).and_then(|id| map.units().get(*id))?;
                    let image = map.units().kind_of(unit).map_or(DEFAULT_UNIT_IMAGE, |kind| kind.image.as_str());
                    Some((image.to_string(), unit.side(), unit.level, unit.defending))
                }
                crate::game::Task::City { .. } => None,
            })
            .collect();
        let icons: Vec<Option<ImageHandle>> = keys.iter().map(|k| k.as_ref().map(|k| self.icon(k))).collect();
        let width = self.screen.minimap_rect().w - self.screen.ui.dimension_converter().dp_to_px(18);
        if self.orders.show(&mut self.screen.ui, rows, &icons, width) {
            self.orders_h = -1;
        }
        // Its box: what the minimap, the buttons and the city leave.
        let ui = &self.screen.ui;
        let sidebar = ui.rect(self.sidebar);
        let top = ui.rect(self.orders_scroll).y;
        let h = (sidebar.h - top).max(ui.dimension_converter().dp_to_px(40));
        if h != self.orders_h && top > 0 {
            let w = self.screen.minimap_rect().w;
            self.screen.ui.set_layout_params(self.orders_scroll, Some(LayoutParams::fixed(w, h)));
            if let Some(parent) = self.screen.ui.parent(self.orders_scroll) {
                self.screen.ui.invalidate_layout(parent);
            }
            self.orders_h = h;
        }
    }

    /// The rows of the sidebar's list.
    pub fn orders_rows(&self) -> &[orders_list::Row] {
        self.orders.rows()
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
        let mut list = CityList::open(ui, root, &game.map, own, viewer, (area, LIST_ALIGN), &self.sight);
        // The selected hex's city, if it is in the list, is the chosen one.
        if let Some(city) = game.map.city_at(self.shown) {
            list.choose(ui, &game.map, city.id, &self.sight);
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
            self.temple.as_ref().and_then(|w| open(w.window())),
            self.disband.as_ref().and_then(|w| open(w.window())),
            self.options_window.as_ref().and_then(|w| open(w.window())),
            self.end_ask.as_ref().and_then(|w| open(w.window)),
            self.rally_window.as_ref().and_then(|w| open(w.window())),
            self.patrol_window.as_ref().and_then(|w| open(w.window())),
            self.mission_ask.as_ref().and_then(|(w, _, _)| open(w.window)),
            self.refusal.as_ref().and_then(|w| open(w.window)),
            self.production.as_ref().and_then(|w| open(w.window())),
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
            Some(5) => self.temple.take().map(|w| w.close(ui)).is_some(),
            Some(6) => self.disband.take().map(|w| w.close(ui)).is_some(),
            Some(7) => self.options_window.take().map(|w| w.close(ui)).is_some(),
            Some(8) => self.end_ask.take().map(|w| build::close(ui, w.window)).is_some(),
            Some(9) => self.rally_window.take().map(|w| w.close(ui)).is_some(),
            Some(10) => {
                self.let_patrol_go();
                true
            }
            Some(11) => self.mission_ask.take().map(|(w, _, _)| build::close(ui, w.window)).is_some(),
            Some(12) => self.refusal.take().map(|w| build::close(ui, w.window)).is_some(),
            Some(13) => {
                self.close_production();
                true
            }
            _ => false,
        }
    }

    /// The patrol's window is let go (Never mind, Esc): its units are
    /// selected again, as they were before it asked.
    fn let_patrol_go(&mut self) {
        let Some(window) = self.patrol_window.take() else { return };
        let units = window.units.clone();
        window.close(&mut self.screen.ui);
        let game = &mut self.screen.game;
        if units.iter().all(|id| game.map.units().get(*id).is_some_and(|u| u.side() == game.player())) {
            game.deselect();
            for id in units {
                game.toggle_unit(id);
            }
        }
    }

    /// Do what a window asked.
    fn obey(&mut self, order: CityOrder) {
        // Only looking, but in the person's turn: an order starts it, if
        // it has not been started.
        let looks = matches!(order, CityOrder::Details(_) | CityOrder::GoTo(_) | CityOrder::Chose(_));
        if !looks {
            if !self.can_act() {
                return;
            }
            self.order_given();
        }
        match order {
            CityOrder::SetProduction(id, what) => {
                // An order: only the city's holder, if that is the player.
                self.city_order(Order::Build { city: id, what });
            }
            CityOrder::Details(id) => self.open_city(id),
            CityOrder::PickRally(id) => {
                (self.picking, self.refused, self.rally_item) = (Some(Pick::Rally(id)), None, None);
                self.stops.clear();
                self.make_way(id);
            }
            CityOrder::ItemPick(id, i) => {
                (self.picking, self.refused, self.rally_item) = (Some(Pick::Rally(id)), None, Some(i));
                self.stops.clear();
                self.make_way(id);
            }
            CityOrder::Chose(_) => {}
            CityOrder::QueueAdd(id, what) => {
                let Some(c) = self.screen.game.map.cities().get(id) else { return };
                // Its units go where the last item's went (or, the first, where
                // the city's went).
                let sends = c.queue.items.last().map_or(c.sends.clone(), |item| item.sends.clone());
                let at = c.queue.items.len();
                if at < MAX_QUEUE {
                    self.edit_queue(id, |items, _| items.push(QueueItem { what, count: 1, sends }));
                    if let Some(window) = &mut self.city_window {
                        window.choose_item(at);
                    }
                }
            }
            CityOrder::Enqueue(id, what) => {
                let Some(c) = self.screen.game.map.cities().get(id) else { return };
                let (fresh, building, sends) = (c.queue.is_empty(), c.building, c.sends.clone());
                let at = if fresh { usize::from(building.is_some()) } else { c.queue.items.len() };
                if at >= MAX_QUEUE {
                    self.refuse("The queue is full", &format!("a queue holds {MAX_QUEUE} items at most"));
                    return;
                }
                self.edit_queue(id, |items, end| {
                    if fresh {
                        // What it builds now is the first item, going where
                        // its units go; the queue repeats, as building with
                        // no queue did.
                        items.extend(building.map(|what| QueueItem { what, count: 1, sends: sends.clone() }));
                        *end = QueueEnd::Repeat { from: 0, times: None };
                    }
                    let sends = items.last().map_or(sends, |item| item.sends.clone());
                    items.push(QueueItem { what, count: 1, sends });
                });
                if let Some(window) = &mut self.city_window {
                    window.choose_item(at);
                }
            }
            CityOrder::QueueCount(id, i, up) => self.edit_queue(id, |items, _| {
                if let Some(item) = items.get_mut(i) {
                    item.count = if up { item.count + 1 } else { item.count.saturating_sub(1).max(1) };
                }
            }),
            CityOrder::QueueRemove(id, i) => {
                let chosen = self.city_window.as_ref().and_then(|w| w.item());
                self.edit_queue(id, |items, _| {
                    if i < items.len() {
                        items.remove(i);
                    }
                });
                // The item chosen is the same item still (or the one after
                // the one taken out).
                if let (Some(window), Some(c)) = (&mut self.city_window, chosen) {
                    window.choose_item(if c > i { c - 1 } else { c });
                }
            }
            CityOrder::QueueStop(id) => self.edit_queue(id, |_, end| *end = QueueEnd::Stop),
            CityOrder::QueueRepeat(id, from) => self.edit_queue(id, |_, end| {
                let times = match *end {
                    QueueEnd::Repeat { times, .. } => times,
                    QueueEnd::Stop => None,
                };
                *end = QueueEnd::Repeat { from, times };
            }),
            CityOrder::QueueTimes(id, up) => self.edit_queue(id, |_, end| {
                if let QueueEnd::Repeat { from, times } = *end {
                    // For ever, then once, twice ...: up goes along, down back.
                    let times = match (times, up) {
                        (None, true) => Some(1),
                        (Some(n), true) => Some(n + 1),
                        (Some(1) | None, false) => None,
                        (Some(n), false) => Some(n - 1),
                    };
                    *end = QueueEnd::Repeat { from, times };
                }
            }),
            CityOrder::ItemKind(id, i, what) => self.edit_queue(id, |items, _| {
                if let Some(item) = items.get_mut(i) {
                    item.what = what;
                }
            }),
            CityOrder::ItemStay(id, i) => self.item_sends(id, i, |_| Sends::Stay),
            CityOrder::ItemDefend(id, i) => self.item_sends(id, i, |_| Sends::Defend),
            CityOrder::ItemPoint(id, i, n) => {
                self.item_sends(id, i, |old| Sends::Point {
                    n,
                    bless: matches!(old, Sends::Point { bless: true, .. }),
                });
            }
            CityOrder::ItemBless(id, i) => self.item_sends(id, i, |old| match old {
                Sends::Point { n, bless } => Sends::Point { n: *n, bless: !bless },
                other => other.clone(),
            }),
            CityOrder::NoRally(id) => self.city_order(Order::Rally { city: id, to: Sends::Stay }),
            CityOrder::DefendNew(id) => self.city_order(Order::Rally { city: id, to: Sends::Defend }),
            CityOrder::ToPoint(id, n) => {
                // (A blessing on the way stays as it was, from point to point.)
                let bless = self
                    .screen
                    .game
                    .map
                    .cities()
                    .get(id)
                    .is_some_and(|c| matches!(c.sends, Sends::Point { bless: true, .. }));
                self.city_order(Order::Rally { city: id, to: Sends::Point { n, bless } });
            }
            CityOrder::ToggleBless(id) => {
                let sends = self.screen.game.map.cities().get(id).map(|c| c.sends.clone());
                if let Some(Sends::Point { n, bless }) = sends {
                    self.city_order(Order::Rally { city: id, to: Sends::Point { n, bless: !bless } });
                }
            }
            CityOrder::RaiseWalls(id) => {
                self.city_order(Order::Walls { city: id });
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

    /// A window says the order the person gave was refused (`what`: what
    /// was not done) and why; OK, Enter, Esc or the next click on the map
    /// closes it. A pick under way goes on.
    fn refuse(&mut self, what: &str, why: &str) {
        self.close_refusal();
        let (root, area) = (self.screen.root_window(), self.window_area());
        let ui = &mut self.screen.ui;
        let parts = build::window(ui, root, REFUSED_ID, "Not done");
        build::heading(ui, parts.content, &format!("{what}."));
        build::text(ui, parts.content, &format!("{}.", title(why)));
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        build::gap(ui, parts.content, space);
        build::primary_button(ui, parts.content, REFUSED_OK_ID, "OK");
        // (At the bottom of the map, over the line under the stack: out of
        // the way of the next click, a pick going on.)
        build::place(ui, parts, area, (0.5, 1.0));
        self.refusal = Some(parts);
        self.screen.settle();
    }

    /// A pick refused (`why`): the prompt says so, and so does a window
    /// (what it was a pick for); the pick goes on.
    fn refuse_pick(&mut self, why: String) {
        let what = match self.picking {
            Some(Pick::GoTo) if self.refused_stop => "They cannot go by way of there",
            Some(Pick::GoTo) => "They cannot go there",
            Some(Pick::Rally(_)) => "The city's new units cannot go there",
            Some(Pick::RallyPoint(_)) => "The rally point cannot go there",
            Some(Pick::Patrol) => "They cannot patrol there",
            None => "Not done",
        };
        self.refuse(what, &why);
        self.refused = Some(why);
    }

    /// Close the window that said an order was refused, if it is open.
    fn close_refusal(&mut self) {
        if let Some(parts) = self.refusal.take() {
            build::close(&mut self.screen.ui, parts.window);
        }
    }

    /// What the window that says an order was refused says, while it is
    /// open.
    pub fn refusal(&self) -> Option<Vec<String>> {
        let parts = self.refusal.as_ref().filter(|p| build::is_open(&self.screen.ui, p.window))?;
        let ui = &self.screen.ui;
        // (Its lines and its button, in order.)
        let lines = ui.children(parts.content).iter().filter_map(|id| ui.text(*id)).filter(|t| !t.is_empty());
        Some(lines.map(str::to_string).collect())
    }

    /// True if what is selected is what the screen chose by itself (and the
    /// person has not taken it).
    fn chosen_by_screen(&self) -> bool {
        let selection = self.screen.game.selection();
        !selection.is_empty() && selection == self.auto_selection.as_slice()
    }

    /// Ask before a click sends `units` (the selected stack) to `hex`:
    /// `mission`, whose orders of their own would end, in words
    /// (`info::mission`); and
    /// `join`, if another of the person's stacks is there (its rally point,
    /// if it is one; whether the screen chose `units`): join it, or take
    /// that stack instead (the first button is the one the screen's choice
    /// makes likelier: take it; the person's, join it).
    fn ask_mission(
        &mut self,
        hex: Hex,
        units: Vec<UnitId>,
        (mission, join): (Option<&str>, Option<(Option<u8>, bool)>),
    ) {
        self.close_mission_ask();
        self.mission_windows = self.open_windows();
        let (root, area) = (self.screen.root_window(), self.window_area());
        let (them, their, they_go) = match units.len() {
            1 => ("it", "Its", "it goes"),
            _ => ("them", "Their", "they go"),
        };
        let place = info::place(&self.screen.game.map, hex);
        let rounds = self.screen.game.rounds();
        let ui = &mut self.screen.ui;
        let title = if join.is_some() { "Join, or take that stack?" } else { "New orders?" };
        let parts = build::window(ui, root, MISSION_ASK_ID, title);
        let space = ui.dimension_converter().dp_to_px(SPACING_DP * 2);
        let send = match join {
            Some((Some(n), _)) => {
                build::heading(ui, parts.content, &format!("Send {them} to rally point {n}, or take the stack there?"));
                format!("Send {them} to the point")
            }
            Some((None, _)) => {
                build::heading(ui, parts.content, &format!("Join the stack at {place}, or take that stack?"));
                "Join it".to_string()
            }
            None => format!("Send {them}"),
        };
        // (In rounds nothing moves yet: what joining is for.)
        if rounds && matches!(join, Some((None, _))) {
            let text = "They join it as the round plays out. Send that stack on afterwards and it waits for them: \
                        they go, and fight, as one.";
            build::text(ui, parts.content, text);
        }
        match (mission, join) {
            (Some(mission), Some(_)) => {
                build::text(ui, parts.content, &format!("{mission}: that ends if {they_go}."));
            }
            (Some(mission), None) => {
                build::heading(ui, parts.content, &format!("{mission}."));
                build::text(ui, parts.content, &format!("Send {them} to {place} instead? {their} orders end."));
            }
            (None, _) => {}
        }
        build::gap(ui, parts.content, space);
        let buttons = build::row(ui, parts.content, space);
        match join {
            Some((_, true)) => {
                build::primary_button(ui, buttons, MISSION_TAKE_ID, "Take that stack");
                build::button(ui, buttons, MISSION_GO_ID, &send);
                build::button(ui, buttons, MISSION_KEEP_ID, "Never mind");
                build::text(ui, parts.content, "(Enter takes it; Esc: never mind.)");
            }
            Some((_, false)) => {
                build::primary_button(ui, buttons, MISSION_GO_ID, &send);
                build::button(ui, buttons, MISSION_TAKE_ID, "Take that stack");
                build::button(ui, buttons, MISSION_KEEP_ID, "Never mind");
                build::text(ui, parts.content, &format!("(Enter: {}; Esc: never mind.)", send.to_lowercase()));
            }
            None => {
                build::primary_button(ui, buttons, MISSION_GO_ID, &send);
                build::button(ui, buttons, MISSION_KEEP_ID, &format!("Keep {} orders", their.to_lowercase()));
                build::text(ui, parts.content, "(Enter sends; Esc keeps.)");
            }
        }
        build::place(ui, parts, area, (0.5, 0.3));
        self.mission_ask = Some((parts, hex, units));
        self.mission_takes = matches!(join, Some((_, true)));
        self.screen.settle();
    }

    /// Take the stack on the hex the window asked about, instead of sending
    /// the selected one there.
    fn mission_take(&mut self) {
        let Some((parts, hex, _)) = self.mission_ask.take() else { return };
        build::close(&mut self.screen.ui, parts.window);
        self.screen.game.deselect();
        self.auto_selection.clear();
        self.choose(hex);
    }

    /// Which of the screen's windows are open, a bit each (the window that
    /// asks about a stack's orders goes when another opens over it).
    fn open_windows(&self) -> u32 {
        let ui = &self.screen.ui;
        let parts = |p: &Option<WindowParts>| p.as_ref().is_some_and(|p| build::is_open(ui, p.window));
        let open = [
            self.temple.as_ref().is_some_and(|w| w.is_open(ui)),
            self.disband.as_ref().is_some_and(|w| w.is_open(ui)),
            self.patrol_window.as_ref().is_some_and(|w| w.is_open(ui)),
            self.options_window.as_ref().is_some_and(|w| w.is_open(ui)),
            self.rally_window.as_ref().is_some_and(|w| w.is_open(ui)),
            self.city_window.as_ref().is_some_and(|w| w.is_open(ui)),
            self.city_list.as_ref().is_some_and(|w| w.is_open(ui)),
            self.log.as_ref().is_some_and(|w| w.is_open(ui)),
            self.battle.as_ref().is_some_and(|w| w.is_open(ui)),
            parts(&self.end_ask),
            parts(&self.refusal),
            parts(&self.cover),
            parts(&self.game_over),
            parts(&self.ask_new),
            self.production.as_ref().is_some_and(|w| w.is_open(ui)),
        ];
        open.iter().enumerate().fold(0, |bits, (i, open)| bits | (u32::from(*open) << i))
    }

    /// In rounds: the ways the person's stacks take as the round plays out
    /// (`Game::ways_this_round`), worked out again when the map changed or
    /// the person did.
    fn plan_ways(&mut self) {
        let game = &self.screen.game;
        if !game.rounds() || !self.persons_turn() || self.cover.is_some() {
            if self.plans_of.take().is_some() {
                self.plans.clear();
                self.own_way.clear();
            }
            return;
        }
        // (The whole selection: a unit left out by its box goes its own
        // way, another stack's.)
        let key = (game.map.revision(), game.player(), game.selection().to_vec(), self.options.own_orders_only);
        if self.plans_of.as_ref() != Some(&key) {
            let selection = game.selection();
            let (mut own, mut others): (Vec<Hex>, Vec<Hex>) = (Vec::new(), Vec::new());
            for (units, way) in game.ways_by_stack(key.1) {
                let selected = units.iter().any(|id| selection.contains(id));
                let into = if selected { &mut own } else { &mut others };
                for hex in way {
                    if !into.contains(&hex) {
                        into.push(hex);
                    }
                }
            }
            if self.options.own_orders_only {
                others.clear();
            }
            (self.plans, self.own_way) = (others, own);
            self.plans_of = Some(key);
        }
    }

    /// V, or the View menu: in rounds the map outlines every stack's
    /// orders, or the selected stack's only.
    fn toggle_orders_shown(&mut self) {
        self.options.own_orders_only = !self.options.own_orders_only;
        self.notice = Some(
            if self.options.own_orders_only {
                "The map outlines the selected stack's orders only (V: everyone's)."
            } else {
                "The map outlines all your stacks' orders (V: the selected stack's only)."
            }
            .to_string(),
        );
    }

    /// The window pixel in the middle of a hex of the main map.
    fn pixel_of(&self, hex: Hex) -> Point {
        let [x, y] = self.screen.view.to_ndc(self.screen.view.hex_center(hex));
        let r = self.screen.map_rect();
        Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32)
    }

    /// Move the map so that the middle of `hex` is at the window pixel
    /// `to`, as far as the map's edges let it.
    fn put_hex_at(&mut self, hex: Hex, to: Point) {
        let (view, map) = (&self.screen.view, self.screen.map_rect());
        let (rel, middle, (half_w, half_h)) = (view.hex_center(hex), view.middle(), view.half_extent());
        let x = 2.0 * (to.x - map.x) as f32 / map.w.max(1) as f32 - 1.0;
        let y = 1.0 - 2.0 * (to.y - map.y) as f32 / map.h.max(1) as f32;
        self.screen.view.set_middle(middle[0] + rel[0] - x * half_w, middle[1] + rel[1] - y * half_h);
    }

    /// A battle is shown on `at`: the map rings the hex and it flashes
    /// (the owner's word, 2026-11-22: "When combat starts it can be hard
    /// to tell where ... open the combat window above the location (so you
    /// can see it) and do something to highlight the stacks fighting"),
    /// for `ms` (`None`: while the battle's window is open); and the hex is
    /// in view, clear of the battle's window if one is open. The window
    /// goes to the first of its places (`window_places`: the top of the
    /// map first) that leaves the hex clear with the map as it is; if none
    /// does, the map moves to show the hex below the window at the top, or
    /// beside it at a side; and where the map cannot move so (its edge, a
    /// map all in view), the window goes where the hex is clear all the
    /// same, over the sidebar if it must. (The owner, 2026-11-22: "a
    /// battle at the top of the map has the battle hex covered by the
    /// combat dialog".)
    fn mark_battle(&mut self, at: Hex, ms: Option<u64>) {
        self.battle_mark = Some((at, self.now, ms.map(|ms| self.now + ms)));
        if !self.screen.view.shows(at) {
            self.screen.view.center_on(at);
        }
        let Some(window) = self.battle.as_ref().filter(|b| b.is_open(&self.screen.ui)).map(|b| b.window()) else {
            return;
        };
        let (rect, map) = (self.screen.ui.rect(window), self.screen.map_rect());
        let screen = self.screen.ui.rect(self.screen.ui.root());
        let edge = self.screen.ui.dimension_converter().dp_to_px(SPACING_DP);
        // (A hex's height, in pixels: the hex and half of one around it
        // are to show.)
        let near = (map.h as f32 * self.screen.view.to_ndc([0.0, 1.0])[1].abs()) as i32;
        let places = window_places(map, screen, (rect.w, rect.h), edge);
        let clear = |s: &Session| places.iter().copied().find(|place| apart(*place, s.pixel_of(at)) >= near);
        let mut place = clear(self);
        // The map moves: the hex in the middle of what a window at the
        // top, at the left or over the sidebar leaves of the map.
        let (top, left, far) = (places[0], places[2], places[6]);
        let room = 3 * near.max(1);
        let (mid_x, mid_y) = (map.x + map.w / 2, map.y + map.h / 2);
        let shown = [
            (map.y + map.h - (top.y + top.h), Point::new(mid_x, (top.y + top.h + map.y + map.h) / 2)),
            (map.x + map.w - (left.x + left.w), Point::new((left.x + left.w + map.x + map.w) / 2, mid_y)),
            (far.x - map.x, Point::new((map.x + far.x) / 2, mid_y)),
        ];
        for (space, to) in shown {
            if place.is_none() && space >= room {
                let before = self.screen.view.middle();
                self.put_hex_at(at, to);
                place = clear(self);
                if place.is_none() {
                    self.screen.view.set_middle(before[0], before[1]);
                }
            }
        }
        // (A window as big as the map and more: where it covers the hex
        // least.)
        let p = self.pixel_of(at);
        let place = place.or_else(|| places.iter().copied().max_by_key(|place| apart(*place, p))).unwrap_or(rect);
        self.screen.ui.set_rect(window, place);
    }

    /// The hex a battle shown is fought on, while it is marked; and how
    /// bright its flash is now (0 to 1): on and off while the battle is
    /// being played out, or for as long as it is marked with no window.
    fn battle_marked(&self) -> Option<(Hex, f32)> {
        let (at, since, until) = self.battle_mark?;
        let window = self.battle.as_ref().filter(|b| b.is_open(&self.screen.ui));
        let playing = match until {
            Some(until) => self.now < until,
            None => window.is_some_and(|b| !b.done()),
        };
        if !playing && (until.is_some() || window.is_none()) {
            return None;
        }
        let phase = (self.now.saturating_sub(since) % BATTLE_FLASH_MS) as f32 / BATTLE_FLASH_MS as f32;
        Some((at, if playing { 1.0 - (2.0 * phase - 1.0).abs() } else { 0.0 }))
    }

    /// The window that asks about a stack's orders goes once it is not the
    /// question: the selection is another, a pick is under way, or another
    /// window has opened over it. And the screen's choice of a stack is
    /// forgotten once the selection is anything else (the person's choice
    /// from then on, even of the same stack again).
    fn forget_stale_questions(&mut self) {
        let selection = self.screen.game.selection();
        if let Some((_, _, units)) = &self.mission_ask {
            let same = selection.len() == units.len() && units.iter().all(|u| selection.contains(u));
            if !same || self.picking.is_some() || self.open_windows() & !self.mission_windows != 0 {
                self.close_mission_ask();
            }
        }
        if self.screen.game.selection() != self.auto_selection.as_slice() {
            self.auto_selection.clear();
        }
    }

    /// The window that asked about a stack's orders goes, if it is open.
    fn close_mission_ask(&mut self) {
        if let Some((parts, _, _)) = self.mission_ask.take() {
            build::close(&mut self.screen.ui, parts.window);
        }
    }

    /// The person said to send the stack: the click goes through (if the
    /// stack is still what is selected).
    fn mission_go(&mut self) {
        let Some((parts, hex, units)) = self.mission_ask.take() else { return };
        build::close(&mut self.screen.ui, parts.window);
        if self.screen.game.selection() == units.as_slice() {
            self.overriding = true;
            self.choose(hex);
            self.overriding = false;
        }
    }

    /// True while a window asks before a stack's orders are overridden.
    pub fn asks_mission(&self) -> bool {
        self.mission_ask.as_ref().is_some_and(|(parts, _, _)| build::is_open(&self.screen.ui, parts.window))
    }

    /// The person gave an order: their turn starts, if it has not (their log
    /// was open between turns): "so does any order".
    fn order_given(&mut self) {
        if self.between && self.persons_turn() {
            self.start_turn_with(false);
        }
    }

    /// The pick under way, if any, is over (another stack or city is gone
    /// to): its stops are forgotten, and a window that made way for it is
    /// not coming back.
    fn drop_pick(&mut self) {
        (self.picking, self.refused, self.rally_item, self.reopen) = (None, None, None, None);
        self.stops.clear();
        // (And the patrol's window, and one that asks about a stack's
        // orders: the person has gone on to something else.)
        if let Some(window) = self.patrol_window.take() {
            window.close(&mut self.screen.ui);
        }
        self.close_mission_ask();
    }

    /// The city's window makes way for the map while a hex is picked for
    /// its units (it is back after the pick: `come_back`).
    fn make_way(&mut self, city: CityId) {
        if let Some(window) = self.city_window.take_if(|w| w.city() == city) {
            self.reopen = Some((city, window.item()));
            let embedded = window.embedded();
            window.close(&mut self.screen.ui);
            // In the production panel: the panel makes way, all of it.
            if embedded {
                self.close_production();
            }
        }
    }

    /// The city's window that made way for a pick is back, its item chosen.
    fn come_back(&mut self) {
        let Some((city, item)) = self.reopen.take() else { return };
        self.open_city(city);
        if let (Some(window), Some(i)) = (&mut self.city_window, item) {
            window.choose_item(i);
        }
    }

    /// Change `city`'s queue as `edit` says (its items, what it does after
    /// the last), and give the order. A repeat from an item that is gone
    /// repeats from the last; with no items, there is no queue.
    fn edit_queue(&mut self, city: CityId, edit: impl FnOnce(&mut Vec<QueueItem>, &mut QueueEnd)) {
        let Some(c) = self.screen.game.map.cities().get(city) else { return };
        let (mut items, mut end) = (c.queue.items.clone(), c.queue.end);
        edit(&mut items, &mut end);
        // The queue emptied: the city builds nothing (the queue is the one
        // way to build, `CityOrder::Enqueue`).
        if items.is_empty() {
            self.city_order(Order::Build { city, what: None });
            return;
        }
        if let QueueEnd::Repeat { from, times } = end {
            end = match items.len() {
                0 => QueueEnd::Stop,
                n => QueueEnd::Repeat { from: from.min(n - 1), times },
            };
        }
        self.city_order(Order::Queue { city, items, end });
    }

    /// Give an order about a city; if it is refused, the line under the
    /// stack says why.
    fn city_order(&mut self, order: Order) {
        if let Err(why) = self.screen.game.apply(order) {
            self.refuse("The city cannot do that", &why);
        }
    }

    /// Where the units of `city`'s queue's `i`th item go: what `to` makes
    /// of where they went.
    fn item_sends(&mut self, city: CityId, i: usize, to: impl FnOnce(&Sends) -> Sends) {
        self.edit_queue(city, |items, _| {
            if let Some(item) = items.get_mut(i) {
                item.sends = to(&item.sends);
            }
        });
    }

    /// A click on a button of a window about cities, or on one that opens
    /// one. False if it was none of those.
    fn window_click(&mut self, id: TbId) -> bool {
        if id == TbId::new(PRODUCTION_ID) {
            self.toggle_production();
            return true;
        }
        if id == TbId::new(production_window::PRODUCTION_CLOSE_ID) {
            self.close_production();
            return true;
        }
        if self.production.as_ref().is_some_and(|w| w.owns(id)) {
            let click = self.production.as_mut().and_then(|w| w.click(id));
            if let Some(click) = click {
                self.production_click(click);
            }
            return true;
        }
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
        if let Some(order) = self.city_window.as_mut().and_then(|w| w.click(id)) {
            self.obey(order);
            return true;
        }
        if let Some(task) = self.orders.click(id) {
            self.see_to(task);
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
                order = list.click(ui, map, id, sight);
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

    /// One frame of host work before drawing (see `MapScreen::frame`): what
    /// is being shown goes on (`show`); a turn shown to its end is handed
    /// over; and on the computer's turns, once nothing is being shown, it
    /// plays on ([`COMPUTER_TURN_MS`] after its last turn): an order at a
    /// time with moves shown (Options), else a whole turn.
    pub fn frame(&mut self, now_ms: u64) {
        self.now = now_ms;
        // In the past nothing goes on but what is shown of it: a replay
        // being played, a step once the one before has been shown.
        if self.past.is_some() {
            self.see();
            self.show(now_ms);
            if self.past.as_ref().is_some_and(|past| past.playing) && self.showing.is_idle() {
                self.replay_step();
                self.show(now_ms);
            }
            if let Some(battle) = &mut self.battle {
                battle.tick(&mut self.screen.ui, now_ms);
            }
            self.screen.frame(now_ms);
            self.refresh();
            return;
        }
        self.see();
        self.show(now_ms);
        if self.pass_due && self.showing.is_idle() {
            self.pass_due = false;
            self.turn_passed();
        }
        // (In rounds the computer's orders are plans: there is nothing to
        // follow before the round plays out, and nothing to wait for.)
        let rounds = self.screen.game.rounds();
        let due = rounds || self.last_play.is_none_or(|last| now_ms >= last.saturating_add(COMPUTER_TURN_MS));
        let idle = self.showing.is_idle() && !self.pass_due;
        if self.screen.game.playing_out() {
            // In rounds: the round plays out, a tick shown before the next.
            if self.cover.is_none() && idle {
                self.play_round(now_ms);
                self.show(now_ms);
            }
        } else if !self.persons_turn() && self.cover.is_none() && due && idle {
            if self.options.show_moves && !rounds {
                self.play_steps(now_ms);
                self.show(now_ms);
            } else {
                let before = (self.screen.game.player(), self.screen.game.turn());
                self.play_one();
                if (self.screen.game.player(), self.screen.game.turn()) != before {
                    self.last_play = Some(now_ms);
                }
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
        // An order starts the turn, if it has not been started (with
        // nothing selected by the screen: the click is the person's).
        if self.between {
            self.start_turn_with(false);
        }
        // In rounds, a round being shown again (hotseat): the stacks are
        // where they went (a plan makes no events to cut it short, and the
        // person would plan for stacks drawn where they are not); its
        // battles still play.
        if self.screen.game.rounds() {
            self.showing.finish_moves();
        }
        // With a stack selected: a click on another of the person's stacks
        // it could join (in reach, or at a rally point) asks whether it
        // joins it or that stack is taken instead (the owner's word,
        // 2026-11-21: "switching stacks is legit but so is combining them
        // ... a popup to ask intent"); one beyond reach is taken. Any other
        // order for a stack with orders of its own (it defends, marches,
        // patrols, marauds) asks first too ("a popup ... explaining the
        // units' current mission and asking for permission to override").
        let by_screen = self.chosen_by_screen();
        let game = &self.screen.game;
        let selection = game.selection().to_vec();
        let units: Vec<&crate::state::Unit> = selection.iter().filter_map(|id| game.map.units().get(*id)).collect();
        if let Some(here) = units.first().map(|u| u.pos()) {
            let side = game.player();
            let theirs =
                |id: &UnitId| !selection.contains(id) && game.map.units().get(*id).is_some_and(|u| u.side() == side);
            let other_stack = game.map.units_at(hex).iter().any(theirs);
            let to_point = game.rally_target(side, hex);
            let reach = game.can_move_to(hex) || to_point.is_some();
            if hex == here {
                // (The person takes the stack the screen chose.)
                self.auto_selection.clear();
            } else if !self.overriding && other_stack && !reach {
                self.screen.game.deselect();
            } else if !self.overriding && other_stack {
                let mission = info::mission(game, &units);
                self.ask_mission(hex, selection, (mission.as_deref(), Some((to_point, by_screen))));
                return;
            } else if !self.overriding {
                if let Some(mission) = info::mission(game, &units) {
                    self.ask_mission(hex, selection, (Some(&mission), None));
                    return;
                }
            }
        }
        self.close_mission_ask();
        let before = self.screen.game.history().len();
        let sent = self.screen.game.selection().to_vec();
        // (Where a stack selected to be sent on has its plan's end: a click
        // there is no order, and the panels stay about the stack.)
        let end = self.screen.game.onward_from();
        let outcome = self.screen.game.select_hex(hex);
        self.shown = hex;
        if end == Some(hex) {
            let game = &self.screen.game;
            self.shown = game.selected().and_then(|id| game.map.units().get(id)).map_or(hex, |u| u.pos());
        }
        match outcome {
            // Beyond their reach: sent there (Go to). The cursor follows
            // them, or the next stack is selected.
            SelectOutcome::Sent(_) => {
                // (One met on the way, unseen, is a battle: told.)
                self.tell_battle(before);
                if self.screen.game.rounds() {
                    self.notice =
                        Some("Planned: they set off as the round plays out (Ctrl+Z: not after all).".to_string());
                }
                let game = &self.screen.game;
                if let Some(at) = sent.iter().find_map(|id| game.map.units().get(*id)).map(|u| u.pos()) {
                    self.screen.game.set_cursor(at);
                    self.shown = at;
                    self.after_move(at, &sent);
                }
            }
            SelectOutcome::Refused(why) => {
                self.refuse("They cannot go there", &why);
                let game = &self.screen.game;
                self.shown = game.selected().and_then(|id| game.map.units().get(id)).map_or(hex, |u| u.pos());
            }
            SelectOutcome::Fought(_) => {
                self.tell_battle(before);
                self.after_move(hex, &sent);
            }
            // Onto a temple: does the stack seek its blessing? (Options: it
            // does, unasked.)
            // In rounds, a plan: nothing has moved.
            SelectOutcome::Moved { .. } if self.screen.game.rounds() => {
                let game = &self.screen.game;
                let side = game.player();
                let stands =
                    |id: &UnitId| !sent.contains(id) && game.map.units().get(*id).is_some_and(|u| u.side() == side);
                let joins = game.map.units_at(hex).iter().any(stands) && !game.joining(side, hex).is_empty();
                if joins {
                    // Onto a stack of their own: they join it, and the
                    // stack is selected with them, to be sent on as one
                    // (the owner's word, 2026-11-22: "order one stack to
                    // another then combine them and order them as a new
                    // stack").
                    self.screen.game.deselect();
                    self.screen.game.select_hex(hex);
                    self.shown = hex;
                    self.auto_selection.clear();
                    self.notice = Some(
                        "Planned: they join this stack as the round plays out. Sent on, it waits for them \
                         (Ctrl+Z: not after all)."
                            .to_string(),
                    );
                    return;
                }
                let at = sent.iter().find_map(|id| game.map.units().get(*id)).map_or(hex, |u| u.pos());
                self.shown = at;
                // While they have movement left where the plan ends, they
                // stay selected, to be sent on from there: a waypoint (the
                // owner's word, 2026-11-22: "setting a goto point could
                // leave the stack active with MP to spend from that
                // point"). Selected again, they start afresh.
                if self.screen.game.select_onward(&sent) {
                    self.auto_selection.clear();
                    self.picking = None;
                    self.notice = Some(
                        "Planned: they go there as the round plays out. Another click: on from there (the stack: afresh)."
                            .to_string(),
                    );
                    return;
                }
                self.notice = Some(
                    "Planned: they go there as the round plays out, fighting who they meet (Ctrl+Z: not after all)."
                        .to_string(),
                );
                self.after_move(at, &sent);
            }
            SelectOutcome::Moved { .. } => {
                if self.options.always_bless {
                    self.bless_unasked(hex);
                } else {
                    self.offer_blessing(hex);
                }
                self.after_move(hex, &sent);
            }
            _ => {}
        }
    }

    /// The first battle in the history since it was `before` long, if there
    /// was one: played out in its window; or told in a line (Options).
    fn tell_battle(&mut self, before: usize) {
        let history = self.screen.game.history();
        let battle = (before..history.len()).find(|i| matches!(history[*i].event, Event::Battle { .. }));
        if let Some(record) = battle {
            self.told.push(record);
            if self.options.quick_battles {
                self.flash_battle(record);
            } else {
                self.open_battle(record);
            }
        }
    }

    /// A battle told in a line (Options), not played out: how it went
    /// under the map, the map on the hex, which flashes a moment.
    fn flash_battle(&mut self, record: usize) {
        let game = &self.screen.game;
        let side = if self.persons_turn() { game.player() } else { self.viewer() };
        let lines = info::battle_outcome(game, record, side);
        self.notice = Some(format!("{} (L: the log, to watch it.)", lines.join(" ")));
        if let Some(Event::Battle { at, .. }) = game.history().get(record).map(|r| r.event.clone()) {
            self.mark_battle(at, Some(BATTLE_FLASH_MS * 2));
        }
    }

    /// The person's stack (`units`) moved to `at` (or fought there): while
    /// it can go on this turn, it stays selected, whatever the Options (the
    /// owner's word, 2026-11-21: "if a stack is moved and it has more MP
    /// leave it selected, in FOW games the user may have to creep into the
    /// fog ... they can hit D if they are really done", and "the entire
    /// stack, the goal is to make it easier to creep into fog not to break
    /// the stack up"); once it cannot, nothing is, or the next stack that
    /// waits for orders if the person said so (Options).
    fn after_move(&mut self, at: Hex, units: &[UnitId]) {
        if !self.keep_going(units) && self.options.next_after_move {
            self.go_on(Some(at));
        }
    }

    /// Select `units` (the person's, just moved) if they can go on this
    /// turn together: the whole stack, never some of it, those of them on
    /// the hex the first is on and on no standing order (a march still
    /// under way, a patrol, marauding, defending); at sea that is the ship
    /// and those it carries. False, and nothing selected, if together they
    /// can go nowhere (one of them has no moves left, say).
    fn keep_going(&mut self, units: &[UnitId]) -> bool {
        let game = &mut self.screen.game;
        game.deselect();
        let (map, side) = (&game.map, game.player());
        let Some(at) = units.iter().find_map(|id| map.units().get(*id)).map(|u| u.pos()) else { return false };
        let kept: Vec<UnitId> = units
            .iter()
            .filter_map(|id| map.units().get(*id))
            .filter(|u| u.side() == side && u.pos() == at)
            .filter(|u| u.route.is_empty() && !u.defending && !u.on_duty())
            .map(|u| u.id())
            .collect();
        let Some((first, rest)) = kept.split_first() else { return false };
        game.select_unit(*first);
        for id in rest {
            game.toggle_unit(*id);
        }
        if game.range().is_some_and(|r| r.reachable().next().is_some()) {
            // (The person's stack, not the screen's choice.)
            self.auto_selection.clear();
            self.picking = None;
            return true;
        }
        game.deselect();
        false
    }

    /// The person is done with the stack on `here`: the selection goes, and
    /// on to the next stack, if one waits for orders.
    fn go_on(&mut self, here: Option<Hex>) {
        self.picking = None;
        self.screen.game.deselect();
        if !self.screen.game.waiting_stacks(self.screen.game.player()).is_empty() {
            self.cycled = here.or(self.cycled);
            self.next_stack(false);
            self.auto_selection = self.screen.game.selection().to_vec();
        }
    }

    /// Ask whether the person's stack on `at` seeks the temple's blessing, if
    /// it is a temple that has not blessed all of it (`temple_window`).
    fn offer_blessing(&mut self, at: Hex) {
        if !self.persons_turn() {
            return;
        }
        let side = self.screen.game.player();
        let Some(units) = temple_window::seekers(&self.screen.game.map, side, at) else { return };
        if let Some(old) = self.temple.take() {
            old.close(&mut self.screen.ui);
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let icons = self.unit_icons(&units);
        let window = TempleWindow::ask(&mut self.screen.ui, (root, area), &self.screen.game.map, (at, &units), &icons);
        self.temple = Some(window);
        self.screen.settle();
    }

    /// The person's stack on `at` seeks the temple's blessing unasked
    /// (Options), if it is a temple that has not blessed all of it; a line
    /// under the stack says what each was given.
    fn bless_unasked(&mut self, at: Hex) {
        let side = self.screen.game.player();
        let Some(units) = temple_window::seekers(&self.screen.game.map, side, at) else { return };
        let Ok(events) = self.screen.game.apply(Order::Bless { units }) else { return };
        let given: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Event::Blessed { name, blessing, .. } => Some(format!("{name} {}", blessing.words())),
                _ => None,
            })
            .collect();
        self.notice = Some(format!("The temple blessed them: {}.", given.join(", ")));
    }

    /// B, or Seek a blessing: the selected stack (or, with none, the stack
    /// on the hex shown) asks.
    pub fn seek_blessing(&mut self) {
        if !self.can_act() {
            return;
        }
        let game = &self.screen.game;
        let at = game.selected().and_then(|id| game.map.units().get(id)).map_or(self.shown, |u| u.pos());
        self.offer_blessing(at);
    }

    /// The stack the temple's window asked about seeks the blessing; the
    /// window then says what each was given.
    fn bless_stack(&mut self) {
        let Some(ask) = self.temple.take() else { return };
        self.order_given();
        let (at, units) = (ask.at, ask.units.clone());
        ask.close(&mut self.screen.ui);
        let (root, area) = (self.screen.root_window(), self.window_area());
        let selection = self.screen.game.selection().to_vec();
        let events = match self.screen.game.apply(Order::Bless { units: units.clone() }) {
            Ok(events) => events,
            Err(why) => {
                // (The stack went from the temple, or some of it did.)
                self.temple = Some(TempleWindow::refused(&mut self.screen.ui, (root, area), at, &why));
                self.screen.settle();
                return;
            }
        };
        let icons = self.unit_icons(&units);
        let window = TempleWindow::tell(
            &mut self.screen.ui,
            (root, area),
            &self.screen.game.map,
            (at, &units),
            (&events, &icons),
        );
        self.temple = Some(window);
        // The stack that was selected (kept after its move onto the temple)
        // has spent its moves on the blessing: it is no longer selected, and
        // the next stack that waits is, if the person said so (Options).
        if !selection.is_empty() && selection.iter().all(|id| units.contains(id)) {
            self.after_move(at, &selection);
        }
        self.screen.settle();
    }

    /// X, or Disband: ask whether the selected units (the person's) go home
    /// for good.
    pub fn ask_disband(&mut self) {
        let units = self.screen.game.selection().to_vec();
        self.ask_disband_of(units);
    }

    /// Ask whether `units` (those of them that are the player's) go home
    /// for good: the selected units (X), or a stack of the production
    /// window's.
    fn ask_disband_of(&mut self, units: Vec<UnitId>) {
        if !self.can_act() {
            return;
        }
        let game = &self.screen.game;
        let units: Vec<UnitId> = units
            .into_iter()
            .filter(|id| game.map.units().get(*id).is_some_and(|u| u.side() == game.player()))
            .collect();
        if units.is_empty() {
            return;
        }
        if let Some(old) = self.disband.take() {
            old.close(&mut self.screen.ui);
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let icons = self.unit_icons(&units);
        let window = crate::disband_window::DisbandWindow::ask(
            &mut self.screen.ui,
            (root, area),
            &self.screen.game.map,
            (&units, None),
            &icons,
        );
        self.disband = Some(window);
        self.screen.settle();
    }

    /// The units the window asked about go home for good.
    /// (Those of them still there and the player's; a pick for them, Go to,
    /// is over. If the game refuses, the window says why, and asks again.)
    fn disband_units(&mut self) {
        let Some(ask) = self.disband.take() else { return };
        self.order_given();
        let asked = ask.units.clone();
        ask.close(&mut self.screen.ui);
        self.picking = None;
        let game = &self.screen.game;
        let units: Vec<UnitId> = asked
            .iter()
            .copied()
            .filter(|id| game.map.units().get(*id).is_some_and(|u| u.side() == game.player()))
            .collect();
        if units.is_empty() {
            return;
        }
        if let Err(why) = self.screen.game.apply(Order::Disband { units: units.clone() }) {
            let (root, area) = (self.screen.root_window(), self.window_area());
            let icons = self.unit_icons(&units);
            let window = crate::disband_window::DisbandWindow::ask(
                &mut self.screen.ui,
                (root, area),
                &self.screen.game.map,
                (&units, Some(&why)),
                &icons,
            );
            self.disband = Some(window);
        }
        self.screen.settle();
    }

    /// The disband window, while it is open.
    pub fn disband(&self) -> Option<&crate::disband_window::DisbandWindow> {
        self.disband.as_ref().filter(|w| w.is_open(&self.screen.ui))
    }

    /// The temple's window, while it is open.
    pub fn temple(&self) -> Option<&TempleWindow> {
        self.temple.as_ref().filter(|t| t.is_open(&self.screen.ui))
    }

    /// Watch the battle of the history's record `record`, from the start, if
    /// the game still keeps it.
    pub fn open_battle(&mut self, record: usize) {
        let Some(fight) = self.screen.game.fight(record).cloned() else { return };
        if let Some(old) = self.battle.take() {
            old.close(&mut self.screen.ui);
        }
        let icons = [&fight.attackers, &fight.defenders].map(|units| {
            units.iter().map(|u| self.icon(&(u.image.clone(), u.side, u.level, false))).collect::<Vec<_>>()
        });
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
        let window = BattleWindow::open(
            &mut self.screen.ui,
            root,
            &fight,
            icons,
            (&place, outcome),
            (area, (0.5, 0.2)),
            self.options.fast_battles,
        );
        self.battle = Some(window);
        self.screen.settle();
        self.mark_battle(fight.at, None);
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

    /// True if something happened to `side` since it last ended a turn
    /// that its log tells: what the others did to it, and what went wrong
    /// for it at the end of its last turn or the start of this one (a march
    /// stopped short, a battle on patrol, units left unpaid); not what goes
    /// on as it should (units built, blessed on their way).
    fn has_news(&self, side: Side) -> bool {
        let game = &self.screen.game;
        let records = game.history().get(self.seen_by(side)..).unwrap_or_default();
        records.iter().any(|r| {
            let routine = r.turn == side
                && matches!(r.event, Event::Built { .. } | Event::Blessed { .. } | Event::Promoted { .. });
            !routine && !info::record_lines(&game.map, r, side).is_empty()
        })
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
        if game.playing_out() {
            return "The round plays out...".to_string();
        }
        if !self.persons_turn() && side != Side::Grey {
            let does = if game.rounds() { "gives its orders" } else { "is playing" };
            return format!("{} {does}...", title(side_name(side)));
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
        let side = self.screen.game.player();
        if self.persons_turn() { side } else { self.person_here().unwrap_or(side) }
    }

    /// The person the screen is: who played here last; or, when nobody has
    /// yet (a game the computer begins, a saved game played on in the
    /// computer's turn or while a round plays out), the person whose turn
    /// comes next (the review, 2026-11-23: the screen was the computer's
    /// side's until then, or nobody's, and fog of war hid nothing).
    fn person_here(&self) -> Option<Side> {
        self.last_person.or_else(|| {
            let game = &self.screen.game;
            let order = game.order();
            let at = order.iter().position(|s| *s == game.player()).unwrap_or(0);
            (0..order.len())
                .map(|i| order[(at + i) % order.len()])
                .find(|s| !game.out().contains(s) && game.controller(*s) == Some(&Controller::Person))
        })
    }

    /// Who gives the orders at this screen now: the side whose turn it is, if
    /// a person plays it here and the game goes on; else nobody
    /// (`Side::Grey`: the buttons that give orders are greyed).
    fn actor(&self) -> Side {
        if self.can_act() { self.screen.game.player() } else { Side::Grey }
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
        if !self.can_act() || self.screen.game.apply(Order::Undo).is_err() {
            return;
        }
        self.showing.finish_moves();
        self.order_given();
        let game = &mut self.screen.game;
        if let Some(pos) = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos()) {
            game.set_cursor(pos);
            self.shown = pos;
        }
    }

    /// Select the next stack of the player's that waits for orders, and look
    /// at it: the nearest to the selected stack (or else to the last one this
    /// went to, or to the cursor) that it has not gone to since it last went
    /// round them all. With none waiting, any of the player's stacks. `back`
    /// (Shift+N): the one it came from before, back along the way it came.
    pub fn next_stack(&mut self, back: bool) {
        self.drop_pick();
        // (The person's choice: N.)
        self.auto_selection.clear();
        let (side, mine) = (self.viewer(), self.persons_turn());
        let game = &self.screen.game;
        let selected = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        let from = selected.or(self.cycled);
        let stacks = game.stacks_of(side);
        let hex = if back {
            // Back the way it came, to a stack that is still there.
            let mut came = None;
            while let Some(hex) = self.trail.pop() {
                if Some(hex) != from && stacks.contains(&hex) {
                    came = Some(hex);
                    break;
                }
            }
            came.or_else(|| game.next_stack_of(side, from, true))
        } else {
            let waiting = game.waiting_stacks(side);
            let pool = if waiting.is_empty() { stacks } else { waiting };
            let mut fresh: Vec<Hex> =
                pool.iter().copied().filter(|h| Some(*h) != from && !self.visited.contains(h)).collect();
            if fresh.is_empty() {
                // Round them all: again from the nearest.
                self.visited.clear();
                fresh = pool.iter().copied().filter(|h| Some(*h) != from).collect();
            }
            let origin = from.unwrap_or(game.map.cursor);
            let nearest = fresh.into_iter().min_by_key(|h| (origin.distance(*h), h.row, h.col));
            // The stack it leaves has been seen to, too.
            if let (Some(_), Some(from)) = (nearest, from) {
                self.trail.push(from);
                if !self.visited.contains(&from) {
                    self.visited.push(from);
                }
            }
            nearest.or(pool.first().copied())
        };
        let Some(hex) = hex else { return };
        if !back {
            self.visited.push(hex);
        }
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
        self.drop_pick();
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
        if own && self.can_act() {
            self.drop_pick();
            self.picking = Some(Pick::GoTo);
        }
    }

    /// P, or Patrol: the next clicks on the map (or Enter) pick the selected
    /// units' waypoints, from where they stand: Shift+click one on the way,
    /// a plain click the last (their own hex again: round and round). Then a
    /// window asks whether they attack.
    pub fn start_patrol(&mut self) {
        let game = &self.screen.game;
        let own = game.selected().and_then(|id| game.map.units().get(id)).is_some_and(|u| u.side() == game.player());
        if own && self.can_act() {
            self.drop_pick();
            self.picking = Some(Pick::Patrol);
        }
    }

    /// The last waypoint of a patrol was picked: if it is a patrol the
    /// selected units can go, the window asks how it fights; else the line
    /// says why, and the picking goes on.
    fn patrol_picked(&mut self, hex: Hex) {
        let game = &self.screen.game;
        let units = game.selection().to_vec();
        let Some(here) = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos()) else { return };
        let waypoints: Vec<Hex> = [here].into_iter().chain(self.stops.iter().copied()).chain([hex]).collect();
        let checked = Patrol::new(&waypoints, None)
            .and_then(|patrol| game.check_waypoints(game.player(), &units, &waypoints).map(|()| patrol));
        let patrol = match checked {
            Ok(patrol) => patrol,
            Err(why) => {
                self.picking = Some(Pick::Patrol);
                self.refuse_pick(why);
                return;
            }
        };
        self.stops.clear();
        if let Some(old) = self.patrol_window.take() {
            old.close(&mut self.screen.ui);
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let picked = (units.as_slice(), waypoints.as_slice());
        let window = PatrolWindow::ask(&mut self.screen.ui, (root, area), picked, &patrol, self.patrol_odds);
        self.patrol_window = Some(window);
        // While it asks, nothing is selected: a click or Enter on the map
        // gives the stack no order (it keeps its units, and the map outlines
        // its waypoints).
        self.screen.game.deselect();
        self.screen.settle();
    }

    /// What the patrol's window chose: the units patrol, attacking at those
    /// odds or better (or never), and take this turn's part at once.
    fn patrol_chosen(&mut self, attack: Option<u32>) {
        let Some(window) = self.patrol_window.take() else { return };
        let (units, waypoints) = (window.units.clone(), window.waypoints.clone());
        window.close(&mut self.screen.ui);
        if !self.can_act() {
            return;
        }
        self.drop_pick();
        self.order_given();
        self.patrol_odds = attack;
        let before = self.screen.game.history().len();
        match self.screen.game.apply(Order::Patrol { units: units.clone(), waypoints, attack }) {
            Ok(_) => {
                // The cursor follows them; nothing stays selected (or the
                // next stack is, if the person said so).
                let game = &self.screen.game;
                let at = units.iter().find_map(|id| game.map.units().get(*id)).map(|u| u.pos());
                self.screen.game.deselect();
                self.tell_battle(before);
                if let Some(at) = at {
                    self.screen.game.set_cursor(at);
                    self.shown = at;
                    self.after_move(at, &units);
                }
            }
            Err(why) => {
                // (They stay selected, to try again.)
                for (i, id) in units.iter().enumerate() {
                    if i == 0 {
                        self.screen.game.select_unit(*id);
                    } else {
                        self.screen.game.toggle_unit(*id);
                    }
                }
                self.refuse("They cannot patrol there", &why);
            }
        }
    }

    /// The window that asks how a patrol fights, while it is open.
    pub fn patrol_window(&self) -> Option<&PatrolWindow> {
        self.patrol_window.as_ref().filter(|w| w.is_open(&self.screen.ui))
    }

    /// U, or Maraud: the computer leads the selected units (`marauder`), at
    /// once with what movement they have and at the end of each of their
    /// side's turns, until another order; if they all maraud already, they
    /// are called back (and stay selected). Then on to the next stack, as
    /// with Done.
    pub fn toggle_maraud(&mut self) {
        if !self.can_act() {
            return;
        }
        self.order_given();
        let game = &self.screen.game;
        let units = game.selection().to_vec();
        let on = !units.iter().all(|id| game.map.units().get(*id).is_some_and(|u| u.marauding));
        let here = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        if units.is_empty() {
            return;
        }
        if let Err(why) = self.screen.game.apply(Order::Maraud { units: units.clone(), on }) {
            self.refuse(if on { "They cannot maraud" } else { "They cannot be called back" }, &why);
            return;
        }
        if !on {
            self.screen.game.refresh();
            return;
        }
        let before = self.screen.game.history().len();
        crate::marauder::lead(&mut self.screen.game, &units);
        self.tell_battle(before);
        let game = &self.screen.game;
        let at = units.iter().find_map(|id| game.map.units().get(*id)).map(|u| u.pos()).or(here);
        if let Some(at) = at {
            self.screen.game.set_cursor(at);
            self.shown = at;
        }
        self.go_on(at);
    }

    /// At the end of a person's turn, the computer leads their marauders
    /// (once a turn).
    fn lead_marauders(&mut self) {
        let game = &self.screen.game;
        let turn = (game.turn(), game.player());
        if !self.persons_turn() || game.winner().is_some() || self.marauded == Some(turn) {
            return;
        }
        self.marauded = Some(turn);
        crate::marauder::maraud(&mut self.screen.game, turn.1);
    }

    /// F, or Defend: the selected units defend where they stand, until they
    /// are sent somewhere (Next stack and the tasks pass them by); if they
    /// all defend already, they stand down. Then on to the next stack, as
    /// with Done.
    pub fn defend_stack(&mut self) {
        if !self.can_act() {
            return;
        }
        self.order_given();
        let game = &self.screen.game;
        let units = game.selection().to_vec();
        let on = !units.iter().all(|id| game.map.units().get(*id).is_some_and(|u| u.defending));
        if units.is_empty() {
            return;
        }
        if let Err(why) = self.screen.game.apply(Order::Defend { units, on }) {
            self.refuse(if on { "They cannot defend here" } else { "They cannot stand down" }, &why);
            return;
        }
        if !on {
            // Standing down: they stay selected, to be sent somewhere.
            self.screen.game.refresh();
            return;
        }
        self.after_aside();
    }

    /// Done (D, or the Done button): the selected units are put aside for
    /// this turn (Next stack and the tasks pass them by), and the next
    /// stack that waits is selected.
    pub fn done_with_stack(&mut self) {
        if !self.can_act() {
            return;
        }
        self.order_given();
        let units = self.screen.game.selection().to_vec();
        let before = self.screen.game.history().len();
        // A marauder that fought goes on (as a patrol does, by the rules).
        let game = &self.screen.game;
        let marauders: Vec<UnitId> = units
            .iter()
            .copied()
            .filter(|id| game.map.units().get(*id).is_some_and(|u| u.marauding && u.fought))
            .collect();
        let here = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        if units.is_empty() || self.screen.game.apply(Order::Done { units }).is_err() {
            return;
        }
        crate::marauder::lead(&mut self.screen.game, &marauders);
        self.tell_battle(before);
        self.go_on(here);
    }

    /// The selected stack was put aside (done, or defending): the selection
    /// goes, and on to the next stack, if one waits for orders.
    fn after_aside(&mut self) {
        let game = &self.screen.game;
        let here = game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos());
        self.go_on(here);
    }

    /// What a pick on the map does: the selected units go there, or the
    /// city sends its new units there.
    fn picked(&mut self, pick: Pick, hex: Hex) {
        if self.between && self.persons_turn() {
            self.start_turn_with(false);
        }
        match pick {
            Pick::GoTo => {
                let units = self.screen.game.selection().to_vec();
                let before = self.screen.game.history().len();
                let via: Vec<Hex> = self.onward_stops().into_iter().chain(self.stops.iter().copied()).collect();
                match self.screen.game.apply(Order::GoTo { units: units.clone(), to: hex, via }) {
                    Ok(_) => {
                        self.stops.clear();
                        self.tell_battle(before);
                        // The cursor follows them; nothing stays selected
                        // (or the next stack is, if the person said so).
                        let game = &self.screen.game;
                        let at = units.iter().find_map(|id| game.map.units().get(*id)).map(|u| u.pos());
                        self.screen.game.deselect();
                        if let Some(at) = at {
                            self.screen.game.set_cursor(at);
                            self.shown = at;
                            self.after_move(at, &units);
                        }
                    }
                    // Refused: say why, and pick again.
                    Err(why) => {
                        self.picking = Some(Pick::GoTo);
                        self.refuse_pick(why);
                    }
                }
            }
            Pick::RallyPoint(n) => {
                let side = self.screen.game.player();
                let defend = self.screen.game.rally_point(side, n).is_some_and(|p| p.defend);
                match self.screen.game.apply(Order::RallyPoint { n, point: Some(RallyPoint { at: hex, defend }) }) {
                    Ok(_) => {
                        self.stops.clear();
                        if self.rally_window().is_none() {
                            self.toggle_rally_window();
                        }
                    }
                    Err(why) => {
                        self.picking = Some(pick);
                        self.refuse_pick(why);
                    }
                }
            }
            Pick::Patrol => self.patrol_picked(hex),
            Pick::Rally(city) => {
                // A rally point's hex, with no stops: to that rally point.
                let side = self.screen.game.player();
                let to = match self.screen.game.rally_point_at(side, hex) {
                    Some(n) if self.stops.is_empty() => Sends::Point { n, bless: false },
                    _ => Sends::Route(self.stops.iter().copied().chain([hex]).collect()),
                };
                // (For an item of its queue, if a window chose one.)
                let item = self
                    .rally_item
                    .filter(|i| self.screen.game.map.cities().get(city).is_some_and(|c| *i < c.queue.items.len()));
                let order = match item {
                    Some(i) => {
                        let c = self.screen.game.map.cities().get(city);
                        let (mut items, end) =
                            c.map_or((Vec::new(), QueueEnd::Stop), |c| (c.queue.items.clone(), c.queue.end));
                        items[i].sends = to;
                        Order::Queue { city, items, end }
                    }
                    None => Order::Rally { city, to },
                };
                match self.screen.game.apply(order) {
                    Ok(_) => {
                        self.stops.clear();
                        self.come_back();
                    }
                    // Refused: say why, and pick again.
                    Err(why) => {
                        self.picking = Some(pick);
                        self.refuse_pick(why);
                    }
                }
            }
        }
    }

    /// The stops of the plan the selected stack is to be sent on from
    /// (`Game::onward_from`): a Go to for it goes by way of them, as a
    /// click beyond its reach does. None if it is selected afresh.
    fn onward_stops(&self) -> Vec<Hex> {
        let game = &self.screen.game;
        match game.onward_from() {
            Some(_) => game.selected().and_then(|id| game.map.units().get(id)).map_or(Vec::new(), |u| u.route.clone()),
            None => Vec::new(),
        }
    }

    /// Shift+click (or Shift+Enter) while picking where units go: a stop on
    /// the way, if they (or what the city builds) can get there from the
    /// last; else it says why. The picking goes on: a click without Shift
    /// picks where they end.
    fn add_stop(&mut self, pick: Pick, hex: Hex) {
        // (A rally point has no stops: Shift makes no difference.)
        if let Pick::RallyPoint(_) = pick {
            self.picking = None;
            self.picked(pick, hex);
            return;
        }
        let game = &self.screen.game;
        let side = game.player();
        let route: Vec<Hex> = self.stops.iter().copied().chain([hex]).collect();
        let by_plan: Vec<Hex> = self.onward_stops().into_iter().chain(route.iter().copied()).collect();
        let checked = match pick {
            Pick::GoTo => game.check_route(side, game.selection(), &by_plan),
            Pick::Rally(city) => game.check_rally(side, city, &route),
            Pick::RallyPoint(_) => Ok(()),
            Pick::Patrol => game.check_waypoints(side, game.selection(), &route),
        };
        match checked {
            Ok(()) => {
                self.stops.push(hex);
                self.refused = None;
            }
            Err(why) => {
                self.refused_stop = true;
                self.refuse_pick(why);
                self.refused_stop = false;
            }
        }
    }

    /// See to a task of the list: select its stack and look at it, or open
    /// its city.
    fn see_to(&mut self, task: Task) {
        self.drop_pick();
        // (A stack chosen from the list is the person's choice.)
        self.auto_selection.clear();
        let mine = self.persons_turn();
        match task {
            Task::Stack { at, .. } => {
                // The units the row is about, not all on the hex (the
                // review, 2026-11-23: a row that said "2 units" selected
                // the garrison beside them too).
                let game = &self.screen.game;
                let about = if mine { game.task_units(game.player(), at) } else { Vec::new() };
                self.select_these(at, &about);
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

    /// Select `units`, of the stack on `at`, and no others, and look at
    /// them (a task's row, a stack's row of the production window); not
    /// the person's turn, or none of them: look only.
    fn select_these(&mut self, at: Hex, units: &[UnitId]) {
        self.drop_pick();
        self.auto_selection.clear();
        let mine = self.persons_turn();
        let game = &mut self.screen.game;
        game.deselect();
        if !mine {
            self.look_at(at);
            return;
        }
        let side = game.player();
        let units: Vec<UnitId> =
            units.iter().copied().filter(|id| game.map.units().get(*id).is_some_and(|u| u.side() == side)).collect();
        if let Some((first, rest)) = units.split_first() {
            game.select_unit(*first);
            for id in rest {
                game.toggle_unit(*id);
            }
            self.cycled = Some(at);
        }
        self.look_at(at);
    }

    /// Open the production window (E, or the Cities menu's Production...),
    /// or close it.
    pub fn toggle_production(&mut self) {
        if self.production.as_ref().is_some_and(|w| w.is_open(&self.screen.ui)) {
            self.close_production();
            return;
        }
        let shown = self.screen.game.map.city_at(self.shown).map(|c| c.id);
        self.open_production(shown);
    }

    /// Open the production panel over the map (if it is not open), on
    /// `city` if that is the person's.
    fn open_production(&mut self, city: Option<CityId>) {
        if self.covered() || !self.people_in_play() {
            return;
        }
        let (side, acting) = (self.viewer(), self.can_act());
        // (Its own list of the cities stands for the list of cities, which
        // would be under it.)
        if let Some(list) = self.city_list.take() {
            list.close(&mut self.screen.ui);
        }
        if !self.production.as_ref().is_some_and(|w| w.is_open(&self.screen.ui)) {
            let (root, area) = (self.screen.root_window(), self.screen.map_rect());
            let window = ProductionWindow::open(&mut self.screen.ui, root, &self.screen.game, (side, acting), area);
            self.production = Some(window);
        }
        if let (Some(window), Some(city)) = (&mut self.production, city) {
            window.choose_city(&self.screen.game, city);
            window.refresh(&mut self.screen.ui, &self.screen.game, (side, acting));
        }
        self.show_chosen_city();
        self.screen.settle();
    }

    /// A key of the map's while the production panel is open, which has
    /// them (the owner's word, 2026-11-24: "we want to start making sure the
    /// game works well with the keyboard as well as mouse"). Tab goes from
    /// part to part (the cities, the queue, the units); Up and Down choose
    /// a city or an item; the arrows go from unit to unit, Enter adds the
    /// one there to the queue; - and + one fewer or one more of the chosen
    /// item, Delete takes it out.
    fn production_key(&mut self, key: MapKey) {
        use production_window::Area;
        let Some(window) = self.production.as_mut() else { return };
        let focus = window.focus();
        let city = self.city_window.as_ref().filter(|w| w.embedded()).map(|w| (w.city(), w.item(), w.kinds()));
        let items =
            city.and_then(|(id, _, _)| self.screen.game.map.cities().get(id)).map_or(0, |c| c.queue.items.len());
        match key {
            MapKey::Tab | MapKey::BackTab => {
                window.next_focus(key == MapKey::Tab);
                let units = window.focus() == Area::Units;
                if let Some(city) = self.city_window.as_mut() {
                    let at = city.focused_unit().or(Some(0));
                    city.focus_unit(if units { at } else { None });
                }
            }
            MapKey::Up | MapKey::Down if focus == Area::Cities => {
                window.step(key == MapKey::Down);
            }
            MapKey::Up | MapKey::Down if focus == Area::Queue => {
                if let Some((_, item, _)) = city.filter(|_| items > 0) {
                    let at = item.unwrap_or(0);
                    let next = if key == MapKey::Down { (at + 1).min(items - 1) } else { at.saturating_sub(1) };
                    if let Some(w) = self.city_window.as_mut() {
                        w.choose_item(next);
                    }
                }
            }
            MapKey::Up | MapKey::Down | MapKey::Left | MapKey::Right if focus == Area::Units => {
                if let Some((_, _, kinds)) = city.filter(|c| c.2 > 0) {
                    let at = self.city_window.as_ref().and_then(|w| w.focused_unit()).unwrap_or(0);
                    let row = city_windows::BOXES_A_ROW;
                    let next = match key {
                        MapKey::Left => at.saturating_sub(1),
                        MapKey::Right => (at + 1).min(kinds - 1),
                        MapKey::Up => at.saturating_sub(row),
                        _ => (at + row).min(kinds - 1),
                    };
                    let actor = self.actor();
                    if let Some(w) = self.city_window.as_mut() {
                        w.focus_unit(Some(next));
                        // (Laid out first, so that its details say the unit.)
                        w.refresh(&mut self.screen.ui, &self.screen.game.map, (actor, info::Known::All));
                        w.show_focus(&mut self.screen.ui);
                    }
                }
            }
            MapKey::Enter if focus == Area::Cities => {
                window.set_focus(Area::Units);
                if let Some(w) = self.city_window.as_mut() {
                    w.focus_unit(Some(0));
                }
            }
            MapKey::Enter | MapKey::ShiftEnter if focus == Area::Units => {
                let unit = self.city_window.as_ref().and_then(|w| w.focused_unit());
                if let (Some((id, _, _)), Some(what)) = (city, unit) {
                    self.obey(CityOrder::Enqueue(id, what));
                }
            }
            MapKey::ZoomIn | MapKey::ZoomOut => {
                if let Some((id, Some(item), _)) = city {
                    self.obey(CityOrder::QueueCount(id, item, key == MapKey::ZoomIn));
                }
            }
            MapKey::Delete => {
                if let Some((id, Some(item), _)) = city {
                    self.obey(CityOrder::QueueRemove(id, item));
                }
            }
            _ => {}
        }
    }

    /// Close the production panel, and the city in it.
    fn close_production(&mut self) {
        if let Some(window) = self.city_window.take_if(|w| w.embedded()) {
            window.close(&mut self.screen.ui);
        }
        if let Some(window) = self.production.take() {
            window.close(&mut self.screen.ui);
        }
    }

    /// The city chosen in the production panel is the one in its right
    /// column: built there when the choice changed (the city window's view,
    /// `CityWindow::embed`; the owner's word, 2026-11-24: "Merge the
    /// production and city dialogs").
    fn show_chosen_city(&mut self) {
        let Some(window) = self.production.as_ref().filter(|w| w.is_open(&self.screen.ui)) else {
            if let Some(city) = self.city_window.take_if(|w| w.embedded()) {
                city.close(&mut self.screen.ui);
            }
            return;
        };
        let (chosen, host) = (window.chosen_city(), window.city_host());
        let shown = self.city_window.as_ref().filter(|w| w.embedded()).map(|w| w.city());
        if shown == chosen {
            return;
        }
        if let Some(city) = self.city_window.take() {
            city.close(&mut self.screen.ui);
        }
        let Some(id) = chosen else {
            self.screen.settle();
            return;
        };
        let icons = self.production_icons(id);
        let (seer, actor) = (self.seer(), self.actor());
        let (ui, game) = (&mut self.screen.ui, &self.screen.game);
        let known = game.map.cities().get(id).map_or(info::Known::All, |c| info::Known::of(c, seer, &self.sight));
        self.city_window = CityWindow::embed(ui, host, &game.map, id, (actor, known), &icons);
        if let Some(window) = &mut self.city_window {
            window.set_walls(ui, &game.map, actor, walls_price(game, actor, id));
        }
        if let Some(window) = &mut self.production {
            window.relayout(&mut self.screen.ui);
        }
        self.screen.settle();
    }

    /// What a click in the production window asks for. Looking always;
    /// orders only when the person may give them.
    fn production_click(&mut self, click: ProductionClick) {
        match click {
            ProductionClick::Select { at, units } => self.select_these(at, &units),
            ProductionClick::Disband(units) => self.ask_disband_of(units),
            ProductionClick::StopAll => {
                if !self.can_act() {
                    return;
                }
                self.order_given();
                let side = self.screen.game.player();
                let cities: Vec<CityId> = self
                    .screen
                    .game
                    .map
                    .cities()
                    .iter()
                    .filter(|c| c.side == side && (c.production().is_some() || !c.queue.is_empty()))
                    .map(|c| c.id)
                    .collect();
                let mut refused = Vec::new();
                for city in cities {
                    if let Err(why) = self.screen.game.apply(Order::Build { city, what: None }) {
                        let name = self.screen.game.map.cities().get(city).map_or(String::new(), |c| c.name.clone());
                        refused.push(format!("{name}: {why}"));
                    }
                }
                if !refused.is_empty() {
                    self.refuse("Some cities could not stop", &refused.join("; "));
                }
            }
        }
    }

    /// Go to the next stack or city that needs orders (T; `back`: the one
    /// before, Shift+T), in the sidebar's list: the stack selected, the city
    /// in production (the owner's word, 2026-11-24: the Tasks window went,
    /// "T steps the list").
    pub fn next_needing(&mut self, back: bool) {
        if self.covered() || !self.people_in_play() {
            return;
        }
        self.refresh_orders();
        let shown = self.shown;
        let map = &self.screen.game.map;
        let here = self.orders.rows().iter().map(|r| r.task).find(|t| match t {
            crate::game::Task::Stack { at, .. } => *at == shown,
            crate::game::Task::City { city, .. } => map.cities().get(*city).is_some_and(|c| c.pos == shown),
        });
        if let Some(task) = self.orders.next_needing(here.as_ref(), back) {
            self.see_to(task);
        }
    }

    /// Open the rally points' window (R, or the Units menu's), or close it.
    pub fn toggle_rally_window(&mut self) {
        if let Some(window) = self.rally_window.take() {
            if window.is_open(&self.screen.ui) {
                window.close(&mut self.screen.ui);
                return;
            }
        }
        if self.covered() || !self.people_in_play() {
            return;
        }
        let (root, area, side) = (self.screen.root_window(), self.window_area(), self.viewer());
        let window = RallyWindow::open(&mut self.screen.ui, root, &self.screen.game, side, (area, LIST_ALIGN));
        self.rally_window = Some(window);
        self.screen.settle();
    }

    /// The rally points' window, while it is open.
    pub fn rally_window(&self) -> Option<&RallyWindow> {
        self.rally_window.as_ref().filter(|w| w.is_open(&self.screen.ui))
    }

    /// Do what a click in the rally points' window asked.
    fn rally_click(&mut self, click: RallyClick) {
        if !matches!(click, RallyClick::Look(_)) && self.can_act() {
            self.order_given();
        }
        let side = self.screen.game.player();
        match click {
            RallyClick::Look(n) => {
                if let Some(point) = self.screen.game.rally_point(self.viewer(), n) {
                    self.look_at(point.at);
                }
            }
            RallyClick::Place(n) if self.can_act() => {
                // The window makes way for the map, and is back once the
                // point is placed.
                if let Some(window) = self.rally_window.take() {
                    window.close(&mut self.screen.ui);
                }
                self.drop_pick();
                self.picking = Some(Pick::RallyPoint(n));
            }
            RallyClick::Remove(n) if self.can_act() => {
                if let Err(why) = self.screen.game.apply(Order::RallyPoint { n, point: None }) {
                    self.refuse("The rally point cannot be taken away", &why);
                }
            }
            RallyClick::Defend(n, defend) if self.can_act() => {
                if let Some(point) = self.screen.game.rally_point(side, n) {
                    let point = Some(RallyPoint { defend, ..point });
                    if let Err(why) = self.screen.game.apply(Order::RallyPoint { n, point }) {
                        self.refuse("The rally point cannot be changed", &why);
                    }
                }
            }
            _ => {}
        }
    }

    /// Open the options' window (the Game menu's Options), or close it.
    pub fn toggle_options(&mut self) {
        if let Some(window) = self.options_window.take() {
            if window.is_open(&self.screen.ui) {
                window.close(&mut self.screen.ui);
                return;
            }
        }
        let (root, area) = (self.screen.root_window(), self.window_area());
        let window = OptionsWindow::open(&mut self.screen.ui, root, self.options, (area, (0.5, 0.3)));
        self.options_window = Some(window);
        self.screen.settle();
    }

    /// The options' window, while it is open.
    pub fn options_window(&self) -> Option<&OptionsWindow> {
        self.options_window.as_ref().filter(|w| w.is_open(&self.screen.ui))
    }

    /// What the person at the screen has chosen (Options).
    pub fn options(&self) -> Options {
        self.options
    }

    /// Choose the options (the shell carries them from one game to the next).
    pub fn set_options(&mut self, options: Options) {
        self.options = options;
    }

    /// The list of tasks, while it is open.
    /// The production window, while it is open.
    pub fn production(&self) -> Option<&ProductionWindow> {
        self.production.as_ref().filter(|w| w.is_open(&self.screen.ui))
    }

    /// What the next click on the map picks, if anything.
    pub fn picking(&self) -> Option<Pick> {
        self.picking
    }

    /// The window of the selected hex's city (O, City details), if it is one.
    pub fn open_shown_city(&mut self) {
        let city = self.screen.game.map.city_at(self.shown).filter(|c| self.sight.knows(c.pos));
        if let Some(city) = city.map(|c| c.id) {
            self.open_city(city);
        }
    }

    /// Look at the person's capital (H; in hotseat each turn starts there).
    pub fn go_to_capital(&mut self) {
        if let Some(capital) = self.screen.game.capital(self.viewer()) {
            self.look_at(capital);
        }
    }

    /// Look at the person's rally point `n` (0 to 9, its digit; the owner's
    /// word, 2026-11-24: "Jumping to rally points will be useful to manage
    /// games on huge maps even if not rallying there"): the view and the
    /// cursor go there. One that is not set says so, under the stack.
    pub fn go_to_rally_point(&mut self, n: usize) {
        if self.covered() || !self.people_in_play() {
            return;
        }
        match self.screen.game.rally_point(self.seer(), n as u8) {
            Some(point) => {
                self.look_at(point.at);
                self.notice = None;
            }
            None => self.notice = Some(format!("Rally point {n} is not set: R to place it.")),
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
        let temple = own && units.is_some_and(|u| temple_window::seekers(&game.map, game.player(), u.pos()).is_some());
        // (O: one's own city, in the production panel.)
        let viewer = self.viewer();
        let city = self.people_in_play()
            && game.map.city_at(self.shown).is_some_and(|c| c.side == viewer && viewer != Side::Grey);
        let people = self.people_in_play();
        let orders_shown =
            if self.options.own_orders_only { "Outline all orders" } else { "Outline this stack's orders only" };
        // (In rounds a side says its orders are done.)
        let turn_word = match (self.between, game.rounds()) {
            (true, _) => "Start turn",
            (false, true) => "Orders done",
            (false, false) => "End turn",
        };
        let menu = if id == TbId::new(MENU_GAME_ID) {
            // (In a browser Ctrl+T and Ctrl+N are its own, and a page does
            // not quit: its tab is closed.)
            let keys = |key: char| if WEB { String::new() } else { command(key) };
            let menu = Menu::new(MENU_GAME_ID)
                .item(END_TURN_ID, turn_word, &keys('T'), turn)
                .item(UNDO_ID, "Undo", &command('Z'), turn && game.can_undo())
                .separator()
                .item(OPTIONS_ID, "Options...", &command(','), true)
                .item(NEW_GAME_ASK_ID, "New game...", &keys('N'), true);
            if WEB { menu } else { menu.item(QUIT_ID, "Quit", &command('Q'), true) }
        } else if id == TbId::new(MENU_UNITS_ID) {
            Menu::new(MENU_UNITS_ID)
                .item(NEXT_STACK_ID, "Next stack", "N", people)
                .item(PREV_STACK_ID, "Previous stack", "Shift+N", people)
                .separator()
                .item(GO_TO_ID, "Go to...", "G", own)
                .item(DONE_ID, "Done for this turn", "D", own)
                .item(DEFEND_ID, "Defend here (or stand down)", "F", own)
                .item(PATROL_ID, "Patrol...", "P", own)
                .item(MARAUD_ID, "Maraud (or call back)", "U", own)
                .item(BLESS_ID, "Seek a blessing", "B", temple)
                .item(DISBAND_ID, "Disband...", "X", own)
                .separator()
                .item(RALLY_POINTS_ID, "Rally points...", "R", people)
                .item(DESELECT_ID, "Drop the selection", "Esc", units.is_some())
        } else if id == TbId::new(MENU_CITIES_ID) {
            Menu::new(MENU_CITIES_ID)
                .item(NEXT_CITY_ID, "Next city", "C", people)
                .item(PREV_CITY_ID, "Previous city", "Shift+C", people)
                .item(CITY_DETAILS_ID, "This city's production", "O", city)
                .separator()
                .item(MY_CITIES_ID, "My cities", "M", people)
                .item(ALL_CITIES_ID, "All cities", "A", true)
                .separator()
                .item(PRODUCTION_ID, "Production...", "E", people)
        } else if id == TbId::new(MENU_VIEW_ID) {
            let menu = Menu::new(MENU_VIEW_ID)
                .item(TASKS_ID, "Next that needs orders", "T", people)
                .item(LOG_ID, "Log", "L", true)
                .item(ORDERS_SHOWN_ID, orders_shown, "V", people && self.screen.game.rounds())
                .item(REPLAY_ID, "Replay what happened", "W", self.can_replay())
                .separator()
                .item(CAPITAL_ID, "Your capital", "H", people)
                .item(ZOOM_IN_ID, "Zoom in", "=", true)
                .item(ZOOM_OUT_ID, "Zoom out", "-", true);
            // (In a browser fullscreen is the browser's: `help::window_section`.)
            if WEB { menu } else { menu.item(FULLSCREEN_ID, "Fullscreen", fullscreen_keys(), true) }
        } else if id == TbId::new(MENU_HELP_ID) {
            let mut menu = Menu::new(MENU_HELP_ID).item(HELP_KEYS_ID, "Keys and mouse", "F1  or  ?", true).separator();
            for (topic, title) in guide::TOPICS {
                menu = menu.item(topic, title, "", true);
            }
            menu.separator().item(about::ABOUT_ID, about::TITLE, "", true)
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
                    } else {
                        self.begin_orders();
                        self.show_last_round();
                    }
                }
            }
            self.refresh();
            return;
        }
        for command in self.screen.take_map_commands() {
            if matches!(command, MapCommand::Click(..) | MapCommand::Key(..)) {
                self.notice = None;
            }
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
            // The production panel has the keys while it is open (the
            // owner's word, 2026-11-24: "the game works well with the
            // keyboard as well as mouse"): Up and Down go from city to city;
            // the other arrows and Enter are its, not the hidden map's.
            let producing = self.production.as_ref().is_some_and(|w| w.is_open(&self.screen.ui));
            if producing && !self.screen.help_open() {
                if let MapCommand::Key(key) = command {
                    if key != MapKey::Esc {
                        self.production_key(key);
                        continue;
                    }
                }
            }
            // Esc closes the help if it is open, else the window on top, and
            // with none open it drops the selection.
            // A window that says an order was refused: Esc or Enter closes it
            // (a pick under way goes on), and a click on the map too.
            let refusing = self.refusal.as_ref().is_some_and(|p| build::is_open(&self.screen.ui, p.window));
            if refusing && matches!(command, MapCommand::Key(MapKey::Esc | MapKey::Enter)) {
                self.close_refusal();
                continue;
            }
            if refusing && matches!(command, MapCommand::Click(..)) {
                self.close_refusal();
            }
            if command == MapCommand::Key(MapKey::Esc) && !self.screen.help_open() {
                let picked = self.picking.take().is_some();
                if picked {
                    // (A window that made way for the pick is back.)
                    self.come_back();
                } else if !self.close_top_window() {
                    // (From the past, with nothing selected: back to now.)
                    if self.past.is_some() && self.screen.game.selected().is_none() {
                        self.back_to_now();
                    }
                    self.screen.game.deselect();
                }
            }
            self.screen.navigate(command);
            // While a battle's window is open, Enter is its: it ends the
            // battle if it is still being played out, and closes the window
            // once it is over (one the screen shows by itself, which holds
            // what happens next; the person's own attack; one watched again
            // from the log: it once went to the map for the last two).
            if command == MapCommand::Key(MapKey::Enter) && self.battle_holds() {
                match self.battle.take() {
                    Some(mut battle) if !battle.done() => {
                        battle.skip(&mut self.screen.ui);
                        self.battle = Some(battle);
                    }
                    Some(battle) => battle.close(&mut self.screen.ui),
                    None => {}
                }
                continue;
            }
            // While a window asks about a stack's orders, Enter is its first
            // button.
            if command == MapCommand::Key(MapKey::Enter) && self.asks_mission() {
                if self.mission_takes {
                    self.mission_take()
                } else {
                    self.mission_go()
                }
                continue;
            }
            // While the patrol's window asks, Enter is its Patrol.
            if command == MapCommand::Key(MapKey::Enter) && self.picking.is_none() {
                if let Some(odds) = self.patrol_window().map(|w| w.chosen()) {
                    self.patrol_chosen(odds);
                    continue;
                }
            }
            // A click, or Enter at the cursor, chooses a hex (or picks it).
            // C++: MapWidget::OnEvent
            // With Shift, while picking, a stop on the way.
            let (hex, stop) = match command {
                MapCommand::Click(p, size, shift) => (self.screen.pick(p, size), shift),
                MapCommand::Key(MapKey::Enter) => (Some(self.screen.game.map.cursor), false),
                MapCommand::Key(MapKey::ShiftEnter) => (Some(self.screen.game.map.cursor), true),
                _ => (None, false),
            };
            match (hex, self.picking.take()) {
                (Some(hex), Some(pick)) if stop => {
                    self.picking = Some(pick);
                    self.add_stop(pick, hex);
                }
                (Some(hex), Some(pick)) => {
                    self.refused = None;
                    self.picked(pick, hex);
                }
                (Some(hex), None) => self.choose(hex),
                (None, pick) => self.picking = pick,
            }
        }
        for command in self.screen.pump_shared() {
            self.notice = None;
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
                RootCommand::Click(id) if id == TbId::new(END_ANYWAY_ID) => {
                    self.close_end_ask();
                    self.end_turn();
                }
                RootCommand::Click(id) if id == TbId::new(END_TASKS_ID) => {
                    self.close_end_ask();
                    self.next_needing(false);
                }
                RootCommand::Click(id) if id == TbId::new(END_KEEP_ID) => self.close_end_ask(),
                RootCommand::Click(id) if id == TbId::new(REFUSED_OK_ID) => self.close_refusal(),
                RootCommand::Click(id) if id == TbId::new(MISSION_GO_ID) => self.mission_go(),
                RootCommand::Click(id) if id == TbId::new(MISSION_TAKE_ID) => self.mission_take(),
                RootCommand::Click(id) if id == TbId::new(MISSION_KEEP_ID) => self.close_mission_ask(),
                RootCommand::Click(id) if id == TbId::new(PREV_STACK_ID) => self.next_stack(true),
                RootCommand::Click(id) if id == TbId::new(PREV_CITY_ID) => self.next_city(true),
                RootCommand::Click(id) if id == TbId::new(DESELECT_ID) => {
                    self.picking = None;
                    self.screen.game.deselect();
                }
                RootCommand::Click(id) if id == TbId::new(CAPITAL_ID) => self.go_to_capital(),
                RootCommand::Click(id) if id == TbId::new(about::ABOUT_ID) => {
                    self.screen.show_topic(about::TITLE, &about::sections());
                }
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
                RootCommand::Click(id) if id == TbId::new(DEFEND_ID) => self.defend_stack(),
                RootCommand::Click(id) if id == TbId::new(BLESS_ID) => self.seek_blessing(),
                RootCommand::Click(id) if id == TbId::new(DISBAND_ID) => self.ask_disband(),
                RootCommand::Click(id) if id == TbId::new(crate::disband_window::DISBAND_YES_ID) => {
                    self.disband_units();
                }
                RootCommand::Click(id) if id == TbId::new(crate::disband_window::DISBAND_NO_ID) => {
                    if let Some(window) = self.disband.take() {
                        window.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(temple_window::TEMPLE_BLESS_ID) => self.bless_stack(),
                RootCommand::Click(id)
                    if id == TbId::new(temple_window::TEMPLE_NOT_NOW_ID)
                        || id == TbId::new(temple_window::TEMPLE_CLOSE_ID) =>
                {
                    if let Some(temple) = self.temple.take() {
                        temple.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(PATROL_ID) => self.start_patrol(),
                RootCommand::Click(id) if id == TbId::new(MARAUD_ID) => self.toggle_maraud(),
                RootCommand::Click(id) if id == TbId::new(patrol_window::PATROL_CANCEL_ID) => self.let_patrol_go(),
                RootCommand::Click(id) if self.patrol_window.as_ref().is_some_and(|w| w.owns(id)) => {
                    let click = self.patrol_window.as_mut().and_then(|w| w.click(&mut self.screen.ui, id));
                    if let Some(PatrolClick::Ok(odds)) = click {
                        self.patrol_chosen(odds);
                    }
                }
                RootCommand::Click(id) if id == TbId::new(TASKS_ID) => self.next_needing(false),
                RootCommand::Click(id) if id == TbId::new(ORDERS_SHOWN_ID) => self.toggle_orders_shown(),
                RootCommand::Click(id) if id == TbId::new(REPLAY_ID) => self.play_replay(),
                RootCommand::Click(id) if id == TbId::new(OPTIONS_ID) => self.toggle_options(),
                RootCommand::Click(id) if id == TbId::new(RALLY_POINTS_ID) => self.toggle_rally_window(),
                RootCommand::Click(id) if id == TbId::new(rally_window::RALLY_CLOSE_ID) => {
                    if let Some(window) = self.rally_window.take() {
                        window.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id)
                    if self.rally_window.as_ref().and_then(|w| w.click(&self.screen.ui, id)).is_some() =>
                {
                    if let Some(click) = self.rally_window.as_ref().and_then(|w| w.click(&self.screen.ui, id)) {
                        self.rally_click(click);
                    }
                }
                RootCommand::Key(',', mods) if mods.intersects(Modifiers::CTRL | Modifiers::SUPER) => {
                    self.toggle_options();
                }
                RootCommand::Click(id) if id == TbId::new(options_window::OPTIONS_CLOSE_ID) => {
                    if let Some(window) = self.options_window.take() {
                        window.close(&mut self.screen.ui);
                    }
                }
                RootCommand::Click(id)
                    if self
                        .options_window
                        .as_ref()
                        .and_then(|w| w.click(&self.screen.ui, id, self.options))
                        .is_some() =>
                {
                    if let Some(options) =
                        self.options_window.as_ref().and_then(|w| w.click(&self.screen.ui, id, self.options))
                    {
                        self.options = options;
                    }
                }
                RootCommand::Click(id) if id == TbId::new(log_window::LOG_CLOSE_ID) => {
                    if let Some(log) = self.log.take() {
                        log.close(&mut self.screen.ui);
                    }
                }
                // While production is open: where the chosen item's units go
                // (a rally point's digit; K keep here, F defend here).
                RootCommand::Key(ch, mods)
                    if !mods.intersects(Modifiers::CTRL | Modifiers::SUPER | Modifiers::ALT)
                        && self.production().is_some()
                        && (ch.is_ascii_digit() || matches!(ch.to_ascii_lowercase(), 'k' | 'f')) =>
                {
                    let id = match ch.to_ascii_lowercase() {
                        'k' => city_windows::CITY_NO_RALLY_ID.to_string(),
                        'f' => city_windows::CITY_DEFEND_ID.to_string(),
                        digit => city_windows::point_id(digit as usize - '0' as usize),
                    };
                    let points = self.screen.game.rally_points(self.viewer());
                    let unset = ch.is_ascii_digit() && points[ch as usize - '0' as usize].is_none();
                    let city = self.city_window.as_mut().filter(|w| w.embedded());
                    if unset {
                        self.notice = Some(format!("Rally point {ch} is not set: R to place it."));
                    } else if let Some(order) = city.and_then(|w| w.click(TbId::new(&id))) {
                        self.obey(order);
                    }
                }
                // While production is open: its filters (All, Idle, Waiting,
                // Building).
                RootCommand::Key(ch, mods)
                    if !mods.intersects(Modifiers::CTRL | Modifiers::SUPER | Modifiers::ALT)
                        && self.production().is_some()
                        && matches!(ch.to_ascii_lowercase(), 'a' | 'i' | 'w' | 'b') =>
                {
                    let filter = match ch.to_ascii_lowercase() {
                        'i' => production_window::Filter::Idle,
                        'w' => production_window::Filter::Waiting,
                        'b' => production_window::Filter::Building,
                        _ => production_window::Filter::All,
                    };
                    if let Some(window) = &mut self.production {
                        window.set_filter(filter);
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
                        'f' | 'F' => self.defend_stack(),
                        'b' | 'B' => self.seek_blessing(),
                        'x' | 'X' => self.ask_disband(),
                        't' => self.next_needing(false),
                        'T' => self.next_needing(true),
                        'e' | 'E' => self.toggle_production(),
                        'm' | 'M' => self.toggle_list(true),
                        'a' | 'A' => self.toggle_list(false),
                        'o' | 'O' => self.open_shown_city(),
                        'h' | 'H' => self.go_to_capital(),
                        'r' | 'R' => self.toggle_rally_window(),
                        'p' | 'P' => self.start_patrol(),
                        'u' | 'U' => self.toggle_maraud(),
                        'v' | 'V' => self.toggle_orders_shown(),
                        'w' | 'W' => self.play_replay(),
                        '0'..='9' => self.go_to_rally_point(ch as usize - '0' as usize),
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
                        // (A pick for the units selected before is over.)
                        if self.picking.is_some_and(|p| matches!(p, Pick::GoTo | Pick::Patrol)) {
                            self.drop_pick();
                        }
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

    /// The window closes: the game is kept first (`keep_in`).
    fn confirm_quit(&mut self) -> bool {
        self.save_now();
        true
    }

    fn next_turn(&mut self) -> bool {
        Session::next_turn(self)
    }

    /// Where the selected unit can go: once it is shown there (a stack
    /// kept selected after its move is not marked from where it went
    /// before it has got there, nor into land it has yet to be shown to
    /// see).
    fn marked(&self, hex: Hex) -> bool {
        let game = &self.screen.game;
        game.can_move_to(hex) && !game.selected().is_some_and(|id| self.showing.goes(id))
    }

    /// The stops picked so far (Go to, or a city's rally point), or else
    /// the route of the selected unit, if it is on one.
    /// The person's rally points.
    fn flags(&self) -> &[(Hex, u8)] {
        &self.flags
    }

    fn minimap_marks(&self) -> crate::render::MiniMarks {
        let open = self.production.as_ref().filter(|w| w.is_open(&self.screen.ui));
        open.map(|w| w.minimap_marks(&self.screen.game)).unwrap_or_default()
    }

    fn outlined(&self) -> &[Hex] {
        if let Some(window) = self.patrol_window() {
            return &window.waypoints;
        }
        if self.picking.is_some() {
            return &self.stops;
        }
        let game = &self.screen.game;
        let unit = game.selected().and_then(|id| game.map.units().get(id));
        // In rounds: the ways the person's stacks take as the round plays
        // out (a patrol selected: its waypoints).
        if game.rounds() && unit.is_none_or(|u| u.patrol.is_none()) {
            return &self.plans;
        }
        unit.map_or(&[], |u| u.patrol.as_ref().map_or(u.route.as_slice(), |p| p.stops.as_slice()))
    }

    /// The selected unit shows on its hex, over the others of its stack (in a
    /// city, its pip is ringed).
    fn front_unit(&self) -> Option<UnitId> {
        self.screen.game.selected()
    }

    /// The way the selected stack is to take (rounds), in a color of its
    /// own; the hex the selected stack stands on, in green (once it is
    /// shown there); and the hex a battle shown is fought on.
    fn rings(&self) -> Vec<(Hex, [f32; 3])> {
        let game = &self.screen.game;
        let mut rings: Vec<(Hex, [f32; 3])> = Vec::new();
        if self.picking.is_none() && self.patrol_window().is_none() {
            rings.extend(self.own_way.iter().map(|hex| (*hex, OWN_WAY_BORDER)));
        }
        let selected = game.selected().filter(|id| !self.showing.goes(*id)).and_then(|id| game.map.units().get(id));
        if let Some(unit) = selected.filter(|_| self.cover.is_none()) {
            rings.push((unit.pos(), SELECTED_BORDER));
        }
        if let Some((at, _)) = self.battle_marked() {
            rings.push((at, BATTLE_BORDER));
        }
        rings
    }

    /// The owner's word, 2026-11-22: "Marking the battle locations on the
    /// minimap is a good idea, make sure replay also does it." The battles
    /// the person at the screen was in since they last ended a turn (what
    /// their log has as new: in rounds, the round that played out), and
    /// the one being shown now, which flashes there as on the map. In the
    /// past (the replay), those up to where it has got to.
    fn battles(&self) -> Vec<(Hex, f32)> {
        if self.cover.is_some() {
            return Vec::new();
        }
        let now = self.battle_marked();
        let game = &self.screen.game;
        let mut battles: Vec<(Hex, f32)> = Vec::new();
        if let Some(side) = Scene::viewer(self) {
            for record in game.history().iter().skip(self.seen_by(side)) {
                if let Event::Battle { at, attacker, defender, .. } = &record.event {
                    let shown = now.is_some_and(|(hex, _)| hex == *at);
                    if (*attacker == side || *defender == side) && !shown && !battles.iter().any(|(b, _)| b == at) {
                        battles.push((*at, 0.0));
                    }
                }
            }
        }
        battles.extend(now);
        battles
    }

    fn flash(&self) -> Option<(Hex, [f32; 4])> {
        let (at, bright) = self.battle_marked()?;
        let [r, g, b, a] = BATTLE_FLASH;
        (bright > 0.0).then_some((at, [r, g, b, a * bright]))
    }

    /// The person at the screen: their units show what they are; the
    /// others', on the map, are banners.
    fn viewer(&self) -> Option<Side> {
        Some(self.seer()).filter(|side| *side != Side::Grey)
    }

    fn sight(&self) -> &Sight {
        self.shown_sight.as_ref().map_or(&self.sight, |(sight, _)| sight)
    }

    fn moving(&self) -> Vec<crate::render::Moving> {
        self.showing.moving(&self.screen.game.map, &self.screen.view, self.now)
    }
}

/// A word with a capital: "Red".
fn title(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

/// What raising a city's walls costs `actor` now, or why not: only in its
/// own turn.
fn walls_price(game: &Game, actor: Side, city: CityId) -> Result<u32, String> {
    if game.player() != actor {
        return Err("it is not its turn".to_string());
    }
    game.walls_price(actor, city)
}
