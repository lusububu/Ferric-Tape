// Jiles-Atherton hysteresis, RK4. Direct port of the Hysteresis class in tape-core.js.

// Series below |x| < 1e-2 instead of the paper's 1e-4 cut-off: L'(x) loses precision to
// cancellation in doubles before 1e-4, so the next series term is kept.
pub fn langevin(x: f64) -> f64 {
    if x.abs() < 1e-2 {
        x / 3.0 - x * x * x / 45.0
    } else {
        1.0 / x.tanh() - 1.0 / x
    }
}

pub fn langevin_d(x: f64) -> f64 {
    if x.abs() < 1e-2 {
        1.0 / 3.0 - x * x / 15.0
    } else {
        let ct = 1.0 / x.tanh();
        1.0 / (x * x) - ct * ct + 1.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicalParams {
    pub ms: f64,
    pub a: f64,
    pub k: f64,
    pub c: f64,
    pub alpha: f64,
}

impl Default for PhysicalParams {
    fn default() -> Self {
        // Physical constants for ferric oxide tape, Chowdhury 2019 table (fig. 6) and sec. 4.1, in A/m
        PhysicalParams { ms: 3.5e5, a: 22e3, k: 27e3, c: 0.17, alpha: 1.6e-3 }
    }
}

pub struct Hysteresis {
    pub t: f64,
    pub m: f64,
    pub hap: f64,
    pub hadp: f64,
    pub ms: f64,
    pub a: f64,
    pub k: f64,
    pub c: f64,
    pub alpha: f64,
    pub makeup: f64,
    pub nc: f64,
    pub msa: f64,
}

impl Hysteresis {
    pub fn new(fs_over: f64) -> Self {
        let mut h = Hysteresis {
            t: 1.0 / fs_over,
            m: 0.0,
            hap: 0.0,
            hadp: 0.0,
            ms: 0.0,
            a: 0.0,
            k: 0.0,
            c: 0.0,
            alpha: 0.0,
            makeup: 1.0,
            nc: 0.0,
            msa: 0.0,
        };
        h.set_physical(PhysicalParams::default());
        h
    }

    pub fn set_physical(&mut self, t: PhysicalParams) {
        if self.ms > 0.0 && t.ms != self.ms {
            // keep the stored state inside the new ceiling
            self.m *= t.ms / self.ms;
        }
        self.ms = t.ms;
        self.a = t.a;
        self.k = t.k;
        self.c = t.c;
        self.alpha = t.alpha;
        self.makeup = 1.0;
        self.nc = 1.0 - self.c;
        self.msa = self.ms / self.a;
    }

    #[inline]
    fn f(&self, h: f64, hd: f64, m: f64) -> f64 {
        let q = (h + self.alpha * m) / self.a;
        let d_man = self.ms * langevin(q) - m;
        let ld = langevin_d(q);
        let d_s = if hd >= 0.0 { 1.0 } else { -1.0 };
        let d_m = if (d_s > 0.0) == (d_man > 0.0) { 1.0 } else { 0.0 };
        let den1 = self.nc * d_s * self.k - self.alpha * d_man;
        let t1 = if den1 != 0.0 { self.nc * d_m * d_man / den1 * hd } else { 0.0 };
        let t2 = self.c * self.msa * hd * ld;
        (t1 + t2) / (1.0 - self.c * self.alpha * self.msa * ld)
    }

    // Same RK4 step, but the bias part of the field is evaluated exactly (value and derivative)
    // instead of through the trapezoidal derivative, which is inaccurate at 55 kHz with only
    // 7 to 14 samples per bias cycle. ha: audio field now; ph: bias phase now; b: amplitude; w: rad/s.
    pub fn process_bias(&mut self, ha: f64, ph: f64, b: f64, w: f64) -> f64 {
        let t = self.t;
        let hap = self.hap;
        let hadp = self.hadp;
        let had = 2.0 * (ha - hap) / t - hadp;
        // adaptive sub-steps: keep the audio-field change per RK4 step below a quarter of the knee width a,
        // so fast, large fields (e.g. a strongly pre-emphasised record current) cannot overshoot
        let n = ((ha - hap).abs() / (0.25 * self.a)).ceil().max(1.0).min(32.0) as i32;
        let h = t / n as f64;
        let d_ha = ha - hap;
        let d_hd = had - hadp;
        let w_t = w * t;
        let mut m = self.m;
        for i in 0..n {
            let u0 = i as f64 / n as f64;
            let um = (i as f64 + 0.5) / n as f64;
            let u1 = (i as f64 + 1.0) / n as f64;
            let p0 = ph - w_t * (1.0 - u0);
            let pm = ph - w_t * (1.0 - um);
            let p1 = ph - w_t * (1.0 - u1);
            let h0 = hap + d_ha * u0 + b * p0.cos();
            let d0 = hadp + d_hd * u0 - b * w * p0.sin();
            let hm = hap + d_ha * um + b * pm.cos();
            let dm = hadp + d_hd * um - b * w * pm.sin();
            let h1 = hap + d_ha * u1 + b * p1.cos();
            let d1 = hadp + d_hd * u1 - b * w * p1.sin();
            let k1 = h * self.f(h0, d0, m);
            let k2 = h * self.f(hm, dm, m + k1 / 2.0);
            let k3 = h * self.f(hm, dm, m + k2 / 2.0);
            let k4 = h * self.f(h1, d1, m + k3);
            m += k1 / 6.0 + k2 / 3.0 + k3 / 3.0 + k4 / 6.0;
            // the magnetisation cannot physically exceed saturation; clamp there (and recover from NaN)
            if !m.is_finite() {
                m = 0.0;
            } else if m > self.ms {
                m = self.ms;
            } else if m < -self.ms {
                m = -self.ms;
            }
        }
        self.m = m;
        self.hap = ha;
        self.hadp = had;
        m
    }

    pub fn reset(&mut self) {
        self.m = 0.0;
        self.hap = 0.0;
        self.hadp = 0.0;
    }
}
