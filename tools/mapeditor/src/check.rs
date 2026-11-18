//! `mapeditor --check <file>`: read a map file the way the game does and say
//! what is in it, including the things the reader forgives (short and missing
//! rows are filled with grass) that are probably mistakes in a file written by
//! hand.

use hexgame::state::{Map, MapError, TileType};

/// What a map file holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub rows: u16,
    pub cols: u16,
    /// How many tiles of each terrain, in the order of `TileType::ALL`.
    pub counts: [usize; TileType::ALL.len()],
    /// What the reader accepted but the author may not have meant.
    pub warnings: Vec<String>,
}

/// Read `text` as a map file. An error is what the game would say.
pub fn check(text: &str) -> Result<Report, MapError> {
    let map = Map::parse(text)?;
    let (rows, cols) = (usize::from(map.rows()), usize::from(map.cols()));
    let mut counts = [0; TileType::ALL.len()];
    for row in 0..map.rows() {
        for col in 0..map.cols() {
            if let Some(tile) = map.tile(hexgame::state::Hex::new(row, col)) {
                let i = TileType::ALL.iter().position(|k| *k == tile.kind).unwrap_or(0);
                counts[i] += 1;
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
    Ok(Report { rows: map.rows(), cols: map.cols(), counts, warnings })
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} rows x {} columns", self.rows, self.cols)?;
        for (kind, count) in TileType::ALL.iter().zip(self.counts) {
            if count > 0 {
                writeln!(f, "  {:<9}{} {count}", kind.name(), kind.to_char())?;
            }
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
}
