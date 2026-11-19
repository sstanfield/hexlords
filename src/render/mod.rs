//! Drawing with miniquad: the hex models, the shaders, the textures, and the
//! renderers of the main map and the minimap; and, on the CPU, the units'
//! icons for the panels (`unit_icon`).
//!
//! The geometry is built on the CPU by code that needs no GPU (`hex_mesh`,
//! `MapGeometry`, `minimap_hex`), so `cargo test` covers it; only the code that
//! takes a `RenderingBackend` needs a window, and the screenshot mode shows what
//! that draws.
//!
//! C++: src/render/

pub mod hex_mesh;
pub mod map_renderer;
pub mod mesh;
pub mod minimap_image;
pub mod minimap_renderer;
pub mod png_out;
pub mod roads;
pub mod shaders;
pub mod textures;
pub mod unit_icon;

pub use hex_mesh::HexVariant;
pub use map_renderer::{
    CITY_SIDES, MapGeometry, MapRenderer, Marks, RANGE_OVERLAY, city_image, side_color, tile_image,
};
pub use minimap_image::minimap_image;
pub use minimap_renderer::MiniMapRenderer;
pub use shaders::Pipelines;
pub use textures::AssetDirs;
pub use unit_icon::unit_icon;

use turbo_ferret::Rect;

/// A rectangle in window pixels (top left origin, y down) as the `(x, y, w, h)`
/// that miniquad's `apply_viewport` and `apply_scissor_rect` take, which count y
/// from the bottom of the framebuffer. C++: src/gui/glwidget.cpp GLWidget::OnPaint
pub fn gl_rect(rect: Rect, framebuffer_height: i32) -> (i32, i32, i32, i32) {
    (rect.x, framebuffer_height - rect.bottom(), rect.w, rect.h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gl_rects_count_from_the_bottom() {
        // The whole framebuffer.
        assert_eq!(gl_rect(Rect::new(0, 0, 1024, 768), 768), (0, 0, 1024, 768));
        // The demo's map, in the top left corner: its bottom edge is 231 pixels up.
        assert_eq!(gl_rect(Rect::new(0, 0, 819, 537), 768), (0, 231, 819, 537));
        // The minimap, top right.
        assert_eq!(gl_rect(Rect::new(824, 0, 200, 231), 768), (824, 537, 200, 231));
        // Something at the bottom.
        assert_eq!(gl_rect(Rect::new(10, 700, 50, 68), 768), (10, 0, 50, 68));
    }
}
