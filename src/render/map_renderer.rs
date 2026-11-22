//! The main map: the visible hexes with their terrain textures and borders,
//! the roads over them, the units (a disc of their side's color and their
//! kind's picture), the cities (each drawn with its owner's picture, roofs in
//! the owner's color, and a pip for each unit in it: a city shows itself, not
//! a unit), and the overlay on the hexes the caller marks (in the game: where
//! the selected unit can move).
//!
//! The geometry is built on the CPU each frame (`MapGeometry`, which needs no
//! GPU and is tested without one) and drawn in layers: the terrain, one mesh
//! per texture; the roads; the discs; the units; the overlay. The C++ drew hex
//! by hex, each with up to three draw calls; hexes do not overlap, so the
//! layers look the same.
//!
//! C++: src/render/maprender.cpp MapRenderer (renderMap, CTX::drawHex,
//! CTX::getTileTexture)
//!
//! Every terrain has its own tile image (the C++ had seven for ten terrains).

use std::collections::HashMap;

use miniquad::{RenderingBackend, TextureId};

use super::hex_mesh::{HEX_BARY, HEX_INDICES, HEX_VERTICES, HexVariant};
use super::mesh::{BufferPool, MeshList};
use super::roads::{self, ROAD_PICTURES};
use super::shaders::{FlatVertex, HexVertex, Pipelines};
use super::textures::{AssetDirs, Sampling};
use crate::game::{Ghost, SEE_ALL, Sight};
use crate::state::scenario::side_name;
use crate::state::{DEFAULT_UNIT_IMAGE, Hex, Map, Side, TileType, UnitId};
use crate::view::{HEX_HEIGHT_ADJUST, MapView};

/// The border of the hex under the cursor. C++: drawHex (`isSelected`)
pub const CURSOR_BORDER: [f32; 3] = [1.0, 0.0, 0.0];
/// The border of every other hex.
pub const HEX_BORDER: [f32; 3] = [0.0, 0.0, 0.0];
/// The border of a hex the caller outlines (in the game: the stops of a
/// route), as thick as the cursor's.
pub const STOP_BORDER: [f32; 3] = [1.0, 0.82, 0.2];
/// The border of the hex the selected stack stands on (the owner's word,
/// 2026-11-22: "outline the selected stack/hex (could be a city) in
/// green"), of the hexes of the way it is to take, and of the hex a battle
/// is fought on.
pub const SELECTED_BORDER: [f32; 3] = [0.25, 1.0, 0.35];
pub const OWN_WAY_BORDER: [f32; 3] = [1.0, 1.0, 1.0];
pub const BATTLE_BORDER: [f32; 3] = [1.0, 0.35, 0.0];
/// The tint over the hex a battle is fought on, at its brightest.
pub const BATTLE_FLASH: [f32; 4] = [1.0, 0.1, 0.0, 0.55];
/// Laid over the hexes a selected unit can move to. C++: HexOverlayFragment.glsl
pub const RANGE_OVERLAY: [f32; 4] = [1.0, 0.0, 0.0, 0.3];
/// Drawn for a unit whose kind has no picture of its own.
pub const UNIT_IMAGE: &str = DEFAULT_UNIT_IMAGE;
/// A border that starts past the rim: none. Roads and units are drawn over
/// the terrain's.
const NO_BORDER: [f32; 4] = [0.0, 0.0, 0.0, 2.0];
/// The disc under a unit: how far it reaches from the hex's middle (a hex is
/// 1 from its middle to its points), and the dark ring around it.
pub const DISC_RADIUS: f32 = 0.5;
pub const DISC_RING: f32 = 0.05;
pub const RING_COLOR: [f32; 4] = [0.08, 0.07, 0.07, 1.0];
/// A veteran's ring, and an elite unit's (or a captain's of level 2 and 3):
/// silver and gold, and wider.
pub const VETERAN_RING: [f32; 4] = [0.80, 0.82, 0.86, 1.0];
pub const ELITE_RING: [f32; 4] = [0.96, 0.76, 0.22, 1.0];
pub const LEVEL_RING: f32 = 0.09;

/// The ring around a unit of a level: its color and how wide it is.
pub fn level_ring(level: u8) -> ([f32; 4], f32) {
    match level {
        0 | 1 => (RING_COLOR, DISC_RING),
        2 => (VETERAN_RING, LEVEL_RING),
        _ => (ELITE_RING, LEVEL_RING),
    }
}
/// A city shows its own picture, not a unit: its garrison is a pip for each
/// unit, in its side's color, in rows over the gate, the first unit at the
/// left of the bottom row. `pip_at` places them; a stack in the field says
/// how many it is so too.
const PIPS_PER_ROW: usize = 5;
pub const PIP_ROWS: usize = 2;
// Room for a full stack and two commanders.
const _: () = assert!(PIPS_PER_ROW * PIP_ROWS >= crate::state::MAX_STACK + 2);
const PIP_RADIUS: f32 = 0.075;
const PIP_RING: f32 = 0.03;
const PIP_STEP: f32 = 0.22;
const PIP_BOTTOM: f32 = -0.78;
const PIP_ROW_STEP: f32 = 0.24;
/// The ring of the selected unit's pip (the color of a pressed button's text).
pub const SELECTED_RING: [f32; 4] = [1.0, 0.82, 0.29, 1.0];
/// The ring of the pip of a commander in one of its side's cities.
pub const LEADER_RING: [f32; 4] = [0.96, 0.96, 0.92, 1.0];
/// The corners a disc is drawn with.
const DISC_CORNERS: usize = 24;

/// A side's color: on the discs under its units and its cities' banners.
pub fn side_color(side: Side) -> [f32; 3] {
    match side {
        Side::Black => [0.14, 0.14, 0.16],
        Side::White => [0.93, 0.93, 0.90],
        Side::Red => [0.78, 0.17, 0.13],
        Side::Green => [0.17, 0.55, 0.25],
        Side::Blue => [0.18, 0.35, 0.78],
        Side::Orange => [0.91, 0.50, 0.12],
        Side::Yellow => [0.93, 0.80, 0.20],
        Side::Purple => [0.52, 0.25, 0.64],
        Side::Grey => [0.55, 0.55, 0.55],
    }
}

/// The hexes of the map that get a tint laid over them.
pub struct Marks<'a> {
    /// The tint: red, green, blue and how much of it.
    pub color: [f32; 4],
    pub marked: &'a dyn Fn(Hex) -> bool,
    /// The unit that shows on its hex whoever else stands there (the selected
    /// one); in a city, the unit whose pip is ringed. Of every other hex the
    /// first unit shows.
    pub front: Option<UnitId>,
    /// Whose units show what they are (their picture): the others' are
    /// banners, their size and no more. `None`: everyone's show.
    pub viewer: Option<Side>,
    /// What is known of the map (fog of war): the hexes out of sight are
    /// dimmed, with no units on them (another side's stack remembered on one
    /// shows as a ghost of its banner) and their cities in the colors of who
    /// held them when they were seen last; land never seen is black, and
    /// nothing of it shows (`UNKNOWN`).
    pub sight: &'a Sight,
    /// The hexes outlined (`STOP_BORDER`; the cursor's border wins): in the
    /// game, the stops of the route being picked, or of the selected unit's.
    pub outlined: &'a [Hex],
    /// The flags on the map, each with its number (0 to 9): in the game, the
    /// viewer's rally points (`push_flag`).
    pub flags: &'a [(Hex, u8)],
    /// The stacks shown going from hex to hex (moves being shown): drawn
    /// where they are on their way, and not on their hexes.
    pub moving: &'a [Moving],
    /// Hexes ringed in a color of their own, as thick as the cursor's
    /// border, which they win over (as over `outlined`); of two for a hex,
    /// the later. In the game: the selected stack's hex, the way it is to
    /// take, where a battle is.
    pub rings: &'a [(Hex, [f32; 3])],
    /// A hex with a tint of its own over everything on it: in the game,
    /// where a battle is being fought (it flashes).
    pub flash: Option<(Hex, [f32; 4])>,
}

/// A stack shown going from hex to hex: its units, the point of the map it
/// is at (between two hexes' middles, `MapView::hex_center`), and the hex
/// it is nearest (what the fog says of it: another side's shows where the
/// viewer sees).
#[derive(Clone, Debug, PartialEq)]
pub struct Moving {
    pub units: Vec<UnitId>,
    pub at: [f32; 2],
    pub hex: Hex,
}

impl Marks<'_> {
    /// Marks of a map seen whole, everyone's units shown: a tool's.
    pub fn plain(color: [f32; 4], marked: &dyn Fn(Hex) -> bool, front: Option<UnitId>) -> Marks<'_> {
        Marks {
            color,
            marked,
            front,
            viewer: None,
            sight: &SEE_ALL,
            outlined: &[],
            flags: &[],
            moving: &[],
            rings: &[],
            flash: None,
        }
    }
}

/// The dimming over a hex in the fog.
pub const FOG: [f32; 4] = [0.02, 0.02, 0.06, 0.5];
/// A hex never seen (total fog of war): all of it this color.
pub const UNKNOWN: [u8; 3] = [6, 6, 12];
/// How much of a remembered stack shows in the fog (its ghost).
pub const GHOST_ALPHA: f32 = 0.85;

/// The image a terrain is drawn with, relative to the asset directory: every
/// terrain has its own, made by `tools/assetgen` (`docs/tiles.md`). A city's
/// is nobody's city; a side's is [`city_image`].
pub fn tile_image(kind: TileType) -> String {
    format!("tiles/{}_tile.png", kind.name())
}

/// The image a city held by `side` is drawn with: its roofs in the side's
/// color (nobody's: the city terrain's own tile).
pub fn city_image(side: Side) -> String {
    match side {
        Side::Grey => tile_image(TileType::City),
        side => format!("tiles/city_{}_tile.png", side_name(side)),
    }
}

