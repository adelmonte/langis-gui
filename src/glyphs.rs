//! A few symbols Desktop draws from its own symbol font, drawn here with
//! shapes instead: a pin, a poll and the GIF tag.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Shape, Stroke, Ui, Vec2};

/// A push pin, tilted, filling `rect`.
pub fn pin(ui: &Ui, rect: Rect, color: Color32) {
    let s = rect.width() / 18.;
    let at = |x: f32, y: f32| {
        // Rotate 45 degrees around the center.
        let (cx, cy) = (x - 9., y - 9.);
        let a = std::f32::consts::FRAC_PI_4;
        rect.center() + Vec2::new(cx * a.cos() - cy * a.sin(), cx * a.sin() + cy * a.cos()) * s
    };
    let head = vec![at(6., 2.), at(12., 2.), at(11., 4.), at(11., 8.), at(14., 11.), at(4., 11.), at(7., 8.), at(7., 4.)];
    ui.painter().add(Shape::convex_polygon(head[..4].iter().chain(head[6..].iter()).copied().collect(), color, Stroke::NONE));
    ui.painter().add(Shape::convex_polygon(vec![at(7., 8.), at(11., 8.), at(14., 11.), at(4., 11.)], color, Stroke::NONE));
    ui.painter().line_segment([at(9., 11.), at(9., 17.)], Stroke::new(1.6 * s, color));
}

/// Three bars of different lengths, like a poll's results.
pub fn poll(ui: &Ui, rect: Rect, color: Color32) {
    let s = rect.width() / 18.;
    let stroke = Stroke::new(2. * s, color);
    let left = rect.left() + 3. * s;
    for (i, len) in [10., 6., 12.].into_iter().enumerate() {
        let y = rect.top() + (4. + i as f32 * 5.) * s;
        ui.painter().line_segment([Pos2::new(left, y), Pos2::new(left + len * s, y)], stroke);
    }
}

/// "GIF" in a rounded box.
pub fn gif(ui: &Ui, rect: Rect, color: Color32) {
    let s = rect.width() / 18.;
    let r = Rect::from_center_size(rect.center(), Vec2::new(16. * s, 11. * s));
    ui.painter().rect_stroke(r, CornerRadius::same((2. * s) as u8), Stroke::new(1.4 * s, color), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, "GIF", crate::theme::semibold(6.5 * s), color);
}
