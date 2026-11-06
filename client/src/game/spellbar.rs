// The spells in his slots (his loadout), bottom right (in place of the TS
// page's controls panel): a round glyph each, the one in hand lit. For a controller, laid out as its
// face buttons are (Y above, X and B either side, A below; a spell bound
// elsewhere takes a place left free), each with its button's glyph at its
// lower right; for the keyboard, in a row, each with its key. A spell
// cooling down is shaded, the shade sweeping away as it comes ready; the
// wind blades' heat rings theirs, red and flashing once overheated.

use super::binds::{Action, Binds, PadButton};
use super::hud;
use super::spells::N_SPELLS;
use egui::{Align2, Rect, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Pad,
}

#[derive(Clone, Debug)]
pub struct SpellBar {
    // the device used last (its glyphs shown)
    pub device: Device,
    pub spell: usize,
    // each spell's cooldown left (1 just cast .. 0 ready), and the wind
    // blades' heat (0..1), overheated or not
    pub cooling: [f32; N_SPELLS],
    pub heat: f32,
    pub overheated: bool,
    // the spells he has (the others shown locked)
    pub unlocked: [bool; N_SPELLS],
    // the spell in each of his slots (slot k cast by Spell(k)'s binding;
    // none: an empty slot)
    pub loadout: Vec<Option<usize>>,
}

impl Default for SpellBar {
    fn default() -> Self {
        SpellBar { device: Device::Keyboard, spell: 0, cooling: [0.0; N_SPELLS], heat: 0.0, overheated: false, unlocked: [true; N_SPELLS], loadout: vec![Some(0)] }
    }
}

// (a spell's circle's radius, and the gap between two)
const R: f32 = 21.0;
const GAP: f32 = 7.0;

