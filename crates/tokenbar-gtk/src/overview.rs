//! Overview lens: headline totals from the usage-graph summary, shown as a row
//! of stat tiles. Data arrives asynchronously, so labels start as placeholders
//! and `update` fills them in.

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label, Orientation};

use crate::data::GraphSummary;

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

/// Format an integer with thousands separators (e.g. 1234567 -> "1,234,567").
fn group_thousands(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    let bytes = digits.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::group_thousands;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(42), "42");
        assert_eq!(group_thousands(1234), "1,234");
        assert_eq!(group_thousands(1234567), "1,234,567");
        assert_eq!(group_thousands(-9876543), "-9,876,543");
    }
}
