//! The window that asks how a patrol fights, once its waypoints are picked
//! (P, then Shift+click each waypoint and click the last): never (a scout,
//! in the fog), or at what odds at least it attacks the other sides' stacks
//! it sees (the owner, 2026-11-21: "should it attack, and if so what are its
//! minimum odds"). One of the choices is pressed (at first the one made
//! last time); Patrol (or Enter) gives the order (`Order::Patrol`).
//! (The owner, 2026-11-21: "the patrol dialog probably needs an Ok".)
//!
//! Not a port: the C++ had no orders.

use turbo_ferret::{Rect, TbId, Ui, WidgetId};

use crate::gui::build::{self, WindowParts};
use crate::state::{Hex, Patrol, UnitId};

pub const PATROL_WINDOW_ID: &str = "patrol_window";
pub const PATROL_OK_ID: &str = "patrol_ok";
pub const PATROL_CANCEL_ID: &str = "patrol_cancel";
/// What it offers: never attack, or attack at these odds or better.
pub const CHOICES: [(Option<u32>, &str); 5] = [
    (None, "Never attack: a scout keeps out of fights"),
    (Some(90), "Attack at 90% or better: sure fights only"),
    (Some(75), "Attack at 75% or better"),
    (Some(60), "Attack at 60% or better"),
    (Some(50), "Attack at 50% or better: even fights too"),
];
const SPACING_DP: i32 = 8;

/// The id of the button of the `i`th of [`CHOICES`].
pub fn choice_id(i: usize) -> String {
    format!("patrol_choice_{i}")
}

/// What a click in the window asks for.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PatrolClick {
    /// One of the choices: it is the one pressed now.
    Chose(Option<u32>),
    /// Patrol: the order, with the choice pressed.
    Ok(Option<u32>),
}

/// The window, about to send `units` round `waypoints`; `chosen`, the
/// choice pressed (at first the one made last time).
pub struct PatrolWindow {
    parts: WindowParts,
    pub units: Vec<UnitId>,
    pub waypoints: Vec<Hex>,
    chosen: Option<u32>,
    buttons: Vec<WidgetId>,
}

impl PatrolWindow {
    /// Ask how `units` patrol `waypoints` (`patrol`: as they would), the
    /// choice `last` (the one made last time) pressed.
    pub fn ask(
        ui: &mut Ui,
        (root, area): (WidgetId, Rect),
        (units, waypoints): (&[UnitId], &[Hex]),
        patrol: &Patrol,
        last: Option<u32>,
    ) -> PatrolWindow {
        let parts = build::window(ui, root, PATROL_WINDOW_ID, "Patrol");
        build::heading(ui, parts.content, "Does it attack what it meets?");
        let way = if patrol.round {
            format!("Round and round {} waypoints, turn after turn.", patrol.stops.len())
        } else {
            format!("Back and forth along {} waypoints, turn after turn.", patrol.stops.len())
        };
        build::text(ui, parts.content, &way);
        build::text(ui, parts.content, "If it attacks: the stacks it sees and can reach, once a turn at most.");
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        build::gap(ui, parts.content, space);
        let column = build::column(ui, parts.content, space / 2);
        let mut buttons = Vec::new();
        for (i, (odds, words)) in CHOICES.iter().enumerate() {
            let button = build::button(ui, column, &choice_id(i), words);
            build::text_from_left(ui, button);
            ui.set_group_id(button, TbId::new("patrol_choices"));
            ui.set_value(button, i32::from(*odds == last));
            buttons.push(button);
        }
        build::gap(ui, parts.content, space);
        build::text(ui, parts.content, "After a battle it waits for your word: D lets it go on.");
        build::gap(ui, parts.content, space);
        let row = build::row(ui, parts.content, space);
        build::primary_button(ui, row, PATROL_OK_ID, "Patrol");
        build::button(ui, row, PATROL_CANCEL_ID, "Never mind");
        // As wide as the widest of them.
        let widest = buttons.iter().map(|b| ui.preferred_size_free(*b).pref.w).max().unwrap_or(0);
        for button in &buttons {
            build::set_width(ui, *button, widest);
        }
        build::place(ui, parts, area, (0.5, 0.25));
        PatrolWindow { parts, units: units.to_vec(), waypoints: waypoints.to_vec(), chosen: last, buttons }
    }

    /// The choice pressed now (the odds; `None`: never attack): what
    /// Patrol, or Enter, orders.
    pub fn chosen(&self) -> Option<u32> {
        self.chosen
    }

    /// True if `id` is one of its choices, or Patrol.
    pub fn owns(&self, id: TbId) -> bool {
        id == TbId::new(PATROL_OK_ID) || (0..CHOICES.len()).any(|i| id == TbId::new(&choice_id(i)))
    }

    /// What a click asks for, if it was on one of its choices or Patrol; a
    /// choice is the one pressed from then on.
    pub fn click(&mut self, ui: &mut Ui, id: TbId) -> Option<PatrolClick> {
        if id == TbId::new(PATROL_OK_ID) {
            return Some(PatrolClick::Ok(self.chosen));
        }
        let i = (0..CHOICES.len()).find(|i| id == TbId::new(&choice_id(*i)))?;
        self.chosen = CHOICES[i].0;
        for (j, button) in self.buttons.iter().enumerate() {
            ui.set_value(*button, i32::from(j == i));
        }
        Some(PatrolClick::Chose(self.chosen))
    }

    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }
}
