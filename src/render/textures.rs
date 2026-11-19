//! PNG files to GPU textures: read through Turbo Ferret's resource loader, decoded
//! by Turbo Ferret's PNG decoder, uploaded clamped.
//!
//! C++: src/render/imageutils.cpp read_png_file (stb_image)

use miniquad::{
    FilterMode, MipmapFilterMode, RenderingBackend, TextureFormat, TextureId, TextureKind, TextureParams, TextureWrap,
};
use turbo_ferret::image::{Limits, png};
use turbo_ferret::{Image, ResourceLoader};

/// Read and decode a PNG file (a path relative to the asset directory) to
/// straight-alpha RGBA, top row first.
pub fn load_image(loader: &mut dyn ResourceLoader, path: &str) -> Result<Image, String> {
    let bytes = loader.load(path).map_err(|e| e.to_string())?;
    png::decode(&bytes, &Limits::DEFAULT).map_err(|e| format!("{path}: {e}"))
}

/// How a texture is read when it is drawn at another size than its own.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Sampling {
    /// The one nearest pixel: for pictures drawn a pixel for a pixel, and for
    /// render targets.
    Nearest,
    /// A mix of the pixels around the point, from a smaller copy of the
    /// picture when it is drawn small (mipmaps). The terrain tiles: a hex is
    /// anything from 20 to 270 pixels wide, and a 256 pixel tile read one
    /// pixel at a time shimmers when it is drawn small.
    ///
    /// A picture read this way should have no transparency next to what is
    /// drawn of it: the mix would take the nothing in.
    Smooth,
}

/// The parameters of a texture: RGBA, clamped, read as `sampling` says.
pub fn sampled_params(width: u32, height: u32, sampling: Sampling) -> TextureParams {
    let (filter, mipmap_filter) = match sampling {
        Sampling::Nearest => (FilterMode::Nearest, MipmapFilterMode::None),
        Sampling::Smooth => (FilterMode::Linear, MipmapFilterMode::Linear),
    };
    TextureParams {
        kind: TextureKind::Texture2D,
        format: TextureFormat::RGBA8,
        wrap: TextureWrap::Clamp,
        min_filter: filter,
        mag_filter: filter,
        mipmap_filter,
        width,
        height,
        allocate_mipmaps: sampling == Sampling::Smooth,
        sample_count: 1,
    }
}

/// The parameters of a texture without mipmaps, read by the nearest pixel:
/// what the render targets are made with.
pub fn texture_params(width: u32, height: u32) -> TextureParams {
    sampled_params(width, height, Sampling::Nearest)
}

/// Upload an image. Its first row is the texture's v = 0, as in the C++.
pub fn upload(ctx: &mut dyn RenderingBackend, image: &Image, sampling: Sampling) -> TextureId {
    let params = sampled_params(image.size.w as u32, image.size.h as u32, sampling);
    let texture = ctx.new_texture_from_data_and_format(&image.rgba, params);
    if sampling == Sampling::Smooth {
        ctx.texture_generate_mipmaps(texture);
    }
    texture
}

/// Load a PNG file as a texture.
pub fn load_texture(
    ctx: &mut dyn RenderingBackend,
    loader: &mut dyn ResourceLoader,
    path: &str,
    sampling: Sampling,
) -> Result<TextureId, String> {
    let image = load_image(loader, path)?;
    if image.size.w <= 0 || image.size.h <= 0 {
        return Err(format!("{path}: an empty image"));
    }
    Ok(upload(ctx, &image, sampling))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tile_image;
    use crate::state::TileType;
    use turbo_ferret::FsLoader;

    fn loader() -> FsLoader {
        FsLoader::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"))
    }

    #[test]
    fn every_tile_and_the_unit_decode() {
        let mut loader = loader();
        let tiles = TileType::ALL.into_iter().map(|kind| (tile_image(kind), 256));
        for (path, size) in tiles.chain([("units/default_unit.png".to_string(), 64)]) {
            let image = load_image(&mut loader, &path).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!((image.size.w, image.size.h), (size, size), "{path}");
            assert_eq!(image.rgba.len(), (size * size * 4) as usize, "{path}");
        }
    }

    #[test]
    fn the_unit_image_is_partly_transparent_and_tiles_are_not() {
        let mut loader = loader();
        let unit = load_image(&mut loader, "units/default_unit.png").unwrap();
        let alphas = |image: &Image| image.rgba.chunks(4).map(|p| p[3]).collect::<Vec<u8>>();
        assert!(alphas(&unit).contains(&0), "the unit is drawn over the terrain");
        assert!(alphas(&unit).contains(&255));
        // The tiles are scaled smoothly, which mixes a pixel with its
        // neighbours: there must be no nothing among them, not even outside
        // the hex. And their smaller copies need sides that halve evenly.
        for kind in TileType::ALL {
            let tile = load_image(&mut loader, &tile_image(kind)).unwrap();
            assert!(alphas(&tile).iter().all(|&a| a == 255), "{kind:?} has transparent pixels");
            assert!((tile.size.w as u32).is_power_of_two() && tile.size.w == tile.size.h, "{kind:?}");
        }
    }

    #[test]
    fn tiles_are_scaled_smoothly_and_the_rest_by_the_nearest_pixel() {
        let smooth = sampled_params(256, 256, Sampling::Smooth);
        assert_eq!((smooth.min_filter, smooth.mag_filter), (FilterMode::Linear, FilterMode::Linear));
        assert_eq!(smooth.mipmap_filter, MipmapFilterMode::Linear);
        assert!(smooth.allocate_mipmaps);
        let nearest = texture_params(100, 60);
        assert_eq!((nearest.min_filter, nearest.mag_filter), (FilterMode::Nearest, FilterMode::Nearest));
        assert_eq!(nearest.mipmap_filter, MipmapFilterMode::None);
        assert!(!nearest.allocate_mipmaps);
        assert_eq!((nearest.width, nearest.height), (100, 60));
    }

    #[test]
    fn a_missing_or_broken_file_is_an_error_that_names_it() {
        let mut loader = loader();
        let err = load_image(&mut loader, "tiles/no_such_tile.png").unwrap_err();
        assert!(err.contains("no_such_tile.png"), "{err}");
        // Not a PNG.
        let err = load_image(&mut loader, "skin/skin.tb.txt").unwrap_err();
        assert!(err.contains("skin.tb.txt"), "{err}");
    }
}
