// The quality panel (web/quality.ts): Auto / Low / Med / High. Each level
// is a preset of the compile-time step counts (params.json's "quality");
// choosing one asks the app for that level (Game::quality_request), which
// loads its build and says when it runs (Game::quality_done).
//
// Auto watches the frame rate: once a level has settled it steps down if
// frames come slower than 40 fps (and will not go back up to that level),
// and up if they keep near the display's rate.

use web_time::Instant;

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
    fn index(self) -> usize {
        LEVELS.iter().position(|&l| l == self).unwrap_or(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Fixed(Level),
}

impl Choice {
    pub fn key(self) -> &'static str {
        match self {
            Choice::Auto => "auto",
            Choice::Fixed(l) => l.key(),
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        if s == "auto" { Some(Choice::Auto) } else { Level::from_key(s).map(Choice::Fixed) }
    }
}

const SETTLE_S: f32 = 2.5; // after a switch, before judging the frame rate
const WINDOW_S: f32 = 3.0; // how long a judgement averages over
const SLOW_FPS: f32 = 40.0;
const FAST_FPS: f32 = 57.0; // near a 60 Hz display's rate: room for more

#[derive(Clone, Debug)]
pub struct Quality {
    // None: none (the compile-time parameters match no preset)
    pub choice: Option<Choice>,
    pub level: Option<Level>,
    // as stored (the choice, before setup() applies a fallback)
    stored: Option<Choice>,
    pub enabled: bool,
    pub busy: bool,
    // the level asked of the app, not yet taken by it
    request: Option<Level>,
    since: Instant, // when the level last settled
    frames: u32,
    window_start: Instant,
    too_slow: Vec<Level>,
}

impl Quality {
    pub fn new(stored: Option<Choice>) -> Self {
        Quality {
            choice: stored,
            level: None,
            stored,
            enabled: false,
            busy: false,
            request: None,
            since: Instant::now(),
            frames: 0,
            window_start: Instant::now(),
            too_slow: Vec::new(),
        }
    }

    // `level`: the level running now, if known. `fallback`: the choice when
    // none is stored. Disabled, it does nothing (and the panel is not shown).
    pub fn setup(&mut self, level: Option<Level>, fallback: Option<Choice>, enabled: bool) {
        self.level = level;
        self.choice = self.stored.or(fallback);
        self.enabled = enabled;
        self.since = Instant::now();
        self.frames = 0;
    }

    // The level to start at: the chosen one, or medium for auto.
    pub fn initial(&self) -> Level {
        match self.choice {
            Some(Choice::Fixed(l)) => l,
            _ => Level::Med,
        }
    }

    pub fn choose(&mut self, c: Choice) {
        self.choice = Some(c);
        self.stored = Some(c);
        self.too_slow.clear();
        if let Choice::Fixed(l) = c {
            self.switch_to(l);
        }
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
        self.since = Instant::now();
        self.frames = 0;
    }

    // Called once per drawn frame; `counting` is false while paused or hidden.
    pub fn frame(&mut self, counting: bool) {
        self.frame_at(counting, Instant::now());
    }

    pub fn frame_at(&mut self, counting: bool, now: Instant) {
        if !self.enabled || self.choice != Some(Choice::Auto) || self.busy || !counting {
            self.frames = 0;
            return;
        }
        if now.duration_since(self.since).as_secs_f32() < SETTLE_S {
            return;
        }
        if self.frames == 0 {
            self.window_start = now;
        }
        self.frames += 1;
        let elapsed = now.duration_since(self.window_start).as_secs_f32();
        if elapsed < WINDOW_S {
            return;
        }
        let fps = (self.frames - 1) as f32 / elapsed;
        self.frames = 0;
        let i = self.level.map_or(1, Level::index);
        if fps < SLOW_FPS && i > 0 {
            if let Some(l) = self.level {
                self.too_slow.push(l);
            }
            self.switch_to(LEVELS[i - 1]);
        } else if fps > FAST_FPS && i < LEVELS.len() - 1 && !self.too_slow.contains(&LEVELS[i + 1]) {
            self.switch_to(LEVELS[i + 1]);
        } else if self.level.is_none() {
            self.switch_to(LEVELS[i]);
        }
    }

    // The panel: a button per choice (in the top bar).
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
            let choices = [Choice::Auto, Choice::Fixed(Level::Low), Choice::Fixed(Level::Med), Choice::Fixed(Level::High)];
            for c in choices {
                let text = match (c, self.choice, self.level) {
                    (Choice::Auto, Some(Choice::Auto), Some(l)) => format!("Auto·{}", l.key()),
                    (Choice::Auto, ..) => "Auto".to_string(),
                    (Choice::Fixed(l), ..) => l.name().to_string(),
                };
                let pressed = Some(c) == self.choice;
                // (in auto, the level it picked stands out too)
                let active = matches!(c, Choice::Fixed(l) if Some(l) == self.level) && self.choice == Some(Choice::Auto);
                let tip = match c {
                    Choice::Auto => "Pick the quality by the frame rate".to_string(),
                    Choice::Fixed(l) => {
                        format!("Quality: {}{}", l.name(), if self.busy && pressed { " (loading…)" } else { "" })
                    }
                };
                if super::hud::toggle_button(ui, &text, pressed, active).on_hover_text(tip).clicked() {
                    chosen = Some(c);
                }
            }
        });
        if let Some(c) = chosen {
            self.choose(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // Frames at `fps` for `secs`, from `t` (or when it last settled, if
    // later); the time after.
    fn run(q: &mut Quality, t: Option<Instant>, fps: f32, secs: f32) -> Instant {
        let n = (fps * secs) as u32;
        let mut now = t.map_or(q.since, |t| t.max(q.since));
        for _ in 0..n {
            now += Duration::from_secs_f32(1.0 / fps);
            q.frame_at(true, now);
        }
        now
    }

    #[test]
    fn auto_steps_down_and_not_back_up() {
        let mut q = Quality::new(None);
        q.setup(Some(Level::Med), Some(Choice::Auto), true);
        let t = run(&mut q, None, 30.0, 6.0);
        assert_eq!(q.take_request(), Some(Level::Low));
        assert!(q.busy);
        q.done(Some(Level::Low));
        // fast now: but med was too slow
        run(&mut q, Some(t), 60.0, 6.0);
        assert_eq!(q.take_request(), None);
        // a fixed choice asks for its level at once
        q.choose(Choice::Fixed(Level::High));
        assert_eq!(q.take_request(), Some(Level::High));
        assert_eq!(q.initial(), Level::High);
    }

    #[test]
    fn auto_steps_up() {
        let mut q = Quality::new(Some(Choice::Auto));
        q.setup(Some(Level::Med), Some(Choice::Fixed(Level::Low)), true);
        assert_eq!(q.choice, Some(Choice::Auto));
        run(&mut q, None, 60.0, 6.0);
        assert_eq!(q.take_request(), Some(Level::High));
    }
}
