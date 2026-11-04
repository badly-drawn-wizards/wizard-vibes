// Game controller input (web/gamepad.ts), through gilrs (natively, and the
// Gamepad API in the browser, which only exposes a controller once one of
// its buttons is pressed on the page). gilrs maps controllers to its own
// standard layout, so the TS's raw-Xbox fallback layout is not needed.
//
//   left stick     fly along / across the camera's view
//   right stick    turn the camera
//   RT / LT        climb / descend (by how far they are pulled)
//   L3             boost
//   X / A / B      choose a spell: fire bolt, lightning strike, wind blade
//   RB             cast it (held: again as soon as it can)
//   LB             raise the shield (held)
//   A              (defeated) try again
//   View / Back    start the fight again
//   Y / Start      cycle the mode

use gilrs::{Axis, Button, Gilrs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pad {
    pub mov: [f32; 2],  // x right, y forward
    pub look: [f32; 2], // x right, y up
    pub up: f32,        // -1..1
    pub boost: bool,
    pub toggle: bool,         // pressed since the last read
    pub spell: Option<usize>, // a spell chosen since the last read
    pub cast: bool,           // held
    pub shield: bool,         // held
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

// The buttons a read looks at (for "anything pressed").
const BUTTONS: [Button; 15] = [
    Button::South,
    Button::East,
    Button::North,
    Button::West,
    Button::LeftTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
];

pub struct Pads {
    gilrs: Option<Gilrs>,
    toggle_held: bool,
    confirm_held: bool,
    reset_held: bool,
    spell_held: Option<usize>,
    seen: Vec<String>,
}

impl Default for Pads {
    fn default() -> Self {
        let gilrs = match Gilrs::new() {
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
        Pads { gilrs, toggle_held: false, confirm_held: false, reset_held: false, spell_held: None, seen: Vec::new() }
    }
}

impl Pads {
    // The first connected controller's state, if there is one.
    pub fn read(&mut self) -> Option<Pad> {
        let gilrs = self.gilrs.as_mut()?;
        // (gilrs keeps each controller's state up to date as its events are
        // taken)
        while gilrs.next_event().is_some() {}
        // (natively gilrs also offers keyboards' media and system-control
        // keys as gamepads: a controller has sticks)
        let (_, g) = gilrs.gamepads().find(|(_, g)| g.is_connected() && g.axis_code(gilrs::Axis::LeftStickX).is_some())?;
        let name = g.name().to_string();
        if !self.seen.contains(&name) {
            log::info!("controller: {name} ({:?} mapping)", g.mapping_source());
            self.seen.push(name);
        }
        let pressed = |b: Button| g.is_pressed(b);
        let value = |b: Button| g.button_data(b).map_or(0.0, |d| d.value());
        let [mx, my] = stick(g.value(Axis::LeftStickX), g.value(Axis::LeftStickY));
        let [lx, ly] = stick(g.value(Axis::RightStickX), g.value(Axis::RightStickY));
        // (a trigger reported as an axis instead: count only its upper half)
        let rt = value(Button::RightTrigger2).max(g.value(Axis::RightZ).max(0.0));
        let lt = value(Button::LeftTrigger2).max(g.value(Axis::LeftZ).max(0.0));
        let t = pressed(Button::North) || pressed(Button::Start);
        let toggle = t && !self.toggle_held;
        self.toggle_held = t;
        let held = [Button::West, Button::South, Button::East].iter().position(|&b| pressed(b));
        let spell = if held.is_some() && held != self.spell_held { held } else { None };
        self.spell_held = held;
        let a = pressed(Button::South);
        let confirm = a && !self.confirm_held;
        self.confirm_held = a;
        let back = pressed(Button::Select);
        let reset = back && !self.reset_held;
        self.reset_held = back;
        Some(Pad {
            // (gilrs has the sticks' y up already)
            mov: [mx, my],
            look: [lx, ly],
            up: (rt - lt).clamp(-1.0, 1.0),
            boost: pressed(Button::LeftThumb),
            toggle,
            spell,
            cast: pressed(Button::RightTrigger),
            shield: pressed(Button::LeftTrigger),
            confirm,
            reset,
            active: BUTTONS.iter().any(|&b| pressed(b))
                || mx != 0.0
                || my != 0.0
                || lx != 0.0
                || ly != 0.0
                || rt > 0.1
                || lt > 0.1,
        })
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
}
