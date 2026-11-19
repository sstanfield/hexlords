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
//! - [`render`]: drawing with miniquad; the geometry is built without a GPU.
//! - [`screen`]: the UI, a map and its views together, without a window: what
//!   the game and the tools share (tests drive it).
//! - [`session`]: the game on a map screen.
//! - [`script`]: scripted input, for screenshots and tests.
//! - [`app`]: the window (a miniquad `EventHandler`) that runs a scene.

// `unsafe` is for calling C or Objective-C where miniquad leaves something out,
// and only in `platform` (today: `platform::bring_to_front` on macOS).
#![deny(unsafe_code)]

pub mod app;
pub mod game;
pub mod gui;
pub mod info;
pub mod minimap_view;
pub mod paths;
pub mod platform;
pub mod render;
pub mod screen;
pub mod script;
pub mod session;
pub mod state;
pub mod view;
