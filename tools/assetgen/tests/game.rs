//! The generator and the game it draws for agree: on the terrains, on how a
//! hex lies in its tile, on the map file, and on the tiles in `assets/tiles`,
//! which are the generator's output and nothing else.

use std::path::PathBuf;

use assetgen::ASPECT;
use assetgen::icons::{self, Icon};
use assetgen::overlay;
use assetgen::png::encode_rgba;
use assetgen::preview::{FAR, continent, map_text};
use assetgen::road;
use assetgen::scatter::Features;
use assetgen::terrain::Terrain;
use assetgen::tile::{Edge, hex_distance, render, render_all};
use hexgame::render::{HexVariant, roads, tile_image};
use hexgame::state::{Map, TileType};
use turbo_ferret::image::{Limits, png};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The game's terrain for one of the generator's.
fn game_terrain(terrain: Terrain) -> TileType {
    TileType::from_char(terrain.letter()).unwrap_or_else(|| panic!("the game has no letter {}", terrain.letter()))
}

#[test]
fn the_terrains_are_the_games() {
    assert_eq!(Terrain::ALL.len(), TileType::ALL.len());
    let mut seen = Vec::new();
    for terrain in Terrain::ALL {
        let kind = game_terrain(terrain);
        assert_eq!(terrain.name(), kind.name());
        assert_eq!(terrain.letter(), kind.to_char());
        assert!(!seen.contains(&kind));
        seen.push(kind);
    }
}

#[test]
fn the_squeeze_is_the_games() {
    assert_eq!(ASPECT, hexgame::view::HEX_HEIGHT_ADJUST);
}

#[test]
fn a_tile_lies_in_its_image_as_the_games_hex_mesh_reads_it() {
    // The mesh's corners, in the hex model (y up) and in the texture.
    let (positions, uvs) = (HexVariant::Full.positions(), HexVariant::Full.uvs());
    let size = 64;
    let tile = render(Terrain::Water, size, Edge::Cutout, &Features::new());
    let alpha_at = |u: f32, v: f32| {
        let (x, y) = (((u * size as f32) as usize).min(size - 1), ((v * size as f32) as usize).min(size - 1));
        tile[(y * size + x) * 4 + 3]
    };
    for (pos, uv) in positions.iter().zip(uvs) {
        // u runs with x, v against y: the image's first row is the hex's top.
        assert_eq!(*uv, [(pos[0] + 1.0) / 2.0, (1.0 - pos[1]) / 2.0], "the corner at {pos:?}");
        // The corners are on the tile's rim (the center is its middle) ...
        let rim = hex_distance(uv[0] * 2.0 - 1.0, uv[1] * 2.0 - 1.0);
        assert!(rim == 0.0 || rim == 1.0, "{pos:?}: {rim}");
        // ... and a little inside each, the cutout tile has terrain.
        let (u, v) = (0.5 + (uv[0] - 0.5) * 0.97, 0.5 + (uv[1] - 0.5) * 0.97);
        assert_eq!(alpha_at(u, v), 255, "the corner at {pos:?}");
    }
    // The texture's own corners are outside the hex.
    assert_eq!(alpha_at(0.02, 0.02), 0);
    assert_eq!(alpha_at(0.98, 0.98), 0);
}

