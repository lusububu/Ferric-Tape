// circuit registry and circuitFIR: playback FIR for a circuit machine (eq. 13 head losses with
// the effective recorded depth, the lumped record loss, and the EQ roots above the oversampled
// band). Direct port from tape-core.js.

use crate::data::{self, circuit_meta, overbias, sp_for, CircuitMeta};
use crate::headloss::head_loss;
use crate::zpk::{roots_mag, OutOfBand};

pub struct Circuit {
    pub meta: CircuitMeta,
    pub rec_z: &'static [[f64; 2]],
    pub rec_p: &'static [[f64; 2]],
    pub pb_z: &'static [[f64; 2]],
    pub pb_p: &'static [[f64; 2]],
    pub overbias: Option<(f64, &'static [f64; 12])>,
}

pub fn circuit_for(machine: &str, speed: f64) -> Circuit {
    let (rec_z, rec_p, pb_z, pb_p) = sp_for(machine, speed);
    Circuit { meta: circuit_meta(machine), rec_z, rec_p, pb_z, pb_p, overbias: overbias(machine, speed) }
}

// playback FIR for a circuit machine: eq. 13 head losses with the effective recorded depth,
// the lumped record loss, and the EQ roots above the oversampled band
pub struct Fir {
    pub h: Vec<f64>,
    #[allow(dead_code)]
    pub mag: Vec<f64>,
}

pub fn circuit_fir(c: &Circuit, fs: f64, order: usize, speed_ips: f64, spacing_um: f64, thick_um: f64, gap_um: f64, extra: Option<&(OutOfBand, OutOfBand)>) -> Fir {
    let n = order + 1;
    let v = speed_ips * 0.0254;
    let d = spacing_um * 1e-6;
    let del = thick_um * 1e-6 * c.meta.depth_frac;
    let g = gap_um * 1e-6;
    let dr = c.meta.drec;
    let nb = 1024usize;
    let mut mag = vec![0.0f64; nb + 1];
    for i in 0..=nb {
        let f = i as f64 / nb as f64 * fs / 2.0;
        let mut m = head_loss(f, v, d, del, g) * (-2.0 * std::f64::consts::PI * f / v * dr).exp();
        if let Some((dr_oob, dp_oob)) = extra {
            m *= roots_mag(&dr_oob.z, f, false) * roots_mag(&dr_oob.p, f, true);
            m *= roots_mag(&dp_oob.z, f, false) * roots_mag(&dp_oob.p, f, true);
        }
        mag[i] = m;
    }
    let mut h = vec![0.0f64; n];
    let mid = order as f64 / 2.0;
    for ni in 0..n {
        let mut s = 0.0;
        let t = ni as f64 - mid;
        for i in 0..=nb {
            let w = if i == 0 || i == nb { 1.0 } else { 2.0 };
            s += w * mag[i] * (std::f64::consts::PI * i as f64 * t / nb as f64).cos();
        }
        h[ni] = s / (2.0 * nb as f64) * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * ni as f64 / order as f64).cos());
    }
    let dc: f64 = h.iter().sum();
    for v in h.iter_mut() {
        *v /= dc;
    }
    Fir { h, mag }
}

#[allow(dead_code)]
pub fn machine_ids() -> &'static [&'static str] {
    &data::MACHINE_IDS
}
