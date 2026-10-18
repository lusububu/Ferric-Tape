// Tape Machine VST3: a Rust/nih-plug port of the tape-core.js DSP engine from the "Them Changes"
// tape-machine web app. The circuit modelling, hysteresis solver, EQ/zpk-to-biquad conversion,
// transport (wow/flutter) model, and per-machine calibration tables are all ported line-for-line
// from that JS core; see data.rs/channel.rs/machine.rs for the module-by-module mapping back to
// tape-core.js. Parameter curves (the knob -> physical-unit mappings below) are copied from the
// PARAMS table in the web app's page.template.html, so the plugin's knobs behave the same way.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::Arc;

mod biquad;
mod channel;
mod circuits;
mod data;
mod editor;
mod headloss;
mod hysteresis;
mod knob;
mod machine;
mod scope;
mod stages;
mod transport;
mod zpk;

use machine::TapeMachine;
use scope::ScopeState;

const OVERSAMPLE: usize = 8;

struct TapeMachinePlugin {
    params: Arc<TapeMachineParams>,
    tm: Option<TapeMachine>,
    last_key: (MachineChoice, f32),
    scope: Arc<ScopeState>,
}

#[derive(Params)]
pub(crate) struct TapeMachineParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "machine"]
    pub machine: EnumParam<MachineChoice>,
    #[id = "tape"]
    pub tape: EnumParam<TapeChoice>,
    #[id = "speed"]
    pub speed: EnumParam<SpeedChoice>,

    #[id = "input"]
    pub input: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,

    // Ms / a / k / c of the Jiles-Atherton model, labelled Ceiling/Knee/Grip/Give in the web UI
    #[id = "ceiling"]
    pub ceiling: FloatParam,
    #[id = "knee"]
    pub knee: FloatParam,
    #[id = "grip"]
    pub grip: FloatParam,
    #[id = "give"]
    pub give: FloatParam,

    #[id = "spacing"]
    pub spacing: FloatParam,
    #[id = "thickness"]
    pub thickness: FloatParam,
    #[id = "gap"]
    pub gap: FloatParam,

    #[id = "wow"]
    pub wow: FloatParam,
    #[id = "flutter"]
    pub flutter: FloatParam,
    #[id = "hiss"]
    pub hiss: FloatParam,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub(crate) enum MachineChoice {
    #[id = "j37"]
    #[name = "Stellan J-37"]
    J37,
    #[id = "a80"]
    #[name = "Stellan A-80"]
    A80,
    #[id = "a77"]
    #[name = "Arvox A-7"]
    A77,
    #[id = "tc260"]
    #[name = "Tonica TC-6"]
    Tc260,
    #[id = "atr100"]
    #[name = "Apexa MT-100"]
    Atr100,
}

impl MachineChoice {
    fn id(&self) -> &'static str {
        match self {
            MachineChoice::J37 => "j37",
            MachineChoice::A80 => "a80",
            MachineChoice::A77 => "a77",
            MachineChoice::Tc260 => "tc260",
            MachineChoice::Atr100 => "atr100",
        }
    }
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub(crate) enum TapeChoice {
    #[id = "ferric"]
    #[name = "Standard ferric"]
    Ferric,
    #[id = "studio"]
    #[name = "Audex 456"]
    Studio,
    #[id = "sm468"]
    #[name = "Nordtape SM-68"]
    Sm468,
    #[id = "a632"]
    #[name = "Audex 632"]
    A632,
    #[id = "cobalt"]
    #[name = "Cobalt-ferric"]
    Cobalt,
    #[id = "chrome"]
    #[name = "Chrome"]
    Chrome,
    #[id = "metal"]
    #[name = "Metal particle"]
    Metal,
}

impl TapeChoice {
    fn id(&self) -> &'static str {
        match self {
            TapeChoice::Ferric => "ferric",
            TapeChoice::Studio => "studio",
            TapeChoice::Sm468 => "sm468",
            TapeChoice::A632 => "a632",
            TapeChoice::Cobalt => "cobalt",
            TapeChoice::Chrome => "chrome",
            TapeChoice::Metal => "metal",
        }
    }
}

// Not every machine supports every speed; TapeMachine::update() snaps an unsupported choice to
// that machine's reference speed, same as the JS engine does.
#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub(crate) enum SpeedChoice {
    #[id = "3.75"]
    #[name = "3.75 ips"]
    S3_75,
    #[id = "7.5"]
    #[name = "7.5 ips"]
    S7_5,
    #[id = "15"]
    #[name = "15 ips"]
    S15,
    #[id = "30"]
    #[name = "30 ips"]
    S30,
}

impl SpeedChoice {
    fn ips(&self) -> f64 {
        match self {
            SpeedChoice::S3_75 => 3.75,
            SpeedChoice::S7_5 => 7.5,
            SpeedChoice::S15 => 15.0,
            SpeedChoice::S30 => 30.0,
        }
    }
}

