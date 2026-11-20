//! The new game screen: the maps to choose from, how to play (`Mode`), the
//! order of the turns, and who plays each side of the chosen map (a person or
//! the computer); the chosen map is shown beside them, to look over, and the
//! game starts with the button (`Shell`, in `shell.rs`, then hands the window
//! to a `Session`).
//!
//! Not a port: the C++ started its demo at once.

use std::path::{Path, PathBuf};

use turbo_ferret::widgets::{ScrollContainer, ScrollMode};
use turbo_ferret::{LayoutParams, Size, TbId, WidgetId};

use crate::game::Game;
use crate::gui::RootCommand;
use crate::gui::build;
use crate::help::HelpSection;
use crate::screen::{MapScreen, Scene};
use crate::setup::{Controller, Fog, Mode, Setup, TurnOrder};
use crate::state::combat::{MAX_CHANCE, System};
use crate::state::scenario::side_name;
use crate::state::{Hex, Map, Side, TileType};

/// The layout, relative to the asset directory.
pub const LAYOUT: &str = "layouts/new_game.tb.txt";
/// The button that starts the game.
pub const START_ID: &str = "start_game";
/// The buttons of the settings.
pub const MODE_TURNS_ID: &str = "mode_turns";
pub const MODE_ROUNDS_ID: &str = "mode_rounds";
pub const ORDER_FIXED_ID: &str = "order_fixed";
pub const ORDER_DRAWN_ID: &str = "order_drawn";
pub const BATTLES_LINES_ID: &str = "battles_lines";
pub const BATTLES_STRENGTH_ID: &str = "battles_strength";
pub const CHANCE_LESS_ID: &str = "chance_less";
pub const CHANCE_MORE_ID: &str = "chance_more";
pub const FOG_OFF_ID: &str = "fog_off";
pub const FOG_ON_ID: &str = "fog_on";
pub const FOG_TOTAL_ID: &str = "fog_total";
pub const GOLD_LESS_ID: &str = "gold_less";
pub const GOLD_MORE_ID: &str = "gold_more";
/// How much a press of Gold's - or + changes it, and the most it can be.
const GOLD_STEP: u32 = 10;
const MAX_GOLD: u32 = 1000;
/// The agent the computer plays with, unless told otherwise.
pub const COMPUTER: &str = "steady";
/// The computer's players a seat can have, in the order a click on its
/// Computer button goes through them (`agents`, `planner`).
pub const COMPUTERS: [&str; 4] = ["steady", "bold", "careful", "simple"];

/// The button of the `i`th map.
pub fn map_id(i: usize) -> String {
    format!("map_{i}")
}

/// The button that has a person (`person`) or the computer play a side.
pub fn seat_id(side: Side, person: bool) -> String {
    format!("seat_{}_{}", side_name(side), if person { "person" } else { "computer" })
}

/// The map takes this share of the window's width; the minimap this share of
/// the height.
const MAP_WIDTH_SHARE: f32 = 0.62;
const MINIMAP_HEIGHT_SHARE: f32 = 0.3;
const SPACING_DP: i32 = 5;
/// The list of maps: how high it is at most, in dp.
const MAPS_DP: i32 = 140;

/// The maps there are to play: the game's (`assets/maps`: a `.map` file, or a
/// directory with a `terrain.map`), and the owner's private ones beside the
/// assets (`encumbered/`), by name.
pub fn find_maps(assets: &Path) -> Vec<PathBuf> {
    let mut maps = Vec::new();
    let encumbered = assets.parent().map(|root| root.join("encumbered"));
    for dir in [Some(assets.join("maps")), encumbered].into_iter().flatten() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "map") || p.join("terrain.map").is_file())
            .collect();
        found.sort();
        maps.extend(found);
    }
    maps
}

