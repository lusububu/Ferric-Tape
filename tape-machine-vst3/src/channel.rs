// One processing channel: record amplifier EQ and output stage, tape (bias + hysteresis),
// playback amplifier EQ (with the head's d/dt), head and record losses (as an FIR), transformers,
// output stage limit. Direct port of the TapeChannel class from tape-core.js.

use crate::biquad::{hp1, make_lp8, run_chain, Biquad};
use crate::circuits::{circuit_for, Circuit};
use crate::hysteresis::Hysteresis;
use crate::stages::{clip8, make_rec_stage, RecordStage};
use crate::zpk::{zpk_to_sos, OutOfBand};

pub struct ChannelParams {
    pub bias_freq: f64,
    pub in_gain: f64,
    pub out_gain: f64,
    pub h_scale: f64,
    pub bias_amp: f64,
    pub hiss_amp: f64,
    pub pb_clip: f64,
    pub m_scale: f64,
}

struct Circ {
    rec: Vec<Biquad>,
    pb: Vec<Biquad>,
    stage: Box<dyn RecordStage + Send>,
    in_xf: Biquad,
    out_xf: Biquad,
    hlp: Biquad,
    out_c: f64,
    hard_out: bool,
    extra: (OutOfBand, OutOfBand),
}

pub struct TapeChannel {
    fs: f64,
    os: usize,
    fso: f64,
    hyst: Hysteresis,
    up: Vec<Biquad>,
    down: Vec<Biquad>,
    bias_phase: f64,
    fir: Vec<f64>,
    fir_buf: Vec<f64>,
    fir_pos: usize,
    dl: Vec<f64>,
    dl_pos: usize,
    dcb: Biquad,
    seed: u32,
    pub sx: f64,
    pub sy: f64,
    ckey: Option<(String, f64)>, // (machine, ips)
    circ: Option<Circ>,
}

impl TapeChannel {
    pub fn new(fs: f64, os: usize, seed: u32) -> Self {
        let fso = fs * os as f64;
        let dl_len = (fs * 0.25).ceil() as usize;
        TapeChannel {
            fs,
            os,
            fso,
            hyst: Hysteresis::new(fso),
            up: make_lp8(fso, fs * 0.45),
            down: make_lp8(fso, fs * 0.45),
            bias_phase: 0.0,
            fir: vec![0.0; 101],
            fir_buf: vec![0.0; 202],
            fir_pos: 0,
            dl: vec![0.0; dl_len.max(1)],
            dl_pos: 0,
            dcb: Biquad::new().highpass(fs, 12.0, 0.707),
            seed: if seed == 0 { 1 } else { seed },
            sx: 0.0,
            sy: 0.0,
            ckey: None,
            circ: None,
        }
    }

