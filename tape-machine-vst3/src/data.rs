// Direct port of the data tables (MACHINES, TAPES, BIASCAL, J37E, A80E, A77E, TC260E, ATR100E,
// MCAL_CIRCUIT) from tape-core.js. Values are transcribed verbatim from the JS source; see that
// file's comments for sourcing and caveats (schematic-derived for J37/A80, NAB-spec reasoned
// estimates for A77/TC-260/ATR-100).

use std::collections::HashMap;

pub type Zpk = (&'static [[f64; 2]], &'static [[f64; 2]]); // (zeros, poles)

// ---------- tape formulations ----------
#[derive(Clone, Copy, Debug)]
pub struct Tape {
    pub label: &'static str,
    pub ms: f64,
    pub a: f64,
    pub k: f64,
    pub c: f64,
    pub alpha: f64,
    pub bpeak: f64,
    pub g0: f64,
    pub rec: [f64; 12],
}

pub const TAPE_FERRIC: Tape = Tape { label: "Standard ferric", ms: 350000.0, a: 22000.0, k: 27000.0, c: 0.17, alpha: 0.0016, bpeak: 8561.0, g0: 5.2637, rec: [0.05, 0.0, -0.05, -0.34, -0.77, -1.88, -3.09, -4.74, -5.88, -6.9, -9.18, -14.16] };
pub const TAPE_STUDIO: Tape = Tape { label: "Audex 456", ms: 350141.0, a: 20778.0, k: 25500.0, c: 0.17, alpha: 0.0016, bpeak: 8085.0, g0: 5.5784, rec: [0.05, 0.0, -0.05, -0.34, -0.77, -1.88, -3.09, -4.74, -5.88, -6.91, -9.18, -14.17] };
pub const TAPE_SM468: Tape = Tape { label: "Nordtape SM-68", ms: 372966.0, a: 24444.0, k: 30000.0, c: 0.17, alpha: 0.0016, bpeak: 9512.0, g0: 5.0466, rec: [0.05, 0.0, -0.05, -0.34, -0.77, -1.88, -3.08, -4.73, -5.88, -6.9, -9.18, -14.16] };
pub const TAPE_A632: Tape = Tape { label: "Audex 632", ms: 159659.0, a: 20126.0, k: 24700.0, c: 0.17, alpha: 0.0016, bpeak: 7831.0, g0: 2.6137, rec: [0.05, 0.0, -0.05, -0.33, -0.76, -1.87, -3.07, -4.72, -5.87, -6.88, -9.16, -14.14] };
pub const TAPE_COBALT: Tape = Tape { label: "Cobalt-ferric", ms: 366056.0, a: 48889.0, k: 60000.0, c: 0.17, alpha: 0.0016, bpeak: 19024.0, g0: 2.4664, rec: [0.05, 0.0, -0.05, -0.33, -0.76, -1.87, -3.07, -4.72, -5.86, -6.88, -9.16, -14.14] };
pub const TAPE_CHROME: Tape = Tape { label: "Chrome", ms: 477465.0, a: 57037.0, k: 70000.0, c: 0.17, alpha: 0.0016, bpeak: 22194.0, g0: 2.7587, rec: [0.05, 0.0, -0.05, -0.33, -0.76, -1.87, -3.07, -4.72, -5.87, -6.89, -9.16, -14.14] };
pub const TAPE_METAL: Tape = Tape { label: "Metal particle", ms: 1671127.0, a: 101852.0, k: 125000.0, c: 0.17, alpha: 0.0016, bpeak: 39632.0, g0: 5.43, rec: [0.05, 0.0, -0.05, -0.34, -0.77, -1.88, -3.09, -4.74, -5.88, -6.9, -9.18, -14.16] };

pub fn tape_by_id(id: &str) -> Tape {
    match id {
        "studio" => TAPE_STUDIO,
        "sm468" => TAPE_SM468,
        "a632" => TAPE_A632,
        "cobalt" => TAPE_COBALT,
        "chrome" => TAPE_CHROME,
        "metal" => TAPE_METAL,
        _ => TAPE_FERRIC,
    }
}

