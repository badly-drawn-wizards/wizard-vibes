// Game controller input (web/gamepad.ts), in an Xbox controller's layout:
//
//   left stick     fly along / across the camera's view
//   right stick    turn the camera
//   RT / LT        climb / descend (by how far they are pulled)
//   L3             boost
//   X / A / B / Y  choose a spell: fire bolt, lightning strike, wind blade,
//                  healing
//   RB             cast it (held: again as soon as it can; the lightning
//                  locks on while it is held)
//   LB             raise the shield (held)
//   D-pad up/down  zoom in / out
//   A              (defeated) try again
//   View / Back    start the fight again
//   Start          cycle the mode
//
// Natively, through gilrs; with only the Linux driver's mapping (no SDL
// mapping for the controller), its codes put X on BTN_NORTH and Y on
// BTN_WEST, which gilrs reads as the top and left buttons: they are swapped
// back. In the browser, straight from the Gamepad API: its "standard"
// layout where the browser gives one, else an Xbox controller's own order,
// as Firefox on Linux reports one (which gilrs's web backend read as the
// standard layout: Back as a trigger, LT as the right stick...).

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub mov: [f32; 2],  // x right, y forward
    pub look: [f32; 2], // x right, y up
    pub up: f32,        // -1..1
    pub boost: bool,
    pub toggle: bool,         // Start pressed since the last read
    pub spell: Option<usize>, // a spell chosen since the last read
    pub cast: bool,           // held
    pub shield: bool,         // held
    pub zoom: f32,            // -1 in .. 1 out (held)
    pub confirm: bool,        // A pressed since the last read
    pub reset: bool,          // View / Back pressed since the last read
    pub active: bool,         // anything pressed or pushed (for showing its glyphs)
}

const DEAD: f32 = 0.15;

// A stick with a radial dead zone, rescaled so it still reaches 1.
pub fn stick(x: f32, y: f32) -> [f32; 2] {
    let m = x.hypot(y);
    if m < DEAD {
        return [0.0, 0.0];
    }
    let s = ((m - DEAD) / (1.0 - DEAD)).min(1.0) / m;
    [x * s, y * s]
}

// A controller's state in an Xbox controller's terms (sticks' y up,
// triggers 0..1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Raw {
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    pub lb: bool,
    pub rb: bool,
    pub back: bool,
    pub start: bool,
    pub ls: bool,
    pub up: bool,
    pub down: bool,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub lt: f32,
    pub rt: f32,
}

#[derive(Default)]
pub struct Pads {
    #[cfg(not(target_arch = "wasm32"))]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    started: bool,
    // the last read's buttons (for "pressed since")
    last: Raw,
    seen: Vec<String>,
    // (in the browser, the trigger axes rest at -1 once used, but read 0
    // until then: each is counted once it has moved)
    #[cfg(target_arch = "wasm32")]
    triggers_live: [bool; 2],
}

impl Pads {
    // The first connected controller's state, if there is one.
    pub fn read(&mut self) -> Option<Pad> {
        let raw = self.raw()?;
        Some(self.pad(raw))
    }

    fn pad(&mut self, r: Raw) -> Pad {
        let last = self.last;
        self.last = r;
        let [mx, my] = stick(r.left[0], r.left[1]);
        let [lx, ly] = stick(r.right[0], r.right[1]);
        // (a spell button, as it goes down)
        let spell = if r.x && !last.x {
            Some(0)
        } else if r.a && !last.a {
            Some(1)
        } else if r.b && !last.b {
            Some(2)
        } else if r.y && !last.y {
            Some(3)
        } else {
            None
        };
        Pad {
            mov: [mx, my],
            look: [lx, ly],
            up: (r.rt - r.lt).clamp(-1.0, 1.0),
            boost: r.ls,
            toggle: r.start && !last.start,
            spell,
            cast: r.rb,
            shield: r.lb,
            zoom: (r.down as i32 - r.up as i32) as f32,
            confirm: r.a && !last.a,
            reset: r.back && !last.back,
            active: r.a || r.b || r.x || r.y || r.lb || r.rb || r.back || r.start || r.ls || r.up || r.down
                || mx != 0.0
                || my != 0.0
                || lx != 0.0
                || ly != 0.0
                || r.rt > 0.1
                || r.lt > 0.1,
        }
    }

