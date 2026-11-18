//! HexGame: a hex map strategy game foundation.
//!
//! This is the Rust port of the hexGame engine demo
//! (<https://github.com/sstanfield/hexGame>, C++). The UI is Turbo Ferret and the
//! graphics go through miniquad. Ported code names where it came from in a
//! comment: `// C++: src/state/map.cpp Map::placeUnit`.
//!
//! - [`state`]: the map, units, cities and movement. No UI, no graphics.
//! - [`game`]: the state plus what the player has selected.
//! - [`view`], [`minimap_view`]: the pure math of what the map widgets show.
//! - [`gui`]: the game's Turbo Ferret widgets.
//! - [`session`]: the UI and the game together, without a window (tests drive it).
//! - [`app`]: the window (a miniquad `EventHandler`).

#![forbid(unsafe_code)]

pub mod app;
pub mod game;
pub mod gui;
pub mod minimap_view;
pub mod paths;
pub mod session;
pub mod state;
pub mod view;
