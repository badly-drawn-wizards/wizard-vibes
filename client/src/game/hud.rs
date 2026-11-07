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

// The ring round a slider the menu's cursor has taken hold of (its value
// moving with left and right): thicker, and tinted inside.
pub fn held_ring(ui: &Ui, r: &Response) {
    let rect = r.rect.expand(2.5);
    ui.painter().rect_filled(rect, 5, FOCUS.gamma_multiply(0.18));
    ui.painter().rect_stroke(rect, 5, Stroke::new(3.0, FOCUS), egui::StrokeKind::Outside);
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
// Round it, his health (when there is a fight) on the left and his mana on
// the right, as arcs filling from below (the mana's red, short of it); its
// cross at `cross`, where aim assist turns his spell (else its centre).
#[allow(clippy::too_many_arguments)]
pub fn crosshair(painter: &egui::Painter, c: Pos2, cross: Pos2, locked: bool, shielded: bool, health: Option<f32>, mana: f32, short: bool) {
    let opacity = if shielded { 0.3 } else if locked { 1.0 } else { 0.75 };
    let white = Color32::WHITE.gamma_multiply(opacity);
    painter.rect_filled(Rect::from_center_size(cross, Vec2::new(2.0, 7.0)), 0, white);
    painter.rect_filled(Rect::from_center_size(cross, Vec2::new(7.0, 2.0)), 0, white);
    // (the gauges: 100° each side, from below up)
    let gauge = |left: bool, frac: f32, colour: Color32| {
        let r = 22.0;
        let span = 100f32.to_radians();
        let at = |f: f32| {
            let a = std::f32::consts::PI - 0.5 * (std::f32::consts::PI - span) - f * span;
            let a = if left { -a } else { a };
            c + Vec2::new(a.sin(), -a.cos()) * r
        };
        let line = |f0: f32, f1: f32, stroke: Stroke| {
            let n = 24;
            let pts: Vec<Pos2> = (0..=n).map(|k| at(f0 + (f1 - f0) * k as f32 / n as f32)).collect();
            painter.add(Shape::line(pts, stroke));
        };
        line(0.0, 1.0, Stroke::new(3.0, Color32::from_black_alpha(110)));
        if frac > 0.0 {
            line(0.0, frac.clamp(0.0, 1.0), Stroke::new(2.0, colour.gamma_multiply(opacity.max(0.6))));
        }
    };
    if let Some(hp) = health {
        gauge(true, hp, Color32::from_rgb(0xff, 0x5a, 0x5a));
    }
    gauge(false, mana, if short { SHORT } else { Color32::from_rgb(0x6f, 0xb4, 0xff) });
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
    // a direction of the D-pad (up, down, left, right)
    DPad(u8),
    // the mouse, its left (false) or right (true) button pressed (a click),
    // or its middle (its wheel)
    Mouse(bool),
    MouseMiddle,
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
            // (dark, to read on the menu's light panels too)
            p.rect_filled(r, 4, bg(0.85));
            p.rect_stroke(r, 4, Stroke::new(1.0, KBD_BORDER), StrokeKind::Inside);
            // (its bottom edge thicker)
            p.line_segment([r.left_bottom() + Vec2::new(3.0, -1.5), r.right_bottom() + Vec2::new(-3.0, -1.5)], Stroke::new(1.0, KBD_BORDER));
            p.galley(Align2::CENTER_CENTER.anchor_size(r.center() - Vec2::new(0.0, 0.5), g.size()).min, g, FG);
        }
        Chip::Face(b) => {
            let (fill, ink) = face_colours(b);
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
        Chip::Mouse(right) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 18.0), Sense::hover());
            paint_mouse(ui.painter(), rect.center(), 8.0, FG, right);
        }
        Chip::MouseMiddle => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 18.0), Sense::hover());
            paint_mouse_middle(ui.painter(), rect.center(), 8.0, FG);
        }
        Chip::DPad(d) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 18.0), Sense::hover());
            paint_dpad(ui.painter(), Pos2::new(rect.left() + 11.0, rect.center().y), 9.0, d);
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
    // a skull (enemies defeated)
    Skull,
    // a soul (souls gathered)
    Soul,
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
        Icon::Skull => {
            // a round crown, a narrower jaw, two eyes and a nose dark in it
            let crown = c + Vec2::new(0.0, -0.12 * s);
            p.circle_filled(crown, 0.62 * s, colour);
            p.rect_filled(Rect::from_center_size(c + Vec2::new(0.0, 0.42 * s), Vec2::new(0.7 * s, 0.4 * s)), 2, colour);
            let dark = bg(1.0);
            p.circle_filled(crown + Vec2::new(-0.25 * s, 0.05 * s), 0.17 * s, dark);
            p.circle_filled(crown + Vec2::new(0.25 * s, 0.05 * s), 0.17 * s, dark);
            p.add(Shape::convex_polygon(
                vec![c + Vec2::new(0.0, 0.12 * s), c + Vec2::new(-0.07 * s, 0.26 * s), c + Vec2::new(0.07 * s, 0.26 * s)],
                dark,
                Stroke::NONE,
            ));
            for k in [-1.0, 0.0, 1.0] {
                let x = c.x + k * 0.17 * s;
                p.line_segment([Pos2::new(x, c.y + 0.3 * s), Pos2::new(x, c.y + 0.62 * s)], Stroke::new(1.0, dark));
            }
        }
        Icon::Soul => {
            // a round head, and a tail waving away from it
            let head = c + Vec2::new(-0.35 * s, -0.1 * s);
            p.circle_filled(head, 0.32 * s, colour);
            let pts: Vec<Pos2> = (0..=10)
                .map(|k| {
                    let t = k as f32 / 10.0;
                    head + Vec2::new(0.25 * s + 0.75 * s * t, 0.18 * s * (t * 7.0).sin() + 0.3 * s * t)
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(0.18 * s, colour)));
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

// The D-pad (a cross `r` from its centre to its arms' ends), direction `d`
// (up, down, left, right) lit.
pub fn paint_dpad(p: &egui::Painter, c: Pos2, r: f32, d: u8) {
    let w = 0.36 * r;
    let dim = Color32::from_rgb(0x3a, 0x37, 0x60);
    p.rect_filled(Rect::from_center_size(c, Vec2::new(2.0 * r, 2.0 * w)), 1, dim);
    p.rect_filled(Rect::from_center_size(c, Vec2::new(2.0 * w, 2.0 * r)), 1, dim);
    let arm = match d {
        0 => Vec2::new(0.0, -1.0),
        1 => Vec2::new(0.0, 1.0),
        2 => Vec2::new(-1.0, 0.0),
        _ => Vec2::new(1.0, 0.0),
    };
    let size = if arm.x == 0.0 { Vec2::new(2.0 * w, r - w) } else { Vec2::new(r - w, 2.0 * w) };
    p.rect_filled(Rect::from_center_size(c + arm * (0.5 * (r + w)), size), 1, Color32::WHITE);
}

// A controller's button glyph (a chip's), drawn at `c`, `r` its radius (a
// face button's; the others to match).
pub fn paint_pad_glyph(p: &egui::Painter, c: Pos2, r: f32, chip: Chip) {
    let font = FontId::proportional(1.1 * r);
    let label = |s: &str, ink: Color32| {
        let g = p.layout_no_wrap(s.to_string(), font.clone(), ink);
        p.galley(Align2::CENTER_CENTER.anchor_size(c, g.size()).min, g, ink);
    };
    let shoulder = Color32::from_rgb(0x3a, 0x37, 0x60);
    match chip {
        Chip::Face(b) => {
            let (fill, ink) = face_colours(b);
            p.circle_filled(c, r, fill);
            label(&b.to_string(), ink);
        }
        Chip::Shoulder(s) => {
            let w = p.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x + 0.9 * r;
            let rect = Rect::from_center_size(c, Vec2::new(w.max(2.0 * r), 2.0 * r));
            let k = r as u8;
            p.rect_filled(rect, CornerRadius { nw: k, ne: k, sw: k / 2, se: k / 2 }, shoulder);
            label(s, Color32::WHITE);
        }
        Chip::ShoulderIcon(which) => {
            let rect = Rect::from_center_size(c, Vec2::new(2.6 * r, 2.0 * r));
            let k = r as u8;
            p.rect_filled(rect, CornerRadius { nw: k, ne: k, sw: k / 2, se: k / 2 }, shoulder);
            paint_icon(p, Rect::from_center_size(c, Vec2::splat(1.1 * r)), which, Color32::WHITE);
        }
        Chip::Stick(s) => {
            p.circle_filled(c, r, shoulder);
            p.circle_stroke(c, 0.8 * r, Stroke::new(0.3 * r, Color32::from_rgb(0x26, 0x24, 0x4a)));
            label(&s.to_string(), Color32::WHITE);
        }
        Chip::DPad(d) => {
            p.circle_filled(c, 1.1 * r, bg(0.85));
            paint_dpad(p, c, r, d);
        }
        Chip::Key(k) => paint_key_glyph(p, c, 2.0 * r, k),
        Chip::Mouse(right) => paint_mouse(p, c, r, FG, right),
        Chip::MouseMiddle => paint_mouse_middle(p, c, r, FG),
        Chip::Text(s) => label(s, DIM),
    }
}

// A key's glyph, `h` high, centred on `c`.
pub fn paint_key_glyph(p: &egui::Painter, c: Pos2, h: f32, k: &str) {
    let g = p.layout_no_wrap(k.to_string(), mono(0.62 * h), FG);
    let rect = Rect::from_center_size(c, Vec2::new((g.size().x + 0.5 * h).max(h), h));
    p.rect_filled(rect, 3, bg(0.9));
    p.rect_stroke(rect, 3, Stroke::new(1.0, KBD_BORDER), StrokeKind::Inside);
    p.galley(Align2::CENTER_CENTER.anchor_size(c, g.size()).min, g, FG);
}

// A face button's fill and its letter's colour.
fn face_colours(b: char) -> (Color32, Color32) {
    match b {
        'A' => (Color32::from_rgb(0x3f, 0x9c, 0x35), Color32::WHITE),
        'B' => (Color32::from_rgb(0xc8, 0x35, 0x2e), Color32::WHITE),
        'X' => (Color32::from_rgb(0x2c, 0x6f, 0xd1), Color32::WHITE),
        _ => (Color32::from_rgb(0xc9, 0xa2, 0x27), Color32::from_rgb(0x22, 0x22, 0x22)),
    }
}

// The spells' colours: fire, lightning, wind (cyan), healing (green), and the rest.
pub const SPELL_COLOURS: [Color32; crate::game::spells::N_SPELLS] = [
    Color32::from_rgb(0xff, 0x8a, 0x3a),
    Color32::from_rgb(0xff, 0xe2, 0x6a),
    Color32::from_rgb(0x6a, 0xe8, 0xff),
    Color32::from_rgb(0x6a, 0xff, 0x8a),
    // frost lance, arcane missiles, meteor, gravity well, spirit familiar
    Color32::from_rgb(0xb0, 0xe8, 0xff),
    Color32::from_rgb(0xc8, 0x8a, 0xff),
    Color32::from_rgb(0xff, 0x6a, 0x30),
    Color32::from_rgb(0x9a, 0x7a, 0xff),
    Color32::from_rgb(0xd8, 0xf0, 0xff),
];

// Spell `i`'s glyph in a circle of radius `r` round `c`: a flame, a
// lightning bolt, gusts of wind, a cross.
pub fn paint_spell(p: &egui::Painter, c: Pos2, r: f32, i: usize, colour: Color32) {
    let v = |x: f32, y: f32| c + Vec2::new(x, y) * r;
    match i {
        0 => {
            // a flame: a round base drawn up to a point, a brighter one inside
            let flame = |s: f32, col: Color32| {
                let base = v(0.0, 0.26 * s + 0.08);
                let rad = 0.5 * s * r;
                p.circle_filled(base, rad, col);
                let tip = v(0.1 * s, -0.8 * s + 0.08);
                // (the tangents from the tip to the base)
                let d = base - tip;
                let a = (rad / d.length()).asin();
                let dir = d.normalized();
                let side = |t: f32| {
                    let rot = Vec2::new(dir.x * t.cos() - dir.y * t.sin(), dir.x * t.sin() + dir.y * t.cos());
                    tip + rot * (d.length() * a.cos())
                };
                p.add(Shape::convex_polygon(vec![tip, side(a), side(-a)], col, Stroke::NONE));
            };
            flame(1.0, colour);
            flame(0.5, Color32::from_rgb(0xff, 0xe8, 0xa0).gamma_multiply(colour.a() as f32 / 255.0));
        }
        1 => {
            // a bolt: a zigzag, thick
            let pts = vec![v(0.22, -0.66), v(-0.18, 0.02), v(0.14, 0.02), v(-0.2, 0.66)];
            p.add(Shape::line(pts, Stroke::new(0.17 * r, colour)));
            p.circle_filled(v(-0.2, 0.66), 0.085 * r, colour);
        }
        2 => {
            // gusts: three swept lines, curling at their ends
            for (k, (y, len)) in [(-0.34, 1.0), (0.0, 0.8), (0.34, 0.6)].into_iter().enumerate() {
                let n = 12;
                let x0 = -0.55 + 0.1 * k as f32;
                let mut pts: Vec<Pos2> = (0..=n)
                    .map(|j| {
                        let t = j as f32 / n as f32;
                        v(x0 + t * len, y + 0.06 * (t * std::f32::consts::PI).sin())
                    })
                    .collect();
                // (a curl back at the end)
                let end = x0 + len;
                for j in 1..=6 {
                    let a = j as f32 / 6.0 * 1.4 * std::f32::consts::PI;
                    pts.push(v(end + 0.12 * a.sin(), y - 0.12 + 0.12 * a.cos()));
                }
                p.add(Shape::line(pts, Stroke::new(0.1 * r, colour)));
            }
        }
        4 => {
            // an ice shard: a long diamond, and a chip beside it
            p.add(Shape::convex_polygon(vec![v(0.45, -0.7), v(0.15, -0.05), v(-0.45, 0.7), v(-0.15, 0.05)], colour, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![v(-0.35, -0.45), v(-0.2, -0.3), v(-0.35, -0.15), v(-0.5, -0.3)], colour, Stroke::NONE));
        }
        5 => {
            // sparkles: three four-pointed stars
            for (x, y, s) in [(-0.35, 0.25, 0.32), (0.3, 0.3, 0.26), (0.0, -0.35, 0.4)] {
                let c2 = v(x, y);
                let (o, i) = (s * r, 0.3 * s * r);
                let pts: Vec<Pos2> = (0..8)
                    .map(|k| {
                        let a = k as f32 * std::f32::consts::FRAC_PI_4;
                        let rr = if k % 2 == 0 { o } else { i };
                        c2 + Vec2::new(a.sin(), -a.cos()) * rr
                    })
                    .collect();
                for k in 0..8 {
                    p.add(Shape::convex_polygon(vec![c2, pts[k], pts[(k + 1) % 8]], colour, Stroke::NONE));
                }
            }
        }
        6 => {
            // a meteor: a rock, its trail of fire streaming up behind
            for k in 0..3 {
                let o = (k as f32 - 1.0) * 0.18;
                p.line_segment([v(0.15 + o, 0.15 - o), v(0.75 + o, -0.6 - o)], Stroke::new(0.12 * r, colour.gamma_multiply(0.6)));
            }
            p.circle_filled(v(-0.15, 0.3), 0.38 * r, colour);
        }
        7 => {
            // a spiral, inwards
            let pts: Vec<Pos2> = (0..=40)
                .map(|k| {
                    let t = k as f32 / 40.0;
                    let a = t * 3.0 * std::f32::consts::TAU;
                    c + Vec2::new(a.cos(), a.sin()) * (0.75 * (1.0 - t) + 0.05) * r
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(0.11 * r, colour)));
        }
        8 => {
            // a familiar: a round spirit, wings either side, two eyes
            p.circle_filled(c, 0.38 * r, colour);
            for s in [-1.0f32, 1.0] {
                p.add(Shape::convex_polygon(vec![v(0.3 * s, -0.05), v(0.85 * s, -0.5), v(0.7 * s, 0.1)], colour, Stroke::NONE));
                p.circle_filled(v(0.13 * s, -0.05), 0.07 * r, bg(1.0));
            }
        }
        _ => {
            // a cross, rounded
            let w = 0.3 * r;
            let l = 1.25 * r;
            p.rect_filled(Rect::from_center_size(c, Vec2::new(w, l)), (0.3 * w) as u8, colour);
            p.rect_filled(Rect::from_center_size(c, Vec2::new(l, w)), (0.3 * w) as u8, colour);
        }
    }
}

// A mouse, `r` half its height, round c: its outline, its left (or right)
// button filled (a click).
pub fn paint_mouse(p: &egui::Painter, c: Pos2, r: f32, colour: Color32, right: bool) {
    let body = Rect::from_center_size(c, Vec2::new(1.3 * r, 2.0 * r));
    let k = (0.62 * r) as u8;
    p.rect_stroke(body, k, Stroke::new(1.3, colour), StrokeKind::Inside);
    let button = if right {
        Rect::from_min_max(Pos2::new(body.center().x, body.top()), Pos2::new(body.right(), body.top() + 0.8 * r))
    } else {
        Rect::from_min_max(body.min, Pos2::new(body.center().x, body.top() + 0.8 * r))
    };
    let corner = if right { CornerRadius { nw: 0, ne: k, sw: 0, se: 0 } } else { CornerRadius { nw: k, ne: 0, sw: 0, se: 0 } };
    p.rect_filled(button.shrink(1.2), corner, colour);
    p.line_segment([Pos2::new(body.center().x, body.top()), Pos2::new(body.center().x, body.top() + 0.8 * r)], Stroke::new(1.0, colour));
    p.line_segment([Pos2::new(body.left(), body.top() + 0.8 * r), Pos2::new(body.right(), body.top() + 0.8 * r)], Stroke::new(1.0, colour));
}

// An outlined button (as plain_button) with a glyph before its label.
pub fn glyph_button(ui: &mut Ui, glyph: Chip, label: &str, size: f32) -> Response {
    let inner = Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(4).inner_margin(Margin::symmetric(14, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            chip(ui, glyph);
            ui.label(text(label, size, FG));
        });
    });
    let r = ui.interact(inner.response.rect, ui.next_auto_id(), Sense::click());
    if r.hovered() {
        ui.painter().rect_stroke(inner.response.rect, 4, Stroke::new(1.0, DIM), StrokeKind::Inside);
    }
    r
}