    fn note(&mut self, name: &str, how: &str) {
        if !self.seen.iter().any(|s| s == name) {
            log::info!("controller: {name} ({how})");
            self.seen.push(name.to_string());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn raw(&mut self) -> Option<Raw> {
        use gilrs::{Axis, Button, MappingSource};
        if !self.started {
            self.started = true;
            self.gilrs = match gilrs::Gilrs::new() {
                Ok(g) => Some(g),
                Err(gilrs::Error::NotImplemented(g)) => {
                    log::info!("controllers: not supported on this platform");
                    Some(g)
                }
                Err(e) => {
                    log::warn!("controllers unavailable: {e}");
                    None
                }
            };
        }
        let gilrs = self.gilrs.as_mut()?;
        // (gilrs keeps each controller's state up to date as its events are
        // taken)
        while gilrs.next_event().is_some() {}
        // (natively gilrs also offers keyboards' media and system-control
        // keys as gamepads: a controller has sticks)
        let (_, g) = gilrs.gamepads().find(|(_, g)| g.is_connected() && g.axis_code(Axis::LeftStickX).is_some())?;
        let driver = g.mapping_source() == MappingSource::Driver;
        let name = g.name().to_string();
        let pressed = |b: Button| g.is_pressed(b);
        let value = |b: Button| g.button_data(b).map_or(0.0, |d| d.value());
        // (the Linux driver's X and Y, read by gilrs as the top and left)
        let (x, y) = if driver { (pressed(Button::North), pressed(Button::West)) } else { (pressed(Button::West), pressed(Button::North)) };
        let raw = Raw {
            a: pressed(Button::South),
            b: pressed(Button::East),
            x,
            y,
            lb: pressed(Button::LeftTrigger),
            rb: pressed(Button::RightTrigger),
            back: pressed(Button::Select),
            start: pressed(Button::Start),
            ls: pressed(Button::LeftThumb),
            // (the D-pad as buttons, or as a hat's axis)
            up: pressed(Button::DPadUp) || g.value(Axis::DPadY) > 0.5,
            down: pressed(Button::DPadDown) || g.value(Axis::DPadY) < -0.5,
            // (gilrs has the sticks' y up already)
            left: [g.value(Axis::LeftStickX), g.value(Axis::LeftStickY)],
            right: [g.value(Axis::RightStickX), g.value(Axis::RightStickY)],
            // (a trigger reported as an axis instead: count only its upper half)
            lt: value(Button::LeftTrigger2).max(g.value(Axis::LeftZ).max(0.0)),
            rt: value(Button::RightTrigger2).max(g.value(Axis::RightZ).max(0.0)),
        };
        self.note(&name, if driver { "the driver's mapping, X and Y swapped back" } else { "an SDL mapping" });
        Some(raw)
    }

    #[cfg(target_arch = "wasm32")]
    fn raw(&mut self) -> Option<Raw> {
        use wasm_bindgen::JsCast;
        let pads = web_sys::window()?.navigator().get_gamepads().ok()?;
        let g = pads.iter().filter_map(|p| p.dyn_into::<web_sys::Gamepad>().ok()).find(|g| g.connected() && g.axes().length() >= 2)?;
        let buttons = g.buttons();
        let axes = g.axes();
        let button = |i: u32| buttons.get(i).dyn_into::<web_sys::GamepadButton>().map(|b| b.pressed() || b.value() > 0.5).unwrap_or(false);
        let axis = |i: u32| axes.get(i).as_f64().unwrap_or(0.0) as f32;
        let standard = g.mapping() == web_sys::GamepadMappingType::Standard;
        let raw = if standard {
            // buttons: A B X Y LB RB LT RT Back Start LS RS up down left
            // right; axes: the sticks (y down)
            let trigger = |i: u32| buttons.get(i).dyn_into::<web_sys::GamepadButton>().map(|b| b.value() as f32).unwrap_or(0.0);
            Raw {
                a: button(0),
                b: button(1),
                x: button(2),
                y: button(3),
                lb: button(4),
                rb: button(5),
                back: button(8),
                start: button(9),
                ls: button(10),
                up: button(12),
                down: button(13),
                left: [axis(0), -axis(1)],
                right: [axis(2), -axis(3)],
                lt: trigger(6),
                rt: trigger(7),
            }
        } else {
            // an Xbox controller's own order: buttons A B X Y LB RB Back
            // Start Guide LS RS; axes LX LY LT RX RY RT, the D-pad's x and y
            // (y down); the triggers' axes -1 at rest
            let (lt, rt) = (axis(2), axis(5));
            if lt != 0.0 {
                self.triggers_live[0] = true;
            }
            if rt != 0.0 {
                self.triggers_live[1] = true;
            }
            let pull = |v: f32, live: bool| if live { (0.5 * (v + 1.0)).clamp(0.0, 1.0) } else { 0.0 };
            Raw {
                a: button(0),
                b: button(1),
                x: button(2),
                y: button(3),
                lb: button(4),
                rb: button(5),
                back: button(6),
                start: button(7),
                ls: button(9),
                up: axis(7) < -0.5,
                down: axis(7) > 0.5,
                left: [axis(0), -axis(1)],
                right: [axis(3), -axis(4)],
                lt: pull(lt, self.triggers_live[0]),
                rt: pull(rt, self.triggers_live[1]),
            }
        };
        self.note(&g.id(), if standard { "the browser's standard mapping" } else { "no standard mapping: read as an Xbox controller" });
        Some(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_zone() {
        assert_eq!(stick(0.1, 0.1), [0.0, 0.0]);
        let [x, y] = stick(1.0, 0.0);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let [x, _] = stick(0.575, 0.0);
        assert!((x - 0.5).abs() < 1e-6);
    }

    #[test]
    fn presses_and_holds() {
        let mut p = Pads::default();
        let pad = p.pad(Raw { y: true, start: true, up: true, ..Default::default() });
        assert_eq!(pad.spell, Some(3));
        assert!(pad.toggle);
        assert_eq!(pad.zoom, -1.0);
        // (held: not again)
        let pad = p.pad(Raw { y: true, start: true, ..Default::default() });
        assert_eq!(pad.spell, None);
        assert!(!pad.toggle);
        let pad = p.pad(Raw { x: true, rb: true, ..Default::default() });
        assert_eq!(pad.spell, Some(0));
        assert!(pad.cast);
    }
}
