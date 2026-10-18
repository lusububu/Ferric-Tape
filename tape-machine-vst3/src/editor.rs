// egui editor for the Tape Machine plugin, recreating the web app's look and its explanatory
// copy: the masthead with its corner readout, the function/roman-numeral header and caption on
// each signal-stage panel, the per-machine/per-tape notes under the Deck selectors, PP Mori as
// the display/body typeface (page.template.html uses Inter Tight, which isn't available to
// embed here; PP Mori is free for personal use, matching how this plugin is being used for now),
// three live readouts (L/R peak meters, the hysteresis scope, and an analytic frequency-response
// plot; see each draw_* function below), and two more fidelity passes:
//   - A 2-column / 3-column grid (Deck beside Hysteresis+Frequency response, then Record/
//     Playback/Transport in a row), instead of one long single-column stack, mirroring the web
//     app's 12-column grid (.s7/.s5 on the first row, three .s4's on the second).
//   - Soft radial-gradient-style color blobs behind each panel's content (approximating the
//     blurred CSS `.field` backgrounds) and a faint tiled grain texture over the whole window
//     (approximating the CSS grain overlay), egui having no blur filter to draw a true gaussian
//     blur with.
//
// Three honest simplifications, flagged rather than silently dropped:
//   - The masthead's bias readout shows each machine's nominal target frequency (Machine.fb),
//     not the live, sample-rate-quantized value the web app computes, since this editor doesn't
//     have the audio thread's sample rate to hand.
//   - The frequency-response plot doesn't shade each machine's published tolerance band: that
//     spec table (loF/hiF/±dB per machine/speed) was never ported to data.rs, only the response
//     curve itself was.
//   - The background blobs are concentric flat-alpha circles, not a true gaussian blur (egui's
//     painter has no blur primitive), so they read as a soft glow rather than the CSS's smooth
//     blurred wash. Close enough to break up the flat fills; not pixel-identical.

use std::sync::{Arc, Mutex};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, resizable_window::ResizableWindow, EguiState};

use crate::data::{self, lookup};
use crate::knob::{knob, panel_frame, Tone, ACCENT, INK, PAPER};
use crate::scope::ScopeState;
use crate::{map_gap, map_spacing, map_thickness, MachineChoice, SpeedChoice, TapeChoice, TapeMachineParams};

// Palette lifted from :root in page.template.html.
const GROUND: egui::Color32 = egui::Color32::from_rgb(0xc4, 0xc4, 0xc2); // --ground (page bg)
const GROUND2: egui::Color32 = egui::Color32::from_rgb(0xb9, 0xba, 0xb8); // --ground2 (masthead bg)
const C1: egui::Color32 = egui::Color32::from_rgb(0x00, 0x6e, 0xb1); // --c1, blue (Record bg)
const C2: egui::Color32 = egui::Color32::from_rgb(0xff, 0x55, 0x19); // --c2, orange (Transport bg / accent)
const C3: egui::Color32 = egui::Color32::from_rgb(0xb3, 0x91, 0xff); // --c3, purple (Playback bg)
const C4: egui::Color32 = egui::Color32::from_rgb(0x2b, 0x00, 0x08); // --c4, dark maroon, decorative only
const C5: egui::Color32 = egui::Color32::from_rgb(0x26, 0x2d, 0x2e); // --c5, charcoal (Deck bg)
const C6: egui::Color32 = egui::Color32::from_rgb(0xff, 0xc7, 0xe6); // --c6, pink, decorative only
const DECK_BG: egui::Color32 = C5;
const RECORD_BG: egui::Color32 = C1;
const PLAYBACK_BG: egui::Color32 = C3;
const TRANSPORT_BG: egui::Color32 = C2;
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
    EguiState::from_size(820, 880)
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

// ---------- decorative "field" backdrops: soft radial color blobs behind each panel ----------

// Paints one soft-edged color blob: a stack of circles shrinking in radius and growing in alpha
// toward `center`, which reads as a rough glow/blur without needing a real blur filter.
fn soft_blob(painter: &egui::Painter, center: egui::Pos2, max_radius: f32, color: egui::Color32, peak_alpha: u8) {
    const RINGS: i32 = 10;
    for step in 0..RINGS {
        let frac = step as f32 / (RINGS - 1) as f32; // 0.0 = outer (first paint), 1.0 = center (last paint)
        let r = max_radius * (1.0 - frac * 0.88);
        let a = (peak_alpha as f32 * frac.powf(1.4)) as u8;
        if a == 0 {
            continue;
        }
        painter.circle_filled(center, r.max(2.0), egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a));
    }
}

