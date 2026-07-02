//! Models lens: every model ranked by cost, backed by
//! `tb_reports::model_report`. Loaded lazily the first time the tab is shown
//! (the report re-parses logs), on a worker thread so the UI stays responsive.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{ActionRow, PreferencesGroup, PreferencesPage};
use gtk4::{glib, Align, Label};
use serde_json::Value;

use crate::data;
use crate::format::{compact, group_thousands};

pub struct ModelsView {
    page: PreferencesPage,
    group: PreferencesGroup,
    loading_row: ActionRow,
    loaded: Cell<bool>,
}

struct ModelRow {
    model: String,
    provider: String,
    total: i64,
    messages: i64,
    cost: f64,
}

impl ModelsView {
    pub fn new() -> Rc<Self> {
        let page = PreferencesPage::new();
        let group = PreferencesGroup::builder().title("Models by cost").build();
        let loading_row = ActionRow::builder().title("Loading…").build();
        group.add(&loading_row);
        page.add(&group);
        Rc::new(Self { page, group, loading_row, loaded: Cell::new(false) })
    }

    pub fn widget(&self) -> &PreferencesPage {
        &self.page
    }

    /// Load the report the first time the tab becomes visible.
    pub fn ensure_loaded(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded::<Option<Value>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(data::load_models());
        });
        let this = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(payload) = rx.recv().await {
                this.populate(payload);
            }
        });
    }

    fn populate(&self, payload: Option<Value>) {
        self.group.remove(&self.loading_row);

        let rows = payload.as_ref().and_then(parse_rows).unwrap_or_default();
        if rows.is_empty() {
            let empty = ActionRow::builder()
                .title("No model usage found")
                .subtitle("Run an agent, then reopen TokenBar")
                .build();
            self.group.add(&empty);
            return;
        }

        for r in rows {
            let cost = Label::new(Some(&format!("${:.2}", r.cost)));
            cost.add_css_class("numeric");
            cost.set_valign(Align::Center);
            let row = ActionRow::builder()
                .title(&r.model)
                .subtitle(&format!(
                    "{} · {} tokens · {} msgs",
                    r.provider,
                    compact(r.total),
                    group_thousands(r.messages)
                ))
                .build();
            row.add_suffix(&cost);
            self.group.add(&row);
        }
    }
}

/// Parse the `entries` array into cost-descending rows.
fn parse_rows(payload: &Value) -> Option<Vec<ModelRow>> {
    let entries = payload.get("entries")?.as_array()?;
    let mut rows: Vec<ModelRow> = entries
        .iter()
        .map(|e| ModelRow {
            model: str_field(e, "model", "unknown"),
            provider: str_field(e, "provider", ""),
            total: i64_field(e, "total"),
            messages: i64_field(e, "messageCount"),
            cost: e.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
        })
        .collect();
    rows.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    Some(rows)
}

fn str_field(v: &Value, key: &str, default: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn i64_field(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}
