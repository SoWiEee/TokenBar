//! Bridge from the shared `tb_reports` core to the GTK views.
//!
//! Calls `tb_reports::usage_graph::run` (the same contribution-graph payload the
//! macOS app consumes) and extracts both the 3D bars (week × weekday) and the
//! headline summary. Token totals are sqrt-compressed against the busiest day so
//! a few heavy days don't flatten the rest.
//!
//! TODO(phase 2/3): this runs on the caller's thread (blocking; first call may
//! fetch pricing). It is already driven from a worker thread in `main`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::Value;

use crate::graph::{self, Bar};

// --- Disk cache, keyed by newest source mtime --------------------------------
//
// Each report re-parses ~1.5 GB of session logs (tens of seconds). We cache the
// computed payload per report in ~/.cache/tokenbar-gtk, keyed by the newest
// source mtime (a fast stat sweep): unchanged logs → reload instead of
// re-parse. TOKENBAR_CACHE_STALE_OK=1 serves the cache regardless of freshness
// (fast iteration / opening while an agent is actively writing logs, which
// would otherwise bump the mtime key every few seconds).

fn cache_path(name: &str) -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("tokenbar-gtk").join(name))
}

fn source_token() -> Option<u64> {
    tokscale_core::latest_source_mtime_ms(&tokscale_core::LocalParseOptions::default()).ok()
}

fn read_cache_file(name: &str) -> Option<Value> {
    let raw = std::fs::read_to_string(cache_path(name)?).ok()?;
    serde_json::from_str(&raw).ok()
}

fn read_cache(name: &str, token: u64) -> Option<Value> {
    let cached = read_cache_file(name)?;
    if cached.get("token")?.as_u64()? == token {
        cached.get("payload").cloned()
    } else {
        None
    }
}

fn read_cache_any(name: &str) -> Option<Value> {
    read_cache_file(name)?.get("payload").cloned()
}

fn write_cache(name: &str, token: u64, payload: &Value) {
    let Some(path) = cache_path(name) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let entry = serde_json::json!({ "token": token, "payload": payload });
    if let Ok(text) = serde_json::to_string(&entry) {
        let _ = std::fs::write(&path, text);
    }
}

/// Load a report payload through the disk cache: stale-ok short-circuit, then
/// mtime-keyed hit, else run `compute`, cache, and return it. `label` is used
/// only for the timing log line.
fn cached_payload(
    cache_file: &str,
    label: &str,
    compute: impl FnOnce() -> Result<Value, String>,
) -> Option<Value> {
    let started = Instant::now();

    if std::env::var_os("TOKENBAR_CACHE_STALE_OK").is_some() {
        if let Some(payload) = read_cache_any(cache_file) {
            eprintln!("{label} cache (stale-ok) in {:?}", started.elapsed());
            return Some(payload);
        }
    }

    let token = source_token();
    if let Some(token) = token {
        if let Some(payload) = read_cache(cache_file, token) {
            eprintln!("{label} cache hit in {:?}", started.elapsed());
            return Some(payload);
        }
    }

    match compute() {
        Ok(payload) => {
            eprintln!("{label} parsed in {:?}", started.elapsed());
            if let Some(token) = token {
                write_cache(cache_file, token, &payload);
            }
            Some(payload)
        }
        Err(e) => {
            eprintln!("{label} failed: {e}");
            None
        }
    }
}

/// Raw per-model report payload (cached). Interpreted by the Models lens.
pub fn load_models() -> Option<Value> {
    cached_payload("models-cache.json", "models", || {
        tb_reports::model_report::run("")
    })
}

/// Headline totals for the Overview lens.
#[derive(Debug, Clone, Default)]
pub struct GraphSummary {
    pub total_tokens: i64,
    pub total_cost: f64,
    pub active_days: i64,
    pub total_days: i64,
}

/// Everything the views need from one usage-graph load.
pub struct GraphData {
    pub bars: Vec<Bar>,
    pub summary: GraphSummary,
}

/// Load usage from the shared core. Falls back to the demo grid + empty summary
/// when there is no local data or the core call fails, so the UI is never blank.
pub fn load() -> GraphData {
    let payload = cached_payload("graph-cache.json", "graph", || {
        tb_reports::usage_graph::run("")
    });
    match payload {
        Some(payload) => {
            let bars = bars_from_payload(&payload).unwrap_or_else(|| {
                eprintln!("usage graph had no contributions; using demo grid");
                graph::demo_grid(53, 7)
            });
            let summary = summary_from_payload(&payload);
            GraphData { bars, summary }
        }
        None => GraphData {
            bars: graph::demo_grid(53, 7),
            summary: GraphSummary::default(),
        },
    }
}

fn summary_from_payload(payload: &serde_json::Value) -> GraphSummary {
    let s = payload.get("summary");
    let field_i64 = |k: &str| s.and_then(|s| s.get(k)).and_then(|v| v.as_i64()).unwrap_or(0);
    let field_f64 = |k: &str| s.and_then(|s| s.get(k)).and_then(|v| v.as_f64()).unwrap_or(0.0);
    GraphSummary {
        total_tokens: field_i64("totalTokens"),
        total_cost: field_f64("totalCost"),
        active_days: field_i64("activeDays"),
        total_days: field_i64("totalDays"),
    }
}

/// Map the `contributions` array into a full weeks × 7 grid (missing days become
/// empty cells). Returns None when there are no usable contributions.
fn bars_from_payload(payload: &serde_json::Value) -> Option<Vec<Bar>> {
    let contributions = payload.get("contributions")?.as_array()?;

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
        let v = ((*tokens / max_tokens) as f32).sqrt();
        intensity.insert((col, row), v);
    }

    let cols = max_col + 1;
    Some(graph::grid_from(cols, 7, |c, r| {
        intensity.get(&(c, r)).copied().unwrap_or(0.0)
    }))
}