// A padlock, `r` its half height, round c.
pub fn paint_lock(p: &egui::Painter, c: Pos2, r: f32, colour: Color32) {
    let body = Rect::from_center_size(c + Vec2::new(0.0, 0.3 * r), Vec2::new(1.4 * r, 1.1 * r));
    p.rect_filled(body, 2, colour);
    let n = 12;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let a = std::f32::consts::PI * k as f32 / n as f32;
            c + Vec2::new(-0.45 * r * a.cos(), -0.25 * r - 0.5 * r * a.sin())
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(0.22 * r, colour)));
}

// Skill `id`'s logo, `r` its half size, round c, in `colour` (see
// skills.rs): a spell's glyph for the spells and what improves them, ringed
// for a spell it unlocks; else its own (a heart, a drop, waves, a burst,
// a skull, a soul).
// A ring of radius r round c, `frac` (0..1) of it from the top, clockwise
// (or back, anticlockwise).
pub fn ring_arc(p: &egui::Painter, c: Pos2, r: f32, frac: f32, back: bool, stroke: Stroke) {
    let n = (64.0 * frac).ceil().max(1.0) as usize;
    let s = if back { -1.0 } else { 1.0 };
    let pts = (0..=n)
        .map(|k| {
            let a = s * std::f32::consts::TAU * frac * k as f32 / n as f32;
            c + Vec2::new(a.sin(), -a.cos()) * r
        })
        .collect();
    p.add(Shape::line(pts, stroke));
}

