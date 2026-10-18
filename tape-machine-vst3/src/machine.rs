// Top-level tape machine: calibration, FIR building, and per-sample processing of both channels.
// Direct port of the TapeMachine class from tape-core.js.

use std::collections::HashMap;

use crate::channel::{ChannelParams, TapeChannel};
use crate::circuits::{circuit_fir, circuit_for};
use crate::data::{bias_cal, cal_by_id, lookup, machine_by_id, sim_bias_freq, tape_by_id, MACHINE_IDS};
use crate::hysteresis::PhysicalParams;
use crate::transport::{Transport, TransportParams};

pub struct Params {
    pub machine: String,
    pub tape: String,
    pub ms: f64,
    pub a: f64,
    pub k: f64,
    pub c: f64,
    pub bias: f64,
    pub speed: f64,
    pub spacing: f64,
    pub thickness: f64,
    pub gap: f64,
    pub wow: f64,
    pub flutter: f64,
    pub hiss: f64,
    pub input: f64,
    pub output: f64,

    // derived, computed in update()
    pub bias_freq: f64,
    pub in_gain: f64,
    pub out_gain: f64,
    pub h_scale: f64,
    pub bias_amp: f64,
    pub m_scale: f64,
    pub pb_clip: f64,
    pub hiss_amp: f64,
    pub wf_scale: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            machine: "j37".to_string(),
            tape: "ferric".to_string(),
            ms: 3.5e5,
            a: 22e3,
            k: 27e3,
            c: 0.17,
            bias: 0.5,
            speed: 15.0,
            spacing: 0.3,
            thickness: 10.0,
            gap: 2.54,
            wow: 0.5,
            flutter: 0.5,
            hiss: 0.5,
            input: 0.5,
            output: 0.5,
            bias_freq: 80000.0,
            in_gain: 1.0,
            out_gain: 1.0,
            h_scale: 1.0,
            bias_amp: 0.0,
            m_scale: 1.0,
            pb_clip: 0.0,
            hiss_amp: 0.0,
            wf_scale: 1.0,
        }
    }
}

pub struct TapeMachine {
    fs: f64,
    os: usize,
    pub ch: [TapeChannel; 2],
    tr: Transport,
    wf_cache: HashMap<String, f64>,
    pub p: Params,
}

fn valid_machine_id(id: &str) -> bool {
    MACHINE_IDS.contains(&id)
}

impl TapeMachine {
    pub fn new(fs: f64, os: usize) -> Self {
        let mut m = TapeMachine {
            fs,
            os,
            ch: [TapeChannel::new(fs, os, 11), TapeChannel::new(fs, os, 23)],
            tr: Transport::new(fs),
            wf_cache: HashMap::new(),
            p: Params::default(),
        };
        m.update();
        m
    }

    pub fn set_machine(&mut self, machine: &str) {
        self.p.machine = machine.to_string();
        self.update();
    }
    pub fn set_tape(&mut self, tape: &str) {
        self.p.tape = tape.to_string();
        self.update();
    }
    pub fn set_speed(&mut self, speed: f64) {
        self.p.speed = speed;
        self.update();
    }
    pub fn set_knobs(&mut self, bias: f64, spacing: f64, thickness: f64, gap: f64, wow: f64, flutter: f64, hiss: f64, input: f64, output: f64) {
        self.p.bias = bias;
        self.p.spacing = spacing;
        self.p.thickness = thickness;
        self.p.gap = gap;
        self.p.wow = wow;
        self.p.flutter = flutter;
        self.p.hiss = hiss;
        self.p.input = input;
        self.p.output = output;
        self.update();
    }

