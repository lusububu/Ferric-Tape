// egui editor for the Tape Machine plugin, recreating the web app's look and its explanatory
// copy: the masthead with its corner readout, the function/roman-numeral header and caption on
// each signal-stage panel, the per-machine/per-tape notes under the Deck selectors, PP Mori as
// the display/body typeface (page.template.html uses Inter Tight, which isn't available to
// embed here; PP Mori is free for personal use, matching how this plugin is being used for now),
// and three live readouts the first pass of this port skipped:
//   - L/R peak meters (16-LED bars, -48..0 dBFS in 3 dB steps, same mapping as showLevel() in
//     page.template.html), fed from TapeMachine::process_rt via scope.rs.
//   - The hysteresis scope (record signal vs. recovered signal, "H in -> M out"), fed from the
//     same sx/sy pair the web app's canvas plots, published once per audio block.
//   - The frequency-response plot, which (like the web app's own version) is computed
//     analytically from the current machine/speed/head-geometry via circuits::frequency_response
//     rather than read from live audio, so it updates immediately as those controls move.
//
// Two honest simplifications, flagged rather than silently dropped:
//   - The masthead's bias readout shows each machine's nominal target frequency (Machine.fb),
//     not the live, sample-rate-quantized value the web app computes, since this editor doesn't
//     have the audio thread's sample rate to hand.
//   - The frequency-response plot doesn't shade each machine's published tolerance band: that
//     spec table (loF/hiF/±dB per machine/speed) was never ported to data.rs, only the response
//     curve itself was.

use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, resizable_window::ResizableWindow, EguiState};

use crate::data::{self, lookup};
use crate::knob::{knob, panel_frame, Tone, ACCENT, INK, PAPER};
use crate::scope::ScopeState;
use crate::{map_gap, map_spacing, map_thickness, MachineChoice, SpeedChoice, TapeChoice, TapeMachineParams};

// Palette lifted from :root in page.template.html.
const GROUND: egui::Color32 = egui::Color32::from_rgb(0xc4, 0xc4, 0xc2); // --ground (page bg)
const GROUND2: egui::Color32 = egui::Color32::from_rgb(0xb9, 0xba, 0xb8); // --ground2 (masthead bg)
const DECK_BG: egui::Color32 = egui::Color32::from_rgb(0x26, 0x2d, 0x2e); // --c5 (.deck)
const RECORD_BG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x6e, 0xb1); // --c1 (.rec)
const PLAYBACK_BG: egui::Color32 = egui::Color32::from_rgb(0xb3, 0x91, 0xff); // --c3 (.pb)
const TRANSPORT_BG: egui::Color32 = egui::Color32::from_rgb(0xff, 0x55, 0x19); // --c2 (.tr)
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x4b, 0x4d); // --muted
fn line_color() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 46) // --line-ish, scope/plot gridlines
}
fn led_off_color() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 36)
}
const LED_ON: egui::Color32 = egui::Color32::from_rgb(0xff, 0x8a, 0x52);
const LED_LIT: egui::Color32 = egui::Color32::from_rgb(0xff, 0xd3, 0xb8);

// Font family names for the three PP Mori weights embedded below.
const MORI_REGULAR: &str = "mori-regular";
const MORI_SEMIBOLD: &str = "mori-semibold";
const MORI_BLACK: &str = "mori-black";

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(MORI_REGULAR.to_owned(), egui::FontData::from_static(include_bytes!("../assets/fonts/PPMori-Regular.otf")).into());
    fonts.font_data.insert(MORI_SEMIBOLD.to_owned(), egui::FontData::from_static(include_bytes!("../assets/fonts/PPMori-Semibold.otf")).into());
    fonts.font_data.insert(MORI_BLACK.to_owned(), egui::FontData::from_static(include_bytes!("../assets/fonts/PPMori-Black.otf")).into());

    // Regular becomes the default proportional font (used by every RichText that doesn't ask for
    // a specific family), with egui's own default kept right after it as a fallback for any
    // glyph PP Mori doesn't cover (¾, ½, ×, the en dash, etc.).
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, MORI_REGULAR.to_owned());
    fonts.families.insert(egui::FontFamily::Name(MORI_SEMIBOLD.into()), vec![MORI_SEMIBOLD.to_owned(), MORI_REGULAR.to_owned()]);
    fonts.families.insert(egui::FontFamily::Name(MORI_BLACK.into()), vec![MORI_BLACK.to_owned(), MORI_REGULAR.to_owned()]);
    // Monospace stays egui's default (Hack): page.template.html's IBM Plex Mono isn't embedded.

    ctx.set_fonts(fonts);
}

