# cytonorm_operator

Batch normalisation for cytometry in Tercen. **Version 2 is a Rust implementation and a
replacement, not a port, of 1.x.** It follows
[`cytonormpy`](https://github.com/TarikExner/cytonormpy) 1.0.2. The R implementation (1.x, which
wrapped R CytoNorm and clustered internally with FlowSOM) is kept on the
[`r-legacy`](https://github.com/tercen/cytonorm_operator/tree/r-legacy) branch and the
`r-legacy-1.2.2` tag. 2.0 was developed as `tercen/cytonorm_rust_operator` and merged here with
its history.

## Changes from 1.x

- **No internal clustering.** 1.x ran FlowSOM inside the operator (`cluster`, default 10). 2.0
  takes cluster labels from a column factor (`cluster_factor`), typically from the FlowSOM
  operator (2.x has `train_factor` for clustering on the batch controls, as CytoNorm does).
  `cluster` must be 1; a 1.x step with `cluster` = 10 fails with a message saying what to do.
- **Different output.** 2.0 writes one `cytonorm` value per crosstab cell (joined on row and
  column), like any transform. 1.x returned the normalised data as a wide table.
- **Numerics follow cytonormpy**: matched to 2.1e-13 over the reference fixtures and 2.9e-14
  across three clusters. 1.x's unit test compares R CytoNorm's output, so it does not carry over.
- **New properties:** `n_quantiles`, `min_cells`, `seed`, `batch_factor`, `type_factor`,
  `reference_value`, `cluster_factor`, `collect_max_cells`.
- **gRPC operator**, static image; needs a Tercen server with gRPC operator support.

## What it does

For each batch, cluster and channel, it takes the quantiles of the **batch controls**, averages
those quantiles across batches to form a goal, fits a monotone spline from each batch onto that
goal, and applies it to every cell of the batch. Batch effects move a distribution; the spline
moves it back.

## Input

The projection follows the 1.x operator's (rows, columns, colours = batch, labels = type):

| | |
|---|---|
| rows | channel |
| columns | cell, and the factors below |
| colours | batch |
| labels | type, where `Train` marks a batch control |
| y | the value, usually already arcsinh transformed |

The batch and type can also be named directly with `batch_factor` and `type_factor`, which is
useful when they are plain column factors.

## Properties

| Name | Default | Description |
|---|---:|---|
| `cluster` | 1 | FlowSOM clusters. **Only 1 is supported**: this version does not cluster. |
| `cluster_factor` | *(none)* | Column factor holding a cluster label per cell, e.g. from the FlowSOM operator. |
| `number_of_cells` | 6000 | Reference cells sampled per group to fit; 0 uses all of them. |
| `n_quantiles` | 99 | 99 is what the reference runs use; cytonormpy's notebook default is 101. |
| `min_cells` | 101 | Below this a group is passed through unchanged. |
| `seed` | 1 | Seed for the subsample, so a fit repeats. |
| `reference_value` | `Train` | The value in the type factor that marks a batch control. |
| `collect_max_cells` | 20000000 | Above this the result is streamed rather than buffered. |

## Output

One column, `<namespace>.cytonorm`, one value per cell.

## Parity

`cargo test` reproduces cytonormpy on committed synthetic fixtures: **quantiles and goal exactly**,
and normalised values to **2e-13** over 144,000 values. The same comparison run through a Tercen
instance — projection, operator, server, export — gives the same 2e-13.

The reference configuration matters. cytonormpy's default spline tangents overshoot, and the
runs this port targets replace them with PCHIP tangents and slope-1 extrapolation; reproducing
the library's defaults would not reproduce the study.

## Not here yet

The self-organising map. Clustering comes in as a factor, or there is one cluster, which is
CytoNorm without clustering and a legitimate configuration in its own right.
