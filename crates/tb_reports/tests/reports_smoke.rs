//! Integration smoke tests enabled by extracting tb_reports out of the
//! staticlib. Run each report against an isolated empty HOME so there is no
//! local session data: the pure logic must degrade to an empty-but-Ok result,
//! never an error or a panic.

use std::path::PathBuf;

/// Point HOME/XDG at an empty temp dir so no real logs are discovered, and
/// route tokscale's config dir there too. Also isolates XDG_DATA_HOME and
/// CODEX_HOME so a real dev/CI shell can't leak session data into the test.
/// Returns the guard dir (kept alive for the test's duration).
fn isolated_home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tb-reports-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("HOME", &dir);
    std::env::set_var("XDG_CONFIG_HOME", dir.join(".config"));
    std::env::set_var("TOKSCALE_CONFIG_DIR", dir.join("tokscale"));
    std::env::set_var("XDG_DATA_HOME", dir.join(".local/share"));
    std::env::set_var("CODEX_HOME", dir.join(".codex"));
    dir
}

#[test]
fn usage_graph_run_on_empty_home_is_ok() {
    let _dir = isolated_home();
    // All-time graph over an empty machine: no data, but a well-formed payload.
    let result = tb_reports::usage_graph::run("");
    assert!(result.is_ok(), "usage_graph::run errored on empty HOME: {result:?}");
}

#[test]
fn detect_subscriptions_on_empty_home_is_empty() {
    let _dir = isolated_home();
    // No opencode auth.json present → no subscription providers detected.
    let subs = tb_reports::opencode_integrations::detect_subscriptions();
    assert!(subs.is_empty(), "expected no subscriptions on empty HOME, got: {subs:?}");
}
