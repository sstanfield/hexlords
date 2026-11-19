//! `mapeditor --check <map>`: read a map the way the game does and say what
//! is in it, including the things the reader forgives (short and missing rows
//! are filled with grass, a city tile nobody named) that are probably
//! mistakes in a file written by hand.

use std::path::Path;

use hexgame::render::AssetDirs;
use hexgame::state::scenario::{self, side_name};
use hexgame::state::{Hex, Map, MapError, TileType};

/// What a map file holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub rows: u16,
    pub cols: u16,
    /// How many tiles of each terrain, in the order of `TileType::ALL`.
    pub counts: [usize; TileType::ALL.len()],
    /// How many tiles have a road, and how many of those are over water
    /// (bridges) and over mountains (passes).
    pub roads: usize,
    pub bridges: usize,
    pub passes: usize,
    /// The kinds of unit, a line each (a map directory's `units.txt`).
    pub units: Vec<String>,
    /// The cities, a line each (a map directory's `cities.txt`).
    pub cities: Vec<String>,
    /// What the reader accepted but the author may not have meant.
    pub warnings: Vec<String>,
}

/// Read `text` as a map file. An error is what the game would say.
pub fn check(text: &str) -> Result<Report, MapError> {
    let map = Map::parse(text)?;
    let (rows, cols) = (usize::from(map.rows()), usize::from(map.cols()));
    let mut counts = [0; TileType::ALL.len()];
    let (mut roads, mut bridges, mut passes) = (0, 0, 0);
    for row in 0..map.rows() {
        for col in 0..map.cols() {
            if let Some(tile) = map.tile(Hex::new(row, col)) {
                counts[tile.kind.index()] += 1;
                roads += usize::from(tile.road);
                bridges += usize::from(tile.road && tile.kind == TileType::Water);
                passes += usize::from(tile.road && tile.kind == TileType::Mountain);
            }
        }
    }

    // The map's rows start on the file's third line.
    let lines: Vec<&str> = text.lines().skip(2).map(str::trim_end).collect();
    let mut warnings = Vec::new();
    for (r, line) in lines.iter().take(rows).enumerate() {
        let tiles = line.chars().count();
        if tiles < cols {
            warnings.push(format!(
                "line {}: row {r} has {tiles} tiles, {} short of {cols} (the rest is grass)",
                r + 3,
                cols - tiles
            ));
        }
    }
    if lines.len() < rows {
        warnings.push(format!("{} of {rows} rows are missing (they are grass)", rows - lines.len()));
    }
    let extra = lines.iter().skip(rows).filter(|l| !l.trim().is_empty()).count();
    if extra > 0 {
        warnings.push(format!("{extra} lines after the last row are ignored"));
    }
    let (units, cities) = (Vec::new(), Vec::new());
    Ok(Report { rows: map.rows(), cols: map.cols(), counts, roads, bridges, passes, units, cities, warnings })
}