impl SpellBar {
    // The glyphs, at the bottom right of `rect` (the view).
    pub fn ui(&self, ctx: &egui::Context, rect: Rect, binds: &Binds) {
        let pad = self.device == Device::Pad;
        // (all four slots: those he has not yet, locked)
        let n = 4;
        // (the circles' centres, from the area's top left)
        let centres: Vec<Vec2> = if pad {
            let d = 1.5 * R + 5.0;
            let mid = Vec2::splat(d + R + 6.0);
            // (each face button's place: Y, X, B, A)
            let places = [Vec2::new(0.0, -d), Vec2::new(-d, 0.0), Vec2::new(d, 0.0), Vec2::new(0.0, d)];
            let faces = [PadButton::Y, PadButton::X, PadButton::B, PadButton::A];
            let mut taken = [false; 4];
            let mut at = [None; 4];
            for i in 0..n {
                if let Some(k) = binds.pad(Action::Spell(i)).and_then(|b| faces.iter().position(|&f| f == b)) {
                    if !taken[k] {
                        taken[k] = true;
                        at[i] = Some(k);
                    }
                }
            }
            (0..n)
                .map(|i| {
                    let k = at[i].unwrap_or_else(|| {
                        let k = taken.iter().position(|t| !t).unwrap_or(0);
                        taken[k] = true;
                        k
                    });
                    mid + places[k]
                })
                .collect()
        } else {
            (0..n).map(|i| Vec2::new(R + 2.0 + i as f32 * (2.0 * R + GAP), R + 2.0)).collect()
        };
        let size = if pad { Vec2::splat(2.0 * (2.5 * R + 11.0)) } else { Vec2::new(n as f32 * (2.0 * R + GAP) + 8.0, 2.0 * R + 12.0) };
        egui::Area::new(egui::Id::unique("wizard-spells"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(rect.right_bottom() - Vec2::new(14.0, 14.0))
            .interactable(false)
            .show(ctx, |ui| {
                let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let p = ui.painter();
                for (slot, off) in centres.iter().enumerate() {
                    let c = area.min + *off;
                    if slot >= self.loadout.len() {
                        // (a slot he has yet to unlock: a padlock)
                        p.circle_filled(c, R, hud::bg(0.35));
                        p.circle_stroke(c, R, Stroke::new(1.0, hud::BORDER));
                        hud::paint_lock(p, c, 0.45 * R, hud::DIM);
                        continue;
                    }
                    let Some(i) = self.loadout.get(slot).copied().flatten().map(|i| i.min(N_SPELLS - 1)) else {
                        // (an empty slot)
                        p.circle_filled(c, R, hud::bg(0.35));
                        p.circle_stroke(c, R, Stroke::new(1.0, hud::BORDER));
                        continue;
                    };
                    let on = i == self.spell;
                    let colour = hud::SPELL_COLOURS[i];
                    if on {
                        p.circle_filled(c, R + 3.0, colour.gamma_multiply(0.22));
                        p.circle_filled(c, R, hud::bg(0.85));
                        p.circle_stroke(c, R, Stroke::new(2.5, colour));
                    } else {
                        p.circle_filled(c, R, hud::bg(0.6));
                        p.circle_stroke(c, R, Stroke::new(1.0, hud::BORDER));
                    }
                    if !self.unlocked[i] {
                        // (locked: a padlock, the spell faint behind it)
                        hud::paint_spell(p, c, 0.62 * R, i, colour.gamma_multiply(0.12));
                        hud::paint_lock(p, c, 0.45 * R, hud::DIM);
                        continue;
                    }
                    hud::paint_spell(p, c, 0.62 * R, i, if on { colour } else { colour.gamma_multiply(0.45) });
                    // cooling down: the part still to come shaded, from the
                    // top round clockwise
                    if self.cooling[i] > 0.0 {
                        let f = self.cooling[i];
                        sector(p, c, R - 1.0, f, egui::Color32::from_rgba_unmultiplied(170, 160, 230, 80));
                        // (its edge, sweeping)
                        let a = std::f32::consts::TAU * f;
                        p.line_segment([c, c + Vec2::new(a.sin(), -a.cos()) * (R - 1.0)], Stroke::new(1.5, hud::FG));
                        ctx.request_repaint();
                    }
                    // the wind blades' heat round theirs
                    if i == 2 && (self.heat > 0.0 || self.overheated) {
                        let hot = if self.overheated {
                            let flash = 0.6 + 0.4 * (ctx.input(|i| i.time) as f32 * 10.0).sin();
                            egui::Color32::from_rgb(0xff, 0x40, 0x30).gamma_multiply(flash)
                        } else {
                            heat_colour(self.heat)
                        };
                        arc(p, c, R + 4.0, self.heat.max(if self.overheated { 1.0 } else { 0.0 }), Stroke::new(3.0, hot));
                        ctx.request_repaint();
                    }
                    // its button, or key, at its lower right
                    let corner = c + Vec2::splat(0.7 * R);
                    if pad {
                        if let Some(b) = binds.pad(Action::Spell(slot)) {
                            hud::paint_pad_glyph(p, corner, 7.5, b.chip());
                        }
                    } else if let Some(k) = binds.key(Action::Spell(slot)) {
                        hud::paint_key_glyph(p, corner + Vec2::new(0.0, 1.0), 15.0, k.name());
                    }
                }
            });
    }
}

// The part `frac` (0..1) of the disc of radius r round c, from the top
// clockwise, filled.
fn sector(p: &egui::Painter, c: egui::Pos2, r: f32, frac: f32, fill: egui::Color32) {
    let n = (48.0 * frac).ceil().max(1.0) as usize;
    let at = |k: usize| {
        let a = std::f32::consts::TAU * frac * k as f32 / n as f32;
        c + Vec2::new(a.sin(), -a.cos()) * r
    };
    // (a fan of thin slices: each convex)
    for k in 0..n {
        p.add(egui::Shape::convex_polygon(vec![c, at(k), at(k + 1)], fill, Stroke::NONE));
    }
}

// The part `frac` (0..1) of the circle of radius r round c, from the top
// clockwise.
fn arc(p: &egui::Painter, c: egui::Pos2, r: f32, frac: f32, stroke: Stroke) {
    let n = (64.0 * frac).ceil().max(1.0) as usize;
    let pts = (0..=n)
        .map(|k| {
            let a = std::f32::consts::TAU * frac * k as f32 / n as f32;
            c + Vec2::new(a.sin(), -a.cos()) * r
        })
        .collect();
    p.add(egui::Shape::line(pts, stroke));
}

// The heat's colour: cool cyan, through amber, to red.
fn heat_colour(h: f32) -> egui::Color32 {
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t);
    let (cyan, amber, red) = ([0.42, 0.91, 1.0], [1.0, 0.75, 0.25], [1.0, 0.25, 0.19]);
    let c = if h < 0.6 { lerp(cyan, amber, h / 0.6) } else { lerp(amber, red, (h - 0.6) / 0.4) };
    egui::Color32::from_rgb((255.0 * c[0]) as u8, (255.0 * c[1]) as u8, (255.0 * c[2]) as u8)
}
