//! Maps drawn with the tiles, to look at the art without the game: a mixed
//! map close up, a made-up continent from far away, and every terrain in a
//! block without borders, where a seam between two tiles would show.
//!
//! A map here is its rows of letters, as in a map file
//! (`docs/map-format.md`), and it is drawn the way the game draws one: odd
//! columns half a hex higher, y at [`ASPECT`] of x, a black border around
//! each hex.

use crate::ASPECT;
use crate::noise::{plain, unit};
use crate::scatter::Features;
use crate::shapes::length;
use crate::terrain::Terrain;
use crate::tile::{Edge, hex_distance, render_all};

/// A map: its rows, each a row of map file letters.
pub type Letters = Vec<Vec<u8>>;

/// Every terrain's tile at one size, in the order of [`Terrain::ALL`].
pub struct TileSet {
    pub size: usize,
    pub tiles: Vec<Vec<u8>>,
}

/// How a map is drawn.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Shot {
    /// The picture's size in pixels.
    pub w: usize,
    pub h: usize,
    /// Pixels for one unit of the hex model (a hex is 2 units wide).
    pub scale: f32,
    /// The point of the map in the middle of the picture, in model units from
    /// the middle of hex (0, 0): x to the right, y *downward*.
    pub center: (f32, f32),
    /// Draw the game's black border around each hex.
    pub border: bool,
}

/// Behind the map, and behind a tile's transparent pixels: the game's grey.
const BACKGROUND: u8 = 128;
/// The game's border starts here on the way from a hex's middle to its rim.
const BORDER_FROM: f32 = 0.95;

/// Draw a map: opaque RGBA, the top row first. Each pixel takes the one pixel
/// of the tile it falls on.
pub fn draw_map(map: &[Vec<u8>], set: &TileSet, shot: Shot) -> Vec<u8> {
    let Shot { w, h, scale, center, border } = shot;
    let size = set.size;
    let mut out = vec![0u8; w * h * 4];
    for py in 0..h {
        for px in 0..w {
            let mx = center.0 + (px as f32 + 0.5 - w as f32 / 2.0) / scale;
            let my = center.1 + (py as f32 + 0.5 - h as f32 / 2.0) / (scale * ASPECT);
            // The hex the point is in: the nearest of three columns' hexes.
            let nearest = (mx / 1.5).round() as i32;
            let mut best = (f32::MAX, 0, 0, 0.0, 0.0);
            for col in nearest - 1..=nearest + 1 {
                let odd = col.rem_euclid(2) as f32;
                let row = ((my + odd) / 2.0).round() as i32;
                let (lx, ly) = (mx - 1.5 * col as f32, my - (2.0 * row as f32 - odd));
                let d = hex_distance(lx, ly);
                if d < best.0 {
                    best = (d, row, col, lx, ly);
                }
            }
            let (d, row, col, lx, ly) = best;
            let mut color = [BACKGROUND; 3];
            let on_map = row >= 0 && col >= 0 && (row as usize) < map.len() && (col as usize) < map[0].len();
            if on_map {
                let letter = map[row as usize][col as usize];
                let terrain = Terrain::from_letter(letter as char).unwrap_or(Terrain::Grass);
                let tx = (((lx + 1.0) / 2.0 * size as f32) as usize).min(size - 1);
                let ty = (((ly + 1.0) / 2.0 * size as f32) as usize).min(size - 1);
                let texel = &set.tiles[terrain.index()][(ty * size + tx) * 4..];
                let alpha = u32::from(texel[3]);
                for (out, &part) in color.iter_mut().zip(texel) {
                    *out = ((u32::from(part) * alpha + u32::from(BACKGROUND) * (255 - alpha)) / 255) as u8;
                }
                if border && d > BORDER_FROM {
                    color = [0, 0, 0];
                }
            }
            let o = (py * w + px) * 4;
            out[o..o + 3].copy_from_slice(&color);
            out[o + 3] = 255;
        }
    }
    out
}

