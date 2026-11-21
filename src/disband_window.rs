//! The window that asks before units are disbanded (the owner, 2026-11-20:
//! "Add a disband option to remove units"; armies need it to upgrade). It
//! names each of the selected units and its upkeep, and Disband sends them
//! home for good (`Order::Disband`).
//!
//! Not a port: the C++ had no upkeep.

use turbo_ferret::{Rect, Ui, WidgetId};

use crate::gui::build::{self, WindowParts};
use crate::state::{Map, UnitId};

pub const DISBAND_WINDOW_ID: &str = "disband_window";
pub const DISBAND_YES_ID: &str = "disband_yes";
pub const DISBAND_NO_ID: &str = "disband_no";
const SPACING_DP: i32 = 8;

/// The window, asking about `units`.
pub struct DisbandWindow {
    parts: WindowParts,
    pub units: Vec<UnitId>,
    /// What it says of each unit, a line each.
    pub lines: Vec<String>,
}

impl DisbandWindow {
    /// Ask whether `units` go home for good.
    pub fn ask(ui: &mut Ui, (root, area): (WidgetId, Rect), map: &Map, units: &[UnitId]) -> DisbandWindow {
        let all = map.units();
        let lines: Vec<String> = units
            .iter()
            .filter_map(|id| all.get(*id))
            .map(|u| format!("{}: upkeep {} a turn", u.title(), u.upkeep))
            .collect();
        let upkeep: u32 = units.iter().filter_map(|id| all.get(*id)).map(|u| u.upkeep).sum();
        let parts = build::window(ui, root, DISBAND_WINDOW_ID, "Disband");
        let one = units.len() == 1;
        build::heading(ui, parts.content, if one { "Disband this unit?" } else { "Disband these units?" });
        let gone = if one { "It goes home for good" } else { "They go home for good" };
        build::text(ui, parts.content, &format!("{gone}, and {upkeep} gold a turn of upkeep with them."));
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        build::gap(ui, parts.content, space);
        for line in &lines {
            build::text(ui, parts.content, line);
        }
        build::gap(ui, parts.content, space * 2);
        let buttons = build::row(ui, parts.content, space * 2);
        build::button(ui, buttons, DISBAND_YES_ID, "Disband");
        build::button(ui, buttons, DISBAND_NO_ID, "Keep them");
        build::place(ui, parts, area, (0.5, 0.08));
        DisbandWindow { parts, units: units.to_vec(), lines }
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
