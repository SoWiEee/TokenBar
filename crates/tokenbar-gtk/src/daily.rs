//! Daily lens: each active day, most recent first, from the shared
//! contribution-graph payload (no extra parse).

use std::rc::Rc;

use serde_json::Value;

use crate::data;
use crate::format::{compact, group_thousands};
use crate::list_lens::{ListLens, ListRow};

pub fn new() -> Rc<ListLens> {
    ListLens::new(
        "Daily activity",
        "No daily usage found",
        "Run an agent, then reopen TokenBar",
        data::load_graph_raw,
        map_rows,
    )
}

fn map_rows(payload: &Value) -> Vec<ListRow> {
    let Some(days) = payload.get("contributions").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut rows: Vec<(String, ListRow)> = days
        .iter()
        .filter_map(|d| {
            let date = d.get("date").and_then(Value::as_str)?.to_string();
            let totals = d.get("totals");
            let tokens = totals
                .and_then(|t| t.get("tokens"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if tokens == 0 {
                return None; // skip empty days
            }
            let cost = totals
                .and_then(|t| t.get("cost"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let messages = totals
                .and_then(|t| t.get("messages"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let row = ListRow {
                title: date.clone(),
                subtitle: format!(
                    "{} tokens · {} msgs",
                    compact(tokens),
                    group_thousands(messages)
                ),
                value: format!("${cost:.2}"),
            };
            Some((date, row))
        })
        .collect();
    // Most recent day first (ISO dates sort lexically).
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter().map(|(_, row)| row).collect()
}
