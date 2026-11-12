//! Scalar root finding shared by the event searches.

/// Brent's method: a root of `f` in `[a, b]`, where `fa = f(a)` and
/// `fb = f(b)` have opposite signs (or one of them is zero). Converges to
/// within `tol` (absolute, in the units of `x`).
///
/// Reference: R. P. Brent, *Algorithms for Minimization without
/// Derivatives* (1973), ch. 4, in the form of Press et al., *Numerical
/// Recipes* (2nd ed.), §9.3 (`zbrent`).
pub fn brent<F: FnMut(f64) -> f64>(mut f: F, a: f64, b: f64, fa: f64, fb: f64, tol: f64) -> f64 {
    if fa == 0.0 {
        return a;
    }
    if fb == 0.0 {
        return b;
    }
    let (mut a, mut b, mut fa, mut fb) = (a, b, fa, fb);
    let (mut c, mut fc) = (b, fb);
    let mut d = b - a;
    let mut e = d;
    for _ in 0..200 {
        if (fb > 0.0) == (fc > 0.0) {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * f64::EPSILON * b.abs() + 0.5 * tol;
        let xm = 0.5 * (c - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return b;
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            // Inverse quadratic interpolation, or secant when a == c.
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * xm * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * xm * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            }
            p = p.abs();
            let min1 = 3.0 * xm * q - (tol1 * q).abs();
            let min2 = (e * q).abs();
            if 2.0 * p < min1.min(min2) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol1 { d } else { tol1.copysign(xm) };
        fb = f(b);
    }
    b
}

/// Sign changes of `f` over sorted, strictly increasing sample points.
/// Returns `(a, b, f(a), f(b))` for every adjacent pair whose values differ
/// in sign (a zero counts with the following positive or negative run).
pub fn sign_changes<F: FnMut(f64) -> f64>(mut f: F, nodes: &[f64]) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for &x in nodes {
        let fx = f(x);
        if let Some((xp, fp)) = prev
            && ((fp < 0.0 && fx >= 0.0) || (fp >= 0.0 && fx < 0.0))
        {
            out.push((xp, x, fp, fx));
        }
        prev = Some((x, fx));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brent_finds_roots() {
        let mut calls = 0;
        let r = brent(
            |x| {
                calls += 1;
                x * x * x - 2.0 * x - 5.0
            },
            2.0,
            3.0,
            -1.0,
            16.0,
            1e-12,
        );
        assert!((r - 2.094_551_481_542_327).abs() < 1e-11, "{r}");
        assert!(calls < 15, "{calls}");
        assert_eq!(brent(|x| x, 0.0, 1.0, 0.0, 1.0, 1e-9), 0.0);
        let c = brent(f64::cos, 0.0, 3.0, 1.0, 3f64.cos(), 1e-13);
        assert!((c - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn sign_changes_brackets() {
        let nodes: Vec<f64> = (0..=10).map(f64::from).collect();
        let b = sign_changes(|x| (x - 2.5) * (x - 7.5), &nodes);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].0, b[0].1), (2.0, 3.0));
        assert_eq!((b[1].0, b[1].1), (7.0, 8.0));
    }
}
