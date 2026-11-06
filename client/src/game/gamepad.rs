// Game controller input (web/gamepad.ts), in an Xbox controller's layout:
// the sticks fly (the left, along / across the camera's view) and turn the
// camera (the right); the buttons as bound (binds.rs: by default RT / LT
// climb / descend, by how far they are pulled, L3 boosts, X A B Y choose a
// spell, RB casts it, LB raises the shield, the D-pad's up / down zoom,
// View cycles the mode, Start pauses). Fixed: A tries again once he is
// defeated; in the pause menu the D-pad or the left stick moves (held, it
// repeats), A chooses, B goes back, the shoulders or the triggers change
// tabs.
//
// Natively, through gilrs, as its mapping has the buttons. In the browser, straight from the Gamepad API: its "standard"
// layout where the browser gives one, else an Xbox controller's own order,
// as Firefox on Linux reports one (which gilrs's web backend read as the
// standard layout: Back as a trigger, LT as the right stick...).

use super::binds::{Action, Binds, PadButton, PAD_BUTTONS};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub mov: [f32; 2],  // x right, y forward
    pub look: [f32; 2], // x right, y up
    pub up: f32,        // -1..1
    pub boost: bool,          // the dash button held (picture mode's speed)
    pub dash: bool,           // the dash button pressed since the last read
    pub mode: bool,           // the mode's button pressed since the last read
    pub pause: bool,          // pause's button pressed since the last read
    pub back: bool,           // B pressed since the last read
    pub nav: [bool; 4],       // the D-pad, or the left stick, (up, down, left, right) pressed since the last read (or repeating)
    pub tab: i32,             // LB / LT (-1) or RB / RT (1) pressed since the last read
    pub spell: Option<usize>, // a spell chosen since the last read
    pub cast: bool,           // held
    pub shield: bool,         // held
    pub zoom: f32,            // -1 in .. 1 out (held)
    pub confirm: bool,        // A pressed since the last read
    pub pressed: Option<PadButton>, // a button pressed since the last read (for binding it)
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
    pub rs: bool,
    pub up: bool,
    pub down: bool,
    pub dleft: bool,
    pub dright: bool,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub lt: f32,
    pub rt: f32,
}

impl Raw {
    // How far button `b` is pressed, 0..1 (a trigger by how far it is
    // pulled).
    pub fn value(&self, b: PadButton) -> f32 {
        match b {
            PadButton::LT => self.lt,
            PadButton::RT => self.rt,
            b => self.down(b) as i32 as f32,
        }
    }

    pub fn down(&self, b: PadButton) -> bool {
        match b {
            PadButton::A => self.a,
            PadButton::B => self.b,
            PadButton::X => self.x,
            PadButton::Y => self.y,
            PadButton::LB => self.lb,
            PadButton::RB => self.rb,
            PadButton::LT => self.lt > 0.5,
            PadButton::RT => self.rt > 0.5,
            PadButton::View => self.back,
            PadButton::Start => self.start,
            PadButton::LS => self.ls,
            PadButton::RS => self.rs,
            PadButton::Up => self.up,
            PadButton::Down => self.down,
            PadButton::DLeft => self.dleft,
            PadButton::DRight => self.dright,
        }
    }
}

#[derive(Default)]
pub struct Pads {
    #[cfg(not(target_arch = "wasm32"))]
    gilrs: Option<gilrs::Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    started: bool,
    // X and Y read the other way round (as some controllers report them:
    // the default; the controls tab can turn it off)
    pub swap_xy: bool,
    // the last read's buttons (for "pressed since")
    last: Raw,
    // the left stick's direction (up, down, left, right) as a menu's
    // cursor reads it, and when it next repeats
    stick_dir: Option<usize>,
    stick_next: Option<web_time::Instant>,
    seen: Vec<String>,
    // (in the browser, the trigger axes rest at -1 once used, but read 0
    // until then: each is counted once it has moved)
    #[cfg(target_arch = "wasm32")]
    triggers_live: [bool; 2],
}

impl Pads {
    // With X and Y swapped, or not.
    pub fn new(swap_xy: bool) -> Pads {
        Pads { swap_xy, ..Default::default() }
    }

    // The first connected controller's state, if there is one, its
    // buttons as bound.
    pub fn read(&mut self, binds: &Binds) -> Option<Pad> {
        // (the tests play without one: a real controller, plugged in, would
        // steer them)
        if cfg!(test) {
            return None;
        }
        let raw = self.raw()?;
        Some(self.pad(raw, binds))
    }

