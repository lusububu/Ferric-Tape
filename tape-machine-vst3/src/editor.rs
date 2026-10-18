// egui editor for the Tape Machine plugin, matching the web app (page.template.html) closely:
// the "Tape machine" masthead and subhead, big section headings inside each stage panel (not
// just the small mono label row), segmented tape-speed buttons filtered to the current machine's
// valid speeds, a real Bypass toggle, richer machine descriptions, PP Mori as the display/body
// typeface, three live readouts (L/R peak meters, the hysteresis scope, an analytic frequency-
// response plot), and two fidelity passes the previous attempt only approximated:
//   - Smooth background washes instead of visible concentric rings: each panel's backdrop is a
//     small texture rendered pixel-by-pixel with analytic radial falloff (no geometry, no
//     banding), then stretched over the panel with bilinear filtering.
//   - The actual grain algorithm from page.template.html's makeGrain(): a 160x160 texture where
//     each pixel is black or white with alpha drawn from the same triangular-ish distribution
//     (average of three uniform randoms), tiled over the whole window.
//
// Honest differences from the web app, flagged rather than silently dropped or faked:
//   - No "Play" button or demo loop: a VST has no audio of its own to play, the host's transport
//     supplies real audio, so that control (and the "Demo loop · 92 bpm" / "Ready. Sound starts
//     when you press Play." strings tied to it) has no equivalent here and isn't recreated.
//   - "Bypass" IS real here (new BoolParam, wired into TapeMachinePlugin::process in lib.rs): a
//     dry/wet toggle is a legitimate plugin feature, not just decoration, so it's implemented
//     rather than left as a dead button.
//   - "Oversampling" is shown as a single non-interactive "8x" chip, not a working 8x/16x toggle:
//     the Rust engine's oversample factor (OVERSAMPLE in lib.rs) is a compile-time constant, so a
//     16x option would need real DSP work (variable-size oversampling buffers), not just a GUI
//     control. Showing a clickable 16x that didn't do anything would be misleading.
//   - The masthead's bias readout shows each machine's nominal target frequency (Machine.fb), not
//     the live, sample-rate-quantized value the web app computes, since the editor doesn't have
//     the audio thread's sample rate at paint time.
//   - The frequency-response plot doesn't shade each machine's published tolerance band: that
//     spec table (loF/hiF/±dB per machine/speed) was never ported into data.rs, only the response
//     curve itself was.

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
    egui::Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 46)
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
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, MORI_REGULAR.to_owned());
    fonts.families.insert(egui::FontFamily::Name(MORI_SEMIBOLD.into()), vec![MORI_SEMIBOLD.to_owned(), MORI_REGULAR.to_owned()]);
    fonts.families.insert(egui::FontFamily::Name(MORI_BLACK.into()), vec![MORI_BLACK.to_owned(), MORI_REGULAR.to_owned()]);
    ctx.set_fonts(fonts);
}

fn mori(size: f32, weight: &str) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(weight.into()))
}

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(900, 900)
}

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

fn speed_choice_for(ips: f64) -> SpeedChoice {
    if (ips - 3.75).abs() < 0.01 {
        SpeedChoice::S3_75
    } else if (ips - 7.5).abs() < 0.01 {
        SpeedChoice::S7_5
    } else if (ips - 15.0).abs() < 0.01 {
        SpeedChoice::S15
    } else {
        SpeedChoice::S30
    }
}

// The <option> label text from page.template.html's #machSel, verbatim.
fn machine_title(id: &str) -> &'static str {
    match id {
        "j37" => "Stellan J-37 · 1964 · valve studio",
        "a80" => "Stellan A-80 · 1970 · studio",
        "a77" => "Arvox A-7 · 1967 · semi-pro",
        "atr100" => "Apexa MT-100 · 1976 · mastering",
        "tc260" => "Tonica TC-6 · 1965 · home deck",
        _ => "",
    }
}

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

fn big_heading(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).font(mori(27.0, MORI_SEMIBOLD)).color(color));
}

