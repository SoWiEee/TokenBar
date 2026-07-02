//! Agents lens: sub-agents ranked by cost (a `ListLens` over
//! `tb_reports::agents_report`). Messages with no agent attribution fold into a
//! single "Main" bucket upstream, so every message is accounted for.

use std::rc::Rc;

use serde_json::Value;

use crate::data;
use crate::format::{compact, group_thousands};
use crate::list_lens::{i64_field, str_field, ListLens, ListRow};

pub fn new() -> Rc<ListLens> {
    ListLens::new(
        "Agents by cost",
        "No agent usage found",
        "Run an agent, then reopen TokenBar",
        data::load_agents,
        map_rows,
    )
}

fn map_rows(payload: &Value) -> Vec<ListRow> {
    let Some(entries) = payload.get("entries").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut rows: Vec<(f64, ListRow)> = entries
        .iter()
        .map(|e| {
            let cost = e.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
            let clients = e
                .get("clients")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let row = ListRow {
                title: str_field(e, "agent", "Main"),
                subtitle: format!(
                    "{} · {} tokens · {} msgs",
                    clients,
                    compact(i64_field(e, "total")),
                    group_thousands(i64_field(e, "messages"))
                ),
                value: format!("${cost:.2}"),
            };
            (cost, row)
        })
        .collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    rows.into_iter().map(|(_, row)| row).collect()
}
