// egui editor for the Tape Machine plugin, matching the web app (page.template.html) closely:
// the "Tape machine" masthead and subhead, big section headings inside each stage panel (not
// just the small mono label row), segmented tape-speed buttons filtered to the current machine's
// valid speeds, a real Bypass toggle, richer machine descriptions, PP Mori as the display/body
// typeface, three live readouts (L/R peak meters, the hysteresis scope, an analytic frequency-
// response plot), and two fidelity passes the previous attempt only approximated:
//   - Each panel's background wash is a real browser-rendered PNG (assets/backgrounds/*.png),
//     captured straight out of the working HTML with headless Chromium rather than re-derived
//     from the CSS by hand, then stretched over the panel with bilinear filtering. An earlier
//     version rendered a small texture pixel-by-pixel with analytic radial falloff to approximate
//     the CSS radial-gradient() rules; that approximation produced real, repeated bugs (visible
//     banding, coverage gaps when a panel's measured height grew past the texture's guess), which
//     the captured PNGs sidestep entirely since they're exact pixel data, not an approximation.
//   - The actual grain algorithm from page.template.html's makeGrain(): a 160x160 texture where
//     each pixel is black or white with alpha drawn from the same triangular-ish distribution
//     (average of three uniform randoms), tiled over the whole window. This one stayed procedural
//     (not baked into the panel PNGs) because the real grain is a page-wide overlay sitting above
//     every panel AND its foreground content, not something that belongs to any one panel's
//     background - baking it per-panel would double up with this overlay and still not cover text.
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
use nih_plug_egui::{create_egui_editor, egui, EguiState};

use crate::data;
use crate::knob::{knob, panel_frame, Tone, ACCENT, INK, PAPER, PANEL_MARGIN, UI_SCALE};
use crate::scope::ScopeState;
use crate::{map_gap, map_spacing, map_thickness, MachineChoice, SpeedChoice, TapeChoice, TapeMachineParams};

// Palette lifted from :root in page.template.html.
// --ground2, --c1, --c2, --c3 and --c5 aren't needed as Color32 constants any more: they only
// ever fed per-panel Frame fills, and every panel's fill is now TRANSPARENT so the one composite
// backdrop (FULL_BG_PNG, which already has these colours baked in) shows through instead.
const GROUND: egui::Color32 = egui::Color32::from_rgb(0xc4, 0xc4, 0xc2); // --ground (page bg)
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
    egui::FontId::new(size * UI_SCALE, egui::FontFamily::Name(weight.into()))
}

// Centralizes the many `.monospace().size(x)` / `FontId::monospace(x)` call sites below so they
// all pick up UI_SCALE the same way `mori()` does, rather than each needing its own `* UI_SCALE`.
fn mono(size: f32) -> egui::FontId {
    egui::FontId::monospace(size * UI_SCALE)
}

pub fn default_state() -> Arc<EguiState> {
    // With no ScrollArea and no resize handle (see create()), this is the plugin's one and only
    // window size, so it has to be large enough to hold the full layout at UI_SCALE without
    // clipping anything. I can't render egui here to measure it, so this is a worked estimate, not
    // a measured one: the masthead, the Deck/Hysteresis/Frequency-response row, and the
    // Record/Playback/Transport row, each summed from their own margins/headings/captions/controls
    // at UI_SCALE=1.6, land near 420 + 1200 + 460 points tall plus outer margins, and the CSS
    // layout this mirrors was designed for roughly 1180 points wide, which at UI_SCALE becomes
    // roughly 1900. Rounded up for safety margin on both axes. Worth flagging plainly: at this
    // scale and with no scrolling, that's a genuinely large fixed window for a plugin (most
    // run well under 1200x900) - if that's too big for comfortable use in your DAW, the fix is
    // either a lower UI_SCALE (knob.rs) or going back to a scrollable/resizable window, not a
    // smaller number here, since a smaller number here would just clip content again.
    EguiState::from_size(1900, 2400)
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

fn big_heading(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text).font(mori(27.0, MORI_SEMIBOLD)).color(color));
}

