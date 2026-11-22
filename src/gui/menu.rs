//! A menu bar, built from code: a title for each menu (a flat button), and,
//! while one is open, its items in a box under its title, a row each: what it
//! does on the left, its keys on the right. An item is a button: a click on it
//! reaches the root window as any button's does (`RootCommand::Click` with the
//! item's id), so an item and a button with the same id do the same thing.
//!
//! The scene says what the menus hold (it builds a [`Menu`] when one opens, so
//! that an item can say what it would do now) and opens and closes them: a
//! click on a title opens its menu, or closes it if it is the one open; any
//! other click, a press on the map, or Esc closes it. Nothing here takes the
//! keyboard: it stays with the map.
//!
//! Not a port: the C++ had no menus.

use turbo_ferret::layout::stack::{LayoutDistributionPosition, LayoutPosition};
use turbo_ferret::widgets::Layout;
use turbo_ferret::{Axis, LayoutParams, Point, Rect, TbId, Ui, WidgetId};

use super::build;

/// The skins of a title, a menu's box, a row and the keys in it
/// (`assets/skin/hexlords.tb.txt`).
pub const TITLE_SKIN: &str = "MenuTitle";
pub const DROP_SKIN: &str = "MenuDrop";
pub const ROW_SKIN: &str = "ListRow";
pub const KEYS_SKIN: &str = "MenuKeys";
/// The id of an open menu's box.
pub const DROP_ID: &str = "menu_drop";
/// The room between an item's words and its keys, and around a line between
/// items, in dp.
const KEYS_GAP_DP: i32 = 28;
const SEPARATOR_DP: i32 = 6;

/// An item in words: its id, what it does, its keys.
pub type ItemWords<'a> = (&'a str, &'a str, &'a str);

/// What a menu holds, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// What it does, under the id of the click it makes; and its keys.
    Item { id: String, label: String, keys: String, enabled: bool },
    /// A little room between two groups of items.
    Separator,
}

/// A menu: its title's id, and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub id: String,
    pub entries: Vec<Entry>,
}

impl Menu {
    pub fn new(id: &str) -> Menu {
        Menu { id: id.to_string(), entries: Vec::new() }
    }

    /// An item, `enabled` or greyed.
    pub fn item(mut self, id: &str, label: &str, keys: &str, enabled: bool) -> Menu {
        let (id, label, keys) = (id.to_string(), label.to_string(), keys.to_string());
        self.entries.push(Entry::Item { id, label, keys, enabled });
        self
    }

    pub fn separator(mut self) -> Menu {
        self.entries.push(Entry::Separator);
        self
    }

    /// Its items: the id, the words and the keys of each.
    pub fn items(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Item { id, label, keys, .. } => Some((id.as_str(), label.as_str(), keys.as_str())),
            Entry::Separator => None,
        })
    }
}

/// The titles, and the menu open, if one is.
pub struct MenuBar {
    titles: Vec<(TbId, WidgetId)>,
    open: Option<Open>,
}

struct Open {
    menu: TbId,
    drop: WidgetId,
    items: Vec<(String, String, String)>,
}

impl MenuBar {
    /// Put a title for each menu (its id and its words) in `parent`, a layout
    /// of the menu bar.
    pub fn new(ui: &mut Ui, parent: WidgetId, titles: &[(&str, &str)]) -> MenuBar {
        let titles = titles
            .iter()
            .map(|(id, title)| {
                let button = build::button(ui, parent, id, title);
                ui.set_skin(button, TbId::new(TITLE_SKIN));
                (TbId::new(id), button)
            })
            .collect();
        MenuBar { titles, open: None }
    }

    /// True if `id` is one of the titles.
    pub fn is_title(&self, id: TbId) -> bool {
        self.titles.iter().any(|(title, _)| *title == id)
    }

    /// The title under the pointer, if any.
    pub fn hovered_title(&self, ui: &Ui) -> Option<TbId> {
        let over = ui.hovered()?;
        self.titles.iter().find(|(_, button)| ui.is_ancestor_of(*button, over)).map(|(id, _)| *id)
    }

