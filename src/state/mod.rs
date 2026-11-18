//! The game state: map, units, cities and movement. No UI and no graphics here,
//! so all of it is tested without a window.
//!
//! C++: src/state/

pub mod cities;
pub mod hex;
pub mod map;
pub mod movement;
pub mod terrain;
pub mod units;

pub use cities::{Cities, City, CityId, UnitProd};
pub use hex::Hex;
pub use map::{MAX_COLS, MAX_ROWS, MAX_STACK, MIN_COLS, MIN_ROWS, Map, MapError, PlaceError, Tile};
pub use movement::{MoveRange, compute_range};
pub use terrain::{MoveCost, TileType};
pub use units::{MoveCostId, NUM_SIDES, Side, Unit, UnitId, Units};