fn section_label(ui: &mut egui::Ui, title: &str, numeral: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title.to_uppercase()).font(mono(10.5)).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(numeral).font(mono(10.5)).color(color));
        });
    });
}

fn uppercase_label(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(text.to_uppercase()).font(mono(9.5)).color(color));
}

// A pressed/unpressed button matching the web app's `.seg button[aria-pressed]` look: filled when
// active, outlined when not.
fn seg_button(ui: &mut egui::Ui, label: &str, active: bool, fg: egui::Color32, fill_when_active: egui::Color32) -> egui::Response {
    let (text_color, bg) = if active { (INK, fill_when_active) } else { (fg, egui::Color32::TRANSPARENT) };
    let button = egui::Button::new(egui::RichText::new(label).font(mono(10.5)).color(text_color))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0f32, fg.gamma_multiply(0.5)))
        .corner_radius(0.0);
    ui.add(button)
}

// Groups the dropdown's options under headings, matching the reference HTML's `<optgroup>`
// structure on #machSel and #tapeSel ("Tape machines" / "Home deck", "Reel-to-reel tapes" /
// "Cassette-style formulas"). `groups` lists each heading with its options' #[id = "..."] strings
// (what T::variants() returns) in the order they should display - NOT necessarily the enum's own
// declaration order. That distinction matters for MachineChoice specifically: its variants are
// declared J37, A80, A77, Tc260, Atr100 (Tc260 in the middle), because nih_plug's EnumParam
// persists a saved preset's choice as that declaration index, so reordering the enum to put
// Tc260 last would silently remap anyone's already-saved presets to a different machine. Grouping
// the display here instead - looking up each listed id's real index with `position()` rather than
// assuming display order matches it - gets the HTML's grouping without that hazard.
fn enum_combo<T>(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    param: &EnumParam<T>,
    setter: &ParamSetter,
    text_color: egui::Color32,
    groups: &[(&str, &[&'static str])],
    display: impl Fn(&'static str) -> &'static str,
) where
    T: Enum + PartialEq + Copy + 'static,
{
    let current = param.value();
    let variants = T::variants();
    let current_idx = current.to_index();
    uppercase_label(ui, label, text_color);
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width())
        .selected_text(egui::RichText::new(display(variants[current_idx])).font(mono(11.0)).color(INK))
        .show_ui(ui, |ui| {
            for (heading, ids) in groups {
                ui.add_enabled(false, egui::Label::new(egui::RichText::new(heading.to_uppercase()).font(mono(9.0)).color(MUTED)));
                for id in *ids {
                    let Some(i) = variants.iter().position(|v| v == id) else { continue };
                    let selected = i == current_idx;
                    if ui.selectable_label(selected, display(id)).clicked() && !selected {
                        setter.begin_set_parameter(param);
                        setter.set_parameter(param, T::from_index(i));
                        setter.end_set_parameter(param);
                    }
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

// ---------- backdrop: one composite screenshot of the whole panel, not seven per-panel washes ----------
//
// A previous version captured each panel's `.field` gradient wash as its own PNG (seven separate
// textures, one per panel) and painted each over its own panel rect every frame. This version
// instead uses one screenshot of the entire reference page's content area (every panel's wash,
// already laid out with the real gaps and proportions between them, baked into one image) and
// paints it once as a backdrop behind the whole layout, with every panel's own Frame fill now
// TRANSPARENT so that single image shows through. This is simpler and guarantees the exact
// relative positioning the reference page has between panels (no per-panel seams to keep in
// sync), at the cost of one piece of real risk worth flagging plainly: a single static image
// can't adapt if egui's own live layout ends up a different total height than the captured page's
// was (different font metrics, a future content change, etc). The seven-PNG version was immune to
// that because each wash self-corrected to its own panel's measured height every frame; this
// version only self-corrects the TOTAL height (see content_h below), not each panel's internal
// proportions within that total, so if that ever drifts, re-capture this PNG against the current
// build rather than assuming it still lines up.
// Regenerate after any layout or palette change in the source HTML with:
// `python3 tools/capture_backgrounds.py path/to/Them_Changes_Tape_Machine.html` and rename its
// output, or screenshot `#app`/`.wrap` directly as one clip (see that script's docstring).
const FULL_BG_PNG: &[u8] = include_bytes!("../assets/backgrounds/full.png");

fn load_png_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let decoded = image::load_from_memory(bytes).expect("embedded panel background PNG failed to decode").to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
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
        ui.label(egui::RichText::new(label).font(mono(10.0)).color(text_color));
        for i in 0..16 {
            let on = i < n;
            let lit = on && i == n - 1 && db > -6.0;
            let col = if lit { LED_LIT } else if on { LED_ON } else { led_off_color() };
            let (rect, _resp) = ui.allocate_exact_size(egui::Vec2::splat(9.0 * UI_SCALE), egui::Sense::hover());
            ui.painter_at(rect).circle_filled(rect.center(), 3.6 * UI_SCALE, col);
        }
    });
}

fn draw_hysteresis_scope(ui: &mut egui::Ui, trace: &[[f32; 2]]) {
    let size = egui::Vec2::new(ui.available_width().max(200.0 * UI_SCALE), 170.0 * UI_SCALE);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 200));

    let cx = rect.center().x;
    let cy = rect.center().y;
    painter.line_segment([egui::pos2(cx, rect.top() + 8.0 * UI_SCALE), egui::pos2(cx, rect.bottom() - 8.0 * UI_SCALE)], egui::Stroke::new(1.0f32, line_color()));
    painter.line_segment([egui::pos2(rect.left() + 8.0 * UI_SCALE, cy), egui::pos2(rect.right() - 8.0 * UI_SCALE, cy)], egui::Stroke::new(1.0f32, line_color()));
    painter.text(egui::pos2(rect.right() - 16.0 * UI_SCALE, cy - 12.0 * UI_SCALE), egui::Align2::CENTER_CENTER, "H", mono(11.0), MUTED);
    painter.text(egui::pos2(cx + 12.0 * UI_SCALE, rect.top() + 14.0 * UI_SCALE), egui::Align2::CENTER_CENTER, "M", mono(11.0), MUTED);

    if trace.len() < 4 {
        painter.text(rect.left_bottom() + egui::vec2(14.0 * UI_SCALE, -14.0 * UI_SCALE), egui::Align2::LEFT_BOTTOM, "Waiting for signal", mono(11.0), MUTED);
        return;
    }

    let mut mx = 0.05f32;
    let mut my = 0.05f32;
    for p in trace {
        mx = mx.max(p[0].abs());
        my = my.max(p[1].abs());
    }
    let half_w = (rect.width() / 2.0 - 14.0 * UI_SCALE).max(1.0);
    let half_h = (rect.height() / 2.0 - 14.0 * UI_SCALE).max(1.0);
    let sx = half_w / mx;
    let sy = half_h / my;
    let pts: Vec<egui::Pos2> = trace.iter().map(|p| egui::pos2(cx + p[0] * sx, cy - p[1] * sy)).collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6f32, ACCENT)));
}

