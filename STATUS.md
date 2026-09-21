## 0.1.2: the platform's unit test (2026-09-21)

Faris asked whether the operators used the platform's test setup. This one did not.
`tests/test.json` now projects `cytonorm_golden_long.csv` — three batches, three KMeans clusters
from cytonormpy, channels on rows, cells on columns, the batch, type and cluster as column
factors named through the properties — with a full fit (`number_of_cells = 0`), and diffs the
assembled relations. Its expected values were taken from a Studio run and every one of the
108,000 checked against cytonormpy's own output before being committed: worst relative
difference **2.9e-14**. This is the only test that sees the result the way Tercen assembles it;
`cargo test` stops at the bytes the operator writes.

# cytonorm_rust_operator — status, morning of 2026-09-21

Built overnight against `~/tercen/goals/2026-09-20-cytonorm.md`.

## Where it got to

| check | result |
|---|---|
| quantiles and goal vs cytonormpy | **exact** (0.0e0 over 891 values) |
| normalised values vs cytonormpy, in process | 2.1e-13 over 144,000 values |
| the same, exported from a Tercen instance | **2.1e-13** — the whole path agrees |
| `cargo test` | 20 tests green |
| Studio dev run | 144,000 values in 0.4 s, peak RSS 36 MB |

The platform comparison is the one that matters: it covers the projection, the colour and label
factors, the fit, the TSON the operator writes, the server's reading of it, and the export.

## Published

`0.1.0` is tagged and `ghcr.io/tercen/cytonorm_rust_operator:0.1.0` is pushed. **The package is
private**, so nothing can pull it until it is made public in the package settings — there is no
API for that. The release's install check failed the same way the other operators' do: the
Actions token cannot fetch a zipball from a private repository. The image push itself succeeded.

```
https://github.com/orgs/tercen/packages/container/cytonorm_rust_operator/settings
tercenctl operator install --repo https://github.com/tercen/cytonorm_rust_operator --tag 0.1.0 --team library
```

## Clustering, 2026-09-21

`cluster > 1` is still refused, and now for a better reason: clustering is its own step.
`flowsom_rust_operator` is a drop-in for the R FlowSOM operator and bit-identical to it; project
its `metacluster_id` and name that factor in `cluster_factor`. The refusal message says so.

The multi-cluster path is now checked against cytonormpy rather than assumed:
`tests/cluster_parity.rs` fits a spline per (batch, cluster, channel) over three KMeans clusters
and matches **2.9e-14 over 108,000 values**. `tests/reference_parity.rs` cannot reach that — with
no clusterer everything sits in cluster -1, so a spline fitted for the wrong triple would not
show.

## What is deliberately not here
- **Real-data fixtures.** Everything committed is synthetic. The run-9 check belongs in a local
  run from a path in an environment variable, as `flowvs-rs` does it.

## Known, and worth reading before trusting a number

- With `number_of_cells` at its default the fit uses a **subsample**, so results differ from a
  full fit by around a percent. That is CytoNorm behaving as designed, not drift: set
  `number_of_cells = 0` to reproduce a reference run exactly.
- The memory model books a constant 600 MB, which the runs so far are far inside. It has not
  been measured on a cohort-scale crosstab.

## Next

1. A run on the real 19.5 M-cell crosstab, which also refits the memory model.
2. Cluster labels from the R FlowSOM operator, end to end.
3. The map, in `cytonorm-rs`, where `flowsom-rs` can reuse it.
