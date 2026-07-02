//! TokenBar — Linux (GTK4) frontend.
//!
//! Phase 1: an orbitable 3D contribution graph in a libadwaita window, rendered
//! with glow in a GTK GLArea. Shares the Rust core with the macOS app via the
//! `tb_reports` crate (real data wired in next; demo data for now).

mod camera;
mod data;
mod graph;
mod renderer;

use std::cell::{Cell, RefCell};
use std::ptr;
use std::rc::Rc;

use adw::prelude::*;
use adw::{Application, ApplicationWindow};
use gtk4::{gdk, glib, EventControllerScroll, EventControllerScrollFlags, GLArea, GestureDrag};

use camera::Orbit;
use renderer::Renderer;

const APP_ID: &str = "com.nyanako.tokenbar.gtk";
/// Drag/scroll sensitivity.
const ORBIT_SENSITIVITY: f32 = 0.008;
const ZOOM_STEP: f32 = 0.1;

fn main() -> glib::ExitCode {
    // Headless data probe: load the bars, report count + timing, exit. No GL,
    // no display — used to diagnose the data path independently of the GUI.
    if std::env::args().any(|a| a == "--probe") {
        let t = std::time::Instant::now();
        let bars = data::load_bars();
        let tallest = bars.iter().map(|b| b.height).fold(0.0_f32, f32::max);
        eprintln!(
            "load_bars: {} bars, tallest {:.2}, in {:?}",
            bars.len(),
            tallest,
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
    // Start with a flat placeholder grid so the window is instant; real usage
    // loads on a worker thread (parsing + first-run pricing fetch is slow) and
    // swaps in when ready.
    let placeholder = graph::grid_from(24, 7, |_, _| 0.0);
    let cam = Rc::new(RefCell::new(Orbit::fit(graph::bounding_radius(&placeholder))));

    let gl_area = GLArea::new();
    // Force desktop GL (not GLES) for the `#version 330 core` shaders; request a
    // depth buffer for the 3D scene.
    gl_area.set_allowed_apis(gdk::GLAPI::GL);
    gl_area.set_has_depth_buffer(true);

    // GL resources live on the GLArea context: built in realize, dropped in
    // unrealize; shared with the render callback via Rc<RefCell<…>>.
    let state: Rc<RefCell<Option<Renderer>>> = Rc::new(RefCell::new(None));

    gl_area.connect_realize({
        let state = state.clone();
        move |area| {
            area.make_current();
            if let Some(err) = area.error() {
                eprintln!("GLArea realize error: {err}");
                return;
            }
            let gl = unsafe {
                glow::Context::from_loader_function(|s| epoxy::get_proc_addr(s) as *const _)
            };
            match Renderer::new(gl, &placeholder) {
                Ok(r) => *state.borrow_mut() = Some(r),
                Err(e) => eprintln!("renderer init failed: {e}"),
            }
        }
    });

    spawn_data_load(&gl_area, &state, &cam);

    gl_area.connect_render({
        let state = state.clone();
        let cam = cam.clone();
        move |area, _ctx| {
            if let Some(renderer) = state.borrow().as_ref() {
                let aspect = area.width().max(1) as f32 / area.height().max(1) as f32;
                let vp = cam.borrow().view_proj(aspect);
                renderer.draw(&vp);
            }
            glib::Propagation::Stop
        }
    });

    gl_area.connect_unrealize({
        let state = state.clone();
        move |area| {
            area.make_current();
            state.borrow_mut().take();
        }
    });

    wire_orbit_gestures(&gl_area, &cam);

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(820)
        .default_height(600)
        .title("TokenBar")
        .content(&gl_area)
        .build();
    window.present();
}

/// Load the real usage bars on a worker thread and swap them into the renderer
/// on the UI thread when ready, re-framing the camera to the loaded grid.
fn spawn_data_load(
    gl_area: &GLArea,
    state: &Rc<RefCell<Option<Renderer>>>,
    cam: &Rc<RefCell<Orbit>>,
) {
    let (tx, rx) = async_channel::bounded::<Vec<graph::Bar>>(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(data::load_bars());
    });

    let gl_area = gl_area.clone();
    let state = state.clone();
    let cam = cam.clone();
    glib::spawn_future_local(async move {
        let Ok(bars) = rx.recv().await else {
            return;
        };
        if let Some(renderer) = state.borrow_mut().as_mut() {
            gl_area.make_current();
            renderer.set_bars(&bars);
        }
        *cam.borrow_mut() = Orbit::fit(graph::bounding_radius(&bars));
        gl_area.queue_render();
    });
}

/// Drag to orbit (azimuth/elevation), scroll to zoom. Absolute-from-start drag
/// avoids drift: the orientation at drag-begin plus the gesture offset.
fn wire_orbit_gestures(gl_area: &GLArea, cam: &Rc<RefCell<Orbit>>) {
    let drag = GestureDrag::new();
    let start = Rc::new(Cell::new((0.0_f32, 0.0_f32)));
    drag.connect_drag_begin({
        let cam = cam.clone();
        let start = start.clone();
        move |_, _, _| {
            let c = cam.borrow();
            start.set((c.azimuth, c.elevation));
        }
    });
    drag.connect_drag_update({
        let cam = cam.clone();
        let area = gl_area.clone();
        let start = start.clone();
        move |_, offset_x, offset_y| {
            let (az0, ele0) = start.get();
            cam.borrow_mut().set_orbit(
                az0 - offset_x as f32 * ORBIT_SENSITIVITY,
                ele0 + offset_y as f32 * ORBIT_SENSITIVITY,
            );
            area.queue_render();
        }
    });
    gl_area.add_controller(drag);

    let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll({
        let cam = cam.clone();
        let area = gl_area.clone();
        move |_, _, dy| {
            cam.borrow_mut().zoom(1.0 + dy as f32 * ZOOM_STEP);
            area.queue_render();
            glib::Propagation::Stop
        }
    });
    gl_area.add_controller(scroll);
}
