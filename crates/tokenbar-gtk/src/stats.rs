//! Stats lens: headline totals plus activity streaks, computed from the shared
//! contribution-graph payload (no extra parse). Lazy-loaded.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{ActionRow, PreferencesGroup, PreferencesPage};
use chrono::{Duration, NaiveDate};
use gtk4::{glib, Align, Label};
use serde_json::Value;

use crate::data;
use crate::format::{compact, group_thousands};

pub struct StatsView {
    page: PreferencesPage,
    group: PreferencesGroup,
    loading_row: ActionRow,
    loaded: Cell<bool>,
}

impl StatsView {
    pub fn new() -> Rc<Self> {
        let page = PreferencesPage::new();
        let group = PreferencesGroup::builder().title("Statistics").build();
        let loading_row = ActionRow::builder().title("Loading…").build();
        group.add(&loading_row);
        page.add(&group);
        Rc::new(Self { page, group, loading_row, loaded: Cell::new(false) })
    }

    pub fn widget(&self) -> &PreferencesPage {
        &self.page
    }

    pub fn ensure_loaded(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded::<Option<Value>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(data::load_graph_raw());
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

        let Some(payload) = payload else {
            self.add_stat("No data", "—");
            return;
        };
        let s = payload.get("summary");
        let i64f = |k: &str| s.and_then(|s| s.get(k)).and_then(Value::as_i64).unwrap_or(0);
        let f64f = |k: &str| s.and_then(|s| s.get(k)).and_then(Value::as_f64).unwrap_or(0.0);

        let active = i64f("activeDays");
        let total_cost = f64f("totalCost");
        let avg = if active > 0 { total_cost / active as f64 } else { 0.0 };
        let (current, longest) = streaks(&active_dates(&payload));

        self.add_stat("Total tokens", &compact(i64f("totalTokens")));
        self.add_stat("Total cost", &format!("${total_cost:.2}"));
        self.add_stat("Active days", &format!("{active} / {}", i64f("totalDays")));
        self.add_stat("Average per active day", &format!("${avg:.2}"));
        self.add_stat("Busiest day", &format!("${:.2}", f64f("maxCostInSingleDay")));
        self.add_stat("Current streak", &streak_label(current));
        self.add_stat("Longest streak", &streak_label(longest));
        let _ = group_thousands; // reserved for future message-count stats
    }

    fn add_stat(&self, title: &str, value: &str) {
        let label = Label::new(Some(value));
        label.add_css_class("numeric");
        label.set_valign(Align::Center);
        let row = ActionRow::builder().title(title).build();
        row.add_suffix(&label);
        self.group.add(&row);
    }
}

fn streak_label(days: i64) -> String {
    match days {
        1 => "1 day".to_string(),
        n => format!("{n} days"),
    }
}

/// Sorted, unique active dates (days with any tokens).
fn active_dates(payload: &Value) -> Vec<NaiveDate> {
    let Some(days) = payload.get("contributions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut dates: Vec<NaiveDate> = days
        .iter()
        .filter(|d| {
            d.get("totals")
                .and_then(|t| t.get("tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0)
                > 0
        })
        .filter_map(|d| {
            d.get("date")
                .and_then(Value::as_str)
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        })
        .collect();
    dates.sort_unstable();
    dates.dedup();
    dates
}

/// (current streak, longest streak) of consecutive active calendar days.
/// The current streak is the run ending on the most recent active day.
fn streaks(dates: &[NaiveDate]) -> (i64, i64) {
    if dates.is_empty() {
        return (0, 0);
    }
    let mut longest = 1i64;
    let mut run = 1i64;
    let mut current = 1i64;
    for pair in dates.windows(2) {
        if pair[1] == pair[0] + Duration::days(1) {
            run += 1;
        } else {
            run = 1;
        }
        longest = longest.max(run);
        current = run; // ends up as the run through the last (most recent) day
    }
    (current, longest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn streaks_finds_current_and_longest() {
        // A 3-day run, a gap, then a 2-day run ending most-recently.
        let dates = [
            d("2026-06-01"),
            d("2026-06-02"),
            d("2026-06-03"),
            d("2026-06-10"),
            d("2026-06-11"),
        ];
        assert_eq!(streaks(&dates), (2, 3));
    }

    #[test]
    fn streaks_handles_single_and_empty() {
        assert_eq!(streaks(&[]), (0, 0));
        assert_eq!(streaks(&[d("2026-06-01")]), (1, 1));
    }
}
