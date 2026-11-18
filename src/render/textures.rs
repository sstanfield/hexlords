//! PNG files to GPU textures: read through Turbo Ferret's resource loader, decoded
//! by Turbo Ferret's PNG decoder, uploaded with nearest filtering and clamping.
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

/// The parameters of every texture the game makes: RGBA, nearest, clamped, no
/// mipmaps.
pub fn texture_params(width: u32, height: u32) -> TextureParams {
    TextureParams {
        kind: TextureKind::Texture2D,
        format: TextureFormat::RGBA8,
        wrap: TextureWrap::Clamp,
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::None,
        width,
        height,
        allocate_mipmaps: false,
        sample_count: 1,
    }
}

/// Upload an image. Its first row is the texture's v = 0, as in the C++.
pub fn upload(ctx: &mut dyn RenderingBackend, image: &Image) -> TextureId {
    ctx.new_texture_from_data_and_format(&image.rgba, texture_params(image.size.w as u32, image.size.h as u32))
}

/// Load a PNG file as a texture.
pub fn load_texture(
    ctx: &mut dyn RenderingBackend,
    loader: &mut dyn ResourceLoader,
    path: &str,
) -> Result<TextureId, String> {
    let image = load_image(loader, path)?;
    if image.size.w <= 0 || image.size.h <= 0 {
        return Err(format!("{path}: an empty image"));
    }
    Ok(upload(ctx, &image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use turbo_ferret::FsLoader;

    fn loader() -> FsLoader {
        FsLoader::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"))
    }

    #[test]
    fn every_tile_and_the_unit_decode() {
        let mut loader = loader();
        for (path, size) in [
            ("tiles/grass_tile.png", 64),
            ("tiles/forest_tile.png", 256),
            ("tiles/swamp_tile.png", 256),
            ("tiles/hill_tile.png", 256),
            ("tiles/mountain_tile.png", 64),
            ("tiles/water_tile.png", 256),
            ("tiles/citycenter_tile.png", 256),
            ("units/default_unit.png", 64),
        ] {
            let image = load_image(&mut loader, path).unwrap_or_else(|e| panic!("{e}"));
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
        let grass = load_image(&mut loader, "tiles/grass_tile.png").unwrap();
        assert!(alphas(&grass).iter().all(|&a| a == 255));
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
