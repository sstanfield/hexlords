//! The game state: map, units, cities and movement. No UI and no graphics here,
//! so all of it is tested without a window.
//!
//! C++: src/state/

pub mod cities;
pub mod combat;
pub mod hex;
pub mod map;
pub mod movement;
pub mod rng;
pub mod scenario;
pub mod terrain;
pub mod units;

pub use cities::{Cities, City, CityId, UnitProd, WALL_PRICE, WALLS_CAP, wall_price};
pub use hex::{Direction, Hex, HexSide};
pub use map::{MAX_COLS, MAX_ROWS, MAX_STACK, MIN_COLS, MIN_ROWS, Map, MapError, PlaceError, Tile};
pub use movement::{
    ATTACK_COST, MoveRange, Passage, attack_or_step, compute_range, hostile, passage, path_with, reach_with, step_cost,
    step_toward, step_toward_by, step_toward_with, way_over_land,
};
pub use rng::Rng;
pub use scenario::{Scenario, ScenarioError};
pub use terrain::{MoveCost, TileType};
pub use units::{
    Blessing, DEFAULT_UNIT_IMAGE, LEVEL_EXPERIENCE, MAX_LEVEL, MoveCostId, NUM_SIDES, Side, Travel, Unit, UnitId,
    UnitKind, UnitKindId, Units, command_bonus, level_for,
};
