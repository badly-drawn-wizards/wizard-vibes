// The controls panel (web/help.ts), bottom right: keyboard keys or
// controller glyphs, whichever was used last, and the spell in hand.
// Dismissing it leaves a help button that brings it back; that choice is
// remembered.

use super::hud::{self, Chip, Chip::*};
use super::spells::SPELL_NAMES;
use egui::{Align2, Color32, Margin, Rect, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Pad,
}

const KEYBOARD: &[(&str, &[Chip])] = &[
    ("Mode", &[Key("F")]),
    ("Fly", &[Key("W"), Key("A"), Key("S"), Key("D")]),
    ("Climb / descend", &[Key("E"), Text(" / "), Key("Q")]),
    ("Boost", &[Key("Shift")]),
    ("Spell", &[Key("1"), Key("2"), Key("3"), Key("4")]),
    ("Cast", &[Key("Space")]),
    ("Shield", &[Key("R"), Text(" (hold)")]),
    ("Try again (defeated)", &[Key("Enter")]),
    ("Camera", &[Text("drag · wheel")]),
    ("Pause / menu", &[Key("P"), Text(" / "), Key("Esc")]),
    ("Music", &[Key("M")]),
];

const PAD: &[(&str, &[Chip])] = &[
    ("Mode", &[ShoulderIcon(hud::Icon::View)]),
    ("Pause / menu", &[Shoulder("≡")]),
    ("Fly", &[Stick('L')]),
    ("Camera", &[Stick('R')]),
    ("Zoom in / out", &[Text("D-pad ↑ / ↓")]),
    ("Climb / descend", &[Shoulder("RT"), Text(" / "), Shoulder("LT")]),
    ("Boost", &[Stick('L'), Text("↓")]),
    ("Spell", &[Face('X'), Face('A'), Face('B'), Face('Y')]),
    ("Cast", &[Shoulder("RB")]),
    ("Shield", &[Shoulder("LB"), Text(" (hold)")]),
];

#[derive(Clone, Debug)]
pub struct Help {
    pub closed: bool,
    pub device: Device,
    pub spell: usize,
}

impl Help {
    pub fn new(closed: bool) -> Self {
        Help { closed, device: Device::Keyboard, spell: 1 }
    }

    // The panel, or its button, at the bottom right of `rect` (the view).
    pub fn ui(&mut self, ctx: &egui::Context, rect: Rect) {
        let corner = rect.right_bottom() - Vec2::new(12.0, 12.0);
        if self.closed {
            egui::Area::new(egui::Id::unique("wizard-help-open"))
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(corner)
                .show(ctx, |ui| {
                    let b = egui::Button::new(hud::text("?", hud::FONT, hud::FG))
                        .fill(hud::bg(0.6))
                        .stroke(Stroke::new(1.0, hud::BORDER))
                        .corner_radius(15)
                        .min_size(Vec2::splat(30.0));
                    if ui.add(b).on_hover_text("Controls").clicked() {
                        self.closed = false;
                    }
                });
            return;
        }
        egui::Area::new(egui::Id::unique("wizard-help"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(corner)
            .show(ctx, |ui| {
                hud::panel(0.75, 8, Margin::symmetric(12, 10)).stroke(Stroke::new(1.0, hud::BORDER)).show(ui, |ui| {
                    let width = 270.0f32.min(rect.width() - 24.0) - 24.0;
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        ui.label(hud::text("Controls", 12.0, hud::FG));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // (dim, lit when hovered)
                            let (r, close) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::click());
                            let colour = if close.hovered() { hud::FG } else { hud::DIM };
                            hud::paint_icon(ui.painter(), r, hud::Icon::Cross, colour);
                            if close.on_hover_text("Hide").clicked() {
                                self.closed = true;
                            }
                        });
                    });
                    ui.add_space(2.0);
                    let rows = if self.device == Device::Pad { PAD } else { KEYBOARD };
                    for (what, how) in rows {
                        ui.horizontal(|ui| {
                            ui.label(hud::text(*what, 12.0, hud::DIM));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // (laid out from the right: the chips in reverse)
                                ui.spacing_mut().item_spacing.x = 0.0;
                                for &c in how.iter().rev() {
                                    hud::chip(ui, c);
                                }
                            });
                        });
                    }
                    ui.add_space(4.0);
                    let (line, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), egui::Sense::hover());
                    ui.painter().rect_filled(line, 0, hud::BORDER);
                    ui.add_space(4.0);
                    let keys: [Chip; 4] = if self.device == Device::Pad {
                        [Face('X'), Face('A'), Face('B'), Face('Y')]
                    } else {
                        [Key("1"), Key("2"), Key("3"), Key("4")]
                    };
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for (i, name) in SPELL_NAMES.iter().enumerate() {
                        let on = i == self.spell;
                        let colour: Color32 = if on { hud::FG } else { hud::DIM };
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            hud::chips(ui, &[keys[i]]);
                            let label = if on { format!(" {name} ◂") } else { format!(" {name}") };
                            ui.label(hud::text(label, 12.0, colour));
                        });
                    }
                });
            });
    }
}
