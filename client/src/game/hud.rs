// The HUD's look (web/index.html's <style>), redone in egui: its colours,
// its panels and buttons, and the pieces drawn by hand (bars, crosshair,
// the hurt flash, key and controller glyphs).

use egui::{
    epaint::Vertex, Align2, Color32, CornerRadius, FontId, Frame, Margin, Mesh, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2,
};

pub const FG: Color32 = Color32::from_rgb(0xd8, 0xd4, 0xff);
pub const DIM: Color32 = Color32::from_rgb(0x8a, 0x86, 0xb8);
pub const BORDER: Color32 = Color32::from_rgb(0x2c, 0x29, 0x50);
pub const KBD_BORDER: Color32 = Color32::from_rgb(0x4a, 0x46, 0x80);
pub const SHORT: Color32 = Color32::from_rgb(0xff, 0x6a, 0x5a);
pub const DEFEAT: Color32 = Color32::from_rgb(0xff, 0x8a, 0x8a);
pub const FONT: f32 = 13.0;

// The page's background (#05040d) at `alpha`.
pub fn bg(alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(5, 4, 13, (alpha * 255.0).round() as u8)
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn text(s: impl Into<String>, size: f32, colour: Color32) -> RichText {
    RichText::new(s).font(mono(size)).color(colour)
}

// A floating panel's frame: the page's background at `alpha`, rounded.
pub fn panel(alpha: f32, radius: u8, margin: Margin) -> Frame {
    Frame::new().fill(bg(alpha)).corner_radius(CornerRadius::same(radius)).inner_margin(margin)
}

// A top-bar button (#hud button): dim and outlined; `pressed` (its
// aria-pressed) lights its text and border, `active` its text.
pub fn toggle_button(ui: &mut Ui, label: &str, pressed: bool, active: bool) -> Response {
    let colour = if pressed || active { FG } else { DIM };
    let border = if pressed { FG } else { BORDER };
    let r = padded(ui, Vec2::new(6.0, 2.0), |ui| {
        ui.add(
            egui::Button::new(text(label, FONT, colour))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, border))
                .corner_radius(4),
        )
    });
    focus_ring(ui, &r);
    r
}

// A ring round a widget that has the keyboard's (or controller's) focus,
// for those drawn in their own colours, which egui's focus look does not
// reach.
pub fn focus_ring(ui: &Ui, r: &Response) {
    if r.has_focus() {
        ui.painter().rect_stroke(r.rect.expand(2.5), 5, Stroke::new(2.0, FOCUS), egui::StrokeKind::Outside);
    }
}

// (the focus ring's colour)
pub const FOCUS: Color32 = Color32::from_rgb(0xff, 0xc8, 0x57);

// A switch whose state shows by its fill: filled when on, outlined when
// off (its label the same either way).
pub fn fill_toggle(ui: &mut Ui, label: &str, on: bool) -> Response {
    let (fill, colour) = if on { (FG, Color32::from_rgb(0x10, 0x12, 0x1c)) } else { (Color32::TRANSPARENT, DIM) };
    let r = padded(ui, Vec2::new(7.0, 2.0), |ui| {
        ui.add(egui::Button::new(text(label, FONT, colour)).fill(fill).stroke(Stroke::new(1.0, if on { FG } else { BORDER })).corner_radius(4))
    });
    focus_ring(ui, &r);
    r
}

// A small triangle button pointing right (closed) or left (open), as an
// accordion's handle.
pub fn triangle(ui: &mut Ui, open: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(14.0, 16.0), egui::Sense::click());
    let c = r.center();
    let colour = if resp.hovered() || resp.has_focus() { FG } else { DIM };
    let (dx, h) = (4.0, 5.0);
    let pts = if open {
        vec![c + Vec2::new(dx, -h), c + Vec2::new(dx, h), c + Vec2::new(-dx, 0.0)]
    } else {
        vec![c + Vec2::new(-dx, -h), c + Vec2::new(-dx, h), c + Vec2::new(dx, 0.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, colour, Stroke::NONE));
    resp
}

fn padded<R>(ui: &mut Ui, padding: Vec2, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = padding;
        add(ui)
    })
    .inner
}

// An outlined button in the foreground colour (#vitals .reset, #defeat
// button), with `padding` round its text.
pub fn plain_button(ui: &mut Ui, label: &str, size: f32, padding: Vec2) -> Response {
    padded(ui, padding, |ui| {
        ui.add(
            egui::Button::new(text(label, size, FG))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(4),
        )
    })
}

// A bar (#vitals .bar): `frac` of it filled with a gradient from `a` to `b`,
// outlined in `border`.
pub fn bar(ui: &mut Ui, width: f32, frac: f32, a: Color32, b: Color32, border: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 11.0), Sense::hover());
    let inner = rect.shrink(1.0);
    let fill = Rect::from_min_size(inner.min, Vec2::new(inner.width() * frac.clamp(0.0, 1.0), inner.height()));
    if fill.width() > 0.0 {
        // (the gradient spans the filled part, as the span's own background)
        let mut mesh = Mesh::default();
        let [l, r] = [a, b];
        for (p, c) in [(fill.left_top(), l), (fill.right_top(), r), (fill.right_bottom(), r), (fill.left_bottom(), l)] {
            mesh.vertices.push(Vertex { pos: p, uv: egui::epaint::WHITE_UV, color: c });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().with_clip_rect(inner).add(Shape::mesh(mesh));
    }
    ui.painter().rect_stroke(rect, 4, Stroke::new(1.0, border), StrokeKind::Inside);
}

