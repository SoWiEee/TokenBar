//! The orbitable 3D contribution-graph widget: a GLArea driving the glow
//! `Renderer`, with drag-to-orbit / scroll-to-zoom. Embeds as one page of the
//! app's view stack; `apply_bars` swaps in freshly loaded data.
//!
//! GL state is only ever touched inside realize/render (where the GLArea's
//! context is current). Data arriving off the main render path is staged in
//! `pending` and uploaded at the top of the next render.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, glib, EventControllerScroll, EventControllerScrollFlags, GLArea, GestureDrag};

use crate::camera::Orbit;
use crate::graph::{self, Bar};
use crate::renderer::Renderer;

const ORBIT_SENSITIVITY: f32 = 0.008;
const ZOOM_STEP: f32 = 0.1;

pub struct GraphView {
    area: GLArea,
    cam: Rc<RefCell<Orbit>>,
    /// Bars waiting to be uploaded inside the next render (where GL is current).
    pending: Rc<RefCell<Option<Vec<Bar>>>>,
}

impl GraphView {
    pub fn new() -> Self {
        let placeholder = graph::grid_from(24, 7, |_, _| 0.0);
        let cam = Rc::new(RefCell::new(Orbit::fit(graph::bounding_radius(&placeholder))));
        let pending: Rc<RefCell<Option<Vec<Bar>>>> = Rc::new(RefCell::new(None));

        let area = GLArea::new();
        area.set_allowed_apis(gdk::GLAPI::GL);
        area.set_has_depth_buffer(true);
        area.set_hexpand(true);
        area.set_vexpand(true);

        let state: Rc<RefCell<Option<Renderer>>> = Rc::new(RefCell::new(None));

        area.connect_realize({
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
                let placeholder = graph::grid_from(24, 7, |_, _| 0.0);
                match Renderer::new(gl, &placeholder) {
                    Ok(r) => *state.borrow_mut() = Some(r),
                    Err(e) => eprintln!("renderer init failed: {e}"),
                }
            }
        });

        area.connect_render({
            let state = state.clone();
            let cam = cam.clone();
            let pending = pending.clone();
            move |area, _ctx| {
                if let Some(renderer) = state.borrow_mut().as_mut() {
                    // Upload any freshly loaded bars now that GL is current.
                    if let Some(bars) = pending.borrow_mut().take() {
                        renderer.set_bars(&bars);
                    }
                    let aspect = area.width().max(1) as f32 / area.height().max(1) as f32;
                    let vp = cam.borrow().view_proj(aspect);
                    renderer.draw(&vp);
                }
                glib::Propagation::Stop
            }
        });

        area.connect_unrealize({
            let state = state.clone();
            move |area| {
                area.make_current();
                state.borrow_mut().take();
            }
        });

        wire_orbit_gestures(&area, &cam);

        Self { area, cam, pending }
    }

    /// The embeddable widget.
    pub fn widget(&self) -> &GLArea {
        &self.area
    }

    /// Stage a new set of bars for upload on the next render and re-frame the
    /// camera. Safe to call from any main-thread context (no GL here).
    pub fn apply_bars(&self, bars: &[Bar]) {
        *self.cam.borrow_mut() = Orbit::fit(graph::bounding_radius(bars));
        *self.pending.borrow_mut() = Some(bars.to_vec());
        self.area.queue_render();
    }
}

/// Drag to orbit (absolute from drag-start to avoid drift), scroll to zoom.
fn wire_orbit_gestures(area: &GLArea, cam: &Rc<RefCell<Orbit>>) {
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
        let area = area.clone();
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
    area.add_controller(drag);

    let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll({
        let cam = cam.clone();
        let area = area.clone();
        move |_, _, dy| {
            cam.borrow_mut().zoom(1.0 + dy as f32 * ZOOM_STEP);
            area.queue_render();
            glib::Propagation::Stop
        }
    });
    area.add_controller(scroll);
}