fn mori(size: f32, weight: &str) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(weight.into()))
}

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(700, 920)
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
        ui.label(egui::RichText::new(title).font(mori(13.0, MORI_SEMIBOLD)).color(color));
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

// 16 LEDs across -48..0 dBFS, 3 dB each; same mapping as showLevel() in page.template.html.
fn draw_vu_row(ui: &mut egui::Ui, label: &str, peak: f32, text_color: egui::Color32) {
    let db = if peak > 0.0 { 20.0 * peak.log10() } else { -99.0 };
    let n = (((db + 48.0) / 3.0).ceil() as i32).clamp(0, 16);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).monospace().size(10.0).color(text_color));
        for i in 0..16 {
            let on = i < n;
            let lit = on && i == n - 1 && db > -6.0;
            let col = if lit {
                LED_LIT
            } else if on {
                LED_ON
            } else {
                led_off_color()
            };
            let (rect, _resp) = ui.allocate_exact_size(egui::Vec2::splat(9.0), egui::Sense::hover());
            ui.painter_at(rect).circle_filled(rect.center(), 3.6, col);
        }
    });
}

// "H in -> M out": record signal (x) against the recovered signal (y), same pair the web app's
// scope canvas plots. `trace` is a snapshot of ScopeState's current block.
fn draw_hysteresis_scope(ui: &mut egui::Ui, trace: &[[f32; 2]]) {
    let size = egui::Vec2::new(ui.available_width().max(200.0), 190.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PAPER);

    let cx = rect.center().x;
    let cy = rect.center().y;
    painter.line_segment([egui::pos2(cx, rect.top() + 8.0), egui::pos2(cx, rect.bottom() - 8.0)], egui::Stroke::new(1.0f32, line_color()));
    painter.line_segment([egui::pos2(rect.left() + 8.0, cy), egui::pos2(rect.right() - 8.0, cy)], egui::Stroke::new(1.0f32, line_color()));
    painter.text(egui::pos2(rect.right() - 16.0, cy - 12.0), egui::Align2::CENTER_CENTER, "H", egui::FontId::monospace(11.0), MUTED);
    painter.text(egui::pos2(cx + 12.0, rect.top() + 14.0), egui::Align2::CENTER_CENTER, "M", egui::FontId::monospace(11.0), MUTED);

    if trace.len() < 4 {
        painter.text(
            rect.left_bottom() + egui::vec2(14.0, -14.0),
            egui::Align2::LEFT_BOTTOM,
            "Waiting for signal",
            egui::FontId::monospace(11.0),
            MUTED,
        );
        return;
    }

    let mut mx = 0.05f32;
    let mut my = 0.05f32;
    for p in trace {
        mx = mx.max(p[0].abs());
        my = my.max(p[1].abs());
    }
    let half_w = (rect.width() / 2.0 - 14.0).max(1.0);
    let half_h = (rect.height() / 2.0 - 14.0).max(1.0);
    let sx = half_w / mx;
    let sy = half_h / my;
    let pts: Vec<egui::Pos2> = trace.iter().map(|p| egui::pos2(cx + p[0] * sx, cy - p[1] * sy)).collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6f32, ACCENT)));
}