/// Make up a continent of `rows` x `cols` hexes: land in the middle of a sea,
/// with ridges of hills and mountains, forest and swamp where it is wet,
/// desert where it is dry, and cities, temples and ruins on the grass, well
/// apart.
pub fn continent(rows: usize, cols: usize, seed: u32) -> Letters {
    let mut map = vec![vec![b'w'; cols]; rows];
    // A hex's middle on the screen.
    let center = |r: usize, c: usize| (1.5 * c as f32, (2.0 * r as f32 - (c % 2) as f32) * ASPECT);
    let (width, height) = center(rows - 1, cols - 1);
    for (r, row) in map.iter_mut().enumerate() {
        for (c, tile) in row.iter_mut().enumerate() {
            let (x, y) = center(r, c);
            let (nx, ny) = (x / width - 0.5, y / height - 0.5);
            // How high the ground is: noise, falling off toward the map's edges.
            let elevation = plain(x * 0.06, y * 0.06, seed) - (nx * nx + ny * ny) * 1.3;
            let ridge = 1.0 - (plain(x * 0.05, y * 0.05, seed + 10) - 0.5).abs() * 2.0;
            let wet = plain(x * 0.08, y * 0.08, seed + 20);
            *tile = if elevation < 0.30 {
                b'w'
            } else if ridge > 0.93 && elevation > 0.36 {
                b'm'
            } else if ridge > 0.84 && elevation > 0.33 {
                b'h'
            } else if wet > 0.60 {
                b'f'
            } else if wet < 0.36 {
                b'd'
            } else if wet > 0.555 && elevation < 0.36 {
                b's'
            } else {
                b'g'
            };
        }
    }
    // The places: on grass, away from the map's edge and from each other.
    let mut placed: Vec<(f32, f32)> = Vec::new();
    for (letter, count, gap) in [(b'c', 9, 13.0f32), (b't', 3, 9.0), (b'r', 4, 9.0)] {
        let mut n = 0;
        for i in 0..20000 {
            if n == count {
                break;
            }
            let r = (unit(i, 0, seed + u32::from(letter)) * rows as f32) as usize;
            let c = (unit(i, 1, seed + u32::from(letter)) * cols as f32) as usize;
            if map[r][c] != b'g' || r < 2 || c < 2 || r + 2 >= rows || c + 2 >= cols {
                continue;
            }
            let p = center(r, c);
            if placed.iter().any(|q| length(p.0 - q.0, p.1 - q.1) < gap) {
                continue;
            }
            map[r][c] = letter;
            placed.push(p);
            n += 1;
        }
    }
    map
}

/// A map as a map file's text.
pub fn map_text(map: &[Vec<u8>]) -> String {
    let mut text = format!("{}\n{}\n", map.len(), map.first().map_or(0, Vec::len));
    for row in map {
        text.extend(row.iter().map(|&letter| letter as char));
        text.push('\n');
    }
    text
}

/// The point in the middle of a map (see [`Shot::center`]).
pub fn middle(map: &[Vec<u8>]) -> (f32, f32) {
    (1.5 * (map[0].len() as f32 - 1.0) / 2.0, map.len() as f32 - 1.5)
}

/// A picture to look at.
pub struct Picture {
    /// The file's name.
    pub name: &'static str,
    pub w: usize,
    pub h: usize,
    /// Opaque RGBA, the top row first.
    pub rgba: Vec<u8>,
}