// Fills `rect` with `base`, then paints each (x_frac, y_frac, color) blob clipped to that rect,
// mirroring one panel's `.field` radial-gradient stack from page.template.html.
fn paint_field(ui: &egui::Ui, rect: egui::Rect, base: egui::Color32, blobs: &[(f32, f32, egui::Color32)], peak_alpha: u8) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, base);
    let max_r = rect.width().max(rect.height()) * 0.62;
    for &(fx, fy, color) in blobs {
        let center = egui::pos2(rect.left() + rect.width() * fx, rect.top() + rect.height() * fy);
        soft_blob(&painter, center, max_r, color, peak_alpha);
    }
}

// The rect a panel's content is about to occupy: full available width, a caller-chosen height
// (panels don't know their own final height ahead of time in immediate-mode egui, so this is an
// estimate sized to each panel's typical content, used only for the background blobs, which
// don't need to be pixel-exact).
fn field_rect(ui: &egui::Ui, height: f32) -> egui::Rect {
    egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), height))
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
    let size = egui::Vec2::new(ui.available_width().max(200.0), 170.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PAPER.gamma_multiply(0.97));

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
    let size = egui::Vec2::new(ui.available_width().max(260.0), 190.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PAPER.gamma_multiply(0.97));

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
    painter.line_segment(
        [egui::pos2(l, fy(0.0)), egui::pos2(r, fy(0.0))],
        egui::Stroke::new(1.0f32, egui::Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 110)),
    );

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

// ---------- grain overlay: a tiled, pseudo-random noise texture over the whole window ----------

fn make_noise_texture(ctx: &egui::Context) -> egui::TextureHandle {
    const N: usize = 48;
    let mut image = egui::ColorImage::new([N, N], egui::Color32::TRANSPARENT);
    let mut seed: u32 = 0x9e37_79b9;
    for px in image.pixels.iter_mut() {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let v = (seed >> 24) as u8;
        let alpha = (v as u16 * 22 / 255) as u8; // keep it faint: grain, not static
        *px = egui::Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, alpha);
    }
    ctx.load_texture("tape-machine-grain", image, egui::TextureOptions::NEAREST)
}

fn paint_grain(painter: &egui::Painter, rect: egui::Rect, tex: &egui::TextureHandle) {
    const TILE: f32 = 48.0;
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let mut y = rect.top();
    while y < rect.bottom() {
        let mut x = rect.left();
        while x < rect.right() {
            let tile_rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(TILE, TILE)).intersect(rect);
            painter.image(tex.id(), tile_rect, uv, egui::Color32::WHITE);
            x += TILE;
        }
        y += TILE;
    }
}

