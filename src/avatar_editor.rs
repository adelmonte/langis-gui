//! Desktop's `AvatarEditor`: the photo large, Upload photo, Text and
//! Remove, and its preset avatars (initials on each of the avatar colors);
//! `AvatarTextEditor` for up to three letters on a color. Text avatars are
//! drawn here into a 512 pixel PNG, as Desktop renders them to a canvas.

use std::sync::Arc;

use egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use langis_core::profiles::AvatarChange;

use crate::panes::{buttons, modal, title};
use crate::theme::{self, Palette};
use crate::widgets;
use crate::i18n::tr;

/// Desktop's default avatars (`PersonalAvatarIcons`, `GroupAvatarIcons`):
/// each icon with its avatar color (A100 is 0).
const PERSONAL: [(&str, &[u8], usize); 12] = [
    ("abstract_01", include_bytes!("../assets/avatars/avatar_abstract_01.svg"), 3),
    ("abstract_02", include_bytes!("../assets/avatars/avatar_abstract_02.svg"), 2),
    ("abstract_03", include_bytes!("../assets/avatars/avatar_abstract_03.svg"), 7),
    ("cat", include_bytes!("../assets/avatars/avatar_cat.svg"), 9),
    ("dog", include_bytes!("../assets/avatars/avatar_dog.svg"), 4),
    ("fox", include_bytes!("../assets/avatars/avatar_fox.svg"), 9),
    ("tucan", include_bytes!("../assets/avatars/avatar_tucan.svg"), 2),
    ("pig", include_bytes!("../assets/avatars/avatar_pig.svg"), 6),
    ("dinosour", include_bytes!("../assets/avatars/avatar_dinosour.svg"), 3),
    ("sloth", include_bytes!("../assets/avatars/avatar_sloth.svg"), 8),
    ("incognito", include_bytes!("../assets/avatars/avatar_incognito.svg"), 11),
    ("ghost", include_bytes!("../assets/avatars/avatar_ghost.svg"), 0),
];

const GROUP: [(&str, &[u8], usize); 12] = [
    ("balloon", include_bytes!("../assets/avatars/avatar_balloon.svg"), 8),
    ("book", include_bytes!("../assets/avatars/avatar_book.svg"), 2),
    ("briefcase", include_bytes!("../assets/avatars/avatar_briefcase.svg"), 1),
    ("celebration", include_bytes!("../assets/avatars/avatar_celebration.svg"), 7),
    ("drink", include_bytes!("../assets/avatars/avatar_drink.svg"), 0),
    ("football", include_bytes!("../assets/avatars/avatar_football.svg"), 11),
    ("heart", include_bytes!("../assets/avatars/avatar_heart.svg"), 0),
    ("house", include_bytes!("../assets/avatars/avatar_house.svg"), 8),
    ("melon", include_bytes!("../assets/avatars/avatar_melon.svg"), 2),
    ("soccerball", include_bytes!("../assets/avatars/avatar_soccerball.svg"), 1),
    ("sunset", include_bytes!("../assets/avatars/avatar_sunset.svg"), 3),
    ("surfboard", include_bytes!("../assets/avatars/avatar_surfboard.svg"), 11),
];

/// Desktop's `AvatarTextEditor` limit, in characters.
const TEXT_MAX: usize = 3;

pub struct AvatarEditor {
    /// What the preview shows now: a photo's bytes (picked or drawn), or
    /// none (initials).
    picked: Option<Arc<Vec<u8>>>,
    /// The photo there was when opened, shown until something's chosen.
    current: Option<String>,
    removed: bool,
    /// Editing text: the letters and the color.
    text: Option<(String, usize)>,
    initials: String,
    name: String,
    /// A group's editor offers the group icons.
    group: bool,
}

impl AvatarEditor {
    pub fn new(current: Option<String>, name: &str, group: bool) -> Self {
        let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
        Self { picked: None, current, removed: false, text: None, initials, name: name.to_owned(), group }
    }