// The dash's meter (left of the vitals): a ring filling as it comes ready
// (`ready`, 0..1), two chevrons in it, lit once it is.
pub fn dash_meter(ui: &mut Ui, ready: f32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let r = 0.5 * size - 2.0;
    let on = ready >= 1.0;
    let lit = Color32::from_rgb(0x9a, 0xe6, 0xff);
    p.circle_filled(c, r, bg(0.55));
    p.circle_stroke(c, r, Stroke::new(1.0, BORDER));
    if !on {
        ring_arc(p, c, r, ready, false, Stroke::new(2.5, lit.gamma_multiply(0.6)));
    } else {
        p.circle_stroke(c, r, Stroke::new(2.0, lit));
    }
    let ink = if on { lit } else { DIM };
    for dx in [-0.22, 0.18] {
        let o = c + Vec2::new(dx * r, 0.0);
        p.line_segment([o + Vec2::new(-0.18 * r, -0.32 * r), o + Vec2::new(0.14 * r, 0.0)], Stroke::new(2.0, ink));
        p.line_segment([o + Vec2::new(0.14 * r, 0.0), o + Vec2::new(-0.18 * r, 0.32 * r)], Stroke::new(2.0, ink));
    }
    resp
}

pub fn paint_skill(p: &egui::Painter, c: Pos2, r: f32, id: &str, colour: Color32) {
    let icon = |which: Icon| paint_icon(p, Rect::from_center_size(c, Vec2::splat(2.0 * r)), which, colour);
    let spell = |i: usize| paint_spell(p, c, r, i, colour);
    let ring = || p.circle_stroke(c, 1.15 * r, Stroke::new(1.2, colour));
    match id {
        "soul" => icon(Icon::Soul),
        "vitality" => icon(Icon::Heart),
        "reaper" => icon(Icon::Skull),
        "chrono" => {
            // a clock, its hands, and an arrow round it going back
            p.circle_stroke(c, 0.62 * r, Stroke::new(1.4, colour));
            p.line_segment([c, c + Vec2::new(0.0, -0.45 * r)], Stroke::new(1.4, colour));
            p.line_segment([c, c + Vec2::new(-0.3 * r, 0.12 * r)], Stroke::new(1.4, colour));
            ring_arc(p, c, 0.95 * r, 0.7, true, Stroke::new(1.4, colour));
            let a = -0.7 * std::f32::consts::TAU;
            let tip = c + Vec2::new(a.sin(), -a.cos()) * 0.95 * r;
            p.add(Shape::convex_polygon(vec![tip + Vec2::new(-0.22 * r, -0.05 * r), tip + Vec2::new(0.12 * r, -0.2 * r), tip + Vec2::new(0.05 * r, 0.18 * r)], colour, Stroke::NONE));
        }
        "healing" => {
            spell(3);
            ring();
        }
        "mending" => {
            spell(3);
            // (and an arrow up beside it: more)
            let a = c + Vec2::new(0.75 * r, -0.2 * r);
            p.add(Shape::convex_polygon(vec![a + Vec2::new(0.0, -0.35 * r), a + Vec2::new(0.25 * r, 0.0), a + Vec2::new(-0.25 * r, 0.0)], colour, Stroke::NONE));
        }
        "lightning" => {
            spell(1);
            ring();
        }
        "static" => {
            spell(1);
            // (sparks round it)
            for k in 0..4 {
                let a = std::f32::consts::FRAC_PI_2 * k as f32 + 0.6;
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([c + d * 0.75 * r, c + d * r], Stroke::new(1.2, colour));
            }
        }
        "wind" => {
            spell(2);
            ring();
        }
        "cool" => {
            spell(2);
            // (a snowflake's six arms, small, beside it)
            let s = c + Vec2::new(0.7 * r, 0.6 * r);
            for k in 0..3 {
                let a = std::f32::consts::PI / 3.0 * k as f32;
                let d = Vec2::new(a.cos(), a.sin()) * 0.3 * r;
                p.line_segment([s - d, s + d], Stroke::new(1.0, colour));
            }
        }
        "kindling" => spell(0),
        "frost" | "arcane" | "meteor" | "gravity" | "familiar" => {
            spell(match id {
                "frost" => 4,
                "arcane" => 5,
                "meteor" => 6,
                "gravity" => 7,
                _ => 8,
            });
            ring();
        }
        "inferno" => {
            // a burst: rays out from a ball
            p.circle_filled(c, 0.35 * r, colour);
            for k in 0..8 {
                let a = std::f32::consts::FRAC_PI_4 * k as f32;
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([c + d * 0.5 * r, c + d * if k % 2 == 0 { r } else { 0.8 * r }], Stroke::new(1.6, colour));
            }
        }
        "well" => {
            // a drop
            let base = c + Vec2::new(0.0, 0.25 * r);
            p.circle_filled(base, 0.5 * r, colour);
            p.add(Shape::convex_polygon(vec![c + Vec2::new(0.0, -0.85 * r), base + Vec2::new(0.47 * r, -0.15 * r), base + Vec2::new(-0.47 * r, -0.15 * r)], colour, Stroke::NONE));
        }
        "flow" => {
            // waves
            for k in 0..3 {
                let y = (k as f32 - 1.0) * 0.45 * r;
                let pts: Vec<Pos2> = (0..=12)
                    .map(|j| {
                        let t = j as f32 / 12.0;
                        c + Vec2::new((t - 0.5) * 1.8 * r, y + 0.15 * r * (t * std::f32::consts::TAU).sin())
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.6, colour)));
            }
        }
        "slot2" | "slot3" | "slot4" => {
            // a spell slot: a ring, and as many dots as slots it brings
            let n = id[4..].parse::<usize>().unwrap_or(2);
            p.circle_stroke(c, 0.55 * r, Stroke::new(1.6, colour));
            for k in 0..n {
                let a = std::f32::consts::TAU * k as f32 / n as f32;
                p.circle_filled(c + Vec2::new(a.sin(), -a.cos()) * 0.85 * r, 0.14 * r, colour);
            }
            p.circle_filled(c, 0.18 * r, colour);
        }
        // (a spell's upgrade: its glyph, and a mark of what it does)
        "glacier" | "singularity" | "volley" | "fork" | "kindred" | "starfall" => {
            let (i, mark) = match id {
                "glacier" => (4, 0),
                "singularity" => (7, 1),
                "volley" => (5, 2),
                "fork" => (1, 3),
                "kindred" => (8, 1),
                _ => (6, 2),
            };
            spell(i);
            let m = c + Vec2::new(0.78 * r, -0.62 * r);
            match mark {
                // (frost: a flake)
                0 => {
                    for k in 0..3 {
                        let a = std::f32::consts::PI * k as f32 / 3.0;
                        let d = Vec2::new(a.cos(), a.sin()) * 0.3 * r;
                        p.line_segment([m - d, m + d], Stroke::new(1.2, colour));
                    }
                }
                // (more: a ring round it)
                1 => {
                    ring();
                }
                // (more of them: a plus)
                2 => {
                    p.line_segment([m - Vec2::new(0.28 * r, 0.0), m + Vec2::new(0.28 * r, 0.0)], Stroke::new(1.6, colour));
                    p.line_segment([m - Vec2::new(0.0, 0.28 * r), m + Vec2::new(0.0, 0.28 * r)], Stroke::new(1.6, colour));
                }
                // (a fork)
                _ => {
                    p.line_segment([m + Vec2::new(0.0, 0.3 * r), m], Stroke::new(1.3, colour));
                    p.line_segment([m, m + Vec2::new(-0.25 * r, -0.3 * r)], Stroke::new(1.3, colour));
                    p.line_segment([m, m + Vec2::new(0.25 * r, -0.3 * r)], Stroke::new(1.3, colour));
                }
            }
        }
        "swift" => {
            // two chevrons, the dash's
            for dx in [-0.3, 0.25] {
                let o = c + Vec2::new(dx * r, 0.0);
                p.line_segment([o + Vec2::new(-0.25 * r, -0.45 * r), o + Vec2::new(0.2 * r, 0.0)], Stroke::new(1.8, colour));
                p.line_segment([o + Vec2::new(0.2 * r, 0.0), o + Vec2::new(-0.25 * r, 0.45 * r)], Stroke::new(1.8, colour));
            }
        }
        "aegis" => {
            // a shield
            let pts = vec![c + Vec2::new(-0.55 * r, -0.55 * r), c + Vec2::new(0.55 * r, -0.55 * r), c + Vec2::new(0.5 * r, 0.1 * r), c + Vec2::new(0.0, 0.7 * r), c + Vec2::new(-0.5 * r, 0.1 * r)];
            p.add(Shape::closed_line(pts, Stroke::new(1.6, colour)));
            p.line_segment([c + Vec2::new(0.0, -0.4 * r), c + Vec2::new(0.0, 0.45 * r)], Stroke::new(1.2, colour));
        }
        _ => {
            p.circle_stroke(c, 0.5 * r, Stroke::new(1.5, colour));
        }
    }
}

// A mouse, its wheel lit (a middle click).
pub fn paint_mouse_middle(p: &egui::Painter, c: Pos2, r: f32, colour: Color32) {
    let body = Rect::from_center_size(c, Vec2::new(1.3 * r, 2.0 * r));
    p.rect_stroke(body, (0.62 * r) as u8, Stroke::new(1.3, colour), StrokeKind::Inside);
    p.line_segment([Pos2::new(body.left(), body.top() + 0.8 * r), Pos2::new(body.right(), body.top() + 0.8 * r)], Stroke::new(1.0, colour));
    p.rect_filled(Rect::from_center_size(Pos2::new(c.x, body.top() + 0.42 * r), Vec2::new(0.3 * r, 0.6 * r)), 1, colour);
}
