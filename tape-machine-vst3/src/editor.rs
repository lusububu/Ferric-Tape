// egui editor for the Tape Machine plugin, recreating the web app's look: the same four
// color-coded panels (Deck charcoal, Record blue, Playback purple, Transport orange, pulled
// straight from the :root CSS variables in page.template.html) and the same arc-gauge knobs
// (src/knob.rs), rather than generic sliders.

use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, resizable_window::ResizableWindow, EguiState};

use crate::knob::{knob, panel_frame, Tone, INK, PAPER};
use crate::{MachineChoice, SpeedChoice, TapeChoice, TapeMachineParams};

// Palette lifted from :root in page.template.html.
const GROUND: egui::Color32 = egui::Color32::from_rgb(0xc4, 0xc4, 0xc2); // --ground (page bg)
const DECK_BG: egui::Color32 = egui::Color32::from_rgb(0x26, 0x2d, 0x2e); // --c5 (.deck)
const RECORD_BG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x6e, 0xb1); // --c1 (.rec)
const PLAYBACK_BG: egui::Color32 = egui::Color32::from_rgb(0xb3, 0x91, 0xff); // --c3 (.pb)
const TRANSPORT_BG: egui::Color32 = egui::Color32::from_rgb(0xff, 0x55, 0x19); // --c2 (.tr)

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(640, 640)
}

fn section_label(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).heading().strong().color(color));
    ui.add_space(6.0);
}

fn enum_combo<T>(ui: &mut egui::Ui, id: &str, label: &str, param: &EnumParam<T>, setter: &ParamSetter, text_color: egui::Color32)
where
    T: Enum + PartialEq + Copy + 'static,
{
    let current = param.value();
    let variants = T::variants();
    let current_idx = current.to_index();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(text_color).monospace());
        egui::ComboBox::from_id_salt(id)
            .selected_text(egui::RichText::new(variants[current_idx]).color(INK))
            .show_ui(ui, |ui| {
                for (i, name) in variants.iter().enumerate() {
                    let selected = i == current_idx;
                    if ui.selectable_label(selected, *name).clicked() && !selected {
                        setter.begin_set_parameter(param);
                        setter.set_parameter(param, T::from_index(i));
                        setter.end_set_parameter(param);
                    }
                }
            });
    });
}

fn fmt_pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}
fn fmt_db(v: f32) -> String {
    let db = (v - 0.5) * 36.0;
    format!("{}{:.1} dB", if db > 0.0 { "+" } else { "" }, db)
}

pub fn create(params: Arc<TapeMachineParams>, editor_state: Arc<EguiState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state.clone(),
        (),
        |ctx, _| {
            // Flat, poster-like look: no rounding, no drop shadows, matching the web app.
            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
            style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
            ctx.set_style(style);
        },
        move |egui_ctx, setter, _state| {
            ResizableWindow::new("tape-machine-editor").min_size(egui::Vec2::new(480.0, 460.0)).show(egui_ctx, editor_state.as_ref(), |ui| {
                ui.visuals_mut().override_text_color = Some(INK);
                egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(egui::RichText::new("TAPE MACHINE").size(26.0).strong().color(INK));
                        ui.label(egui::RichText::new("Five physically modelled tape machines").monospace().size(11.0).color(INK.gamma_multiply(0.7)));
                        ui.add_space(10.0);

                        // ---------- Deck: machine / tape / speed ----------
                        panel_frame(DECK_BG).show(ui, |ui| {
                            section_label(ui, "Deck", PAPER);
                            enum_combo::<MachineChoice>(ui, "machine", "Machine", &params.machine, setter, PAPER);
                            enum_combo::<TapeChoice>(ui, "tape", "Tape", &params.tape, setter, PAPER);
                            enum_combo::<SpeedChoice>(ui, "speed", "Speed", &params.speed, setter, PAPER);
                        });
                        ui.add_space(10.0);

                        // ---------- Record: input / bias / hysteresis shape ----------
                        panel_frame(RECORD_BG).show(ui, |ui| {
                            section_label(ui, "Record", PAPER);
                            ui.horizontal(|ui| {
                                knob(ui, "Input", &fmt_db(params.input.value()), Tone::Dark, &params.input, setter);
                                knob(ui, "Bias", &fmt_pct(params.bias.value()), Tone::Dark, &params.bias, setter);
                                knob(ui, "Ceiling", &fmt_pct(params.ceiling.value()), Tone::Dark, &params.ceiling, setter);
                                knob(ui, "Knee", &fmt_pct(params.knee.value()), Tone::Dark, &params.knee, setter);
                                knob(ui, "Grip", &fmt_pct(params.grip.value()), Tone::Dark, &params.grip, setter);
                                knob(ui, "Give", &fmt_pct(params.give.value()), Tone::Dark, &params.give, setter);
                            });
                        });
                        ui.add_space(10.0);

                        // ---------- Playback: head geometry / output ----------
                        panel_frame(PLAYBACK_BG).show(ui, |ui| {
                            section_label(ui, "Playback", INK);
                            ui.horizontal(|ui| {
                                knob(ui, "Spacing", &fmt_pct(params.spacing.value()), Tone::Light, &params.spacing, setter);
                                knob(ui, "Coating", &fmt_pct(params.thickness.value()), Tone::Light, &params.thickness, setter);
                                knob(ui, "Gap", &fmt_pct(params.gap.value()), Tone::Light, &params.gap, setter);
                                knob(ui, "Output", &fmt_db(params.output.value()), Tone::Light, &params.output, setter);
                            });
                        });
                        ui.add_space(10.0);

                        // ---------- Transport: wow / flutter / hiss ----------
                        panel_frame(TRANSPORT_BG).show(ui, |ui| {
                            section_label(ui, "Transport", PAPER);
                            ui.horizontal(|ui| {
                                knob(ui, "Wow", &fmt_pct(params.wow.value()), Tone::Dark, &params.wow, setter);
                                knob(ui, "Flutter", &fmt_pct(params.flutter.value()), Tone::Dark, &params.flutter, setter);
                                knob(ui, "Hiss", &fmt_pct(params.hiss.value()), Tone::Dark, &params.hiss, setter);
                            });
                        });
                    });
                });
            });
        },
    )
}
