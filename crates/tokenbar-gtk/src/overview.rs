//! Overview lens: headline totals from the usage-graph summary, shown as a row
//! of stat tiles. Data arrives asynchronously, so labels start as placeholders
//! and `update` fills them in.

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label, Orientation};

use crate::data::GraphSummary;
use crate::format::group_thousands;

pub struct Overview {
    root: GtkBox,
    tokens: Label,
    cost: Label,
    active: Label,
}

impl Overview {
    pub fn new() -> Self {
        let tiles = GtkBox::new(Orientation::Horizontal, 24);
        tiles.set_halign(Align::Center);
        tiles.set_valign(Align::Center);
        tiles.set_hexpand(true);
        tiles.set_vexpand(true);

        let (tokens_tile, tokens) = stat_tile("Total tokens");
        let (cost_tile, cost) = stat_tile("Total cost");
        let (active_tile, active) = stat_tile("Active days");
        tiles.append(&tokens_tile);
        tiles.append(&cost_tile);
        tiles.append(&active_tile);

        Self { root: tiles, tokens, cost, active }
    }

    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    pub fn update(&self, summary: &GraphSummary) {
        self.tokens.set_text(&group_thousands(summary.total_tokens));
        self.cost.set_text(&format!("${:.2}", summary.total_cost));
        self.active
            .set_text(&format!("{} / {}", summary.active_days, summary.total_days));
    }
}

/// A single stat tile: a big value over a dim caption.
fn stat_tile(caption: &str) -> (GtkBox, Label) {
    let tile = GtkBox::new(Orientation::Vertical, 4);
    tile.set_halign(Align::Center);

    let value = Label::new(Some("…"));
    value.add_css_class("title-1");

    let cap = Label::new(Some(caption));
    cap.add_css_class("dim-label");
    cap.add_css_class("caption");

    tile.append(&value);
    tile.append(&cap);
    (tile, value)
}

