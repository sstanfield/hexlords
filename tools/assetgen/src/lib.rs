//! The HexGame tile generator: draws the terrain tiles the game shows on its
//! map. Everything is computed (noise, shapes, a few lights), so the art is
//! original, has no licence but the game's, and can be made again at any size:
//!
//! ```sh
//! cargo run -p assetgen --release                 # assets/tiles, as the game ships them
//! cargo run -p assetgen --release -- --previews target/previews
//! ```
//!
//! `docs/tiles.md` says how a tile lies in its texture and how to change the
//! art. In short: the texture is square and the flat-topped hex fills it, its
//! points at the middle of the left and right edges; the game draws y at
//! 0.863 of x, so the art is drawn as it will look on the screen and stretched
//! upward into the texture; and every terrain repeats on the hex grid, so a
//! tile runs on into its neighbours of the same terrain without a seam.
//!
//! - [`png`]: the PNG writer.
//! - [`noise`]: hashes and noise that repeats on the hex grid.
//! - [`shapes`]: colors, distances to shapes, light.
//! - [`scatter`]: the trees, peaks, reeds ... strewn over a tile.
//! - [`terrain`]: the ten terrains, one function each.
//! - [`tile`]: a terrain as a tile image; the atlas.
//! - [`preview`]: maps drawn with the tiles, to look at them without the game.
//!
//! It began as a prototype outside the repository (a `tiles/assetgen` crate
//! handed over on 2026-11-18); this is that program, arranged as the
//! repository's tools are, and it draws the same pictures.

#![forbid(unsafe_code)]

pub mod noise;
pub mod png;
pub mod preview;
pub mod scatter;
pub mod shapes;
pub mod terrain;
pub mod tile;

/// The game draws y at this much of x, so that hexes look regular (about
/// √3/2). It is `hexgame::view::HEX_HEIGHT_ADJUST`; a test holds them together.
#[allow(clippy::excessive_precision)] // the game's constant, digit for digit
pub const ASPECT: f32 = 0.863_309_35;
