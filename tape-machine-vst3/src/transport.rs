// Wow, flutter and drift transport model. Direct port of the Transport class from tape-core.js.

use crate::biquad::Biquad;

pub struct Transport {
    fs: f64,
    ph1: f64,
    ph2: f64,
    ph3: f64,
    seed: u32,
    drift: [Biquad; 2],
}

pub struct TransportParams {
    pub speed: f64,
    pub wow: f64,
    pub flutter: f64,
    pub wf_scale: f64,
}

impl Transport {
    pub fn new(fs: f64) -> Self {
        Transport {
            fs,
            ph1: 0.0,
            ph2: 0.0,
            ph3: 0.0,
            seed: 7,
            drift: [Biquad::new().lowpass(fs, 1.0, 0.707), Biquad::new().lowpass(fs, 1.0, 0.707)],
        }
    }

    fn rand(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        self.seed as f64 / 4294967296.0 * 2.0 - 1.0
    }

    // delay in ms around the 6 ms centre, before depth scaling
    pub fn raw(&mut self, sp: f64, wow: f64, flutter: f64) -> f64 {
        let fs = self.fs;
        self.ph1 += 2.0 * std::f64::consts::PI * 0.6 * sp / fs;
        self.ph2 += 2.0 * std::f64::consts::PI * 1.7 * sp / fs;
        self.ph3 += 2.0 * std::f64::consts::PI * 14.0 * sp / fs;
        if self.ph1 > 1e4 {
            self.ph1 %= 2.0 * std::f64::consts::PI;
            self.ph2 %= 2.0 * std::f64::consts::PI;
            self.ph3 %= 2.0 * std::f64::consts::PI;
        }
        let mut r = self.rand() * 40.0;
        r = self.drift[0].run(r);
        r = self.drift[1].run(r);
        wow * (1.2 * self.ph1.sin() + 0.5 * (self.ph2 + 1.3).sin() + r) + flutter * 0.08 * (self.ph3.sin() + 0.35 * (2.0 * self.ph3 + 0.7).sin())
    }

    pub fn next(&mut self, p: &TransportParams) -> f64 {
        let ms = 6.0 + p.wf_scale * self.raw(p.speed / 7.5, p.wow, p.flutter);
        ms.max(1.0) * self.fs / 1000.0
    }
}

// peak speed error of the transport model at depth 1 with the default 50/50 knobs (simulated, 20 s)
pub fn transport_peak(fs: f64, speed: f64) -> f64 {
    let mut t = Transport::new(fs);
    let mut prev: Option<f64> = None;
    let mut peak = 0.0;
    let n = (fs * 20.0) as i64;
    for i in 0..n {
        let ms = t.raw(speed / 7.5, 0.5, 0.5);
        if let Some(p) = prev {
            if i as f64 > fs * 2.0 {
                peak = f64::max(peak, (ms - p).abs() / 1000.0 * fs);
            }
        }
        prev = Some(ms);
    }
    peak
}
