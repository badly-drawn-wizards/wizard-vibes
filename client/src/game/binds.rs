// The controls, remappable (the pause menu's Controls tab): an action's key
// and its controller button. Taking a binding that another action has
// swaps the two, so no action is left without one it had. Kept between
// runs, as names (egui's keys' names, and the buttons' below).

use super::hud::{Chip, Icon};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Climb,
    Descend,
    Dash,
    Cast,
    Shield,
    // the spells, in spells' order (see spell_name)
    Spell(usize),
    ZoomIn,
    ZoomOut,
    Mode,
    Pause,
}

pub const ACTIONS: [Action; 17] = [
    Action::Forward,
    Action::Back,
    Action::Left,
    Action::Right,
    Action::Climb,
    Action::Descend,
    Action::Dash,
    Action::Cast,
    Action::Shield,
    Action::Spell(0),
    Action::Spell(1),
    Action::Spell(2),
    Action::Spell(3),
    Action::ZoomIn,
    Action::ZoomOut,
    Action::Mode,
    Action::Pause,
];

impl Action {
    pub fn label(self) -> String {
        let t = crate::i18n::text();
        match self {
            Action::Forward => t.action_forward(),
            Action::Back => t.action_back(),
            Action::Left => t.action_left(),
            Action::Right => t.action_right(),
            Action::Climb => t.action_climb(),
            Action::Descend => t.action_descend(),
            Action::Dash => t.action_dash(),
            Action::Cast => t.action_cast(),
            Action::Shield => t.action_shield(),
            Action::Spell(i) => return t.action_spell_slot(i + 1),
            Action::ZoomIn => t.action_zoom_in(),
            Action::ZoomOut => t.action_zoom_out(),
            Action::Mode => t.settings_mode(),
            Action::Pause => t.action_pause(),
        }
        .to_string()
    }

    fn key(self) -> String {
        match self {
            Action::Spell(i) => format!("spell{i}"),
            a => format!("{a:?}").to_lowercase(),
        }
    }

    fn index(self) -> usize {
        ACTIONS.iter().position(|&a| a == self).unwrap()
    }
}

// A key, or a modifier held alone (egui has no keys for those), or a mouse
// button (counted only while the mouse is taken: see look.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBind {
    Key(egui::Key),
    Shift,
    Ctrl,
    Alt,
    Mouse(egui::PointerButton),
}

impl KeyBind {
    pub fn name(self) -> &'static str {
        match self {
            KeyBind::Key(k) => k.name(),
            KeyBind::Shift => "Shift",
            KeyBind::Ctrl => "Ctrl",
            KeyBind::Alt => "Alt",
            KeyBind::Mouse(egui::PointerButton::Primary) => "Left click",
            KeyBind::Mouse(egui::PointerButton::Secondary) => "Right click",
            KeyBind::Mouse(egui::PointerButton::Middle) => "Middle click",
            KeyBind::Mouse(egui::PointerButton::Extra1) => "Mouse 4",
            KeyBind::Mouse(egui::PointerButton::Extra2) => "Mouse 5",
        }
    }

    // Its name as shown (a mouse button's in the language chosen).
    pub fn shown(self) -> &'static str {
        let t = crate::i18n::text();
        match self {
            KeyBind::Mouse(egui::PointerButton::Primary) => t.mouse_left(),
            KeyBind::Mouse(egui::PointerButton::Secondary) => t.mouse_right(),
            KeyBind::Mouse(egui::PointerButton::Middle) => t.mouse_middle(),
            KeyBind::Mouse(egui::PointerButton::Extra1) => t.mouse_mouse4(),
            KeyBind::Mouse(egui::PointerButton::Extra2) => t.mouse_mouse5(),
            _ => self.name(),
        }
    }

    pub fn is_mouse(self) -> bool {
        matches!(self, KeyBind::Mouse(_))
    }

    fn from_name(s: &str) -> Option<KeyBind> {
        match s {
            "Shift" => Some(KeyBind::Shift),
            "Ctrl" => Some(KeyBind::Ctrl),
            "Alt" => Some(KeyBind::Alt),
            "Left click" => Some(KeyBind::Mouse(egui::PointerButton::Primary)),
            "Right click" => Some(KeyBind::Mouse(egui::PointerButton::Secondary)),
            "Middle click" => Some(KeyBind::Mouse(egui::PointerButton::Middle)),
            "Mouse 4" => Some(KeyBind::Mouse(egui::PointerButton::Extra1)),
            "Mouse 5" => Some(KeyBind::Mouse(egui::PointerButton::Extra2)),
            _ => egui::Key::from_name(s).map(KeyBind::Key),
        }
    }

    // Held, in this frame's input.
    pub fn down(self, i: &egui::InputState) -> bool {
        match self {
            KeyBind::Key(k) => i.key_down(k),
            KeyBind::Shift => i.modifiers.shift,
            KeyBind::Ctrl => i.modifiers.ctrl,
            KeyBind::Alt => i.modifiers.alt,
            KeyBind::Mouse(b) => i.pointer.button_down(b),
        }
    }

    // The modifier among `now` that was not down in `before`, if any (a
    // modifier pressed alone).
    pub fn modifier_pressed(before: egui::Modifiers, now: egui::Modifiers) -> Option<KeyBind> {
        if now.shift && !before.shift {
            Some(KeyBind::Shift)
        } else if now.ctrl && !before.ctrl {
            Some(KeyBind::Ctrl)
        } else if now.alt && !before.alt {
            Some(KeyBind::Alt)
        } else {
            None
        }
    }
}