pub fn create(params: Arc<TapeMachineParams>, editor_state: Arc<EguiState>, scope: Arc<ScopeState>) -> Option<Box<dyn Editor>> {
    let grain_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);

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
            {
                let mut guard = grain_tex.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(make_noise_texture(egui_ctx));
                }
            }

            ResizableWindow::new("tape-machine-editor").min_size(egui::Vec2::new(560.0, 560.0)).show(egui_ctx, editor_state.as_ref(), |ui| {
                ui.visuals_mut().override_text_color = Some(INK);
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 10.0);
                let outer = egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let machine_id = params.machine.value().id();
                        let tape_id = params.tape.value().id();
                        let speed_ips = params.speed.value().ips();
                        let machine = data::machine_by_id(machine_id);
                        let tape = data::tape_by_id(tape_id);

                        // ---------- Masthead ----------
                        egui::Frame::new().fill(GROUND2).inner_margin(egui::Margin::same(18)).show(ui, |ui| {
                            let rect = field_rect(ui, 118.0);
                            paint_field(ui, rect, GROUND2, &[(0.3, 0.7, C2), (0.72, 0.38, C1), (0.55, 0.8, C3), (0.85, 0.85, C4)], 30);
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

                        // ---------- Row 1: Deck (left, wider) beside Hysteresis + Frequency response (right) ----------
                        ui.columns(2, |cols| {
                            // Deck: machine / tape / speed / output level
                            panel_frame(DECK_BG).show(&mut cols[0], |ui| {
                                let rect = field_rect(ui, 300.0);
                                paint_field(ui, rect, DECK_BG, &[(0.22, 0.7, C1), (0.8, 0.24, C2), (0.58, 0.22, C3), (0.15, 0.88, C4)], 55);
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

                            // Hysteresis scope + frequency response, stacked
                            panel_frame(PAPER).show(&mut cols[1], |ui| {
                                let rect = field_rect(ui, 230.0);
                                paint_field(ui, rect, PAPER, &[(0.5, 0.35, C2), (0.5, 0.85, C6)], 22);
                                section_header(ui, "Hysteresis", "H in \u{2192} M out", INK);
                                caption(ui, "Record signal (x) against the tape's magnetism (y). Raise Input or lower Bias to see the loop bend open.", MUTED);
                                draw_hysteresis_scope(ui, &scope.snapshot_trace());
                            });
                            cols[1].add_space(10.0);
                            panel_frame(PAPER).show(&mut cols[1], |ui| {
                                let rect = field_rect(ui, 250.0);
                                paint_field(ui, rect, PAPER, &[(0.5, 0.35, C2), (0.5, 0.85, C6)], 16);
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
                        });
                        ui.add_space(10.0);

                        // ---------- Row 2: Record / Playback / Transport, three across ----------
                        ui.columns(3, |cols| {
                            panel_frame(RECORD_BG).show(&mut cols[0], |ui| {
                                let rect = field_rect(ui, 180.0);
                                paint_field(ui, rect, RECORD_BG, &[(0.3, 0.3, C1), (0.8, 0.9, C3)], 45);
                                section_header(ui, "Record · magnetise", "I\u{2013}III", PAPER);
                                caption(ui, "The signal, plus a high-frequency bias tone, magnetizes the tape as it passes the record head.", PAPER.gamma_multiply(0.9));
                                egui::Grid::new("record-knobs").spacing(egui::vec2(6.0, 8.0)).show(ui, |ui| {
                                    knob(ui, "Input", &fmt_db(params.input.value()), Tone::Dark, &params.input, setter);
                                    knob(ui, "Bias", &fmt_pct(params.bias.value()), Tone::Dark, &params.bias, setter);
                                    knob(ui, "Ceiling", &fmt_pct(params.ceiling.value()), Tone::Dark, &params.ceiling, setter);
                                    ui.end_row();
                                    knob(ui, "Knee", &fmt_pct(params.knee.value()), Tone::Dark, &params.knee, setter);
                                    knob(ui, "Grip", &fmt_pct(params.grip.value()), Tone::Dark, &params.grip, setter);
                                    knob(ui, "Give", &fmt_pct(params.give.value()), Tone::Dark, &params.give, setter);
                                    ui.end_row();
                                });
                            });

                            panel_frame(PLAYBACK_BG).show(&mut cols[1], |ui| {
                                let rect = field_rect(ui, 180.0);
                                paint_field(ui, rect, PLAYBACK_BG, &[(0.7, 0.2, C3), (0.2, 0.9, C6)], 45);
                                section_header(ui, "Playback · head losses", "IV\u{2013}V", INK);
                                caption(ui, "Tape-to-head spacing, tape thickness, and head gap width all soften the highs on playback.", MUTED);
                                egui::Grid::new("playback-knobs").spacing(egui::vec2(6.0, 8.0)).show(ui, |ui| {
                                    knob(ui, "Spacing", &fmt_pct(params.spacing.value()), Tone::Light, &params.spacing, setter);
                                    knob(ui, "Coating", &fmt_pct(params.thickness.value()), Tone::Light, &params.thickness, setter);
                                    ui.end_row();
                                    knob(ui, "Gap", &fmt_pct(params.gap.value()), Tone::Light, &params.gap, setter);
                                    knob(ui, "Output", &fmt_db(params.output.value()), Tone::Light, &params.output, setter);
                                    ui.end_row();
                                });
                            });

                            panel_frame(TRANSPORT_BG).show(&mut cols[2], |ui| {
                                let rect = field_rect(ui, 180.0);
                                paint_field(ui, rect, TRANSPORT_BG, &[(0.25, 0.25, C2), (0.8, 0.8, C4)], 45);
                                section_header(ui, "Transport · speed error", "VI", PAPER);
                                let wf_pct = lookup(machine.wf, speed_ips) * 100.0;
                                caption(
                                    ui,
                                    &format!("\u{d7}1.00 = {} peak speed error, {:.2} % at {} ips", machine.label, wf_pct, speed_name(speed_ips)),
                                    PAPER.gamma_multiply(0.9),
                                );
                                egui::Grid::new("transport-knobs").spacing(egui::vec2(6.0, 8.0)).show(ui, |ui| {
                                    knob(ui, "Wow", &fmt_pct(params.wow.value()), Tone::Dark, &params.wow, setter);
                                    knob(ui, "Flutter", &fmt_pct(params.flutter.value()), Tone::Dark, &params.flutter, setter);
                                    ui.end_row();
                                    knob(ui, "Hiss", &fmt_pct(params.hiss.value()), Tone::Dark, &params.hiss, setter);
                                    ui.end_row();
                                });
                            });
                        });
                    });
                });

                if let Some(tex) = grain_tex.lock().unwrap().as_ref() {
                    paint_grain(&ui.painter_at(outer.response.rect), outer.response.rect, tex);
                }
            });
        },
    )
}