    /// The modal. Some(change) on Save, Some(Keep) on Cancel.
    pub fn show(&mut self, ui: &Ui, toast: &mut Option<String>) -> Option<AvatarChange> {
        let palette = Palette::of(ui.ctx());
        let mut out = None;
        let closed = modal(ui, "avatar-editor", 360., |ui| {
            if let Some((text, color)) = self.text.as_mut() {
                title(ui, tr("Text"));
                // AvatarTextEditor: the letters large on the color.
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.), Sense::hover());
                    let (bg, fg) = theme::avatar_colors(*color as u8);
                    ui.painter().circle_filled(rect.center(), 48., bg);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text.as_str(), theme::regular(text_size(text) * 96. / 512.), fg);
                });
                ui.add_space(12.);
                let field = ui.add(egui::TextEdit::singleline(text).hint_text("Aa").char_limit(TEXT_MAX).align(egui::Align2::CENTER_TOP).desired_width(f32::INFINITY));
                field.request_focus();
                ui.add_space(12.);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(8.);
                    for i in 0..theme::AVATAR_COLORS.len() {
                        let (r, hit) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                        let (bg, fg) = theme::avatar_colors(i as u8);
                        ui.painter().circle_filled(r.center(), 16., bg);
                        ui.painter().circle_filled(r.center(), 6., fg);
                        if i == *color {
                            ui.painter().circle_stroke(r.center(), 17.5, egui::Stroke::new(2., palette.text));
                        }
                        if hit.clicked() {
                            *color = i;
                        }
                    }
                });
                match buttons(ui, &[tr("Done"), tr("Cancel")]) {
                    Some(0) if !text.trim().is_empty() => {
                        self.picked = Some(Arc::new(text_avatar(text.trim(), *color)));
                        self.removed = false;
                        self.text = None;
                    }
                    Some(1) => self.text = None,
                    _ => {}
                }
                return;
            }
            title(ui, tr("Edit photo"));
            // The preview, 96px.
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.), Sense::hover());
                match (&self.picked, &self.current) {
                    (Some(bytes), _) => {
                        egui::Image::from_bytes(format!("bytes://avatar-editor/{}", bytes.len()), bytes.to_vec()).corner_radius(CornerRadius::same(48)).paint_at(ui, rect);
                    }
                    (None, Some(path)) if !self.removed => {
                        egui::Image::new(format!("file://{path}")).corner_radius(CornerRadius::same(48)).paint_at(ui, rect);
                    }
                    _ => widgets::paint_avatar(ui, rect, &widgets::AvatarSpec::person(&self.name, 0)),
                }
            });
            ui.add_space(12.);
            ui.horizontal(|ui| {
                let has = self.picked.is_some() || (self.current.is_some() && !self.removed);
                let n = if has { 3. } else { 2. };
                ui.add_space(((ui.available_width() - n * 84.) / 2.).max(0.));
                if tool(ui, &palette, "photo", "Upload photo")
                    && let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file()
                {
                    match std::fs::read(&path).ok().and_then(|b| crate::profile::square_photo(&b)) {
                        Some(bytes) => {
                            self.picked = Some(Arc::new(bytes));
                            self.removed = false;
                        }
                        None => *toast = Some("That picture couldn't be read.".into()),
                    }
                }
                if tool(ui, &palette, "text-square", tr("Text")) {
                    self.text = Some((self.initials.clone(), 0));
                }
                if has && tool(ui, &palette, "trash", tr("Remove")) {
                    self.picked = None;
                    self.removed = true;
                }
            });
            ui.add_space(16.);
            // Desktop's default avatars: an icon on its color.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(10.);
                for (name, svg, color) in if self.group { GROUP } else { PERSONAL } {
                    let (r, hit) = ui.allocate_exact_size(Vec2::splat(48.), Sense::click());
                    let (bg, _) = theme::avatar_colors(color as u8);
                    ui.painter().circle_filled(r.center(), 24., bg);
                    egui::Image::from_bytes(format!("bytes://avatar_{name}.svg"), svg).paint_at(ui, r);
                    if hit.hovered() {
                        ui.painter().circle_stroke(r.center(), 25.5, egui::Stroke::new(2., palette.text_secondary));
                    }
                    if hit.clicked()
                        && let Some(png) = icon_avatar(svg, color)
                    {
                        self.picked = Some(Arc::new(png));
                        self.removed = false;
                    }
                }
            });
            match buttons(ui, &[tr("Save"), tr("Cancel")]) {
                Some(0) => {
                    out = Some(match (&self.picked, self.removed) {
                        (Some(bytes), _) => AvatarChange::New(bytes.clone()),
                        (None, true) => AvatarChange::Remove,
                        (None, false) => AvatarChange::Keep,
                    })
                }
                Some(_) => out = Some(AvatarChange::Keep),
                None => {}
            }
        });
        if closed && out.is_none() {
            out = Some(AvatarChange::Keep);
        }
        out
    }
}

