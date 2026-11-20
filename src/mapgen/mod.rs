//! The random map generator (`mapgen`): from a seed and a few knobs, a whole
//! map directory in the game's format (`docs/map-format.md`), with a report
//! of what it measures and a picture of it (`docs/mapgen.md`).
//!
//! It is pure: no files, no clock, no state of its own beyond what it is
//! given, so the same seed and knobs make the same map on every machine, and
//! the game can call it for a new random map one day. Its stages each draw on
//! their own stream of chance (`rng`), so changing what comes later leaves
//! what came before as it was.
//!
//! - [`grid`]: the hex grid's helpers: walks, shortest ways, pieces, the
//!   quantile pick.
//! - [`reach`]: movement classes, and who gets where.
//! - [`stats`]: what any map measures (`mapgen stats`).
//! - [`check`]: what `mapgen check` says beyond what the reader refuses.
//!
//! Not a port: the C++ had no maps but one.

pub mod check;
pub mod grid;
pub mod reach;
pub mod stats;

/// A terrain file of the smallest map the reader takes (20 x 20), with a
/// drawing in its top left corner and `fill` around it: for tests.
#[cfg(test)]
pub(crate) fn padded(drawing: &[&str], fill: char) -> String {
    let rows: Vec<String> = (0..20)
        .map(|r| {
            let line = drawing.get(r).copied().unwrap_or("");
            format!("{line}{}", fill.to_string().repeat(20 - line.chars().count()))
        })
        .collect();
    format!("20\n20\n{}\n", rows.join("\n"))
}
