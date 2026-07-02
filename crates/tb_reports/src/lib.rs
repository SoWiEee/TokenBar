//! Shared report/agent logic for TokenBar frontends.
//!
//! Extracted from `tb_core_ffi` so both the macOS Swift app (via the C-ABI in
//! `tb_core_ffi`) and the Linux GTK app can drive the same session-parsing,
//! aggregation, pricing, and quota logic without duplicating it. The report
//! modules remain ports of the Tauri backend modules of the same names; keep
//! them diffable against the originals.
