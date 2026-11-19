//! `assetgen`: draw the terrain tiles, the roads and the units' signs, and
//! write them as PNG files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use assetgen::icons::{self, Icon};
use assetgen::overlay;
use assetgen::png::encode_rgba;
use assetgen::preview::previews;
use assetgen::road;
use assetgen::scatter::Features;
use assetgen::terrain::Terrain;
use assetgen::tile::{Edge, SIZES, atlas, render_all};

const USAGE: &str = "\
usage: assetgen [--out <dir>] [--size <pixels>] [--cutout] [--atlas] [--previews <dir>]

Draws the game's pictures and writes them into an asset directory:
  tiles/<terrain>_tile.png          the ten terrains
  roads/road_NN.png, bridge_NN.png  a road over land and over water, in each
                                    of its 14 shapes
  units/<name>.png                  the units' signs

  --out <dir>        the asset directory (default: assets, the game's)
  --size <pixels>    the side of a tile and of a road (default: 256, what the
                     game ships; the signs are half that)
  --cutout           leave what is outside the hex transparent in the tiles
                     (the game's are filled to the corners; see docs/tiles.md)
  --atlas            also write tiles/atlas.png and atlas.txt: all tiles in one
  --previews <dir>   also draw maps with the pictures into <dir>, to look at";

/// The game's assets, when run from the repository's root.
const DEFAULT_OUT: &str = "assets";
const DEFAULT_SIZE: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Args {
    out: PathBuf,
    size: usize,
    edge: Edge,
    atlas: bool,
    previews: Option<PathBuf>,
}

impl Args {
    /// Read the command line (without the program's name). `Ok(None)`: help
    /// was asked for.
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
        let mut parsed = Args {
            out: PathBuf::from(DEFAULT_OUT),
            size: DEFAULT_SIZE,
            edge: Edge::Filled,
            atlas: false,
            previews: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
            match arg.as_str() {
                "--out" => parsed.out = PathBuf::from(value("a directory")?),
                "--size" => {
                    let text = value("a number of pixels")?;
                    let size: usize = text.parse().map_err(|_| format!("--size: \"{text}\" is not a number"))?;
                    if !SIZES.contains(&size) {
                        return Err(format!("--size: from {} to {} pixels", SIZES.start(), SIZES.end()));
                    }
                    parsed.size = size;
                }
                "--cutout" => parsed.edge = Edge::Cutout,
                "--atlas" => parsed.atlas = true,
                "--previews" => parsed.previews = Some(PathBuf::from(value("a directory")?)),
                "--help" | "-h" => return Ok(None),
                _ => return Err(format!("unknown argument {arg}")),
            }
        }
        Ok(Some(parsed))
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) -> Result<(), String> {
    let png = encode_rgba(w, h, rgba).ok_or(format!("{}: could not encode the picture", path.display()))?;
    write(path, &png)
}

fn make_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

fn run(args: &Args) -> Result<(), String> {
    let features = Features::new();
    let size = args.size;

    let dir = args.out.join("tiles");
    make_dir(&dir)?;
    let tiles = render_all(size, args.edge, &features);
    for (terrain, tile) in Terrain::ALL.into_iter().zip(&tiles) {
        write_png(&dir.join(format!("{}_tile.png", terrain.name())), size, size, tile)?;
    }
    println!("wrote {} tiles of {size} x {size} pixels to {}", tiles.len(), dir.display());
    if args.atlas {
        let atlas = atlas(&tiles, size, "atlas.png");
        write_png(&dir.join("atlas.png"), atlas.w, atlas.h, &atlas.rgba)?;
        write(&dir.join("atlas.txt"), atlas.text.as_bytes())?;
        println!("wrote atlas.png ({} x {}) and atlas.txt to {}", atlas.w, atlas.h, dir.display());
    }

    let dir = args.out.join("roads");
    make_dir(&dir)?;
    let shapes = road::shapes();
    for shape in &shapes {
        let land = overlay::render(size, road::ROAD_RIM, &road::road(*shape));
        write_png(&dir.join(road::file_name(false, *shape)), size, size, &land)?;
        let water = overlay::render(size, road::BRIDGE_RIM, &road::bridge(*shape));
        write_png(&dir.join(road::file_name(true, *shape)), size, size, &water)?;
    }
    println!("wrote {} roads and as many bridges to {}", shapes.len(), dir.display());

    let dir = args.out.join("units");
    make_dir(&dir)?;
    let side = (size / 2).max(*SIZES.start());
    for icon in Icon::ALL {
        let picture = overlay::render(side, icons::OUTLINE, &|x, y| icon.ink(x, y));
        write_png(&dir.join(format!("{}.png", icon.name())), side, side, &picture)?;
    }
    println!("wrote {} units' signs of {side} x {side} pixels to {}", Icon::ALL.len(), dir.display());

    if let Some(dir) = &args.previews {
        make_dir(dir)?;
        let previews = previews(&features);
        for picture in &previews.pictures {
            write_png(&dir.join(picture.name), picture.w, picture.h, &picture.rgba)?;
        }
        for (name, text) in &previews.maps {
            write(&dir.join(name), text.as_bytes())?;
        }
        println!("wrote {} previews to {}", previews.pictures.len(), dir.display());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("assetgen: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("assetgen: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_make_the_games_tiles() {
        let args = parse(&[]).unwrap().unwrap();
        assert_eq!(
            args,
            Args { out: PathBuf::from("assets"), size: 256, edge: Edge::Filled, atlas: false, previews: None }
        );
    }

    #[test]
    fn every_option() {
        let args = parse(&["--out", "x/y", "--size", "64", "--cutout", "--atlas", "--previews", "p"]).unwrap().unwrap();
        assert_eq!(
            args,
            Args {
                out: PathBuf::from("x/y"),
                size: 64,
                edge: Edge::Cutout,
                atlas: true,
                previews: Some(PathBuf::from("p"))
            }
        );
        assert_eq!(parse(&["--help"]), Ok(None));
        assert_eq!(parse(&["--size", "32", "-h"]), Ok(None));
    }

    #[test]
    fn bad_arguments_say_what_is_wrong() {
        for (args, why) in [
            (&["--out"][..], "--out needs a directory"),
            (&["--size"], "--size needs a number of pixels"),
            (&["--size", "big"], "--size: \"big\" is not a number"),
            (&["--size", "4"], "--size: from 8 to 2048 pixels"),
            (&["--size", "4096"], "--size: from 8 to 2048 pixels"),
            (&["--previews"], "--previews needs a directory"),
            (&["--sizes", "32,64"], "unknown argument --sizes"),
        ] {
            assert_eq!(parse(args).unwrap_err(), why);
        }
    }

    #[test]
    fn a_run_writes_the_tiles_the_roads_the_signs_the_atlas_and_the_previews() {
        let dir = std::env::temp_dir().join(format!("assetgen-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let args = Args {
            out: dir.join("assets"),
            size: 16,
            edge: Edge::Filled,
            atlas: true,
            previews: Some(dir.join("look")),
        };
        run(&args).unwrap();
        let is_png = |path: PathBuf| std::fs::read(&path).map(|bytes| bytes[1..4] == *b"PNG").unwrap_or(false);
        for terrain in Terrain::ALL {
            assert!(is_png(dir.join("assets/tiles").join(format!("{}_tile.png", terrain.name()))));
        }
        assert!(
            std::fs::read_to_string(dir.join("assets/tiles/atlas.txt")).unwrap().starts_with("# atlas.png 100x40:")
        );
        assert!(is_png(dir.join("assets/tiles/atlas.png")));
        for name in ["road_00.png", "road_09.png", "road_63.png", "bridge_00.png", "bridge_09.png", "bridge_63.png"] {
            assert!(is_png(dir.join("assets/roads").join(name)), "{name}");
        }
        assert_eq!(std::fs::read_dir(dir.join("assets/roads")).unwrap().count(), 28);
        for name in ["militia.png", "wardens.png", "archers.png", "riders.png"] {
            assert!(is_png(dir.join("assets/units").join(name)), "{name}");
        }
        for name in ["near.png", "near_no_border.png", "closeup.png", "far.png", "seams.png", "far.map"] {
            assert!(dir.join("look").join(name).is_file(), "{name}");
        }
        // Somewhere that cannot be a directory: an error that names it.
        let bad = Args { out: dir.join("assets/tiles/atlas.txt"), ..args };
        assert!(run(&bad).unwrap_err().contains("atlas.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
