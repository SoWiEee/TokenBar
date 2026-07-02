//! Bridge from the shared `tb_reports` core to renderable bars.
//!
//! Calls `tb_reports::usage_graph::run` (the same contribution-graph payload the
//! macOS app consumes) and lays each day out GitHub-style — week = column,
//! weekday = row — reusing the pure `graph::grid_from` builder. Token totals are
//! sqrt-compressed against the busiest day so a few heavy days don't flatten the
//! rest.
//!
//! TODO(phase 2/3): this runs on the caller's thread (blocking; first call may
//! fetch pricing). Move it onto a worker thread feeding the UI via a channel.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};

use crate::graph::{self, Bar};

/// Real usage bars, falling back to the demo grid when there is no local data or
/// the core call fails (so the window is never blank during development).
pub fn load_bars() -> Vec<Bar> {
    match tb_reports::usage_graph::run("") {
        Ok(payload) => match bars_from_payload(&payload) {
            Some(bars) => bars,
            None => {
                eprintln!("usage graph had no contributions; using demo grid");
                graph::demo_grid(53, 7)
            }
        },
        Err(e) => {
            eprintln!("usage_graph::run failed: {e}; using demo grid");
            graph::demo_grid(53, 7)
        }
    }
}

/// Map the `contributions` array of the usage-graph payload into a full
/// weeks × 7 grid (missing days become empty cells). Returns None when the
/// payload has no usable contributions.
fn bars_from_payload(payload: &serde_json::Value) -> Option<Vec<Bar>> {
    let contributions = payload.get("contributions")?.as_array()?;

    // (date, token total) for every day that parsed.
    let mut days: Vec<(NaiveDate, f64)> = Vec::with_capacity(contributions.len());
    for c in contributions {
        let Some(date) = c
            .get("date")
            .and_then(|d| d.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        else {
            continue;
        };
        let tokens = c
            .get("totals")
            .and_then(|t| t.get("tokens"))
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        days.push((date, tokens as f64));
    }
    if days.is_empty() {
        return None;
    }

    // Column 0 starts on the Sunday on/before the earliest day so weekdays align
    // to fixed rows across every column.
    let earliest = days.iter().map(|(d, _)| *d).min()?;
    let start = earliest - Duration::days(earliest.weekday().num_days_from_sunday() as i64);
    let max_tokens = days.iter().map(|(_, t)| *t).fold(0.0_f64, f64::max).max(1.0);

    let mut intensity: HashMap<(usize, usize), f32> = HashMap::with_capacity(days.len());
    let mut max_col = 0usize;
    for (date, tokens) in &days {
        let offset = (*date - start).num_days();
        if offset < 0 {
            continue;
        }
        let col = (offset / 7) as usize;
        let row = date.weekday().num_days_from_sunday() as usize;
        max_col = max_col.max(col);
        // sqrt compresses the heavy-tailed token distribution into [0, 1].
        let v = ((*tokens / max_tokens) as f32).sqrt();
        intensity.insert((col, row), v);
    }

    let cols = max_col + 1;
    Some(graph::grid_from(cols, 7, |c, r| {
        intensity.get(&(c, r)).copied().unwrap_or(0.0)
    }))
}