    fn pad(&mut self, r: Raw, binds: &Binds) -> Pad {
        let r = if self.swap_xy { Raw { x: r.y, y: r.x, ..r } } else { r };
        let last = self.last;
        self.last = r;
        let [mx, my] = stick(r.left[0], r.left[1]);
        let [lx, ly] = stick(r.right[0], r.right[1]);
        let value = |a: Action| binds.pad(a).map_or(0.0, |b| r.value(b));
        let held = |a: Action| binds.pad(a).is_some_and(|b| r.down(b));
        let hit = |b: PadButton| r.down(b) && !last.down(b);
        let pressed = |a: Action| binds.pad(a).is_some_and(hit);
        // (a spell's button, as it goes down)
        let spell = (0..4).find(|&i| pressed(Action::Spell(i)));
        let mut nav = [hit(PadButton::Up), hit(PadButton::Down), hit(PadButton::DLeft), hit(PadButton::DRight)];
        if let Some(d) = self.stick_nav(r.left) {
            nav[d] = true;
        }
        Pad {
            mov: [mx, my],
            look: [lx, ly],
            up: (value(Action::Climb) - value(Action::Descend)).clamp(-1.0, 1.0),
            boost: held(Action::Dash),
            dash: pressed(Action::Dash),
            mode: pressed(Action::Mode),
            pause: pressed(Action::Pause),
            back: hit(PadButton::B),
            nav,
            tab: (hit(PadButton::RB) || hit(PadButton::RT)) as i32 - (hit(PadButton::LB) || hit(PadButton::LT)) as i32,
            spell,
            cast: held(Action::Cast),
            shield: held(Action::Shield),
            zoom: (held(Action::ZoomOut) as i32 - held(Action::ZoomIn) as i32) as f32,
            confirm: hit(PadButton::A),
            pressed: PAD_BUTTONS.into_iter().find(|&b| hit(b)),
            active: r.a || r.b || r.x || r.y || r.lb || r.rb || r.back || r.start || r.ls || r.rs || r.up || r.down || r.dleft || r.dright
                || mx != 0.0
                || my != 0.0
                || lx != 0.0
                || ly != 0.0
                || r.rt > 0.1
                || r.lt > 0.1,
        }
    }

    // The left stick as a menu's cursor reads it: a direction once it is
    // pushed well over (the way it is pushed most), again after 0.4 s held
    // and then every 0.13 s; let back nearly to the middle, none.
    fn stick_nav(&mut self, [x, y]: [f32; 2]) -> Option<usize> {
        let m = x.abs().max(y.abs());
        let dir = if m < if self.stick_dir.is_some() { 0.35 } else { 0.6 } {
            None
        } else if y.abs() >= x.abs() {
            Some(if y > 0.0 { 0 } else { 1 })
        } else {
            Some(if x < 0.0 { 2 } else { 3 })
        };
        let now = web_time::Instant::now();
        let ms = |t: u64| now + std::time::Duration::from_millis(t);
        if dir != self.stick_dir {
            self.stick_dir = dir;
            self.stick_next = dir.map(|_| ms(400));
            return dir;
        }
        match (dir, self.stick_next) {
            (Some(d), Some(t)) if now >= t => {
                self.stick_next = Some(ms(130));
                Some(d)
            }
            _ => None,
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
        use gilrs::{Axis, Button};
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
        let source = g.mapping_source();
        let name = g.name().to_string();
        let pressed = |b: Button| g.is_pressed(b);
        let value = |b: Button| g.button_data(b).map_or(0.0, |d| d.value());
        let raw = Raw {
            a: pressed(Button::South),
            b: pressed(Button::East),
            x: pressed(Button::West),
            y: pressed(Button::North),
            lb: pressed(Button::LeftTrigger),
            rb: pressed(Button::RightTrigger),
            back: pressed(Button::Select),
            start: pressed(Button::Start),
            ls: pressed(Button::LeftThumb),
            rs: pressed(Button::RightThumb),
            // (the D-pad as buttons, or as a hat's axis)
            up: pressed(Button::DPadUp) || g.value(Axis::DPadY) > 0.5,
            down: pressed(Button::DPadDown) || g.value(Axis::DPadY) < -0.5,
            dleft: pressed(Button::DPadLeft) || g.value(Axis::DPadX) < -0.5,
            dright: pressed(Button::DPadRight) || g.value(Axis::DPadX) > 0.5,
            // (gilrs has the sticks' y up already)
            left: [g.value(Axis::LeftStickX), g.value(Axis::LeftStickY)],
            right: [g.value(Axis::RightStickX), g.value(Axis::RightStickY)],
            // (a trigger reported as an axis instead: count only its upper half)
            lt: value(Button::LeftTrigger2).max(g.value(Axis::LeftZ).max(0.0)),
            rt: value(Button::RightTrigger2).max(g.value(Axis::RightZ).max(0.0)),
        };
        self.note(&name, &format!("{source:?} mapping"));
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
                rs: button(11),
                up: button(12),
                down: button(13),
                dleft: button(14),
                dright: button(15),
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
                rs: button(10),
                up: axis(7) < -0.5,
                down: axis(7) > 0.5,
                dleft: axis(6) < -0.5,
                dright: axis(6) > 0.5,
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
        let b = Binds::default();
        let pad = p.pad(Raw { y: true, start: true, back: true, up: true, rb: true, ..Default::default() }, &b);
        assert_eq!(pad.spell, Some(3));
        assert!(pad.pause && pad.mode);
        assert_eq!(pad.zoom, -1.0);
        assert_eq!(pad.nav, [true, false, false, false]);
        assert_eq!(pad.tab, 1);
        // (held: not again)
        let pad = p.pad(Raw { y: true, start: true, back: true, up: true, rb: true, ..Default::default() }, &b);
        assert_eq!(pad.spell, None);
        assert!(!pad.pause && !pad.mode);
        assert_eq!(pad.nav, [false; 4]);
        assert_eq!(pad.tab, 0);
        // the left stick moves a menu's cursor, once as it is pushed over
        let pad = p.pad(Raw { left: [0.0, -0.9], ..Default::default() }, &b);
        assert_eq!(pad.nav, [false, true, false, false]);
        let pad = p.pad(Raw { left: [0.0, -0.5], ..Default::default() }, &b);
        assert_eq!(pad.nav, [false; 4]);
        let pad = p.pad(Raw { x: true, rb: true, ..Default::default() }, &b);
        assert_eq!(pad.spell, Some(0));
        assert!(pad.cast);
    }
}
