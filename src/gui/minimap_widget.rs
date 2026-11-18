//! The minimap's widget: layout and input. Like `MapWidget` it draws nothing
//! (the host draws the minimap into its rectangle) and holds no game state: a
//! press or a drag on it records where the player pointed, and the session moves
//! the main map there.
//!
//! C++: src/gui/minimapwidget.cpp MiniMapWidget (whose input did nothing)

use turbo_ferret::{Ctx, Event, EventKind, Point, Size, Widget};

/// Something the player did on the minimap.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MiniMapCommand {
    /// Look at this point of the minimap: a press, or a drag with the button
    /// held. The point is in pixels from the top left corner of the minimap area
    /// (the widget inside its padding), whose size comes along. During a drag it
    /// may be outside the area.
    Goto(Point, Size),
}

#[derive(Default)]
pub struct MiniMapWidget {
    commands: Vec<MiniMapCommand>,
    /// The button is down on the minimap: pointer moves are a drag.
    pressed: bool,
}

impl MiniMapWidget {
    /// The commands recorded since the last call, oldest first.
    pub fn take_commands(&mut self) -> Vec<MiniMapCommand> {
        std::mem::take(&mut self.commands)
    }

    fn goto(&mut self, cx: &Ctx, ev: &Event) {
        let area = cx.padding_rect(cx.this());
        self.commands.push(MiniMapCommand::Goto(ev.pos - area.origin(), area.size()));
    }
}

impl Widget for MiniMapWidget {
    fn type_name(&self) -> &'static str {
        "MiniMapWidget"
    }

    fn on_event(&mut self, cx: &mut Ctx, ev: &Event) -> bool {
        match ev.kind {
            EventKind::PointerDown => {
                self.pressed = true;
                self.goto(cx, ev);
                true
            }
            // While the button is held the widget has the pointer, wherever it goes.
            EventKind::PointerMove if self.pressed => {
                self.goto(cx, ev);
                true
            }
            EventKind::PointerUp => {
                self.pressed = false;
                true
            }
            EventKind::Click => true,
            _ => false,
        }
    }

    /// The press ended some other way (the window lost the focus, say).
    fn on_capture_changed(&mut self, _cx: &mut Ctx, captured: bool) {
        if !captured {
            self.pressed = false;
        }
    }
}