fn section_label(ui: &mut egui::Ui, title: &str, numeral: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title.to_uppercase()).monospace().size(10.5).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(numeral).monospace().size(10.5).color(color));
        });
    });
}

fn caption(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).monospace().size(10.5).color(color));
    ui.add_space(10.0);
}

fn uppercase_label(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text.to_uppercase()).monospace().size(9.5).color(color));
}

// A pressed/unpressed button matching the web app's `.seg button[aria-pressed]` look: filled when
// active, outlined when not.
fn seg_button(ui: &mut egui::Ui, label: &str, active: bool, fg: egui::Color32, fill_when_active: egui::Color32) -> egui::Response {
    let (text_color, bg) = if active { (INK, fill_when_active) } else { (fg, egui::Color32::TRANSPARENT) };
    let button = egui::Button::new(egui::RichText::new(label).monospace().size(10.5).color(text_color))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0f32, fg.gamma_multiply(0.5)))
        .corner_radius(0.0);
    ui.add(button)
}

fn enum_combo<T>(ui: &mut egui::Ui, id: &str, label: &str, param: &EnumParam<T>, setter: &ParamSetter, text_color: egui::Color32, display: impl Fn(&'static str) -> &'static str)
where
    T: Enum + PartialEq + Copy + 'static,
{
    let current = param.value();
    let variants = T::variants();
    let current_idx = current.to_index();
    uppercase_label(ui, label, text_color);
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width())
        .selected_text(egui::RichText::new(display(variants[current_idx])).monospace().size(11.0).color(INK))
        .show_ui(ui, |ui| {
            for (i, name) in variants.iter().enumerate() {
                let selected = i == current_idx;
                if ui.selectable_label(selected, display(name)).clicked() && !selected {
                    setter.begin_set_parameter(param);
                    setter.set_parameter(param, T::from_index(i));
                    setter.end_set_parameter(param);
                }
            }
        });
}

fn fmt_pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}
fn fmt_db(v: f32) -> String {
    let db = (v - 0.5) * 36.0;
    format!("{}{:.1} dB", if db > 0.0 { "+" } else { "" }, db)
}

// ---------- backgrounds: smooth analytic washes + authentic film grain, both as textures ----------

// One radial color stop: center (fraction of the texture size), color, radius (fraction of the
// texture's larger dimension) and peak strength (0..1), mirroring one radial-gradient() layer
// from a `.field` rule in page.template.html.
struct Stop {
    x: f32,
    y: f32,
    color: egui::Color32,
    radius: f32,
    strength: f32,
}

fn s(x: f32, y: f32, color: egui::Color32, radius: f32, strength: f32) -> Stop {
    Stop { x, y, color, radius, strength }
}

// Renders a small texture with `base` filled in and each stop blended in with a smooth
// (quadratic) falloff from its center, no hard edges or rings since this is computed per pixel
// analytically rather than built from discrete painter shapes. Stretched over a panel with
// bilinear filtering, this reads as a soft blurred wash.
fn make_wash_texture(ctx: &egui::Context, name: &str, base: egui::Color32, stops: &[Stop]) -> egui::TextureHandle {
    const W: usize = 160;
    const H: usize = 112;
    let mut image = egui::ColorImage::new([W, H], base);
    for yi in 0..H {
        for xi in 0..W {
            let px = xi as f32 / W as f32;
            let py = yi as f32 / H as f32;
            let mut r = base.r() as f32;
            let mut g = base.g() as f32;
            let mut b = base.b() as f32;
            for stop in stops {
                let dx = (px - stop.x) * (W as f32 / H as f32).max(1.0);
                let dy = py - stop.y;
                let dist = (dx * dx + dy * dy).sqrt();
                let t = (1.0 - dist / stop.radius).clamp(0.0, 1.0);
                let a = stop.strength * t * t; // quadratic falloff: smooth, no ring edges
                r = r + (stop.color.r() as f32 - r) * a;
                g = g + (stop.color.g() as f32 - g) * a;
                b = b + (stop.color.b() as f32 - b) * a;
            }
            image.pixels[yi * W + xi] = egui::Color32::from_rgb(r.round() as u8, g.round() as u8, b.round() as u8);
        }
    }
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}

