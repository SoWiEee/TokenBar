//! Overview lens: headline totals (stat tiles) plus subscription-quota cards.
//! Totals arrive with the shared graph load; quota is fetched lazily (network)
//! the first time the tab is shown.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{ActionRow, Clamp, PreferencesGroup};
use gtk4::{glib, Align, Box as GtkBox, Label, LevelBar, Orientation, ScrolledWindow};
use serde_json::Value;

use crate::data::GraphSummary;
use crate::format::group_thousands;

pub struct Overview {
    root: ScrolledWindow,
    tokens: Label,
    cost: Label,
    active: Label,
    quota_group: PreferencesGroup,
    quota_loading: ActionRow,
    quota_loaded: Cell<bool>,
}

impl Overview {
    pub fn new() -> Rc<Self> {
        let tiles = GtkBox::new(Orientation::Horizontal, 24);
        tiles.set_halign(Align::Center);
        let (tokens_tile, tokens) = stat_tile("Total tokens");
        let (cost_tile, cost) = stat_tile("Total cost");
        let (active_tile, active) = stat_tile("Active days");
        tiles.append(&tokens_tile);
        tiles.append(&cost_tile);
        tiles.append(&active_tile);

        let quota_group = PreferencesGroup::builder()
            .title("Subscription quota")
            .description("OAuth quota across your logged-in agents")
            .build();
        let quota_loading = ActionRow::builder().title("Loading…").build();
        quota_group.add(&quota_loading);

        let content = GtkBox::new(Orientation::Vertical, 28);
        content.set_margin_top(28);
        content.set_margin_bottom(28);
        content.append(&tiles);
        content.append(&quota_group);

        let clamp = Clamp::builder().maximum_size(640).child(&content).build();
        let root = ScrolledWindow::builder().child(&clamp).vexpand(true).build();

        Rc::new(Self {
            root,
            tokens,
            cost,
            active,
            quota_group,
            quota_loading,
            quota_loaded: Cell::new(false),
        })
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }

    /// Fill the stat tiles from the shared graph summary.
    pub fn update(&self, summary: &GraphSummary) {
        self.tokens.set_text(&group_thousands(summary.total_tokens));
        self.cost.set_text(&format!("${:.2}", summary.total_cost));
        self.active
            .set_text(&format!("{} / {}", summary.active_days, summary.total_days));
    }

    /// Fetch the OAuth quota (network) the first time the tab is shown.
    pub fn ensure_quota_loaded(self: &Rc<Self>) {
        if self.quota_loaded.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded::<Option<Value>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(crate::data::load_quota());
        });
        let this = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(payload) = rx.recv().await {
                this.populate_quota(payload);
            }
        });
    }

    fn populate_quota(&self, payload: Option<Value>) {
        self.quota_group.remove(&self.quota_loading);

        let agents = payload
            .as_ref()
            .and_then(|p| p.get("agents"))
            .and_then(Value::as_array);
        let Some(agents) = agents.filter(|a| !a.is_empty()) else {
            let row = ActionRow::builder()
                .title("No quota available")
                .subtitle("Sign in to Claude, Codex, … to see subscription usage")
                .build();
            self.quota_group.add(&row);
            return;
        };

        for agent in agents {
            let name = agent_name(agent);
            if let Some(err) = agent.get("error").and_then(Value::as_str) {
                let row = ActionRow::builder().title(&name).subtitle(err).build();
                self.quota_group.add(&row);
                continue;
            }
            let windows = agent.get("windows").and_then(Value::as_array);
            match windows.filter(|w| !w.is_empty()) {
                Some(windows) => {
                    for w in windows {
                        self.quota_group.add(&quota_row(&name, w));
                    }
                }
                None => {
                    let row = ActionRow::builder()
                        .title(&name)
                        .subtitle("No usage window reported")
                        .build();
                    self.quota_group.add(&row);
                }
            }
        }
    }
}

/// "Claude · Pro" style label from a snapshot's clientId + plan.
fn agent_name(agent: &Value) -> String {
    let client = agent
        .get("clientId")
        .and_then(Value::as_str)
        .unwrap_or("agent");
    let mut name = capitalize(client);
    if let Some(plan) = agent
        .get("identity")
        .and_then(|i| i.get("plan"))
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
    {
        name.push_str(" · ");
        name.push_str(plan);
    }
    name
}

/// One usage-window row: "{agent} — {window}", a remaining-% bar, and reset text.
fn quota_row(agent: &str, window: &Value) -> ActionRow {
    let label = window.get("label").and_then(Value::as_str).unwrap_or("");
    let remaining = window
        .get("remainingPercent")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let reset = window
        .get("resetText")
        .and_then(Value::as_str)
        .unwrap_or("");

    let bar = LevelBar::builder()
        .min_value(0.0)
        .max_value(100.0)
        .value(remaining)
        .width_request(120)
        .valign(Align::Center)
        .build();
    let pct = Label::new(Some(&format!("{remaining:.0}%")));
    pct.add_css_class("numeric");
    pct.add_css_class("dim-label");
    pct.set_width_chars(4);
    pct.set_valign(Align::Center);

    let suffix = GtkBox::new(Orientation::Horizontal, 8);
    suffix.append(&bar);
    suffix.append(&pct);

    let row = ActionRow::builder()
        .title(&format!("{agent} — {label}"))
        .subtitle(if reset.is_empty() { "remaining" } else { reset })
        .build();
    row.add_suffix(&suffix);
    row
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
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