pub const TAPE_IDS: [&str; 7] = ["ferric", "studio", "sm468", "a632", "cobalt", "chrome", "metal"];

// ---------- machines (published specs) ----------
#[derive(Clone, Debug)]
pub struct Machine {
    pub id: &'static str,
    pub label: &'static str,
    pub fb: f64,
    pub speeds: &'static [f64],
    pub refspeed: f64,
    pub noise: &'static [(f64, f64)], // speed -> dBFS
    pub wf: &'static [(f64, f64)],    // speed -> peak speed error
}

pub const MACH_TC260: Machine = Machine { id: "tc260", label: "Tonica TC-6", fb: 55000.0, speeds: &[3.75, 7.5], refspeed: 7.5, noise: &[(3.75, -56.0), (7.5, -56.0)], wf: &[(3.75, 0.0025), (7.5, 0.0019)] };
pub const MACH_J37: Machine = Machine { id: "j37", label: "Stellan J-37", fb: 80000.0, speeds: &[7.5, 15.0], refspeed: 15.0, noise: &[(7.5, -74.0), (15.0, -78.0)], wf: &[(7.5, 0.0015), (15.0, 0.0010)] };
pub const MACH_A77: Machine = Machine { id: "a77", label: "Arvox A-7", fb: 120000.0, speeds: &[3.75, 7.5], refspeed: 7.5, noise: &[(3.75, -81.0), (7.5, -84.0)], wf: &[(3.75, 0.0010), (7.5, 0.0008)] };
pub const MACH_A80: Machine = Machine { id: "a80", label: "Stellan A-80", fb: 240000.0, speeds: &[7.5, 15.0], refspeed: 15.0, noise: &[(7.5, -78.0), (15.0, -80.0)], wf: &[(7.5, 0.0012), (15.0, 0.0008)] };
pub const MACH_ATR100: Machine = Machine { id: "atr100", label: "Apexa MT-100", fb: 432000.0, speeds: &[3.75, 7.5, 15.0, 30.0], refspeed: 15.0, noise: &[(3.75, -78.0), (7.5, -78.0), (15.0, -80.0), (30.0, -80.0)], wf: &[(3.75, 0.0010), (7.5, 0.0006), (15.0, 0.0004), (30.0, 0.0003)] };

pub fn machine_by_id(id: &str) -> &'static Machine {
    match id {
        "tc260" => &MACH_TC260,
        "a77" => &MACH_A77,
        "a80" => &MACH_A80,
        "atr100" => &MACH_ATR100,
        _ => &MACH_J37,
    }
}

pub const MACHINE_IDS: [&str; 5] = ["j37", "a80", "a77", "tc260", "atr100"];

pub fn lookup(table: &[(f64, f64)], speed: f64) -> f64 {
    for &(s, v) in table {
        if (s - speed).abs() < 1e-9 {
            return v;
        }
    }
    table[0].1
}

// simulated bias frequency: the machine's, capped at a third of the internal rate, locked to fso/N
pub fn sim_bias_freq(fs: f64, os: f64, target: f64) -> f64 {
    let fso = fs * os;
    let t = target.min(fso / 3.0);
    fso / (fso / t).round().max(3.0)
}

// ---------- ferric small-signal calibration per simulated bias frequency (cal5.js) ----------
pub struct BiasCalEntry {
    pub fb: f64,
    pub bpeak: f64,
    pub g0: f64,
    pub rec: [f64; 12],
}

