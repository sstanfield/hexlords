//! The options of the person at the screen (the Game menu's Options): how
//! much the game shows them and asks them as a turn goes. They are not
//! rules: they change what the screen does for the person, never what an
//! order does, and the computer's players know nothing of them.
//!
//! Not a port: the C++ had no options.

use turbo_ferret::{Rect, TbId, Ui, WidgetId};

use crate::gui::build::{self, WindowParts};

pub const OPTIONS_WINDOW_ID: &str = "options_window";
pub const OPTIONS_CLOSE_ID: &str = "options_close";
/// The options' boxes.
pub const NEXT_AFTER_MOVE_ID: &str = "option_next_after_move";
pub const QUICK_BATTLES_ID: &str = "option_quick_battles";
pub const FAST_BATTLES_ID: &str = "option_fast_battles";
pub const SHOW_MOVES_ID: &str = "option_show_moves";
pub const ALWAYS_BLESS_ID: &str = "option_always_bless";
pub const ASK_END_TURN_ID: &str = "option_ask_end_turn";

/// The space between things, in dp.
const SPACING_DP: i32 = 8;

/// An option's box: its id, its words, and which option it is.
type OptionBox = (&'static str, &'static str, fn(&mut Options) -> &mut bool);

/// What the person at the screen has chosen.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Options {
    /// The stack that waits for orders next is selected: when a turn
    /// starts, and after a move (a battle too), as after Done.
    pub next_after_move: bool,
    /// A battle of theirs is told in a line under the map instead of played
    /// out in its window (the log's Watch plays it).
    pub quick_battles: bool,
    /// A battle played out goes at twice the pace (at first it goes at
    /// the slower: the owner's word, 2026-11-21, "more tension").
    pub fast_battles: bool,
    /// Moves are shown as they happen: stacks go from hex to hex, the
    /// person's own and the others' they can see, the land opens up as
    /// theirs go, and the computer plays an order at a time with the map on
    /// what it does (the owner's word, 2026-11-21).
    pub show_moves: bool,
    /// A stack moved onto a temple seeks its blessing without being asked.
    pub always_bless: bool,
    /// End turn asks first while stacks wait for orders or cities build
    /// nothing, or the gold will not pay the upkeep.
    pub ask_end_turn: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            next_after_move: true,
            quick_battles: false,
            fast_battles: false,
            show_moves: true,
            always_bless: false,
            ask_end_turn: true,
        }
    }
}

impl Options {
    /// Every option off: the screen selects nothing by itself, tells every
    /// battle in its window, asks at every temple and never before End turn.
    pub fn manual() -> Options {
        Options {
            next_after_move: false,
            quick_battles: false,
            fast_battles: false,
            show_moves: false,
            always_bless: false,
            ask_end_turn: false,
        }
    }

    /// Each option's box: its id, its words, and the option.
    fn boxes() -> [OptionBox; 6] {
        [
            (
                NEXT_AFTER_MOVE_ID,
                "Select the next stack that waits for orders, after a move and as a turn starts",
                |o| &mut o.next_after_move,
            ),
            (QUICK_BATTLES_ID, "Tell a battle in a line, not played out (the log can play it)", |o| {
                &mut o.quick_battles
            }),
            (FAST_BATTLES_ID, "Play battles out fast (twice the pace)", |o| &mut o.fast_battles),
            (SHOW_MOVES_ID, "Show moves as they happen, and the computer's an order at a time", |o| &mut o.show_moves),
            (ALWAYS_BLESS_ID, "On a temple, seek its blessing without asking", |o| &mut o.always_bless),
            (ASK_END_TURN_ID, "Ask before ending a turn with stacks or cities that need orders", |o| {
                &mut o.ask_end_turn
            }),
        ]
    }
}

/// The window of options: a box for each, ticked if it is on.
pub struct OptionsWindow {
    parts: WindowParts,
    /// The boxes, in the order of `Options::boxes`.
    boxes: Vec<WidgetId>,
}

impl OptionsWindow {
    /// Open it on `parent` (the root window), placed in `area` at `align`
    /// (see `build::place`), its boxes as `options` are.
    pub fn open(ui: &mut Ui, parent: WidgetId, options: Options, (area, align): (Rect, (f32, f32))) -> OptionsWindow {
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        let parts = build::window(ui, parent, OPTIONS_WINDOW_ID, "Options");
        build::text(ui, parts.content, "What the game does for you as a turn goes.");
        build::gap(ui, parts.content, space);
        let column = build::column(ui, parts.content, space / 2);
        let mut boxes = Vec::new();
        let mut options = options;
        for (id, words, option) in Options::boxes() {
            let resource = format!("TBClickLabel: text: \"{words}\"\n\tTBCheckBox: id: {id}, is-focusable: 0\n");
            if ui.inflate(column, &resource).is_err() {
                continue;
            }
            let Some(check) = ui.find(column, TbId::new(id)) else { continue };
            ui.set_value(check, i32::from(*option(&mut options)));
            boxes.push(check);
        }
        build::gap(ui, parts.content, space);
        build::button(ui, parts.content, OPTIONS_CLOSE_ID, "Close");
        build::place(ui, parts, area, align);
        OptionsWindow { parts, boxes }
    }

    /// Its window (the session closes the one on top first).
    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    /// True while it is open (its close button removes it).
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// A click on one of its boxes (the box has turned itself on or off):
    /// the options as its boxes say now. `None` if it was none of them.
    pub fn click(&self, ui: &Ui, id: TbId, options: Options) -> Option<Options> {
        let i = Options::boxes().iter().position(|(box_id, _, _)| id == TbId::new(box_id))?;
        let check = *self.boxes.get(i)?;
        let mut options = options;
        let (_, _, option) = Options::boxes()[i];
        *option(&mut options) = ui.value(check).as_int() != 0;
        Some(options)
    }
}
