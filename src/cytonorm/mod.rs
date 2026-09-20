//! CytoNorm's normalisation, without any Tercen types.
//!
//! The reference is `cytonormpy` 1.0.2 **as runs 7–9 configure it**: 99 quantiles, PCHIP
//! tangents with slope-1 extrapolation, and a batch-mean goal. Its defaults differ, and would
//! not reproduce this study.
//!
//! The method in one paragraph: for each batch, cluster and channel, take the quantiles of the
//! **reference** cells (the batch controls). Average those quantiles across batches to get a
//! goal. Fit a monotone spline from each batch's quantiles onto the goal, and apply it to every
//! cell of that batch, cluster and channel — controls and samples alike. Batch effects move the
//! distribution; the spline moves it back.
//!
//! Clustering is somebody else's job here: labels come in with the data. That is how CytoNorm
//! is usually run (FlowSOM upstream), and a single cluster is a legitimate configuration in its
//! own right, which is what the reference falls back to when no clusterer is attached.
pub mod quantiles;
pub mod spline;

use std::collections::BTreeMap;

use spline::Spline;

/// Which cells define the goal: the batch controls, not the samples.
pub const REFERENCE: &str = "reference";

/// One cell's coordinates. Channels are columns of `values`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub batch: usize,
    pub cluster: i64,
}

/// A fitted model: one spline per batch, cluster and channel.
#[derive(Debug, Default)]
pub struct Model {
    pub n_channels: usize,
    splines: BTreeMap<(Key, usize), Spline>,
    /// Quantiles per key and channel, kept for the diagnostics table.
    pub quantiles: BTreeMap<(Key, usize), Vec<f64>>,
    pub goal: BTreeMap<(i64, usize), Vec<f64>>,
    pub probs: Vec<f64>,
}

impl Model {
    /// Fit from reference cells only.
    ///
    /// `values[i]` is one cell's channels, `keys[i]` its batch and cluster. Cells whose cluster
    /// has too few reference events in a batch get an identity spline: with nothing to estimate,
    /// leaving the data alone is the honest answer.
    pub fn fit(
        values: &[Vec<f64>],
        keys: &[Key],
        n_channels: usize,
        n_quantiles: usize,
        min_cells: usize,
    ) -> Self {
        let mut groups: BTreeMap<(Key, usize), Vec<f64>> = BTreeMap::new();
        for (v, k) in values.iter().zip(keys) {
            for (c, x) in v.iter().enumerate() {
                groups.entry((*k, c)).or_default().push(*x);
            }
        }
        Self::fit_groups(groups, n_channels, n_quantiles, min_cells)
    }

    /// Fit from reference values already grouped by (batch, cluster) and channel.
    ///
    /// The quantiles of one channel do not depend on any other, so an operator reading a
    /// crosstab cell by cell never has to reassemble whole cells: it can sample each group
    /// independently. This is the entry point it uses.
    pub fn fit_groups(
        mut groups: BTreeMap<(Key, usize), Vec<f64>>,
        n_channels: usize,
        n_quantiles: usize,
        min_cells: usize,
    ) -> Self {
        let probs = quantiles::probabilities(n_quantiles);

        // quantiles per (batch, cluster, channel)
        let mut q: BTreeMap<(Key, usize), Vec<f64>> = BTreeMap::new();
        for ((key, c), col) in groups.iter_mut() {
            col.retain(|v| v.is_finite());
            if col.len() < min_cells {
                continue;
            }
            q.insert((*key, *c), quantiles::quantiles(col, &probs));
        }

        // the goal: the mean across batches, per cluster and channel
        let mut goal: BTreeMap<(i64, usize), Vec<f64>> = BTreeMap::new();
        let mut counts: BTreeMap<(i64, usize), f64> = BTreeMap::new();
        for ((key, c), v) in &q {
            let entry = goal.entry((key.cluster, *c)).or_insert_with(|| vec![0.0; v.len()]);
            for (g, x) in entry.iter_mut().zip(v) {
                *g += x;
            }
            *counts.entry((key.cluster, *c)).or_insert(0.0) += 1.0;
        }
        for (k, v) in goal.iter_mut() {
            let n = counts[k];
            for g in v.iter_mut() {
                *g /= n;
            }
        }

        let mut splines = BTreeMap::new();
        for ((key, c), qv) in &q {
            let s = match goal.get(&(key.cluster, *c)) {
                Some(g) => Spline::fit(qv, g),
                None => Spline::Identity,
            };
            splines.insert((*key, *c), s);
        }

        Model {
            n_channels,
            splines,
            quantiles: q,
            goal,
            probs,
        }
    }