pub const BIASCAL: [BiasCalEntry; 10] = [
    BiasCalEntry { fb: 54277.0, bpeak: 9154.0, g0: 5.2444, rec: [0.06, 0.0, -0.06, -0.39, -0.89, -2.14, -3.43, -5.13, -6.28, -7.29, -9.77, -15.03] },
    BiasCalEntry { fb: 54857.0, bpeak: 9154.0, g0: 5.2457, rec: [0.06, 0.0, -0.06, -0.39, -0.89, -2.13, -3.43, -5.13, -6.28, -7.22, -8.52, -10.74] },
    BiasCalEntry { fb: 58800.0, bpeak: 8147.0, g0: 5.2826, rec: [0.03, 0.0, -0.04, -0.24, -0.55, -1.41, -2.41, -3.9, -5.01, -6.06, -8.56, -13.67] },
    BiasCalEntry { fb: 76800.0, bpeak: 8147.0, g0: 5.2728, rec: [0.04, 0.0, -0.04, -0.27, -0.62, -1.55, -2.62, -4.17, -5.29, -6.27, -7.66, -9.93] },
    BiasCalEntry { fb: 78400.0, bpeak: 7686.0, g0: 5.2758, rec: [0.04, 0.0, -0.04, -0.28, -0.64, -1.61, -2.71, -4.28, -5.42, -6.48, -9.04, -14.3] },
    BiasCalEntry { fb: 88200.0, bpeak: 6453.0, g0: 5.2973, rec: [0.03, 0.0, -0.03, -0.19, -0.45, -1.18, -2.07, -3.44, -4.51, -5.56, -8.08, -13.19] },
    BiasCalEntry { fb: 117600.0, bpeak: 6841.0, g0: 5.2998, rec: [0.03, 0.0, -0.03, -0.18, -0.42, -1.11, -1.95, -3.27, -4.32, -5.36, -7.88, -13.0] },
    BiasCalEntry { fb: 128000.0, bpeak: 6453.0, g0: 5.3041, rec: [0.02, 0.0, -0.03, -0.17, -0.4, -1.06, -1.88, -3.17, -4.2, -5.17, -6.59, -8.9] },
    BiasCalEntry { fb: 235200.0, bpeak: 4822.0, g0: 5.3237, rec: [0.01, 0.0, -0.01, -0.09, -0.22, -0.61, -1.13, -2.05, -2.86, -3.75, -6.23, -11.43] },
    BiasCalEntry { fb: 256000.0, bpeak: 4549.0, g0: 5.3259, rec: [0.01, 0.0, -0.01, -0.09, -0.21, -0.59, -1.08, -1.97, -2.76, -3.56, -4.9, -7.23] },
];

pub fn bias_cal(fb: f64) -> &'static BiasCalEntry {
    let mut best = &BIASCAL[0];
    for c in BIASCAL.iter() {
        if (c.fb - fb).abs() < (best.fb - fb).abs() {
            best = c;
        }
    }
    best
}

// Record-side small-signal sensitivity table frequencies, dB re 700 Hz
pub const REC_F: [f64; 12] = [50.0, 700.0, 1000.0, 2000.0, 3000.0, 5000.0, 7000.0, 10000.0, 12500.0, 15000.0, 18000.0, 20000.0];

pub fn rec_sens(f: f64, rec_db: &[f64]) -> f64 {
    if f <= REC_F[0] {
        return 10f64.powf(rec_db[0] / 20.0);
    }
    for i in 1..REC_F.len() {
        if f <= REC_F[i] {
            let t = (f.ln() - REC_F[i - 1].ln()) / (REC_F[i].ln() - REC_F[i - 1].ln());
            return 10f64.powf((rec_db[i - 1] + t * (rec_db[i] - rec_db[i - 1])) / 20.0);
        }
    }
    10f64.powf(rec_db[rec_db.len() - 1] / 20.0)
}

// ---------- Studer J37 electronics ----------
pub struct V2bParams {
    pub ia0: f64,
    pub va0: f64,
    pub k: f64,
    pub mu: f64,
    pub r28: f64,
    pub r29: f64,
    pub c19: f64,
    pub boot: f64,
    pub vg0vu: f64,
}

pub const J37_V2B: V2bParams = V2bParams { ia0: 0.003425707757658497, va0: 114.8931526903117, k: 0.006211299937499417, mu: 33.0, r28: 820.0, r29: 2700.0, c19: 3.9e-09, boot: 0.91, vg0vu: 3.5546150432207955 };

pub const J37_DEPTH_FRAC: f64 = 0.3;
pub const J37_DREC: f64 = 7e-06;
pub const J37_XFLF: f64 = 30.0;
pub const J37_OUTMAXDB: f64 = 13.1;

// over-bias at 15 ips per the J37 manual (E2b)
pub const J37_OB_RATIO: f64 = 5.15086657041684;
pub const J37_OB_RECOVER: [f64; 12] = [0.0, 0.0, 0.0, 0.0, -0.01, -0.02, -0.04, -0.07, -0.11, -0.19, -0.7, -2.6];

