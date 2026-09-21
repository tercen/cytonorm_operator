//! Parity with cytonormpy **when there is more than one cluster**, which
//! `tests/reference_parity.rs` cannot reach: with no clusterer attached everything sits in
//! cluster `-1`, so a bug in the grouping — a spline fitted for the wrong triple, or applied to
//! the wrong cells — would not show there.
//!
//! `fixtures/gen_cluster_fixtures.py` runs the Python with a KMeans clusterer. The clusterer
//! itself is not the subject: the operator takes labels as an input factor, so the fixture only
//! needs labels that are reproducible. What is checked is the fit and the application, per
//! (batch, cluster, channel).
use std::collections::{BTreeMap, HashMap};

use cytonorm_operator::cytonorm::{Key, Model};

const N_QUANTILES: usize = 99;

fn read_csv(path: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .unwrap()
        .split(',')
        .map(str::to_string)
        .collect();
    let rows = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(str::to_string).collect())
        .collect();
    (header, rows)
}

#[test]
fn a_spline_per_batch_cluster_and_channel_matches_cytonormpy() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");
    let (h, rows) = read_csv(&format!("{dir}cluster_input.csv"));
    let meta = ["file_name", "batch", "type", "cluster"];
    let channels: Vec<String> = h
        .iter()
        .filter(|c| !meta.contains(&c.as_str()))
        .cloned()
        .collect();
    let idx = |n: &str| h.iter().position(|c| c == n).unwrap();
    let (i_batch, i_type, i_cluster) = (idx("batch"), idx("type"), idx("cluster"));
    let chan_idx: Vec<usize> = channels.iter().map(|c| idx(c)).collect();

    // Both indexed in sorted label order, as cytonormpy indexes them.
    let index_of = |col: usize| -> HashMap<String, i64> {
        let mut v: Vec<String> = rows.iter().map(|r| r[col].clone()).collect();
        v.sort();
        v.dedup();
        v.into_iter()
            .enumerate()
            .map(|(i, s)| (s, i as i64))
            .collect()
    };
    let batch_of = index_of(i_batch);
    let cluster_of = index_of(i_cluster);
    assert!(
        cluster_of.len() > 1,
        "the fixture must have several clusters"
    );

    let keys: Vec<Key> = rows
        .iter()
        .map(|r| Key {
            batch: batch_of[&r[i_batch]] as usize,
            cluster: cluster_of[&r[i_cluster]],
        })
        .collect();
    let values: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| chan_idx.iter().map(|i| r[*i].parse().unwrap()).collect())
        .collect();
    let is_reference: Vec<bool> = rows.iter().map(|r| r[i_type] == "Train").collect();

    // Fit on the reference cells only, grouped by (batch, cluster, channel).
    let mut groups: BTreeMap<(Key, usize), Vec<f64>> = BTreeMap::new();
    for ((v, k), _) in values
        .iter()
        .zip(&keys)
        .zip(&is_reference)
        .filter(|(_, r)| **r)
    {
        for (c, x) in v.iter().enumerate() {
            groups.entry((*k, c)).or_default().push(*x);
        }
    }
    let n_groups = groups.len();
    let model = Model::fit_groups(groups, channels.len(), N_QUANTILES, 50);
    assert_eq!(
        n_groups,
        batch_of.len() * cluster_of.len() * channels.len(),
        "every (batch, cluster, channel) should have reference cells in this fixture"
    );

    // Apply to every cell and compare with what cytonormpy wrote.
    let (want_h, want_rows) = read_csv(&format!("{dir}cluster_normalized.csv"));
    let want_idx: Vec<usize> = channels
        .iter()
        .map(|c| want_h.iter().position(|x| x == c).unwrap())
        .collect();
    assert_eq!(want_rows.len(), rows.len());

    let (mut worst, mut n) = (0.0f64, 0usize);
    for (row, (v, k)) in values.iter().zip(&keys).enumerate() {
        for (c, x) in v.iter().enumerate() {
            let got = model.apply_one(*k, c, *x);
            let want: f64 = want_rows[row][want_idx[c]].parse().unwrap();
            worst = worst.max((got - want).abs() / want.abs().max(1e-9));
            n += 1;
        }
    }
    println!(
        "{n} values across {} clusters: worst relative {worst:.2e}",
        cluster_of.len()
    );
    assert!(worst < 1e-10, "normalised values differ by {worst:e}");
}
