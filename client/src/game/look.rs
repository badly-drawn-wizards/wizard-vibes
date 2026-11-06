// Mouse look: a click on the view takes the mouse (hidden, held in place:
// the browser's pointer lock, or the window's cursor grab natively), and
// moving it then turns the camera; pausing (or the defeat panel) lets it
// go. In the browser, Escape lets it go first: that pauses too (see
// Game::step).

#[derive(Default)]
pub struct MouseLook {
    // taken (as asked: the browser may refuse, or let go itself)
    wanted: bool,
    // the browser has it locked, or had since it was asked
    engaged: bool,
    asked: Option<web_time::Instant>,
    #[cfg(target_arch = "wasm32")]
    web: Option<std::rc::Rc<std::cell::Cell<(f32, f32)>>>,
}

impl MouseLook {
    // Takes the mouse.
    pub fn grab(&mut self, ctx: &egui::Context) {
        self.wanted = true;
        self.engaged = false;
        self.asked = Some(web_time::Instant::now());
        #[cfg(not(target_arch = "wasm32"))]
        {
            use egui::{viewport::CursorGrab, ViewportCommand};
            // (locked where the platform can: Wayland, Windows, macOS; X11
            // has only confined, which the lock that fails leaves in place)
            ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::Confined));
            ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::Locked));
            ctx.send_viewport_cmd(ViewportCommand::CursorVisible(false));
            self.engaged = true;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            self.listen();
            if let Some(c) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("view")) {
                c.request_pointer_lock();
            }
        }
    }

    // Lets it go.
    pub fn release(&mut self, ctx: &egui::Context) {
        if !self.wanted {
            return;
        }
        self.wanted = false;
        self.engaged = false;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use egui::{viewport::CursorGrab, ViewportCommand};
            ctx.send_viewport_cmd(ViewportCommand::CursorGrab(CursorGrab::None));
            ctx.send_viewport_cmd(ViewportCommand::CursorVisible(true));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            if let Some(d) = web_sys::window().and_then(|w| w.document()) {
                if d.pointer_lock_element().is_some() {
                    d.exit_pointer_lock();
                }
            }
        }
    }

    // Whether it is taken (and held).
    pub fn held(&self) -> bool {
        self.wanted && self.locked_now()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn locked_now(&self) -> bool {
        true
    }

    #[cfg(target_arch = "wasm32")]
    fn locked_now(&self) -> bool {
        web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.pointer_lock_element().is_some())
    }

    // Whether the mouse, once taken, was let go of from outside (the
    // browser's Escape; or it never took it: then it is given up on),
    // once: the game then pauses.
    pub fn lost(&mut self) -> bool {
        if !self.wanted {
            return false;
        }
        let locked = self.locked_now();
        if locked {
            self.engaged = true;
            return false;
        }
        if self.engaged {
            self.wanted = false;
            self.engaged = false;
            return true;
        }
        if self.asked.is_some_and(|t| t.elapsed().as_secs_f32() > 1.5) {
            self.wanted = false;
        }
        false
    }

    // How far the mouse has moved since the last frame, while held.
    pub fn take(&mut self, ctx: &egui::Context) -> (f32, f32) {
        #[cfg(not(target_arch = "wasm32"))]
        let d = ctx.input(|i| {
            i.events.iter().fold((0.0, 0.0), |(x, y), e| match e {
                egui::Event::MouseMoved(v) => (x + v.x, y + v.y),
                _ => (x, y),
            })
        });
        #[cfg(target_arch = "wasm32")]
        let d = {
            let _ = ctx;
            self.web.as_ref().map_or((0.0, 0.0), |m| m.replace((0.0, 0.0)))
        };
        if self.held() { d } else { (0.0, 0.0) }
    }

    // (in the browser: the mouse's movement while locked, gathered from
    // its events between frames)
    #[cfg(target_arch = "wasm32")]
    fn listen(&mut self) {
        use wasm_bindgen::{closure::Closure, JsCast};
        if self.web.is_some() {
            return;
        }
        let moved = std::rc::Rc::new(std::cell::Cell::new((0.0f32, 0.0f32)));
        let m = moved.clone();
        let on_move = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
            let locked = web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.pointer_lock_element().is_some());
            if locked {
                let (x, y) = m.get();
                m.set((x + e.movement_x() as f32, y + e.movement_y() as f32));
            }
        });
        if let Some(d) = web_sys::window().and_then(|w| w.document()) {
            let _ = d.add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref());
        }
        // (for the page's life)
        on_move.forget();
        self.web = Some(moved);
    }
}
