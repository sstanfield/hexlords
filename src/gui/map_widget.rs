//! The main map's widget: layout, focus and input. It draws nothing (the host
//! draws the map into its rectangle before the UI is painted) and holds no game
//! state: it records what the player did, and the session applies it.
//!
//! C++: src/gui/mapwidget.cpp MapWidget, src/gui/glwidget.cpp GLWidget

use turbo_ferret::{Ctx, Event, EventKind, Point, Size, SpecialKey, Widget};

/// A key the map acts on.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MapKey {
    Left,
    Right,
    Up,
    Down,
    /// Enter: choose the hex under the cursor.
    Enter,
    /// `1`
    ZoomOut,
    /// `2`
    ZoomIn,
}

/// Something the player did on the map. Points are in pixels from the top left
/// corner of the map area (the widget inside its padding), whose size comes
/// along, so that the receiver can turn them into hexes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MapCommand {
    /// The pointer moved over the map.
    Hover(Point, Size),
    /// A click (press and release) on the map.
    Click(Point, Size),
    Key(MapKey),
}

#[derive(Default)]
pub struct MapWidget {
    commands: Vec<MapCommand>,
}

impl MapWidget {
    /// The commands recorded since the last call, oldest first.
    pub fn take_commands(&mut self) -> Vec<MapCommand> {
        std::mem::take(&mut self.commands)
    }

    /// An event's position relative to the map area, and that area's size.
    fn map_point(cx: &Ctx, ev: &Event) -> (Point, Size) {
        let area = cx.padding_rect(cx.this());
        (ev.pos - area.origin(), area.size())
    }
}

impl Widget for MapWidget {
    fn type_name(&self) -> &'static str {
        "MapWidget"
    }

    /// C++: MapWidget::OnEvent. It consumes what the C++ consumed, and also the
    /// `1` and `2` presses, which the C++ acted on but let through (deviation D15).
    fn on_event(&mut self, cx: &mut Ctx, ev: &Event) -> bool {
        match ev.kind {
            EventKind::PointerMove => {
                let (p, size) = Self::map_point(cx, ev);
                self.commands.push(MapCommand::Hover(p, size));
                false
            }
            EventKind::Click => {
                let (p, size) = Self::map_point(cx, ev);
                self.commands.push(MapCommand::Click(p, size));
                true
            }
            EventKind::KeyDown => {
                let key = match (ev.special, ev.key) {
                    (SpecialKey::Left, _) => MapKey::Left,
                    (SpecialKey::Right, _) => MapKey::Right,
                    (SpecialKey::Up, _) => MapKey::Up,
                    (SpecialKey::Down, _) => MapKey::Down,
                    (SpecialKey::Enter, _) => MapKey::Enter,
                    (SpecialKey::Esc, _) => return true,
                    (SpecialKey::None, Some('1')) => MapKey::ZoomOut,
                    (SpecialKey::None, Some('2')) => MapKey::ZoomIn,
                    _ => return false,
                };
                self.commands.push(MapCommand::Key(key));
                true
            }
            // The releases of the keys used on the way down.
            EventKind::KeyUp => is_map_key(ev.special),
            _ => false,
        }
    }
}

/// The special keys the map widgets take: the arrows, Esc and Enter.
pub(crate) fn is_map_key(key: SpecialKey) -> bool {
    use SpecialKey as S;
    matches!(key, S::Left | S::Up | S::Right | S::Down | S::Esc | S::Enter)
}