// Loosely based on makeGrain() in page.template.html: a texture where each pixel is black or
// white, with alpha drawn from the average of three uniform random numbers (a cheap
// triangular/near-normal distribution), which is what gives real film grain its look instead of
// flat uniform noise.
//
// Two changes from a literal port, both because this is drawn with no antialiasing, unlike the
// browser, which implicitly softens the 160px canvas through devicePixelRatio scaling and a small
// blur filter before compositing it at `opacity:.55`. There is no mix-blend-mode on it in the
// source CSS, only that opacity, so there's no blend mode to port; getting a comparable *look* out
// of a sharp renderer instead means cutting both knobs further than the CSS numbers:
//   - GRAIN_LAYER_OPACITY stacks on top of the per-pixel alpha, same idea as the CSS opacity, but
//     considerably lower (0.55, then 0.22, both still read as too strong once rendered without any
//     of the browser's softening, and the UI-wide 1.5x zoom added afterwards made it worse: it
//     stretches this texture's texels over more physical screen pixels, so each grain speckle got
//     visibly bigger and blockier on top of already being too strong).
//   - GRAIN_JITTER narrows the random spread that drives the alpha (was 150 in the JS), so most
//     pixels land close to mid-grey instead of swinging to full black/white, for a finer texture
//     rather than a scatter of stark dots.
// GRAIN_SUPERSAMPLE renders the texture at a higher native resolution than the 160-point tile it's
// drawn into (still tiled at 160 points, matching the CSS tile size), so the UI zoom has texel
// detail to stretch into instead of blowing up a small number of big flat-colored texels.
const GRAIN_LAYER_OPACITY: f32 = 0.10;
const GRAIN_JITTER: f32 = 55.0;
const GRAIN_SUPERSAMPLE: usize = 2;

fn make_grain_texture(ctx: &egui::Context) -> egui::TextureHandle {
    const N: usize = 160 * GRAIN_SUPERSAMPLE;
    let mut image = egui::ColorImage::new([N, N], egui::Color32::TRANSPARENT);
    let mut seed: u32 = 0x2545_f491;
    let mut rand01 = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    for px in image.pixels.iter_mut() {
        let v = 128.0 + ((rand01() + rand01() + rand01()) / 3.0 - 0.5) * GRAIN_JITTER;
        let base = if v > 128.0 { 255u8 } else { 0u8 };
        let alpha = ((v - 128.0).abs().clamp(0.0, 255.0) * GRAIN_LAYER_OPACITY) as u8;
        *px = egui::Color32::from_rgba_unmultiplied(base, base, base, alpha);
    }
    ctx.load_texture("tape-machine-grain", image, egui::TextureOptions::LINEAR)
}

fn paint_texture_over(painter: &egui::Painter, rect: egui::Rect, tex: &egui::TextureHandle) {
    painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
}

fn paint_grain_tiled(painter: &egui::Painter, rect: egui::Rect, tex: &egui::TextureHandle) {
    // Tile at 160 points, matching the HTML's `background-size:160px 160px` on its 160x160 grain
    // canvas (the texture itself is rendered at GRAIN_SUPERSAMPLE times that resolution, but still
    // mapped across this same 160-point tile, so the tiling frequency matches the CSS regardless).
    const TILE: f32 = 160.0;
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
            let col = if lit { LED_LIT } else if on { LED_ON } else { led_off_color() };
            let (rect, _resp) = ui.allocate_exact_size(egui::Vec2::splat(9.0), egui::Sense::hover());
            ui.painter_at(rect).circle_filled(rect.center(), 3.6, col);
        }
    });
}