/// Every side a city can belong to, nobody last: the order of the cities'
/// meshes and textures.
pub const CITY_SIDES: [Side; 9] = [
    Side::Black,
    Side::White,
    Side::Red,
    Side::Green,
    Side::Blue,
    Side::Orange,
    Side::Yellow,
    Side::Purple,
    Side::Grey,
];

/// How many terrains, and so tile images, there are.
const TERRAINS: usize = TileType::ALL.len();

/// One frame of the main map as triangles, in the widget's normalized device
/// coordinates.
#[derive(Default)]
pub struct MapGeometry {
    /// The terrain, a mesh for each kind's image (indexed like `TileType::ALL`;
    /// the cities are not here).
    terrain: [MeshList<HexVertex>; TERRAINS],
    /// The cities, a mesh for each owner's picture (indexed by `Side::index`).
    cities: [MeshList<HexVertex>; CITY_SIDES.len()],
    /// The roads, a mesh for each picture (`roads::picture`).
    roads: Vec<MeshList<HexVertex>>,
    /// The discs under the units and the cities' pips.
    discs: MeshList<FlatVertex>,
    /// Over the units: the other sides' stacks as banners, and how many
    /// stand in each stack.
    signs: MeshList<FlatVertex>,
    /// The hexes that hold units, by the image of the unit that is shown.
    units: Vec<(String, MeshList<HexVertex>)>,
    /// The marked hexes.
    overlay: MeshList<FlatVertex>,
    /// The hexes never seen.
    unknown: MeshList<HexVertex>,
    /// What `units_with` gives for an image nothing is drawn with.
    none: MeshList<HexVertex>,
}

/// Where the pip of the `i`th of a garrison of `count` units goes on the city's
/// hex (in the hex model), or `None` past the pips there is room for.
pub fn pip_at(i: usize, count: usize) -> Option<[f32; 2]> {
    if i >= count || i >= PIPS_PER_ROW * PIP_ROWS {
        return None;
    }
    let row = i / PIPS_PER_ROW;
    let in_row = (count - row * PIPS_PER_ROW).min(PIPS_PER_ROW);
    let x = ((i % PIPS_PER_ROW) as f32 - (in_row - 1) as f32 / 2.0) * PIP_STEP;
    Some([x, PIP_BOTTOM + row as f32 * PIP_ROW_STEP])
}

/// A disc of a color around a point of a hex, cut where the hex is cut (at
/// the map's edges). `at` and `radius` are in the hex model, where the disc
/// is as much taller as the screen is squeezed, so that it is round there.
fn push_disc(
    list: &mut MeshList<FlatVertex>,
    view: &MapView,
    hex_center: [f32; 2],
    variant: HexVariant,
    at: [f32; 2],
    radius: f32,
    color: [f32; 4],
) {
    // The part of its square the hex's model covers.
    let positions = variant.positions();
    let bound = |axis: usize| {
        let values = positions.iter().map(|p| p[axis]);
        (values.clone().fold(f32::MAX, f32::min), values.fold(f32::MIN, f32::max))
    };
    let ((left, right), (bottom, top)) = (bound(0), bound(1));
    let place = |x: f32, y: f32| FlatVertex {
        pos: view.to_ndc([hex_center[0] + x.clamp(left, right), hex_center[1] + y.clamp(bottom, top)]),
        color,
    };
    let mut vertices = vec![place(at[0], at[1])];
    let mut indices = Vec::with_capacity(DISC_CORNERS * 3);
    for i in 0..DISC_CORNERS {
        let angle = i as f32 / DISC_CORNERS as f32 * std::f32::consts::TAU;
        vertices.push(place(at[0] + radius * angle.cos(), at[1] + radius * angle.sin() / HEX_HEIGHT_ADJUST));
        let next = (i + 1) % DISC_CORNERS;
        indices.extend([0, i as u16 + 1, next as u16 + 1]);
    }
    list.push(&vertices, &indices);
}

/// A shape of one color in a hex, as a fan from its first corner (which
/// must see every other: the shapes here are such), corners in the hex
/// model, cut where the hex is cut.
fn push_shape(
    list: &mut MeshList<FlatVertex>,
    view: &MapView,
    hex_center: [f32; 2],
    variant: HexVariant,
    corners: &[[f32; 2]],
    color: [f32; 4],
) {
    let positions = variant.positions();
    let bound = |axis: usize| {
        let values = positions.iter().map(|p| p[axis]);
        (values.clone().fold(f32::MAX, f32::min), values.fold(f32::MIN, f32::max))
    };
    let ((left, right), (bottom, top)) = (bound(0), bound(1));
    let vertices: Vec<FlatVertex> = corners
        .iter()
        .map(|[x, y]| FlatVertex {
            pos: view.to_ndc([hex_center[0] + x.clamp(left, right), hex_center[1] + y.clamp(bottom, top)]),
            color,
        })
        .collect();
    let indices: Vec<u16> = (1..corners.len().saturating_sub(1) as u16).flat_map(|i| [0, i, i + 1]).collect();
    list.push(&vertices, &indices);
}

/// Another side's stack: a banner in its color on a pole, its tail cut in
/// two, outlined dark; what is in it is not shown.
fn push_banner(
    list: &mut MeshList<FlatVertex>,
    view: &MapView,
    center: [f32; 2],
    variant: HexVariant,
    color: [f32; 3],
    alpha: f32,
) {
    let [r, g, b] = color;
    let dark = [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2], RING_COLOR[3] * alpha];
    let a = HEX_HEIGHT_ADJUST;
    // The cloth: from the pole to its cut tail; the pole.
    let cloth = |grow: f32| {
        let (x0, x1, y0, y1) = (-0.24 - grow, 0.42 + grow, (0.0 - grow) / a, (0.5 + grow) / a);
        let notch = 0.24 + grow * 0.5;
        [[x0, (y0 + y1) / 2.0], [x0, y0], [x1, y0], [notch, (y0 + y1) / 2.0], [x1, y1], [x0, y1]]
    };
    let pole = |grow: f32| {
        let (x0, x1, y0, y1) = (-0.31 - grow, -0.24 + grow, (-0.5 - grow) / a, (0.55 + grow) / a);
        [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    };
    push_shape(list, view, center, variant, &pole(0.04), dark);
    push_shape(list, view, center, variant, &cloth(0.04), dark);
    push_shape(list, view, center, variant, &pole(0.0), [0.85, 0.80, 0.68, alpha]);
    push_shape(list, view, center, variant, &cloth(0.0), [r, g, b, alpha]);
}

/// The colors of a flag's number (a rally point's): dark on a light cloth,
/// light on a dark one. The cloth is the side's color (`side_color`; the
/// owner's word, 2026-11-21: "in your side's color").
pub const FLAG_NUMBER_DARK: [f32; 4] = [0.12, 0.09, 0.05, 1.0];
pub const FLAG_NUMBER_LIGHT: [f32; 4] = [0.97, 0.95, 0.90, 1.0];

/// The number's color on a cloth of `color`: dark on a light one.
fn flag_number(color: [f32; 3]) -> [f32; 4] {
    let [r, g, b] = color;
    if 0.299 * r + 0.587 * g + 0.114 * b > 0.55 { FLAG_NUMBER_DARK } else { FLAG_NUMBER_LIGHT }
}

/// Which of a digit's seven bars are lit: top, top right, bottom right,
/// bottom, bottom left, top left, middle.
const DIGIT_BARS: [[bool; 7]; 10] = [
    [true, true, true, true, true, true, false],
    [false, true, true, false, false, false, false],
    [true, true, false, true, true, false, true],
    [true, true, true, true, false, false, true],
    [false, true, true, false, false, true, true],
    [true, false, true, true, false, true, true],
    [true, false, true, true, true, true, true],
    [true, true, true, false, false, false, false],
    [true, true, true, true, true, true, true],
    [true, true, true, true, false, true, true],
];

/// A rally point's flag, in the top left of its hex: a pole, and a cloth
/// with its number on it in bars, outlined dark.
fn push_flag(
    list: &mut MeshList<FlatVertex>,
    view: &MapView,
    (center, variant): ([f32; 2], HexVariant),
    n: u8,
    color: [f32; 3],
) {
    let [r, g, b] = color;
    let a = HEX_HEIGHT_ADJUST;
    let rect = |x0: f32, x1: f32, y0: f32, y1: f32| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let (grow, gy) = (0.04, 0.04 / a);
    // The pole, and the cloth from its top.
    let (pole, cloth) = ((-0.56, -0.49, -0.10, 0.92), (-0.49, 0.16, 0.30, 0.92));
    for (x0, x1, y0, y1) in [pole, cloth] {
        push_shape(list, view, center, variant, &rect(x0 - grow, x1 + grow, y0 - gy, y1 + gy), RING_COLOR);
    }
    push_shape(list, view, center, variant, &rect(pole.0, pole.1, pole.2, pole.3), [0.85, 0.80, 0.68, 1.0]);
    push_shape(list, view, center, variant, &rect(cloth.0, cloth.1, cloth.2, cloth.3), [r, g, b, 1.0]);
    // The number, in seven bars.
    let (x0, x1, y0, y1) = (-0.30, -0.02, 0.38, 0.84);
    let (tx, ty) = (0.07, 0.07 / a);
    let ym = (y0 + y1) / 2.0;
    let bars = [
        rect(x0, x1, y1 - ty, y1),
        rect(x1 - tx, x1, ym, y1),
        rect(x1 - tx, x1, y0, ym),
        rect(x0, x1, y0, y0 + ty),
        rect(x0, x0 + tx, y0, ym),
        rect(x0, x0 + tx, ym, y1),
        rect(x0, x1, ym - ty / 2.0, ym + ty / 2.0),
    ];
    let lit = DIGIT_BARS[usize::from(n % 10)];
    for (bar, on) in bars.iter().zip(lit) {
        if on {
            push_shape(list, view, center, variant, bar, flag_number(color));
        }
    }
}