/// Read a map, a file or a directory (`docs/map-format.md`), and report on
/// all of it. For a directory that is also its kinds of unit and its cities,
/// and whether every unit's picture is there: in the map's own assets or in
/// the game's (`assets`, if it is known).
pub fn check_path(path: &Path, assets: Option<&Path>) -> Result<Report, String> {
    let terrain = scenario::terrain_path(path);
    let text = std::fs::read_to_string(&terrain).map_err(|e| format!("{}: {e}", terrain.display()))?;
    let mut report = check(&text).map_err(|e| format!("{}: {e}", terrain.display()))?;
    if !path.is_dir() {
        return Ok(report);
    }
    let whole = scenario::load(path).map_err(|e| e.to_string())?;
    let map = &whole.map;
    for kind in map.units().kinds() {
        report.units.push(format!(
            "{} \"{}\": strength {}, moves {}, upkeep {}, price {}, {}",
            kind.id, kind.name, kind.strength, kind.moves, kind.upkeep, kind.price, kind.image
        ));
    }
    for city in map.cities().iter() {
        let builds: Vec<String> = city.units.iter().map(|p| format!("{} ({})", p.basename, p.time)).collect();
        let builds = if builds.is_empty() { "nothing".to_string() } else { builds.join(", ") };
        report.cities.push(format!(
            "{} at {}: {}, walls {}, builds {builds}, garrison {}",
            city.name,
            city.pos,
            side_name(city.side),
            city.walls,
            map.units_at(city.pos).len()
        ));
    }
    report.warnings.extend(whole.warnings.iter().cloned());
    if let Some(assets) = assets {
        let dirs = AssetDirs::new(assets, whole.assets.as_deref());
        for kind in map.units().kinds() {
            if let Err(e) = dirs.image(&kind.image) {
                report.warnings.push(format!("the picture of {}: {e}", kind.id));
            }
        }
    }
    Ok(report)
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} rows x {} columns", self.rows, self.cols)?;
        for (kind, count) in TileType::ALL.iter().zip(self.counts) {
            if count > 0 {
                writeln!(f, "  {:<9}{} {count}", kind.name(), kind.to_char())?;
            }
        }
        if self.roads > 0 {
            writeln!(f, "  roads {} (of them {} bridges, {} passes)", self.roads, self.bridges, self.passes)?;
        }
        for unit in &self.units {
            writeln!(f, "  unit {unit}")?;
        }
        for city in &self.cities {
            writeln!(f, "  city {city}")?;
        }
        for warning in &self.warnings {
            writeln!(f, "warning: {warning}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: usize, row: &str) -> String {
        let mut text = format!("{rows}\n{}\n", row.len());
        for _ in 0..rows {
            text.push_str(row);
            text.push('\n');
        }
        text
    }

    #[test]
    fn a_good_file_has_counts_and_no_warnings() {
        let report = check(&grid(20, "ggggggggggwwwwwwwwwwffff")).unwrap();
        assert_eq!((report.rows, report.cols), (20, 24));
        assert_eq!(report.counts[0], 200, "grass");
        assert_eq!(report.counts[1], 80, "forest");
        assert_eq!(report.counts[6], 200, "water");
        assert_eq!(report.counts.iter().sum::<usize>(), 20 * 24);
        assert_eq!(report.warnings, Vec::<String>::new());
        let shown = report.to_string();
        assert!(shown.starts_with("20 rows x 24 columns\n"), "{shown}");
        assert!(shown.contains("  grass    g 200\n") && shown.contains("  water    w 200\n"), "{shown}");
        assert!(!shown.contains("swamp"), "{shown}");
    }

    #[test]
    fn what_the_reader_forgives_is_a_warning() {
        // Two rows are missing, and the first one is short.
        let text = grid(18, "wwwwwwwwwwwwwwwwwwww").replacen("18\n", "20\n", 1).replacen(
            "wwwwwwwwwwwwwwwwwwww\n",
            "wwwww\n",
            1,
        );
        let report = check(&text).unwrap();
        assert_eq!(
            report.warnings,
            [
                "line 3: row 0 has 5 tiles, 15 short of 20 (the rest is grass)".to_string(),
                "2 of 20 rows are missing (they are grass)".to_string()
            ]
        );
        assert_eq!(report.counts[0], 15 + 40);

        let mut text = grid(20, "gggggggggggggggggggg");
        text.push_str("\ncity 3 4 Somewhere\n");
        assert_eq!(check(&text).unwrap().warnings, ["1 lines after the last row are ignored".to_string()]);
    }

    #[test]
    fn what_the_reader_refuses_is_an_error() {
        let text = grid(20, "ggggggggggggggggggxg");
        assert_eq!(check(&text).unwrap_err().to_string(), "map file line 3: unknown terrain 'x' at column 19");
        assert!(check("5\n5\n").is_err(), "too small");
    }

    #[test]
    fn roads_are_counted() {
        let report = check(&grid(20, "ggggGGGWWMgggggggggg")).unwrap();
        assert_eq!((report.roads, report.bridges, report.passes), (20 * 6, 20 * 2, 20));
        assert_eq!(report.counts[0], 20 * 17, "grass, with or without a road");
        assert!(report.to_string().contains("  roads 120 (of them 40 bridges, 20 passes)\n"), "{report}");
        assert!(!check(&grid(20, "gggggggggggggggggggg")).unwrap().to_string().contains("roads"));
    }

    #[test]
    fn a_map_directory_is_checked_as_a_whole() {
        let dir = std::env::temp_dir().join(format!("mapeditor-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let game_assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut rows = vec!["g".repeat(20); 20];
        rows[4] = "ggggcGGcgggggggggggg".to_string();
        std::fs::write(dir.join("terrain.map"), format!("20\n20\n{}\n", rows.join("\n"))).unwrap();
        std::fs::write(
            dir.join("units.txt"),
            "unit riders\nname Riders\nimage units/riders.png\nmoves 9\nenter grass 2\n\
             unit ghosts\nimage units/ghosts.png\nmoves 3\nenter grass 1\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("cities.txt"),
            "city 4 4\nname Wrenfold\nside red\nwalls 1\nproduce riders 2\ngarrison riders 2\n",
        )
        .unwrap();

        let report = check_path(&dir, Some(&game_assets)).unwrap();
        assert_eq!((report.rows, report.cols, report.roads), (20, 20, 2));
        assert_eq!(
            report.units,
            [
                "riders \"Riders\": strength 1, moves 9, upkeep 0, price 0, units/riders.png".to_string(),
                "ghosts \"ghosts\": strength 1, moves 3, upkeep 0, price 0, units/ghosts.png".to_string()
            ]
        );
        assert_eq!(report.cities, ["Wrenfold at (4, 4): red, walls 1, builds riders (2), garrison 2".to_string()]);
        // The city nobody named, and the picture that is nowhere.
        assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
        assert_eq!(report.warnings[0], "the city tile at (4, 7) has no entry in cities.txt");
        assert!(report.warnings[1].starts_with("the picture of ghosts: units/ghosts.png: no such file"));
        let shown = report.to_string();
        assert!(shown.contains("  unit riders \"Riders\"") && shown.contains("  city Wrenfold at (4, 4)"), "{shown}");

        // The map brings the picture itself: no warning for it.
        std::fs::create_dir_all(dir.join("assets/units")).unwrap();
        std::fs::copy(game_assets.join("units/riders.png"), dir.join("assets/units/ghosts.png")).unwrap();
        assert_eq!(check_path(&dir, Some(&game_assets)).unwrap().warnings.len(), 1);
        // Without knowing the game's assets, pictures are not looked for.
        std::fs::remove_dir_all(dir.join("assets")).unwrap();
        assert_eq!(check_path(&dir, None).unwrap().warnings.len(), 1);

        // A mistake in one of the files is an error that names it.
        std::fs::write(dir.join("cities.txt"), "city 4 5\nname Wrenfold\n").unwrap();
        let e = check_path(&dir, None).unwrap_err();
        assert!(e.contains("cities.txt line 1: Wrenfold is at (4, 5), which is grass"), "{e}");
        // The terrain file alone is checked as a file: no word about its cities.
        let alone = check_path(&dir.join("terrain.map"), None).unwrap();
        assert!(alone.warnings.is_empty() && alone.cities.is_empty());
        assert!(check_path(&dir.join("none.map"), None).unwrap_err().contains("none.map"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
