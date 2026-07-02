//! A reusable lazy-loaded list lens: a scrollable boxed list, one row per
//! entry, backed by a cached report. Models and Agents are both instances —
//! they differ only in which payload they load and how a row is built.
//!
//! `load` and `map` are plain function pointers (Copy + Send) so `load` can run
//! on a worker thread while the lens itself stays on the (Rc, !Send) UI side.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{ActionRow, PreferencesGroup, PreferencesPage};
use gtk4::{glib, Align, Label};
use serde_json::Value;

/// One rendered row: a title, a dim subtitle, and a right-aligned value.
pub struct ListRow {
    pub title: String,
    pub subtitle: String,
    pub value: String,
}

/// Read a string field, falling back to `default`.
pub fn str_field(v: &Value, key: &str, default: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or(default).to_string()
}

/// Read an integer field, defaulting to 0.
pub fn i64_field(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

pub struct ListLens {
    page: PreferencesPage,
    group: PreferencesGroup,
    loading_row: ActionRow,
    loaded: Cell<bool>,
    load: fn() -> Option<Value>,
    map: fn(&Value) -> Vec<ListRow>,
    empty_title: &'static str,
    empty_subtitle: &'static str,
}

impl ListLens {
    pub fn new(
        group_title: &str,
        empty_title: &'static str,
        empty_subtitle: &'static str,
        load: fn() -> Option<Value>,
        map: fn(&Value) -> Vec<ListRow>,
    ) -> Rc<Self> {
        let page = PreferencesPage::new();
        let group = PreferencesGroup::builder().title(group_title).build();
        let loading_row = ActionRow::builder().title("Loading…").build();
        group.add(&loading_row);
        page.add(&group);
        Rc::new(Self {
            page,
            group,
            loading_row,
            loaded: Cell::new(false),
            load,
            map,
            empty_title,
            empty_subtitle,
        })
    }

    pub fn widget(&self) -> &PreferencesPage {
        &self.page
    }

    /// Load the report the first time the tab becomes visible.
    pub fn ensure_loaded(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let load = self.load;
        let (tx, rx) = async_channel::bounded::<Option<Value>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(load());
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

        let rows = payload.as_ref().map(|p| (self.map)(p)).unwrap_or_default();
        if rows.is_empty() {
            let empty = ActionRow::builder()
                .title(self.empty_title)
                .subtitle(self.empty_subtitle)
                .build();
            self.group.add(&empty);
            return;
        }

        for r in rows {
            let value = Label::new(Some(&r.value));
            value.add_css_class("numeric");
            value.set_valign(Align::Center);
            let row = ActionRow::builder().title(&r.title).subtitle(&r.subtitle).build();
            row.add_suffix(&value);
            self.group.add(&row);
        }
    }
}
