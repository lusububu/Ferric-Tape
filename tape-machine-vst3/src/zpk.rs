// analog poles/zeros -> biquads at rate fs: per-root prewarping, bilinear transform, unity at 1 kHz
// Direct port of zpkToSOS from tape-core.js.

use crate::biquad::Biquad;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

impl C {
    pub fn new(re: f64, im: f64) -> Self {
        C { re, im }
    }
    pub fn mag(&self) -> f64 {
        self.re.hypot(self.im)
    }
}

// roots above the cutoff, handed back for out-of-band magnitude use (circuitFIR's `extra`)
#[derive(Clone, Default)]
pub struct OutOfBand {
    pub z: Vec<C>,
    pub p: Vec<C>,
}

pub fn zpk_to_sos(zz: &[[f64; 2]], pp: &[[f64; 2]], fs: f64, fmax: Option<f64>) -> (Vec<Biquad>, OutOfBand) {
    let lim = 2.0 * std::f64::consts::PI * fmax.unwrap_or(0.45 * fs);
    let mut z: Vec<C> = zz.iter().map(|a| C::new(a[0], a[1])).collect();
    let mut p: Vec<C> = pp.iter().map(|a| C::new(a[0], a[1])).collect();

    let mut out = OutOfBand::default();
    out.z = z.iter().filter(|r| r.mag() >= lim).cloned().collect();
    out.p = p.iter().filter(|r| r.mag() >= lim).cloned().collect();

    z = z.into_iter().filter(|r| r.mag() < lim).collect();
    p = p.into_iter().filter(|r| r.mag() < lim).collect();

    // cancel near-DC pole/zero pairs, pin the rest
    let tiny = |r: &C| r.mag() < 1.0;
    for i in (0..p.len()).rev() {
        if tiny(&p[i]) {
            if let Some(j) = z.iter().position(tiny) {
                z.remove(j);
                p.remove(i);
            } else {
                p[i] = C::new(-2.0 * std::f64::consts::PI * 0.5, 0.0);
            }
        }
    }
    for r in z.iter_mut() {
        if tiny(r) {
            *r = C::new(0.0, 0.0);
        }
    }

    let warp = |r: C| -> C {
        let m = r.mag();
        if m == 0.0 {
            return r;
        }
        let k = 2.0 * fs * (m / (2.0 * fs)).tan() / m;
        C::new(r.re * k, r.im * k)
    };
    let bil = |r: C| -> C {
        let r = warp(r);
        let a = 2.0 * fs;
        let nr = a + r.re;
        let ni = r.im;
        let dr = a - r.re;
        let di = -r.im;
        let d = dr * dr + di * di;
        C::new((nr * dr + ni * di) / d, (ni * dr - nr * di) / d)
    };

    let mut zd: Vec<C> = z.iter().map(|&r| bil(r)).collect();
    let mut pd: Vec<C> = p.iter().map(|&r| bil(r)).collect();
    while zd.len() < pd.len() {
        zd.push(C::new(-1.0, 0.0));
    }
    while pd.len() < zd.len() {
        pd.push(C::new(0.0, 0.0));
    }

    // pair roots into quadratics: conjugate pairs first, then reals two at a time
    let pair_up = |rs: &[C]| -> Vec<[f64; 2]> {
        let cx: Vec<C> = rs.iter().filter(|r| r.im > 1e-12).cloned().collect();
        let mut re: Vec<C> = rs.iter().filter(|r| r.im.abs() <= 1e-12).cloned().collect();
        re.sort_by(|a, b| b.re.partial_cmp(&a.re).unwrap());
        let mut out = Vec::new();
        for r in &cx {
            out.push([-2.0 * r.re, r.re * r.re + r.im * r.im]);
        }
        let mut i = 0;
        while i < re.len() {
            if i + 1 < re.len() {
                out.push([-(re[i].re + re[i + 1].re), re[i].re * re[i + 1].re]);
            } else {
                out.push([-re[i].re, 0.0]);
            }
            i += 2;
        }
        out
    };

    let mut nq = pair_up(&zd);
    let mut dq = pair_up(&pd);
    while nq.len() < dq.len() {
        nq.push([0.0, 0.0]);
    }
    while dq.len() < nq.len() {
        dq.push([0.0, 0.0]);
    }

    let mut secs: Vec<Biquad> = dq
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let mut b = Biquad::new();
            b.b0 = 1.0;
            b.b1 = nq[i][0];
            b.b2 = nq[i][1];
            b.a1 = d[0];
            b.a2 = d[1];
            b
        })
        .collect();

    // normalise to unity magnitude at 1 kHz
    let w = 2.0 * std::f64::consts::PI * 1000.0 / fs;
    let mut g = 1.0;
    for b in &secs {
        let cr = w.cos();
        let ci = -w.sin();
        let c2r = (2.0 * w).cos();
        let c2i = -(2.0 * w).sin();
        let nr = b.b0 + b.b1 * cr + b.b2 * c2r;
        let ni = b.b1 * ci + b.b2 * c2i;
        let dr = 1.0 + b.a1 * cr + b.a2 * c2r;
        let di = b.a1 * ci + b.a2 * c2i;
        g *= nr.hypot(ni) / dr.hypot(di);
    }
    if let Some(first) = secs.first_mut() {
        first.b0 /= g;
        first.b1 /= g;
        first.b2 /= g;
    }

    (secs, out)
}

// magnitude of a set of analog roots at f, relative to 1 kHz
pub fn roots_mag(rs: &[C], f: f64, is_pole: bool) -> f64 {
    let mut g = 1.0;
    for r in rs {
        let m = (-r.re).hypot(2.0 * std::f64::consts::PI * f - r.im) / (-r.re).hypot(2.0 * std::f64::consts::PI * 1000.0 - r.im);
        g *= if is_pole { 1.0 / m } else { m };
    }
    g
}
