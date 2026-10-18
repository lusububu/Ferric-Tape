// egui editor for the Tape Machine plugin, recreating the web app's look and its explanatory
// copy: the masthead with its corner readout, the function/roman-numeral header and caption on
// each signal-stage panel, and the per-machine/per-tape notes under the Deck selectors. All of
// this text is pulled straight from page.template.html's refreshLabels()/MACH_NOTE/TAPE_NOTE, not
// invented, with one honest simplification: the corner readout shows each machine's nominal
// target bias frequency (Machine.fb), not the live, sample-rate-quantized value the web app
// computes per buffer, since the editor doesn't have the audio thread's sample rate to hand. Live
// VU meters, the hysteresis scope and the frequency-response plot are not ported here: they'd
// need the audio thread to stream data to the UI thread, which this file doesn't yet do.

use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, resizable_window::ResizableWindow, EguiState};

use crate::data::{self, lookup};
use crate::knob::{knob, panel_frame, Tone, INK, PAPER};
use crate::{MachineChoice, SpeedChoice, TapeChoice, TapeMachineParams};

// Palette lifted from :root in page.template.html.
const GROUND: egui::Color32 = egui::Color32::from_rgb(0xc4, 0xc4, 0xc2); // --ground (page bg)
const GROUND2: egui::Color32 = egui::Color32::from_rgb(0xb9, 0xba, 0xb8); // --ground2 (masthead bg)
const DECK_BG: egui::Color32 = egui::Color32::from_rgb(0x26, 0x2d, 0x2e); // --c5 (.deck)
const RECORD_BG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x6e, 0xb1); // --c1 (.rec)
const PLAYBACK_BG: egui::Color32 = egui::Color32::from_rgb(0xb3, 0x91, 0xff); // --c3 (.pb)
const TRANSPORT_BG: egui::Color32 = egui::Color32::from_rgb(0xff, 0x55, 0x19); // --c2 (.tr)
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x4b, 0x4d); // --muted

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(680, 760)
}

// Same copy as speedName in page.template.html.
fn speed_name(ips: f64) -> &'static str {
    if (ips - 3.75).abs() < 0.01 {
        "3¾"
    } else if (ips - 7.5).abs() < 0.01 {
        "7½"
    } else if (ips - 15.0).abs() < 0.01 {
        "15"
    } else {
        "30"
    }
}

// MACH_NOTE from page.template.html, verbatim.
fn machine_note(id: &str) -> &'static str {
    match id {
        "tc260" => "A home reel-to-reel deck from the mid-60s, warm and a little loose.",
        "j37" => "A 1-inch valve studio machine, driven hot for a thick, saturated sound.",
        "a77" => "A semi-pro ¼-inch deck, cleaner and tighter than a home machine.",
        "a80" => "A ¼-inch studio machine with transistor and op-amp electronics: clean headroom with just a touch of grit on peaks.",
        "atr100" => "A mastering machine built for the cleanest possible transfer, from slow to very fast tape speeds.",
        _ => "",
    }
}

// TAPE_NOTE from page.template.html, verbatim.
fn tape_note(id: &str) -> &'static str {
    match id {
        "ferric" => "A classic reel-to-reel formulation, and the baseline this model is built around.",
        "studio" => "A warm, punchy studio tape with a vintage character.",
        "sm468" => "A higher-output tape with a bit more headroom before it saturates.",
        "a632" => "A thinner, longer-play tape with slightly less headroom.",
        "cobalt" => "An output-boosted formulation, more often found in cassettes than on reels.",
        "chrome" => "Known for crisp highs; more common in cassettes than on reels.",
        "metal" => "A high-output formulation with the most headroom of the group.",
        _ => "",
    }
}

fn section_header(ui: &mut egui::Ui, title: &str, numeral: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).monospace().size(11.0).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(numeral).monospace().size(11.0).color(color));
        });
    });
}

fn caption(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).monospace().size(10.5).color(color));
    ui.add_space(10.0);
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
            ResizableWindow::new("tape-machine-editor").min_size(egui::Vec2::new(480.0, 520.0)).show(egui_ctx, editor_state.as_ref(), |ui| {
                ui.visuals_mut().override_text_color = Some(INK);
                egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let machine_id = params.machine.value().id();
                        let tape_id = params.tape.value().id();
                        let speed_ips = params.speed.value().ips();
                        let machine = data::machine_by_id(machine_id);
                        let tape = data::tape_by_id(tape_id);

                        // ---------- Masthead ----------
                        egui::Frame::new().fill(GROUND2).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
                            ui.label(egui::RichText::new("TAPE MACHINE").size(34.0).strong().color(INK));
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} · {} ips · {}", machine.label, speed_name(speed_ips), tape.label))
                                        .monospace()
                                        .size(10.5)
                                        .color(INK),
                                );
                                ui.add_space(14.0);
                                ui.label(egui::RichText::new(format!("Target bias · {:.1} kHz", machine.fb / 1000.0)).monospace().size(10.5).color(INK));
                                ui.add_space(14.0);
                                ui.label(egui::RichText::new("8× oversampled").monospace().size(10.5).color(INK));
                            });
                        });
                        ui.add_space(10.0);

                        // ---------- Deck: machine / tape / speed ----------
                        panel_frame(DECK_BG).show(ui, |ui| {
                            ui.label(egui::RichText::new("Deck").size(18.0).strong().color(PAPER));
                            ui.add_space(6.0);
                            enum_combo::<MachineChoice>(ui, "machine", "Machine", &params.machine, setter, PAPER);
                            ui.label(egui::RichText::new(machine_note(machine_id)).monospace().size(10.0).color(PAPER.gamma_multiply(0.85)));
                            ui.add_space(6.0);
                            enum_combo::<TapeChoice>(ui, "tape", "Tape", &params.tape, setter, PAPER);
                            ui.label(
                                egui::RichText::new(format!("{} Bias and level adjust to match.", tape_note(tape_id)))
                                    .monospace()
                                    .size(10.0)
                                    .color(PAPER.gamma_multiply(0.85)),
                            );
                            ui.add_space(6.0);
                            enum_combo::<SpeedChoice>(ui, "speed", "Speed", &params.speed, setter, PAPER);
                        });
                        ui.add_space(10.0);

                        // ---------- Record: input / bias / hysteresis shape ----------
                        panel_frame(RECORD_BG).show(ui, |ui| {
                            section_header(ui, "Record · magnetise", "I\u{2013}III", PAPER);
                            caption(ui, "The signal, plus a high-frequency bias tone, magnetizes the tape as it passes the record head.", PAPER.gamma_multiply(0.9));
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
                            section_header(ui, "Playback · head losses", "IV\u{2013}V", INK);
                            caption(ui, "Tape-to-head spacing, tape thickness, and head gap width all soften the highs on playback.", MUTED);
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
                            section_header(ui, "Transport · speed error", "VI", PAPER);
                            let wf_pct = lookup(machine.wf, speed_ips) * 100.0;
                            caption(
                                ui,
                                &format!("\u{d7}1.00 = {} peak speed error, {:.2} % at {} ips", machine.label, wf_pct, speed_name(speed_ips)),
                                PAPER.gamma_multiply(0.9),
                            );
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
