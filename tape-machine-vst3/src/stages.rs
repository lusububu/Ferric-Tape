// Record-stage nonlinearities: clip8, ClipRecordStage, V2bStage/ValveRecordStage (J37 valve).
// Direct port from tape-core.js.

use crate::data::{J37_V2B, V2bParams};

const REF_0VU: f64 = 0.12589254117941673; // 10^(-18/20): 0 VU = -18 dBFS

// smooth limit: |x| -> c as x grows, unity gain at small signals (16th-power knee: an op-amp
// running into its rails)
#[inline]
pub fn clip8(x: f64, c: f64) -> f64 {
    let u = x / c;
    let u2 = u * u;
    let u4 = u2 * u2;
    let u8 = u4 * u4;
    x / (1.0 + u8 * u8).sqrt().sqrt().sqrt().sqrt()
}

pub trait RecordStage {
    fn process(&mut self, u: f64) -> f64;
    fn gain_at(&self, f: f64) -> f64;
}

// op-amp record chain: preamp output (after the EQ) then the driver, each limiting at its
// headroom above 0 VU
pub struct ClipRecordStage {
    c1: f64,
    c2: f64,
}

impl ClipRecordStage {
    pub fn new(pre_db: f64, drv_db: f64) -> Self {
        ClipRecordStage { c1: REF_0VU * 10f64.powf(pre_db / 20.0), c2: REF_0VU * 10f64.powf(drv_db / 20.0) }
    }
}

impl RecordStage for ClipRecordStage {
    fn process(&mut self, u: f64) -> f64 {
        clip8(clip8(u, self.c1), self.c2)
    }
    fn gain_at(&self, _f: f64) -> f64 {
        1.0
    }
}

// the record output valve V2b driving the head as a current source: solves Ia = K (Vgk + Va/mu)^1.5
// with cathode degeneration R28 and the bootstrapped R29 (bypassed by C19 at 7.5 ips)
pub struct V2bStage {
    v: &'static V2bParams,
    c19: bool,
    t: f64,
    dvn: f64,
    dip: f64,
    vgk0: f64,
    x0: f64,
    ca: f64,
    cb: f64,
}

impl V2bStage {
    pub fn new(fso: f64, ips: f64) -> Self {
        let v = &J37_V2B;
        let c19 = ips < 10.0;
        let t = 1.0 / fso;
        let vgk0 = -v.ia0 * v.r28;
        let x0 = vgk0 + v.va0 / v.mu;
        let tc = v.r29 * v.c19;
        let ca = (2.0 * tc - t) / (2.0 * tc + t);
        let cb = t * v.r29 / (2.0 * tc + t);
        V2bStage { v, c19, t, dvn: 0.0, dip: 0.0, vgk0, x0, ca, cb }
    }

    pub fn gm_small(&self, f: f64) -> f64 {
        let v = self.v;
        let gm = 1.5 * v.k * self.x0.sqrt();
        let w = 2.0 * std::f64::consts::PI * f;
        let zr = if self.c19 { v.r29 / (1.0_f64).hypot(w * v.r29 * v.c19) } else { v.r29 };
        1.0 / (1.0 / gm + v.r28 + v.boot * zr)
    }

    pub fn process(&mut self, vin: f64) -> f64 {
        let v = self.v;
        let mut di = self.dip;
        for _it in 0..3 {
            let (dvn, ddvn) = if self.c19 { (self.ca * self.dvn + self.cb * (di + self.dip), self.cb) } else { (di * v.r29, v.r29) };
            let mut x = self.x0 + vin - di * v.r28 - v.boot * dvn;
            if x > v.va0 / v.mu {
                x = v.va0 / v.mu; // grid current: no swing past Vgk = 0
            }
            let sq = if x > 0.0 { x.sqrt() } else { 0.0 };
            let ia = v.k * x * sq;
            let dia = 1.5 * v.k * sq;
            let f = di - (ia - v.ia0);
            let df = 1.0 + dia * (v.r28 + v.boot * ddvn);
            let step = f / df;
            di -= step;
            if step.abs() < 1e-9 {
                break;
            }
        }
        if self.c19 {
            self.dvn = self.ca * self.dvn + self.cb * (di + self.dip);
        }
        self.dip = di;
        di
    }

    #[allow(dead_code)]
    pub fn t(&self) -> f64 {
        self.t
    }
}

// the J37 output valve, normalised: input and output are in 0 VU = -18 dBFS units, unity
// small-signal gain at 1 kHz
pub struct ValveRecordStage {
    v: V2bStage,
    k: f64,
    g: f64,
}

impl ValveRecordStage {
    pub fn new(fso: f64, ips: f64) -> Self {
        let v = V2bStage::new(fso, ips);
        let k = J37_V2B.vg0vu / REF_0VU;
        let g = 1.0 / (v.gm_small(1000.0) * k);
        ValveRecordStage { v, k, g }
    }
}

impl RecordStage for ValveRecordStage {
    fn process(&mut self, u: f64) -> f64 {
        self.v.process(u * self.k) * self.g
    }
    fn gain_at(&self, f: f64) -> f64 {
        self.v.gm_small(f) / self.v.gm_small(1000.0)
    }
}

pub fn make_rec_stage(machine: &str, fso: f64, ips: f64) -> Box<dyn RecordStage + Send> {
    let meta = crate::data::circuit_meta(machine);
    if meta.is_valve {
        Box::new(ValveRecordStage::new(fso, ips))
    } else {
        Box::new(ClipRecordStage::new(meta.rec_clip_db, meta.drv_clip_db))
    }
}