// The crosshair (#crosshair), centred on `c`: a cross in a ring, or in a
// red diamond when `locked`; faint while the shield is up.
pub fn crosshair(painter: &egui::Painter, c: Pos2, locked: bool, shielded: bool) {
    let opacity = if shielded { 0.3 } else if locked { 1.0 } else { 0.75 };
    let white = Color32::WHITE.gamma_multiply(opacity);
    painter.rect_filled(Rect::from_center_size(c, Vec2::new(2.0, 7.0)), 0, white);
    painter.rect_filled(Rect::from_center_size(c, Vec2::new(7.0, 2.0)), 0, white);
    if locked {
        // a 22 px square with rounded corners, turned 45°
        let r = 11.0 * std::f32::consts::SQRT_2 - 0.75;
        let pts = vec![c + Vec2::new(0.0, -r), c + Vec2::new(r, 0.0), c + Vec2::new(0.0, r), c + Vec2::new(-r, 0.0)];
        let red = Color32::from_rgb(0xff, 0x5a, 0x6a).gamma_multiply(opacity);
        painter.add(Shape::closed_line(pts, Stroke::new(1.5, red)));
    } else {
        painter.circle_stroke(c, 10.25, Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.8 * opacity)));
    }
}

// The hurt flash (#hurt): over `rect`, clear in the middle and red towards
// the edges (radial-gradient(ellipse at center, transparent 45%,
// rgb(170 10 20 / 0.85)), the ellipse through the corners), at `opacity`.
pub fn hurt(painter: &egui::Painter, rect: Rect, opacity: f32) {
    if opacity < 0.004 {
        return;
    }
    const N: usize = 24;
    let red = |t: f32| {
        let k = ((t - 0.45) / 0.55).clamp(0.0, 1.0) * 0.85 * opacity;
        Color32::from_rgba_unmultiplied(170, 10, 20, (k * 255.0).round() as u8)
    };
    let mut mesh = Mesh::default();
    for j in 0..=N {
        for i in 0..=N {
            let u = i as f32 / N as f32;
            let v = j as f32 / N as f32;
            // (the farthest corner is at 1)
            let t = ((2.0 * u - 1.0).powi(2) + (2.0 * v - 1.0).powi(2)).sqrt() / std::f32::consts::SQRT_2;
            let pos = Pos2::new(rect.left() + u * rect.width(), rect.top() + v * rect.height());
            mesh.vertices.push(Vertex { pos, uv: egui::epaint::WHITE_UV, color: red(t) });
        }
    }
    let w = (N + 1) as u32;
    for j in 0..N as u32 {
        for i in 0..N as u32 {
            let a = j * w + i;
            mesh.add_triangle(a, a + 1, a + w + 1);
            mesh.add_triangle(a, a + w + 1, a + w);
        }
    }
    painter.add(Shape::mesh(mesh));
}