// A controller's button, in an Xbox controller's names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    View,
    Start,
    LS,
    RS,
    Up,
    Down,
    DLeft,
    DRight,
}

pub const PAD_BUTTONS: [PadButton; 16] = [
    PadButton::A,
    PadButton::B,
    PadButton::X,
    PadButton::Y,
    PadButton::LB,
    PadButton::RB,
    PadButton::LT,
    PadButton::RT,
    PadButton::View,
    PadButton::Start,
    PadButton::LS,
    PadButton::RS,
    PadButton::Up,
    PadButton::Down,
    PadButton::DLeft,
    PadButton::DRight,
];

impl PadButton {
    fn name(self) -> String {
        format!("{self:?}")
    }

    fn from_name(s: &str) -> Option<PadButton> {
        PAD_BUTTONS.into_iter().find(|b| b.name() == s)
    }

    // Its glyph.
    pub fn chip(self) -> Chip {
        match self {
            PadButton::A => Chip::Face('A'),
            PadButton::B => Chip::Face('B'),
            PadButton::X => Chip::Face('X'),
            PadButton::Y => Chip::Face('Y'),
            PadButton::LB => Chip::Shoulder("LB"),
            PadButton::RB => Chip::Shoulder("RB"),
            PadButton::LT => Chip::Shoulder("LT"),
            PadButton::RT => Chip::Shoulder("RT"),
            PadButton::View => Chip::ShoulderIcon(Icon::View),
            PadButton::Start => Chip::Shoulder("≡"),
            PadButton::LS => Chip::Stick('L'),
            PadButton::RS => Chip::Stick('R'),
            PadButton::Up => Chip::DPad(0),
            PadButton::Down => Chip::DPad(1),
            PadButton::DLeft => Chip::DPad(2),
            PadButton::DRight => Chip::DPad(3),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binds {
    keys: [Option<KeyBind>; ACTIONS.len()],
    pad: [Option<PadButton>; ACTIONS.len()],
}

impl Default for Binds {
    fn default() -> Self {
        use egui::Key as K;
        let mut b = Binds { keys: [None; ACTIONS.len()], pad: [None; ACTIONS.len()] };
        let k = |key| Some(KeyBind::Key(key));
        for (a, key, pad) in [
            (Action::Forward, k(K::W), None),
            (Action::Back, k(K::S), None),
            (Action::Left, k(K::A), None),
            (Action::Right, k(K::D), None),
            (Action::Climb, k(K::E), Some(PadButton::RT)),
            (Action::Descend, k(K::Q), Some(PadButton::LT)),
            (Action::Dash, Some(KeyBind::Shift), Some(PadButton::LS)),
            (Action::Cast, Some(KeyBind::Mouse(egui::PointerButton::Primary)), Some(PadButton::RB)),
            (Action::Shield, Some(KeyBind::Mouse(egui::PointerButton::Secondary)), Some(PadButton::LB)),
            (Action::Spell(0), k(K::Num1), Some(PadButton::X)),
            (Action::Spell(1), k(K::Num2), Some(PadButton::A)),
            (Action::Spell(2), k(K::Num3), Some(PadButton::B)),
            (Action::Spell(3), k(K::Num4), Some(PadButton::Y)),
            (Action::ZoomIn, None, Some(PadButton::Up)),
            (Action::ZoomOut, None, Some(PadButton::Down)),
            (Action::Mode, k(K::F), Some(PadButton::View)),
            (Action::Pause, k(K::P), Some(PadButton::Start)),
        ] {
            b.keys[a.index()] = key;
            b.pad[a.index()] = pad;
        }
        b
    }
}

impl Binds {
    pub fn key(&self, a: Action) -> Option<KeyBind> {
        self.keys[a.index()]
    }

    pub fn pad(&self, a: Action) -> Option<PadButton> {
        self.pad[a.index()]
    }

    // The action a key is bound to, if any.
    pub fn key_action(&self, k: KeyBind) -> Option<Action> {
        ACTIONS.into_iter().find(|&a| self.key(a) == Some(k))
    }

    // Binds `a` to key `k`: an action that had it takes `a`'s old key.
    pub fn set_key(&mut self, a: Action, k: KeyBind) {
        let old = self.keys[a.index()];
        if let Some(other) = self.key_action(k) {
            self.keys[other.index()] = old;
        }
        self.keys[a.index()] = Some(k);
    }

    // Binds `a` to button `b`: an action that had it takes `a`'s old one.
    pub fn set_pad(&mut self, a: Action, b: PadButton) {
        let old = self.pad[a.index()];
        if let Some(other) = ACTIONS.into_iter().find(|&o| self.pad(o) == Some(b)) {
            self.pad[other.index()] = old;
        }
        self.pad[a.index()] = Some(b);
    }

    // As kept: action -> (key, button), names ("" for none).
    pub fn saved(&self) -> BTreeMap<String, (String, String)> {
        ACTIONS
            .into_iter()
            .map(|a| {
                let k = self.key(a).map_or(String::new(), |k| k.name().to_string());
                let p = self.pad(a).map_or(String::new(), |b| b.name());
                (a.key(), (k, p))
            })
            .collect()
    }

    // From what was kept (the defaults for any action missing from it).
    pub fn restored(s: &BTreeMap<String, (String, String)>) -> Binds {
        let mut b = Binds::default();
        for a in ACTIONS {
            if let Some((k, p)) = s.get(&a.key()) {
                b.keys[a.index()] = KeyBind::from_name(k);
                b.pad[a.index()] = PadButton::from_name(p);
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taking_a_binding_swaps() {
        let mut b = Binds::default();
        b.set_key(Action::Cast, KeyBind::Key(egui::Key::W));
        assert_eq!(b.key(Action::Cast), Some(KeyBind::Key(egui::Key::W)));
        assert_eq!(b.key(Action::Forward), Some(KeyBind::Mouse(egui::PointerButton::Primary)));
        b.set_pad(Action::Spell(0), PadButton::A);
        assert_eq!(b.pad(Action::Spell(0)), Some(PadButton::A));
        assert_eq!(b.pad(Action::Spell(1)), Some(PadButton::X));
    }

    #[test]
    fn kept_and_restored() {
        let mut b = Binds::default();
        b.set_key(Action::Dash, KeyBind::Ctrl);
        b.set_key(Action::Mode, KeyBind::Mouse(egui::PointerButton::Middle));
        b.set_pad(Action::ZoomIn, PadButton::RS);
        assert_eq!(Binds::restored(&b.saved()), b);
    }
}
