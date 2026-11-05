// The quality panel (web/quality.ts): Low / Med / High. Each level is a
// preset of the compile-time step counts (params.json's "quality");
// choosing one asks the app for that level (Game::quality_request), which
// loads its build and says when it runs (Game::quality_done). (The frame
// rate is the render scale's to keep up: see scale.rs.)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Low,
    Med,
    High,
}

pub const LEVELS: [Level; 3] = [Level::Low, Level::Med, Level::High];

impl Level {
    // As params.json's presets and the stored choice name them.
    pub fn key(self) -> &'static str {
        match self {
            Level::Low => "low",
            Level::Med => "med",
            Level::High => "high",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Level::Low => "Low",
            Level::Med => "Med",
            Level::High => "High",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        LEVELS.into_iter().find(|l| l.key() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Quality {
    // the level chosen (None: none yet, e.g. the compile-time parameters
    // match no preset)
    pub choice: Option<Level>,
    pub level: Option<Level>,
    // as stored (the choice, before setup() applies a fallback)
    stored: Option<Level>,
    pub enabled: bool,
    pub busy: bool,
    // the level asked of the app, not yet taken by it
    request: Option<Level>,
}

impl Quality {
    pub fn new(stored: Option<Level>) -> Self {
        Quality { choice: stored, level: None, stored, enabled: false, busy: false, request: None }
    }

    // `level`: the level running now, if known. `fallback`: the choice when
    // none is stored. Disabled, it does nothing (and the panel is not shown).
    pub fn setup(&mut self, level: Option<Level>, fallback: Option<Level>, enabled: bool) {
        self.level = level;
        self.choice = self.stored.or(fallback);
        self.enabled = enabled;
    }

    pub fn choose(&mut self, l: Level) {
        self.choice = Some(l);
        self.stored = Some(l);
        self.switch_to(l);
    }

    fn switch_to(&mut self, level: Level) {
        if Some(level) == self.level && !self.busy {
            return;
        }
        self.busy = true;
        self.request = Some(level);
    }

    // The level the app should load now, if any (taken once).
    pub fn take_request(&mut self) -> Option<Level> {
        self.request.take()
    }

    // The app's switch finished: running `level` (None: it failed, and the
    // old level runs on).
    pub fn done(&mut self, level: Option<Level>) {
        if let Some(l) = level {
            self.level = Some(l);
        }
        self.busy = false;
    }

    // The panel: a button per level (in the top bar).
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.enabled {
            return;
        }
        let mut chosen = None;
        ui.scope(|ui| {
            if self.busy {
                ui.set_opacity(0.7);
            }
            ui.spacing_mut().item_spacing.x = 0.0;
            for l in LEVELS {
                let pressed = Some(l) == self.choice;
                let tip = format!("Quality: {}{}", l.name(), if self.busy && pressed { " (loading…)" } else { "" });
                if super::hud::toggle_button(ui, l.name(), pressed, false).on_hover_text(tip).clicked() {
                    chosen = Some(l);
                }
            }
        });
        if let Some(l) = chosen {
            self.choose(l);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_asks_for_its_level() {
        let mut q = Quality::new(None);
        q.setup(Some(Level::Med), Some(Level::Med), true);
        assert_eq!(q.take_request(), None, "already running");
        q.choose(Level::High);
        assert_eq!(q.take_request(), Some(Level::High));
        assert!(q.busy);
        q.done(Some(Level::High));
        assert!(!q.busy);
        assert_eq!(q.level, Some(Level::High));
    }
}