fn draw_hysteresis_scope(ui: &mut egui::Ui, trace: &[[f32; 2]]) {
    let size = egui::Vec2::new(ui.available_width().max(200.0), 170.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 200));

    let cx = rect.center().x;
    let cy = rect.center().y;
    painter.line_segment([egui::pos2(cx, rect.top() + 8.0), egui::pos2(cx, rect.bottom() - 8.0)], egui::Stroke::new(1.0f32, line_color()));
    painter.line_segment([egui::pos2(rect.left() + 8.0, cy), egui::pos2(rect.right() - 8.0, cy)], egui::Stroke::new(1.0f32, line_color()));
    painter.text(egui::pos2(rect.right() - 16.0, cy - 12.0), egui::Align2::CENTER_CENTER, "H", egui::FontId::monospace(11.0), MUTED);
    painter.text(egui::pos2(cx + 12.0, rect.top() + 14.0), egui::Align2::CENTER_CENTER, "M", egui::FontId::monospace(11.0), MUTED);

    if trace.len() < 4 {
        painter.text(rect.left_bottom() + egui::vec2(14.0, -14.0), egui::Align2::LEFT_BOTTOM, "Waiting for signal", egui::FontId::monospace(11.0), MUTED);
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

fn draw_frequency_response(ui: &mut egui::Ui, mag: &[f64]) {
    let size = egui::Vec2::new(ui.available_width().max(260.0), 190.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 200));

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

pub fn create(params: Arc<TapeMachineParams>, editor_state: Arc<EguiState>, scope: Arc<ScopeState>) -> Option<Box<dyn Editor>> {
    let grain_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let mast_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let deck_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let paper_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let record_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let playback_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    let transport_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    // Deck's height is matched to the Hysteresis+Frequency-response column rather than a fixed
    // guess, so the two columns bottom out evenly like the reference page. egui lays out columns
    // left-to-right in one pass, so Deck (the left column) can't know the right column's height
    // until after it's drawn; this caches the right column's rendered height and applies it to
    // Deck one frame later, which is the standard immediate-mode-GUI way to match sibling sizes
    // and settles invisibly since the editor repaints continuously.
    let right_col_height: Mutex<f32> = Mutex::new(500.0);

    create_egui_editor(
        editor_state.clone(),
        (),
        |ctx, _| {
            install_fonts(ctx);
            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
            style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
            ctx.set_style(style);
            // The style guide's own rem values for labels/captions/knob text (0.7-0.9rem) are
            // genuinely small in CSS terms, which only reads as "normal" in a browser because
            // screenshots are typically captured at a higher effective DPI/zoom than a 1:1-point
            // render here produces. UI_ZOOM scales the whole interface (text, spacing, everything,
            // proportions untouched) rather than hand-inflating every individual font size. This is
            // a starting guess, not a measured match to the reference screenshots, so it may still
            // need to go up or down.
            ctx.set_zoom_factor(1.5);
        },
        move |egui_ctx, setter, _state| {
            egui_ctx.request_repaint();

            macro_rules! get_tex {
                ($slot:expr, $name:expr, $base:expr, $stops:expr) => {{
                    let mut guard = $slot.lock().unwrap();
                    if guard.is_none() {
                        *guard = Some(make_wash_texture(egui_ctx, $name, $base, $stops));
                    }
                    guard.as_ref().unwrap().clone()
                }};
            }
            {
                let mut g = grain_tex.lock().unwrap();
                if g.is_none() {
                    *g = Some(make_grain_texture(egui_ctx));
                }
            }

            ResizableWindow::new("tape-machine-editor").min_size(egui::Vec2::new(620.0, 600.0)).show(egui_ctx, editor_state.as_ref(), |ui| {
                ui.visuals_mut().override_text_color = Some(INK);
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 10.0);
                let outer = egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let machine_id = params.machine.value().id();
                        let tape_id = params.tape.value().id();
                        let speed_ips = params.speed.value().ips();
                        let machine = data::machine_by_id(machine_id);
                        let tape = data::tape_by_id(tape_id);
                        let bypassed = params.bypass.value();

                        // ---------- Masthead ----------
                        egui::Frame::new().fill(GROUND2).inner_margin(egui::Margin::symmetric(22, 20)).show(ui, |ui| {
                            let rect = field_rect(ui, 160.0);
                            let tex = get_tex!(
                                mast_tex,
                                "tape-machine-mast",
                                GROUND2,
                                &[s(0.3, 0.7, C2, 0.75, 0.55), s(0.72, 0.38, C1, 0.7, 0.5), s(0.55, 0.8, C3, 0.65, 0.4), s(0.9, 0.9, C4, 0.6, 0.35)]
                            );
                            paint_texture_over(&ui.painter_at(rect), rect, &tex);

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
                            ui.add_space(6.0);
                            // Matches the style guide's `.mast h1{font:600 clamp(3rem,11vw,8rem)...}`:
                            // weight 600 (Semibold, not Black) and a size that scales with the
                            // panel's width between 48px and 128px, mirroring the CSS clamp/vw rule
                            // rather than a fixed point size.
                            let title_px = (ui.available_width() * 0.11).clamp(48.0, 128.0);
                            ui.label(egui::RichText::new("Tape machine").font(mori(title_px, MORI_SEMIBOLD)).color(INK));
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Five tape machines modelled down to the circuit level, with hiss, wow and flutter to match.")
                                    .size(13.0)
                                    .color(INK),
                            );
                        });
                        ui.add_space(10.0);

                        // ---------- Row 1: Deck (left, wider) beside Hysteresis + Frequency response (right) ----------
                        ui.columns(2, |cols| {
                            let deck_target_h = *right_col_height.lock().unwrap();
                            panel_frame(DECK_BG).show(&mut cols[0], |ui| {
                                ui.set_min_height(deck_target_h);
                                let rect = field_rect(ui, deck_target_h);
                                let tex = get_tex!(
                                    deck_tex,
                                    "tape-machine-deck",
                                    DECK_BG,
                                    &[s(0.22, 0.7, C1, 0.75, 0.6), s(0.8, 0.24, C2, 0.7, 0.5), s(0.58, 0.22, C3, 0.6, 0.4), s(0.15, 0.88, C4, 0.55, 0.4)]
                                );
                                paint_texture_over(&ui.painter_at(rect), rect, &tex);

                                big_heading(ui, "Deck", PAPER);
                                ui.add_space(6.0);

                                // Tape / Bypass toggle
                                ui.horizontal(|ui| {
                                    if seg_button(ui, "Tape", !bypassed, PAPER, PAPER).clicked() && bypassed {
                                        setter.begin_set_parameter(&params.bypass);
                                        setter.set_parameter(&params.bypass, false);
                                        setter.end_set_parameter(&params.bypass);
                                    }
                                    if seg_button(ui, "Bypass", bypassed, PAPER, PAPER).clicked() && !bypassed {
                                        setter.begin_set_parameter(&params.bypass);
                                        setter.set_parameter(&params.bypass, true);
                                        setter.end_set_parameter(&params.bypass);
                                    }
                                });
                                ui.add_space(8.0);

                                enum_combo::<MachineChoice>(ui, "machine", "Machine", &params.machine, setter, PAPER, machine_title);
                                ui.label(egui::RichText::new(machine_note(machine_id)).monospace().size(10.0).color(PAPER.gamma_multiply(0.85)));
                                ui.add_space(6.0);
                                enum_combo::<TapeChoice>(ui, "tape", "Tape", &params.tape, setter, PAPER, |n| n);
                                ui.label(
                                    egui::RichText::new(format!("{} Bias and level adjust to match.", tape_note(tape_id)))
                                        .monospace()
                                        .size(10.0)
                                        .color(PAPER.gamma_multiply(0.85)),
                                );
                                ui.add_space(8.0);

                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        uppercase_label(ui, "Tape speed (ips)", PAPER);
                                        ui.horizontal(|ui| {
                                            for &ips in machine.speeds {
                                                let active = (ips - speed_ips).abs() < 0.01;
                                                if seg_button(ui, speed_name(ips), active, PAPER, PAPER).clicked() && !active {
                                                    setter.begin_set_parameter(&params.speed);
                                                    setter.set_parameter(&params.speed, speed_choice_for(ips));
                                                    setter.end_set_parameter(&params.speed);
                                                }
                                            }
                                        });
                                    });
                                    ui.add_space(18.0);
                                    ui.vertical(|ui| {
                                        uppercase_label(ui, "Oversampling", PAPER);
                                        seg_button(ui, "8×", true, PAPER, PAPER); // fixed; see module doc
                                    });
                                });
                                ui.add_space(10.0);

                                uppercase_label(ui, "Output level", PAPER);
                                draw_vu_row(ui, "L", scope.peak(0), PAPER);
                                draw_vu_row(ui, "R", scope.peak(1), PAPER);
                                if bypassed {
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new("Bypassed: dry signal passing through unprocessed.").monospace().size(10.0).color(PAPER.gamma_multiply(0.85)));
                                }
                            });

                            let right_resp = cols[1]
                                .vertical(|ui| {
                                    panel_frame(PAPER).show(ui, |ui| {
                                        let rect = field_rect(ui, 230.0);
                                        let tex = get_tex!(paper_tex, "tape-machine-paper", PAPER, &[s(0.5, 0.35, C2, 0.7, 0.3), s(0.5, 0.85, C6, 0.7, 0.3)]);
                                        paint_texture_over(&ui.painter_at(rect), rect, &tex);
                                        big_heading(ui, "Hysteresis", INK);
                                        section_label(ui, "", "H in \u{2192} M out", INK);
                                        caption(ui, "Record signal (x) against the tape's magnetism (y). Raise Input or lower Bias to see the loop bend open.", MUTED);
                                        draw_hysteresis_scope(ui, &scope.snapshot_trace());
                                    });
                                    ui.add_space(10.0);
                                    panel_frame(PAPER).show(ui, |ui| {
                                        let rect = field_rect(ui, 250.0);
                                        let tex = get_tex!(paper_tex, "tape-machine-paper", PAPER, &[s(0.5, 0.35, C2, 0.7, 0.3), s(0.5, 0.85, C6, 0.7, 0.3)]);
                                        paint_texture_over(&ui.painter_at(rect), rect, &tex);
                                        big_heading(ui, "Frequency response", INK);
                                        section_label(ui, "", &format!("{} ips", speed_name(speed_ips)), INK);
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
                                })
                                .response;
                            *right_col_height.lock().unwrap() = right_resp.rect.height();
                        });
                        ui.add_space(10.0);

                        // ---------- Row 2: Record / Playback / Transport, three across ----------
                        ui.columns(3, |cols| {
                            panel_frame(RECORD_BG).show(&mut cols[0], |ui| {
                                let rect = field_rect(ui, 300.0);
                                let tex = get_tex!(record_tex, "tape-machine-record", RECORD_BG, &[s(0.3, 0.3, C1, 0.8, 0.5), s(0.8, 0.9, C3, 0.7, 0.4)]);
                                paint_texture_over(&ui.painter_at(rect), rect, &tex);
                                section_label(ui, "Record · magnetise", "I\u{2013}III", PAPER);
                                big_heading(ui, "Record", PAPER);
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
                                let rect = field_rect(ui, 300.0);
                                let tex = get_tex!(playback_tex, "tape-machine-playback", PLAYBACK_BG, &[s(0.7, 0.2, C3, 0.8, 0.5), s(0.2, 0.9, C6, 0.7, 0.4)]);
                                paint_texture_over(&ui.painter_at(rect), rect, &tex);
                                section_label(ui, "Playback · head losses", "IV\u{2013}V", INK);
                                big_heading(ui, "Playback", INK);
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
                                let rect = field_rect(ui, 300.0);
                                let tex = get_tex!(transport_tex, "tape-machine-transport", TRANSPORT_BG, &[s(0.25, 0.25, C2, 0.8, 0.5), s(0.8, 0.8, C4, 0.7, 0.4)]);
                                paint_texture_over(&ui.painter_at(rect), rect, &tex);
                                section_label(ui, "Transport · speed error", "VI", PAPER);
                                big_heading(ui, "Transport", PAPER);
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
                    paint_grain_tiled(&ui.painter_at(outer.response.rect), outer.response.rect, tex);
                }
            });
        },
    )
}
