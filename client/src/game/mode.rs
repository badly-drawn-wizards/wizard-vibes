// The mode (web/mode.ts), cycled by F (or Y / Start on a controller) or
// chosen in the top bar: fixed (he follows the flight path, as in the
// demo), flight (free flight) and fight (free flight, with the enemies).
// Remembered; a note says which on each change.

use web_time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeName {
    Fixed,
    Flight,
    Fight,
}

pub const ORDER: [ModeName; 3] = [ModeName::Fixed, ModeName::Flight, ModeName::Fight];

impl ModeName {
    pub fn label(self) -> &'static str {
        match self {
            ModeName::Fixed => crate::i18n::tr("Fixed: on the flight path"),
            ModeName::Flight => crate::i18n::tr("Flight: free flight"),
            ModeName::Fight => crate::i18n::tr("Fight: enemies about"),
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            ModeName::Fixed => crate::i18n::tr("Fixed"),
            ModeName::Flight => crate::i18n::tr("Flight"),
            ModeName::Fight => crate::i18n::tr("Fight"),
        }
    }
    // As stored (the TS's names).
    pub fn key(self) -> &'static str {
        match self {
            ModeName::Fixed => "fixed",
            ModeName::Flight => "flight",
            ModeName::Fight => "fight",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        ORDER.into_iter().find(|m| m.key() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Mode {
    pub name: ModeName,
    allowed: [bool; 3],
    // why a mode is ruled out
    reasons: [Option<String>; 3],
    // when the note was last shown
    pub noted: Option<Instant>,
}

fn index(m: ModeName) -> usize {
    ORDER.iter().position(|&x| x == m).unwrap_or(0)
}

impl Mode {
    pub fn new(name: ModeName) -> Self {
        Mode { name, allowed: [true; 3], reasons: Default::default(), noted: None }
    }

    // Rules a mode out (the TS: fight under WebGL, where spells cannot be
    // drawn).
    pub fn forbid(&mut self, m: ModeName, reason: &str) {
        self.allowed[index(m)] = false;
        self.reasons[index(m)] = Some(reason.to_string());
        if self.name == m {
            self.name = ModeName::Flight;
        }
    }

    pub fn allowed(&self, m: ModeName) -> bool {
        self.allowed[index(m)]
    }

    // A button's tooltip.
    pub fn tooltip(&self, m: ModeName) -> String {
        self.reasons[index(m)].clone().unwrap_or_else(|| crate::i18n::trf("{} (F cycles)", &[&m.label()]))
    }

    pub fn free(&self) -> bool {
        self.name != ModeName::Fixed
    }

    pub fn fight(&self) -> bool {
        self.name == ModeName::Fight
    }

    // Whether it changed.
    pub fn cycle(&mut self) -> bool {
        let mut i = index(self.name);
        loop {
            i = (i + 1) % ORDER.len();
            if self.allowed[i] {
                break;
            }
        }
        self.set(ORDER[i])
    }

    // Whether it changed.
    pub fn set(&mut self, m: ModeName) -> bool {
        if !self.allowed(m) || m == self.name {
            return false;
        }
        self.name = m;
        self.noted = Some(Instant::now());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_past_forbidden() {
        let mut m = Mode::new(ModeName::Fight);
        assert!(m.cycle());
        assert_eq!(m.name, ModeName::Fixed);
        m.forbid(ModeName::Fight, "no");
        m.cycle();
        assert_eq!(m.name, ModeName::Flight);
        m.cycle();
        assert_eq!(m.name, ModeName::Fixed);
        assert!(!m.set(ModeName::Fight));
        assert_eq!(ModeName::from_key("fight"), Some(ModeName::Fight));
    }
}