/// A stack's size: a pip for each unit, in its side's color, along the
/// bottom of its hex as a city's garrison is (`pip_at`).
fn push_size(
    list: &mut MeshList<FlatVertex>,
    view: &MapView,
    center: [f32; 2],
    variant: HexVariant,
    count: usize,
    side: Side,
    alpha: f32,
) {
    let [r, g, b] = side_color(side);
    let dark = [RING_COLOR[0], RING_COLOR[1], RING_COLOR[2], RING_COLOR[3] * alpha];
    for at in (0..count).map_while(|i| pip_at(i, count)) {
        push_disc(list, view, center, variant, at, PIP_RADIUS + PIP_RING, dark);
        push_disc(list, view, center, variant, at, PIP_RADIUS, [r, g, b, alpha]);
    }
}

impl MapGeometry {
    pub fn new() -> MapGeometry {
        MapGeometry::default()
    }

    /// Rebuild for the current map and view. `marks` says which hexes get a
    /// tint, and which: where the selected unit can go, in the game.
    /// C++: MapRenderer::renderMap, drawHex
    pub fn build(&mut self, map: &Map, view: &MapView, marks: &Marks) {
        for list in self.terrain.iter_mut().chain(&mut self.cities) {
            list.clear();
        }
        self.roads.resize_with(ROAD_PICTURES, MeshList::new);
        for list in &mut self.roads {
            list.clear();
        }
        self.discs.clear();
        self.signs.clear();
        for (_, list) in &mut self.units {
            list.clear();
        }
        self.overlay.clear();
        self.unknown.clear();

        let (rows, cols) = (map.rows(), map.cols());
        let hidden: Vec<UnitId> = marks.moving.iter().flat_map(|m| m.units.iter().copied()).collect();
        for visible in view.visible_hexes() {
            let hex = visible.hex;
            let Some(tile) = map.tile(hex) else { continue };
            let variant = HexVariant::choose(hex.row, hex.col, rows, cols);
            let (positions, uvs) = (variant.positions(), variant.uvs());
            let is_cursor = hex == map.cursor;
            let outlined = !is_cursor && marks.outlined.contains(&hex);
            let ring = marks.rings.iter().rev().find(|(at, _)| *at == hex).map(|(_, color)| *color);
            let [r, g, b] = match (ring, is_cursor, outlined) {
                (Some(color), _, _) => color,
                (None, true, _) => CURSOR_BORDER,
                (None, false, true) => STOP_BORDER,
                _ => HEX_BORDER,
            };
            let border = [r, g, b, view.border_threshold(ring.is_some() || is_cursor || outlined)];
            let corner =
                |i: usize| view.to_ndc([positions[i][0] + visible.center[0], positions[i][1] + visible.center[1]]);

            let vertices: [HexVertex; HEX_VERTICES] =
                std::array::from_fn(|i| HexVertex { pos: corner(i), uv: uvs[i], bary: HEX_BARY[i], border });
            // Land never seen: black (its border too, but the cursor's), and
            // nothing on it.
            if !marks.sight.knows(hex) {
                self.unknown.push(&vertices, &HEX_INDICES);
                continue;
            }
            // A city is drawn with its owner's picture (in the fog, of who held
            // it when it was seen last).
            let seen = marks.sight.sees(hex);
            let owner = map.city_at(hex).map_or(Side::Grey, |city| marks.sight.holder(city));
            match tile.kind {
                TileType::City => self.cities[owner.index()].push(&vertices, &HEX_INDICES),
                kind => self.terrain[kind.index()].push(&vertices, &HEX_INDICES),
            }

            // The road, over the terrain and its border. The places have a
            // picture of their own in the middle of their tile: roads run up
            // to them and are not painted over them.
            let place = matches!(tile.kind, TileType::City | TileType::Temple | TileType::Ruin);
            if tile.road && !place {
                let (shape, turns) = roads::shape_of(map.road_links(hex));
                let road: [HexVertex; HEX_VERTICES] = std::array::from_fn(|i| HexVertex {
                    pos: corner(i),
                    uv: roads::turned_uv(positions[i], turns),
                    bary: HEX_BARY[i],
                    border: NO_BORDER,
                });
                self.roads[roads::picture(shape, tile.kind == TileType::Water)].push(&road, &HEX_INDICES);
            }

            // The units here (not those shown going: they are drawn where
            // they are on their way).
            let here: Vec<UnitId> = tile.units().iter().copied().filter(|id| !hidden.contains(id)).collect();
            // A city's garrison: its pips.
            if tile.kind == TileType::City && seen {
                let garrison: Vec<(UnitId, Side)> =
                    here.iter().filter_map(|id| map.units().get(*id).map(|unit| (*id, unit.side()))).collect();
                for (i, (id, side)) in garrison.iter().enumerate() {
                    let Some(at) = pip_at(i, garrison.len()) else { break };
                    // (The viewer's commander in it: its pip ringed light,
                    // and wider, to find the cities their leaders are in.)
                    let leads = map.is_commander(*id) && marks.viewer.is_none_or(|viewer| viewer == *side);
                    let (ring, width) = match (marks.front == Some(*id), leads) {
                        (true, _) => (SELECTED_RING, PIP_RING),
                        (false, true) => (LEADER_RING, 2.0 * PIP_RING),
                        _ => (RING_COLOR, PIP_RING),
                    };
                    let [r, g, b] = side_color(*side);
                    push_disc(&mut self.discs, view, visible.center, variant, at, PIP_RADIUS + width, ring);
                    push_disc(&mut self.discs, view, visible.center, variant, at, PIP_RADIUS, [r, g, b, 1.0]);
                }
            }

            // Elsewhere the stack (`push_stack`).
            if tile.kind != TileType::City && seen && !here.is_empty() {
                self.push_stack(map, view, (visible.center, variant), &here, marks);
            }

            // The rally points' flags, over the units, in the viewer's color
            // (whose they are; grey with nobody's).
            let flag_color = side_color(marks.viewer.unwrap_or(Side::Grey));
            for (_, n) in marks.flags.iter().filter(|(at, _)| *at == hex) {
                push_flag(&mut self.signs, view, (visible.center, variant), *n, flag_color);
            }

            if (marks.marked)(hex) {
                let flat: [FlatVertex; HEX_VERTICES] =
                    std::array::from_fn(|i| FlatVertex { pos: corner(i), color: marks.color });
                self.overlay.push(&flat, &HEX_INDICES);
            }
            if let Some((_, color)) = marks.flash.filter(|(at, _)| *at == hex) {
                let flat: [FlatVertex; HEX_VERTICES] = std::array::from_fn(|i| FlatVertex { pos: corner(i), color });
                self.overlay.push(&flat, &HEX_INDICES);
            }
            if !seen {
                // What was seen there last, under the fog: a ghost of its banner
                // (in a city, of its garrison's pips).
                if let Some(&Ghost { side, count, .. }) = marks.sight.ghost_at(hex) {
                    if tile.kind != TileType::City {
                        push_banner(&mut self.signs, view, visible.center, variant, side_color(side), GHOST_ALPHA);
                    }
                    push_size(&mut self.signs, view, visible.center, variant, count, side, GHOST_ALPHA);
                }
                let flat: [FlatVertex; HEX_VERTICES] =
                    std::array::from_fn(|i| FlatVertex { pos: corner(i), color: FOG });
                self.overlay.push(&flat, &HEX_INDICES);
            }
        }
        // The stacks shown going, where they are: the viewer's, and another
        // side's where the viewer sees.
        for moving in marks.moving {
            let theirs = moving
                .units
                .first()
                .and_then(|id| map.units().get(*id))
                .is_some_and(|u| marks.viewer.is_some_and(|v| v != u.side()));
            let shows = if theirs { marks.sight.sees(moving.hex) } else { marks.sight.knows(moving.hex) };
            if shows && map.contains(moving.hex) {
                self.push_stack(map, view, (moving.at, HexVariant::Full), &moving.units, marks);
            }
        }
    }

    /// A stack at `center` (a hex's middle; or, going, a point between two),
    /// the unit that shows the selected one if it is among `units`, else
    /// the first: of the viewer's (or with no viewer), a disc of its side's
    /// color with the picture of its kind on it, and a pip for each unit;
    /// of another side's, a banner and the pips.
    fn push_stack(
        &mut self,
        map: &Map,
        view: &MapView,
        (center, variant): ([f32; 2], HexVariant),
        units: &[UnitId],
        marks: &Marks,
    ) {
        // (A stack with a commander shows it, whoever is selected: its
        // banner is how its side finds the stacks its leaders are with.)
        let leader = units.iter().copied().find(|id| map.is_commander(*id));
        let shown = leader.or(marks.front.filter(|id| units.contains(id))).or(units.first().copied());
        let Some(unit) = shown.and_then(|id| map.units().get(id)) else { return };
        if marks.viewer.is_some_and(|v| v != unit.side()) {
            // Another side's stack: a banner, and how many there are.
            push_banner(&mut self.signs, view, center, variant, side_color(unit.side()), 1.0);
            push_size(&mut self.signs, view, center, variant, units.len(), unit.side(), 1.0);
            return;
        }
        let [r, g, b] = side_color(unit.side());
        let middle = [0.0, 0.0];
        let (ring, width) = level_ring(unit.level);
        push_disc(&mut self.discs, view, center, variant, middle, DISC_RADIUS + width, ring);
        push_disc(&mut self.discs, view, center, variant, middle, DISC_RADIUS, [r, g, b, 1.0]);
        push_size(&mut self.signs, view, center, variant, units.len(), unit.side(), 1.0);
        let image = map.units().kind_of(unit).map_or(UNIT_IMAGE, |kind| kind.image.as_str());
        let (positions, uvs) = (variant.positions(), variant.uvs());
        let sign: [HexVertex; HEX_VERTICES] = std::array::from_fn(|i| HexVertex {
            pos: view.to_ndc([positions[i][0] + center[0], positions[i][1] + center[1]]),
            uv: uvs[i],
            bary: HEX_BARY[i],
            border: NO_BORDER,
        });
        let layer = match self.units.iter().position(|(name, _)| name == image) {
            Some(i) => i,
            None => {
                self.units.push((image.to_string(), MeshList::new()));
                self.units.len() - 1
            }
        };
        self.units[layer].1.push(&sign, &HEX_INDICES);
    }