pub const J37_15_REC_Z: [[f64; 2]; 9] = [[49454766.71060157, 0.0], [-99733.32769541717, 0.0], [-20201.996877982412, 0.0], [-3.757504243114192, 0.0], [-0.6935283952047023, 2.3394445315644306], [-0.6935283952047022, -2.3394445315644306], [-5.98983527195839e-14, 0.0], [-377.0739064856712, 0.0], [0.0, 0.0]];
pub const J37_15_REC_P: [[f64; 2]; 9] = [[-220987.36324372652, 682963.1814948489], [-220987.36324372652, -682963.1814948489], [-20669.725583419153, 0.0], [-205.28550869199026, 0.0], [-59.541253067963005, 0.0], [-5.773564567900404, 18.374662760103615], [-5.773564567900405, -18.374662760103615], [-4.787869911230623, 0.0], [-0.08755989642798524, 0.0]];
pub const J37_15_PB_Z: [[f64; 2]; 9] = [[-27502.753681670107, 0.0], [-3.030303030303682, 0.0], [-1.5361652692817603e-05, 0.022324963689138163], [-1.5361652692817603e-05, -0.022324963689138163], [3.6894340682874343e-07, 503856.6671189827], [3.6894340682874343e-07, -503856.66711898276], [3.171212894700336e-10, 0.0], [0.0, 0.0], [-61728.3950617284, 0.0]];
pub const J37_15_PB_P: [[f64; 2]; 10] = [[-162101.22884042736, 404584.49436562165], [-162101.2288404274, -404584.49436562165], [-234245.10366901406, 0.0], [-148736.2676802667, 0.0], [-6.088267964434135, 111.24477388523107], [-6.088267964434134, -111.24477388523106], [-77.47752903176324, 0.0], [-0.6514351310674275, 0.0], [-24.382628091372393, 0.0], [-19.55047023488088, 0.0]];

pub const J37_75_REC_Z: [[f64; 2]; 9] = [[32726869.207484733, 0.0], [-22874.320182729833, 0.0], [-20201.861026249913, 0.0], [-3.757564193974944, 0.0], [-0.6934392510391272, 2.3394681700073274], [-0.6934392510391271, -2.339468170007327], [-5.843800678000232e-14, 0.0], [-377.0739064856712, 0.0], [0.0, 0.0]];
pub const J37_75_REC_P: [[f64; 2]; 9] = [[-161974.4932583515, 295823.16272841796], [-161974.49325835146, -295823.16272841796], [-20291.88046388882, 0.0], [-205.2608035351867, 0.0], [-59.54120812931653, 0.0], [-5.778747495538596, 18.370024575718972], [-5.778747495538596, -18.370024575718976], [-4.787871689726251, 0.0], [-0.08754195784563197, 0.0]];
pub const J37_75_PB_Z: [[f64; 2]; 9] = [[-13819.792143349658, 0.0], [-3.0303030303031653, 0.0], [-1.535261858983359e-05, 0.022324963696223957], [-1.535261858983359e-05, -0.022324963696223957], [5.97058085680512e-07, 503856.6671189863], [5.97058085680512e-07, -503856.66711898625], [2.392787748530492e-10, 0.0], [0.0, 0.0], [-32679.7385620915, 0.0]];
pub const J37_75_PB_P: [[f64; 2]; 10] = [[-161933.9777186809, 404408.29052655207], [-161933.9777186809, -404408.29052655207], [-216703.45359645857, 0.0], [-85257.1062505636, 0.0], [-6.272889231301795, 111.1271096112849], [-6.272889231301793, -111.1271096112849], [-77.4177210371205, 0.0], [-0.6514349206434099, 0.0], [-24.379024408131354, 0.0], [-19.551684687480016, 0.0]];

// ---------- Studer A80 electronics ----------
pub const A80_DEPTH_FRAC: f64 = 0.3;
pub const A80_DREC: f64 = 7e-06;
pub const A80_XFLF: f64 = 5.0;
pub const A80_OUTMAXDB: f64 = 13.4;
pub const A80_RECCLIPDB: f64 = 26.9;
pub const A80_DRVCLIPDB: f64 = 35.4;
pub const A80_PBCLIPDB: f64 = 12.9;