// Analytic playback-head/record-loss magnitude response, log-frequency / dB plot, mirroring
// drawResp() in page.template.html (minus its shaded published-tolerance band; see module doc).
fn draw_frequency_response(ui: &mut egui::Ui, mag: &[f64]) {
    let size = egui::Vec2::new(ui.available_width().max(260.0), 210.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PAPER);

    let l = rect.left() + 42.0;
    let r = rect.right() - 10.0;
    let t = rect.top() + 10.0;
    let b = rect.bottom() - 20.0;
    let db_min = -36.0f32;
    let db_max = 9.0f32;
    let log20 = 20f32.log10();
    let log_span = 20000f32.log10() - log20;
    let fx = |f: f32| l + (f.log10() - log20) / log_span * (r - l);
    let fy = |db: f32| t + (db_max - db) / (db_max - db_min) * (b - t);

    for f in [20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0f32] {
        let x = fx(f);
        painter.line_segment([egui::pos2(x, t), egui::pos2(x, b)], egui::Stroke::new(1.0f32, line_color()));
        let label = if f >= 1000.0 { format!("{}k", (f / 1000.0) as i32) } else { format!("{}", f as i32) };
        painter.text(egui::pos2(x, b + 11.0), egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(9.0), MUTED);
    }
    let mut db = db_min;
    while db <= db_max + 0.01 {
        let y = fy(db);
        painter.line_segment([egui::pos2(l, y), egui::pos2(r, y)], egui::Stroke::new(1.0f32, line_color()));
        painter.text(egui::pos2(l - 5.0, y), egui::Align2::RIGHT_CENTER, format!("{}", db as i32), egui::FontId::monospace(9.0), MUTED);
        db += 6.0;
    }
    painter.line_segment([egui::pos2(l, fy(0.0)), egui::pos2(r, fy(0.0))], egui::Stroke::new(1.0f32, egui::Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 110)));

    if mag.len() < 2 {
        return;
    }
    let nb = mag.len() - 1;
    let fs_r = 48000.0f64;
    let at = |f: f64| -> f64 {
        let idx = (f / (fs_r / 2.0) * nb as f64).clamp(0.0, nb as f64);
        let i0 = (idx.floor() as usize).min(nb - 1);
        let fr = idx - i0 as f64;
        (mag[i0] * (1.0 - fr) + mag[i0 + 1] * fr).abs()
    };
    let refv = at(700.0).max(1e-9);
    let steps = ((r - l).max(1.0)) as i32;
    let mut pts = Vec::with_capacity(steps as usize + 1);
    for px in 0..=steps {
        let frac = px as f64 / steps as f64;
        let f = 10f64.powf(log20 as f64 + frac * (log_span as f64));
        let a = (at(f) / refv).max(1e-6);
        let dbv = (20.0 * a.log10()).clamp(db_min as f64, db_max as f64) as f32;
        pts.push(egui::pos2(l + px as f32, fy(dbv)));
    }
    painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0f32, INK)));
}

pub fn create(params: Arc<TapeMachineParams>, editor_state: Arc<EguiState>, scope: Arc<ScopeState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        editor_state.clone(),
        (),
        |ctx, _| {
            install_fonts(ctx);
            // Flat, poster-like look: no rounding, no drop shadows, matching the web app.
            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
            style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
            ctx.set_style(style);
        },
        move |egui_ctx, setter, _state| {
            // Meters and the scope animate continuously, so keep repainting even without input.
            egui_ctx.request_repaint();

            ResizableWindow::new("tape-machine-editor").min_size(egui::Vec2::new(480.0, 560.0)).show(egui_ctx, editor_state.as_ref(), |ui| {
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
                            ui.label(egui::RichText::new("TAPE MACHINE").font(mori(34.0, MORI_BLACK)).color(INK));
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

                        // ---------- Deck: machine / tape / speed / output level ----------
                        panel_frame(DECK_BG).show(ui, |ui| {
                            ui.label(egui::RichText::new("Deck").font(mori(18.0, MORI_SEMIBOLD)).color(PAPER));
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
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Output level").monospace().size(10.0).color(PAPER.gamma_multiply(0.85)));
                            draw_vu_row(ui, "L", scope.peak(0), PAPER);
                            draw_vu_row(ui, "R", scope.peak(1), PAPER);
                        });
                        ui.add_space(10.0);

                        // ---------- Hysteresis scope ----------
                        panel_frame(PAPER).show(ui, |ui| {
                            section_header(ui, "Hysteresis", "H in \u{2192} M out", INK);
                            caption(ui, "Record signal (x) against the tape's magnetism (y). Raise Input or lower Bias to see the loop bend open.", MUTED);
                            draw_hysteresis_scope(ui, &scope.snapshot_trace());
                        });
                        ui.add_space(10.0);

                        // ---------- Frequency response ----------
                        panel_frame(PAPER).show(ui, |ui| {
                            section_header(ui, "Frequency response", &format!("{} ips", speed_name(speed_ips)), INK);
                            caption(ui, &format!("How {} shapes the frequency response at {} ips.", machine.label, speed_name(speed_ips)), MUTED);
                            let mag = crate::circuits::frequency_response(
                                machine_id,
                                speed_ips,
                                map_spacing(params.spacing.value() as f64),
                                map_thickness(params.thickness.value() as f64),
                                map_gap(params.gap.value() as f64),
                            );
                            draw_frequency_response(ui, &mag);
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
