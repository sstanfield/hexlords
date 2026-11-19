//! A terrain as a tile image, and the tiles together as an atlas.
//!
//! The image is square and the flat-topped hex fills it: its points are at
//! the middle of the left and the right edge, its flat edges run along the
//! top and the bottom from a quarter to three quarters of the width. Row 0 is
//! the top of the hex. This is the game's hex model (2 wide, 2 high) with
//! `u = (x + 1) / 2` and `v = (1 - y) / 2`.

use crate::ASPECT;
use crate::scatter::Features;
use crate::shapes::Rgb;
use crate::sides::Side;
use crate::terrain::{Terrain, city};

/// What is outside the hex in a tile's image.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Edge {
    /// The terrain goes on to the image's corners, and the image has no
    /// transparency. The game's tiles are made like this: it scales them
    /// smoothly (mipmaps), which mixes a pixel with its neighbours, and next
    /// to the hex's rim those must not be nothing.
    Filled,
    /// Nothing: transparent, but for one pixel of terrain past the rim. A
    /// tile to look at, or for a renderer that takes single pixels.
    Cutout,
}

/// The smallest and the largest tile, in pixels a side.
pub const SIZES: std::ops::RangeInclusive<usize> = 8..=2048;

/// How far a point of the hex model is from the hex's middle, as a share of
/// the way to its rim: 1 on the rim.
pub fn hex_distance(x: f32, y: f32) -> f32 {
    y.abs().max(x.abs() + y.abs() * 0.5)
}

/// Draw a terrain as a `size` x `size` image: straight-alpha RGBA, the top
/// row first. Every pixel is the mean of a grid of samples (6 x 6 up to 64
/// pixels, 4 x 4 above).
pub fn render(terrain: Terrain, size: usize, edge: Edge, features: &Features) -> Vec<u8> {
    render_with(size, edge, &|x, y| terrain.shade(x, y, features))
}

/// A side's city as a tile, its roofs in the side's color (see [`render`]).
pub fn render_city(side: &Side, size: usize, edge: Edge, features: &Features) -> Vec<u8> {
    render_with(size, edge, &|x, y| city(x, y, features, side.color))
}

