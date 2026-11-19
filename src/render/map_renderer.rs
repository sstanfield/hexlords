//! The main map: the visible hexes with their terrain textures and borders,
//! the roads over them, the units (a disc of their side's color and their
//! kind's picture), a banner on a city that has an owner, and the overlay on
//! the hexes the caller marks (in the game: where the selected unit can move).
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
use crate::state::{DEFAULT_UNIT_IMAGE, Hex, Map, Side, TileType, UnitId};
use crate::view::{HEX_HEIGHT_ADJUST, MapView};

/// The border of the hex under the cursor. C++: drawHex (`isSelected`)
pub const CURSOR_BORDER: [f32; 3] = [1.0, 0.0, 0.0];
/// The border of every other hex.
pub const HEX_BORDER: [f32; 3] = [0.0, 0.0, 0.0];
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
const DISC_RING: f32 = 0.05;
const RING_COLOR: [f32; 4] = [0.08, 0.07, 0.07, 1.0];
/// The banner of a city's owner: a small disc toward the hex's upper right.
const BANNER_AT: [f32; 2] = [0.42, 0.66];
const BANNER_RADIUS: f32 = 0.16;
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
    /// one). Of every other hex the first unit shows.
    pub front: Option<UnitId>,
}

/// The image a terrain is drawn with, relative to the asset directory: every
/// terrain has its own, made by `tools/assetgen` (`docs/tiles.md`).
pub fn tile_image(kind: TileType) -> String {
    format!("tiles/{}_tile.png", kind.name())
}

/// How many terrains, and so tile images, there are.
const TERRAINS: usize = TileType::ALL.len();

/// One frame of the main map as triangles, in the widget's normalized device
/// coordinates.
#[derive(Default)]
pub struct MapGeometry {
    /// The terrain, a mesh for each kind's image (indexed like `TileType::ALL`).
    terrain: [MeshList<HexVertex>; TERRAINS],
    /// The roads, a mesh for each picture (`roads::picture`).
    roads: Vec<MeshList<HexVertex>>,
    /// The discs under the units and the cities' banners.
    discs: MeshList<FlatVertex>,
    /// The hexes that hold units, by the image of the unit that is shown.
    units: Vec<(String, MeshList<HexVertex>)>,
    /// The marked hexes.
    overlay: MeshList<FlatVertex>,
    /// What `units_with` gives for an image nothing is drawn with.
    none: MeshList<HexVertex>,
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

impl MapGeometry {
    pub fn new() -> MapGeometry {
        MapGeometry::default()
    }

    /// Rebuild for the current map and view. `marks` says which hexes get a
    /// tint, and which: where the selected unit can go, in the game.
    /// C++: MapRenderer::renderMap, drawHex
    pub fn build(&mut self, map: &Map, view: &MapView, marks: &Marks) {
        for list in &mut self.terrain {
            list.clear();
        }
        self.roads.resize_with(ROAD_PICTURES, MeshList::new);
        for list in &mut self.roads {
            list.clear();
        }
        self.discs.clear();
        for (_, list) in &mut self.units {
            list.clear();
        }
        self.overlay.clear();

        let (rows, cols) = (map.rows(), map.cols());
        for visible in view.visible_hexes() {
            let hex = visible.hex;
            let Some(tile) = map.tile(hex) else { continue };
            let variant = HexVariant::choose(hex.row, hex.col, rows, cols);
            let (positions, uvs) = (variant.positions(), variant.uvs());
            let is_cursor = hex == map.cursor;
            let [r, g, b] = if is_cursor { CURSOR_BORDER } else { HEX_BORDER };
            let border = [r, g, b, view.border_threshold(is_cursor)];
            let corner =
                |i: usize| view.to_ndc([positions[i][0] + visible.center[0], positions[i][1] + visible.center[1]]);

            let vertices: [HexVertex; HEX_VERTICES] =
                std::array::from_fn(|i| HexVertex { pos: corner(i), uv: uvs[i], bary: HEX_BARY[i], border });
            self.terrain[tile.kind.index()].push(&vertices, &HEX_INDICES);

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

            // A city's banner, in its owner's color.
            if tile.kind == TileType::City {
                if let Some(city) = map.city_at(hex).filter(|c| c.side != Side::Grey) {
                    let [r, g, b] = side_color(city.side);
                    let ring = BANNER_RADIUS + DISC_RING * 0.8;
                    push_disc(&mut self.discs, view, visible.center, variant, BANNER_AT, ring, RING_COLOR);
                    push_disc(&mut self.discs, view, visible.center, variant, BANNER_AT, BANNER_RADIUS, [r, g, b, 1.0]);
                }
            }

            // The unit that shows (the selected one if it stands here, else
            // the first): a disc of its side's color, and on it the picture
            // of its kind.
            let shown = marks.front.filter(|id| tile.units().contains(id)).or(tile.units().first().copied());
            if let Some(unit) = shown.and_then(|id| map.units().get(id)) {
                let [r, g, b] = side_color(unit.side());
                let middle = [0.0, 0.0];
                push_disc(&mut self.discs, view, visible.center, variant, middle, DISC_RADIUS + DISC_RING, RING_COLOR);
                push_disc(&mut self.discs, view, visible.center, variant, middle, DISC_RADIUS, [r, g, b, 1.0]);
                let image = map.units().kind_of(unit).map_or(UNIT_IMAGE, |kind| kind.image.as_str());
                let sign: [HexVertex; HEX_VERTICES] =
                    std::array::from_fn(|i| HexVertex { border: NO_BORDER, ..vertices[i] });
                let layer = match self.units.iter().position(|(name, _)| name == image) {
                    Some(i) => i,
                    None => {
                        self.units.push((image.to_string(), MeshList::new()));
                        self.units.len() - 1
                    }
                };
                self.units[layer].1.push(&sign, &HEX_INDICES);
            }

            if (marks.marked)(hex) {
                let flat: [FlatVertex; HEX_VERTICES] =
                    std::array::from_fn(|i| FlatVertex { pos: corner(i), color: marks.color });
                self.overlay.push(&flat, &HEX_INDICES);
            }
        }
    }

