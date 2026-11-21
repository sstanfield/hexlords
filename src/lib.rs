//! HexGame: a hex map strategy game foundation.
//!
//! This is the Rust port of the hexGame engine demo
//! (<https://github.com/sstanfield/hexGame>, C++). The UI is Turbo Ferret and the
//! graphics go through miniquad. Ported code names where it came from in a
//! comment: `// C++: src/state/map.cpp Map::placeUnit`.
//!
//! - [`state`]: the map, units, cities and movement. No UI, no graphics.
//! - [`game`]: the state plus what the player has selected, whose turn it is,
//!   and the rules that carry out [`orders`]; [`setup`], a game before it
//!   starts.
//! - [`view`], [`minimap_view`]: the pure math of what the map widgets show.
//! - [`gui`]: the game's Turbo Ferret widgets.
//! - [`render`]: drawing with miniquad; the geometry is built without a GPU.
//! - [`screen`]: the UI, a map and its views together, without a window: what
//!   the game and the tools share (tests drive it).
//! - [`help`]: what the help window says (every window has one, on F1).
//! - [`session`]: the game on a map screen; [`city_windows`], its windows
//!   about cities.
//! - [`new_game`]: the screen that sets a game up; [`shell`], the window's
//!   scene from the one to the other.
//! - [`script`]: scripted input, for screenshots and tests.
//! - [`agents`]: the computer's players, which also play without a window
//!   ([`headless`]).
//! - [`app`]: the window (a miniquad `EventHandler`) that runs a scene.
//! - [`mapgen`]: the random map generator, pure (`tools/mapgen` is its
//!   command line).

// `unsafe` is for calling C or Objective-C where miniquad leaves something out,
// and only in `platform` (today: `platform::bring_to_front` on macOS).
#![deny(unsafe_code)]

pub mod agents;
pub mod app;
pub mod battle_window;
pub mod city_windows;
pub mod game;
pub mod gui;
pub mod guide;
pub mod headless;
pub mod help;
pub mod info;
pub mod log_window;
pub mod mapgen;
pub mod minimap_view;
pub mod new_game;
pub mod orders;
pub mod paths;
pub mod planner;
pub mod platform;
pub mod render;
pub mod screen;
pub mod script;
pub mod session;
pub mod setup;
pub mod shell;
pub mod state;
pub mod tasks_window;
pub mod temple_window;
pub mod view;