    /// Transform one cell in place. A key the model never saw is left alone.
    pub fn apply(&self, key: Key, channels: &mut [f64]) {
        for (c, v) in channels.iter_mut().enumerate() {
            if let Some(s) = self.splines.get(&(key, c)) {
                *v = s.eval(*v);
            }
        }
    }

    /// Transform one value.
    pub fn apply_one(&self, key: Key, channel: usize, v: f64) -> f64 {
        match self.splines.get(&(key, channel)) {
            Some(s) => s.eval(v),
            None => v,
        }
    }

    /// Keys that were fitted, for reporting.
    pub fn fitted_keys(&self) -> Vec<Key> {
        let mut v: Vec<Key> = self.splines.keys().map(|(k, _)| *k).collect();
        v.dedup();
        v
    }

    pub fn is_identity(&self, key: Key, channel: usize) -> bool {
        matches!(self.splines.get(&(key, channel)), Some(Spline::Identity) | None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(shift: f64, n: usize, seed: u64) -> Vec<f64> {
        // deterministic pseudo-normals, scaled by the batch shift
        let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (0..n)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u1 = (((s >> 11) as f64 + 0.5) / (1u64 << 53) as f64).max(1e-12);
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let u2 = ((s >> 11) as f64 + 0.5) / (1u64 << 53) as f64;
                shift * (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
            })
            .collect()
    }

    #[test]
    fn a_batch_shift_is_removed() {
        // two batches, one channel, differing only by a scale factor
        let mut values = Vec::new();
        let mut keys = Vec::new();
        for (b, shift) in [(0usize, 1.0), (1, 2.0)] {
            for v in sample(shift, 4000, b as u64 + 1) {
                values.push(vec![v]);
                keys.push(Key {
                    batch: b,
                    cluster: -1,
                });
            }
        }
        let m = Model::fit(&values, &keys, 1, 99, 50);

        // after normalisation the two batches' quantiles should agree far better than before
        let take = |b: usize, normalise: bool| -> Vec<f64> {
            let mut col: Vec<f64> = values
                .iter()
                .zip(&keys)
                .filter(|(_, k)| k.batch == b)
                .map(|(v, k)| {
                    if normalise {
                        m.apply_one(*k, 0, v[0])
                    } else {
                        v[0]
                    }
                })
                .collect();
            quantiles::quantiles(&mut col, &[0.1, 0.5, 0.9])
        };
        let before: f64 = take(0, false)
            .iter()
            .zip(take(1, false))
            .map(|(a, b)| (a - b).abs())
            .sum();
        let after: f64 = take(0, true)
            .iter()
            .zip(take(1, true))
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(after < before / 10.0, "before {before:.3}, after {after:.3}");
    }

    #[test]
    fn a_cluster_with_too_few_reference_cells_is_left_alone() {
        let values = vec![vec![1.0], vec![2.0]];
        let keys = vec![
            Key {
                batch: 0,
                cluster: 7,
            },
            Key {
                batch: 0,
                cluster: 7,
            },
        ];
        let m = Model::fit(&values, &keys, 1, 99, 50);
        assert!(m.is_identity(keys[0], 0));
        assert_eq!(m.apply_one(keys[0], 0, 42.0), 42.0);
    }
}
