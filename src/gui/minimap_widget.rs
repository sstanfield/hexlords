//! The minimap's widget: layout and focus. Like `MapWidget` it draws nothing;
//! the host draws the minimap into its rectangle. Its input does nothing yet: it
//! takes clicks and the map keys so that they do not reach anything else.
//!
//! C++: src/gui/minimapwidget.cpp MiniMapWidget

use turbo_ferret::{Ctx, Event, EventKind, Widget};

use super::map_widget::is_map_key;

#[derive(Default)]
pub struct MiniMapWidget;

impl Widget for MiniMapWidget {
    fn type_name(&self) -> &'static str {
        "MiniMapWidget"
    }

    /// C++: MiniMapWidget::OnEvent
    fn on_event(&mut self, _cx: &mut Ctx, ev: &Event) -> bool {
        match ev.kind {
            EventKind::Click => true,
            EventKind::KeyDown | EventKind::KeyUp => is_map_key(ev.special),
            _ => false,
        }
    }
}
