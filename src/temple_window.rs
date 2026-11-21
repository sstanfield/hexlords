//! A temple's window (the owner's rules, 2026-11-20). When a person's stack
//! goes onto a temple that has not blessed all of it (or the stack there is
//! sent with B, or Seek a blessing), it asks, for the whole stack, whether
//! it seeks the temple's blessing: a line for each unit, not yet blessed
//! here or blessed here before, and what seeking it costs (those blessed
//! stop for the turn; the others keep their moves). Then it says what each
//! was given (`Order::Bless`), or that it was blessed here before.
//!
//! Not a port: the C++ had no temples.

use turbo_ferret::{Rect, Ui, WidgetId};

use crate::gui::build::{self, WindowParts};
use crate::orders::Event;
use crate::state::{Hex, Map, Side, TileType, UnitId};

pub const TEMPLE_WINDOW_ID: &str = "temple_window";
pub const TEMPLE_BLESS_ID: &str = "temple_bless";
pub const TEMPLE_NOT_NOW_ID: &str = "temple_not_now";
pub const TEMPLE_CLOSE_ID: &str = "temple_close";
const SPACING_DP: i32 = 8;

/// The units of `side` on `at`, if it is a temple that has not blessed all
/// of them: the stack that may seek its blessing.
pub fn seekers(map: &Map, side: Side, at: Hex) -> Option<Vec<UnitId>> {
    if !map.tile(at).is_some_and(|t| t.kind == TileType::Temple) {
        return None;
    }
    let units: Vec<UnitId> = map.units_at(at).iter().copied().filter(|id| id.side == side).collect();
    let unblessed = units.iter().any(|id| map.units().get(*id).is_some_and(|u| !u.blessed_at(at)));
    (unblessed && !units.is_empty()).then_some(units)
}

/// The window: asking, or telling what came of it.
pub struct TempleWindow {
    parts: WindowParts,
    /// The temple, and the stack it asks about (empty once it has told).
    pub at: Hex,
    pub units: Vec<UnitId>,
    /// What it says of each unit, a line each.
    pub lines: Vec<String>,
}

impl TempleWindow {
    /// Ask whether the stack `units` on the temple `at` seeks its blessing.
    pub fn ask(ui: &mut Ui, (root, area): (WidgetId, Rect), map: &Map, at: Hex, units: &[UnitId]) -> TempleWindow {
        let lines: Vec<String> = units
            .iter()
            .filter_map(|id| map.units().get(*id))
            .map(|u| {
                let state = if u.blessed_at(at) { "blessed here before" } else { "not yet blessed here" };
                format!("{}: {state}", u.title())
            })
            .collect();
        let parts = build::window(ui, root, TEMPLE_WINDOW_ID, "A temple");
        build::heading(ui, parts.content, "Seek the temple's blessing?");
        build::text(ui, parts.content, "Each it has not blessed is blessed, and stops here for the turn;");
        build::text(ui, parts.content, "each it blessed before keeps its moves.");
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        build::gap(ui, parts.content, space);
        for line in &lines {
            build::text(ui, parts.content, line);
        }
        build::gap(ui, parts.content, space * 2);
        let buttons = build::row(ui, parts.content, space * 2);
        build::button(ui, buttons, TEMPLE_BLESS_ID, "Seek the blessing");
        build::button(ui, buttons, TEMPLE_NOT_NOW_ID, "Not now");
        build::place(ui, parts, area, (0.5, 0.08));
        TempleWindow { parts, at, units: units.to_vec(), lines }
    }

    /// Say what the blessing gave each of `units` (`events`: what the order
    /// did), or that the temple blessed it before.
    pub fn tell(
        ui: &mut Ui,
        (root, area): (WidgetId, Rect),
        map: &Map,
        (at, units): (Hex, &[UnitId]),
        events: &[Event],
    ) -> TempleWindow {
        let lines: Vec<String> = units
            .iter()
            .filter_map(|id| map.units().get(*id))
            .map(|u| {
                let given = events.iter().find_map(|e| match e {
                    Event::Blessed { unit, blessing, .. } if *unit == u.id() => Some(*blessing),
                    _ => None,
                });
                match given {
                    Some(blessing) => format!("{}: {}", u.title(), blessing.words()),
                    None => format!("{}: blessed here before; it keeps its moves", u.title()),
                }
            })
            .collect();
        let parts = build::window(ui, root, TEMPLE_WINDOW_ID, "A temple");
        build::heading(ui, parts.content, "The temple's blessing");
        let space = ui.dimension_converter().dp_to_px(SPACING_DP);
        build::gap(ui, parts.content, space);
        for line in &lines {
            build::text(ui, parts.content, line);
        }
        build::gap(ui, parts.content, space * 2);
        build::button(ui, parts.content, TEMPLE_CLOSE_ID, "Close");
        build::place(ui, parts, area, (0.5, 0.08));
        TempleWindow { parts, at, units: Vec::new(), lines }
    }

    pub fn window(&self) -> WidgetId {
        self.parts.window
    }

    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    /// True while it asks (it has not told yet).
    pub fn asks(&self) -> bool {
        !self.units.is_empty()
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }
}
