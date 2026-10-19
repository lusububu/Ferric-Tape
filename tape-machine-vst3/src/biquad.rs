// Direct port of the biquad (RBJ cookbook), direct form I, from tape-core.js.

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
}

impl Biquad {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lowpass(mut self, fs: f64, f: f64, q: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * f / fs;
        let cs = w.cos();
        let al = w.sin() / (2.0 * q);
        let a0 = 1.0 + al;
        self.b0 = (1.0 - cs) / 2.0 / a0;
        self.b1 = (1.0 - cs) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - al) / a0;
        self
    }

    pub fn peak(mut self, fs: f64, f: f64, q: f64, db: f64) -> Self {
        let a = 10f64.powf(db / 40.0);
        let w = 2.0 * std::f64::consts::PI * f / fs;
        let cs = w.cos();
        let al = w.sin() / (2.0 * q);
        let a0 = 1.0 + al / a;
        self.b0 = (1.0 + al * a) / a0;
        self.b1 = -2.0 * cs / a0;
        self.b2 = (1.0 - al * a) / a0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - al / a) / a0;
        self
    }

    pub fn highpass(mut self, fs: f64, f: f64, q: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * f / fs;
        let cs = w.cos();
        let al = w.sin() / (2.0 * q);
        let a0 = 1.0 + al;
        self.b0 = (1.0 + cs) / 2.0 / a0;
        self.b1 = -(1.0 + cs) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - al) / a0;
        self
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

// 8th-order Butterworth lowpass as 4 biquads (Q values for N=8)
const BW8_Q: [f64; 4] = [0.50980, 0.60134, 0.89998, 2.56292];

pub fn make_lp8(fs: f64, f: f64) -> Vec<Biquad> {
    BW8_Q.iter().map(|&q| Biquad::new().lowpass(fs, f, q)).collect()
}

#[inline]
pub fn run_chain(chain: &mut [Biquad], mut x: f64) -> f64 {
    for b in chain.iter_mut() {
        x = b.run(x);
    }
    x
}

// first-order high-pass (transformer low-frequency roll-off)
pub fn hp1(fs: f64, fc: f64) -> Biquad {
    let mut b = Biquad::new();
    let k = (std::f64::consts::PI * fc / fs).tan();
    b.b0 = 1.0 / (1.0 + k);
    b.b1 = -b.b0;
    b.b2 = 0.0;
    b.a1 = (k - 1.0) / (k + 1.0);
    b.a2 = 0.0;
    b
}