    /// The hexes of one terrain.
    pub fn terrain(&self, kind: TileType) -> &MeshList<HexVertex> {
        &self.terrain[kind.index()]
    }

    /// The hexes whose road is drawn with one picture (`roads::picture`).
    pub fn roads(&self, picture: usize) -> &MeshList<HexVertex> {
        self.roads.get(picture).unwrap_or(&self.none)
    }

    /// The discs under the units and the cities' banners, each with its dark
    /// ring first.
    pub fn discs(&self) -> &MeshList<FlatVertex> {
        &self.discs
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
    /// Indexed by `roads::picture`.
    roads: Vec<TextureId>,
    /// The units' pictures that have been asked for, by image; `None` for one
    /// that could not be read (the default unit's is drawn instead).
    units: HashMap<String, Option<TextureId>>,
    /// The picture of a unit without one.
    unit: TextureId,
    geometry: MapGeometry,
    buffers: BufferPool,
}

/// The textures every map needs: the tiles, the roads, the default unit.
fn load_all(
    ctx: &mut dyn RenderingBackend,
    dirs: &AssetDirs,
) -> Result<(Vec<TextureId>, Vec<TextureId>, TextureId), String> {
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
    let mut road_pictures = Vec::with_capacity(ROAD_PICTURES);
    for picture in 0..ROAD_PICTURES {
        road_pictures.push(load(ctx, &roads::road_image(picture), Sampling::Smooth)?);
    }
    let unit = load(ctx, UNIT_IMAGE, Sampling::Nearest)?;
    Ok((tiles, road_pictures, unit))
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
        let (tiles, roads, unit) = load_all(ctx, &dirs)?;
        Ok(MapRenderer {
            dirs,
            tiles,
            roads,
            units: HashMap::new(),
            unit,
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
        let (tiles, roads, unit) = load_all(ctx, &dirs)?;
        self.delete_textures(ctx);
        (self.dirs, self.tiles, self.roads, self.unit) = (dirs, tiles, roads, unit);
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
                eprintln!("a unit's picture: {e}");
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
        if !geometry.overlay().is_empty() {
            ctx.apply_pipeline(&pipelines.flat);
            draw_flat(ctx, buffers, geometry.overlay());
        }
    }

    fn delete_textures(&mut self, ctx: &mut dyn RenderingBackend) {
        let units = self.units.drain().filter_map(|(_, texture)| texture);
        for texture in self.tiles.drain(..).chain(self.roads.drain(..)).chain(units).chain([self.unit]) {
            ctx.delete_texture(texture);
        }
    }

    /// Free the GPU objects.
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        self.delete_textures(ctx);
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
        g.build(&game.map, view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex), front: None });
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
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });
        for (col, kind) in TileType::ALL.into_iter().enumerate() {
            let want = center_of(&view, Hex::new(10, col as u16 + 5));
            let drawn = hexes(g.terrain(kind));
            assert!(drawn.iter().any(|h| h[0].pos == want), "{kind:?}");
            if kind != TileType::Water {
                assert_eq!(drawn.len(), 1, "{kind:?}");
            }
        }
    }

    #[test]
    fn the_start_view_is_grass_with_a_mountain_diagonal_and_one_unit() {
        let game = Game::demo();
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
        let game = Game::demo();
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
        let mut game = Game::demo();
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
    fn a_unit_is_a_disc_of_its_sides_color_and_its_picture_on_the_hex() {
        let game = Game::demo();
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
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });

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

    #[test]
    fn units_are_drawn_by_their_kinds_picture_and_owned_cities_fly_a_banner() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[10] = "gggggcggggcgggcggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit a\nimage units/militia.png\nmoves 5\nenter grass 1\nunit b\nimage units/riders.png\nmoves 5\nenter grass 1\nunit c\nmoves 5\nenter grass 1\n";
        let cities = "city 10 5\nname One\nside red\ngarrison b\ngarrison a\n\
                      city 10 10\nname Two\nside blue\ngarrison a 2\n\
                      city 10 14\nname Three\ngarrison c\n";
        let (map, _) = crate::state::scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let mut view = MapView::new(20, 20, Hex::new(10, 10));
        view.resize(819, 537);
        let mut g = MapGeometry::new();
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });

        // The first unit of a hex is the one that shows.
        let at = |image: &str| -> Vec<[f32; 2]> { hexes(g.units_with(image)).iter().map(|h| h[0].pos).collect() };
        assert_eq!(at("units/riders.png"), [center_of(&view, Hex::new(10, 5))]);
        assert_eq!(at("units/militia.png"), [center_of(&view, Hex::new(10, 10))]);
        assert_eq!(at(UNIT_IMAGE), [center_of(&view, Hex::new(10, 14))], "a kind without a picture");
        assert_eq!(g.units().count(), 3);

        // Discs: a ring and a color for each unit, and for each banner.
        let colors: Vec<[f32; 4]> =
            g.discs().meshes().iter().flat_map(|m| m.vertices.chunks(DISC_CORNERS + 1)).map(|d| d[0].color).collect();
        let solid = |side: Side| {
            let [r, g, b] = side_color(side);
            [r, g, b, 1.0]
        };
        let count = |color: [f32; 4]| colors.iter().filter(|c| **c == color).count();
        assert_eq!(count(solid(Side::Red)), 2, "the banner and the unit's disc");
        assert_eq!(count(solid(Side::Blue)), 2);
        assert_eq!(count(solid(Side::Grey)), 1, "nobody's city has no banner");
        assert_eq!(count(RING_COLOR), 5);
        assert_eq!(colors.len(), 10);
        // The sides can be told apart.
        let sides: Vec<[f32; 3]> = Side::PLAYERS.into_iter().chain([Side::Grey]).map(side_color).collect();
        for (i, a) in sides.iter().enumerate() {
            for b in &sides[i + 1..] {
                assert!((0..3).any(|k| (a[k] - b[k]).abs() > 0.15), "{a:?} {b:?}");
            }
        }

        // The selected unit shows on its hex instead of the first: in the red
        // city the militia stands behind the rider.
        let militia = map.units_at(Hex::new(10, 5))[1];
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: Some(militia) });
        assert!(g.units_with("units/riders.png").is_empty());
        assert_eq!(at_all(&g, "units/militia.png"), 2);
        // A unit that is not on a hex changes nothing there.
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });
        assert_eq!(at_all(&g, "units/riders.png"), 1);

        // The map changes and the pictures follow: the rider is gone.
        let mut map = map;
        let rider = map.units_at(Hex::new(10, 5))[0];
        map.kill_unit(rider);
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });
        assert!(g.units_with("units/riders.png").is_empty());
        assert_eq!(g.units().count(), 2);
        assert_eq!(at_all(&g, "units/militia.png"), 2);
    }

    fn at_all(g: &MapGeometry, image: &str) -> usize {
        hexes(g.units_with(image)).len()
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
        g.build(&map, &view, &Marks { color: RANGE_OVERLAY, marked: &|_| false, front: None });
        let top = center_of(&view, hex)[1];
        let vertices: Vec<&FlatVertex> = g.discs().meshes().iter().flat_map(|m| m.vertices.iter()).collect();
        assert_eq!(vertices.len(), 2 * (DISC_CORNERS + 1));
        assert!(vertices.iter().all(|v| v.pos[1] <= top + 1e-6), "the disc stays below the map's edge");
        assert!(vertices.iter().any(|v| v.pos[1] < top - 0.01), "and its lower half is there");
    }

    #[test]
    fn the_overlay_covers_the_selected_units_range() {
        let mut game = Game::demo();
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
        let game = Game::demo();
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
        let mut game = Game::demo();
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
        let game = Game::demo();
        let view = view_at(START);
        let mut g = MapGeometry::new();
        g.build(&game.map, &view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex), front: None });
        let first = g.terrain(TileType::Grass).vertex_count();
        g.build(&game.map, &view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex), front: None });
        assert_eq!(g.terrain(TileType::Grass).vertex_count(), first);
        assert_eq!(hexes(g.units_with(UNIT_IMAGE)).len(), 1);
    }
}
