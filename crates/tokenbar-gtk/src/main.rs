//! TokenBar — Linux (GTK4) frontend.
//!
//! Phase 1: prove the GLArea → epoxy → glow rendering chain on this machine by
//! drawing a triangle in a libadwaita window. The orbitable 3D contribution
//! graph (backed by the shared `tb_reports` core) replaces the triangle once
//! the chain is confirmed.

mod renderer;

use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

use adw::prelude::*;
use adw::{Application, ApplicationWindow};
use gtk4::{gdk, glib, GLArea};

use renderer::Renderer;

const APP_ID: &str = "com.nyanako.tokenbar.gtk";

fn main() -> glib::ExitCode {
    // Load GL function pointers through epoxy (GTK's GL dispatch library) so
    // glow can resolve them via `epoxy::get_proc_addr` at realize time.
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
    let gl_area = GLArea::new();
    // Force desktop GL (not GLES) so the `#version 330 core` shaders compile,
    // and request a depth buffer now for the 3D graph that follows.
    gl_area.set_allowed_apis(gdk::GLAPI::GL);
    gl_area.set_has_depth_buffer(true);

    // GL resources live on the GLArea's context: created in realize, dropped in
    // unrealize. Shared with the render callback via Rc<RefCell<…>>.
    let state: Rc<RefCell<Option<Renderer>>> = Rc::new(RefCell::new(None));

    gl_area.connect_realize({
        let state = state.clone();
        move |area| {
            area.make_current();
            if let Some(err) = area.error() {
                eprintln!("GLArea realize error: {err}");
                return;
            }
            let gl =
                unsafe { glow::Context::from_loader_function(|s| epoxy::get_proc_addr(s) as *const _) };
            match Renderer::new(gl) {
                Ok(r) => *state.borrow_mut() = Some(r),
                Err(e) => eprintln!("renderer init failed: {e}"),
            }
        }
    });

    gl_area.connect_render({
        let state = state.clone();
        move |_area, _ctx| {
            if let Some(renderer) = state.borrow().as_ref() {
                renderer.draw();
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

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(720)
        .default_height(520)
        .title("TokenBar")
        .content(&gl_area)
        .build();
    window.present();
}
