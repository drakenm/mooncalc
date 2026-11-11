//! Small one-dimensional numerical helpers shared by the event modules
//! (apsides, nodes, eclipses, seasons): extremum and root refinement of a
//! smooth function of time. Standard methods (golden-section search, Brent
//! 1973 "Algorithms for Minimization without Derivatives" ch. 5; the
//! Illinois variant of regula falsi, Dowell & Jarratt 1971).

/// Golden-section search for the minimum of `f` on `[a, b]`, which must
/// contain a single minimum. Returns `(x, f(x))` with `x` within `tol` of
/// the minimum.
pub(crate) fn minimize(
    mut f: impl FnMut(f64) -> f64,
    mut a: f64,
    mut b: f64,
    tol: f64,
) -> (f64, f64) {
    const R: f64 = 0.618_033_988_749_894_8; // (√5 − 1) / 2
    let mut c = b - R * (b - a);
    let mut d = a + R * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);
    while (b - a).abs() > tol {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - R * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + R * (b - a);
            fd = f(d);
        }
    }
    let x = 0.5 * (a + b);
    (x, f(x))
}

/// Golden-section search for the maximum of `f` on `[a, b]`.
pub(crate) fn maximize(mut f: impl FnMut(f64) -> f64, a: f64, b: f64, tol: f64) -> (f64, f64) {
    let (x, fx) = minimize(|t| -f(t), a, b, tol);
    (x, -fx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_section() {
        let (x, fx) = minimize(|x| (x - 1.234).powi(2) + 3.0, -10.0, 10.0, 1e-9);
        assert!((x - 1.234).abs() < 1e-7 && (fx - 3.0).abs() < 1e-12);
        let (x, _) = maximize(|x| x.sin(), 0.0, 3.0, 1e-9);
        assert!((x - std::f64::consts::FRAC_PI_2).abs() < 1e-7);
    }
}
