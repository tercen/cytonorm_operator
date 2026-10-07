//! Quantiles, the way the reference computes them.
//!
//! `numba_quantiles` interpolates linearly at `p·(n−1)` on the sorted values, which is numpy's
//! default and R's type 7. The probabilities themselves are `i/(NQ+1)` for `i = 1..NQ`, so 99
//! quantiles are 0.01 … 0.99 — the setting runs 7–9 use, not the notebook's 101.

/// `i/(n+1)` for `i = 1..n`.
pub fn probabilities(n: usize) -> Vec<f64> {
    (1..=n).map(|i| i as f64 / (n as f64 + 1.0)).collect()
}

/// Quantiles of `values` at `probs`. `values` is sorted in place.
pub fn quantiles(values: &mut [f64], probs: &[f64]) -> Vec<f64> {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    quantiles_sorted(values, probs)
}

/// Quantiles of an already sorted sample.
pub fn quantiles_sorted(sorted: &[f64], probs: &[f64]) -> Vec<f64> {
    if sorted.is_empty() {
        return vec![f64::NAN; probs.len()];
    }
    let n = sorted.len();
    probs
        .iter()
        .map(|p| {
            let position = p * (n as f64 - 1.0);
            let lo = position.floor() as usize;
            let hi = position.ceil() as usize;
            if lo == hi {
                sorted[lo]
            } else {
                sorted[lo] + (sorted[hi] - sorted[lo]) * (position - lo as f64)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probabilities_match_the_reference_grid() {
        let p = probabilities(99);
        assert_eq!(p.len(), 99);
        assert!((p[0] - 0.01).abs() < 1e-15);
        assert!((p[98] - 0.99).abs() < 1e-15);
    }

    #[test]
    fn quantiles_are_type_7() {
        // numpy: np.quantile([1,2,3,4], [0, .25, .5, .75, 1]) -> 1, 1.75, 2.5, 3.25, 4
        let mut v = [4.0, 1.0, 3.0, 2.0];
        let q = quantiles(&mut v, &[0.0, 0.25, 0.5, 0.75, 1.0]);
        for (got, want) in q.iter().zip([1.0, 1.75, 2.5, 3.25, 4.0]) {
            assert!((got - want).abs() < 1e-12, "{got} vs {want}");
        }
    }

    #[test]
    fn an_empty_sample_gives_nan_rather_than_a_panic() {
        assert!(quantiles(&mut [], &[0.5])[0].is_nan());
    }
}
