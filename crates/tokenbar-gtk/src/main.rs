//! TokenBar — Linux (GTK4) frontend.
//!
//! A libadwaita app whose views (an orbitable 3D contribution graph, plus the
//! usage lenses) are backed by the shared Rust core (`tb_reports`) — the same
//! data the macOS app renders. Phase 2 adds the window chrome and view switcher
//! around the Phase 1 graph.

mod agents;
mod camera;
mod daily;
mod data;
mod format;
mod graph;
mod graph_view;
mod hourly;
mod list_lens;
mod models;
mod overview;
mod renderer;
mod stats;

use std::ptr;
use std::rc::Rc;

use adw::prelude::*;
use adw::{
    Application, ApplicationWindow, HeaderBar, ToolbarView, ViewStack, ViewSwitcher,
    ViewSwitcherPolicy,
};
use gtk4::glib;

use graph_view::GraphView;
use overview::Overview;

const APP_ID: &str = "com.nyanako.tokenbar.gtk";

fn main() -> glib::ExitCode {
    // Headless data probe: load once, report counts, exit. No GL, no display.
    if std::env::args().any(|a| a == "--probe") {
        let t = std::time::Instant::now();
        let d = data::load();
        eprintln!(
            "load: {} bars, {} tokens, {} active days, in {:?}",
            d.bars.len(),
            d.summary.total_tokens,
            d.summary.active_days,
            t.elapsed()
        );
        return glib::ExitCode::SUCCESS;
    }

    // Load GL function pointers through epoxy so glow can resolve them at
    // realize time via `epoxy::get_proc_addr`.
    {
        #[cfg(all(unix, not(target_os = "macos")))]
        let library = unsafe { libloading::os::unix::Library::new("libepoxy.so.0") }
            .expect("load libepoxy.so.0");
        epoxy::load_with(|name| {
            unsafe { library.get::<_>(name.as_bytes()) }
                .map(|symbol| *symbol)
                .unwrap_or(ptr::null())
        });
    }

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let graph_view = Rc::new(GraphView::new());
    let overview = Rc::new(Overview::new());
    let daily = daily::new();
    let hourly = hourly::HourlyView::new();
    let stats = stats::StatsView::new();
    let models = models::new();
    let agents = agents::new();

    // View switcher over the lenses. The 3D graph is the first page; more
    // lenses slot in here. Report-backed lenses load lazily on first view.
    let stack = ViewStack::new();
    stack.add_titled_with_icon(graph_view.widget(), Some("graph"), "Graph", "view-grid-symbolic");
    stack.add_titled_with_icon(overview.widget(), Some("overview"), "Overview", "view-list-symbolic");
    stack.add_titled_with_icon(daily.widget(), Some("daily"), "Daily", "x-office-calendar-symbolic");
    stack.add_titled_with_icon(hourly.widget(), Some("hourly"), "Hourly", "preferences-system-time-symbolic");
    stack.add_titled_with_icon(stats.widget(), Some("stats"), "Stats", "utilities-system-monitor-symbolic");
    stack.add_titled_with_icon(models.widget(), Some("models"), "Models", "view-columns-symbolic");
    stack.add_titled_with_icon(agents.widget(), Some("agents"), "Agents", "system-users-symbolic");

    // Lazily load report-backed lenses the first time they're shown.
    stack.connect_visible_child_name_notify({
        let daily = daily.clone();
        let hourly = hourly.clone();
        let stats = stats.clone();
        let models = models.clone();
        let agents = agents.clone();
        move |stack| match stack.visible_child_name().as_deref() {
            Some("daily") => daily.ensure_loaded(),
            Some("hourly") => hourly.ensure_loaded(),
            Some("stats") => stats.ensure_loaded(),
            Some("models") => models.ensure_loaded(),
            Some("agents") => agents.ensure_loaded(),
            _ => {}
        }
    });

    // Icon-only (Narrow) so all seven lenses fit a compact, popover-width header.
    let switcher = ViewSwitcher::builder()
        .stack(&stack)
        .policy(ViewSwitcherPolicy::Narrow)
        .build();

    let header = HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(860)
        .default_height(640)
        .title("TokenBar")
        .content(&toolbar)
        .build();

    spawn_data_load(graph_view.clone(), overview.clone());

    // Dev aid: TOKENBAR_START_PAGE=<id> opens on a given lens (for screenshots).
    if let Some(page) = std::env::var_os("TOKENBAR_START_PAGE").and_then(|s| s.into_string().ok()) {
        stack.set_visible_child_name(&page);
    }

    window.present();
}

/// Load usage on a worker thread and, when ready, push it to every view on the
/// UI thread (the window stays responsive during the slow first load).
fn spawn_data_load(graph_view: Rc<GraphView>, overview: Rc<Overview>) {
    let (tx, rx) = async_channel::bounded::<data::GraphData>(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(data::load());
    });

    glib::spawn_future_local(async move {
        let Ok(graph_data) = rx.recv().await else {
            return;
        };
        graph_view.apply_bars(&graph_data.bars);
        overview.update(&graph_data.summary);
    });
}
