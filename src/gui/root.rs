//! The window-wide keys: quit and fullscreen.
//!
//! C++: src/gui/root_widget.cpp RootWidget

use turbo_ferret::{Ctx, Event, EventKind, Modifiers, SpecialKey, Widget};

/// What a root key asks the host to do.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootCommand {
    Quit,
    ToggleFullscreen,
}

/// The widget that covers the window and holds everything else (id `RootWindow`).
/// Key presses nothing below it wanted arrive here; the host takes the commands
/// they stand for with [`RootWindow::take_commands`].
#[derive(Default)]
pub struct RootWindow {
    commands: Vec<RootCommand>,
}

impl RootWindow {
    /// The commands recorded since the last call, oldest first.
    pub fn take_commands(&mut self) -> Vec<RootCommand> {
        std::mem::take(&mut self.commands)
    }
}

impl Widget for RootWindow {
    fn type_name(&self) -> &'static str {
        "RootWindow"
    }

    /// C++: RootWidget::OnEvent. Three changes: plain `f` and `q` are not swallowed
    /// (deviation D9), Cmd+Q quits as well as Ctrl+Q (D16), and F11 asks for a
    /// real fullscreen switch where the C++ called a stub (D17).
    fn on_event(&mut self, _cx: &mut Ctx, ev: &Event) -> bool {
        if ev.kind != EventKind::KeyDown {
            return false;
        }
        if ev.special == SpecialKey::F11 {
            self.commands.push(RootCommand::ToggleFullscreen);
            return true;
        }
        match ev.key.map(|c| c.to_ascii_lowercase()) {
            // C++: `nextFullscreen`, a stub that does nothing.
            Some('f') if ev.mods.contains(Modifiers::ALT) => true,
            Some('q') if ev.mods.intersects(Modifiers::CTRL | Modifiers::SUPER) => {
                self.commands.push(RootCommand::Quit);
                true
            }
            _ => false,
        }
    }
}