    /// The hexes never seen (total fog of war), drawn in [`UNKNOWN`].
    pub fn unknown(&self) -> &MeshList<HexVertex> {
        &self.unknown
    }

    /// The hexes of one terrain (no cities: see [`MapGeometry::cities`]).
    pub fn terrain(&self, kind: TileType) -> &MeshList<HexVertex> {
        &self.terrain[kind.index()]
    }

    /// The cities that `side` holds (`Side::Grey`: nobody's).
    pub fn cities(&self, side: Side) -> &MeshList<HexVertex> {
        &self.cities[side.index()]
    }

    /// The hexes whose road is drawn with one picture (`roads::picture`).
    pub fn roads(&self, picture: usize) -> &MeshList<HexVertex> {
        self.roads.get(picture).unwrap_or(&self.none)
    }

    /// The discs under the units and the cities' pips, each with its ring
    /// first.
    pub fn discs(&self) -> &MeshList<FlatVertex> {
        &self.discs
    }

    /// Over the units: the banners of the stacks whose units are not shown,
    /// and the stacks' sizes.
    pub fn signs(&self) -> &MeshList<FlatVertex> {
        &self.signs
    }

    /// The hexes that show a unit, by the unit's image.
    pub fn units(&self) -> impl Iterator<Item = (&str, &MeshList<HexVertex>)> {
        self.units.iter().filter(|(_, list)| !list.is_empty()).map(|(image, list)| (image.as_str(), list))
    }

    /// The hexes that show a unit with this image.
    pub fn units_with(&self, image: &str) -> &MeshList<HexVertex> {
        self.units.iter().find(|(name, _)| name == image).map_or(&self.none, |(_, list)| list)
    }

    pub fn overlay(&self) -> &MeshList<FlatVertex> {
        &self.overlay
    }
}

/// Draws the main map into the current viewport.
pub struct MapRenderer {
    /// Where the pictures come from.
    dirs: AssetDirs,
    /// Indexed like `TileType::ALL`.
    tiles: Vec<TextureId>,
    /// The cities' pictures, indexed by `Side::index`.
    cities: Vec<TextureId>,
    /// Indexed by `roads::picture`.
    roads: Vec<TextureId>,
    /// The units' pictures that have been asked for, by image; `None` for one
    /// that could not be read (the default unit's is drawn instead).
    units: HashMap<String, Option<TextureId>>,
    /// The picture of a unit without one.
    unit: TextureId,
    /// A pixel of [`UNKNOWN`], for the land never seen.
    black: TextureId,
    geometry: MapGeometry,
    buffers: BufferPool,
}

/// The textures every map needs.
struct Textures {
    tiles: Vec<TextureId>,
    cities: Vec<TextureId>,
    roads: Vec<TextureId>,
    unit: TextureId,
}

/// The textures every map needs: the tiles, the sides' cities, the roads, the
/// default unit.
fn load_all(ctx: &mut dyn RenderingBackend, dirs: &AssetDirs) -> Result<Textures, String> {
    // On an error, what was loaded before it goes again.
    let mut loaded: Vec<TextureId> = Vec::new();
    let mut load = |ctx: &mut dyn RenderingBackend, path: &str, sampling| match dirs.texture(ctx, path, sampling) {
        Ok(texture) => {
            loaded.push(texture);
            Ok(texture)
        }
        Err(e) => {
            for texture in loaded.drain(..) {
                ctx.delete_texture(texture);
            }
            Err(e)
        }
    };
    // The tiles and the roads are drawn at every size from a few pixels to
    // their own, so they are scaled smoothly.
    let mut tiles = Vec::with_capacity(TERRAINS);
    for kind in TileType::ALL {
        tiles.push(load(ctx, &tile_image(kind), Sampling::Smooth)?);
    }
    let mut cities = Vec::with_capacity(CITY_SIDES.len());
    for side in CITY_SIDES {
        cities.push(load(ctx, &city_image(side), Sampling::Smooth)?);
    }
    let mut roads = Vec::with_capacity(ROAD_PICTURES);
    for picture in 0..ROAD_PICTURES {
        roads.push(load(ctx, &roads::road_image(picture), Sampling::Smooth)?);
    }
    let unit = load(ctx, UNIT_IMAGE, Sampling::Nearest)?;
    Ok(Textures { tiles, cities, roads, unit })
}

/// Draw a layer of textured hexes with the hex pipeline, which is applied.
fn draw_textured(
    ctx: &mut dyn RenderingBackend,
    buffers: &mut BufferPool,
    list: &MeshList<HexVertex>,
    texture: TextureId,
) {
    for mesh in list.meshes() {
        let (vb, ib) = buffers.upload(ctx, mesh);
        ctx.apply_bindings_from_slice(&[vb], ib, &[texture]);
        ctx.draw(0, mesh.indices.len() as i32, 1);
    }
}

/// Draw a layer of flat shapes with the flat pipeline, which is applied.
fn draw_flat(ctx: &mut dyn RenderingBackend, buffers: &mut BufferPool, list: &MeshList<FlatVertex>) {
    for mesh in list.meshes() {
        let (vb, ib) = buffers.upload(ctx, mesh);
        ctx.apply_bindings_from_slice(&[vb], ib, &[]);
        ctx.draw(0, mesh.indices.len() as i32, 1);
    }
}

impl MapRenderer {
    /// Load the tiles, the roads and the default unit's picture.
    /// C++: MapRenderer::MapRenderer
    pub fn new(ctx: &mut dyn RenderingBackend, dirs: AssetDirs) -> Result<MapRenderer, String> {
        let Textures { tiles, cities, roads, unit } = load_all(ctx, &dirs)?;
        let [r, g, b] = UNKNOWN;
        let pixel = turbo_ferret::Image { size: turbo_ferret::Size::new(1, 1), rgba: vec![r, g, b, 255] };
        Ok(MapRenderer {
            dirs,
            tiles,
            cities,
            roads,
            units: HashMap::new(),
            unit,
            black: super::textures::upload(ctx, &pixel, Sampling::Nearest),
            geometry: MapGeometry::new(),
            buffers: BufferPool::new(),
        })
    }

    /// Where the pictures come from.
    pub fn dirs(&self) -> &AssetDirs {
        &self.dirs
    }

    /// Take the pictures from other directories (another map was opened, with
    /// assets of its own or without). If one cannot be read, the old ones stay.
    pub fn set_dirs(&mut self, ctx: &mut dyn RenderingBackend, dirs: AssetDirs) -> Result<(), String> {
        let Textures { tiles, cities, roads, unit } = load_all(ctx, &dirs)?;
        self.delete_textures(ctx);
        (self.dirs, self.tiles, self.cities, self.roads, self.unit) = (dirs, tiles, cities, roads, unit);
        Ok(())
    }

    /// The texture of a unit's image: loaded when it is first drawn. One that
    /// cannot be read is said so once, and the default unit's is drawn.
    fn unit_texture(&mut self, ctx: &mut dyn RenderingBackend, image: &str) -> TextureId {
        if image == UNIT_IMAGE {
            return self.unit;
        }
        if let Some(known) = self.units.get(image) {
            return known.unwrap_or(self.unit);
        }
        // The signs are scaled smoothly: their transparent pixels have the
        // outline's color (`docs/tiles.md`).
        let texture = match self.dirs.texture(ctx, image, Sampling::Smooth) {
            Ok(texture) => Some(texture),
            Err(e) => {
                crate::platform::complain(&format!("a unit's picture: {e}"));
                None
            }
        };
        self.units.insert(image.to_string(), texture);
        texture.unwrap_or(self.unit)
    }

    /// Draw the map. The caller has set the viewport (and the scissor) to the map
    /// widget's rectangle. C++: MapRenderer::renderMap
    pub fn draw(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        pipelines: &Pipelines,
        map: &Map,
        view: &MapView,
        marks: &Marks,
    ) {
        self.geometry.build(map, view, marks);
        self.buffers.rewind();
        let images: Vec<String> = self.geometry.units().map(|(image, _)| image.to_string()).collect();
        let unit_textures: Vec<TextureId> = images.iter().map(|image| self.unit_texture(ctx, image)).collect();

        // The terrain, the roads over it, the discs, the units on them, and
        // the overlay over everything.
        let (geometry, buffers) = (&self.geometry, &mut self.buffers);
        ctx.apply_pipeline(&pipelines.hex);
        for kind in TileType::ALL {
            draw_textured(ctx, buffers, geometry.terrain(kind), self.tiles[kind.index()]);
        }
        for side in CITY_SIDES {
            draw_textured(ctx, buffers, geometry.cities(side), self.cities[side.index()]);
        }
        draw_textured(ctx, buffers, geometry.unknown(), self.black);
        for (picture, texture) in self.roads.iter().enumerate() {
            draw_textured(ctx, buffers, geometry.roads(picture), *texture);
        }
        if !geometry.discs().is_empty() {
            ctx.apply_pipeline(&pipelines.flat);
            draw_flat(ctx, buffers, geometry.discs());
            ctx.apply_pipeline(&pipelines.hex);
            for (image, texture) in images.iter().zip(unit_textures) {
                draw_textured(ctx, buffers, geometry.units_with(image), texture);
            }
        }
        if !geometry.signs().is_empty() {
            ctx.apply_pipeline(&pipelines.flat);
            draw_flat(ctx, buffers, geometry.signs());
        }
        if !geometry.overlay().is_empty() {
            ctx.apply_pipeline(&pipelines.flat);
            draw_flat(ctx, buffers, geometry.overlay());
        }
    }