    /// The id of the menu open, if one is (and it was not closed under us).
    pub fn open_menu(&self, ui: &Ui) -> Option<TbId> {
        self.open.as_ref().filter(|o| build::is_open(ui, o.drop)).map(|o| o.menu)
    }

    /// The open menu's items: the id, the words and the keys of each.
    pub fn items(&self) -> Vec<ItemWords<'_>> {
        self.open.iter().flat_map(|o| &o.items).map(|(i, l, k)| (i.as_str(), l.as_str(), k.as_str())).collect()
    }

    /// Open `menu` under its title, on `root` (the root window), closing the
    /// one that was open.
    pub fn open(&mut self, ui: &mut Ui, root: WidgetId, menu: &Menu) {
        self.close(ui);
        let Some(&(id, title)) = self.titles.iter().find(|(id, _)| *id == TbId::new(&menu.id)) else { return };
        let dp = |ui: &Ui, n: i32| ui.dimension_converter().dp_to_px(n);
        let mut layout = Layout::new(Axis::Y);
        layout.position = LayoutPosition::LeftTop;
        layout.distribution_position = LayoutDistributionPosition::LeftTop;
        layout.spacing = Some(0);
        let drop = ui.add(root, layout);
        ui.set_id(drop, TbId::new(DROP_ID));
        ui.set_skin(drop, TbId::new(DROP_SKIN));
        let mut rows = Vec::new();
        for entry in &menu.entries {
            match entry {
                Entry::Item { id, label, keys, enabled } => {
                    let button = build::button(ui, drop, id, "");
                    ui.set_skin(button, TbId::new(ROW_SKIN));
                    let row = build::row(ui, button, dp(ui, KEYS_GAP_DP));
                    let words = build::text(ui, row, label);
                    let keys_field = build::text(ui, row, keys);
                    ui.set_skin(keys_field, TbId::new(KEYS_SKIN));
                    ui.set_disabled(button, !enabled);
                    rows.push((button, words, keys_field));
                }
                Entry::Separator => {
                    let px = dp(ui, SEPARATOR_DP);
                    build::gap(ui, drop, px);
                }
            }
        }
        // The words in one column and the keys in another, every row as wide
        // as the widest.
        let widest = |ui: &mut Ui, pick: fn(&(WidgetId, WidgetId, WidgetId)) -> WidgetId| {
            rows.iter().map(|r| ui.preferred_size_free(pick(r)).pref.w).max().unwrap_or(0)
        };
        let (words_w, keys_w) = (widest(ui, |r| r.1), widest(ui, |r| r.2));
        for (_, words, keys) in &rows {
            build::set_width(ui, *words, words_w);
            build::set_width(ui, *keys, keys_w);
        }
        let row_w = rows.iter().map(|(b, _, _)| ui.preferred_size_free(*b).pref.w).max().unwrap_or(0);
        for (button, _, _) in &rows {
            build::set_width(ui, *button, row_w);
        }
        // Under its title, inside the window.
        let at = ui.to_root(title, Point::ZERO);
        let below = Point::new(at.x, at.y + ui.rect(title).h);
        let size = ui.preferred_size_free(drop).pref;
        let bounds = Rect::from_size(ui.rect(root).size());
        ui.set_layout_params(drop, Some(LayoutParams::fixed(size.w, size.h)));
        ui.set_rect(drop, Rect::new(below.x, below.y, size.w, size.h).move_in(&bounds).clip(&bounds));
        ui.set_value(title, 1);
        let items = menu.items().map(|(i, l, k)| (i.to_string(), l.to_string(), k.to_string())).collect();
        self.open = Some(Open { menu: id, drop, items });
    }

    /// Close the menu that is open. True if one was.
    pub fn close(&mut self, ui: &mut Ui) -> bool {
        let Some(open) = self.open.take() else { return false };
        if let Some((_, title)) = self.titles.iter().find(|(id, _)| *id == open.menu) {
            ui.set_value(*title, 0);
        }
        let was = build::is_open(ui, open.drop);
        if was {
            ui.remove(open.drop);
        }
        was
    }
}
