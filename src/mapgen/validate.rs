//! The hard checks a generated map must pass, or be made again
//! (`docs/mapgen.md`): the game reads it; `mapgen check` finds no problem
//! (every bridge and pass on a road from land to land); every city is reached
//! as it was planned (on foot, by sea, through the air), and every temple and
//! ruin on the capitals' land on foot; as many cities, capitals and sites as
//! asked; the cities apart and off the edge; and fair. What fails is said, a
//! line each.

use crate::state::{Hex, Side, TileType};

use super::Generated;
use super::check::check;
use super::reach::{MoveClass, ReachClass, reach};
use super::sites::{count, temple_count};

/// What a map fails of the hard checks (nothing if it passes).
pub fn hard(made: &Generated) -> Vec<String> {
    let (cfg, plan, g) = (&made.config, &made.plan, &made.state);
    let mut out = Vec::new();
    let map = match made.map() {
        Ok(map) => map,
        Err(e) => return vec![format!("the game does not read it: {e}")],
    };
    let c = check(&map, true);
    out.extend(c.problems.iter().cloned());
    // Reached as planned.
    for (name, class) in &c.reach {
        if let Some(city) = g.cities.iter().find(|x| x.name == *name) {
            if *class != city.reach {
                out.push(format!("{name} is reached by {}, planned by {}", class.word(), city.reach.word()));
            }
        }
    }
    let capitals: Vec<Hex> = map.cities().iter().filter(|x| x.side != Side::Grey).map(|x| x.pos).collect();
    let walked = reach(&map, MoveClass::Foot, &capitals);
    for h in g.terrain.hexes() {
        if matches!(*g.terrain.get(h), TileType::Temple | TileType::Ruin) && !*g.island.get(h) && !*walked.get(h) {
            out.push(format!(
                "the {} at {h} is not reached on foot",
                if *g.terrain.get(h) == TileType::Temple { "temple" } else { "ruin" }
            ));
        }
    }
    // As many as asked.
    if g.cities.len() != plan.cities as usize {
        out.push(format!("{} cities, {} asked", g.cities.len(), plan.cities));
    }
    let held = g.cities.iter().filter(|x| x.side != Side::Grey).count();
    if held != usize::from(cfg.sides) {
        out.push(format!("{held} capitals for {} sides", cfg.sides));
    }
    for (kind, want, word) in
        [(TileType::Temple, temple_count(cfg, plan), "temples"), (TileType::Ruin, count(cfg.ruins, cfg, plan), "ruins")]
    {
        let have = g.terrain.cells().iter().filter(|t| **t == kind).count();
        if have != want {
            out.push(format!("{have} {word}, {want} asked"));
        }
    }
    let at = |class: ReachClass| g.cities.iter().filter(|x| x.reach == class).count();
    if cfg.sea_cities && at(ReachClass::Sea) == 0 {
        out.push("sea_cities is on, and no city is reached by sea".to_string());
    }
    if cfg.fly_cities && at(ReachClass::Air) == 0 {
        out.push("fly_cities is on, and no city is reached through the air".to_string());
    }
    // Apart and off the edge.
    let gap = cfg.min_gap(plan);
    for (i, a) in g.cities.iter().enumerate() {
        if u32::from(g.terrain.edge_distance(a.hex)) < cfg.edge_margin {
            out.push(format!("{} is at the map's edge", a.name));
        }
        if let Some(b) = g.cities[i + 1..].iter().find(|b| a.hex.distance(b.hex) < gap) {
            out.push(format!("{} and {} are nearer than {gap}", a.name, b.name));
        }
    }
    // Fair.
    for f in g.fairness.iter().filter(|f| !f.ok()) {
        out.push(format!("unfair: {} is {:.2}, at most {}", f.name, f.value, f.limit));
    }
    out
}
