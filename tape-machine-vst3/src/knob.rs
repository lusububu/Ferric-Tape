// A hand-painted knob widget matching the web app's SVG knob exactly: same arc geometry
// (A0=135deg start offset, 270deg sweep), same track/disc/pointer colors per "tone", same
// orange (#ff5519) value arc. See buildKnob() in page.template.html for the source this mirrors.

use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x55, 0x19); // --c2
pub const INK: Color32 = Color32::from_rgb(0x14, 0x14, 0x14); // --ink
pub const PAPER: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xe1); // --paper

// The web app's own rem-derived point sizes (knob at 46px, labels at 10-11px, panel padding at
// 14px) are genuinely small in CSS terms and read as too small once rendered 1:1 in points here,
// most likely because the reference screenshots were captured at a higher effective zoom/DPI than
// that. UI_SCALE is a single multiplier applied everywhere a size, margin or spacing is set, in
// both this file and editor.rs, so every element grows together and stays in the same proportion
// to each other that the CSS specifies. It replaced an earlier attempt that used egui's built-in
// zoom_factor instead: that scales rendering only, not the layout math, so it silently shrank the
// available layout space for a fixed window size and caused panel backgrounds to stop covering
// their panels. This is a reasoned starting value, not one checked against a render, so it may
// still need to move up or down once you see it.
pub const UI_SCALE: f32 = 1.6;

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Dark,  // knob sits on a dark panel: paper disc, ink pointer
    Light, // knob sits on a light panel: ink disc, paper pointer
}

const A0: f32 = 135.0; // start angle, degrees, matching the web knob's A0
const SWEEP: f32 = 270.0; // total sweep, degrees
const KNOB_SIZE: f32 = 46.0 * UI_SCALE; // same pixel size as .knob svg in the CSS, times UI_SCALE
const VIEWBOX: f32 = 56.0; // the SVG's internal viewBox units (unitless, not a size to scale)
const R: f32 = 22.0; // arc radius in viewbox units
const DISC_R: f32 = 15.0; // center disc radius in viewbox units
const STROKE_W: f32 = 5.0;

fn arc_point(center: Pos2, scale: f32, r: f32, deg: f32) -> Pos2 {
    // matches pt(cx,cy,r,deg): angle = (deg-90) in degrees, standard SVG y-down rotation
    let a = (deg - 90.0).to_radians();
    center + Vec2::new(a.cos(), a.sin()) * (r * scale)
}

fn arc_points(center: Pos2, scale: f32, r: f32, a1: f32, a2: f32, segments: usize) -> Vec<Pos2> {
    (0..=segments).map(|i| arc_point(center, scale, r, a1 + (a2 - a1) * (i as f32 / segments as f32))).collect()
}

/// Draws and handles one knob. `value01` is the current normalized value (0..1). Returns the
/// response so callers can check drag/hover if needed.
pub fn knob(ui: &mut egui::Ui, label: &str, value_text: &str, tone: Tone, param: &FloatParam, setter: &ParamSetter) {
    ui.vertical(|ui| {
        ui.set_width(KNOB_SIZE + 14.0 * UI_SCALE);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(KNOB_SIZE), Sense::click_and_drag());

        // vertical drag: moving up increases the value, same convention as the web knob
        if response.dragged() {
            if response.drag_started() {
                setter.begin_set_parameter(param);
            }
            let delta = response.drag_delta().y;
            let new_val = (param.value() - delta / 150.0).clamp(0.0, 1.0);
            setter.set_parameter(param, new_val);
        }
        if response.drag_stopped() {
            setter.end_set_parameter(param);
        }
        if response.hovered() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let step = if scroll > 0.0 { 0.03 } else { -0.03 };
                setter.begin_set_parameter(param);
                setter.set_parameter(param, (param.value() + step).clamp(0.0, 1.0));
                setter.end_set_parameter(param);
            }
        }

        let value01 = param.value().clamp(0.0, 1.0);
        let center = Pos2::new(rect.center().x, rect.center().y);
        let scale = KNOB_SIZE / VIEWBOX;

        let (track_color, disc_color, pointer_color) = match tone {
            Tone::Dark => (Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 90), PAPER, INK),
            Tone::Light => (Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 64), INK, PAPER),
        };

        let painter = ui.painter_at(rect);

        // track arc (full sweep, dim)
        painter.add(egui::Shape::line(arc_points(center, scale, R, -A0, A0, 48), Stroke::new(STROKE_W * scale, track_color)));
        // value arc (orange, from start to current value)
        let value_angle = -A0 + SWEEP * value01;
        if value01 > 0.001 {
            painter.add(egui::Shape::line(arc_points(center, scale, R, -A0, value_angle, 48), Stroke::new(STROKE_W * scale, ACCENT)));
        }
        // center disc
        painter.circle_filled(center, DISC_R * scale, disc_color);
        // pointer: from center out to radius 12 (viewbox units) at the current angle
        let tip = arc_point(center, scale, 12.0, value_angle);
        painter.line_segment([center, tip], Stroke::new(2.5 * scale.max(1.0), pointer_color));

        let text_color = ui.visuals().text_color();
        ui.add_space(2.0 * UI_SCALE);
        ui.label(egui::RichText::new(label).strong().size(11.0 * UI_SCALE).color(text_color));
        ui.label(egui::RichText::new(value_text).monospace().size(10.0 * UI_SCALE).color(text_color.gamma_multiply(0.8)));
    });
}

/// Frame with no rounding and no border, matching the poster-like sharp-cornered panels in the
/// CSS (`.panel{...}` has no border-radius).
pub fn panel_frame(fill: Color32) -> egui::Frame {
    let mut f = egui::Frame::new();
    f.fill = fill;
    f.inner_margin = egui::Margin::same((14.0 * UI_SCALE).round() as i8);
    f.corner_radius = egui::CornerRadius::ZERO;
    f.stroke = Stroke::NONE;
    f
}