    fn delete_textures(&mut self, ctx: &mut dyn RenderingBackend) {
        let units = self.units.drain().filter_map(|(_, texture)| texture);
        let pictures = self.tiles.drain(..).chain(self.cities.drain(..)).chain(self.roads.drain(..));
        for texture in pictures.chain(units).chain([self.unit]) {
            ctx.delete_texture(texture);
        }
    }

    /// Free the GPU objects.
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        self.delete_textures(ctx);
        ctx.delete_texture(self.black);
        self.buffers.delete_all(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;

    const START: Hex = Hex::new(50, 50);

    fn view_at(center: Hex) -> MapView {
        let mut v = MapView::new(100, 100, center);
        v.resize(819, 537);
        v
    }

    fn geometry(game: &Game, view: &MapView) -> MapGeometry {
        let mut g = MapGeometry::new();
        g.build(&game.map, view, &Marks::plain(RANGE_OVERLAY, &|hex| game.can_move_to(hex), None));
        g
    }

    /// The hexes of a layer: their seven vertices each.
    fn hexes(list: &MeshList<HexVertex>) -> Vec<&[HexVertex]> {
        list.meshes().iter().flat_map(|m| m.vertices.chunks(HEX_VERTICES)).collect()
    }

    fn center_of(view: &MapView, hex: Hex) -> [f32; 2] {
        view.to_ndc(view.hex_center(hex))
    }

    #[test]
    fn every_terrain_has_its_own_image() {
        let images: Vec<String> = TileType::ALL.into_iter().map(tile_image).collect();
        assert_eq!(images[0], "tiles/grass_tile.png");
        assert_eq!(tile_image(TileType::City), "tiles/city_tile.png");
        // A side's city has its own; nobody's is the terrain's.
        assert_eq!(city_image(Side::Red), "tiles/city_red_tile.png");
        assert_eq!(city_image(Side::Grey), tile_image(TileType::City));
        for (i, side) in CITY_SIDES.into_iter().enumerate() {
            assert_eq!(side.index(), i, "{side:?}");
            let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
            assert!(AssetDirs::new(&assets, None).find(&city_image(side)).is_some(), "{side:?}");
        }
        assert_eq!(tile_image(TileType::Desert), "tiles/desert_tile.png");
        let mut different = images.clone();
        different.sort();
        different.dedup();
        assert_eq!(different.len(), TERRAINS);
        // And every one is in the repository's assets.
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for image in &images {
            assert!(assets.join(image).is_file(), "{image}");
        }
    }

    #[test]
    fn every_terrain_is_drawn_with_its_own_mesh() {
        // A map with one hex of each terrain in a row.
        let mut map = Map::new(20, 20, TileType::Water).unwrap();
        for (col, kind) in TileType::ALL.into_iter().enumerate() {
            map.tile_mut(Hex::new(10, col as u16 + 5)).unwrap().kind = kind;
        }
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));
        for (col, kind) in TileType::ALL.into_iter().enumerate() {
            let want = center_of(&view, Hex::new(10, col as u16 + 5));
            // (A city hex without a city record is nobody's city.)
            let drawn = hexes(if kind == TileType::City { g.cities(Side::Grey) } else { g.terrain(kind) });
            assert!(drawn.iter().any(|h| h[0].pos == want), "{kind:?}");
            if kind != TileType::Water {
                assert_eq!(drawn.len(), 1, "{kind:?}");
            }
        }
    }

    #[test]
    fn the_start_view_is_grass_with_a_mountain_diagonal_and_one_unit() {
        let game = crate::game::grassland();
        let view = view_at(START);
        let g = geometry(&game, &view);
        let visible = view.visible_hexes().count();
        let grass = hexes(g.terrain(TileType::Grass)).len();
        let mountain = hexes(g.terrain(TileType::Mountain)).len();
        assert_eq!(grass + mountain, visible);
        // Rows 43..=57 are drawn: one mountain in each.
        assert_eq!(mountain, 15);
        for t in [TileType::Forest, TileType::Swamp, TileType::Hill, TileType::Water, TileType::City, TileType::Desert]
        {
            assert!(g.terrain(t).is_empty(), "{t:?}");
        }
        // One hex holds a unit, in the middle; nothing is selected.
        let units = hexes(g.units_with(UNIT_IMAGE));
        assert_eq!(units.len(), 1);
        assert_eq!(units[0][0].pos, [0.0, 0.0]);
        assert!(g.overlay().is_empty());
        // Six triangles a hex.
        assert_eq!(g.terrain(TileType::Grass).index_count(), grass * 18);
    }

    #[test]
    fn hexes_are_placed_by_the_view() {
        let game = crate::game::grassland();
        let view = view_at(START);
        let g = geometry(&game, &view);
        let (wf, hf) = view.factors();
        for hex in hexes(g.terrain(TileType::Grass)) {
            // A whole hex: its center, then its corners around it.
            let [cx, cy] = hex[0].pos;
            for (v, model) in hex.iter().zip(HexVariant::Full.positions()) {
                assert!((v.pos[0] - (cx + model[0] * wf)).abs() < 1e-6);
                assert!((v.pos[1] - (cy + model[1] * hf)).abs() < 1e-6);
            }
            for (i, v) in hex.iter().enumerate() {
                assert_eq!(v.uv, HexVariant::Full.uvs()[i]);
                assert_eq!(v.bary, HEX_BARY[i]);
            }
        }
        // The hex right of the center is in the list, where the view says.
        let want = center_of(&view, Hex::new(50, 51));
        assert!(hexes(g.terrain(TileType::Grass)).iter().any(|h| h[0].pos == want));
    }

    #[test]
    fn the_cursor_hex_has_the_red_border_and_the_others_black() {
        let mut game = crate::game::grassland();
        game.set_cursor(Hex::new(50, 52));
        let view = view_at(START);
        let g = geometry(&game, &view);
        let cursor_at = center_of(&view, Hex::new(50, 52));
        let mut red = 0;
        for t in TileType::ALL {
            for hex in hexes(g.terrain(t)) {
                let is_cursor = hex[0].pos == cursor_at;
                let want = if is_cursor { [1.0, 0.0, 0.0, 0.84] } else { [0.0, 0.0, 0.0, 0.94] };
                for v in hex {
                    for (got, want) in v.border.iter().zip(want) {
                        assert!((got - want).abs() < 1e-6, "{:?}", v.border);
                    }
                }
                red += usize::from(is_cursor);
            }
        }
        assert_eq!(red, 1);
    }

    #[test]
    fn an_outlined_hex_has_the_stops_border() {
        let mut game = crate::game::grassland();
        game.set_cursor(Hex::new(50, 52));
        let view = view_at(START);
        let stops = [Hex::new(50, 51), Hex::new(50, 52)];
        let marks = Marks { outlined: &stops, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        let mut g = MapGeometry::new();
        g.build(&game.map, &view, &marks);
        let border_at = |hex: Hex| {
            let at = center_of(&view, hex);
            hexes(g.terrain(TileType::Grass)).iter().find(|h| h[0].pos == at).map(|h| h[0].border).unwrap()
        };
        let (thick, thin) = (view.border_threshold(true), view.border_threshold(false));
        let [r, g_, b] = STOP_BORDER;
        assert_eq!(border_at(Hex::new(50, 51)), [r, g_, b, thick]);
        // The cursor's border wins.
        assert_eq!(border_at(Hex::new(50, 52)), [1.0, 0.0, 0.0, thick]);
        assert_eq!(border_at(Hex::new(50, 53)), [0.0, 0.0, 0.0, thin]);
    }

    #[test]
    fn a_rally_points_flag_is_in_its_sides_color() {
        let game = crate::game::grassland();
        let view = view_at(START);
        let flags = [(Hex::new(50, 52), 3)];
        let count = |viewer: Option<Side>, color: [f32; 4]| {
            let marks = Marks { flags: &flags, viewer, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
            let mut g = MapGeometry::new();
            g.build(&game.map, &view, &marks);
            g.signs().meshes().iter().flat_map(|m| &m.vertices).filter(|v| v.color == color).count()
        };
        let cloth = |side: Side| {
            let [r, g, b] = side_color(side);
            [r, g, b, 1.0]
        };
        // Red's flag: a red cloth, a light number on it.
        assert!(count(Some(Side::Red), cloth(Side::Red)) > 0);
        assert!(count(Some(Side::Red), FLAG_NUMBER_LIGHT) > 0);
        assert_eq!(count(Some(Side::Red), cloth(Side::Yellow)), 0, "not yellow");
        // Yellow's: a dark number on its light cloth. Nobody's: grey.
        assert!(count(Some(Side::Yellow), FLAG_NUMBER_DARK) > 0);
        assert!(count(None, cloth(Side::Grey)) > 0);
    }

    #[test]
    fn a_unit_is_a_disc_of_its_sides_color_and_its_picture_on_the_hex() {
        let game = crate::game::grassland();
        let view = view_at(START);
        let g = geometry(&game, &view);
        // The unit stands on a mountain: its picture is laid on the same seven
        // places, without a border of its own.
        let unit = hexes(g.units_with(UNIT_IMAGE))[0];
        let mountains = hexes(g.terrain(TileType::Mountain));
        let under = mountains.iter().find(|h| h[0].pos == unit[0].pos).expect("the hex under the unit");
        for (sign, ground) in unit.iter().zip(under.iter()) {
            assert_eq!((sign.pos, sign.uv, sign.bary), (ground.pos, ground.uv, ground.bary));
            assert_eq!(sign.border, NO_BORDER);
        }
        assert_eq!(g.units().map(|(image, _)| image).collect::<Vec<_>>(), [UNIT_IMAGE]);
        assert!(g.units_with("units/none.png").is_empty());

        // Under it: a dark ring, then the disc in the side's color, round on
        // the screen and inside the hex.
        let discs: Vec<&[FlatVertex]> =
            g.discs().meshes().iter().flat_map(|m| m.vertices.chunks(DISC_CORNERS + 1)).collect();
        assert_eq!(discs.len(), 2);
        assert_eq!(discs[0][0].color, RING_COLOR);
        let [r, g_, b] = side_color(Side::Black);
        assert!(discs[1].iter().all(|v| v.color == [r, g_, b, 1.0]));
        assert_eq!(discs[1][0].pos, unit[0].pos, "around the hex's middle");
        let (wf, hf) = view.factors();
        let (w, h) = view.size();
        for v in &discs[1][1..] {
            // In pixels from the middle: the same distance all round.
            let dx = (v.pos[0] - unit[0].pos[0]) * w as f32 / 2.0;
            let dy = (v.pos[1] - unit[0].pos[1]) * h as f32 / 2.0;
            let radius = DISC_RADIUS * wf * w as f32 / 2.0;
            assert!(((dx * dx + dy * dy).sqrt() - radius).abs() < 0.01 * radius, "{dx} {dy} {radius}");
        }
        // The ring too is inside the hex: within its flat edges, top and bottom.
        let reach = (DISC_RADIUS + DISC_RING) * hf / HEX_HEIGHT_ADJUST;
        assert!(reach < hf, "{reach} {hf}");
    }

    /// A map of grass with a road along row 10 from a city, over a river.
    fn road_map() -> Map {
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggcGGWGTggggggggg".to_string();
        Map::parse(&format!("20\n20\n{}\n", rows.join("\n"))).unwrap()
    }

    #[test]
    fn a_road_is_its_shapes_picture_turned() {
        let map = road_map();
        let mut view = MapView::new(20, 20, Hex::new(10, 8));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));

        let mut drawn = 0;
        for col in 6..=9 {
            let hex = Hex::new(10, col);
            let (shape, turns) = roads::shape_of(map.road_links(hex));
            let over_water = col == 8;
            let road = hexes(g.roads(roads::picture(shape, over_water)));
            let this =
                road.iter().find(|h| h[0].pos == center_of(&view, hex)).unwrap_or_else(|| panic!("no road on {hex}"));
            // Through roads: two sides each, opposite or nearly.
            assert_eq!(map.road_links(hex).count_ones(), 2, "{hex}");
            let positions = HexVariant::Full.positions();
            for (i, v) in this.iter().enumerate() {
                assert_eq!(v.uv, roads::turned_uv(positions[i], turns), "{hex}, corner {i}");
                assert_eq!(v.border, NO_BORDER);
            }
            drawn += 1;
        }
        // Nothing else has a road painted on it: not the city the road starts
        // at, not the temple it ends at (a capital T), not the grass around.
        let all: usize = (0..roads::ROAD_PICTURES).map(|p| hexes(g.roads(p)).len()).sum();
        assert_eq!((drawn, all), (4, 4));
        // The bridge is one of the bridges' pictures, the others are roads'.
        let bridges: usize = (roads::ROAD_SHAPES.len()..roads::ROAD_PICTURES).map(|p| hexes(g.roads(p)).len()).sum();
        assert_eq!(bridges, 1);
        assert!(g.roads(99).is_empty());
    }

    const KINDS: &str = "unit a\nimage units/militia.png\nmoves 5\nenter grass 1\n\
                         unit b\nimage units/lightcavalry.png\nmoves 5\nenter grass 1\n\
                         unit c\nmoves 5\nenter grass 1\n";

    /// The discs' middles and colors, the ring of each before it.
    fn discs(g: &MapGeometry) -> Vec<([f32; 2], [f32; 4])> {
        let discs = g.discs().meshes().iter().flat_map(|m| m.vertices.chunks(DISC_CORNERS + 1));
        discs.map(|d| (d[0].pos, d[0].color)).collect()
    }

    fn solid(side: Side) -> [f32; 4] {
        let [r, g, b] = side_color(side);
        [r, g, b, 1.0]
    }

    #[test]
    fn units_are_drawn_by_their_kinds_picture() {
        let terrain = format!("20\n20\n{}\n", vec!["g".repeat(20); 20].join("\n"));
        let (mut map, _) = crate::state::scenario::parse(&terrain, Some(KINDS), None).unwrap();
        let put = |map: &mut Map, side: Side, kind: &str, hex: Hex| {
            let kind = map.units().kind_named(kind).unwrap();
            let unit = map.new_unit_of(side, kind).unwrap();
            map.place_unit(unit, hex).unwrap();
            unit
        };
        put(&mut map, Side::Red, "b", Hex::new(10, 5));
        let militia = put(&mut map, Side::Red, "a", Hex::new(10, 5));
        put(&mut map, Side::Blue, "a", Hex::new(10, 10));
        put(&mut map, Side::Blue, "a", Hex::new(10, 10));
        put(&mut map, Side::Grey, "c", Hex::new(10, 14));
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));

        // The first unit of a hex is the one that shows.
        let at = |g: &MapGeometry, image: &str| -> Vec<[f32; 2]> {
            hexes(g.units_with(image)).iter().map(|h| h[0].pos).collect()
        };
        assert_eq!(at(&g, "units/lightcavalry.png"), [center_of(&view, Hex::new(10, 5))]);
        assert_eq!(at(&g, "units/militia.png"), [center_of(&view, Hex::new(10, 10))]);
        assert_eq!(at(&g, UNIT_IMAGE), [center_of(&view, Hex::new(10, 14))], "a kind without a picture");
        assert_eq!(g.units().count(), 3);

        // Under each a disc of its side's color in a dark ring, in the middle.
        let colors: Vec<[f32; 4]> = discs(&g).into_iter().map(|(_, color)| color).collect();
        let count = |color: [f32; 4]| colors.iter().filter(|c| **c == color).count();
        assert_eq!((count(solid(Side::Red)), count(solid(Side::Blue)), count(solid(Side::Grey))), (1, 1, 1));
        assert_eq!((count(RING_COLOR), colors.len()), (3, 6));
        assert!(discs(&g).iter().any(|(at, _)| *at == center_of(&view, Hex::new(10, 5))));
        // The stacks say how many they are: two pips for each of the two,
        // along the bottom of the hex, in the side's color (and rings); one
        // for the one alone.
        let signs: Vec<([f32; 2], [f32; 4])> = g
            .signs()
            .meshes()
            .iter()
            .flat_map(|m| m.vertices.chunks(DISC_CORNERS + 1))
            .map(|d| (d[0].pos, d[0].color))
            .collect();
        let pips = |side: Side| signs.iter().filter(|(_, c)| *c == solid(side)).map(|(at, _)| *at).collect::<Vec<_>>();
        assert_eq!((pips(Side::Red).len(), pips(Side::Blue).len(), pips(Side::Grey).len()), (2, 2, 1));
        assert!(pips(Side::Blue).iter().all(|at| at[1] < center_of(&view, Hex::new(10, 10))[1]));
        assert_eq!(signs.len(), 10);
        // The sides can be told apart.
        let sides: Vec<[f32; 3]> = Side::PLAYERS.into_iter().chain([Side::Grey]).map(side_color).collect();
        for (i, a) in sides.iter().enumerate() {
            for b in &sides[i + 1..] {
                assert!((0..3).any(|k| (a[k] - b[k]).abs() > 0.15), "{a:?} {b:?}");
            }
        }

        // The selected unit shows on its hex instead of the first: the militia
        // stands behind the rider.
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, Some(militia)));
        assert!(g.units_with("units/lightcavalry.png").is_empty());
        assert_eq!(at_all(&g, "units/militia.png"), 2);
        // A unit that is not on a hex changes nothing there.
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));
        assert_eq!(at_all(&g, "units/lightcavalry.png"), 1);

        // The map changes and the pictures follow: the rider is gone.
        let rider = map.units_at(Hex::new(10, 5))[0];
        map.kill_unit(rider);
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));
        assert!(g.units_with("units/lightcavalry.png").is_empty());
        assert_eq!(g.units().count(), 2);
        assert_eq!(at_all(&g, "units/militia.png"), 2);
    }

    #[test]
    fn a_city_shows_itself_in_its_owners_colors_and_a_pip_for_each_unit() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggcggggcgggcggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let cities = "city 10 5\nname One\nside red\ngarrison b\ngarrison a\n\
                      city 10 10\nname Two\nside blue\ngarrison a 6\n\
                      city 10 14\nname Three\n";
        let (map, _) = crate::state::scenario::parse(&terrain, Some(KINDS), Some(cities)).unwrap();
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));

        // Each city with its owner's picture, nobody's with the city's own;
        // none in the terrain's.
        let at = |list: &MeshList<HexVertex>| -> Vec<[f32; 2]> { hexes(list).iter().map(|h| h[0].pos).collect() };
        assert_eq!(at(g.cities(Side::Red)), [center_of(&view, Hex::new(10, 5))]);
        assert_eq!(at(g.cities(Side::Blue)), [center_of(&view, Hex::new(10, 10))]);
        assert_eq!(at(g.cities(Side::Grey)), [center_of(&view, Hex::new(10, 14))]);
        assert!(g.terrain(TileType::City).is_empty() && g.cities(Side::Green).is_empty());
        // No unit's picture: the cities' own show.
        assert_eq!(g.units().count(), 0);
        // One has two red pips, Two six blue ones, Three (nobody's, empty)
        // nothing.
        let colors: Vec<[f32; 4]> = discs(&g).into_iter().map(|(_, color)| color).collect();
        let count = |colors: &[[f32; 4]], color: [f32; 4]| colors.iter().filter(|c| **c == color).count();
        assert_eq!(count(&colors, solid(Side::Red)), 2);
        assert_eq!(count(&colors, solid(Side::Blue)), 6);
        assert_eq!(count(&colors, solid(Side::Grey)), 0);
        assert_eq!((count(&colors, RING_COLOR), colors.len()), (8, 16));
        // Each pip where `pip_at` says, in the lower half of the hex.
        let place = |hex: Hex, at: [f32; 2]| {
            let middle = view.hex_center(hex);
            view.to_ndc([middle[0] + at[0], middle[1] + at[1]])
        };
        let pips: Vec<[f32; 2]> =
            discs(&g).into_iter().filter(|(_, color)| *color == solid(Side::Blue)).map(|(at, _)| at).collect();
        for (i, pip) in pips.iter().enumerate() {
            let at = place(Hex::new(10, 10), pip_at(i, 6).unwrap());
            assert!((pip[0] - at[0]).abs() < 1e-6 && (pip[1] - at[1]).abs() < 1e-6, "pip {i}");
            assert!(at[1] < center_of(&view, Hex::new(10, 10))[1]);
        }

        // Rows of five from the bottom, each row centered; no more than two
        // (a full stack and two commanders).
        let step = PIP_STEP;
        assert_eq!(pip_at(0, 1), Some([0.0, PIP_BOTTOM]));
        assert_eq!(pip_at(1, 2), Some([step / 2.0, PIP_BOTTOM]));
        assert_eq!(pip_at(4, 7), Some([2.0 * step, PIP_BOTTOM]));
        assert_eq!(pip_at(6, 7), Some([step / 2.0, PIP_BOTTOM + PIP_ROW_STEP]));
        assert_eq!((pip_at(2, 2), pip_at(10, 11)), (None, None));
        // Every pip is inside its hex, whose half width at height y is 1 - |y| / 2.
        for i in 0..PIPS_PER_ROW * PIP_ROWS {
            let [x, y] = pip_at(i, PIPS_PER_ROW * PIP_ROWS).unwrap();
            let r = PIP_RADIUS + PIP_RING;
            let reach = r / HEX_HEIGHT_ADJUST;
            assert!(y - reach > -1.0, "pip {i}");
            assert!(x.abs() + r < 1.0 - (y.abs() + reach) / 2.0, "pip {i}");
        }

        // The selected unit's pip is ringed in gold; it still shows no picture.
        let militia = map.units_at(Hex::new(10, 5))[1];
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, Some(militia)));
        let colors: Vec<[f32; 4]> = discs(&g).into_iter().map(|(_, color)| color).collect();
        assert_eq!((count(&colors, SELECTED_RING), count(&colors, RING_COLOR)), (1, 7));
        let gold = discs(&g).into_iter().find(|(_, color)| *color == SELECTED_RING).unwrap().0;
        assert_eq!(gold, place(Hex::new(10, 5), pip_at(1, 2).unwrap()));
        assert_eq!(g.units().count(), 0);
    }

    fn at_all(g: &MapGeometry, image: &str) -> usize {
        hexes(g.units_with(image)).len()
    }

    /// The owner's word, 2026-11-22: the selected stack's hex in green, its
    /// orders in a color of their own, the hex of a battle ringed and
    /// flashing: rings the caller names win over the cursor's border and
    /// the outlines, the later of two over the earlier.
    #[test]
    fn a_ringed_hex_has_its_own_border_and_a_battles_hex_its_tint() {
        let mut game = crate::game::grassland();
        game.set_cursor(Hex::new(50, 52));
        let view = view_at(START);
        let (on_cursor, on_stop, twice, plain) =
            (Hex::new(50, 52), Hex::new(50, 51), Hex::new(50, 54), Hex::new(50, 53));
        let stops = [on_stop];
        let rings =
            [(on_cursor, SELECTED_BORDER), (on_stop, OWN_WAY_BORDER), (twice, OWN_WAY_BORDER), (twice, BATTLE_BORDER)];
        let flash = [1.0, 0.1, 0.0, 0.3];
        let marks = Marks {
            outlined: &stops,
            rings: &rings,
            flash: Some((twice, flash)),
            ..Marks::plain(RANGE_OVERLAY, &|_| false, None)
        };
        let mut g = MapGeometry::new();
        g.build(&game.map, &view, &marks);
        let border_at = |hex: Hex| {
            let at = center_of(&view, hex);
            hexes(g.terrain(TileType::Grass)).iter().find(|h| h[0].pos == at).map(|h| h[0].border).unwrap()
        };
        let (thick, thin) = (view.border_threshold(true), view.border_threshold(false));
        let with = |[r, g, b]: [f32; 3]| [r, g, b, thick];
        assert_eq!(border_at(on_cursor), with(SELECTED_BORDER), "over the cursor's red");
        assert_eq!(border_at(on_stop), with(OWN_WAY_BORDER), "over an outline");
        assert_eq!(border_at(twice), with(BATTLE_BORDER), "the later of two");
        assert_eq!(border_at(plain), [0.0, 0.0, 0.0, thin]);
        // The battle's hex, and no other, has its tint over it.
        let tints: Vec<([f32; 2], [f32; 4])> = g
            .overlay()
            .meshes()
            .iter()
            .flat_map(|m| m.vertices.chunks(HEX_VERTICES))
            .map(|h| (h[0].pos, h[0].color))
            .collect();
        assert_eq!(tints, [(center_of(&view, twice), flash)]);
    }

    /// "If one of your stacks has a captain then display it with the
    /// captain's banner to make it easier to find your stacks with
    /// leaders": a stack shows its commander, whoever of it is selected,
    /// and a commander's pip in a city of its side is ringed light.
    #[test]
    fn a_stack_with_a_commander_shows_it_and_a_city_rings_its_pip() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggcgggggggggggggg".to_string();
        let terrain = format!(
            "20
20
{}
",
            rows.join(
                "
"
            )
        );
        let kinds = format!("{KINDS}unit lead\nimage units/captain.png\nmoves 5\ncommander 1\nenter grass 1, city 1\n");
        let cities = "city 10 5\nname One\nside red\ngarrison a\ngarrison lead\n";
        let (mut map, _) = crate::state::scenario::parse(&terrain, Some(&kinds), Some(cities)).unwrap();
        let field = Hex::new(10, 12);
        let put = |map: &mut Map, kind: &str| {
            let kind = map.units().kind_named(kind).unwrap();
            let id = map.new_unit_of(Side::Red, kind).unwrap();
            map.place_unit(id, field).unwrap();
            id
        };
        let (soldier, _leader) = (put(&mut map, "a"), put(&mut map, "lead"));
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        // The soldier is first in the stack, and selected: the captain shows.
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, Some(soldier)));
        assert_eq!((at_all(&g, "units/captain.png"), at_all(&g, "units/militia.png")), (1, 0));
        // In the city: two pips, the captain's ringed light and wider.
        let rings: Vec<[f32; 4]> = discs(&g).into_iter().map(|(_, color)| color).collect();
        assert_eq!(rings.iter().filter(|c| **c == LEADER_RING).count(), 1);
        // Another side's viewer is shown a banner and plain pips: nothing
        // of who leads.
        let marks = Marks { viewer: Some(Side::Blue), ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        g.build(&map, &view, &marks);
        let rings: Vec<[f32; 4]> = discs(&g).into_iter().map(|(_, color)| color).collect();
        assert_eq!((rings.iter().filter(|c| **c == LEADER_RING).count(), at_all(&g, "units/captain.png")), (0, 0));
    }

    #[test]
    fn a_stack_remembered_in_the_fog_is_a_ghost_of_its_banner() {
        let terrain = format!("20\n20\n{}\n", vec!["g".repeat(20); 20].join("\n"));
        let (map, _) = crate::state::scenario::parse(&terrain, Some(KINDS), None).unwrap();
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        let ghost = Ghost { at: Hex::new(10, 12), side: Side::Blue, count: 3, round: 1 };
        let sight = Sight::by_hand(&map, &|_| false, None, vec![ghost]);
        let marks = Marks { viewer: Some(Side::Red), sight: &sight, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        g.build(&map, &view, &marks);
        // A banner and three pips in blue, not quite solid, where it was seen.
        let [r, gr, b] = side_color(Side::Blue);
        let ghostly = [r, gr, b, GHOST_ALPHA];
        let blue = g.signs().meshes().iter().flat_map(|m| &m.vertices).filter(|v| v.color == ghostly).count();
        assert!(blue > 3 * (DISC_CORNERS + 1), "{blue}");
        assert_eq!(g.units().count(), 0, "no unit's picture: nobody is seen there");
    }

    #[test]
    fn the_others_stacks_are_banners_and_the_fog_hides_who_is_there() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggggggggggcgggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let cities = "city 10 13\nname Far\nside blue\ngarrison a\n";
        let (mut map, _) = crate::state::scenario::parse(&terrain, Some(KINDS), Some(cities)).unwrap();
        let put = |map: &mut Map, side: Side, hex: Hex| {
            let kind = map.units().kind_named("a").unwrap();
            let unit = map.new_unit_of(side, kind).unwrap();
            map.place_unit(unit, hex).unwrap();
        };
        put(&mut map, Side::Red, Hex::new(10, 5));
        for _ in 0..3 {
            put(&mut map, Side::Blue, Hex::new(10, 8));
        }
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        let sight = Sight::by_hand(&map, &|_| true, None, Vec::new());
        let marks = Marks { viewer: Some(Side::Red), sight: &sight, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        g.build(&map, &view, &marks);
        // Red's own unit shows what it is; blue's stack is a banner in blue
        // with three pips, and no picture.
        assert_eq!(at_all(&g, "units/militia.png"), 1);
        let blue = solid(Side::Blue);
        let banner = g.signs().meshes().iter().flat_map(|m| &m.vertices).filter(|v| v.color == blue).count();
        assert!(banner > 3 * (DISC_CORNERS + 1), "the cloth and three pips");
        assert!(g.overlay().is_empty(), "no fog");
        assert_eq!(hexes(g.cities(Side::Blue)).len(), 1);

        // In the fog (all but the west of the map): no blue at all, the city
        // nobody's, no garrison pips, and the hexes dimmed.
        let sight = Sight::by_hand(&map, &|hex| hex.col < 7, None, Vec::new());
        let marks = Marks { viewer: Some(Side::Red), sight: &sight, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        g.build(&map, &view, &marks);
        assert_eq!(at_all(&g, "units/militia.png"), 1, "red's own, in sight");
        let all_flat = g.signs().meshes().iter().chain(g.discs().meshes()).flat_map(|m| m.vertices.clone());
        assert!(all_flat.into_iter().all(|v| v.color != blue));
        assert!(g.cities(Side::Blue).is_empty());
        assert_eq!(hexes(g.cities(Side::Grey)).len(), 1);
        let fogged = g.overlay().meshes().iter().flat_map(|m| &m.vertices).filter(|v| v.color == FOG).count();
        assert!(fogged > 0 && fogged % HEX_VERTICES == 0);
    }

    #[test]
    fn land_never_seen_is_black_and_nothing_on_it_shows() {
        // Blue's city and a blue stack east of column 9, red's unit west;
        // red has seen columns 0 to 8 only (total fog of war).
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggggggggggcgggggg".to_string();
        rows[11] = "GGGGGGGGGGGGGGGGGGGG".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let cities = "city 10 13\nname Far\nside blue\ngarrison a\n";
        let (mut map, _) = crate::state::scenario::parse(&terrain, Some(KINDS), Some(cities)).unwrap();
        let kind = map.units().kind_named("a").unwrap();
        let red = map.new_unit_of(Side::Red, kind).unwrap();
        map.place_unit(red, Hex::new(10, 5)).unwrap();
        let blue = map.new_unit_of(Side::Blue, kind).unwrap();
        map.place_unit(blue, Hex::new(10, 11)).unwrap();
        map.cursor = Hex::new(5, 15);
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let known = |hex: Hex| hex.col < 9;
        let sight = Sight::by_hand(&map, &|hex| hex.col < 7, Some(&known), Vec::new());
        let marks = Marks { viewer: Some(Side::Red), sight: &sight, ..Marks::plain(RANGE_OVERLAY, &|_| false, None) };
        let mut g = MapGeometry::new();
        g.build(&map, &view, &marks);
        let black = hexes(g.unknown());
        // Every hex east of column 8 in view is black: no terrain, no city,
        // no road, no unit, no fog over it (it is darker than fog).
        let shown_east = view.visible_hexes().filter(|v| v.hex.col >= 9).count();
        assert_eq!(black.len(), shown_east);
        assert!(g.cities(Side::Blue).is_empty() && g.cities(Side::Grey).is_empty(), "the city is not known");
        let blue = solid(Side::Blue);
        let all_flat = g.signs().meshes().iter().chain(g.discs().meshes()).flat_map(|m| m.vertices.clone());
        assert!(all_flat.into_iter().all(|v| v.color != blue), "nor blue's stack");
        assert_eq!(at_all(&g, "units/militia.png"), 1, "red's own shows");
        let road_hexes: usize = (0..ROAD_PICTURES).map(|p| hexes(g.roads(p)).len()).sum();
        let known_road = view.visible_hexes().filter(|v| v.hex.row == 11 && v.hex.col < 9).count();
        assert_eq!(road_hexes, known_road, "the road only where it is known");
        let fogged = g.overlay().meshes().iter().flat_map(|m| &m.vertices).filter(|v| v.color == FOG).count();
        assert_eq!(fogged / HEX_VERTICES, view.visible_hexes().filter(|v| (7..9).contains(&v.hex.col)).count());
        // The cursor keeps its red border on a black hex.
        let cursor = center_of(&view, map.cursor);
        let on_cursor =
            black.iter().find(|h| (h[0].pos[0] - cursor[0]).abs() < 1e-4 && (h[0].pos[1] - cursor[1]).abs() < 1e-4);
        let border = on_cursor.expect("the cursor's hex is black")[0].border;
        assert_eq!([border[0], border[1], border[2]], CURSOR_BORDER);
    }

    #[test]
    fn a_disc_is_cut_where_its_hex_is_cut() {
        // A unit in the top row of an odd column: the hex has no upper half.
        let mut map = Map::new(20, 20, TileType::Grass).unwrap();
        let cost = map.acquire_move_cost(crate::state::MoveCost { grass: 1, ..Default::default() });
        let unit = map.new_unit(Side::Green, "Edge", 0, 1, 5, cost);
        let hex = Hex::new(0, 1);
        map.place_unit(unit, hex).unwrap();
        let mut view = MapView::new(20, 20, Hex::new(1, 1));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks::plain(RANGE_OVERLAY, &|_| false, None));
        let top = center_of(&view, hex)[1];
        let vertices: Vec<&FlatVertex> = g.discs().meshes().iter().flat_map(|m| m.vertices.iter()).collect();
        assert_eq!(vertices.len(), 2 * (DISC_CORNERS + 1));
        assert!(vertices.iter().all(|v| v.pos[1] <= top + 1e-6), "the disc stays below the map's edge");
        assert!(vertices.iter().any(|v| v.pos[1] < top - 0.01), "and its lower half is there");
    }

    #[test]
    fn the_overlay_covers_the_selected_units_range() {
        let mut game = crate::game::grassland();
        game.select_hex(START);
        let view = view_at(START);
        let g = geometry(&game, &view);
        let overlay: Vec<&[FlatVertex]> =
            g.overlay().meshes().iter().flat_map(|m| m.vertices.chunks(HEX_VERTICES)).collect();
        let in_view_and_reachable = view.visible_hexes().filter(|v| game.can_move_to(v.hex)).count();
        assert!(in_view_and_reachable > 100);
        assert_eq!(overlay.len(), in_view_and_reachable);
        assert!(overlay.iter().all(|hex| hex.iter().all(|v| v.color == RANGE_OVERLAY)));
        let centers: Vec<[f32; 2]> = overlay.iter().map(|hex| hex[0].pos).collect();
        // The hex next door is covered; the unit's own hex and mountains are not.
        assert!(centers.contains(&center_of(&view, Hex::new(50, 51))));
        assert!(!centers.contains(&center_of(&view, START)));
        assert!(!centers.contains(&center_of(&view, Hex::new(51, 51))));
        // After the move nothing is selected and the overlay is gone.
        game.select_hex(Hex::new(50, 53));
        assert!(geometry(&game, &view).overlay().is_empty());
    }

    #[test]
    fn edge_hexes_use_the_clipped_models() {
        let game = crate::game::grassland();
        let view = view_at(Hex::new(0, 0));
        let g = geometry(&game, &view);
        let find = |hex: Hex| {
            let at = center_of(&view, hex);
            TileType::ALL
                .into_iter()
                .flat_map(|t| hexes(g.terrain(t)))
                .find(|h| h[0].pos == at)
                .unwrap_or_else(|| panic!("{hex} is not drawn"))
        };
        let (wf, hf) = view.factors();
        let shape = |hex: Hex| -> Vec<[f32; 2]> {
            let h = find(hex);
            h.iter()
                .map(|v| [((v.pos[0] - h[0].pos[0]) / wf * 2.0).round() / 2.0, ((v.pos[1] - h[0].pos[1]) / hf).round()])
                .collect()
        };
        assert_eq!(shape(Hex::new(0, 0)), HexVariant::Left.positions());
        assert_eq!(shape(Hex::new(0, 1)), HexVariant::Top.positions());
        assert_eq!(shape(Hex::new(0, 2)), HexVariant::Full.positions());
        assert_eq!(shape(Hex::new(3, 0)), HexVariant::Left.positions());
        // The clipped texture coordinates come along.
        assert_eq!(find(Hex::new(0, 1)).iter().map(|v| v.uv).collect::<Vec<_>>(), HexVariant::Top.uvs());
        // Nothing off the map is drawn: no hex is centered left of column 0.
        let left_edge = center_of(&view, Hex::new(0, 0))[0];
        assert!(TileType::ALL.into_iter().flat_map(|t| hexes(g.terrain(t))).all(|h| h[0].pos[0] >= left_edge));
    }

    #[test]
    fn the_widest_view_fits_sixteen_bit_indices() {
        // Plan, section 3.4: at the widest zoom the layers stay far below 65,536
        // vertices each, so each is one draw call.
        let mut game = crate::game::grassland();
        game.select_hex(START);
        let mut view = view_at(START);
        while view.zoom_out() {}
        let g = geometry(&game, &view);
        let total: usize = TileType::ALL.into_iter().map(|t| g.terrain(t).vertex_count()).sum();
        assert_eq!(total, view.visible_hexes().count() * HEX_VERTICES);
        assert!(total < 65_536, "{total}");
        for t in TileType::ALL {
            assert!(g.terrain(t).meshes().len() <= 1);
        }
        assert_eq!(g.overlay().meshes().len(), 1);
    }

    #[test]
    fn rebuilding_replaces_the_last_frame() {
        let game = crate::game::grassland();
        let view = view_at(START);
        let mut g = MapGeometry::new();
        g.build(&game.map, &view, &Marks::plain(RANGE_OVERLAY, &|hex| game.can_move_to(hex), None));
        let first = g.terrain(TileType::Grass).vertex_count();
        g.build(&game.map, &view, &Marks::plain(RANGE_OVERLAY, &|hex| game.can_move_to(hex), None));
        assert_eq!(g.terrain(TileType::Grass).vertex_count(), first);
        assert_eq!(hexes(g.units_with(UNIT_IMAGE)).len(), 1);
    }
}
