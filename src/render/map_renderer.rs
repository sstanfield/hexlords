//! The main map: the visible hexes with their terrain textures and borders, the
//! unit image on hexes that hold units, and the overlay on the hexes the caller
//! marks (in the game: where the selected unit can move to).
//!
//! The geometry is built on the CPU each frame (`MapGeometry`, which needs no
//! GPU and is tested without one) and drawn in three layers: the terrain, one
//! mesh per texture; the units; the overlay. The C++ drew hex by hex, each with
//! up to three draw calls; hexes do not overlap, so the layers look the same.
//!
//! C++: src/render/maprender.cpp MapRenderer (renderMap, CTX::drawHex,
//! CTX::getTileTexture)

use miniquad::{RenderingBackend, TextureId};
use turbo_ferret::ResourceLoader;

use super::hex_mesh::{HEX_BARY, HEX_INDICES, HEX_VERTICES, HexVariant};
use super::mesh::{BufferPool, MeshList};
use super::shaders::{FlatVertex, HexVertex, Pipelines};
use super::textures::load_texture;
use crate::state::{Hex, Map, TileType};
use crate::view::MapView;

/// The border of the hex under the cursor. C++: drawHex (`isSelected`)
pub const CURSOR_BORDER: [f32; 3] = [1.0, 0.0, 0.0];
/// The border of every other hex.
pub const HEX_BORDER: [f32; 3] = [0.0, 0.0, 0.0];
/// Laid over the hexes a selected unit can move to. C++: HexOverlayFragment.glsl
pub const RANGE_OVERLAY: [f32; 4] = [1.0, 0.0, 0.0, 0.3];
/// Drawn over a hex that holds units.
pub const UNIT_IMAGE: &str = "units/default_unit.png";

/// The hexes of the map that get a tint laid over them.
pub struct Marks<'a> {
    /// The tint: red, green, blue and how much of it.
    pub color: [f32; 4],
    pub marked: &'a dyn Fn(Hex) -> bool,
}

/// The texture a terrain is drawn with.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TileTexture {
    Grass,
    Forest,
    Swamp,
    Hill,
    Mountain,
    Water,
    City,
}

impl TileTexture {
    pub const ALL: [TileTexture; 7] = [
        TileTexture::Grass,
        TileTexture::Forest,
        TileTexture::Swamp,
        TileTexture::Hill,
        TileTexture::Mountain,
        TileTexture::Water,
        TileTexture::City,
    ];

    /// Cities, temples, ruins and deserts all use the city tile, as in the C++
    /// (which marked the last three "XXX"). C++: CTX::getTileTexture
    pub fn of(kind: TileType) -> TileTexture {
        match kind {
            TileType::Grass => TileTexture::Grass,
            TileType::Forest => TileTexture::Forest,
            TileType::Swamp => TileTexture::Swamp,
            TileType::Hill => TileTexture::Hill,
            TileType::Mountain => TileTexture::Mountain,
            TileType::Water => TileTexture::Water,
            TileType::City | TileType::Temple | TileType::Ruin | TileType::Desert => TileTexture::City,
        }
    }

    /// The image file, relative to the asset directory.
    pub fn path(self) -> &'static str {
        match self {
            TileTexture::Grass => "tiles/grass_tile.png",
            TileTexture::Forest => "tiles/forest_tile.png",
            TileTexture::Swamp => "tiles/swamp_tile.png",
            TileTexture::Hill => "tiles/hill_tile.png",
            TileTexture::Mountain => "tiles/mountain_tile.png",
            TileTexture::Water => "tiles/water_tile.png",
            TileTexture::City => "tiles/citycenter_tile.png",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// One frame of the main map as triangles, in the widget's normalized device
/// coordinates.
#[derive(Default)]
pub struct MapGeometry {
    /// The terrain, by texture (indexed like `TileTexture::ALL`).
    terrain: [MeshList<HexVertex>; 7],
    /// The hexes that hold units, to be drawn with the unit image.
    units: MeshList<HexVertex>,
    /// The marked hexes.
    overlay: MeshList<FlatVertex>,
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
        self.units.clear();
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
            self.terrain[TileTexture::of(tile.kind).index()].push(&vertices, &HEX_INDICES);
            if !tile.units().is_empty() {
                self.units.push(&vertices, &HEX_INDICES);
            }
            if (marks.marked)(hex) {
                let flat: [FlatVertex; HEX_VERTICES] =
                    std::array::from_fn(|i| FlatVertex { pos: corner(i), color: marks.color });
                self.overlay.push(&flat, &HEX_INDICES);
            }
        }
    }

    /// The terrain drawn with `texture`.
    pub fn terrain(&self, texture: TileTexture) -> &MeshList<HexVertex> {
        &self.terrain[texture.index()]
    }

    pub fn units(&self) -> &MeshList<HexVertex> {
        &self.units
    }

    pub fn overlay(&self) -> &MeshList<FlatVertex> {
        &self.overlay
    }
}

/// Draws the main map into the current viewport.
pub struct MapRenderer {
    /// Indexed like `TileTexture::ALL`.
    tiles: [TextureId; 7],
    unit: TextureId,
    geometry: MapGeometry,
    buffers: BufferPool,
}

