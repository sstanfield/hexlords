//! A battle, watched: the attackers' stack on top, the defenders' below, a
//! card for each unit (its icon, its name, the strength it fights with, its
//! hits); a line for each side that says where its bonuses come from (a
//! commander, the ground, the walls); and then the battle played out, an
//! exchange at a time, a few a second: the two that fight lit, the one hit
//! in red with an X, the dead dimmed; and at the end, how it went. Skip goes
//! to the end, Watch again starts it over.
//!
//! It plays back what the battle recorded (`game::Fight`: who fought with
//! what, and each exchange), so it shows exactly what happened, whenever it
//! is watched (`Game::fight` keeps the last few rounds' battles).
//!
//! Not a port: the C++ had no battles.

use turbo_ferret::widgets::ImageWidget;
use turbo_ferret::{ImageHandle, LayoutParams, Rect, TbId, Ui, WidgetId};

use crate::game::{Combatant, Fight, describe_terrain};
use crate::gui::build::{self, WindowParts};
use crate::state::combat::{HITS, QUARTER, System};
use crate::state::scenario::side_name;

/// The window and its buttons.
pub const BATTLE_WINDOW_ID: &str = "battle_window";
pub const BATTLE_SKIP_ID: &str = "battle_skip";
pub const BATTLE_AGAIN_ID: &str = "battle_again";
pub const BATTLE_CLOSE_ID: &str = "battle_close";

/// How long before the first exchange shows, and how long each shows at
/// most and at least, in ms: a battle takes about [`BATTLE_MS`].
const START_MS: u64 = 700;
const SLOWEST_MS: u64 = 450;
const FASTEST_MS: u64 = 150;
const BATTLE_MS: u64 = 6000;
/// A card's width, its icon's size, and the space between things, in dp.
const CARD_DP: i32 = 76;
const ICON_DP: i32 = 36;
const SPACING_DP: i32 = 6;
/// How many lines the outcome has room for.
const OUTCOME_LINES: usize = 6;

/// A unit's card: the box (whose skin says whether it fights or is hit),
/// and the line of its hits.
struct Card {
    root: WidgetId,
    hits: WidgetId,
}

/// The attackers' side of the lists, and the defenders'.
const ATTACKERS: usize = 0;
const DEFENDERS: usize = 1;

/// A battle in a window, while it is open.
pub struct BattleWindow {
    parts: WindowParts,
    fight: Fight,
    cards: [Vec<Card>; 2],
    /// The lines between the stacks: the exchange being shown (who fought,
    /// what came of it).
    status: WidgetId,
    fate: WidgetId,
    /// How it went, shown at the end.
    outcome: Vec<String>,
    outcome_lines: Vec<WidgetId>,
    /// The hits each unit has taken so far, and how many exchanges have been
    /// shown.
    hits: [Vec<u32>; 2],
    shown: usize,
    /// When the next exchange shows (none: when the next tick comes, the
    /// battle starts), and how far apart they show.
    next_at: Option<u64>,
    pace: u64,
}

/// A strength in quarters of a point, as a player reads it: "4", "6½".
pub fn points(quarters: u32) -> String {
    let whole = quarters / QUARTER;
    let part = match quarters % QUARTER {
        1 => "\u{bc}",
        2 => "\u{bd}",
        3 => "\u{be}",
        _ => "",
    };
    if whole == 0 && !part.is_empty() { part.to_string() } else { format!("{whole}{part}") }
}