    pub fn update(&mut self) {
        if !valid_machine_id(&self.p.machine) {
            self.p.machine = "j37".to_string();
        }
        self.p.in_gain = 10f64.powf((self.p.input - 0.5) * 36.0 / 20.0); // -18..+18 dB around the calibrated level
        self.p.out_gain = 10f64.powf((self.p.output - 0.5) * 36.0 / 20.0);

        let tape = tape_by_id(&self.p.tape);
        let ferric = tape_by_id("ferric");
        let mc = machine_by_id(&self.p.machine);
        let circ = circuit_for(&self.p.machine, self.p.speed);

        if !mc.speeds.iter().any(|&s| (s - self.p.speed).abs() < 1e-9) {
            self.p.speed = mc.refspeed;
        }

        // bias locked to fso/N so every bias harmonic aliases onto DC or a bias harmonic, never
        // into the audio band
        self.p.bias_freq = sim_bias_freq(self.fs, self.os as f64, mc.fb);
        let bc = bias_cal(self.p.bias_freq);
        let ob = circ.overbias;

        for ch in self.ch.iter_mut() {
            ch.set_circuit(&self.p.machine, self.p.speed);
        }

        let fir = circuit_fir(&circ, self.fs, 100, self.p.speed, self.p.spacing, self.p.thickness, self.p.gap, self.ch[0].extra());

        // level aligned to recorded flux: 0 VU writes the same magnetisation on every tape, so a
        // tape with more ceiling has more headroom, as on a real machine
        let cal = cal_by_id(&self.p.machine);
        let g0t = bc.g0 * tape.g0 / ferric.g0; // this tape's small-signal sensitivity
        self.p.h_scale = cal.hg / g0t;
        let ob_ratio = ob.map(|(r, _)| r).unwrap_or(1.0);
        self.p.bias_amp = 2.0 * self.p.bias * bc.bpeak * tape.bpeak / ferric.bpeak * ob_ratio; // knob 0..1 = 0..200% of the aligned bias
        self.p.m_scale = 1.0 / (cal.hg * cal.kappa);
        self.p.pb_clip = if circ.meta.pbclipdb > 0.0 { 0.12589254117941673 * 10f64.powf(circ.meta.pbclipdb / 20.0) / self.p.m_scale } else { 0.0 };
        self.p.hiss_amp = self.p.hiss * 2.0 * 10f64.powf(lookup(mc.noise, self.p.speed) / 20.0) * lookup(cal.noise, self.p.speed);

        let key = format!("{}{}", self.p.machine, self.p.speed);
        let fs = self.fs;
        let speed = self.p.speed;
        let wf_scale = *self.wf_cache.entry(key).or_insert_with(|| lookup(mc.wf, speed) / crate::transport::transport_peak(fs, speed));
        self.p.wf_scale = wf_scale;

        for ch in self.ch.iter_mut() {
            ch.hyst_mut().set_physical(PhysicalParams { ms: self.p.ms, a: self.p.a, k: self.p.k, c: self.p.c, alpha: tape.alpha });
            ch.set_fir(&fir.h);
        }
    }

    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        // output protection: untouched below -0.9 dBFS, soft-limited to 0 dBFS above (not part
        // of the model)
        let prot = |y: f64| -> f64 {
            let a = y.abs();
            if a <= 0.9 {
                y
            } else {
                y.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
            }
        };
        let cp = ChannelParams {
            bias_freq: self.p.bias_freq,
            in_gain: self.p.in_gain,
            out_gain: self.p.out_gain,
            h_scale: self.p.h_scale,
            bias_amp: self.p.bias_amp,
            hiss_amp: self.p.hiss_amp,
            pb_clip: self.p.pb_clip,
            m_scale: self.p.m_scale,
        };
        let tp = TransportParams { speed: self.p.speed, wow: self.p.wow, flutter: self.p.flutter, wf_scale: self.p.wf_scale };
        for i in 0..in_l.len() {
            let d = self.tr.next(&tp);
            out_l[i] = prot(self.ch[0].process(in_l[i] as f64, &cp, d)) as f32;
            out_r[i] = prot(self.ch[1].process(in_r[i] as f64, &cp, d)) as f32;
        }
    }

    pub fn reset(&mut self) {
        for ch in self.ch.iter_mut() {
            ch.reset();
        }
    }
}

#[cfg(test)]
mod parity_test {
    use super::*;