impl MapRenderer {
    /// Load the tile and unit images. C++: MapRenderer::MapRenderer
    pub fn new(ctx: &mut dyn RenderingBackend, loader: &mut dyn ResourceLoader) -> Result<MapRenderer, String> {
        let mut tiles = Vec::with_capacity(TileTexture::ALL.len());
        for texture in TileTexture::ALL {
            tiles.push(load_texture(ctx, loader, texture.path())?);
        }
        let unit = load_texture(ctx, loader, UNIT_IMAGE)?;
        let tiles: [TextureId; 7] = tiles.try_into().map_err(|_| "tile textures".to_string())?;
        Ok(MapRenderer { tiles, unit, geometry: MapGeometry::new(), buffers: BufferPool::new() })
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

        // All the terrain, then the units, then the overlay.
        ctx.apply_pipeline(&pipelines.hex);
        let textured = TileTexture::ALL
            .into_iter()
            .map(|t| (self.geometry.terrain(t), self.tiles[t.index()]))
            .chain([(self.geometry.units(), self.unit)]);
        for (list, texture) in textured {
            for mesh in list.meshes() {
                let (vb, ib) = self.buffers.upload(ctx, mesh);
                ctx.apply_bindings_from_slice(&[vb], ib, &[texture]);
                ctx.draw(0, mesh.indices.len() as i32, 1);
            }
        }
        if !self.geometry.overlay().is_empty() {
            ctx.apply_pipeline(&pipelines.flat);
            for mesh in self.geometry.overlay().meshes() {
                let (vb, ib) = self.buffers.upload(ctx, mesh);
                ctx.apply_bindings_from_slice(&[vb], ib, &[]);
                ctx.draw(0, mesh.indices.len() as i32, 1);
            }
        }
    }

    /// Free the GPU objects.
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        for texture in self.tiles.into_iter().chain([self.unit]) {
            ctx.delete_texture(texture);
        }
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
        g.build(&game.map, view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex) });
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
    fn terrain_textures() {
        assert_eq!(TileTexture::of(TileType::Grass), TileTexture::Grass);
        assert_eq!(TileTexture::of(TileType::Mountain), TileTexture::Mountain);
        for kind in [TileType::City, TileType::Temple, TileType::Ruin, TileType::Desert] {
            assert_eq!(TileTexture::of(kind), TileTexture::City, "{kind:?}");
        }
        for (i, t) in TileTexture::ALL.into_iter().enumerate() {
            assert_eq!(t.index(), i);
            assert!(t.path().starts_with("tiles/") && t.path().ends_with("_tile.png"));
        }
    }

    #[test]
    fn the_start_view_is_grass_with_a_mountain_diagonal_and_one_unit() {
        let game = Game::demo();
        let view = view_at(START);
        let g = geometry(&game, &view);
        let visible = view.visible_hexes().count();
        let grass = hexes(g.terrain(TileTexture::Grass)).len();
        let mountain = hexes(g.terrain(TileTexture::Mountain)).len();
        assert_eq!(grass + mountain, visible);
        // Rows 43..=57 are drawn: one mountain in each.
        assert_eq!(mountain, 15);
        for t in [TileTexture::Forest, TileTexture::Swamp, TileTexture::Hill, TileTexture::Water, TileTexture::City] {
            assert!(g.terrain(t).is_empty(), "{t:?}");
        }
        // One hex holds a unit, in the middle; nothing is selected.
        let units = hexes(g.units());
        assert_eq!(units.len(), 1);
        assert_eq!(units[0][0].pos, [0.0, 0.0]);
        assert!(g.overlay().is_empty());
        // Six triangles a hex.
        assert_eq!(g.terrain(TileTexture::Grass).index_count(), grass * 18);
    }

    #[test]
    fn hexes_are_placed_by_the_view() {
        let game = Game::demo();
        let view = view_at(START);
        let g = geometry(&game, &view);
        let (wf, hf) = view.factors();
        for hex in hexes(g.terrain(TileTexture::Grass)) {
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
        assert!(hexes(g.terrain(TileTexture::Grass)).iter().any(|h| h[0].pos == want));
    }

    #[test]
    fn the_cursor_hex_has_the_red_border_and_the_others_black() {
        let mut game = Game::demo();
        game.set_cursor(Hex::new(50, 52));
        let view = view_at(START);
        let g = geometry(&game, &view);
        let cursor_at = center_of(&view, Hex::new(50, 52));
        let mut red = 0;
        for t in TileTexture::ALL {
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
    fn the_unit_layer_repeats_the_hexes_vertices() {
        let game = Game::demo();
        let view = view_at(START);
        let g = geometry(&game, &view);
        // The unit stands on a mountain: the same seven vertices are in both layers.
        let unit = hexes(g.units())[0];
        assert!(hexes(g.terrain(TileTexture::Mountain)).contains(&unit));
        // It is under the cursor at the start, so it carries the red border.
        assert_eq!(unit[0].border[..3], CURSOR_BORDER);
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
            TileTexture::ALL
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
        assert!(TileTexture::ALL.into_iter().flat_map(|t| hexes(g.terrain(t))).all(|h| h[0].pos[0] >= left_edge));
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
        let total: usize = TileTexture::ALL.into_iter().map(|t| g.terrain(t).vertex_count()).sum();
        assert_eq!(total, view.visible_hexes().count() * HEX_VERTICES);
        assert!(total < 65_536, "{total}");
        for t in TileTexture::ALL {
            assert!(g.terrain(t).meshes().len() <= 1);
        }
        assert_eq!(g.overlay().meshes().len(), 1);
    }

    #[test]
    fn rebuilding_replaces_the_last_frame() {
        let game = Game::demo();
        let view = view_at(START);
        let mut g = MapGeometry::new();
        g.build(&game.map, &view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex) });
        let first = g.terrain(TileTexture::Grass).vertex_count();
        g.build(&game.map, &view, &Marks { color: RANGE_OVERLAY, marked: &|hex| game.can_move_to(hex) });
        assert_eq!(g.terrain(TileTexture::Grass).vertex_count(), first);
        assert_eq!(hexes(g.units()).len(), 1);
    }
}