#[test]
fn the_games_tiles_are_what_the_generator_makes() {
    // `cargo run -p assetgen --release` wrote assets/tiles; whoever changes
    // the art runs it again. The numbers come out of sines and square roots,
    // which differ in their last digit from one system to the next, so a
    // channel may be off by a hair.
    let features = Features::new();
    let tiles = render_all(256, Edge::Filled, &features);
    for (terrain, made) in Terrain::ALL.into_iter().zip(&tiles) {
        let path = assets().join(tile_image(game_terrain(terrain)));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let image = png::decode(&bytes, &Limits::DEFAULT).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!((image.size.w, image.size.h), (256, 256), "{}", path.display());
        let worst = image.rgba.iter().zip(made).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        let off = image.rgba.iter().zip(made).filter(|(a, b)| a != b).count();
        assert!(
            worst <= 2 && off <= made.len() / 100,
            "{} is not the generator's tile ({off} bytes differ, by up to {worst}): run `cargo run -p assetgen --release`",
            path.display()
        );
    }
    // No stray pictures: the directory holds the ten tiles.
    let mut files: Vec<String> = std::fs::read_dir(assets().join("tiles"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut expected: Vec<String> = Terrain::ALL.iter().map(|t| format!("{}_tile.png", t.name())).collect();
    expected.sort();
    assert_eq!(files, expected);
}

#[test]
fn the_game_reads_the_generators_pngs() {
    let size = 32;
    let tile = render(Terrain::Temple, size, Edge::Cutout, &Features::new());
    let bytes = encode_rgba(size, size, &tile).unwrap();
    let image = png::decode(&bytes, &Limits::DEFAULT).unwrap();
    assert_eq!((image.size.w, image.size.h), (size as i32, size as i32));
    assert_eq!(image.rgba, tile);
}

#[test]
fn the_previews_continent_is_a_map_the_game_reads() {
    let (rows, cols, seed) = FAR;
    let text = map_text(&continent(rows, cols, seed));
    let map = Map::parse(&text).unwrap();
    assert_eq!((usize::from(map.rows()), usize::from(map.cols())), (rows, cols));
    // And writes back as it was.
    assert_eq!(map.to_text(), text);
}

/// Compare a picture the generator makes with its file in the game's assets.
fn same_as_file(path: &str, size: usize, made: &[u8]) {
    let file = assets().join(path);
    let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let image = png::decode(&bytes, &Limits::DEFAULT).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert_eq!((image.size.w, image.size.h), (size as i32, size as i32), "{path}");
    let worst = image.rgba.iter().zip(made).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    let off = image.rgba.iter().zip(made).filter(|(a, b)| a != b).count();
    assert!(
        worst <= 2 && off <= made.len() / 100,
        "{path} is not the generator's picture ({off} bytes differ, by up to {worst}): run `cargo run -p assetgen --release`"
    );
}

/// The files of one of the game's asset directories, sorted.
fn files_in(dir: &str) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(assets().join(dir))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

#[test]
fn the_games_roads_are_what_the_generator_makes() {
    let mut expected = Vec::new();
    for shape in road::shapes() {
        let land = overlay::render(256, road::ROAD_RIM, &road::road(shape));
        same_as_file(&format!("roads/{}", road::file_name(false, shape)), 256, &land);
        let water = overlay::render(256, road::BRIDGE_RIM, &road::bridge(shape));
        same_as_file(&format!("roads/{}", road::file_name(true, shape)), 256, &water);
        expected.extend([road::file_name(false, shape), road::file_name(true, shape)]);
    }
    expected.sort();
    assert_eq!(files_in("roads"), expected);
}

#[test]
fn the_games_units_signs_are_what_the_generator_makes() {
    let mut expected = vec!["default_unit.png".to_string()]; // the C++ demo's, for a kind with no picture
    for icon in Icon::ALL {
        let picture = overlay::render(128, icons::OUTLINE, &|x, y| icon.ink(x, y));
        same_as_file(&format!("units/{}.png", icon.name()), 128, &picture);
        expected.push(format!("{}.png", icon.name()));
    }
    expected.sort();
    assert_eq!(files_in("units"), expected);
}

#[test]
fn the_game_turns_a_roads_picture_as_the_generator_drew_it() {
    // The same shape and the same turn for every one of the 64 masks ...
    assert_eq!(road::shapes(), roads::ROAD_SHAPES);
    for mask in 0..64u8 {
        let (shape, turns) = road::shape_of(mask);
        let (place, game_turns) = roads::shape_of(mask);
        assert_eq!((roads::ROAD_SHAPES[place], game_turns), (shape, turns), "mask {mask}");
        for over_water in [false, true] {
            let file = format!("roads/{}", road::file_name(over_water, shape));
            assert_eq!(roads::road_image(roads::picture(place, over_water)), file);
        }
    }
    // ... and the same place in the picture for every point of the hex.
    for turns in 0..6u8 {
        for (x, y) in [(0.0, 0.0), (0.3, -0.7), (-0.9, 0.05), (0.5, 1.0), (0.75, 0.5)] {
            let (tx, ty) = road::turn(x, y, turns);
            let uv = roads::turned_uv([x, y], turns);
            assert!(
                (uv[0] - (tx + 1.0) / 2.0).abs() < 1e-6 && (uv[1] - (1.0 - ty) / 2.0).abs() < 1e-6,
                "({x}, {y}), {turns}"
            );
        }
    }
    // The sides are numbered alike: clockwise from the top.
    assert_eq!(hexgame::state::HexSide::ALL.len(), road::SIDES);
    assert_eq!(hexgame::state::HexSide::NorthEast.index(), 1);
}

#[test]
fn a_road_drawn_on_the_games_map_leaves_through_the_sides_the_map_says() {
    // A road hex with roads to its north-east and its south: the picture the
    // game picks, read where the game reads it, has road at the middle of
    // those two sides and nowhere else.
    use hexgame::state::{Hex, HexSide, Map};
    let mut map = Map::new(20, 20, TileType::Grass).unwrap();
    let hex = Hex::new(10, 10);
    for side in [HexSide::NorthEast, HexSide::South] {
        let (row, col) = hex.toward(side);
        map.tile_mut(Hex::new(row as u16, col as u16)).unwrap().road = true;
    }
    map.tile_mut(hex).unwrap().road = true;
    let links = map.road_links(hex);
    assert_eq!(links, 0b001010);
    let (place, turns) = roads::shape_of(links);
    let size = 128;
    let picture = overlay::render(size, road::ROAD_RIM, &road::road(roads::ROAD_SHAPES[place]));
    let alpha_at = |uv: [f32; 2]| {
        let (x, y) = (((uv[0] * size as f32) as usize).min(size - 1), ((uv[1] * size as f32) as usize).min(size - 1));
        picture[(y * size + x) * 4 + 3]
    };
    // The middles of the sides in the hex model, clockwise from the top,
    // pulled in a little from the rim.
    let middles = [[0.0, 1.0], [0.75, 0.5], [0.75, -0.5], [0.0, -1.0], [-0.75, -0.5], [-0.75, 0.5]];
    for side in HexSide::ALL {
        let [x, y] = middles[side.index()];
        let alpha = alpha_at(roads::turned_uv([x * 0.94, y * 0.94], turns));
        let leaves = links & (1 << side.index()) != 0;
        assert_eq!(alpha == 255, leaves, "{side:?}: opacity {alpha}");
    }
}
