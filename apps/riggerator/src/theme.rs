//! Maya-like dark theme.

use egui::{Color32, CornerRadius, Stroke, Visuals};

pub const PANEL: Color32 = Color32::from_rgb(68, 68, 68);
pub const FIELD: Color32 = Color32::from_rgb(43, 43, 43);
pub const WIDGET: Color32 = Color32::from_rgb(93, 93, 93);
pub const HOVER: Color32 = Color32::from_rgb(110, 110, 110);
pub const ACCENT: Color32 = Color32::from_rgb(82, 133, 166);
pub const TEXT: Color32 = Color32::from_rgb(200, 200, 200);
pub const KEYED: Color32 = Color32::from_rgb(205, 39, 41);
pub const ANIMATED: Color32 = Color32::from_rgb(221, 114, 122);
pub const CONSTRAINED: Color32 = Color32::from_rgb(140, 180, 255);
pub const DRIVEN: Color32 = Color32::from_rgb(95, 156, 230);
pub const LOCKED: Color32 = Color32::from_rgb(92, 104, 116);
pub const SEL_GREEN: [f32; 4] = [0.26, 1.0, 0.64, 1.0];
pub const LEAD_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const WIRE_DEFAULT: [f32; 4] = [0.0, 0.016, 0.376, 1.0];

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(60, 60, 60);
    v.extreme_bg_color = FIELD;
    v.faint_bg_color = Color32::from_rgb(74, 74, 74);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = Color32::from_rgb(120, 180, 230);
    let r = CornerRadius::same(2);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(50, 50, 50));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = WIDGET;
    v.widgets.inactive.weak_bg_fill = WIDGET;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.corner_radius = r;
    v.widgets.open.bg_fill = Color32::from_rgb(82, 82, 82);
    v.widgets.open.corner_radius = r;
    v.window_corner_radius = CornerRadius::same(3);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(5.0, 3.0);
        s.spacing.button_padding = egui::vec2(5.0, 2.0);
        s.spacing.interact_size.y = 18.0;
        s.spacing.window_margin = egui::Margin::same(6);
    });
}

pub fn c32(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8)
}