pub const A80_15_REC_Z: [[f64; 2]; 4] = [[-3806.2993337014027, 0.0], [-40228.2615377378, 0.0], [0.0, 235440.80467400557], [0.0, -235440.80467400557]];
pub const A80_15_REC_P: [[f64; 2]; 5] = [[-4369.347257534813, 0.0], [-1884891.2574584577, 0.0], [-269905.5329734331, 0.0], [-48520.13584022254, 182066.84170805916], [-48520.13584022255, -182066.84170805916]];
pub const A80_15_PB_Z: [[f64; 2]; 6] = [[-28571.428571428572, 0.0], [-68102.35270789223, 0.0], [-2.7499401078981586e-07, 1512722.552030425], [-2.7499401078981586e-07, -1512722.5520304246], [9.157394656692238e-09, 501254.70711633534], [9.157394656692238e-09, -501254.7071163353]];
pub const A80_15_PB_P: [[f64; 2]; 7] = [[-18.84955592153876, 0.0], [-251327.41228718346, 0.0], [-1144.0692088398866, 1512721.5905617287], [-1144.0692088398866, -1512721.5905617287], [-455000.29624770087, 0.0], [-125.57150206644721, 501254.69958941184], [-125.57150206644721, -501254.69958941184]];

pub const A80_75_REC_Z: [[f64; 2]; 4] = [[-42221.15761447034, 0.0], [-42216.85822190978, 0.0], [0.0, 235440.80467400557], [0.0, -235440.80467400557]];
pub const A80_75_REC_P: [[f64; 2]; 5] = [[-1884767.482423296, 0.0], [-1884891.2574584577, 0.0], [-269905.5329734331, 0.0], [-48520.13584022254, 182066.84170805916], [-48520.13584022255, -182066.84170805916]];
pub const A80_75_PB_Z: [[f64; 2]; 6] = [[-14285.714285714286, 0.0], [-42872.51841634979, 0.0], [-2.7499401078981586e-07, 1512722.552030425], [-2.7499401078981586e-07, -1512722.5520304246], [9.157394656692238e-09, 501254.70711633534], [9.157394656692238e-09, -501254.7071163353]];
pub const A80_75_PB_P: [[f64; 2]; 7] = [[-18.84955592153876, 0.0], [-251327.41228718346, 0.0], [-1144.0692088398866, 1512721.5905617287], [-1144.0692088398866, -1512721.5905617287], [-455000.29624770087, 0.0], [-125.57150206644721, 501254.69958941184], [-125.57150206644721, -501254.69958941184]];

// ---------- A77/TC-260/ATR-100: NAB time-constant based (reasoned, not schematic-solved) ----------
const NAB_HI_7_5: [[f64; 2]; 1] = [[-20000.0, 0.0]];
const NAB_HI_3_75: [[f64; 2]; 1] = [[-11111.111111111111, 0.0]];
const NAB_LO_P: [[f64; 2]; 2] = [[-314.1592653589793, 0.0], [-18.84955592153876, 0.0]];
const NAB_REC_Z: [[f64; 2]; 1] = [[-314.1592653589793, 0.0]];

pub const A77_DEPTH_FRAC: f64 = 0.04;
pub const A77_DREC: f64 = 0.0000012;
pub const A77_XFLF: f64 = 20.0;
pub const A77_OUTMAXDB: f64 = 14.0;
pub const A77_RECCLIPDB: f64 = 20.0;
pub const A77_DRVCLIPDB: f64 = 26.0;
pub const A77_PBCLIPDB: f64 = 10.0;
pub const A77_375_REC_P: [[f64; 2]; 1] = [[-66666.66666666667, 0.0]];
pub const A77_75_REC_P: [[f64; 2]; 1] = [[-100000.0, 0.0]];