/// The preview pictures, and the maps they show as map files.
pub struct Previews {
    pub pictures: Vec<Picture>,
    /// A file's name and its text.
    pub maps: Vec<(&'static str, String)>,
}

/// The mixed map of the near previews: every terrain, in patches.
const NEAR: [&str; 9] = [
    "wwwwwggffffhhm",
    "wwwwgggfffhhmm",
    "wwwggcggffhmmm",
    "wwgggggtghhmmh",
    "wwsssgggddhhhg",
    "wwssggrgdddcgg",
    "wwwsggggddddgg",
    "wwwwgggdddgggf",
    "wwwwwwggddggff",
];

/// The size of the far preview's continent, and its seed.
pub const FAR: (usize, usize, u32) = (46, 72, 7);

/// Draw the previews. They take single pixels from cutout tiles, so each is
/// drawn with tiles of about the size a hex has in it: 128 pixels near, 256
/// in the close-up, 32 far away, 64 for the seams.
pub fn previews(features: &Features) -> Previews {
    let set = |size: usize| TileSet { size, tiles: render_all(size, Edge::Cutout, features) };
    let (tiny, small, large, huge) = (set(32), set(64), set(128), set(256));
    let picture = |name, map: &[Vec<u8>], set: &TileSet, shot: Shot| Picture {
        name,
        w: shot.w,
        h: shot.h,
        rgba: draw_map(map, set, shot),
    };

    let near: Letters = NEAR.iter().map(|row| row.as_bytes().to_vec()).collect();
    let (rows, cols, seed) = FAR;
    let far = continent(rows, cols, seed);
    // Each terrain in a block of 4 x 4 hexes.
    let seams: Letters =
        (0..8).map(|r| (0..20).map(|c| Terrain::ALL[(r / 4) * 5 + c / 4].letter() as u8).collect()).collect();

    let (w, h) = (1024, 768);
    // About 10 hexes across the window, as at the game's closest zoom.
    let close = Shot { w, h, scale: 68.0, center: middle(&near), border: true };
    // About 68 hexes across, as at its farthest.
    let wide = Shot { w, h, scale: 1024.0 / (68.0 * 1.5), center: middle(&far), border: true };
    let pictures = vec![
        picture("near.png", &near, &large, close),
        picture("near_no_border.png", &near, &large, Shot { border: false, ..close }),
        // The largest tiles, a pixel for a pixel.
        picture("closeup.png", &near, &huge, Shot { scale: huge.size as f32 / 2.0, center: (11.0, 4.5), ..close }),
        picture("far.png", &far, &tiny, wide),
        picture("seams.png", &seams, &small, Shot { w, h: 480, scale: 34.0, center: middle(&seams), border: false }),
    ];
    Previews { pictures, maps: vec![("far.map", map_text(&far))] }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiles of one flat color each, to tell them apart in a picture: tile i
    /// is (10 * i, 0, 200), opaque.
    fn flat_set(size: usize) -> TileSet {
        let tiles = (0..10).map(|i| [10 * i as u8, 0, 200, 255].repeat(size * size)).collect();
        TileSet { size, tiles }
    }

    fn pixel(rgba: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
        let o = (y * w + x) * 4;
        [rgba[o], rgba[o + 1], rgba[o + 2]]
    }

    #[test]
    fn a_map_is_drawn_as_the_game_lays_it_out() {
        // Two rows of three: grass forest hill / swamp desert mountain.
        let map: Letters = vec![b"gfh".to_vec(), b"sdm".to_vec()];
        let (w, h, scale) = (200, 200, 20.0);
        let shot = Shot { w, h, scale, center: (1.5, 0.5), border: true };
        let picture = draw_map(&map, &flat_set(8), shot);
        assert_eq!(picture.len(), w * h * 4);
        // The middle of hex (row, col) in the picture: odd columns half a hex
        // higher, rows 2 x ASPECT apart.
        let at = |row: usize, col: usize| {
            let (mx, my) = (1.5 * col as f32, 2.0 * row as f32 - (col % 2) as f32);
            ((w as f32 / 2.0 + (mx - 1.5) * scale) as usize, (h as f32 / 2.0 + (my - 0.5) * scale * ASPECT) as usize)
        };
        for (row, col, terrain) in [(0, 0, 0), (0, 1, 1), (0, 2, 2), (1, 0, 3), (1, 1, 4), (1, 2, 5)] {
            let (x, y) = at(row, col);
            assert_eq!(pixel(&picture, w, x, y), [10 * terrain, 0, 200], "hex ({row}, {col})");
        }
        // Between two hexes is the border; off the map, the background.
        let ((x0, y0), (_, y1)) = (at(0, 0), at(1, 0));
        assert_eq!(pixel(&picture, w, x0, (y0 + y1) / 2), [0, 0, 0]);
        assert_eq!(pixel(&picture, w, 2, 2), [128, 128, 128]);
        assert_eq!(pixel(&picture, w, w - 3, h - 3), [128, 128, 128]);
        // Without borders the same place is one of the two hexes.
        let plain = draw_map(&map, &flat_set(8), Shot { border: false, ..shot });
        assert_ne!(pixel(&plain, w, x0, (y0 + y1) / 2), [0, 0, 0]);
    }

    #[test]
    fn a_tiles_transparency_shows_the_background() {
        let mut set = flat_set(8);
        set.tiles[0] = [255, 255, 255, 0].repeat(64);
        let map: Letters = vec![b"g".to_vec()];
        let picture = draw_map(&map, &set, Shot { w: 9, h: 9, scale: 4.0, center: (0.0, 0.0), border: false });
        assert_eq!(pixel(&picture, 9, 4, 4), [128, 128, 128]);
    }

    #[test]
    fn the_continent_is_land_in_a_sea_with_every_terrain() {
        let (rows, cols, seed) = FAR;
        let map = continent(rows, cols, seed);
        assert_eq!((map.len(), map[0].len()), (rows, cols));
        let count = |letter: u8| map.iter().flatten().filter(|&&l| l == letter).count();
        for terrain in Terrain::ALL {
            assert!(count(terrain.letter() as u8) > 0, "no {}", terrain.name());
        }
        assert_eq!(Terrain::ALL.iter().map(|t| count(t.letter() as u8)).sum::<usize>(), rows * cols);
        assert_eq!((count(b'c'), count(b't'), count(b'r')), (9, 3, 4));
        // Sea nearly all around, and a good part of the map is land.
        let edge = map[0].iter().chain(&map[rows - 1]).chain(map.iter().flat_map(|row| [&row[0], &row[cols - 1]]));
        let (sea, around) = edge.fold((0, 0), |(sea, all), &l| (sea + usize::from(l == b'w'), all + 1));
        assert!(sea * 10 >= around * 9, "{sea} of {around} edge hexes are water");
        let land = rows * cols - count(b'w');
        assert!((rows * cols / 5..rows * cols * 3 / 4).contains(&land), "{land}");
        // The same every time, and another with another seed.
        assert_eq!(map, continent(rows, cols, seed));
        assert_ne!(map, continent(rows, cols, seed + 1));
    }

    #[test]
    fn a_maps_text_is_a_map_file() {
        let map: Letters = vec![b"gfh".to_vec(), b"sdm".to_vec()];
        assert_eq!(map_text(&map), "2\n3\ngfh\nsdm\n");
        assert_eq!(middle(&map), (1.5, 0.5));
    }
}