impl Default for TapeMachineParams {
    fn default() -> Self {
        // knob defaults copied from the web app's data-d="..." attributes (0..100 -> 0..1 here)
        Self {
            editor_state: editor::default_state(),

            machine: EnumParam::new("Machine", MachineChoice::J37),
            tape: EnumParam::new("Tape", TapeChoice::Ferric),
            speed: EnumParam::new("Speed", SpeedChoice::S15),

            input: FloatParam::new("Input", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            output: FloatParam::new("Output", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            bias: FloatParam::new("Bias", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),

            ceiling: FloatParam::new("Ceiling", 0.4182, FloatRange::Linear { min: 0.0, max: 1.0 }),
            knee: FloatParam::new("Knee", 0.4016, FloatRange::Linear { min: 0.0, max: 1.0 }),
            grip: FloatParam::new("Grip", 0.4572, FloatRange::Linear { min: 0.0, max: 1.0 }),
            give: FloatParam::new("Give", 0.1798, FloatRange::Linear { min: 0.0, max: 1.0 }),

            spacing: FloatParam::new("Spacing", 0.1711, FloatRange::Linear { min: 0.0, max: 1.0 }),
            thickness: FloatParam::new("Coating", 0.4286, FloatRange::Linear { min: 0.0, max: 1.0 }),
            gap: FloatParam::new("Gap", 0.3234, FloatRange::Linear { min: 0.0, max: 1.0 }),

            wow: FloatParam::new("Wow", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            flutter: FloatParam::new("Flutter", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            hiss: FloatParam::new("Hiss", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
        }
    }
}

impl Default for TapeMachinePlugin {
    fn default() -> Self {
        Self { params: Arc::new(TapeMachineParams::default()), tm: None, last_key: (MachineChoice::J37, -1.0), scope: Arc::new(ScopeState::new()) }
    }
}

// PARAMS maps from tape-core.js's page.template.html, knob value v in 0..1 (originally 0..100).
// pub(crate) so the editor can reuse them for the live frequency-response plot and value-text
// readouts, exactly the same mapping the audio path uses.
pub(crate) fn map_spacing(v: f64) -> f64 {
    0.05 + 49.95 * v.powi(3)
}
pub(crate) fn map_thickness(v: f64) -> f64 {
    1.0 + 49.0 * v.powi(2)
}
pub(crate) fn map_gap(v: f64) -> f64 {
    0.5 + 19.5 * v.powi(2)
}
fn map_ceiling(v: f64) -> f64 {
    1e5 * 20f64.powf(v)
}
fn map_knee_grip(v: f64) -> f64 {
    5e3 * 40f64.powf(v)
}
fn map_give(v: f64) -> f64 {
    0.01 + 0.89 * v
}

impl Plugin for TapeMachinePlugin {
    const NAME: &'static str = "Tape Machine";
    const VENDOR: &'static str = "Them Changes";
    const URL: &'static str = "https://claude.ai/artifact/MEfnxg8EMNJG4XxkkhpNPv";
    const EMAIL: &'static str = "info@example.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.params.editor_state.clone(), self.scope.clone())
    }

    fn initialize(&mut self, _audio_io_layout: &AudioIOLayout, buffer_config: &BufferConfig, _context: &mut impl InitContext<Self>) -> bool {
        self.tm = Some(TapeMachine::new(buffer_config.sample_rate as f64, OVERSAMPLE));
        self.apply_params(true);
        true
    }

    fn reset(&mut self) {
        if let Some(tm) = &mut self.tm {
            tm.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.apply_params(false);

        let tm = self.tm.as_mut().expect("initialize() must run before process()");
        let num_samples = buffer.samples();
        let mut in_l = vec![0.0f32; num_samples];
        let mut in_r = vec![0.0f32; num_samples];

        {
            let slices = buffer.as_slice();
            if slices.len() >= 2 {
                in_l.copy_from_slice(slices[0]);
                in_r.copy_from_slice(slices[1]);
            } else if slices.len() == 1 {
                in_l.copy_from_slice(slices[0]);
                in_r.copy_from_slice(slices[0]);
            }
        }

        let mut out_l = vec![0.0f32; num_samples];
        let mut out_r = vec![0.0f32; num_samples];
        tm.process_rt(&in_l, &in_r, &mut out_l, &mut out_r, &self.scope);

        {
            let slices = buffer.as_slice();
            if slices.len() >= 2 {
                slices[0].copy_from_slice(&out_l);
                slices[1].copy_from_slice(&out_r);
            } else if slices.len() == 1 {
                slices[0].copy_from_slice(&out_l);
            }
        }

        ProcessStatus::Normal
    }
}

impl TapeMachinePlugin {
    fn apply_params(&mut self, force: bool) {
        let p = &self.params;
        let machine = p.machine.value();
        let speed = p.speed.value().ips();
        let key = (machine, speed as f32);
        let _changed = force || key != self.last_key;
        self.last_key = key;

        let tm = match &mut self.tm {
            Some(tm) => tm,
            None => return,
        };

        tm.p.machine = machine.id().to_string();
        tm.p.tape = p.tape.value().id().to_string();
        tm.p.speed = speed;
        tm.p.input = p.input.value() as f64;
        tm.p.output = p.output.value() as f64;
        tm.p.bias = p.bias.value() as f64;
        tm.p.ms = map_ceiling(p.ceiling.value() as f64);
        tm.p.a = map_knee_grip(p.knee.value() as f64);
        tm.p.k = map_knee_grip(p.grip.value() as f64);
        tm.p.c = map_give(p.give.value() as f64);
        tm.p.spacing = map_spacing(p.spacing.value() as f64);
        tm.p.thickness = map_thickness(p.thickness.value() as f64);
        tm.p.gap = map_gap(p.gap.value() as f64);
        tm.p.wow = p.wow.value() as f64;
        tm.p.flutter = p.flutter.value() as f64;
        tm.p.hiss = p.hiss.value() as f64;
        tm.update();
    }
}

impl ClapPlugin for TapeMachinePlugin {
    const CLAP_ID: &'static str = "com.themchanges.tape-machine";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Physically modelled analog tape machine");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Distortion, ClapFeature::Utility];
}

impl Vst3Plugin for TapeMachinePlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ThemChangesTapeM";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(TapeMachinePlugin);
nih_export_vst3!(TapeMachinePlugin);
