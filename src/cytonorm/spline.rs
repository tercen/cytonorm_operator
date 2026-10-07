//! The monotone spline, in the flavour runs 7–9 use.
//!
//! cytonormpy fits a cubic Hermite spline through (batch quantile → goal quantile) and by
//! default chooses its tangents with Fritsch–Carlson. Runs 7–9 replace that with **PCHIP**
//! tangents and **slope-1** linear extrapolation, because the default overshoots
//! (`cytonorm_limits_test/README.md`). This implements the replacement, since reproducing the
//! library's default would not reproduce the study.
//!
//! Outside the knots the map is `y_end + (x − x_end)`: values beyond the reference range are
//! shifted, never stretched, which is what keeps a tail from being amplified.

/// A fitted map from one batch's distribution onto the goal.
#[derive(Debug, Clone)]
pub enum Spline {
    /// Nothing to correct: fewer than two distinct points, or a cluster that was not fitted.
    Identity,
    Hermite {
        x: Vec<f64>,
        y: Vec<f64>,
        m: Vec<f64>,
    },
}

impl Spline {
    /// Fit `x → y`. Both are quantiles, so they arrive sorted but may contain ties.
    pub fn fit(x: &[f64], y: &[f64]) -> Self {
        let (x, y) = regularize(x, y);
        if x.len() < 2 || x.iter().all(|v| *v == x[0]) || y.iter().all(|v| *v == y[0]) {
            return Spline::Identity;
        }
        let m = pchip_tangents(&x, &y);
        Spline::Hermite { x, y, m }
    }

    pub fn eval(&self, v: f64) -> f64 {
        let (x, y, m) = match self {
            Spline::Identity => return v,
            Spline::Hermite { x, y, m } => (x, y, m),
        };
        if !v.is_finite() {
            return v;
        }
        // Outside the knots: slope 1 from the end point.
        if v <= x[0] {
            return y[0] + (v - x[0]);
        }
        let last = x.len() - 1;
        if v >= x[last] {
            return y[last] + (v - x[last]);
        }
        // Binary search for the interval, then the Hermite basis.
        let i = match x.binary_search_by(|p| p.partial_cmp(&v).unwrap()) {
            Ok(i) => return y[i],
            Err(i) => i - 1,
        };
        let h = x[i + 1] - x[i];
        let t = (v - x[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * y[i] + h10 * h * m[i] + h01 * y[i + 1] + h11 * h * m[i + 1]
    }
}

/// R's `regularize.values`: sort by x, then collapse ties in x by averaging their y.
fn regularize(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut pairs: Vec<(f64, f64)> = x
        .iter()
        .zip(y)
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .map(|(a, b)| (*a, *b))
        .collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < pairs.len() {
        let mut j = i;
        let mut sum = 0.0;
        while j < pairs.len() && pairs[j].0 == pairs[i].0 {
            sum += pairs[j].1;
            j += 1;
        }
        xs.push(pairs[i].0);
        ys.push(sum / (j - i) as f64);
        i = j;
    }
    (xs, ys)
}

/// Tangents as `scipy.interpolate.PchipInterpolator` computes them.
fn pchip_tangents(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let hk: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let mk: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / hk[i]).collect();
    if n == 2 {
        return vec![mk[0], mk[0]];
    }
    let mut d = vec![0.0; n];
    for i in 1..n - 1 {
        let (m0, m1) = (mk[i - 1], mk[i]);
        // A sign change or a flat segment means a local extremum: the tangent is zero, which is
        // what keeps the interpolant monotone.
        if m0.signum() != m1.signum() || m0 == 0.0 || m1 == 0.0 {
            d[i] = 0.0;
        } else {
            let w1 = 2.0 * hk[i] + hk[i - 1];
            let w2 = hk[i] + 2.0 * hk[i - 1];
            d[i] = (w1 + w2) / (w1 / m0 + w2 / m1);
        }
    }
    d[0] = edge_case(hk[0], hk[1], mk[0], mk[1]);
    d[n - 1] = edge_case(hk[n - 2], hk[n - 3], mk[n - 2], mk[n - 3]);
    d
}

/// scipy's one-sided three-point estimate at an end point, with its shape guards.
fn edge_case(h0: f64, h1: f64, m0: f64, m1: f64) -> f64 {
    let d = ((2.0 * h0 + h1) * m0 - h0 * m1) / (h0 + h1);
    if d.signum() != m0.signum() {
        0.0
    } else if m0.signum() != m1.signum() && d.abs() > 3.0 * m0.abs() {
        3.0 * m0
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_when_there_is_nothing_to_map() {
        assert!(matches!(
            Spline::fit(&[1.0, 1.0], &[2.0, 3.0]),
            Spline::Identity
        ));
        assert_eq!(Spline::Identity.eval(7.5), 7.5);
    }

    #[test]
    fn knots_are_reproduced_exactly() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 2.0, 3.0, 6.0];
        let s = Spline::fit(&x, &y);
        for (xi, yi) in x.iter().zip(&y) {
            assert!((s.eval(*xi) - yi).abs() < 1e-12, "at {xi}");
        }
    }

    #[test]
    fn outside_the_knots_the_slope_is_one() {
        let s = Spline::fit(&[0.0, 1.0, 2.0], &[10.0, 11.5, 13.0]);
        assert!((s.eval(-5.0) - 5.0).abs() < 1e-12);
        assert!((s.eval(7.0) - 18.0).abs() < 1e-12);
    }

    #[test]
    fn ties_in_x_are_collapsed_by_averaging() {
        let (x, y) = regularize(&[1.0, 1.0, 2.0], &[10.0, 20.0, 30.0]);
        assert_eq!(x, vec![1.0, 2.0]);
        assert_eq!(y, vec![15.0, 30.0]);
    }

    #[test]
    fn a_monotone_sample_stays_monotone() {
        let s = Spline::fit(&[0.0, 1.0, 2.0, 3.0, 4.0], &[0.0, 0.1, 5.0, 5.1, 9.0]);
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=400 {
            let v = s.eval(k as f64 / 100.0);
            assert!(v >= prev - 1e-12, "not monotone at {k}");
            prev = v;
        }
    }
}