/// A tool button: a round icon over its label.
fn tool(ui: &mut Ui, palette: &Palette, symbol: &str, label: &str) -> bool {
    let (rect, hit) = ui.allocate_exact_size(Vec2::new(84., 64.), Sense::click());
    let circle = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 20.), Vec2::splat(40.));
    ui.painter().circle_filled(circle.center(), 20., if hit.hovered() { palette.fill_pressed } else { palette.fill_secondary });
    theme::paint_symbol(ui.painter(), symbol, circle.center(), 20., palette.text);
    ui.painter().text(Pos2::new(rect.center().x, rect.bottom() - 8.), egui::Align2::CENTER_CENTER, label, theme::regular(12.), palette.text);
    hit.clicked()
}

/// Desktop's text avatar sizes: smaller for more letters.
fn text_size(text: &str) -> f32 {
    match text.chars().count() {
        0 | 1 => 230.,
        2 => 200.,
        _ => 150.,
    }
}

/// A default avatar's icon on its color, 512 pixels square, as a PNG
/// (Desktop's `avatarDataToBytes`).
pub fn icon_avatar(svg: &[u8], color: usize) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(512, 512)?;
    let (bg, _) = theme::AVATAR_COLORS[color % theme::AVATAR_COLORS.len()];
    pixmap.fill(tiny_skia::Color::from_rgba8((bg >> 16) as u8, (bg >> 8) as u8, bg as u8, 255));
    let scale = 512. / tree.size().width().max(tree.size().height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// Up to three letters on an avatar color, 512 pixels square, as a PNG.
pub fn text_avatar(text: &str, color: usize) -> Vec<u8> {
    use ab_glyph::{Font, FontRef, ScaleFont};
    let (bg, fg) = theme::AVATAR_COLORS[color % theme::AVATAR_COLORS.len()];
    let px = |c: u32| image::Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
    let mut out = image::RgbaImage::from_pixel(512, 512, px(bg));
    if let Ok(font) = FontRef::try_from_slice(include_bytes!("../assets/fonts/Inter-Regular.ttf")) {
        let scaled = font.as_scaled(text_size(text));
        let mut glyphs = Vec::new();
        let mut x = 0f32;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            glyphs.push(id.with_scale_and_position(scaled.scale(), ab_glyph::point(x, scaled.ascent())));
            x += scaled.h_advance(id);
        }
        let (left, top) = (256. - x / 2., 256. - (scaled.ascent() - scaled.descent()) / 2.);
        let ink = px(fg);
        for g in glyphs {
            let Some(outlined) = font.outline_glyph(g) else { continue };
            let b = outlined.px_bounds();
            outlined.draw(|gx, gy, cover| {
                let (x, y) = ((left + b.min.x) as i64 + gx as i64, (top + b.min.y) as i64 + gy as i64);
                if (0..512).contains(&x) && (0..512).contains(&y) {
                    let p = out.get_pixel_mut(x as u32, y as u32);
                    for k in 0..3 {
                        p.0[k] = (p.0[k] as f32 * (1. - cover) + ink.0[k] as f32 * cover) as u8;
                    }
                }
            });
        }
    }
    let mut png = std::io::Cursor::new(Vec::new());
    let _ = out.write_to(&mut png, image::ImageFormat::Png);
    png.into_inner()
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_avatars_render_on_their_color() {
        for (_, svg, color) in super::PERSONAL.iter().chain(super::GROUP.iter()) {
            let png = super::icon_avatar(svg, *color).expect("renders");
            let img = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (512, 512));
            let corner = *img.get_pixel(2, 2);
            assert!(img.pixels().filter(|p| **p != corner).count() > 5_000, "the icon is drawn");
        }
    }

    #[test]
    fn text_avatars_are_square_pngs_with_ink() {
        let png = super::text_avatar("AB", 3);
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (512, 512));
        let corner = *img.get_pixel(5, 5);
        assert!(img.pixels().filter(|p| **p != corner).count() > 5_000, "the letters are drawn");
    }
}