    #[inline]
    fn next_rand(seed: &mut u32) -> f64 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *seed as f64 / 4294967296.0 * 2.0 - 1.0
    }

    pub fn set_fir(&mut self, h: &[f64]) {
        if h.len() != self.fir.len() {
            self.fir = vec![0.0; h.len()];
            self.fir_buf = vec![0.0; 2 * h.len()];
            self.fir_pos = 0;
        }
        self.fir.copy_from_slice(h);
    }

    pub fn hyst_mut(&mut self) -> &mut Hysteresis {
        &mut self.hyst
    }

    pub fn reset(&mut self) {
        self.hyst.reset();
        for b in self.up.iter_mut() {
            b.reset();
        }
        for b in self.down.iter_mut() {
            b.reset();
        }
    }

    pub fn extra(&self) -> Option<&(OutOfBand, OutOfBand)> {
        self.circ.as_ref().map(|c| &c.extra)
    }

    pub fn set_circuit(&mut self, machine: &str, ips: f64) {
        if let Some((m, s)) = &self.ckey {
            if m == machine && (*s - ips).abs() < 1e-9 {
                return;
            }
        }
        self.ckey = Some((machine.to_string(), ips));
        let c: Circuit = circuit_for(machine, ips);
        let (rec, dr) = zpk_to_sos(c.rec_z, c.rec_p, self.fso, None);
        // the engine auto-appends a DC zero to every machine's pb.z, representing the playback
        // head's dM/dt physics (not part of the EQ amp itself)
        let mut pb_z: Vec<[f64; 2]> = c.pb_z.to_vec();
        pb_z.push([0.0, 0.0]);
        let (pb, dp) = zpk_to_sos(&pb_z, c.pb_p, self.fso, None);
        let stage = make_rec_stage(machine, self.fso, ips);
        let in_xf = hp1(self.fs, c.meta.xflf);
        let out_xf = hp1(self.fs, c.meta.xflf);
        // the head's inductance and the bias trap band-limit the record current; without this,
        // the corners of a clipping record stage excite the solver's trapezoidal derivative at
        // Nyquist
        let hlp = Biquad::new().lowpass(self.fso, 40000.0, 0.7071_f64);
        let out_c = 0.12589254117941673 * 10f64.powf(c.meta.outmaxdb / 20.0);
        self.circ = Some(Circ { rec, pb, stage, in_xf, out_xf, hlp, out_c, hard_out: c.meta.hard_out, extra: (dr, dp) });
    }

    // record amplifier EQ and output stage, tape (bias + hysteresis), playback amplifier EQ
    // (with the head's d/dt), head and record losses, transformers, output stage limit
    pub fn process(&mut self, x: f64, p: &ChannelParams, delay_samples: f64) -> f64 {
        let os = self.os;
        let bw = 2.0 * std::f64::consts::PI * p.bias_freq / self.fso;
        let fso = self.fso;
        let mut bias_phase = self.bias_phase;
        let mut seed = self.seed;
        let mut y = 0.0f64;
        {
            let circ = self.circ.as_mut().expect("setCircuit must be called before process");
            let xin = circ.in_xf.run(x) * p.in_gain; // input transformer
            for i in 0..os {
                let up_in = if i == 0 { xin * os as f64 } else { 0.0 };
                let u = run_chain(&mut circ.rec, run_chain(&mut self.up, up_in));
                let ri = circ.hlp.run(circ.stage.process(u)); // record current, normalised
                bias_phase += bw;
                if bias_phase > 6.283185307179586 {
                    bias_phase -= 6.283185307179586;
                }
                let mut m = self.hyst.process_bias(ri * p.h_scale, bias_phase, p.bias_amp, bw * fso);
                if p.hiss_amp > 0.0 {
                    m += Self::next_rand(&mut seed) * p.hiss_amp; // tape noise, in magnetisation units
                }
                y = run_chain(&mut self.down, run_chain(&mut circ.pb, m));
            }
        }
        self.bias_phase = bias_phase;
        self.seed = seed;

        let circ_out_c;
        let circ_hard_out;
        {
            let circ = self.circ.as_ref().unwrap();
            circ_out_c = circ.out_c;
            circ_hard_out = circ.hard_out;
        }

        // reproduce preamp headroom, applied after the de-bias filter: the head never plays
        // back the bias-rate magnetisation
        if p.pb_clip > 0.0 {
            y = clip8(y, p.pb_clip);
        }
        y *= p.m_scale;
        self.sx = x * p.in_gain;
        self.sy = y; // scope: input vs recovered signal

        let l = self.dl.len();
        self.dl[self.dl_pos] = y;
        let rp = self.dl_pos as f64 - delay_samples;
        let ip = rp.floor() as i64;
        let d = rp - ip as f64;
        let sample_at = |dl: &[f64], q: i64| -> f64 {
            let idx = (((ip + q) % l as i64) + l as i64) % l as i64;
            dl[idx as usize]
        };
        let xm1 = sample_at(&self.dl, -1);
        let x0 = sample_at(&self.dl, 0);
        let x1 = sample_at(&self.dl, 1);
        let x2 = sample_at(&self.dl, 2);
        let mut y2 = x0 + d * (x1 - xm1) / 2.0 + d * d * (xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2) + d * d * d * (1.5 * (x0 - x1) + 0.5 * (x2 - xm1));
        self.dl_pos = (self.dl_pos + 1) % l;

        let n = self.fir.len();
        self.fir_buf[self.fir_pos] = y2;
        self.fir_buf[self.fir_pos + n] = y2;
        let mut acc = 0.0f64;
        let base = self.fir_pos + 1;
        for ni in 0..n {
            acc += self.fir[ni] * self.fir_buf[base + ni];
        }
        self.fir_pos = (self.fir_pos + 1) % n;

        {
            let circ = self.circ.as_mut().unwrap();
            y2 = circ.out_xf.run(acc);
        }

        let c = circ_out_c; // output stage limit
        y2 = if circ_hard_out { clip8(y2, c) } else { y2 / (1.0 + (y2 / c).abs().powf(6.0)).powf(1.0 / 6.0) };
        self.dcb.run(y2) * p.out_gain
    }
}
