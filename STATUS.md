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

## What is deliberately not here

- **The self-organising map.** `cluster` above 1 is refused with a message pointing at
  `cluster_factor`, which takes labels from any upstream clusterer. One cluster is CytoNorm
  without clustering.
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