/// A line with a capital.
fn capital(line: &str) -> String {
    let mut chars = line.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

/// A side's name with a capital.
fn title(side: crate::state::Side) -> String {
    let mut chars = side_name(side).chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

/// What a side brings to a battle, in a line: how many, and where its
/// bonuses come from.
fn side_line(fight: &Fight, defending: bool) -> String {
    let (side, units) = if defending { (fight.defender, &fight.defenders) } else { (fight.attacker, &fight.attackers) };
    let who = if side == crate::state::Side::Grey { "Nobody's".to_string() } else { title(side) };
    let mut parts = vec![format!(
        "{who}: {}",
        if units.len() == 1 { "1 unit".to_string() } else { format!("{} units", units.len()) }
    )];
    let leader = units.iter().find(|u| u.commander);
    let led = units.iter().map(|u| u.led).max().unwrap_or(0);
    if let (Some(leader), true) = (leader, led > 0) {
        parts.push(format!("led by a {} (+{led} each)", leader.name));
    }
    if defending {
        let ground = units.first().map_or(0, |u| u.ground);
        if ground > 0 {
            parts.push(format!("on {} (+{ground})", describe_terrain(fight.terrain, false)));
        }
        let walls = units.first().map_or(0, |u| u.walls);
        if walls > 0 {
            parts.push(format!("behind walls {} (+{})", fight.walls, points(walls)));
        }
    }
    parts.join(", ")
}

impl BattleWindow {
    /// Open a battle's window on `parent` (the root window), placed in `area`
    /// at `align` (see `build::place`). `icons` are the units' pictures, the
    /// attackers' and the defenders', in the fight's order; `place` is where
    /// it was ("Bluemoor"); `outcome` is what to say at the end.
    pub fn open(
        ui: &mut Ui,
        parent: WidgetId,
        fight: &Fight,
        icons: [Vec<ImageHandle>; 2],
        (place, outcome): (&str, Vec<String>),
        (area, align): (Rect, (f32, f32)),
    ) -> BattleWindow {
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let parts = build::window(ui, parent, BATTLE_WINDOW_ID, &format!("Battle at {place}"));
        let c = parts.content;
        let defender = if fight.defender == crate::state::Side::Grey {
            "nobody's units".to_string()
        } else {
            side_name(fight.defender).to_string()
        };
        let system = match fight.rules.system {
            System::Lines => "in lines",
            System::Strength => "strength against strength",
        };
        build::heading(ui, c, &format!("{} attacks {defender} at {place}", title(fight.attacker)));
        build::text(
            ui,
            c,
            &format!("Fought {system}, chance {} of 10; each unit takes {HITS} hits.", fight.rules.chance),
        );
        build::gap(ui, c, dp(ui, SPACING_DP));

        let mut cards: [Vec<Card>; 2] = [Vec::new(), Vec::new()];
        let rows = [(&fight.attackers, ATTACKERS, "a"), (&fight.defenders, DEFENDERS, "d")];
        let (mut status, mut fate) = (c, c);
        for (units, which, tag) in rows {
            if which == DEFENDERS {
                build::gap(ui, c, dp(ui, SPACING_DP));
                status = build::heading(ui, c, "");
                fate = build::heading(ui, c, "");
                build::gap(ui, c, dp(ui, SPACING_DP));
            }
            if which == ATTACKERS {
                build::text(ui, c, &side_line(fight, false));
            }
            let row = build::row(ui, c, dp(ui, 4));
            for (i, unit) in units.iter().enumerate() {
                let icon = icons[which].get(i).cloned().unwrap_or_default();
                if let Some(card) = card(ui, row, &format!("card_{tag}_{i}"), unit, icon) {
                    cards[which].push(card);
                }
            }
            if which == DEFENDERS {
                build::text(ui, c, &side_line(fight, true));
            }
        }

        build::gap(ui, c, dp(ui, SPACING_DP));
        let outcome_lines = (0..OUTCOME_LINES).map(|_| build::text(ui, c, "")).collect();
        build::gap(ui, c, dp(ui, SPACING_DP));
        let buttons = build::row(ui, c, dp(ui, SPACING_DP));
        build::button(ui, buttons, BATTLE_SKIP_ID, "Skip");
        build::button(ui, buttons, BATTLE_AGAIN_ID, "Watch again");
        build::button(ui, buttons, BATTLE_CLOSE_ID, "Close");

        let n = fight.trace.len().max(1) as u64;
        let pace = (BATTLE_MS / n).clamp(FASTEST_MS, SLOWEST_MS);
        let hits = [vec![0; fight.attackers.len()], vec![0; fight.defenders.len()]];
        let mut window = BattleWindow {
            parts,
            fight: fight.clone(),
            cards,
            status,
            fate,
            outcome,
            outcome_lines,
            hits,
            shown: 0,
            next_at: None,
            pace,
        };
        window.draw(ui, None);
        build::place(ui, parts, area, align);
        window
    }

    /// Show what is due by `now_ms`: the next exchange, when its time has
    /// come (the first a little after the window opened); the outcome after
    /// the last.
    pub fn tick(&mut self, ui: &mut Ui, now_ms: u64) {
        let Some(next) = self.next_at else {
            self.next_at = Some(now_ms + START_MS);
            return;
        };
        if now_ms < next || self.done() {
            return;
        }
        self.step(ui);
        self.next_at = Some(now_ms + self.pace);
    }

    /// Show the next exchange.
    fn step(&mut self, ui: &mut Ui) {
        let Some(blow) = self.fight.trace.get(self.shown).copied() else { return };
        let side = if blow.hit_attacker { ATTACKERS } else { DEFENDERS };
        if let Some(hits) = self.hits[side].get_mut(blow.unit) {
            *hits = (*hits + 1).min(HITS);
        }
        self.shown += 1;
        self.draw(ui, Some(blow));
    }

    /// Go to the end at once (Skip).
    pub fn skip(&mut self, ui: &mut Ui) {
        while !self.done() {
            self.step(ui);
        }
        self.draw(ui, None);
    }

    /// Start it over (Watch again).
    pub fn again(&mut self, ui: &mut Ui) {
        self.hits = [vec![0; self.fight.attackers.len()], vec![0; self.fight.defenders.len()]];
        self.shown = 0;
        self.next_at = None;
        self.draw(ui, None);
    }

    /// True once every exchange has been shown.
    pub fn done(&self) -> bool {
        self.shown >= self.fight.trace.len()
    }

    /// The cards, the line between the stacks and the outcome as things
    /// stand, with `blow` the exchange just shown.
    fn draw(&mut self, ui: &mut Ui, blow: Option<crate::state::combat::Exchange>) {
        for which in [ATTACKERS, DEFENDERS] {
            for (i, card) in self.cards[which].iter().enumerate() {
                let hits = self.hits[which].get(i).copied().unwrap_or(0);
                let fighting = blow
                    .is_some_and(|b| if which == ATTACKERS { b.attacker == Some(i) } else { b.defender == Some(i) });
                let hit = blow.is_some_and(|b| b.hit_attacker == (which == ATTACKERS) && b.unit == i);
                let skin = if hit {
                    "BattleCard.hit"
                } else if fighting {
                    "BattleCard.fighting"
                } else {
                    "BattleCard"
                };
                if ui.node(card.root).is_some_and(|n| n.skin() != TbId::new(skin)) {
                    ui.set_skin(card.root, TbId::new(skin));
                }
                // A space for none, so that the line keeps its room.
                let marks = if hits == 0 { " ".to_string() } else { vec!["X"; hits as usize].join(" ") };
                if ui.text(card.hits) != Some(marks.as_str()) {
                    ui.set_text(card.hits, &marks);
                }
                let opacity = if hits >= HITS && !hit { 0.35 } else { 1.0 };
                ui.set_opacity(card.root, opacity);
            }
        }
        let done = self.done();
        let (fought, fate) = match blow {
            None if self.shown == 0 => ("Ready...".to_string(), String::new()),
            None => (String::new(), self.result_line()),
            Some(blow) if done => {
                (self.blow_line(blow).0, format!("{}  {}", self.blow_line(blow).1, self.result_line()))
            }
            Some(blow) => self.blow_line(blow),
        };
        ui.set_text(self.status, &fought);
        ui.set_text(self.fate, &fate);
        // The outcome, once it is all shown.
        for (i, widget) in self.outcome_lines.iter().enumerate() {
            let text = if done { self.outcome.get(i).map_or("", String::as_str) } else { "" };
            if ui.text(*widget) != Some(text) {
                ui.set_text(*widget, text);
            }
        }
    }

    /// Who won, in a few words.
    fn result_line(&self) -> String {
        if self.fight.won {
            format!("{} wins.", title(self.fight.attacker))
        } else if self.fight.defender == crate::state::Side::Grey {
            "The defenders hold.".to_string()
        } else {
            format!("{} holds.", title(self.fight.defender))
        }
    }

    /// An exchange in two lines: who fought with what, and who was hit
    /// (whose, since both sides may have the same kinds).
    fn blow_line(&self, blow: crate::state::combat::Exchange) -> (String, String) {
        let unit = |which: usize, i: usize| -> Option<&Combatant> {
            if which == ATTACKERS { self.fight.attackers.get(i) } else { self.fight.defenders.get(i) }
        };
        let whose = |which: usize| {
            let side = if which == ATTACKERS { self.fight.attacker } else { self.fight.defender };
            if side == crate::state::Side::Grey { "nobody".to_string() } else { side_name(side).to_string() }
        };
        let (which, i) = (if blow.hit_attacker { ATTACKERS } else { DEFENDERS }, blow.unit);
        let hit = format!("{}'s {}", whose(which), unit(which, i).map_or("", |u| u.name.as_str()));
        let dead = self.hits[which].get(i).is_some_and(|h| *h >= HITS);
        let fate = if dead { format!("{}: out of the battle.", capital(&hit)) } else { format!("A hit on {hit}.") };
        let fought = match (blow.attacker, blow.defender) {
            (Some(a), Some(d)) => {
                let name = |which: usize, u: Option<&Combatant>| {
                    u.map_or(String::new(), |u| format!("{}'s {} ({})", whose(which), u.name, points(u.quarters)))
                };
                format!(
                    "{} against {}.",
                    capital(&name(ATTACKERS, unit(ATTACKERS, a))),
                    name(DEFENDERS, unit(DEFENDERS, d))
                )
            }
            _ => {
                // By strength: the stacks' totals, of those still standing.
                let total = |which: usize, units: &[Combatant]| -> u32 {
                    units.iter().enumerate().filter(|(i, _)| self.hits[which][*i] < HITS).map(|(_, u)| u.quarters).sum()
                };
                let (a, d) = (total(ATTACKERS, &self.fight.attackers), total(DEFENDERS, &self.fight.defenders));
                format!("{} {} against {} {}.", capital(&whose(ATTACKERS)), points(a), whose(DEFENDERS), points(d))
            }
        };
        (fought, fate)
    }

    /// True while the window is open.
    pub fn is_open(&self, ui: &Ui) -> bool {
        build::is_open(ui, self.parts.window)
    }

    pub fn close(self, ui: &mut Ui) {
        build::close(ui, self.parts.window);
    }

    /// The lines between the stacks.
    pub fn status(&self, ui: &Ui) -> [String; 2] {
        [self.status, self.fate].map(|w| ui.text(w).unwrap_or("").to_string())
    }

    /// The outcome's lines, once it is shown.
    pub fn outcome(&self, ui: &Ui) -> Vec<String> {
        self.outcome_lines.iter().filter_map(|w| ui.text(*w)).filter(|t| !t.is_empty()).map(str::to_string).collect()
    }

    /// Each card's hits so far: the attackers', the defenders'.
    pub fn hits(&self) -> &[Vec<u32>; 2] {
        &self.hits
    }

    /// How many exchanges have been shown, of how many.
    pub fn progress(&self) -> (usize, usize) {
        (self.shown, self.fight.trace.len())
    }
}

/// A unit's card, made in `row`: its icon, name, strength and hits.
fn card(ui: &mut Ui, row: WidgetId, id: &str, unit: &Combatant, icon: ImageHandle) -> Option<Card> {
    let resource = format!(
        "TBLayout: id: {id}, axis: y, spacing: 1, position: center, skin: BattleCard\n\
         \tTBImageWidget: id: {id}_icon\n\
         \tTBTextField: id: {id}_name, text-align: center\n\
         \tTBTextField: id: {id}_strength, text-align: center\n\
         \tTBTextField: id: {id}_hits, text-align: center, skin: HitMark\n"
    );
    ui.inflate(row, &resource).ok()?;
    let find = |ui: &Ui, part: &str| ui.find(row, TbId::new(&format!("{id}{part}")));
    let (root, image, name, strength, hits) =
        (find(ui, "")?, find(ui, "_icon")?, find(ui, "_name")?, find(ui, "_strength")?, find(ui, "_hits")?);
    let dp = |n: i32| ui.dimension_converter().dp_to_px(n);
    let (width, size) = (dp(CARD_DP), dp(ICON_DP));
    ui.set_layout_params(image, Some(LayoutParams::fixed(size, size)));
    ui.with_widget::<ImageWidget, _>(image, |w, cx| w.set_image(cx, icon));
    for field in [name, strength, hits] {
        ui.set_layout_params(
            field,
            Some(LayoutParams { min_w: Some(width), max_w: Some(width), ..LayoutParams::default() }),
        );
    }
    ui.set_text(name, &unit.name);
    // Its strength, and what of it is bonus.
    let bonus = unit.quarters.saturating_sub(unit.strength * QUARTER);
    let text = if bonus > 0 { format!("{} +{}", unit.strength, points(bonus)) } else { unit.strength.to_string() };
    ui.set_text(strength, &text);
    // Room for the marks, none yet.
    ui.set_text(hits, " ");
    Some(Card { root, hits })
}