    // Numeric parity check against tape-core.js's TapeMachine, same engine state
    // (j37/ferric/15ips, 48kHz, 8x oversampling), run with: cargo test -- --nocapture
    #[test]
    fn j37_sine_matches_js_reference() {
        let fs = 48000.0;
        let os = 8;
        let mut tm = TapeMachine::new(fs, os);
        tm.p.machine = "j37".to_string();
        tm.p.tape = "ferric".to_string();
        tm.p.speed = 15.0;
        tm.update();

        let n = 2000;
        let mut in_l = vec![0.0f32; n];
        let mut in_r = vec![0.0f32; n];
        for i in 0..n {
            let v = 0.3 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / fs).sin();
            in_l[i] = v as f32;
            in_r[i] = v as f32;
        }
        let mut out_l = vec![0.0f32; n];
        let mut out_r = vec![0.0f32; n];
        tm.process(&in_l, &in_r, &mut out_l, &mut out_r);

        let mut rms = 0.0f64;
        for i in 1000..n {
            rms += (out_l[i] as f64) * (out_l[i] as f64);
        }
        rms = (rms / (n - 1000) as f64).sqrt();
        println!("Rust rms(last 1000)= {:.6}", rms);
        println!("Rust sample[1000..1010]= {:?}", &out_l[1000..1010]);

        // reference values captured from `node -e` run of tape-core.js with identical params
        let js_rms = 0.165776;
        let js_samples: [f64; 10] = [-0.210544, -0.206359, -0.200241, -0.192098, -0.181373, -0.167780, -0.150629, -0.129019, -0.102525, -0.070817];
        assert!((rms - js_rms).abs() < 1e-3, "rms mismatch: rust={} js={}", rms, js_rms);
        for i in 0..10 {
            let diff = (out_l[1000 + i] as f64 - js_samples[i]).abs();
            assert!(diff < 1e-3, "sample {} mismatch: rust={} js={}", i, out_l[1000 + i], js_samples[i]);
        }
    }

    fn run_case(machine: &str, speed: f64, tape: &str) -> (f64, f64) {
        let fs = 48000.0;
        let os = 8;
        let mut tm = TapeMachine::new(fs, os);
        tm.p.machine = machine.to_string();
        tm.p.tape = tape.to_string();
        tm.p.speed = speed;
        tm.update();
        let n = 2000;
        let mut in_l = vec![0.0f32; n];
        let mut in_r = vec![0.0f32; n];
        for i in 0..n {
            let v = 0.3 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / fs).sin();
            in_l[i] = v as f32;
            in_r[i] = v as f32;
        }
        let mut out_l = vec![0.0f32; n];
        let mut out_r = vec![0.0f32; n];
        tm.process(&in_l, &in_r, &mut out_l, &mut out_r);
        let mut rms = 0.0f64;
        for i in 1000..n {
            rms += (out_l[i] as f64) * (out_l[i] as f64);
        }
        rms = (rms / (n - 1000) as f64).sqrt();
        (rms, out_l[1500] as f64)
    }

    // Reference (rms, sample[1500]) pairs captured from `node -e` runs of tape-core.js, one per
    // machine, to catch data-transcription errors in the per-machine zpk/calibration tables.
    #[test]
    fn all_machines_match_js_reference() {
        let cases: [(&str, f64, &str, f64, f64); 5] = [
            ("a80", 15.0, "ferric", 0.186730, 0.243616),
            ("a77", 7.5, "ferric", 0.082855, 0.092281),
            ("tc260", 7.5, "ferric", 0.192362, 0.195860),
            ("atr100", 15.0, "ferric", 0.197443, 0.209370),
            ("atr100", 30.0, "metal", 0.191771, 0.137246),
        ];
        for (machine, speed, tape, js_rms, js_samp) in cases {
            let (rms, samp) = run_case(machine, speed, tape);
            println!("{machine} {speed} {tape}: rust rms={rms:.6} samp={samp:.6} (js rms={js_rms:.6} samp={js_samp:.6})");
            assert!((rms - js_rms).abs() < 1e-3, "{machine}: rms mismatch rust={rms} js={js_rms}");
            assert!((samp - js_samp).abs() < 1e-3, "{machine}: sample mismatch rust={samp} js={js_samp}");
        }
    }
}