/// A map's name for the list: its file's or directory's name.
pub fn map_name(path: &Path) -> String {
    path.file_stem().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub struct NewGame {
    pub screen: MapScreen,
    maps: Vec<PathBuf>,
    chosen: Option<usize>,
    setup: Setup,
    /// Where the settings go, and what is in it now.
    settings: WidgetId,
    sidebar: WidgetId,
    /// What went wrong with the chosen map, if anything.
    problem: Option<String>,
    start: bool,
}

impl NewGame {
    /// The screen for a window of `viewport` pixels at `dpi`, with `chosen`
    /// (or the game's continent, or the first map) chosen.
    pub fn new(assets: &Path, viewport: Size, dpi: i32, chosen: Option<&Path>) -> Result<NewGame, String> {
        let blank = Game::new(Map::new(20, 20, TileType::Water).map_err(|e| format!("{e:?}"))?);
        let mut screen = MapScreen::new(assets, viewport, dpi, LAYOUT, blank)?;
        // The map is there to be looked over: a drag moves it.
        screen.set_drag_pans(true);
        screen.set_help(vec![HelpSection::new(
            "A new game",
            &[
                ("A map's name", "Choose it: it shows beside the settings"),
                ("Drag on the map", "Move it"),
                ("Turns, Rounds", "How to play: one side after the other (rounds are to come)"),
                ("Order of turns", "The same each round, or drawn anew"),
                ("Battles", "Lines of duels, or strength against strength; how much chance, 0 to 10"),
                ("Gold", "What each side's treasury holds at the start"),
                ("Fog of war", "Each side sees only what is near its units and cities"),
                ("Fog: Total", "And the land a side has never seen is black, shut to its units"),
                ("Person, Computer", "Who plays each side: several people take turns at the screen"),
                ("Start the game", "Play"),
            ],
        )]);
        let find = |screen: &MapScreen, id: &str| {
            screen.ui.find(screen.ui.root(), TbId::new(id)).ok_or(format!("{LAYOUT}: no widget with id {id}"))
        };
        let (settings, sidebar) = (find(&screen, "settings")?, find(&screen, "main_sidebar")?);
        let maps = find_maps(assets);
        let pick = chosen
            .and_then(|c| maps.iter().position(|m| m == c))
            .or_else(|| maps.iter().position(|m| map_name(m) == "continent"))
            .or((!maps.is_empty()).then_some(0));
        let mut setup = Setup::solo(&screen.game.map, None, COMPUTER);
        // Each game its own chance: the seed is drawn here, once (the rules
        // themselves never look at the clock).
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        setup.seed = now.map_or(1, |d| d.as_nanos() as u64);
        let mut new_game =
            NewGame { screen, maps, chosen: None, setup, settings, sidebar, problem: None, start: false };
        new_game.apply_layout_sizes();
        match pick {
            Some(i) => new_game.choose(i),
            None => new_game.rebuild(),
        }
        new_game.screen.settle();
        Ok(new_game)
    }

    /// The maps it offers.
    pub fn maps(&self) -> &[PathBuf] {
        &self.maps
    }

    /// The map chosen, if any.
    pub fn chosen(&self) -> Option<&Path> {
        self.chosen.map(|i| self.maps[i].as_path())
    }

    /// How the game will be set up.
    pub fn setup(&self) -> &Setup {
        &self.setup
    }

    /// To change how the game will be set up (the seed, say).
    pub fn setup_mut(&mut self) -> &mut Setup {
        &mut self.setup
    }

    /// What is wrong with the chosen map, if anything.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Choose the `i`th map: it shows, its sides get their seats (the first
    /// side's a person, the others the computer's), the rest of the setup stays.
    pub fn choose(&mut self, i: usize) {
        let Some(path) = self.maps.get(i).cloned() else { return };
        self.chosen = Some(i);
        match Game::load(&path) {
            Ok((game, _)) => {
                let person = game.setup().seats.iter().find(|s| s.controller == Controller::Person).map(|s| s.side);
                let mut setup = Setup::solo(&game.map, person, COMPUTER);
                (setup.mode, setup.order, setup.seed) = (self.setup.mode, self.setup.order, self.setup.seed);
                setup.combat = self.setup.combat;
                setup.gold = self.setup.gold;
                setup.fog = self.setup.fog;
                self.setup = setup;
                self.screen.set_game(game);
                self.problem = None;
            }
            Err(e) => self.problem = Some(e),
        }
        self.rebuild();
    }

    /// True if the chosen map can be played: it was read, and it has sides.
    pub fn can_start(&self) -> bool {
        self.problem.is_none() && self.chosen.is_some() && !self.setup.seats.is_empty()
    }

    /// True once the game was asked to start, with a map that can be played.
    pub fn wants_to_start(&self) -> bool {
        self.start && self.can_start()
    }

    /// The game to play: the chosen map, set up as chosen. Leaves the screen
    /// with a blank map.
    pub fn take_game(&mut self) -> Game {
        let blank = Game::new(Map::new(20, 20, TileType::Water).expect("20 x 20 is a valid map"));
        std::mem::replace(&mut self.screen.game, blank).with_setup(self.setup.clone())
    }

    /// Size the map and the minimap from the window's size.
    fn apply_layout_sizes(&mut self) {
        let Size { w, h } = self.screen.ui.viewport();
        let spacing = self.screen.ui.dimension_converter().dp_to_px(SPACING_DP);
        let map_w = (w as f32 * MAP_WIDTH_SHARE).floor() as i32;
        let side_w = (w - map_w - spacing).max(0);
        let minimap_h = (h as f32 * MINIMAP_HEIGHT_SHARE) as i32;
        self.screen.set_map_sizes(Size::new(map_w, h.max(0)), Size::new(side_w, minimap_h));
        let narrow = LayoutParams { min_w: Some(0), max_w: Some(side_w), ..LayoutParams::default() };
        self.screen.ui.set_layout_params(self.sidebar, Some(narrow));
    }

    /// Build the settings again: the maps, the mode, the order, the seats.
    fn rebuild(&mut self) {
        let ui = &mut self.screen.ui;
        for child in ui.children(self.settings).to_vec() {
            ui.remove(child);
        }
        let dp = |ui: &turbo_ferret::Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let (c, space) = (self.settings, dp(ui, SPACING_DP));
        let press = |ui: &mut turbo_ferret::Ui, button: WidgetId, group: &str, on: bool| {
            ui.set_group_id(button, TbId::new(group));
            ui.set_value(button, i32::from(on));
        };

        build::heading(ui, c, "Map");
        let scroll = ui.add(c, ScrollContainer::new(ScrollMode::YAuto).with_adapt_content_size(true));
        let side_w = ui.layout_params(self.sidebar).and_then(|lp| lp.max_w).unwrap_or(200);
        let rows = (self.maps.len().max(1) as i32) * dp(ui, 22);
        ui.set_layout_params(scroll, Some(LayoutParams::fixed(side_w, rows.min(dp(ui, MAPS_DP)))));
        let list = build::column(ui, scroll, 0);
        for (i, path) in self.maps.iter().enumerate() {
            let button = build::button(ui, list, &map_id(i), &map_name(path));
            ui.set_skin(button, TbId::new("ListRow"));
            build::set_width(ui, button, side_w - dp(ui, 18));
            build::text_from_left(ui, button);
            press(ui, button, "maps", self.chosen == Some(i));
        }
        if self.maps.is_empty() {
            build::text(ui, list, "No maps in assets/maps.");
        }
        if let Some(problem) = &self.problem {
            build::text(ui, c, problem);
        }

        build::gap(ui, c, space);
        build::heading(ui, c, "How to play");
        let row = build::row(ui, c, space);
        let turns = build::button(ui, row, MODE_TURNS_ID, "Turns");
        press(ui, turns, "mode", self.setup.mode == Mode::Turns);
        let rounds = build::button(ui, row, MODE_ROUNDS_ID, "Rounds (to come)");
        press(ui, rounds, "mode", self.setup.mode == Mode::Rounds);
        ui.set_disabled(rounds, true);

        build::gap(ui, c, space);
        build::heading(ui, c, "Order of turns");
        let row = build::row(ui, c, space);
        let fixed = build::button(ui, row, ORDER_FIXED_ID, "The same");
        press(ui, fixed, "order", self.setup.order == TurnOrder::Fixed);
        let drawn = build::button(ui, row, ORDER_DRAWN_ID, "Drawn each round");
        press(ui, drawn, "order", self.setup.order == TurnOrder::Drawn);

        build::gap(ui, c, space);
        build::heading(ui, c, "Battles");
        let row = build::row(ui, c, space);
        let lines = build::button(ui, row, BATTLES_LINES_ID, "Lines");
        press(ui, lines, "battles", self.setup.combat.system == System::Lines);
        let strength = build::button(ui, row, BATTLES_STRENGTH_ID, "Strength");
        press(ui, strength, "battles", self.setup.combat.system == System::Strength);
        let row = build::row(ui, c, space);
        build::button(ui, row, CHANCE_LESS_ID, "-");
        build::text(ui, row, &format!("Chance {} of {MAX_CHANCE}", self.setup.combat.chance));
        build::button(ui, row, CHANCE_MORE_ID, "+");

        // The gold and the fog of war, each a row that says what it is.
        build::gap(ui, c, space);
        let row = build::row(ui, c, space);
        build::button(ui, row, GOLD_LESS_ID, "-");
        build::text(ui, row, &format!("{} gold for each side at the start", self.setup.gold));
        build::button(ui, row, GOLD_MORE_ID, "+");
        let row = build::row(ui, c, space);
        build::text(ui, row, "Fog of war");
        for (id, word, fog) in
            [(FOG_OFF_ID, "Off", Fog::Off), (FOG_ON_ID, "On", Fog::On), (FOG_TOTAL_ID, "Total", Fog::Total)]
        {
            let button = build::button(ui, row, id, word);
            press(ui, button, "fog", self.setup.fog == fog);
        }

        build::gap(ui, c, space);
        build::heading(ui, c, "Who plays");
        if self.setup.seats.is_empty() {
            build::text(ui, c, "This map has no sides: nothing to play.");
        }
        let fixed_order = self.setup.order == TurnOrder::Fixed;
        let mut names = Vec::new();
        for (i, seat) in self.setup.seats.iter().enumerate() {
            let row = build::row(ui, c, space);
            let name = side_name(seat.side);
            let label = if fixed_order { format!("{}. {name}", i + 1) } else { name.to_string() };
            names.push(build::text(ui, row, &label));
            let person = build::button(ui, row, &seat_id(seat.side, true), "Person");
            let label = match &seat.controller {
                Controller::Computer(agent) => format!("Computer: {agent}"),
                Controller::Person => format!("Computer: {COMPUTER}"),
            };
            let computer = build::button(ui, row, &seat_id(seat.side, false), &label);
            let group = format!("seat_{name}");
            press(ui, person, &group, seat.controller == Controller::Person);
            press(ui, computer, &group, seat.controller != Controller::Person);
        }
        let width = names.iter().map(|n| ui.preferred_size_free(*n).pref.w).max().unwrap_or(0);
        for name in names {
            build::set_width(ui, name, width);
        }
        let playable = self.can_start();
        if let Some(start) = self.screen.ui.find(self.screen.ui.root(), TbId::new(START_ID)) {
            self.screen.ui.set_disabled(start, !playable);
        }
        // Laid out at once, so that the new buttons are where they show.
        self.screen.settle();
    }

    /// The window was resized.
    pub fn resize(&mut self, viewport: Size) {
        self.screen.ui.set_viewport(viewport);
        self.apply_layout_sizes();
        self.rebuild();
    }

    /// Apply what the widgets recorded.
    pub fn pump(&mut self) {
        for command in self.screen.take_map_commands() {
            self.screen.navigate(command);
        }
        for command in self.screen.pump_shared() {
            let RootCommand::Click(id) = command else { continue };
            if id == TbId::new(START_ID) {
                self.start = true;
            } else if id == TbId::new(MODE_TURNS_ID) {
                self.setup.mode = Mode::Turns;
            } else if id == TbId::new(BATTLES_LINES_ID) || id == TbId::new(BATTLES_STRENGTH_ID) {
                let lines = id == TbId::new(BATTLES_LINES_ID);
                self.setup.combat.system = if lines { System::Lines } else { System::Strength };
            } else if id == TbId::new(CHANCE_LESS_ID) || id == TbId::new(CHANCE_MORE_ID) {
                let chance = &mut self.setup.combat.chance;
                *chance = if id == TbId::new(CHANCE_LESS_ID) {
                    chance.saturating_sub(1)
                } else {
                    (*chance + 1).min(MAX_CHANCE)
                };
                self.rebuild();
            } else if id == TbId::new(FOG_OFF_ID) {
                self.setup.fog = Fog::Off;
            } else if id == TbId::new(FOG_ON_ID) {
                self.setup.fog = Fog::On;
            } else if id == TbId::new(FOG_TOTAL_ID) {
                self.setup.fog = Fog::Total;
            } else if id == TbId::new(GOLD_LESS_ID) || id == TbId::new(GOLD_MORE_ID) {
                let gold = &mut self.setup.gold;
                *gold = if id == TbId::new(GOLD_LESS_ID) {
                    gold.saturating_sub(GOLD_STEP)
                } else {
                    (*gold + GOLD_STEP).min(MAX_GOLD)
                };
                self.rebuild();
            } else if id == TbId::new(ORDER_FIXED_ID) || id == TbId::new(ORDER_DRAWN_ID) {
                self.setup.order = if id == TbId::new(ORDER_FIXED_ID) { TurnOrder::Fixed } else { TurnOrder::Drawn };
                self.rebuild();
            } else if let Some(i) = (0..self.maps.len()).find(|i| id == TbId::new(&map_id(*i))) {
                self.choose(i);
            } else {
                let sides: Vec<Side> = self.setup.seats.iter().map(|s| s.side).collect();
                for side in sides {
                    let Some(seat) = self.setup.seats.iter_mut().find(|s| s.side == side) else { continue };
                    seat.controller = if id == TbId::new(&seat_id(side, true)) {
                        Controller::Person
                    } else if id == TbId::new(&seat_id(side, false)) {
                        // A computer already: its next style.
                        match &seat.controller {
                            Controller::Computer(agent) => {
                                let at = COMPUTERS.iter().position(|a| a == agent).map_or(0, |i| i + 1);
                                Controller::Computer(COMPUTERS[at % COMPUTERS.len()].to_string())
                            }
                            Controller::Person => Controller::Computer(COMPUTER.to_string()),
                        }
                    } else {
                        continue;
                    };
                    // The button says which.
                    self.rebuild();
                    break;
                }
            }
        }
        self.screen.sync_game();
    }
}

impl Scene for NewGame {
    fn screen(&self) -> &MapScreen {
        &self.screen
    }

    fn screen_mut(&mut self) -> &mut MapScreen {
        &mut self.screen
    }

    fn resize(&mut self, viewport: Size) {
        NewGame::resize(self, viewport);
    }

    fn pump(&mut self) {
        NewGame::pump(self);
    }

    fn marked(&self, _hex: Hex) -> bool {
        false
    }
}
