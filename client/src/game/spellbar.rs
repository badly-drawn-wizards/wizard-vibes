// The spells, bottom right (in place of the TS page's controls panel): a
// round glyph each, the one in hand lit. For a controller, laid out as its
// face buttons are (Y above, X and B either side, A below; a spell bound
// elsewhere takes a place left free), each with its button's glyph at its
// lower right; for the keyboard, in a row, each with its key.

use super::binds::{Action, Binds, PadButton};
use super::hud;
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
}

impl Default for SpellBar {
    fn default() -> Self {
        SpellBar { device: Device::Keyboard, spell: 1 }
    }
}

// (a spell's circle's radius, and the gap between two)
const R: f32 = 21.0;
const GAP: f32 = 7.0;

impl SpellBar {
    // The glyphs, at the bottom right of `rect` (the view).
    pub fn ui(&self, ctx: &egui::Context, rect: Rect, binds: &Binds) {
        let pad = self.device == Device::Pad;
        // (the circles' centres, from the area's top left)
        let centres: Vec<Vec2> = if pad {
            let d = 1.5 * R + 5.0;
            let mid = Vec2::splat(d + R + 6.0);
            // (each face button's place: Y, X, B, A)
            let places = [Vec2::new(0.0, -d), Vec2::new(-d, 0.0), Vec2::new(d, 0.0), Vec2::new(0.0, d)];
            let faces = [PadButton::Y, PadButton::X, PadButton::B, PadButton::A];
            let mut taken = [false; 4];
            let mut at = [None; 4];
            for i in 0..4 {
                if let Some(k) = binds.pad(Action::Spell(i)).and_then(|b| faces.iter().position(|&f| f == b)) {
                    if !taken[k] {
                        taken[k] = true;
                        at[i] = Some(k);
                    }
                }
            }
            (0..4)
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
            (0..4).map(|i| Vec2::new(R + 2.0 + i as f32 * (2.0 * R + GAP), R + 2.0)).collect()
        };
        let size = if pad { Vec2::splat(2.0 * (2.5 * R + 11.0)) } else { Vec2::new(4.0 * (2.0 * R + GAP) + 8.0, 2.0 * R + 12.0) };
        egui::Area::new(egui::Id::unique("wizard-spells"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(rect.right_bottom() - Vec2::new(14.0, 14.0))
            .interactable(false)
            .show(ctx, |ui| {
                let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let p = ui.painter();
                for (i, off) in centres.iter().enumerate() {
                    let c = area.min + *off;
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
                    hud::paint_spell(p, c, 0.62 * R, i, if on { colour } else { colour.gamma_multiply(0.45) });
                    // its button, or key, at its lower right
                    let corner = c + Vec2::splat(0.7 * R);
                    if pad {
                        if let Some(b) = binds.pad(Action::Spell(i)) {
                            hud::paint_pad_glyph(p, corner, 7.5, b.chip());
                        }
                    } else if let Some(k) = binds.key(Action::Spell(i)) {
                        hud::paint_key_glyph(p, corner + Vec2::new(0.0, 1.0), 15.0, k.name());
                    }
                }
            });
    }
}
