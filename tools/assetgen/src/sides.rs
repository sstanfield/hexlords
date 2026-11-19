//! The sides of the game, as far as the pictures go: a name for file names
//! and a color. A city is drawn once for each, its roofs in the side's color
//! (`terrain::city`). These are the game's `hexgame::render::side_color` and
//! `scenario::side_name`; a test holds them together.

use crate::shapes::Rgb;

/// A side: its name (`neutral` for no side's) and its color.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Side {
    pub name: &'static str,
    pub color: Rgb,
}

/// No side's: the cities nobody holds.
pub const NEUTRAL: Side = Side { name: "neutral", color: [0.55, 0.55, 0.55] };

/// The eight sides that can hold a city, in the game's order.
pub const PLAYERS: [Side; 8] = [
    Side { name: "black", color: [0.14, 0.14, 0.16] },
    Side { name: "white", color: [0.93, 0.93, 0.90] },
    Side { name: "red", color: [0.78, 0.17, 0.13] },
    Side { name: "green", color: [0.17, 0.55, 0.25] },
    Side { name: "blue", color: [0.18, 0.35, 0.78] },
    Side { name: "orange", color: [0.91, 0.50, 0.12] },
    Side { name: "yellow", color: [0.93, 0.80, 0.20] },
    Side { name: "purple", color: [0.52, 0.25, 0.64] },
];

/// The file a side's city is drawn into: `city_tile.png` for nobody's (the
/// terrain's own tile), `city_<side>_tile.png` for a side's.
pub fn city_file(side: &Side) -> String {
    if side.name == NEUTRAL.name { "city_tile.png".to_string() } else { format!("city_{}_tile.png", side.name) }
}