// A glyph in the controls panel: a key, or a controller's button.
#[derive(Clone, Copy, Debug)]
pub enum Chip {
    Text(&'static str),
    // a key (<kbd>)
    Key(&'static str),
    // a face button (A B X Y), in its colour
    Face(char),
    // a shoulder or menu button
    Shoulder(&'static str),
    // a menu button with an icon (View)
    ShoulderIcon(Icon),
    // a stick
    Stick(char),
}

// A row of chips, laid out left to right.
pub fn chips(ui: &mut Ui, row: &[Chip]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for &c in row {
            chip(ui, c);
        }
    });
}

pub fn chip(ui: &mut Ui, c: Chip) {
    let bold = |ui: &Ui, s: &str, colour: Color32| {
        ui.painter().layout_no_wrap(s.to_string(), FontId::proportional(10.0), colour)
    };
    match c {
        Chip::Text(s) => {
            ui.label(text(s, 12.0, DIM));
        }
        Chip::Key(k) => {
            let g = ui.painter().layout_no_wrap(k.to_string(), mono(11.0), FG);
            let w = (g.size().x + 8.0).max(1.4 * 11.0 + 8.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w + 2.0, 17.0), Sense::hover());
            let r = Rect::from_min_max(Pos2::new(rect.left() + 2.0, rect.top()), rect.max);
            let p = ui.painter();
            p.rect_stroke(r, 4, Stroke::new(1.0, KBD_BORDER), StrokeKind::Inside);
            // (its bottom edge thicker)
            p.line_segment([r.left_bottom() + Vec2::new(3.0, -1.5), r.right_bottom() + Vec2::new(-3.0, -1.5)], Stroke::new(1.0, KBD_BORDER));
            p.galley(Align2::CENTER_CENTER.anchor_size(r.center() - Vec2::new(0.0, 0.5), g.size()).min, g, FG);
        }
        Chip::Face(b) => {
            let (fill, ink) = match b {
                'A' => (Color32::from_rgb(0x3f, 0x9c, 0x35), Color32::WHITE),
                'B' => (Color32::from_rgb(0xc8, 0x35, 0x2e), Color32::WHITE),
                'X' => (Color32::from_rgb(0x2c, 0x6f, 0xd1), Color32::WHITE),
                _ => (Color32::from_rgb(0xc9, 0xa2, 0x27), Color32::from_rgb(0x22, 0x22, 0x22)),
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 18.0), Sense::hover());
            let c = Pos2::new(rect.left() + 11.0, rect.center().y);
            ui.painter().circle_filled(c, 9.0, fill);
            let g = bold(ui, &b.to_string(), ink);
            ui.painter().galley(Align2::CENTER_CENTER.anchor_size(c, g.size()).min, g, ink);
        }
        Chip::Shoulder(s) => {
            let g = bold(ui, s, Color32::WHITE);
            let w = g.size().x + 10.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w + 2.0, 18.0), Sense::hover());
            let r = Rect::from_min_max(Pos2::new(rect.left() + 2.0, rect.top()), rect.max);
            let radius = CornerRadius { nw: 9, ne: 9, sw: 4, se: 4 };
            ui.painter().rect_filled(r, radius, Color32::from_rgb(0x3a, 0x37, 0x60));
            ui.painter().galley(Align2::CENTER_CENTER.anchor_size(r.center(), g.size()).min, g, Color32::WHITE);
        }
        Chip::ShoulderIcon(which) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(24.0, 18.0), Sense::hover());
            let r = Rect::from_min_max(Pos2::new(rect.left() + 2.0, rect.top()), rect.max);
            let radius = CornerRadius { nw: 9, ne: 9, sw: 4, se: 4 };
            ui.painter().rect_filled(r, radius, Color32::from_rgb(0x3a, 0x37, 0x60));
            paint_icon(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(10.0)), which, Color32::WHITE);
        }
        Chip::Stick(s) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 18.0), Sense::hover());
            let c = Pos2::new(rect.left() + 11.0, rect.center().y);
            ui.painter().circle_filled(c, 9.0, Color32::from_rgb(0x3a, 0x37, 0x60));
            ui.painter().circle_stroke(c, 7.5, Stroke::new(3.0, Color32::from_rgb(0x26, 0x24, 0x4a)));
            let g = bold(ui, &s.to_string(), Color32::WHITE);
            ui.painter().galley(Align2::CENTER_CENTER.anchor_size(c, g.size()).min, g, Color32::WHITE);
        }
    }
}

// Symbols the default fonts lack, drawn instead: ♥ ✦ ✕ ⧉.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Heart,
    Star,
    Cross,
    // the controller's View button (two overlapping squares)
    View,
}

// An icon `size` points high (and as wide, plus a little room), in `colour`.
pub fn icon(ui: &mut Ui, which: Icon, size: f32, colour: Color32, sense: Sense) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    paint_icon(ui.painter(), rect, which, colour);
    response
}

pub fn paint_icon(p: &egui::Painter, rect: Rect, which: Icon, colour: Color32) {
    let c = rect.center();
    let s = rect.height() * 0.5;
    match which {
        Icon::Heart => {
            // two lobes and a point
            let r = 0.36 * s;
            let l = c + Vec2::new(-0.34 * s, -0.22 * s);
            let rr = c + Vec2::new(0.34 * s, -0.22 * s);
            p.circle_filled(l, r, colour);
            p.circle_filled(rr, r, colour);
            p.add(Shape::convex_polygon(
                vec![l, rr, c + Vec2::new(0.69 * s, -0.12 * s), c + Vec2::new(0.0, 0.62 * s), c + Vec2::new(-0.69 * s, -0.12 * s)],
                colour,
                Stroke::NONE,
            ));
        }
        Icon::Star => {
            // four points, pinched between
            let (o, i) = (0.7 * s, 0.18 * s);
            let pts: Vec<Pos2> = (0..8)
                .map(|k| {
                    let a = k as f32 * std::f32::consts::FRAC_PI_4;
                    let r = if k % 2 == 0 { o } else { i };
                    c + Vec2::new(a.sin(), -a.cos()) * r
                })
                .collect();
            // (not convex: a fan from the centre)
            for k in 0..8 {
                p.add(Shape::convex_polygon(vec![c, pts[k], pts[(k + 1) % 8]], colour, Stroke::NONE));
            }
        }
        Icon::Cross => {
            let d = 0.42 * s;
            let st = Stroke::new(1.3, colour);
            p.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], st);
            p.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], st);
        }
        Icon::View => {
            let w = 0.5 * s;
            let st = Stroke::new(1.0, colour);
            let back = Rect::from_min_size(c + Vec2::new(-0.55 * s, -0.55 * s), Vec2::splat(2.0 * w));
            let front = back.translate(Vec2::splat(0.45 * s));
            p.rect_stroke(back, 1, st, StrokeKind::Inside);
            p.rect_stroke(front, 1, st, StrokeKind::Inside);
        }
    }
}
