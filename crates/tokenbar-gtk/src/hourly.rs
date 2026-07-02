//! Hourly lens: a 24-hour-of-day distribution of token usage, folded from
//! `tb_reports::hourly_report`'s per-slot entries and shown as a row of
//! proportional bars — "when in the day you burn tokens". Lazy-loaded.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use adw::Clamp;
use gtk4::{glib, Align, Box as GtkBox, Label, LevelBar, Orientation, ScrolledWindow};
use serde_json::Value;

use crate::data;
use crate::format::compact;

pub struct HourlyView {
    root: ScrolledWindow,
    list: GtkBox,
    loaded: Cell<bool>,
}

impl HourlyView {
    pub fn new() -> Rc<Self> {
        let list = GtkBox::new(Orientation::Vertical, 4);
        list.set_margin_top(18);
        list.set_margin_bottom(18);

        let loading = Label::new(Some("Loading…"));
        loading.add_css_class("dim-label");
        list.append(&loading);

        let clamp = Clamp::builder().maximum_size(560).child(&list).build();
        let root = ScrolledWindow::builder().child(&clamp).vexpand(true).build();
        Rc::new(Self { root, list, loaded: Cell::new(false) })
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }

    pub fn ensure_loaded(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded::<Option<Value>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(data::load_hourly());
        });
        let this = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(payload) = rx.recv().await {
                this.populate(payload);
            }
        });
    }

    fn populate(&self, payload: Option<Value>) {
        // Clear the loading placeholder.
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let buckets = payload.as_ref().map(fold_by_hour).unwrap_or([0; 24]);
        let max = buckets.iter().copied().max().unwrap_or(0).max(1);

        for (hour, &tokens) in buckets.iter().enumerate() {
            self.list.append(&hour_row(hour, tokens, max));
        }
    }
}

/// One "HH:00  [====bar====]  1.2M" row.
fn hour_row(hour: usize, tokens: i64, max: i64) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 12);

    let label = Label::new(Some(&format!("{hour:02}:00")));
    label.add_css_class("dim-label");
    label.add_css_class("numeric");
    label.set_width_chars(5);
    label.set_xalign(0.0);

    let bar = LevelBar::builder()
        .min_value(0.0)
        .max_value(1.0)
        .value(tokens as f64 / max as f64)
        .hexpand(true)
        .valign(Align::Center)
        .build();

    let count = Label::new(Some(&compact(tokens)));
    count.add_css_class("numeric");
    count.add_css_class("dim-label");
    count.set_width_chars(6);
    count.set_xalign(1.0);

    row.append(&label);
    row.append(&bar);
    row.append(&count);
    row
}

/// Sum tokens into 24 hour-of-day buckets from the "YYYY-MM-DD HH:00" slots.
fn fold_by_hour(payload: &Value) -> [i64; 24] {
    let mut buckets = [0i64; 24];
    let Some(entries) = payload.get("entries").and_then(Value::as_array) else {
        return buckets;
    };
    for e in entries {
        let Some(hour) = e
            .get("hour")
            .and_then(Value::as_str)
            .and_then(|s| s.split(' ').nth(1))
            .and_then(|hm| hm.split(':').next())
            .and_then(|hh| hh.parse::<usize>().ok())
        else {
            continue;
        };
        if hour < 24 {
            buckets[hour] += e.get("total").and_then(Value::as_i64).unwrap_or(0);
        }
    }
    buckets
}
