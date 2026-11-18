//! The game's own Turbo Ferret widgets.
//!
//! None of them holds game state or draws the map. Turbo Ferret creates widgets
//! from layout files through factories that are plain `fn() -> Box<dyn Widget>`,
//! so a widget cannot be handed the game: instead it records commands, and the
//! session takes them after each input event and applies them.
//!
//! C++: src/gui/

pub mod map_widget;
pub mod minimap_widget;
pub mod root;

use turbo_ferret::Ui;

pub use map_widget::{MapCommand, MapKey, MapWidget};
pub use minimap_widget::{MiniMapCommand, MiniMapWidget};
pub use root::{RootCommand, RootWindow};

/// Make the game's widgets known to layout files, under the names the C++ used.
/// C++: TB_WIDGET_FACTORY(MapWidget, ...), TB_WIDGET_FACTORY(MiniMapWidget, ...)
pub fn register(ui: &mut Ui) {
    ui.register::<MapWidget>("MapWidget");
    ui.register::<MiniMapWidget>("MiniMapWidget");
}
