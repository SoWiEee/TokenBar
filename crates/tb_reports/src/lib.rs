//! Shared report/agent logic for TokenBar frontends.
//!
//! Extracted from `tb_core_ffi` so both the macOS Swift app (via the C-ABI in
//! `tb_core_ffi`) and the Linux GTK app can drive the same session-parsing,
//! aggregation, pricing, and quota logic without duplicating it. The report
//! modules remain ports of the Tauri backend modules of the same names; keep
//! them diffable against the originals.

pub mod agent_antigravity;
pub mod agent_copilot;
pub mod agent_history;
pub mod agent_usage;
pub mod agents_report;
pub mod hourly_report;
pub mod model_report;
pub mod opencode_integrations;
pub mod usage_graph;
pub mod usage_tail;
