//! The Hexlords map editor: paint terrain on a hex map and save it as a map file
//! (`docs/map-format.md`). It is the game's main map and minimap
//! (`hexlords::screen`) with a sidebar of Turbo Ferret widgets.
//!
//! - [`editor`]: the editor without its window (tests drive it).
//! - [`brush`]: which hexes a stroke touches.
//! - [`history`]: undo and redo.
//! - [`check`]: `--check`, a report on a map file.

#![forbid(unsafe_code)]

pub mod brush;
pub mod check;
pub mod editor;
pub mod history;