pub const TC260_DEPTH_FRAC: f64 = 0.045;
pub const TC260_DREC: f64 = 0.0000018;
pub const TC260_XFLF: f64 = 50.0;
pub const TC260_OUTMAXDB: f64 = 22.0;
pub const TC260_RECCLIPDB: f64 = 24.0;
pub const TC260_DRVCLIPDB: f64 = 28.0;
pub const TC260_PBCLIPDB: f64 = 20.0;
pub const TC260_375_REC_P: [[f64; 2]; 1] = [[-88888.88888888889, 0.0]];
pub const TC260_75_REC_P: [[f64; 2]; 1] = [[-120000.0, 0.0]];

pub const ATR100_DEPTH_FRAC: f64 = 0.02;
pub const ATR100_DREC: f64 = 5e-7;
pub const ATR100_XFLF: f64 = 10.0;
pub const ATR100_OUTMAXDB: f64 = 28.0;
pub const ATR100_RECCLIPDB: f64 = 30.0;
pub const ATR100_DRVCLIPDB: f64 = 36.0;
pub const ATR100_PBCLIPDB: f64 = 24.0;
pub const ATR100_15_REC_P: [[f64; 2]; 1] = [[-80000.0, 0.0]];
pub const ATR100_30_REC_P: [[f64; 2]; 1] = [[-60000.0, 0.0]];
pub const ATR100_375_REC_P: [[f64; 2]; 1] = [[-66666.66666666667, 0.0]];
pub const ATR100_75_REC_P: [[f64; 2]; 1] = [[-100000.0, 0.0]];

// per-speed zpk data lookup for the NAB-based machines, matching the `.sp[speed]` structure
pub fn nab_sp(machine: &str, speed: f64) -> (Zpk, Zpk) {
    // returns ((pb_z, pb_p), (rec_z, rec_p))
    let pb_z: &'static [[f64; 2]] = if speed <= 5.0 { &NAB_HI_3_75 } else { &NAB_HI_7_5 };
    let pb_p: &'static [[f64; 2]] = &NAB_LO_P;
    let rec_z: &'static [[f64; 2]] = &NAB_REC_Z;
    let rec_p: &'static [[f64; 2]] = match machine {
        "a77" => {
            if speed <= 5.0 { &A77_375_REC_P } else { &A77_75_REC_P }
        }
        "tc260" => {
            if speed <= 5.0 { &TC260_375_REC_P } else { &TC260_75_REC_P }
        }
        "atr100" => {
            if speed <= 5.0 {
                &ATR100_375_REC_P
            } else if speed <= 10.0 {
                &ATR100_75_REC_P
            } else if speed <= 20.0 {
                &ATR100_15_REC_P
            } else {
                &ATR100_30_REC_P
            }
        }
        _ => &A77_75_REC_P,
    };
    ((pb_z, pb_p), (rec_z, rec_p))
}

// ---------- per-machine level calibration (cal5.js, ferric tape, 48 kHz, 8x) ----------
pub struct Cal {
    pub hg: f64,
    pub kappa: f64,
    pub noise: &'static [(f64, f64)],
}

pub const CAL_A80: Cal = Cal { hg: 756565.0, kappa: 0.87136, noise: &[(15.0, 4776800.0), (7.5, 6951700.0)] };
pub const CAL_J37: Cal = Cal { hg: 1540552.0, kappa: 0.7022, noise: &[(15.0, 7755100.0), (7.5, 11141000.0)] };
pub const CAL_A77: Cal = Cal { hg: 3932028.0, kappa: 0.79316, noise: &[(3.75, 17956000.0), (7.5, 16513000.0)] };
pub const CAL_TC260: Cal = Cal { hg: 649105.0, kappa: 0.91795, noise: &[(3.75, 4052000.0), (7.5, 3715500.0)] };
pub const CAL_ATR100: Cal = Cal { hg: 577077.0, kappa: 0.968, noise: &[(15.0, 2752200.0), (30.0, 2735500.0), (3.75, 2996000.0), (7.5, 2780300.0)] };

pub fn cal_by_id(id: &str) -> &'static Cal {
    match id {
        "a80" => &CAL_A80,
        "a77" => &CAL_A77,
        "tc260" => &CAL_TC260,
        "atr100" => &CAL_ATR100,
        _ => &CAL_J37,
    }
}

