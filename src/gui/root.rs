//! The window-wide keys (quit and fullscreen), and whatever else no widget
//! wanted: clicks on buttons and key presses, which the host may give a meaning.
//!
//! C++: src/gui/root_widget.cpp RootWidget

use turbo_ferret::{Ctx, Event, EventKind, Modifiers, SpecialKey, TbId, Widget};

/// What arrived at the root window.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RootCommand {
    Quit,
    ToggleFullscreen,
    /// A click on the widget with this id that nothing consumed: a button.
    Click(TbId),
    /// A character key went down and nothing consumed it. With the map focused
    /// these are the keys the map does not use; a text field keeps what is
    /// typed into it.
    Key(char, Modifiers),
    /// The same for a key without a character (Enter in a text field, say).
    Special(SpecialKey, Modifiers),
}

/// The widget that covers the window and holds everything else (id `RootWindow`).
/// Events nothing below it wanted arrive here; the host takes the commands they
/// stand for with [`RootWindow::take_commands`].
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
    fn on_event(&mut self, cx: &mut Ctx, ev: &Event) -> bool {
        if ev.kind == EventKind::Click {
            if let Some(id) = cx.node(ev.target).map(|n| n.id()).filter(|id| *id != TbId::NONE) {
                self.commands.push(RootCommand::Click(id));
            }
            return false;
        }
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
            // Recorded for the host, and still "not handled" for whoever asks.
            Some(_) => {
                self.commands.extend(ev.key.map(|ch| RootCommand::Key(ch, ev.mods)));
                false
            }
            None => {
                if ev.special != SpecialKey::None {
                    self.commands.push(RootCommand::Special(ev.special, ev.mods));
                }
                false
            }
        }
    }
}
