//! Parity with cytonormpy, configured the way runs 7–9 configure it.
//!
//! `fixtures/gen_fixtures.py` runs the Python on synthetic data — three batches with a
//! multiplicative shift, two reference files and two samples each — and dumps the input, the
//! per-batch quantiles, the goal, and the normalised values. Checking the quantiles separately
//! from the result is what makes a disagreement locatable: a wrong quantile rule and a wrong
//! spline both move the output, and look the same there.
use std::collections::HashMap;

use cytonorm_operator::cytonorm::{Key, Model, quantiles};

const N_QUANTILES: usize = 99;

fn read_csv(path: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut lines = text.lines();
    let header: Vec<String> = lines.next().unwrap().split(',').map(str::to_string).collect();
    let rows = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(str::to_string).collect())
        .collect();
    (header, rows)
}

struct Input {
    values: Vec<Vec<f64>>,
    keys: Vec<Key>,
    is_reference: Vec<bool>,
    channels: Vec<String>,
}

fn load_input() -> Input {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");
    let (h, rows) = read_csv(&format!("{dir}input.csv"));
    let channels: Vec<String> = h
        .iter()
        .filter(|c| !["file_name", "batch", "type"].contains(&c.as_str()))
        .cloned()
        .collect();
    let idx = |n: &str| h.iter().position(|c| c == n).unwrap();
    let (i_batch, i_type) = (idx("batch"), idx("type"));
    let chan_idx: Vec<usize> = channels.iter().map(|c| idx(c)).collect();

    // cytonormpy indexes batches in sorted order, which is what quantiles.csv carries.
    let mut labels: Vec<String> = rows.iter().map(|r| r[i_batch].clone()).collect();
    labels.sort();
    labels.dedup();
    let batch_of: HashMap<String, usize> = labels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.clone(), i))
        .collect();

    let mut out = Input {
        values: Vec::with_capacity(rows.len()),
        keys: Vec::with_capacity(rows.len()),
        is_reference: Vec::with_capacity(rows.len()),
        channels,
    };
    for r in &rows {
        out.values
            .push(chan_idx.iter().map(|i| r[*i].parse().unwrap()).collect());
        out.keys.push(Key {
            batch: batch_of[&r[i_batch]],
            cluster: -1, // no clusterer attached, as in the fixture
        });
        out.is_reference.push(r[i_type] == "Train");
    }
    out
}

fn fit(input: &Input) -> Model {
    let (v, k): (Vec<Vec<f64>>, Vec<Key>) = input
        .values
        .iter()
        .zip(&input.keys)
        .zip(&input.is_reference)
        .filter(|(_, r)| **r)
        .map(|((v, k), _)| (v.clone(), *k))
        .unzip();
    Model::fit(&v, &k, input.channels.len(), N_QUANTILES, 50)
}

#[test]
fn quantiles_and_goal_match_the_reference() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");
    let input = load_input();
    let m = fit(&input);

    let (h, rows) = read_csv(&format!("{dir}quantiles.csv"));
    let idx = |n: &str| h.iter().position(|c| c == n).unwrap();
    let (i_b, i_ch, i_i, i_q, i_g) = (idx("batch"), idx("channel"), idx("i"), idx("q"), idx("goal"));

    let (mut worst_q, mut worst_g, mut n) = (0.0f64, 0.0f64, 0usize);
    for r in &rows {
        let batch: usize = r[i_b].parse().unwrap();
        let c = input.channels.iter().position(|x| *x == r[i_ch]).unwrap();
        let i: usize = r[i_i].parse().unwrap();
        let key = Key {
            batch,
            cluster: -1,
        };
        let got_q = m.quantiles[&(key, c)][i];
        let got_g = m.goal[&(-1, c)][i];
        let (want_q, want_g): (f64, f64) = (r[i_q].parse().unwrap(), r[i_g].parse().unwrap());
        worst_q = worst_q.max((got_q - want_q).abs() / want_q.abs().max(1e-9));
        worst_g = worst_g.max((got_g - want_g).abs() / want_g.abs().max(1e-9));
        n += 1;
    }
    println!("{n} quantiles: worst {worst_q:.2e}, goal worst {worst_g:.2e}");
    assert!(worst_q < 1e-12, "quantiles differ by {worst_q:e}");
    assert!(worst_g < 1e-12, "goal differs by {worst_g:e}");
}

#[test]
fn normalised_values_match_the_reference() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");
    let input = load_input();
    let m = fit(&input);

    let (h, rows) = read_csv(&format!("{dir}normalized.csv"));
    let chan_idx: Vec<usize> = input
        .channels
        .iter()
        .map(|c| h.iter().position(|x| x == c).unwrap())
        .collect();
    assert_eq!(rows.len(), input.values.len());

    let (mut worst, mut worst_at) = (0.0f64, String::new());
    for (r, (v, k)) in rows.iter().zip(input.values.iter().zip(&input.keys)) {
        for (c, ci) in chan_idx.iter().enumerate() {
            let want: f64 = r[*ci].parse().unwrap();
            let got = m.apply_one(*k, c, v[c]);
            let rel = (got - want).abs() / want.abs().max(1e-6);
            if rel > worst {
                worst = rel;
                worst_at = format!("{} batch {} value {}", input.channels[c], k.batch, v[c]);
            }
        }
    }
    println!(
        "{} cells x {} channels: worst relative difference {worst:.2e} ({worst_at})",
        rows.len(),
        input.channels.len()
    );
    assert!(worst < 1e-9, "normalised values differ by {worst:e}");
}

/// The quantile rule itself, against numpy's, on the fixture's own data.
#[test]
fn the_quantile_grid_is_the_one_the_reference_used() {
    let p = quantiles::probabilities(N_QUANTILES);
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/");
    let (h, rows) = read_csv(&format!("{dir}quantiles.csv"));
    let (i_i, i_p) = (
        h.iter().position(|c| c == "i").unwrap(),
        h.iter().position(|c| c == "p").unwrap(),
    );
    for r in rows.iter().take(N_QUANTILES) {
        let i: usize = r[i_i].parse().unwrap();
        let want: f64 = r[i_p].parse().unwrap();
        assert!((p[i] - want).abs() < 1e-15, "probability {i}");
    }
}