// convenience: machine metadata needed by the circuit registry
pub struct CircuitMeta {
    pub xflf: f64,
    pub outmaxdb: f64,
    pub depth_frac: f64,
    pub drec: f64,
    pub pbclipdb: f64,
    pub hard_out: bool,
    pub rec_clip_db: f64,
    pub drv_clip_db: f64,
    pub is_valve: bool,
}

pub fn circuit_meta(id: &str) -> CircuitMeta {
    match id {
        "a80" => CircuitMeta { xflf: A80_XFLF, outmaxdb: A80_OUTMAXDB, depth_frac: A80_DEPTH_FRAC, drec: A80_DREC, pbclipdb: A80_PBCLIPDB, hard_out: true, rec_clip_db: A80_RECCLIPDB, drv_clip_db: A80_DRVCLIPDB, is_valve: false },
        "a77" => CircuitMeta { xflf: A77_XFLF, outmaxdb: A77_OUTMAXDB, depth_frac: A77_DEPTH_FRAC, drec: A77_DREC, pbclipdb: A77_PBCLIPDB, hard_out: true, rec_clip_db: A77_RECCLIPDB, drv_clip_db: A77_DRVCLIPDB, is_valve: false },
        "tc260" => CircuitMeta { xflf: TC260_XFLF, outmaxdb: TC260_OUTMAXDB, depth_frac: TC260_DEPTH_FRAC, drec: TC260_DREC, pbclipdb: TC260_PBCLIPDB, hard_out: true, rec_clip_db: TC260_RECCLIPDB, drv_clip_db: TC260_DRVCLIPDB, is_valve: false },
        "atr100" => CircuitMeta { xflf: ATR100_XFLF, outmaxdb: ATR100_OUTMAXDB, depth_frac: ATR100_DEPTH_FRAC, drec: ATR100_DREC, pbclipdb: ATR100_PBCLIPDB, hard_out: true, rec_clip_db: ATR100_RECCLIPDB, drv_clip_db: ATR100_DRVCLIPDB, is_valve: false },
        _ => CircuitMeta { xflf: J37_XFLF, outmaxdb: J37_OUTMAXDB, depth_frac: J37_DEPTH_FRAC, drec: J37_DREC, pbclipdb: 0.0, hard_out: false, rec_clip_db: 0.0, drv_clip_db: 0.0, is_valve: true },
    }
}

// returns (rec_z, rec_p, pb_z, pb_p) for a given machine+speed, as used by zpkToSOS call sites.
// pb_z does NOT yet have the DC zero appended -- callers append [0,0] themselves, matching
// `sp.pb.z.concat([[0, 0]])` in tape-core.js.
pub fn sp_for(machine: &str, speed: f64) -> (&'static [[f64; 2]], &'static [[f64; 2]], &'static [[f64; 2]], &'static [[f64; 2]]) {
    match machine {
        "j37" => {
            if speed >= 10.0 {
                (&J37_15_REC_Z, &J37_15_REC_P, &J37_15_PB_Z, &J37_15_PB_P)
            } else {
                (&J37_75_REC_Z, &J37_75_REC_P, &J37_75_PB_Z, &J37_75_PB_P)
            }
        }
        "a80" => {
            if speed >= 10.0 {
                (&A80_15_REC_Z, &A80_15_REC_P, &A80_15_PB_Z, &A80_15_PB_P)
            } else {
                (&A80_75_REC_Z, &A80_75_REC_P, &A80_75_PB_Z, &A80_75_PB_P)
            }
        }
        _ => {
            let ((pb_z, pb_p), (rec_z, rec_p)) = nab_sp(machine, speed);
            (rec_z, rec_p, pb_z, pb_p)
        }
    }
}

// over-bias table: only J37 at 15 ips
pub fn overbias(machine: &str, speed: f64) -> Option<(f64, &'static [f64; 12])> {
    if machine == "j37" && speed >= 10.0 {
        Some((J37_OB_RATIO, &J37_OB_RECOVER))
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn all_tapes() -> HashMap<&'static str, Tape> {
    let mut m = HashMap::new();
    for id in TAPE_IDS {
        m.insert(id, tape_by_id(id));
    }
    m
}