/// Draw a picture given by its color at each point of screen space (see
/// [`render`]).
fn render_with(size: usize, edge: Edge, shade: &dyn Fn(f32, f32) -> Rgb) -> Vec<u8> {
    let samples = if size <= 64 { 6 } else { 4 };
    // A pixel's width in the hex model, which is 2 wide.
    let pixel = 2.0 / size as f32;
    let mut out = vec![0u8; size * size * 4];
    for py in 0..size {
        for px in 0..size {
            let mut sum = [0f32; 3];
            let mut covered = 0;
            for sy in 0..samples {
                for sx in 0..samples {
                    let x = (px as f32 + (sx as f32 + 0.5) / samples as f32) * pixel - 1.0;
                    let y = (py as f32 + (sy as f32 + 0.5) / samples as f32) * pixel - 1.0;
                    // A pixel past the rim is drawn, so that the rim itself
                    // never shows what is behind the tile.
                    if edge == Edge::Cutout && hex_distance(x, y) > 1.0 + pixel {
                        continue;
                    }
                    // The model is 2 high; on the screen that is 2 x ASPECT.
                    let color = shade(x, y * ASPECT);
                    for (total, part) in sum.iter_mut().zip(color) {
                        *total += part;
                    }
                    covered += 1;
                }
            }
            if covered > 0 {
                let o = (py * size + px) * 4;
                for (byte, total) in out[o..o + 3].iter_mut().zip(sum) {
                    *byte = ((total / covered as f32).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                out[o + 3] = ((covered * 255 + samples * samples / 2) / (samples * samples)) as u8;
            }
        }
    }
    out
}

/// Every terrain's tile, in the order of [`Terrain::ALL`].
pub fn render_all(size: usize, edge: Edge, features: &Features) -> Vec<Vec<u8>> {
    Terrain::ALL.into_iter().map(|terrain| render(terrain, size, edge, features)).collect()
}

/// The transparent pixels around each tile of an atlas.
pub const ATLAS_PADDING: usize = 2;
/// The tiles in a row of an atlas.
pub const ATLAS_COLUMNS: usize = 5;

/// All the tiles in one image, and where each one is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Atlas {
    pub w: usize,
    pub h: usize,
    /// Straight-alpha RGBA, the top row first.
    pub rgba: Vec<u8>,
    /// A line for the image and one for each tile: `name letter x y w h u0
    /// v0 u1 v1`, the place in pixels and as shares of the image.
    pub text: String,
}

/// Put tiles of `size` pixels (in the order of [`Terrain::ALL`]) into a grid
/// five wide, with [`ATLAS_PADDING`] transparent pixels around each.
/// `file_name` is the image's name in the text's first line.
pub fn atlas(tiles: &[Vec<u8>], size: usize, file_name: &str) -> Atlas {
    let cell = size + 2 * ATLAS_PADDING;
    let rows = tiles.len().div_ceil(ATLAS_COLUMNS);
    let (w, h) = (ATLAS_COLUMNS * cell, rows * cell);
    let mut rgba = vec![0u8; w * h * 4];
    let mut text = format!("# {file_name} {w}x{h}: name letter x y w h u0 v0 u1 v1\n");
    for (i, (tile, terrain)) in tiles.iter().zip(Terrain::ALL).enumerate() {
        let (x0, y0) = ((i % ATLAS_COLUMNS) * cell + ATLAS_PADDING, (i / ATLAS_COLUMNS) * cell + ATLAS_PADDING);
        for (y, row) in tile.chunks_exact(size * 4).enumerate() {
            let o = ((y0 + y) * w + x0) * 4;
            rgba[o..o + size * 4].copy_from_slice(row);
        }
        text.push_str(&format!(
            "{} {} {} {} {} {} {:.6} {:.6} {:.6} {:.6}\n",
            terrain.name(),
            terrain.letter(),
            x0,
            y0,
            size,
            size,
            x0 as f32 / w as f32,
            y0 as f32 / h as f32,
            (x0 + size) as f32 / w as f32,
            (y0 + size) as f32 / h as f32
        ));
    }
    Atlas { w, h, rgba, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(tile: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let o = (y * size + x) * 4;
        [tile[o], tile[o + 1], tile[o + 2], tile[o + 3]]
    }

    #[test]
    fn the_hexs_rim_is_one_away() {
        assert_eq!(hex_distance(0.0, 0.0), 0.0);
        // The six corners and the middles of the flat edges.
        for (x, y) in
            [(1.0, 0.0), (-1.0, 0.0), (0.5, 1.0), (-0.5, 1.0), (0.5, -1.0), (-0.5, -1.0), (0.0, 1.0), (0.0, -1.0)]
        {
            assert_eq!(hex_distance(x, y), 1.0, "({x}, {y})");
        }
        // The middle of a slanted edge, and the square's corner outside the hex.
        assert_eq!(hex_distance(0.75, 0.5), 1.0);
        assert_eq!(hex_distance(1.0, 1.0), 1.5);
    }

    #[test]
    fn a_cutout_tile_is_a_hex_on_nothing() {
        let features = Features::new();
        let size = 32;
        let tile = render(Terrain::Water, size, Edge::Cutout, &features);
        assert_eq!(tile.len(), size * size * 4);
        // The square's corners are outside the hex.
        for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1), (2, 6), (size - 3, size - 7)] {
            assert_eq!(pixel(&tile, size, x, y), [0, 0, 0, 0], "({x}, {y})");
        }
        // The hex is opaque from point to point and from flat edge to flat edge.
        for x in 0..size {
            assert_eq!(pixel(&tile, size, x, size / 2)[3], 255, "column {x} of the middle row");
        }
        for y in 0..size {
            assert_eq!(pixel(&tile, size, size / 2, y)[3], 255, "row {y} of the middle column");
        }
        for x in size / 4..size * 3 / 4 {
            assert_eq!(pixel(&tile, size, x, 0)[3], 255, "the top edge at {x}");
            assert_eq!(pixel(&tile, size, x, size - 1)[3], 255, "the bottom edge at {x}");
        }
        // It is water: blue.
        let [r, g, b, _] = pixel(&tile, size, size / 2, size / 2);
        assert!(b > r && b > g, "({r}, {g}, {b})");
    }

    #[test]
    fn a_filled_tile_is_the_cutout_with_its_corners_painted() {
        let features = Features::new();
        let size = 32;
        for terrain in [Terrain::Forest, Terrain::City] {
            let cutout = render(terrain, size, Edge::Cutout, &features);
            let filled = render(terrain, size, Edge::Filled, &features);
            let mut inside = 0;
            for (c, f) in cutout.chunks_exact(4).zip(filled.chunks_exact(4)) {
                assert_eq!(f[3], 255, "a filled tile has no transparency");
                if c[3] == 255 {
                    assert_eq!(c, f, "inside the hex the two are one picture");
                    inside += 1;
                }
            }
            // The hex is three quarters of its square.
            assert!((size * size * 3 / 4..size * size * 9 / 10).contains(&inside), "{inside}");
            // The corners carry the terrain on, not black.
            assert!(pixel(&filled, size, 0, 0)[1] > 30, "{:?}", pixel(&filled, size, 0, 0));
        }
    }

    #[test]
    fn a_filled_tile_runs_on_across_its_corners() {
        // The corner of the square outside the hex is the neighbour's ground:
        // the pixel in the top left corner is the one half a tile right of and
        // below... rather, the tile's top left corner (model -1, -1) is the hex
        // to the upper left's point (0.5, 0), a hex step of (-1.5, -1) away.
        let features = Features::new();
        let size = 64;
        let tile = render(Terrain::Desert, size, Edge::Filled, &features);
        // A step of (-1.5, -1) in the model is (-48, -32) pixels at 64.
        let mut worst = 0;
        for (x, y) in [(1, 1), (3, 2), (5, 0), (0, 6), (8, 3)] {
            let (here, there) = (pixel(&tile, size, x, y), pixel(&tile, size, x + 48, y + 32));
            worst = worst.max((0..3).map(|k| here[k].abs_diff(there[k])).max().unwrap_or(0));
        }
        assert!(worst <= 3, "the corner and the neighbour's ground differ by {worst}");
    }

    #[test]
    fn every_terrain_has_its_own_tile_and_small_ones_are_drawn_finer() {
        let features = Features::new();
        let tiles = render_all(16, Edge::Filled, &features);
        assert_eq!(tiles.len(), 10);
        for i in 0..tiles.len() {
            assert_eq!(tiles[i].len(), 16 * 16 * 4);
            for j in i + 1..tiles.len() {
                assert_ne!(tiles[i], tiles[j], "{:?} and {:?}", Terrain::ALL[i], Terrain::ALL[j]);
            }
        }
        // The same every time.
        assert_eq!(tiles, render_all(16, Edge::Filled, &features));
    }

    #[test]
    fn an_atlas_has_the_tiles_five_in_a_row_with_room_between() {
        let features = Features::new();
        let size = 16;
        let tiles = render_all(size, Edge::Cutout, &features);
        let atlas = atlas(&tiles, size, "atlas.png");
        assert_eq!((atlas.w, atlas.h), (100, 40));
        assert_eq!(atlas.rgba.len(), 100 * 40 * 4);
        let lines: Vec<&str> = atlas.text.lines().collect();
        assert_eq!(lines.len(), 11);
        assert_eq!(lines[0], "# atlas.png 100x40: name letter x y w h u0 v0 u1 v1");
        assert_eq!(lines[1], "grass g 2 2 16 16 0.020000 0.050000 0.180000 0.450000");
        assert_eq!(lines[6], "mountain m 2 22 16 16 0.020000 0.550000 0.180000 0.950000");
        assert_eq!(lines[10], "ruin r 82 22 16 16 0.820000 0.550000 0.980000 0.950000");
        // Each tile is where its line says, and the padding is nothing.
        for (i, tile) in tiles.iter().enumerate() {
            let (x0, y0) = ((i % 5) * 20 + 2, (i / 5) * 20 + 2);
            for y in 0..size {
                for x in 0..size {
                    assert_eq!(pixel(&atlas.rgba, 100, x0 + x, y0 + y), pixel(tile, size, x, y));
                }
            }
            for k in 0..20 {
                assert_eq!(pixel(&atlas.rgba, 100, x0 - 2 + k, y0 - 1), [0, 0, 0, 0]);
                assert_eq!(pixel(&atlas.rgba, 100, x0 - 1, y0 - 2 + k), [0, 0, 0, 0]);
            }
        }
    }
}