fn draw_frequency_response(ui: &mut egui::Ui, mag: &[f64]) {
    let size = egui::Vec2::new(ui.available_width().max(260.0 * UI_SCALE), 190.0 * UI_SCALE);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0xe8, 0xe6, 0xe1, 200));

    let l = rect.left() + 42.0 * UI_SCALE;
    let r = rect.right() - 10.0 * UI_SCALE;
    let t = rect.top() + 10.0 * UI_SCALE;
    let b = rect.bottom() - 20.0 * UI_SCALE;
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
        painter.text(egui::pos2(x, b + 11.0 * UI_SCALE), egui::Align2::CENTER_CENTER, label, mono(9.0), MUTED);
    }
    let mut db = db_min;
    while db <= db_max + 0.01 {
        let y = fy(db);
        painter.line_segment([egui::pos2(l, y), egui::pos2(r, y)], egui::Stroke::new(1.0f32, line_color()));
        painter.text(egui::pos2(l - 5.0 * UI_SCALE, y), egui::Align2::RIGHT_CENTER, format!("{}", db as i32), mono(9.0), MUTED);
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
    let full_tex: Mutex<Option<egui::TextureHandle>> = Mutex::new(None);
    // Deck's height is matched to the Hysteresis+Frequency-response column rather than a fixed
    // guess, so the two columns bottom out evenly like the reference page. egui lays out columns
    // left-to-right in one pass, so Deck (the left column) can't know the right column's height
    // until after it's drawn; this caches the right column's rendered height and applies it to
    // Deck one frame later, which is the standard immediate-mode-GUI way to match sibling sizes
    // and settles invisibly since the editor repaints continuously.
    let right_col_height: Mutex<f32> = Mutex::new(500.0 * UI_SCALE);
    // The one composite backdrop (FULL_BG_PNG) is painted before the real content is laid out, so
    // its height has to be a guess rather than a measurement on the very first frame; this caches
    // the whole content area's actual rendered height from the previous frame (same one-frame-delay
    // technique as right_col_height above) so the backdrop always covers at least as much as the
    // content turned out to need. Seeded from the composite image's own captured aspect ratio
    // (973.56 / 1180 of the design width) against the default window's content width, rather than a
    // round number, so the very first frame is already close before the real measurement replaces it.
    let content_h: Mutex<f32> = Mutex::new(1530.0 * UI_SCALE);

    create_egui_editor(
        editor_state.clone(),
        (),
        |ctx, _| {
            install_fonts(ctx);
            let mut style = (*ctx.style()).clone();
            style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
            style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
            ctx.set_style(style);
        },
        move |egui_ctx, setter, _state| {
            egui_ctx.request_repaint();

            macro_rules! get_tex {
                ($slot:expr, $name:expr, $png:expr) => {{
                    let mut guard = $slot.lock().unwrap();
                    if guard.is_none() {
                        *guard = Some(load_png_texture(egui_ctx, $name, $png));
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

            // A plain CentralPanel instead of ResizableWindow: ResizableWindow's only job beyond
            // CentralPanel is the draggable resize corner (confirmed by reading its source), so
            // dropping it and leaving editor_state's size untouched gives a fixed-size, non-
            // resizable window, as asked for. Content lays out directly (no ScrollArea) so
            // everything is visible as soon as the plugin opens, as asked for, which only works if
            // default_state()'s window is tall and wide enough to hold the full layout at UI_SCALE
            // without clipping; see that function's comment for how that number was chosen.
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(egui_ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(INK);
                ui.spacing_mut().item_spacing = egui::vec2(14.0 * UI_SCALE, 10.0 * UI_SCALE);
                let outer = egui::Frame::new().fill(GROUND).inner_margin(egui::Margin::same((14.0 * UI_SCALE).round() as i8)).show(ui, |ui| {
                    // One composite backdrop, painted once behind the whole layout below, instead of
                    // seven separate per-panel washes (see FULL_BG_PNG's comment above). Painted
                    // first, using last frame's measured total height, so every panel's content
                    // (added further down in this same ui/layer) draws on top of it.
                    {
                        let rect = field_rect(ui, *content_h.lock().unwrap());
                        let tex = get_tex!(full_tex, "tape-machine-full-bg", FULL_BG_PNG);
                        paint_texture_over(&ui.painter_at(rect), rect, &tex);
                    }
                    let content_resp = ui.vertical(|ui| {
                        let machine_id = params.machine.value().id();
                        let tape_id = params.tape.value().id();
                        let speed_ips = params.speed.value().ips();
                        let machine = data::machine_by_id(machine_id);
                        let tape = data::tape_by_id(tape_id);
                        let bypassed = params.bypass.value();

                        // ---------- Masthead ----------
                        // TRANSPARENT fill: the composite backdrop above already carries the
                        // masthead's own gradient wash in exactly this spot.
                        egui::Frame::new().fill(egui::Color32::TRANSPARENT).inner_margin(egui::Margin::symmetric((22.0 * UI_SCALE).round() as i8, (20.0 * UI_SCALE).round() as i8)).show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} · {} ips · {}", machine.label, speed_name(speed_ips), tape.label))
                                        .font(mono(10.5))
                                        .color(INK),
                                );
                                ui.add_space(14.0 * UI_SCALE);
                                ui.label(egui::RichText::new(format!("Target bias · {:.1} kHz", machine.fb / 1000.0)).font(mono(10.5)).color(INK));
                                ui.add_space(14.0 * UI_SCALE);
                                ui.label(egui::RichText::new("8× oversampled").font(mono(10.5)).color(INK));
                            });
                            ui.add_space(6.0 * UI_SCALE);
                            // Matches the current style guide's `.mast h1{font:600 clamp(2rem,calc(
                            // var(--panel-w) * 0.042),3.25rem)...}`: weight 600 (Semibold, not
                            // Black), scaled from the design's own FIXED panel width rather than the
                            // live window size. An earlier version here scaled from
                            // `ui.available_width()`, which mirrored a bug the reference HTML had
                            // and later fixed: that live width is already UI_SCALE-scaled (everything
                            // in this editor is), so feeding it through another formula and then
                            // through mori() (which multiplies by UI_SCALE again) didn't track the
                            // fixed design size the way a `calc(var(--panel-w) * ...)` rule does -
                            // it tracked the window instead, which here never actually changes, but
                            // the formula read as if it should. DESIGN_PANEL_W_CSS is the same fixed
                            // design width the HTML's --panel-w token holds; mori() still applies
                            // UI_SCALE on top of this, same as the comment below always intended.
                            const DESIGN_PANEL_W_CSS: f32 = 1180.0;
                            let title_px = (DESIGN_PANEL_W_CSS * 0.042).clamp(32.0, 52.0);
                            ui.label(egui::RichText::new("Tape machine").font(mori(title_px, MORI_SEMIBOLD)).color(INK));
                            // No subhead paragraph: the current reference HTML removed every
                            // explanatory line (masthead subhead, machine/tape notes, per-stage
                            // captions) and kept only functional labels and readouts, so this editor
                            // drops the equivalent prose too rather than being the one place that
                            // still carries it.
                        });
                        ui.add_space(10.0 * UI_SCALE);

                        // ---------- Row 1: Deck (left, wider) beside Hysteresis + Frequency response (right) ----------
                        ui.columns(2, |cols| {
                            // right_col_height is the right column's OUTER size (its own two panels'
                            // margins already baked in by egui's layout flow). Deck's own panel_frame
                            // adds its own margins on top of whatever its CONTENT height is, so
                            // forcing content height straight to deck_target_h would make Deck's
                            // outer size deck_target_h + Deck's margins - taller than the right
                            // column, not equal to it. Subtracting Deck's own margins first is what
                            // actually makes the two columns' outer heights match.
                            let deck_target_h = *right_col_height.lock().unwrap();
                            let deck_content_target = (deck_target_h - 2.0 * PANEL_MARGIN).max(0.0);
                            panel_frame(egui::Color32::TRANSPARENT).show(&mut cols[0], |ui| {
                                ui.set_min_height(deck_content_target);

                                big_heading(ui, "Deck", PAPER);
                                ui.add_space(6.0 * UI_SCALE);

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
                                ui.add_space(8.0 * UI_SCALE);

                                // Grouped like the current reference HTML's #machSel/#tapeSel
                                // <optgroup> structure (see enum_combo's doc comment for why the
                                // Machine groups are listed by id rather than relying on the enum's
                                // own declared order).
                                enum_combo::<MachineChoice>(
                                    ui,
                                    "machine",
                                    "Machine",
                                    &params.machine,
                                    setter,
                                    PAPER,
                                    &[("Tape machines", &["j37", "a80", "a77", "atr100"]), ("Home deck", &["tc260"])],
                                    machine_title,
                                );
                                ui.add_space(6.0 * UI_SCALE);
                                enum_combo::<TapeChoice>(
                                    ui,
                                    "tape",
                                    "Tape",
                                    &params.tape,
                                    setter,
                                    PAPER,
                                    &[("Reel-to-reel tapes", &["ferric", "studio", "sm468", "a632"]), ("Cassette-style formulas", &["cobalt", "chrome", "metal"])],
                                    |n| n,
                                );
                                ui.add_space(8.0 * UI_SCALE);

                                // No Oversampling control: the current reference HTML removed it
                                // entirely (the masthead corner readout still reports the fixed
                                // factor as a read-only value - see "8x oversampled" above - but
                                // there's no toggle for it there either, same reasoning as this
                                // module's doc comment already gave for the old non-interactive chip).
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
                                ui.add_space(10.0 * UI_SCALE);

                                uppercase_label(ui, "Output level", PAPER);
                                draw_vu_row(ui, "L", scope.peak(0), PAPER);
                                draw_vu_row(ui, "R", scope.peak(1), PAPER);
                                if bypassed {
                                    ui.add_space(6.0 * UI_SCALE);
                                    ui.label(egui::RichText::new("Bypassed: dry signal passing through unprocessed.").font(mono(10.0)).color(PAPER.gamma_multiply(0.85)));
                                }
                            });

                            let right_resp = cols[1]
                                .vertical(|ui| {
                                    panel_frame(egui::Color32::TRANSPARENT).show(ui, |ui| {
                                        big_heading(ui, "Hysteresis", INK);
                                        section_label(ui, "", "H in \u{2192} M out", INK);
                                        draw_hysteresis_scope(ui, &scope.snapshot_trace());
                                    });
                                    ui.add_space(10.0 * UI_SCALE);
                                    panel_frame(egui::Color32::TRANSPARENT).show(ui, |ui| {
                                        big_heading(ui, "Frequency response", INK);
                                        section_label(ui, "", &format!("{} ips", speed_name(speed_ips)), INK);
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
                        ui.add_space(10.0 * UI_SCALE);

                        // ---------- Row 2: Record / Playback / Transport, three across ----------
                        ui.columns(3, |cols| {
                            panel_frame(egui::Color32::TRANSPARENT).show(&mut cols[0], |ui| {
                                section_label(ui, "Record · magnetise", "I\u{2013}III", PAPER);
                                big_heading(ui, "Record", PAPER);
                                egui::Grid::new("record-knobs").spacing(egui::vec2(6.0 * UI_SCALE, 8.0 * UI_SCALE)).show(ui, |ui| {
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

                            panel_frame(egui::Color32::TRANSPARENT).show(&mut cols[1], |ui| {
                                section_label(ui, "Playback · head losses", "IV\u{2013}V", INK);
                                big_heading(ui, "Playback", INK);
                                egui::Grid::new("playback-knobs").spacing(egui::vec2(6.0 * UI_SCALE, 8.0 * UI_SCALE)).show(ui, |ui| {
                                    knob(ui, "Spacing", &fmt_pct(params.spacing.value()), Tone::Light, &params.spacing, setter);
                                    knob(ui, "Coating", &fmt_pct(params.thickness.value()), Tone::Light, &params.thickness, setter);
                                    ui.end_row();
                                    knob(ui, "Gap", &fmt_pct(params.gap.value()), Tone::Light, &params.gap, setter);
                                    knob(ui, "Output", &fmt_db(params.output.value()), Tone::Light, &params.output, setter);
                                    ui.end_row();
                                });
                            });

                            panel_frame(egui::Color32::TRANSPARENT).show(&mut cols[2], |ui| {
                                section_label(ui, "Transport · speed error", "VI", PAPER);
                                big_heading(ui, "Transport", PAPER);
                                egui::Grid::new("transport-knobs").spacing(egui::vec2(6.0 * UI_SCALE, 8.0 * UI_SCALE)).show(ui, |ui| {
                                    knob(ui, "Wow", &fmt_pct(params.wow.value()), Tone::Dark, &params.wow, setter);
                                    knob(ui, "Flutter", &fmt_pct(params.flutter.value()), Tone::Dark, &params.flutter, setter);
                                    ui.end_row();
                                    knob(ui, "Hiss", &fmt_pct(params.hiss.value()), Tone::Dark, &params.hiss, setter);
                                    ui.end_row();
                                });
                            });
                        });
                    })
                    .response;
                    *content_h.lock().unwrap() = content_resp.rect.height();
                });

                if let Some(tex) = grain_tex.lock().unwrap().as_ref() {
                    paint_grain_tiled(&ui.painter_at(outer.response.rect), outer.response.rect, tex);
                }
            });
        },
    )
}
