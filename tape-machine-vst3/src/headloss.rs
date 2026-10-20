// play-head loss magnitude (eq. 13, 14)
pub fn head_loss(f: f64, v: f64, d: f64, del: f64, g: f64) -> f64 {
    let k = 2.0 * std::f64::consts::PI * f / v;
    let space = (-k * d).exp();
    let thick = if k * del < 1e-9 { 1.0 } else { (1.0 - (-k * del).exp()) / (k * del) };
    let x = k * g / 2.0;
    let gap = if x < 1e-9 { 1.0 } else { x.sin() / x };
    space * thick * gap
}
