//! HexGame: a hex map strategy game foundation.
//!
//! This is the Rust port of the hexGame engine demo
//! (<https://github.com/sstanfield/hexGame>, C++). The UI is Turbo Ferret and the
//! graphics go through miniquad. Ported code names where it came from in a
//! comment: `// C++: src/state/map.cpp Map::placeUnit`.
//!
//! - [`gui`]: the game's Turbo Ferret widgets.
//! - [`session`]: the UI and the game together, without a window (tests drive it).
//! - [`app`]: the window (a miniquad `EventHandler`).

#![forbid(unsafe_code)]

pub mod app;
pub mod gui;
pub mod paths;
pub mod session;
