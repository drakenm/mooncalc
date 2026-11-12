//! Chebyshev series evaluation for SPK types 2 and 3.
//!
//! A component is stored as coefficients `c_0 ... c_n` of
//! `f(s) = Σ c_k T_k(s)` on the normalised interval `s ∈ [-1, 1]`, where
//! `T_0 = 1`, `T_1 = s`, `T_{k+1} = 2 s T_k − T_{k−1}` (*SPK Required
//! Reading*, "Type 2: Chebyshev (position only)"; Press et al., *Numerical
//! Recipes*, 3rd ed., §5.8). The derivative follows from
//! `T'_{k+1} = 2 T_k + 2 s T'_k − T'_{k−1}`, as in SPICE's `CHBINT`.

/// `Σ c_k T_k(s)` by Clenshaw's recurrence.
pub(crate) fn value(c: &[f64], s: f64) -> f64 {
    let (mut b1, mut b2) = (0.0, 0.0);
    for &ck in c.iter().skip(1).rev() {
        let b0 = 2.0 * s * b1 - b2 + ck;
        b2 = b1;
        b1 = b0;
    }
    match c.first() {
        Some(&c0) => c0 + s * b1 - b2,
        None => 0.0,
    }
}

/// `(Σ c_k T_k(s), Σ c_k T'_k(s))`, the value and its derivative with
/// respect to `s`. The value is bit-identical to [`value`], so positions do
/// not depend on whether a velocity was requested.
pub(crate) fn value_and_derivative(c: &[f64], s: f64) -> (f64, f64) {
    (value(c, s), derivative(c, s))
}

/// `Σ c_k T'_k(s)`.
fn derivative(c: &[f64], s: f64) -> f64 {
    let (mut t_prev, mut t) = (1.0, s);
    let (mut d_prev, mut d) = (0.0, 1.0);
    let mut dv = 0.0;
    for (k, &ck) in c.iter().enumerate().skip(1) {
        if k > 1 {
            let t_next = 2.0 * s * t - t_prev;
            let d_next = 2.0 * t + 2.0 * s * d - d_prev;
            t_prev = t;
            t = t_next;
            d_prev = d;
            d = d_next;
        }
        dv += ck * d;
    }
    dv
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed forms: T2 = 2s²−1, T3 = 4s³−3s, T4 = 8s⁴−8s²+1.
    fn direct(c: &[f64; 5], s: f64) -> (f64, f64) {
        let t = [
            1.0,
            s,
            2.0 * s * s - 1.0,
            4.0 * s.powi(3) - 3.0 * s,
            8.0 * s.powi(4) - 8.0 * s * s + 1.0,
        ];
        let d = [
            0.0,
            1.0,
            4.0 * s,
            12.0 * s * s - 3.0,
            32.0 * s.powi(3) - 16.0 * s,
        ];
        let v = c.iter().zip(t).map(|(a, b)| a * b).sum();
        let dv = c.iter().zip(d).map(|(a, b)| a * b).sum();
        (v, dv)
    }

    #[test]
    fn matches_closed_forms() {
        let c = [1.5, -2.25, 0.75, 3.0, -0.5];
        for i in 0..=20 {
            let s = -1.0 + 0.1 * f64::from(i);
            let (v, dv) = direct(&c, s);
            assert!((value(&c, s) - v).abs() < 1e-12);
            let (v2, dv2) = value_and_derivative(&c, s);
            assert!((v2 - v).abs() < 1e-12);
            assert!((dv2 - dv).abs() < 1e-12);
        }
    }

    #[test]
    fn short_series() {
        assert_eq!(value(&[], 0.3), 0.0);
        assert_eq!(value(&[4.0], 0.3), 4.0);
        assert_eq!(value_and_derivative(&[4.0], 0.3), (4.0, 0.0));
        assert_eq!(value_and_derivative(&[4.0, 2.0], 0.5), (5.0, 2.0));
    }
}
